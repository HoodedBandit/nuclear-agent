use agent_providers::load_api_key;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;

use super::*;

pub(super) async fn poll_gmail_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .gmail_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_gmail_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_gmail_connector(
    state: &AppState,
    connector: &GmailConnectorConfig,
) -> Result<GmailPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(GmailPollResponse {
            connector_id: connector.id.clone(),
            processed_messages: 0,
            queued_missions: 0,
            pending_approvals: 0,
        });
    }

    let token = load_gmail_oauth_token(connector)?;
    let messages = fetch_gmail_messages(state, &token, connector).await?;
    let processed_messages = messages.len();
    let mut queued_missions = 0usize;
    let mut pending_approvals = 0usize;

    for msg_stub in &messages {
        let detail = fetch_gmail_message_detail(state, &token, &msg_stub.id).await?;
        let from_address = gmail_extract_header(&detail, "From").unwrap_or_default();
        let subject =
            gmail_extract_header(&detail, "Subject").unwrap_or_else(|| "(no subject)".to_string());
        let body_text = detail.snippet.clone().unwrap_or_default();

        match gmail_message_action(connector, &from_address, &subject, &body_text, &msg_stub.id) {
            GmailMessageAction::Ignore => {}
            GmailMessageAction::Pending(mut approval) => {
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
                        "gmail",
                        format!(
                            "gmail '{}' requires pairing approval for sender={} (run 'nuclear gmail approvals' to review)",
                            connector.id,
                            approval.external_user_id.as_deref().unwrap_or("-"),
                        ),
                    )?;
                }
                pending_approvals += 1;
            }
            GmailMessageAction::Queue { title, details } => {
                let mission_id = gmail_mission_id(&connector.id, &msg_stub.id);
                if state.storage.get_mission(&mission_id)?.is_none() {
                    let mut mission = Mission::new(title.clone(), details);
                    mission.id = mission_id;
                    mission.alias = connector.alias.clone();
                    mission.requested_model = connector.requested_model.clone();
                    mission.status = MissionStatus::Queued;
                    mission.wake_trigger = Some(WakeTrigger::Gmail);
                    if let Some(cwd) = connector.cwd.clone() {
                        mission.workspace_key =
                            Some(resolve_request_cwd(Some(cwd))?.display().to_string());
                    }
                    state.storage.upsert_mission(&mission)?;
                    append_log(
                        state,
                        "info",
                        "gmail",
                        format!(
                            "gmail '{}' queued mission '{}'",
                            connector.id, mission.title
                        ),
                    )?;
                    queued_missions += 1;
                }
            }
        }
    }

    // Persist cursor after processing
    if let Some(last_msg) = messages.last() {
        persist_gmail_cursor(state, &connector.id, Some(last_msg.id.clone())).await?;
    }

    if queued_missions > 0 {
        state.autopilot_wake.notify_waiters();
    }

    Ok(GmailPollResponse {
        connector_id: connector.id.clone(),
        processed_messages,
        queued_missions,
        pending_approvals,
    })
}

pub(super) fn load_gmail_oauth_token(connector: &GmailConnectorConfig) -> Result<String, ApiError> {
    let account = connector
        .oauth_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "gmail connector '{}' has no OAuth token configured",
                    connector.id
                ),
            )
        })?;
    load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to load gmail OAuth token for connector '{}': {error}",
                connector.id
            ),
        )
    })
}

pub(super) fn gmail_oauth_account_in_use(
    connectors: &[GmailConnectorConfig],
    account: &str,
) -> bool {
    let account = account.trim();
    !account.is_empty()
        && connectors.iter().any(|connector| {
            connector
                .oauth_keychain_account
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| value == account)
        })
}

fn gmail_mission_id(connector_id: &str, message_id: &str) -> String {
    format!("gmail:{}:{}", connector_id.trim(), message_id)
}

