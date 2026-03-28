use agent_providers::load_api_key;
use serde::Deserialize;

use super::*;

pub(super) async fn poll_telegram_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .telegram_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_telegram_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_telegram_connector(
    state: &AppState,
    connector: &TelegramConnectorConfig,
) -> Result<TelegramPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(TelegramPollResponse {
            connector_id: connector.id.clone(),
            processed_updates: 0,
            queued_missions: 0,
            pending_approvals: 0,
            last_update_id: connector.last_update_id,
        });
    }

    let token = load_telegram_bot_token(connector)?;
    let offset = connector
        .last_update_id
        .map(|value| value.saturating_add(1));
    let updates = fetch_telegram_updates(state, &token, offset).await?;
    let processed_updates = updates.result.len();
    let mut queued_missions = 0usize;
    let mut pending_approvals = 0usize;
    let mut persisted_update_id = connector.last_update_id;

    for update in updates.result {
        let next_update_id = telegram_next_update_id(persisted_update_id, update.update_id);
        match telegram_update_action(connector, &update) {
            TelegramUpdateAction::Ignore => {}
            TelegramUpdateAction::Pending(mut approval) => {
                let existing = state.storage.get_connector_approval(&approval.id)?;
                let is_new = existing.is_none();
                if let Some(existing) = existing {
                    if existing.status == ConnectorApprovalStatus::Rejected {
                        if next_update_id != persisted_update_id {
                            persist_telegram_cursor(state, &connector.id, next_update_id).await?;
                            persisted_update_id = next_update_id;
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
                        "telegram",
                        format!(
                            "telegram '{}' requires pairing approval for chat={} user={} (run 'nuclear telegram approvals' to review)",
                            connector.id,
                            approval.external_chat_id.as_deref().unwrap_or("-"),
                            approval.external_user_id.as_deref().unwrap_or("-")
                        ),
                    )?;
                }
                pending_approvals += 1;
            }
            TelegramUpdateAction::Queue { title, details } => {
                let mission_id = telegram_mission_id(&connector.id, update.update_id);
                if state.storage.get_mission(&mission_id)?.is_none() {
                    let mut mission = Mission::new(title.clone(), details);
                    mission.id = mission_id;
                    mission.alias = connector.alias.clone();
                    mission.requested_model = connector.requested_model.clone();
                    mission.status = MissionStatus::Queued;
                    mission.wake_trigger = Some(WakeTrigger::Telegram);
                    if let Some(cwd) = connector.cwd.clone() {
                        mission.workspace_key =
                            Some(resolve_request_cwd(Some(cwd))?.display().to_string());
                    }
                    state.storage.upsert_mission(&mission)?;
                    append_log(
                        state,
                        "info",
                        "telegram",
                        format!(
                            "telegram '{}' queued mission '{}'",
                            connector.id, mission.title
                        ),
                    )?;
                    queued_missions += 1;
                }
            }
        }
        if next_update_id != persisted_update_id {
            persist_telegram_cursor(state, &connector.id, next_update_id).await?;
            persisted_update_id = next_update_id;
        }
    }
    if queued_missions > 0 {
        state.autopilot_wake.notify_waiters();
    }

    Ok(TelegramPollResponse {
        connector_id: connector.id.clone(),
        processed_updates,
        queued_missions,
        pending_approvals,
        last_update_id: persisted_update_id,
    })
}

pub(super) fn load_telegram_bot_token(
    connector: &TelegramConnectorConfig,
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
                    "telegram connector '{}' has no bot token configured",
                    connector.id
                ),
            )
        })?;
    load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to load telegram bot token for connector '{}': {error}",
                connector.id
            ),
        )
    })
}

