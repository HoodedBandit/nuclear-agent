use crate::{append_log, resolve_request_cwd, ApiError, AppState};
use agent_core::{
    AppConnectorConfig, AppConnectorUpsertRequest, ConnectorApprovalRecord,
    ConnectorApprovalStatus, ConnectorApprovalUpdateRequest, ConnectorKind, DiscordConnectorConfig,
    DiscordConnectorUpsertRequest, DiscordPollResponse, DiscordSendRequest, DiscordSendResponse,
    GmailConnectorConfig, GmailConnectorUpsertRequest, GmailPollResponse, GmailSendRequest,
    GmailSendResponse, HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest,
    HomeAssistantEntityState, HomeAssistantPollResponse, HomeAssistantServiceCallRequest,
    HomeAssistantServiceCallResponse, InboxConnectorConfig, InboxConnectorUpsertRequest,
    InboxPollResponse, Mission, MissionStatus, SignalConnectorConfig, SignalConnectorUpsertRequest,
    SignalPollResponse, SignalSendRequest, SignalSendResponse, SlackConnectorConfig,
    SlackConnectorUpsertRequest, SlackPollResponse, SlackSendRequest, SlackSendResponse,
    TelegramConnectorConfig, TelegramConnectorUpsertRequest, TelegramPollResponse,
    TelegramSendRequest, TelegramSendResponse, WakeTrigger, WebhookConnectorConfig,
    WebhookConnectorUpsertRequest, WebhookEventRequest, WebhookEventResponse,
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};

mod admin;
mod approvals;
mod discord;
mod gmail;
mod home_assistant;
mod inbox;
mod signal;
mod slack;
mod telegram;
mod webhook;

pub(crate) use admin::{
    delete_app_connector, delete_discord_connector, delete_gmail_connector,
    delete_home_assistant_connector, delete_inbox_connector, delete_signal_connector,
    delete_slack_connector, delete_telegram_connector, delete_webhook_connector,
    get_discord_connector, get_gmail_connector, get_home_assistant_connector, get_inbox_connector,
    get_signal_connector, get_slack_connector, get_telegram_connector, get_webhook_connector,
    list_app_connectors, list_discord_connectors, list_gmail_connectors,
    list_home_assistant_connectors, list_inbox_connectors, list_signal_connectors,
    list_slack_connectors, list_telegram_connectors, list_webhook_connectors, upsert_app_connector,
    upsert_discord_connector, upsert_gmail_connector, upsert_home_assistant_connector,
    upsert_inbox_connector, upsert_signal_connector, upsert_slack_connector,
    upsert_telegram_connector, upsert_webhook_connector,
};
pub(crate) use approvals::{
    approve_connector_approval, list_connector_approvals, reject_connector_approval,
};

pub(crate) async fn poll_inbox_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<InboxPollResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .inbox_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown inbox connector"))?
    };
    let (processed_files, queued_missions) = inbox::process_inbox_connector(&state, &connector)?;
    Ok(Json(InboxPollResponse {
        connector_id: connector.id,
        processed_files,
        queued_missions,
    }))
}

pub(crate) async fn poll_telegram_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<TelegramPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .telegram_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown telegram connector"))?
    };
    Ok(Json(
        telegram::process_telegram_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn send_telegram_message_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<TelegramSendRequest>,
) -> Result<Json<TelegramSendResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .telegram_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown telegram connector"))?
    };
    ensure_connector_enabled(
        connector.enabled,
        "telegram",
        &connector.id,
        "sending messages",
    )?;
    let text = payload.text.trim();
    if text.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "telegram message text must not be empty",
        ));
    }
    let token = telegram::load_telegram_bot_token(&connector)?;
    let message_id = telegram::send_telegram_message(
        &state.http_client,
        &token,
        payload.chat_id,
        text,
        payload.disable_notification,
    )
    .await?;
    append_log(
        &state,
        "info",
        "telegram",
        format!(
            "telegram '{}' sent outbound message to chat {}",
            connector.id, payload.chat_id
        ),
    )?;
    Ok(Json(TelegramSendResponse {
        connector_id: connector.id,
        chat_id: payload.chat_id,
        message_id,
    }))
}

pub(crate) async fn poll_discord_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<DiscordPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .discord_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown discord connector"))?
    };
    Ok(Json(
        discord::process_discord_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn send_discord_message_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<DiscordSendRequest>,
) -> Result<Json<DiscordSendResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .discord_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown discord connector"))?
    };
    ensure_connector_enabled(
        connector.enabled,
        "discord",
        &connector.id,
        "sending messages",
    )?;
    let channel_id = payload.channel_id.trim();
    if channel_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "discord channel_id must not be empty",
        ));
    }
    let content = payload.content.trim();
    if content.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "discord message content must not be empty",
        ));
    }
    let token = discord::load_discord_bot_token(&connector)?;
    let message_id = discord::send_discord_message(
        &state.http_client,
        &token,
        &DiscordSendRequest {
            channel_id: channel_id.to_string(),
            content: content.to_string(),
        },
    )
    .await?;
    append_log(
        &state,
        "info",
        "discord",
        format!(
            "discord '{}' sent outbound message to channel {}",
            connector.id, channel_id
        ),
    )?;
    Ok(Json(DiscordSendResponse {
        connector_id: connector.id,
        channel_id: channel_id.to_string(),
        message_id: Some(message_id),
    }))
}