async fn fetch_gmail_messages(
    state: &AppState,
    token: &str,
    connector: &GmailConnectorConfig,
) -> Result<Vec<GmailMessageStub>, ApiError> {
    let label = connector.label_filter.as_deref().unwrap_or("INBOX");
    let query = format!("is:unread label:{}", label);
    let url = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
    let response = state
        .http_client
        .get(url)
        .bearer_auth(token)
        .query(&[("q", query.as_str()), ("maxResults", "25")])
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("gmail messages.list request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read gmail messages.list response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("gmail messages.list failed ({}): {}", status, body),
        ));
    }
    let parsed: GmailMessagesListResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse gmail messages.list response: {error}"),
        )
    })?;
    Ok(parsed.messages.unwrap_or_default())
}

pub(super) async fn fetch_gmail_message_detail(
    state: &AppState,
    token: &str,
    message_id: &str,
) -> Result<GmailMessageDetail, ApiError> {
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
        message_id
    );
    let response = state
        .http_client
        .get(&url)
        .bearer_auth(token)
        .query(&[
            ("format", "metadata"),
            ("metadataHeaders", "From"),
            ("metadataHeaders", "Subject"),
        ])
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("gmail messages.get request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read gmail messages.get response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("gmail messages.get failed ({}): {}", status, body),
        ));
    }
    let parsed: GmailMessageDetail = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse gmail messages.get response: {error}"),
        )
    })?;
    Ok(parsed)
}

pub(super) async fn send_gmail_message(
    client: &reqwest::Client,
    token: &str,
    to: &str,
    subject: &str,
    body_text: &str,
) -> Result<Option<String>, ApiError> {
    let rfc2822 = format!(
        "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
        to, subject, body_text
    );
    let encoded = URL_SAFE_NO_PAD.encode(rfc2822.as_bytes());
    let url = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "raw": encoded }))
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("gmail messages.send request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read gmail messages.send response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("gmail messages.send failed ({}): {}", status, body),
        ));
    }
    let parsed: GmailSendMessageResponse = serde_json::from_str(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse gmail messages.send response: {error}"),
        )
    })?;
    Ok(parsed.id)
}

async fn persist_gmail_cursor(
    state: &AppState,
    connector_id: &str,
    last_history_id: Option<String>,
) -> Result<(), ApiError> {
    let mut config = state.config.write().await;
    let Some(connector) = config
        .gmail_connectors
        .iter_mut()
        .find(|connector| connector.id == connector_id)
    else {
        return Ok(());
    };
    connector.last_history_id = last_history_id;
    state.storage.save_config(&config)?;
    Ok(())
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(super) enum GmailMessageAction {
    Ignore,
    Pending(ConnectorApprovalRecord),
    Queue { title: String, details: String },
}

pub(super) fn gmail_message_action(
    connector: &GmailConnectorConfig,
    from_address: &str,
    subject: &str,
    body_text: &str,
    message_id: &str,
) -> GmailMessageAction {
    let from_trimmed = from_address.trim();
    if from_trimmed.is_empty() {
        return GmailMessageAction::Ignore;
    }

    let display_text = if body_text.trim().is_empty() {
        subject
    } else {
        body_text
    };

    let title = format!(
        "{} gmail: {}",
        connector.name,
        truncate_for_title(display_text, 72)
    );
    let details = format!(
        "Gmail connector: {}\nMessage id: {}\nFrom: {}\nSubject: {}\nBody:\n{}",
        connector.name, message_id, from_trimmed, subject, body_text
    );

    // Normalize the sender address for comparison (extract email from "Name <email>" format)
    let sender_email = extract_email_address(from_trimmed);

    if connector.require_pairing_approval {
        let sender_allowed = connector
            .allowed_sender_addresses
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&sender_email));
        if !sender_allowed {
            return GmailMessageAction::Pending(build_gmail_pairing_approval(
                connector,
                from_trimmed,
                &sender_email,
                message_id,
                title,
                details,
                display_text,
            ));
        }
    } else if !connector.allowed_sender_addresses.is_empty() {
        let sender_allowed = connector
            .allowed_sender_addresses
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(&sender_email));
        if !sender_allowed {
            return GmailMessageAction::Ignore;
        }
    }

    GmailMessageAction::Queue { title, details }
}

