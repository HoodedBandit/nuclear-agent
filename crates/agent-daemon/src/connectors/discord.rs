use agent_providers::load_api_key;
use serde::Deserialize;

use super::*;

pub(super) async fn poll_discord_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .discord_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_discord_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_discord_connector(
    state: &AppState,
    connector: &DiscordConnectorConfig,
) -> Result<DiscordPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(DiscordPollResponse {
            connector_id: connector.id.clone(),
            processed_messages: 0,
            queued_missions: 0,
            pending_approvals: 0,
            updated_channels: 0,
        });
    }

    let token = load_discord_bot_token(connector)?;
    let mut processed_messages = 0usize;
    let mut queued_missions = 0usize;
    let mut pending_approvals = 0usize;
    let mut updated_channels = 0usize;

    for channel_id in &connector.monitored_channel_ids {
        let after = connector
            .channel_cursors
            .iter()
            .find(|cursor| cursor.channel_id == *channel_id)
            .and_then(|cursor| cursor.last_message_id.clone());
        let mut messages =
            fetch_discord_messages(state, &token, channel_id, after.as_deref()).await?;
        messages.sort_by(discord_message_id_cmp);
        let mut latest_message_id = after.clone();
        let mut channel_updated = false;

        for mut message in messages {
            if message.channel_id.is_none() {
                message.channel_id = Some(channel_id.clone());
            }
            processed_messages += 1;
            let next_message_id = Some(message.id.clone());
            match discord_message_action(connector, &message) {
                DiscordMessageAction::Ignore => {}
                DiscordMessageAction::Pending(mut approval) => {
                    let existing = state.storage.get_connector_approval(&approval.id)?;
                    let is_new = existing.is_none();
                    if let Some(existing) = existing {
                        if existing.status == ConnectorApprovalStatus::Rejected {
                            if next_message_id != latest_message_id {
                                persist_discord_channel_cursor(
                                    state,
                                    &connector.id,
                                    channel_id,
                                    next_message_id.clone(),
                                )
                                .await?;
                                latest_message_id = next_message_id.clone();
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
                        "discord",
                        format!(
                                "discord '{}' requires pairing approval for channel={} user={} (run 'nuclear discord approvals' to review)",
                                connector.id,
                                approval.external_chat_id.as_deref().unwrap_or("-"),
                                approval.external_user_id.as_deref().unwrap_or("-")
                            ),
                        )?;
                    }
                    pending_approvals += 1;
                }
                DiscordMessageAction::Queue { title, details } => {
                    let mission_id = discord_mission_id(&connector.id, channel_id, &message.id);
                    if state.storage.get_mission(&mission_id)?.is_none() {
                        let mut mission = Mission::new(title.clone(), details);
                        mission.id = mission_id;
                        mission.alias = connector.alias.clone();
                        mission.requested_model = connector.requested_model.clone();
                        mission.status = MissionStatus::Queued;
                        mission.wake_trigger = Some(WakeTrigger::Discord);
                        if let Some(cwd) = connector.cwd.clone() {
                            mission.workspace_key =
                                Some(resolve_request_cwd(Some(cwd))?.display().to_string());
                        }
                        state.storage.upsert_mission(&mission)?;
                        append_log(
                            state,
                            "info",
                            "discord",
                            format!(
                                "discord '{}' queued mission '{}'",
                                connector.id, mission.title
                            ),
                        )?;
                        queued_missions += 1;
                    }
                }
            }
            if next_message_id != latest_message_id {
                persist_discord_channel_cursor(
                    state,
                    &connector.id,
                    channel_id,
                    next_message_id.clone(),
                )
                .await?;
                latest_message_id = next_message_id;
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

    Ok(DiscordPollResponse {
        connector_id: connector.id.clone(),
        processed_messages,
        queued_missions,
        pending_approvals,
        updated_channels,
    })
}

pub(super) fn load_discord_bot_token(
    connector: &DiscordConnectorConfig,
) -> Result<String, ApiError> {
    let account = connector
        .bot_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "discord connector '{}' has no bot token configured",
                    connector.id
                ),
            )
        })?;
    load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to load discord bot token for connector '{}': {error}",
                connector.id
            ),
        )
    })
}