pub(crate) async fn poll_slack_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<SlackPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .slack_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown slack connector"))?
    };
    Ok(Json(
        slack::process_slack_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn send_slack_message_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<SlackSendRequest>,
) -> Result<Json<SlackSendResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .slack_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown slack connector"))?
    };
    ensure_connector_enabled(
        connector.enabled,
        "slack",
        &connector.id,
        "sending messages",
    )?;
    let channel_id = payload.channel_id.trim();
    if channel_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "slack channel_id must not be empty",
        ));
    }
    let text = payload.text.trim();
    if text.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "slack message text must not be empty",
        ));
    }
    let token = slack::load_slack_bot_token(&connector)?;
    let (response_channel, message_ts) = slack::send_slack_message(
        &state.http_client,
        &token,
        &SlackSendRequest {
            channel_id: channel_id.to_string(),
            text: text.to_string(),
        },
    )
    .await?;
    append_log(
        &state,
        "info",
        "slack",
        format!(
            "slack '{}' sent outbound message to channel {}",
            connector.id, channel_id
        ),
    )?;
    Ok(Json(SlackSendResponse {
        connector_id: connector.id,
        channel_id: response_channel.unwrap_or_else(|| channel_id.to_string()),
        message_ts,
    }))
}

pub(crate) async fn poll_home_assistant_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<HomeAssistantPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(StatusCode::NOT_FOUND, "unknown Home Assistant connector")
            })?
    };
    Ok(Json(
        home_assistant::process_home_assistant_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn get_home_assistant_entity_state_route(
    State(state): State<AppState>,
    Path((connector_id, entity_id)): Path<(String, String)>,
) -> Result<Json<HomeAssistantEntityState>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(StatusCode::NOT_FOUND, "unknown Home Assistant connector")
            })?
    };
    ensure_connector_enabled(
        connector.enabled,
        "Home Assistant",
        &connector.id,
        "state reads",
    )?;
    home_assistant::ensure_home_assistant_entity_allowed(&connector, entity_id.trim(), "read")?;
    let token = home_assistant::load_home_assistant_token(&connector)?;
    let state_response = home_assistant::fetch_home_assistant_entity_state(
        &state.http_client,
        &connector,
        &token,
        entity_id.trim(),
    )
    .await?;
    Ok(Json(state_response))
}

pub(crate) async fn call_home_assistant_service_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<HomeAssistantServiceCallRequest>,
) -> Result<Json<HomeAssistantServiceCallResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(StatusCode::NOT_FOUND, "unknown Home Assistant connector")
            })?
    };
    ensure_connector_enabled(
        connector.enabled,
        "Home Assistant",
        &connector.id,
        "service calls",
    )?;
    let domain = payload.domain.trim();
    if domain.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant service domain must not be empty",
        ));
    }
    let service = payload.service.trim();
    if service.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant service name must not be empty",
        ));
    }
    let token = home_assistant::load_home_assistant_token(&connector)?;
    let changed_entities = home_assistant::call_home_assistant_service(
        &state.http_client,
        &connector,
        &token,
        &HomeAssistantServiceCallRequest {
            domain: domain.to_string(),
            service: service.to_string(),
            entity_id: payload
                .entity_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
            service_data: payload.service_data.clone(),
        },
    )
    .await?;
    append_log(
        &state,
        "info",
        "home_assistant",
        format!(
            "Home Assistant '{}' called service {}.{}",
            connector.id, domain, service
        ),
    )?;
    Ok(Json(HomeAssistantServiceCallResponse {
        connector_id: connector.id,
        domain: domain.to_string(),
        service: service.to_string(),
        changed_entities,
    }))
}

pub(crate) async fn poll_signal_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<SignalPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .signal_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown signal connector"))?
    };
    Ok(Json(
        signal::process_signal_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn send_signal_message_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<SignalSendRequest>,
) -> Result<Json<SignalSendResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .signal_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown signal connector"))?
    };
    ensure_connector_enabled(
        connector.enabled,
        "signal",
        &connector.id,
        "sending messages",
    )?;
    let target = signal::send_signal_message(&connector, &payload).await?;
    append_log(
        &state,
        "info",
        "signal",
        format!(
            "signal '{}' sent outbound message to {}",
            connector.id, target
        ),
    )?;
    Ok(Json(SignalSendResponse {
        connector_id: connector.id,
        target,
    }))
}

pub(crate) async fn poll_gmail_connector_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<GmailPollResponse>, ApiError> {
    refresh_connector_config(&state).await?;
    let connector = {
        let config = state.config.read().await;
        config
            .gmail_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown gmail connector"))?
    };
    Ok(Json(
        gmail::process_gmail_connector(&state, &connector).await?,
    ))
}

