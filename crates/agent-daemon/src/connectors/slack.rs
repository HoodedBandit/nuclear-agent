use agent_providers::load_api_key;
use serde::Deserialize;

use super::*;

pub(super) async fn poll_slack_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .slack_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_slack_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_slack_connector(
    state: &AppState,
    connector: &SlackConnectorConfig,
) -> Result<SlackPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(SlackPollResponse {
            connector_id: connector.id.clone(),
            processed_messages: 0,
            queued_missions: 0,
            pending_approvals: 0,
            updated_channels: 0,
        });
    }

    let token = load_slack_bot_token(connector)?;
    let mut processed_messages = 0usize;
    let mut queued_missions = 0usize;
    let mut pending_approvals = 0usize;
    let mut updated_channels = 0usize;

    for channel_id in &connector.monitored_channel_ids {
        let oldest = connector
            .channel_cursors
            .iter()
            .find(|cursor| cursor.channel_id == *channel_id)
            .and_then(|cursor| cursor.last_message_ts.clone());
        let mut messages =
            fetch_slack_messages(state, &token, channel_id, oldest.as_deref()).await?;
        messages.sort_by(slack_message_ts_cmp);
        let mut latest_message_ts = oldest.clone();
        let mut channel_updated = false;

        for mut message in messages {
            if message.channel.as_deref().is_none() {
                message.channel = Some(channel_id.clone());
            }
            processed_messages += 1;
            let next_message_ts = Some(message.ts.clone());
            match slack_message_action(connector, &message) {
                SlackMessageAction::Ignore => {}
                SlackMessageAction::Pending(mut approval) => {
                    let existing = state.storage.get_connector_approval(&approval.id)?;
                    let is_new = existing.is_none();
                    if let Some(existing) = existing {
                        if existing.status == ConnectorApprovalStatus::Rejected {
                            if next_message_ts != latest_message_ts {
                                persist_slack_channel_cursor(
                                    state,
                                    &connector.id,
                                    channel_id,
                                    next_message_ts.clone(),
                                )
                                .await?;
                                latest_message_ts = next_message_ts.clone();
                                channel_updated = true;
                            }
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
                            "slack",
                            format!(
                                "slack '{}' requires pairing approval for channel={} user={} (run 'autism slack approvals' to review)",
                                connector.id,
                                approval.external_chat_id.as_deref().unwrap_or("-"),
                                approval.external_user_id.as_deref().unwrap_or("-")
                            ),
                        )?;
                    }
                    pending_approvals += 1;
                }
                SlackMessageAction::Queue { title, details } => {
                    let mission_id = slack_mission_id(&connector.id, channel_id, &message.ts);
                    if state.storage.get_mission(&mission_id)?.is_none() {
                        let mut mission = Mission::new(title.clone(), details);
                        mission.id = mission_id;
                        mission.alias = connector.alias.clone();
                        mission.requested_model = connector.requested_model.clone();
                        mission.status = MissionStatus::Queued;
                        mission.wake_trigger = Some(WakeTrigger::Slack);
                        if let Some(cwd) = connector.cwd.clone() {
                            mission.workspace_key =
                                Some(resolve_request_cwd(Some(cwd))?.display().to_string());
                        }
                        state.storage.upsert_mission(&mission)?;
                        append_log(
                            state,
                            "info",
                            "slack",
                            format!(
                                "slack '{}' queued mission '{}'",
                                connector.id, mission.title
                            ),
                        )?;
                        queued_missions += 1;
                    }
                }
            }
            if next_message_ts != latest_message_ts {
                persist_slack_channel_cursor(
                    state,
                    &connector.id,
                    channel_id,
                    next_message_ts.clone(),
                )
                .await?;
                latest_message_ts = next_message_ts;
                channel_updated = true;
            }
        }

        if channel_updated {
            updated_channels += 1;
        }
    }

    if queued_missions > 0 {
        state.autopilot_wake.notify_waiters();
    }

    Ok(SlackPollResponse {
        connector_id: connector.id.clone(),
        processed_messages,
        queued_missions,
        pending_approvals,
        updated_channels,
    })
}

pub(super) fn load_slack_bot_token(connector: &SlackConnectorConfig) -> Result<String, ApiError> {
    let account = connector
        .bot_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "slack connector '{}' has no bot token configured",
                    connector.id
                ),
            )
        })?;
    load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to load slack bot token for connector '{}': {error}",
                connector.id
            ),
        )
    })
}

pub(super) fn slack_bot_token_account_in_use(
    connectors: &[SlackConnectorConfig],
    account: &str,
) -> bool {
    let account = account.trim();
    !account.is_empty()
        && connectors.iter().any(|connector| {
            connector
                .bot_token_keychain_account
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| value == account)
        })
}

