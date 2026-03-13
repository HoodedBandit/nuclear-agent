use std::{path::PathBuf, time::Duration};

use tokio::{process::Command, time::timeout};

use super::*;

const SIGNAL_CLI_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn poll_signal_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .signal_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_signal_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_signal_connector(
    state: &AppState,
    connector: &SignalConnectorConfig,
) -> Result<SignalPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(SignalPollResponse {
            connector_id: connector.id.clone(),
            processed_messages: 0,
            queued_missions: 0,
            pending_approvals: 0,
        });
    }

    let messages = receive_signal_messages(connector).await?;
    let processed_messages = messages.len();
    let mut queued_missions = 0usize;
    let mut pending_approvals = 0usize;

    for message in messages {
        match signal_message_action(connector, &message) {
            SignalMessageAction::Ignore => {}
            SignalMessageAction::Pending(mut approval) => {
                let existing = state.storage.get_connector_approval(&approval.id)?;
                let is_new = existing.is_none();
                if let Some(existing) = existing {
                    if existing.status == ConnectorApprovalStatus::Rejected {
                        continue;
                    }
                    approval.created_at = existing.created_at;
                    approval.status = existing.status;
                    approval.review_note = existing.review_note;
                    approval.reviewed_at = existing.reviewed_at;
                    approval.queued_mission_id = existing.queued_mission_id;
                }
                approval.updated_at = chrono::Utc::now();
                state.storage.upsert_connector_approval(&approval)?;
                if is_new {
                    append_log(
                        state,
                        "warn",
                        "signal",
                        format!(
                            "signal '{}' requires pairing approval for group={} user={} (run 'autism signal approvals' to review)",
                            connector.id,
                            approval.external_chat_id.as_deref().unwrap_or("-"),
                            approval.external_user_id.as_deref().unwrap_or("-")
                        ),
                    )?;
                }
                pending_approvals += 1;
            }
            SignalMessageAction::Queue { title, details } => {
                let mission_id = signal_mission_id(
                    &connector.id,
                    message.group_id.as_deref(),
                    message.source.as_deref(),
                    message.timestamp,
                );
                if state.storage.get_mission(&mission_id)?.is_none() {
                    let mut mission = Mission::new(title.clone(), details);
                    mission.id = mission_id;
                    mission.alias = connector.alias.clone();
                    mission.requested_model = connector.requested_model.clone();
                    mission.status = MissionStatus::Queued;
                    mission.wake_trigger = Some(WakeTrigger::Signal);
                    if let Some(cwd) = connector.cwd.clone() {
                        mission.workspace_key =
                            Some(resolve_request_cwd(Some(cwd))?.display().to_string());
                    }
                    state.storage.upsert_mission(&mission)?;
                    append_log(
                        state,
                        "info",
                        "signal",
                        format!(
                            "signal '{}' queued mission '{}'",
                            connector.id, mission.title
                        ),
                    )?;
                    queued_missions += 1;
                }
            }
        }
    }

    if queued_missions > 0 {
        state.autopilot_wake.notify_waiters();
    }

    Ok(SignalPollResponse {
        connector_id: connector.id.clone(),
        processed_messages,
        queued_missions,
        pending_approvals,
    })
}

#[allow(clippy::large_enum_variant)]
enum SignalMessageAction {
    Ignore,
    Pending(ConnectorApprovalRecord),
    Queue { title: String, details: String },
}

#[derive(Debug, Clone)]
struct SignalInboundMessage {
    timestamp: i64,
    source: Option<String>,
    source_name: Option<String>,
    group_id: Option<String>,
    group_name: Option<String>,
    text: String,
}