fn build_gmail_pairing_approval(
    connector: &GmailConnectorConfig,
    from_display: &str,
    sender_email: &str,
    message_id: &str,
    title: String,
    details: String,
    text: &str,
) -> ConnectorApprovalRecord {
    let source_key = gmail_pairing_source_key(connector, sender_email);
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Gmail,
        connector.id.clone(),
        connector.name.clone(),
        title,
        details,
        source_key.clone(),
    );
    approval.id = gmail_pairing_approval_id(&source_key);
    approval.source_event_id = Some(message_id.to_string());
    approval.external_chat_id = None;
    approval.external_chat_display = None;
    approval.external_user_id = Some(sender_email.to_string());
    approval.external_user_display = Some(from_display.to_string());
    approval.message_preview = Some(truncate_for_title(text, 240));
    approval
}

fn gmail_pairing_source_key(connector: &GmailConnectorConfig, sender_email: &str) -> String {
    format!(
        "gmail:{}:sender:{}",
        connector.id.trim(),
        sender_email.to_lowercase()
    )
}

fn gmail_pairing_approval_id(source_key: &str) -> String {
    format!("connector-approval:{source_key}")
}

pub(super) fn add_gmail_pairing_allowlist_entries(
    connector: &mut GmailConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<(), ApiError> {
    if let Some(sender) = approval.external_user_id.as_deref() {
        let sender_lower = sender.trim().to_lowercase();
        if !sender_lower.is_empty()
            && !connector
                .allowed_sender_addresses
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&sender_lower))
        {
            connector.allowed_sender_addresses.push(sender_lower);
            connector.allowed_sender_addresses.sort();
            connector.allowed_sender_addresses.dedup();
        }
    }
    Ok(())
}

pub(super) fn queue_gmail_approval_mission(
    state: &AppState,
    connector: &GmailConnectorConfig,
    approval: &ConnectorApprovalRecord,
) -> Result<String, ApiError> {
    let mission_id = approval
        .source_event_id
        .as_deref()
        .map(|msg_id| gmail_mission_id(&connector.id, msg_id))
        .unwrap_or_else(|| format!("gmail:{}:approval:{}", connector.id, approval.id));
    if state.storage.get_mission(&mission_id)?.is_none() {
        let mut mission = Mission::new(approval.title.clone(), approval.details.clone());
        mission.id = mission_id.clone();
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Gmail);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        state.autopilot_wake.notify_waiters();
    }
    Ok(mission_id)
}

/// Extract an email address from a string like "Display Name <user@example.com>" or just "user@example.com".
fn extract_email_address(from: &str) -> String {
    if let Some(start) = from.rfind('<') {
        if let Some(end) = from[start..].find('>') {
            return from[start + 1..start + end].trim().to_lowercase();
        }
    }
    from.trim().to_lowercase()
}

fn gmail_extract_header(detail: &GmailMessageDetail, name: &str) -> Option<String> {
    detail
        .payload
        .as_ref()?
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
}

// --- Gmail API response types ---

#[derive(Debug, Clone, Default, Deserialize)]
struct GmailMessagesListResponse {
    #[serde(default)]
    messages: Option<Vec<GmailMessageStub>>,
    #[serde(default)]
    #[allow(dead_code)]
    result_size_estimate: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GmailMessageStub {
    pub(super) id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) thread_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GmailMessageDetail {
    #[allow(dead_code)]
    pub(super) id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) thread_id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) label_ids: Vec<String>,
    #[serde(default)]
    pub(super) snippet: Option<String>,
    #[serde(default)]
    pub(super) payload: Option<GmailMessagePayload>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GmailMessagePayload {
    #[serde(default)]
    pub(super) headers: Vec<GmailHeader>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct GmailHeader {
    pub(super) name: String,
    pub(super) value: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GmailSendMessageResponse {
    #[serde(default)]
    id: Option<String>,
}

// Reuse local reference to messages for cursor tracking
impl GmailMessageStub {
    fn _id(&self) -> &str {
        &self.id
    }
}