pub(super) fn discord_bot_token_account_in_use(
    connectors: &[DiscordConnectorConfig],
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

async fn fetch_discord_messages(
    state: &AppState,
    token: &str,
    channel_id: &str,
    after: Option<&str>,
) -> Result<Vec<DiscordMessage>, ApiError> {
    let url = super::connector_service_url(
        "https://discord.com",
        "NUCLEAR_DISCORD_API_BASE_URL",
        &format!("/api/v10/channels/{channel_id}/messages"),
    );
    let mut request = state
        .http_client
        .get(url)
        .header("Authorization", format!("Bot {token}"))
        .query(&[("limit", 50_u32)]);
    if let Some(after) = after.filter(|value| !value.trim().is_empty()) {
        request = request.query(&[("after", after)]);
    }
    let response = request.send().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("discord get messages request failed: {error}"),
        )
    })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read discord get messages response: {error}"),
        )
    })?;
    if !status.is_success() {
        let parsed = serde_json::from_str::<DiscordApiError>(&body).ok();
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "discord get messages failed: {}",
                parsed
                    .and_then(|payload| payload.message)
                    .unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    serde_json::from_str::<Vec<DiscordMessage>>(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse discord messages response: {error}"),
        )
    })
}

pub(super) async fn send_discord_message(
    client: &reqwest::Client,
    token: &str,
    payload: &DiscordSendRequest,
) -> Result<String, ApiError> {
    let url = super::connector_service_url(
        "https://discord.com",
        "NUCLEAR_DISCORD_API_BASE_URL",
        &format!("/api/v10/channels/{}/messages", payload.channel_id),
    );
    let response = client
        .post(url)
        .header("Authorization", format!("Bot {token}"))
        .json(&serde_json::json!({ "content": payload.content }))
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("discord send message request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read discord send message response: {error}"),
        )
    })?;
    if !status.is_success() {
        let parsed = serde_json::from_str::<DiscordApiError>(&body).ok();
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "discord send message failed: {}",
                parsed
                    .and_then(|payload| payload.message)
                    .unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    let parsed = serde_json::from_str::<DiscordSendMessageResponse>(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse discord send message response: {error}"),
        )
    })?;
    Ok(parsed.id)
}

async fn persist_discord_channel_cursor(
    state: &AppState,
    connector_id: &str,
    channel_id: &str,
    last_message_id: Option<String>,
) -> Result<(), ApiError> {
    let mut config = state.config.write().await;
    let Some(connector) = config
        .discord_connectors
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
        cursor.last_message_id = last_message_id;
    } else {
        connector
            .channel_cursors
            .push(agent_core::DiscordChannelCursor {
                channel_id: channel_id.to_string(),
                last_message_id,
            });
    }
    connector
        .channel_cursors
        .sort_by(|left, right| left.channel_id.cmp(&right.channel_id));
    state.storage.save_config(&config)?;
    Ok(())
}

#[allow(clippy::large_enum_variant)]
enum DiscordMessageAction {
    Ignore,
    Pending(ConnectorApprovalRecord),
    Queue { title: String, details: String },
}