fn signal_message_action(
    connector: &SignalConnectorConfig,
    message: &SignalInboundMessage,
) -> SignalMessageAction {
    if message.text.trim().is_empty() {
        return SignalMessageAction::Ignore;
    }
    let group_id = message.group_id.as_deref().map(str::trim).unwrap_or("");
    let source_id = message.source.as_deref().map(str::trim).unwrap_or("");

    if !connector.monitored_group_ids.is_empty()
        && (group_id.is_empty()
            || !connector
                .monitored_group_ids
                .iter()
                .any(|value| value.trim() == group_id))
    {
        return SignalMessageAction::Ignore;
    }

    let title = format!(
        "{} signal: {}",
        connector.name,
        truncate_for_title(&message.text, 72)
    );
    let details = format!(
        "Signal connector: {}\nConversation: {}\nSender: {}\nTimestamp: {}\nMessage:\n{}",
        connector.name,
        signal_chat_label(message),
        signal_sender_label(message),
        message.timestamp,
        message.text
    );

    if connector.require_pairing_approval {
        let group_allowed = group_id.is_empty()
            || connector
                .allowed_group_ids
                .iter()
                .any(|value| value.trim() == group_id);
        let user_allowed = connector.allowed_user_ids.is_empty()
            || (!source_id.is_empty()
                && connector
                    .allowed_user_ids
                    .iter()
                    .any(|value| value.trim() == source_id));
        if !group_allowed || !user_allowed {
            return SignalMessageAction::Pending(build_signal_pairing_approval(
                connector, message, title, details,
            ));
        }
    } else {
        if !connector.allowed_group_ids.is_empty()
            && (group_id.is_empty()
                || !connector
                    .allowed_group_ids
                    .iter()
                    .any(|value| value.trim() == group_id))
        {
            return SignalMessageAction::Ignore;
        }
        if !connector.allowed_user_ids.is_empty()
            && (source_id.is_empty()
                || !connector
                    .allowed_user_ids
                    .iter()
                    .any(|value| value.trim() == source_id))
        {
            return SignalMessageAction::Ignore;
        }
    }

    SignalMessageAction::Queue { title, details }
}

fn build_signal_pairing_approval(
    connector: &SignalConnectorConfig,
    message: &SignalInboundMessage,
    title: String,
    details: String,
) -> ConnectorApprovalRecord {
    let source_key = signal_pairing_source_key(connector, message);
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Signal,
        connector.id.clone(),
        connector.name.clone(),
        title,
        details,
        source_key.clone(),
    );
    approval.id = format!("connector-approval:{source_key}");
    approval.source_event_id = Some(message.timestamp.to_string());
    approval.external_chat_id = message.group_id.clone().or_else(|| message.source.clone());
    approval.external_chat_display = Some(signal_chat_label(message));
    approval.external_user_id = message.source.clone();
    approval.external_user_display = Some(signal_sender_label(message));
    approval.message_preview = Some(truncate_for_title(&message.text, 240));
    approval
}

fn signal_pairing_source_key(
    connector: &SignalConnectorConfig,
    message: &SignalInboundMessage,
) -> String {
    let conversation = message.group_id.as_deref().unwrap_or("direct");
    let user_scope = if connector.allowed_user_ids.is_empty() {
        "any"
    } else {
        message.source.as_deref().unwrap_or("unknown")
    };
    format!(
        "signal:{}:conversation:{}:user:{}",
        connector.id.trim(),
        conversation.trim(),
        user_scope.trim()
    )
}

pub(super) fn add_signal_pairing_allowlist_entries(
    connector: &mut SignalConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<(), ApiError> {
    if let Some(group_id) = approval.external_chat_id.as_deref().map(str::trim) {
        if !group_id.is_empty()
            && approval
                .external_chat_display
                .as_deref()
                .is_some_and(|display| display.starts_with("group "))
        {
            if !connector
                .allowed_group_ids
                .iter()
                .any(|value| value.trim() == group_id)
            {
                connector.allowed_group_ids.push(group_id.to_string());
                connector.allowed_group_ids.sort();
                connector.allowed_group_ids.dedup();
            }
            if !connector
                .monitored_group_ids
                .iter()
                .any(|value| value.trim() == group_id)
            {
                connector.monitored_group_ids.push(group_id.to_string());
                connector.monitored_group_ids.sort();
                connector.monitored_group_ids.dedup();
            }
        }
    }
    if let Some(user_id) = approval.external_user_id.as_deref().map(str::trim) {
        if user_id.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "signal approval has an empty user id",
            ));
        }
        if !connector
            .allowed_user_ids
            .iter()
            .any(|value| value.trim() == user_id)
        {
            connector.allowed_user_ids.push(user_id.to_string());
            connector.allowed_user_ids.sort();
            connector.allowed_user_ids.dedup();
        }
    }
    Ok(())
}