pub(crate) async fn send_gmail_message_route(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    Json(payload): Json<GmailSendRequest>,
) -> Result<Json<GmailSendResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .gmail_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown gmail connector"))?
    };
    ensure_connector_enabled(
        connector.enabled,
        "gmail",
        &connector.id,
        "sending messages",
    )?;
    let to = payload.to.trim();
    if to.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "gmail recipient must not be empty",
        ));
    }
    let subject = payload.subject.trim();
    let body = payload.body.trim();
    if body.is_empty() && subject.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "gmail message must have a subject or body",
        ));
    }
    let token = gmail::load_gmail_oauth_token(&connector)?;
    let message_id = gmail::send_gmail_message(
        &state.http_client,
        &token,
        to,
        subject,
        body,
    )
    .await?;
    append_log(
        &state,
        "info",
        "gmail",
        format!(
            "gmail '{}' sent outbound message to {}",
            connector.id, to
        ),
    )?;
    Ok(Json(GmailSendResponse {
        connector_id: connector.id,
        message_id,
    }))
}

pub(crate) async fn receive_webhook_event(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<WebhookEventRequest>,
) -> Result<Json<WebhookEventResponse>, ApiError> {
    let connector = {
        let config = state.config.read().await;
        config
            .webhook_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown webhook connector"))?
    };

    if !connector.enabled {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "webhook connector is disabled",
        ));
    }
    webhook::verify_webhook_token(&connector, &headers)?;

    let title = payload
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{} webhook event", connector.name));
    let details = webhook::render_webhook_prompt(&connector, &payload);
    if details.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "webhook event resolved to an empty mission prompt",
        ));
    }

    let mut mission = Mission::new(title.clone(), details);
    mission.alias = connector.alias.clone();
    mission.requested_model = connector.requested_model.clone();
    mission.status = MissionStatus::Queued;
    mission.wake_trigger = Some(WakeTrigger::Webhook);
    if let Some(cwd) = connector.cwd.clone() {
        mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
    }
    state.storage.upsert_mission(&mission)?;
    state.autopilot_wake.notify_waiters();
    append_log(
        &state,
        "info",
        "webhooks",
        format!(
            "webhook '{}' queued mission '{}'",
            connector.id, mission.title
        ),
    )?;
    Ok(Json(WebhookEventResponse {
        connector_id: connector.id,
        mission_id: mission.id,
        title,
        status: mission.status,
    }))
}

pub(crate) async fn poll_inbox_connectors(state: &AppState) -> Result<usize, ApiError> {
    refresh_connector_config(state).await?;
    let connectors = {
        let config = state.config.read().await;
        config
            .inbox_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let (_, queued_missions) = inbox::process_inbox_connector(state, &connector)?;
        queued += queued_missions;
    }
    queued += telegram::poll_telegram_connectors(state).await?;
    queued += discord::poll_discord_connectors(state).await?;
    queued += slack::poll_slack_connectors(state).await?;
    queued += home_assistant::poll_home_assistant_connectors(state).await?;
    queued += signal::poll_signal_connectors(state).await?;
    queued += gmail::poll_gmail_connectors(state).await?;
    Ok(queued)
}

fn connector_display_name(kind: ConnectorKind) -> &'static str {
    match kind {
        ConnectorKind::App => "app",
        ConnectorKind::Webhook => "webhook",
        ConnectorKind::Inbox => "inbox",
        ConnectorKind::Telegram => "telegram",
        ConnectorKind::Discord => "discord",
        ConnectorKind::Slack => "slack",
        ConnectorKind::HomeAssistant => "home assistant",
        ConnectorKind::Signal => "signal",
        ConnectorKind::Gmail => "gmail",
    }
}

fn connector_log_category(kind: ConnectorKind) -> &'static str {
    match kind {
        ConnectorKind::App => "apps",
        ConnectorKind::Webhook => "webhooks",
        ConnectorKind::Inbox => "inboxes",
        ConnectorKind::Telegram => "telegram",
        ConnectorKind::Discord => "discord",
        ConnectorKind::Slack => "slack",
        ConnectorKind::HomeAssistant => "home_assistant",
        ConnectorKind::Signal => "signal",
        ConnectorKind::Gmail => "gmail",
    }
}

fn ensure_connector_enabled(
    enabled: bool,
    kind: &str,
    connector_id: &str,
    action: &str,
) -> Result<(), ApiError> {
    if enabled {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("{kind} connector '{connector_id}' is disabled for {action}"),
        ))
    }
}

async fn refresh_connector_config(state: &AppState) -> Result<(), ApiError> {
    let latest = state.storage.load_config()?;
    let mut config = state.config.write().await;
    *config = latest;
    Ok(())
}

fn truncate_for_title(value: &str, max_chars: usize) -> String {
    let compact = value
        .lines()
        .next()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| value.split_whitespace().collect::<Vec<_>>().join(" "));
    let compact = compact.trim();
    if compact.is_empty() {
        return "message".to_string();
    }
    let total_chars = compact.chars().count();
    if total_chars <= max_chars {
        compact.to_string()
    } else {
        let truncated = compact
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}.")
    }
}