fn discord_message_action(
    connector: &DiscordConnectorConfig,
    message: &DiscordMessage,
) -> DiscordMessageAction {
    if message.author.bot {
        return DiscordMessageAction::Ignore;
    }
    let Some(text) = discord_message_text(message) else {
        return DiscordMessageAction::Ignore;
    };
    let channel_id = message.channel_id.as_deref().unwrap_or("unknown");
    let title = format!(
        "{} discord: {}",
        connector.name,
        truncate_for_title(&text, 72)
    );
    let details = format!(
        "Discord connector: {}\nChannel: {} (id {})\nGuild id: {}\nSender: {}\nMessage id: {}\nTimestamp: {}\nMessage:\n{}",
        connector.name,
        discord_channel_label(message),
        channel_id,
        message.guild_id.as_deref().unwrap_or("-"),
        discord_sender_label(&message.author),
        message.id,
        message.timestamp.as_deref().unwrap_or("-"),
        text
    );
    if connector.require_pairing_approval {
        let channel_allowed = connector
            .allowed_channel_ids
            .iter()
            .any(|value| value == channel_id);
        let user_allowed = connector.allowed_user_ids.is_empty()
            || connector
                .allowed_user_ids
                .iter()
                .any(|value| value == &message.author.id);
        if !channel_allowed || !user_allowed {
            return DiscordMessageAction::Pending(build_discord_pairing_approval(
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
            return DiscordMessageAction::Ignore;
        }
        if !connector.allowed_user_ids.is_empty()
            && !connector
                .allowed_user_ids
                .iter()
                .any(|value| value == &message.author.id)
        {
            return DiscordMessageAction::Ignore;
        }
    }

    DiscordMessageAction::Queue { title, details }
}

fn build_discord_pairing_approval(
    connector: &DiscordConnectorConfig,
    message: &DiscordMessage,
    title: String,
    details: String,
    text: &str,
) -> ConnectorApprovalRecord {
    let source_key = discord_pairing_source_key(connector, message);
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Discord,
        connector.id.clone(),
        connector.name.clone(),
        title,
        details,
        source_key.clone(),
    );
    approval.id = discord_pairing_approval_id(&source_key);
    approval.source_event_id = Some(message.id.clone());
    approval.external_chat_id = message.channel_id.clone();
    approval.external_chat_display = Some(discord_channel_label(message));
    approval.external_user_id = Some(message.author.id.clone());
    approval.external_user_display = Some(discord_sender_label(&message.author));
    approval.message_preview = Some(truncate_for_title(text, 240));
    approval
}

fn discord_pairing_source_key(
    connector: &DiscordConnectorConfig,
    message: &DiscordMessage,
) -> String {
    let channel_id = message.channel_id.as_deref().unwrap_or("unknown");
    let user_scope = if connector.allowed_user_ids.is_empty() {
        "any"
    } else {
        message.author.id.as_str()
    };
    format!(
        "discord:{}:channel:{}:user:{}",
        connector.id.trim(),
        channel_id,
        user_scope
    )
}

fn discord_pairing_approval_id(source_key: &str) -> String {
    format!("connector-approval:{source_key}")
}

pub(super) fn add_discord_pairing_allowlist_entries(
    connector: &mut DiscordConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<(), ApiError> {
    if let Some(channel_id) = approval.external_chat_id.as_deref().map(str::trim) {
        if channel_id.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "discord approval has an empty channel id",
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
                "discord approval has an empty user id",
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

pub(super) fn queue_discord_approval_mission(
    state: &AppState,
    connector: &DiscordConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<String, ApiError> {
    let channel_id = approval
        .external_chat_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let message_id = approval
        .source_event_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&approval.id);
    let mission_id = discord_mission_id(&connector.id, channel_id, message_id);
    if state.storage.get_mission(&mission_id)?.is_none() {
        let mut mission = Mission::new(approval.title.clone(), approval.details.clone());
        mission.id = mission_id.clone();
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Discord);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        state.autopilot_wake.notify_waiters();
    }
    Ok(mission_id)
}

fn discord_message_text(message: &DiscordMessage) -> Option<String> {
    let content = message.content.trim();
    if !content.is_empty() {
        return Some(content.to_string());
    }
    let attachments = message
        .attachments
        .iter()
        .map(|attachment| {
            if attachment.url.trim().is_empty() {
                attachment.filename.clone()
            } else {
                format!("{} ({})", attachment.filename, attachment.url)
            }
        })
        .collect::<Vec<_>>();
    if attachments.is_empty() {
        None
    } else {
        Some(format!("Attachments:\n{}", attachments.join("\n")))
    }
}

fn discord_channel_label(message: &DiscordMessage) -> String {
    message
        .channel_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            message
                .channel_id
                .as_deref()
                .map(|value| format!("channel {value}"))
                .unwrap_or_else(|| "unknown channel".to_string())
        })
}

fn discord_sender_label(user: &DiscordUser) -> String {
    let mut parts = Vec::new();
    if let Some(global_name) = user
        .global_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(global_name.to_string());
    }
    let username = user.username.trim();
    if !username.is_empty() {
        parts.push(username.to_string());
    }
    parts.push(format!("id={}", user.id));
    parts.join(" ")
}

fn discord_mission_id(connector_id: &str, channel_id: &str, message_id: &str) -> String {
    format!(
        "discord:{}:{}:{}",
        connector_id.trim(),
        channel_id.trim(),
        message_id.trim()
    )
}

fn discord_message_id_cmp(left: &DiscordMessage, right: &DiscordMessage) -> std::cmp::Ordering {
    compare_discord_ids(&left.id, &right.id)
}

fn compare_discord_ids(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.trim().parse::<u64>(), right.trim().parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

#[derive(Debug, Default, Deserialize)]
struct DiscordApiError {
    #[serde(default)]
    message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DiscordMessage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    guild_id: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    author: DiscordUser,
    #[serde(default)]
    attachments: Vec<DiscordAttachment>,
    #[serde(default)]
    channel_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DiscordUser {
    #[serde(default)]
    id: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    global_name: Option<String>,
    #[serde(default)]
    bot: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DiscordAttachment {
    #[serde(default)]
    filename: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DiscordSendMessageResponse {
    #[serde(default)]
    id: String,
}