pub(super) fn queue_signal_approval_mission(
    state: &AppState,
    connector: &SignalConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<String, ApiError> {
    let conversation = approval
        .external_chat_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("direct");
    let timestamp = approval
        .source_event_id
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default();
    let mission_id = signal_mission_id(
        &connector.id,
        Some(conversation),
        approval.external_user_id.as_deref(),
        timestamp,
    );
    if state.storage.get_mission(&mission_id)?.is_none() {
        let mut mission = Mission::new(approval.title.clone(), approval.details.clone());
        mission.id = mission_id.clone();
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Signal);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        state.autopilot_wake.notify_waiters();
    }
    Ok(mission_id)
}

fn signal_chat_label(message: &SignalInboundMessage) -> String {
    if let Some(name) = message
        .group_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("group {name}")
    } else if let Some(group_id) = message
        .group_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("group {group_id}")
    } else if let Some(source) = message
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("direct {source}")
    } else {
        "unknown conversation".to_string()
    }
}

fn signal_sender_label(message: &SignalInboundMessage) -> String {
    let mut parts = Vec::new();
    if let Some(name) = message
        .source_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(name.to_string());
    }
    if let Some(source) = message
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(source.to_string());
    }
    if parts.is_empty() {
        "unknown sender".to_string()
    } else {
        parts.join(" ")
    }
}

fn signal_mission_id(
    connector_id: &str,
    group_id: Option<&str>,
    source: Option<&str>,
    timestamp: i64,
) -> String {
    format!(
        "signal:{}:{}:{}:{}",
        connector_id.trim(),
        group_id.unwrap_or("direct").trim(),
        source.unwrap_or("unknown").trim(),
        timestamp
    )
}

pub(super) async fn send_signal_message(
    connector: &SignalConnectorConfig,
    payload: &SignalSendRequest,
) -> Result<String, ApiError> {
    let text = payload.text.trim();
    if text.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "signal message text must not be empty",
        ));
    }
    let recipient = payload
        .recipient
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let group_id = payload
        .group_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if recipient.is_none() && group_id.is_none() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "signal send requires recipient or group_id",
        ));
    }
    ensure_signal_target_allowed(connector, recipient.as_deref(), group_id.as_deref())?;
    let mut args = vec![
        "-a".to_string(),
        connector.account.trim().to_string(),
        "send".to_string(),
        "-m".to_string(),
        text.to_string(),
    ];
    let target = if let Some(group_id) = group_id {
        args.push("-g".to_string());
        args.push(group_id.clone());
        format!("group {group_id}")
    } else {
        let recipient = recipient.ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "signal connector '{}' requires either recipient or group_id",
                    connector.id
                ),
            )
        })?;
        args.push(recipient.clone());
        recipient
    };
    let _ = run_signal_cli(connector, &args).await?;
    Ok(target)
}

async fn receive_signal_messages(
    connector: &SignalConnectorConfig,
) -> Result<Vec<SignalInboundMessage>, ApiError> {
    let output = run_signal_cli(
        connector,
        &[
            "-a".to_string(),
            connector.account.trim().to_string(),
            "receive".to_string(),
            "--json".to_string(),
            "--timeout".to_string(),
            "1".to_string(),
        ],
    )
    .await?;
    let mut messages = Vec::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(message) = parse_signal_receive_line(line) {
            messages.push(message);
        }
    }
    Ok(messages)
}