async fn fetch_slack_messages(
    state: &AppState,
    token: &str,
    channel_id: &str,
    oldest: Option<&str>,
) -> Result<Vec<SlackMessage>, ApiError> {
    let mut request = state
        .http_client
        .get("https://slack.com/api/conversations.history")
        .bearer_auth(token)
        .query(&[("channel", channel_id), ("limit", "50")]);
    if let Some(oldest) = oldest.filter(|value| !value.trim().is_empty()) {
        request = request.query(&[("oldest", oldest), ("inclusive", "false")]);
    }
    let response = request.send().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("slack conversations.history request failed: {error}"),
        )
    })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read slack conversations.history response: {error}"),
        )
    })?;
    let parsed: SlackHistoryResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse slack conversations.history response: {error}"),
        )
    })?;
    if !status.is_success() || !parsed.ok {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "slack conversations.history failed: {}",
                parsed.error.unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    Ok(parsed.messages)
}

pub(super) async fn send_slack_message(
    client: &reqwest::Client,
    token: &str,
    payload: &SlackSendRequest,
) -> Result<(Option<String>, Option<String>), ApiError> {
    let response = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(token)
        .json(&serde_json::json!({
            "channel": payload.channel_id,
            "text": payload.text,
        }))
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("slack chat.postMessage request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read slack chat.postMessage response: {error}"),
        )
    })?;
    let parsed: SlackPostMessageResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse slack chat.postMessage response: {error}"),
        )
    })?;
    if !status.is_success() || !parsed.ok {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "slack chat.postMessage failed: {}",
                parsed.error.unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    Ok((parsed.channel, parsed.ts))
}

async fn persist_slack_channel_cursor(
    state: &AppState,
    connector_id: &str,
    channel_id: &str,
    last_message_ts: Option<String>,
) -> Result<(), ApiError> {
    let mut config = state.config.write().await;
    let Some(connector) = config
        .slack_connectors
        .iter_mut()
        .find(|connector| connector.id == connector_id)
    else {
        return Ok(());
    };
    if let Some(cursor) = connector
        .channel_cursors
        .iter_mut()
        .find(|cursor| cursor.channel_id == channel_id)
    {
        cursor.last_message_ts = last_message_ts;
    } else {
        connector
            .channel_cursors
            .push(agent_core::SlackChannelCursor {
                channel_id: channel_id.to_string(),
                last_message_ts,
            });
    }
    connector
        .channel_cursors
        .sort_by(|left, right| left.channel_id.cmp(&right.channel_id));
    state.storage.save_config(&config)?;
    Ok(())
}

#[allow(clippy::large_enum_variant)]
enum SlackMessageAction {
    Ignore,
    Pending(ConnectorApprovalRecord),
    Queue { title: String, details: String },
}

fn slack_message_action(
    connector: &SlackConnectorConfig,
    message: &SlackMessage,
) -> SlackMessageAction {
    if message.bot_id.is_some()
        || message
            .subtype
            .as_deref()
            .is_some_and(|value| value != "file_share")
    {
        return SlackMessageAction::Ignore;
    }
    let Some(text) = slack_message_text(message) else {
        return SlackMessageAction::Ignore;
    };
    let channel_id = message.channel.as_deref().unwrap_or("unknown");
    let title = format!(
        "{} slack: {}",
        connector.name,
        truncate_for_title(&text, 72)
    );
    let details = format!(
        "Slack connector: {}\nChannel: {} (id {})\nSender: {}\nMessage ts: {}\nMessage:\n{}",
        connector.name,
        slack_channel_label(message),
        channel_id,
        slack_sender_label(message),
        message.ts,
        text
    );
    if connector.require_pairing_approval {
        let channel_allowed = connector
            .allowed_channel_ids
            .iter()
            .any(|value| value == channel_id);
        let user_allowed = connector.allowed_user_ids.is_empty()
            || message
                .user
                .as_deref()
                .is_some_and(|user| connector.allowed_user_ids.iter().any(|value| value == user));
        if !channel_allowed || !user_allowed {
            return SlackMessageAction::Pending(build_slack_pairing_approval(
                connector, message, title, details, &text,
            ));
        }
    } else {
        if !connector.allowed_channel_ids.is_empty()
            && !connector
                .allowed_channel_ids
                .iter()
                .any(|value| value == channel_id)
        {
            return SlackMessageAction::Ignore;
        }
        if !connector.allowed_user_ids.is_empty()
            && !message
                .user
                .as_deref()
                .is_some_and(|user| connector.allowed_user_ids.iter().any(|value| value == user))
        {
            return SlackMessageAction::Ignore;
        }
    }

    SlackMessageAction::Queue { title, details }
}