pub(super) fn telegram_bot_token_account_in_use(
    connectors: &[TelegramConnectorConfig],
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

pub(super) fn telegram_mission_id(connector_id: &str, update_id: i64) -> String {
    format!("telegram:{}:{}", connector_id.trim(), update_id)
}

pub(super) fn telegram_next_update_id(current: Option<i64>, update_id: i64) -> Option<i64> {
    Some(
        current
            .map(|existing| existing.max(update_id))
            .unwrap_or(update_id),
    )
}

async fn fetch_telegram_updates(
    state: &AppState,
    token: &str,
    offset: Option<i64>,
) -> Result<TelegramGetUpdatesResponse, ApiError> {
    let url = format!("https://api.telegram.org/bot{token}/getUpdates");
    let mut request = state
        .http_client
        .get(url)
        .query(&[("timeout", 0_i64), ("limit", 25_i64)]);
    if let Some(offset) = offset {
        request = request.query(&[("offset", offset)]);
    }
    let response = request.send().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            safe_telegram_request_error("telegram getUpdates request failed", &error),
        )
    })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read telegram getUpdates response: {error}"),
        )
    })?;
    let parsed: TelegramGetUpdatesResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse telegram getUpdates response: {error}"),
        )
    })?;
    if !status.is_success() || !parsed.ok {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "telegram getUpdates failed: {}",
                parsed.description.unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    Ok(parsed)
}

pub(super) async fn send_telegram_message(
    client: &reqwest::Client,
    token: &str,
    chat_id: i64,
    text: &str,
    disable_notification: bool,
) -> Result<Option<i64>, ApiError> {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let response = client
        .post(url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "disable_notification": disable_notification,
        }))
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                safe_telegram_request_error("telegram sendMessage request failed", &error),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read telegram sendMessage response: {error}"),
        )
    })?;
    let parsed: TelegramSendMessageResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse telegram sendMessage response: {error}"),
        )
    })?;
    if !status.is_success() || !parsed.ok {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "telegram sendMessage failed: {}",
                parsed.description.unwrap_or_else(|| status.to_string())
            ),
        ));
    }
    Ok(parsed.result.map(|message| message.message_id))
}

async fn persist_telegram_cursor(
    state: &AppState,
    connector_id: &str,
    last_update_id: Option<i64>,
) -> Result<(), ApiError> {
    let mut config = state.config.write().await;
    let Some(connector) = config
        .telegram_connectors
        .iter_mut()
        .find(|connector| connector.id == connector_id)
    else {
        return Ok(());
    };
    connector.last_update_id = last_update_id;
    state.storage.save_config(&config)?;
    Ok(())
}

fn safe_telegram_request_error(label: &str, error: &reqwest::Error) -> String {
    let detail = redact_telegram_bot_token_in_text(&error.to_string());
    if detail.trim().is_empty() {
        label.to_string()
    } else {
        format!("{label}: {detail}")
    }
}