async fn run_signal_cli(
    connector: &SignalConnectorConfig,
    args: &[String],
) -> Result<String, ApiError> {
    let executable = connector
        .cli_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("signal-cli"));
    let mut command = Command::new(&executable);
    command.kill_on_drop(true);
    command.args(args);
    let output = command.output();
    let output = timeout(SIGNAL_CLI_TIMEOUT, output)
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                format!(
                    "signal-cli timed out for connector '{}' after {}s",
                    connector.id,
                    SIGNAL_CLI_TIMEOUT.as_secs()
                ),
            )
        })?
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!(
                    "failed to execute signal-cli '{}' for connector '{}': {error}",
                    executable.display(),
                    connector.id
                ),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "signal-cli failed for connector '{}': {}",
                connector.id,
                if stderr.is_empty() {
                    output.status.to_string()
                } else {
                    stderr
                }
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn ensure_signal_target_allowed(
    connector: &SignalConnectorConfig,
    recipient: Option<&str>,
    group_id: Option<&str>,
) -> Result<(), ApiError> {
    if let Some(group_id) = group_id.map(str::trim).filter(|value| !value.is_empty()) {
        if !connector.allowed_group_ids.is_empty()
            && !connector
                .allowed_group_ids
                .iter()
                .any(|value| value.trim() == group_id)
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                format!(
                    "signal group '{}' is not allowed for connector '{}'",
                    group_id, connector.id
                ),
            ));
        }
    }
    if let Some(recipient) = recipient.map(str::trim).filter(|value| !value.is_empty()) {
        if !connector.allowed_user_ids.is_empty()
            && !connector
                .allowed_user_ids
                .iter()
                .any(|value| value.trim() == recipient)
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                format!(
                    "signal recipient '{}' is not allowed for connector '{}'",
                    recipient, connector.id
                ),
            ));
        }
    }
    Ok(())
}

fn parse_signal_receive_line(line: &str) -> Option<SignalInboundMessage> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let text = signal_value_string(
        &value,
        &[
            &["envelope", "dataMessage", "message"],
            &["envelope", "dataMessage", "body"],
            &["dataMessage", "message"],
            &["dataMessage", "body"],
        ],
    )?;
    let timestamp = signal_value_i64(
        &value,
        &[
            &["envelope", "timestamp"],
            &["timestamp"],
            &["envelope", "dataMessage", "timestamp"],
        ],
    )
    .unwrap_or_default();
    Some(SignalInboundMessage {
        timestamp,
        source: signal_value_string(
            &value,
            &[
                &["envelope", "source"],
                &["source"],
                &["envelope", "sourceNumber"],
                &["sourceNumber"],
            ],
        ),
        source_name: signal_value_string(
            &value,
            &[
                &["envelope", "sourceName"],
                &["sourceName"],
                &["envelope", "dataMessage", "profileKey", "givenName"],
            ],
        ),
        group_id: signal_value_string(
            &value,
            &[
                &["envelope", "dataMessage", "groupInfo", "groupId"],
                &["dataMessage", "groupInfo", "groupId"],
                &["envelope", "groupInfo", "groupId"],
            ],
        ),
        group_name: signal_value_string(
            &value,
            &[
                &["envelope", "dataMessage", "groupInfo", "title"],
                &["dataMessage", "groupInfo", "title"],
                &["envelope", "dataMessage", "groupInfo", "name"],
                &["dataMessage", "groupInfo", "name"],
            ],
        ),
        text,
    })
}

fn signal_value_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for segment in *path {
            current = current.get(*segment)?;
        }
        current
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn signal_value_i64(value: &serde_json::Value, paths: &[&[&str]]) -> Option<i64> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for segment in *path {
            current = current.get(*segment)?;
        }
        current
            .as_i64()
            .or_else(|| current.as_str().and_then(|value| value.parse::<i64>().ok()))
    })
}