fn build_slack_pairing_approval(
    connector: &SlackConnectorConfig,
    message: &SlackMessage,
    title: String,
    details: String,
    text: &str,
) -> ConnectorApprovalRecord {
    let source_key = slack_pairing_source_key(connector, message);
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Slack,
        connector.id.clone(),
        connector.name.clone(),
        title,
        details,
        source_key.clone(),
    );
    approval.id = slack_pairing_approval_id(&source_key);
    approval.source_event_id = Some(message.ts.clone());
    approval.external_chat_id = message.channel.clone();
    approval.external_chat_display = Some(slack_channel_label(message));
    approval.external_user_id = message.user.clone();
    approval.external_user_display = Some(slack_sender_label(message));
    approval.message_preview = Some(truncate_for_title(text, 240));
    approval
}

fn slack_pairing_source_key(connector: &SlackConnectorConfig, message: &SlackMessage) -> String {
    let channel_id = message.channel.as_deref().unwrap_or("unknown");
    let user_scope = if connector.allowed_user_ids.is_empty() {
        "any"
    } else {
        message.user.as_deref().unwrap_or("unknown")
    };
    format!(
        "slack:{}:channel:{}:user:{}",
        connector.id.trim(),
        channel_id,
        user_scope
    )
}

fn slack_pairing_approval_id(source_key: &str) -> String {
    format!("connector-approval:{source_key}")
}

pub(super) fn add_slack_pairing_allowlist_entries(
    connector: &mut SlackConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<(), ApiError> {
    if let Some(channel_id) = approval.external_chat_id.as_deref().map(str::trim) {
        if channel_id.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "slack approval has an empty channel id",
            ));
        }
        if !connector
            .allowed_channel_ids
            .iter()
            .any(|value| value == channel_id)
        {
            connector.allowed_channel_ids.push(channel_id.to_string());
            connector.allowed_channel_ids.sort();
            connector.allowed_channel_ids.dedup();
        }
        if !connector
            .monitored_channel_ids
            .iter()
            .any(|value| value == channel_id)
        {
            connector.monitored_channel_ids.push(channel_id.to_string());
            connector.monitored_channel_ids.sort();
            connector.monitored_channel_ids.dedup();
        }
    }
    if let Some(user_id) = approval.external_user_id.as_deref().map(str::trim) {
        if user_id.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "slack approval has an empty user id",
            ));
        }
        if !connector
            .allowed_user_ids
            .iter()
            .any(|value| value == user_id)
        {
            connector.allowed_user_ids.push(user_id.to_string());
            connector.allowed_user_ids.sort();
            connector.allowed_user_ids.dedup();
        }
    }
    Ok(())
}

pub(super) fn queue_slack_approval_mission(
    state: &AppState,
    connector: &SlackConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<String, ApiError> {
    let channel_id = approval
        .external_chat_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let message_ts = approval
        .source_event_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&approval.id);
    let mission_id = slack_mission_id(&connector.id, channel_id, message_ts);
    if state.storage.get_mission(&mission_id)?.is_none() {
        let mut mission = Mission::new(approval.title.clone(), approval.details.clone());
        mission.id = mission_id.clone();
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Slack);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        state.autopilot_wake.notify_waiters();
    }
    Ok(mission_id)
}

fn slack_message_text(message: &SlackMessage) -> Option<String> {
    message
        .text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn slack_channel_label(message: &SlackMessage) -> String {
    message
        .channel
        .as_deref()
        .map(|value| format!("channel {value}"))
        .unwrap_or_else(|| "unknown channel".to_string())
}

fn slack_sender_label(message: &SlackMessage) -> String {
    message
        .user
        .as_deref()
        .map(|value| format!("user {value}"))
        .unwrap_or_else(|| "unknown sender".to_string())
}

fn slack_mission_id(connector_id: &str, channel_id: &str, ts: &str) -> String {
    format!(
        "slack:{}:{}:{}",
        connector_id.trim(),
        channel_id.trim(),
        ts.trim()
    )
}

fn slack_message_ts_cmp(left: &SlackMessage, right: &SlackMessage) -> std::cmp::Ordering {
    match (
        left.ts.trim().parse::<f64>(),
        right.ts.trim().parse::<f64>(),
    ) {
        (Ok(left), Ok(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        _ => left.ts.cmp(&right.ts),
    }
}

#[derive(Debug, Default, Deserialize)]
struct SlackHistoryResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    messages: Vec<SlackMessage>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SlackPostMessageResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SlackMessage {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    bot_id: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    ts: String,
    #[serde(default)]
    channel: Option<String>,
}