fn redact_telegram_bot_token_in_text(text: &str) -> String {
    const PREFIX: &str = "https://api.telegram.org/bot";
    let mut cursor = 0usize;
    let mut output = String::new();
    while let Some(relative_index) = text[cursor..].find(PREFIX) {
        let index = cursor + relative_index;
        output.push_str(&text[cursor..index]);
        let token_start = index + PREFIX.len();
        let rest = &text[token_start..];
        let token_end = rest
            .find('/')
            .or_else(|| rest.find(char::is_whitespace))
            .unwrap_or(rest.len());
        output.push_str(PREFIX);
        output.push_str("[REDACTED]");
        cursor = token_start + token_end;
    }
    output.push_str(&text[cursor..]);
    output
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(super) enum TelegramUpdateAction {
    Ignore,
    Pending(ConnectorApprovalRecord),
    Queue { title: String, details: String },
}

pub(super) fn telegram_update_action(
    connector: &TelegramConnectorConfig,
    update: &TelegramUpdate,
) -> TelegramUpdateAction {
    let (source, message) = if let Some(message) = update.message.as_ref() {
        ("message", message)
    } else if let Some(message) = update.channel_post.as_ref() {
        ("channel_post", message)
    } else {
        return TelegramUpdateAction::Ignore;
    };

    if message.from.as_ref().is_some_and(|from| from.is_bot) {
        return TelegramUpdateAction::Ignore;
    }

    let Some(text) = telegram_message_text(message) else {
        return TelegramUpdateAction::Ignore;
    };
    let title = format!(
        "{} telegram: {}",
        connector.name,
        truncate_for_title(text, 72)
    );
    let details = format!(
        "Telegram connector: {}\nUpdate id: {}\nSource: {}\nChat: {} (id {}, type {})\nSender: {}\nMessage id: {}\nMessage date (unix): {}\nMessage:\n{}",
        connector.name,
        update.update_id,
        source,
        telegram_chat_label(&message.chat),
        message.chat.id,
        telegram_chat_type(&message.chat),
        telegram_sender_label(message.from.as_ref()),
        message.message_id,
        message.date,
        text
    );
    if connector.require_pairing_approval {
        let chat_allowed = connector.allowed_chat_ids.contains(&message.chat.id);
        let user_allowed = connector.allowed_user_ids.is_empty()
            || message
                .from
                .as_ref()
                .is_some_and(|from| connector.allowed_user_ids.contains(&from.id));
        if !chat_allowed || !user_allowed {
            return TelegramUpdateAction::Pending(build_telegram_pairing_approval(
                connector, update, message, title, details, text,
            ));
        }
    } else {
        if !connector.allowed_chat_ids.is_empty()
            && !connector.allowed_chat_ids.contains(&message.chat.id)
        {
            return TelegramUpdateAction::Ignore;
        }

        if !connector.allowed_user_ids.is_empty() {
            let Some(from) = message.from.as_ref() else {
                return TelegramUpdateAction::Ignore;
            };
            if !connector.allowed_user_ids.contains(&from.id) {
                return TelegramUpdateAction::Ignore;
            }
        }
    }

    let _ = source;
    TelegramUpdateAction::Queue { title, details }
}

#[cfg(test)]
mod tests {
    use super::redact_telegram_bot_token_in_text;

    #[test]
    fn redact_telegram_bot_token_in_url_text() {
        let redacted = redact_telegram_bot_token_in_text(
            "telegram getUpdates request failed: error sending request for url (https://api.telegram.org/bot123:abc/getUpdates)",
        );
        assert!(!redacted.contains("123:abc"));
        assert!(redacted.contains("https://api.telegram.org/bot[REDACTED]/getUpdates"));
    }
}

fn build_telegram_pairing_approval(
    connector: &TelegramConnectorConfig,
    update: &TelegramUpdate,
    message: &TelegramMessage,
    title: String,
    details: String,
    text: &str,
) -> ConnectorApprovalRecord {
    let source_key = telegram_pairing_source_key(connector, message);
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Telegram,
        connector.id.clone(),
        connector.name.clone(),
        title,
        details,
        source_key.clone(),
    );
    approval.id = telegram_pairing_approval_id(&source_key);
    approval.source_event_id = Some(update.update_id.to_string());
    approval.external_chat_id = Some(message.chat.id.to_string());
    approval.external_chat_display = Some(telegram_chat_label(&message.chat));
    approval.external_user_id = message.from.as_ref().map(|from| from.id.to_string());
    approval.external_user_display = Some(telegram_sender_label(message.from.as_ref()));
    approval.message_preview = Some(truncate_for_title(text, 240));
    approval
}

fn telegram_pairing_source_key(
    connector: &TelegramConnectorConfig,
    message: &TelegramMessage,
) -> String {
    let user_scope = if connector.allowed_user_ids.is_empty() {
        "any".to_string()
    } else {
        message
            .from
            .as_ref()
            .map(|from| from.id.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };
    format!(
        "telegram:{}:chat:{}:user:{}",
        connector.id.trim(),
        message.chat.id,
        user_scope
    )
}

fn telegram_pairing_approval_id(source_key: &str) -> String {
    format!("connector-approval:{source_key}")
}

pub(super) fn add_telegram_pairing_allowlist_entries(
    connector: &mut TelegramConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<(), ApiError> {
    if let Some(chat_id) = approval.external_chat_id.as_deref() {
        let chat_id = chat_id.parse::<i64>().map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("invalid telegram chat id '{}': {error}", chat_id),
            )
        })?;
        if !connector.allowed_chat_ids.contains(&chat_id) {
            connector.allowed_chat_ids.push(chat_id);
            connector.allowed_chat_ids.sort_unstable();
            connector.allowed_chat_ids.dedup();
        }
    }
    if let Some(user_id) = approval.external_user_id.as_deref() {
        let user_id = user_id.parse::<i64>().map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("invalid telegram user id '{}': {error}", user_id),
            )
        })?;
        if !connector.allowed_user_ids.contains(&user_id) {
            connector.allowed_user_ids.push(user_id);
            connector.allowed_user_ids.sort_unstable();
            connector.allowed_user_ids.dedup();
        }
    }
    Ok(())
}

pub(super) fn queue_telegram_approval_mission(
    state: &AppState,
    connector: &TelegramConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<String, ApiError> {
    let mission_id = approval
        .source_event_id
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|update_id| telegram_mission_id(&connector.id, update_id))
        .unwrap_or_else(|| format!("telegram:{}:approval:{}", connector.id, approval.id));
    if state.storage.get_mission(&mission_id)?.is_none() {
        let mut mission = Mission::new(approval.title.clone(), approval.details.clone());
        mission.id = mission_id.clone();
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Telegram);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        state.autopilot_wake.notify_waiters();
    }
    Ok(mission_id)
}

fn telegram_message_text(message: &TelegramMessage) -> Option<&str> {
    message
        .text
        .as_deref()
        .or(message.caption.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn telegram_chat_label(chat: &TelegramChat) -> String {
    chat.title
        .clone()
        .or_else(|| chat.username.as_ref().map(|value| format!("@{value}")))
        .or_else(|| {
            let first = chat.first_name.as_deref().unwrap_or("").trim();
            let last = chat.last_name.as_deref().unwrap_or("").trim();
            let combined = format!("{first} {last}").trim().to_string();
            (!combined.is_empty()).then_some(combined)
        })
        .unwrap_or_else(|| "unknown chat".to_string())
}

fn telegram_chat_type(chat: &TelegramChat) -> &str {
    if chat.kind.trim().is_empty() {
        "unknown"
    } else {
        chat.kind.as_str()
    }
}

fn telegram_sender_label(user: Option<&TelegramUser>) -> String {
    let Some(user) = user else {
        return "unknown sender".to_string();
    };
    let mut parts = Vec::new();
    let display = format!(
        "{} {}",
        user.first_name.trim(),
        user.last_name.as_deref().unwrap_or("").trim()
    )
    .trim()
    .to_string();
    if !display.is_empty() {
        parts.push(display);
    }
    if let Some(username) = user
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("@{username}"));
    }
    parts.push(format!("id={}", user.id));
    parts.join(" ")
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TelegramGetUpdatesResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    result: Vec<TelegramUpdate>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramSendMessageResponse {
    #[serde(default)]
    ok: bool,
    result: Option<TelegramMessageResult>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessageResult {
    message_id: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct TelegramUpdate {
    pub(super) update_id: i64,
    #[serde(default)]
    pub(super) message: Option<TelegramMessage>,
    #[serde(default)]
    pub(super) channel_post: Option<TelegramMessage>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct TelegramMessage {
    pub(super) message_id: i64,
    pub(super) date: i64,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) caption: Option<String>,
    #[serde(default)]
    pub(super) chat: TelegramChat,
    #[serde(default)]
    pub(super) from: Option<TelegramUser>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct TelegramChat {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) username: Option<String>,
    #[serde(rename = "type", default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) first_name: Option<String>,
    #[serde(default)]
    pub(super) last_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct TelegramUser {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) is_bot: bool,
    #[serde(default)]
    pub(super) username: Option<String>,
    #[serde(default)]
    pub(super) first_name: String,
    #[serde(default)]
    pub(super) last_name: Option<String>,
}
