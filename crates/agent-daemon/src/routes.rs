use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{Duration, Utc};

use agent_core::{BatchTaskRequest, BatchTaskResponse, RunTaskRequest, RunTaskResponse};

use crate::{
    add_mission, approve_connector_approval, approve_memory, autonomy_status, autopilot_status,
    get_provider_browser_auth_status,
    call_home_assistant_service_route, cancel_mission, clear_provider_credentials,
    dashboard_bootstrap,
    dashboard::{dashboard_css, dashboard_index, dashboard_js, dashboard_root},
    delegation_status, delete_alias, delete_app_connector, delete_brave_connector, delete_discord_connector,
    delete_gmail_connector, delete_home_assistant_connector, delete_inbox_connector,
    delete_mcp_server, delete_provider, delete_signal_connector, delete_slack_connector,
    delete_telegram_connector, delete_webhook_connector, doctor, enable_autonomy, evolve_status,
    forget_memory, get_brave_connector, get_discord_connector, get_gmail_connector,
    get_home_assistant_connector, get_home_assistant_entity_state_route, get_inbox_connector, get_mission,
    get_permission_preset, get_session, get_signal_connector, get_skill_draft, get_slack_connector,
    get_telegram_connector, get_trust, get_webhook_connector, list_aliases, list_app_connectors,
    list_brave_connectors, list_connector_approvals, list_delegation_targets, list_discord_connectors,
    list_enabled_skills, list_events, list_gmail_connectors, list_home_assistant_connectors,
    list_inbox_connectors, list_logs, list_mcp_servers, list_memories, list_memory_review_queue,
    list_mission_checkpoints, list_missions, list_profile_memories, list_provider_models,
    list_providers, list_sessions, list_signal_connectors, list_skill_drafts,
    list_slack_connectors, list_telegram_connectors, list_webhook_connectors, pause_autonomy,
    pause_evolve_mode, pause_mission, poll_discord_connector_route, poll_gmail_connector_route,
    poll_home_assistant_connector_route, poll_inbox_connector_route, poll_signal_connector_route,
    poll_slack_connector_route, poll_telegram_connector_route, publish_skill_draft,
    provider_browser_auth_callback, provider_browser_auth_complete,
    receive_webhook_event, reject_connector_approval, reject_memory, reject_skill_draft,
    rename_session, resolve_alias_and_provider, resume_autonomy, resume_evolve_mode,
    resume_mission, search_memory, send_discord_message_route, send_gmail_message_route,
    send_signal_message_route, send_slack_message_route, send_telegram_message_route, shutdown,
    start_evolve_mode, start_provider_browser_auth, status, stop_evolve_mode,
    suggest_provider_defaults,
    update_autopilot, update_daemon_config, update_delegation_config, update_enabled_skills,
    update_permission_preset, update_trust, upsert_alias,
    upsert_app_connector, upsert_brave_connector, upsert_discord_connector, upsert_gmail_connector,
    upsert_home_assistant_connector, upsert_inbox_connector, upsert_mcp_server, upsert_memory,
    upsert_provider, upsert_signal_connector, upsert_slack_connector, upsert_telegram_connector,
    upsert_webhook_connector, ApiError, AppState,
};
use crate::{
    execute_batch_request, execute_task_request, DelegationExecutionOptions, TaskRequestInput,
};

const DASHBOARD_SESSION_COOKIE_NAME: &str = "agent_dashboard_session";
const DASHBOARD_SESSION_TTL_SECS: i64 = 12 * 60 * 60;

#[derive(serde::Deserialize)]
struct DashboardSessionRequest {
    token: String,
}

pub(crate) fn build_protected_routes(state: AppState) -> Router {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/dashboard/bootstrap", get(dashboard_bootstrap))
        .route("/v1/shutdown", post(shutdown))
        .route("/v1/providers", get(list_providers).post(upsert_provider))
        .route("/v1/providers/suggest", post(suggest_provider_defaults))
        .route("/v1/providers/{provider_id}", delete(delete_provider))
        .route(
            "/v1/providers/{provider_id}/credentials",
            delete(clear_provider_credentials),
        )
        .route("/v1/provider-auth/start", post(start_provider_browser_auth))
        .route(
            "/v1/provider-auth/{session_id}",
            get(get_provider_browser_auth_status),
        )
        .route(
            "/v1/providers/{provider_id}/models",
            get(list_provider_models),
        )
        .route("/v1/aliases", get(list_aliases).post(upsert_alias))
        .route("/v1/aliases/{alias_name}", delete(delete_alias))
        .route("/v1/trust", get(get_trust).put(update_trust))
        .route(
            "/v1/permissions",
            get(get_permission_preset).put(update_permission_preset),
        )
        .route("/v1/autonomy/status", get(autonomy_status))
        .route("/v1/autonomy/enable", post(enable_autonomy))
        .route("/v1/autonomy/pause", post(pause_autonomy))
        .route("/v1/autonomy/resume", post(resume_autonomy))
        .route("/v1/evolve/status", get(evolve_status))
        .route("/v1/evolve/start", post(start_evolve_mode))
        .route("/v1/evolve/pause", post(pause_evolve_mode))
        .route("/v1/evolve/resume", post(resume_evolve_mode))
        .route("/v1/evolve/stop", post(stop_evolve_mode))
        .route(
            "/v1/autopilot/status",
            get(autopilot_status).put(update_autopilot),
        )
        .route("/v1/daemon/config", put(update_daemon_config))
        .route(
            "/v1/delegation/config",
            get(delegation_status).put(update_delegation_config),
        )
        .route("/v1/delegation/targets", get(list_delegation_targets))
        .route("/v1/mcp", get(list_mcp_servers).post(upsert_mcp_server))
        .route("/v1/mcp/{server_id}", delete(delete_mcp_server))
        .route(
            "/v1/apps",
            get(list_app_connectors).post(upsert_app_connector),
        )
        .route("/v1/apps/{connector_id}", delete(delete_app_connector))
        .route(
            "/v1/webhooks",
            get(list_webhook_connectors).post(upsert_webhook_connector),
        )
        .route(
            "/v1/webhooks/{connector_id}",
            get(get_webhook_connector).delete(delete_webhook_connector),
        )
        .route(
            "/v1/inboxes",
            get(list_inbox_connectors).post(upsert_inbox_connector),
        )
        .route(
            "/v1/inboxes/{connector_id}",
            get(get_inbox_connector).delete(delete_inbox_connector),
        )
        .route(
            "/v1/inboxes/{connector_id}/poll",
            post(poll_inbox_connector_route),
        )
        .route(
            "/v1/telegram",
            get(list_telegram_connectors).post(upsert_telegram_connector),
        )
        .route(
            "/v1/telegram/{connector_id}",
            get(get_telegram_connector).delete(delete_telegram_connector),
        )
        .route(
            "/v1/telegram/{connector_id}/poll",
            post(poll_telegram_connector_route),
        )
        .route(
            "/v1/telegram/{connector_id}/send",
            post(send_telegram_message_route),
        )
        .route(
            "/v1/discord",
            get(list_discord_connectors).post(upsert_discord_connector),
        )
        .route(
            "/v1/discord/{connector_id}",
            get(get_discord_connector).delete(delete_discord_connector),
        )
        .route(
            "/v1/discord/{connector_id}/poll",
            post(poll_discord_connector_route),
        )
        .route(
            "/v1/discord/{connector_id}/send",
            post(send_discord_message_route),
        )
        .route(
            "/v1/slack",
            get(list_slack_connectors).post(upsert_slack_connector),
        )
        .route(
            "/v1/slack/{connector_id}",
            get(get_slack_connector).delete(delete_slack_connector),
        )
        .route(
            "/v1/slack/{connector_id}/poll",
            post(poll_slack_connector_route),
        )
        .route(
            "/v1/slack/{connector_id}/send",
            post(send_slack_message_route),
        )
        .route(
            "/v1/home-assistant",
            get(list_home_assistant_connectors).post(upsert_home_assistant_connector),
        )
        .route(
            "/v1/home-assistant/{connector_id}",
            get(get_home_assistant_connector).delete(delete_home_assistant_connector),
        )
        .route(
            "/v1/home-assistant/{connector_id}/poll",
            post(poll_home_assistant_connector_route),
        )
        .route(
            "/v1/home-assistant/{connector_id}/entities/{entity_id}",
            get(get_home_assistant_entity_state_route),
        )
        .route(
            "/v1/home-assistant/{connector_id}/services",
            post(call_home_assistant_service_route),
        )
        .route(
            "/v1/signal",
            get(list_signal_connectors).post(upsert_signal_connector),
        )
        .route(
            "/v1/signal/{connector_id}",
            get(get_signal_connector).delete(delete_signal_connector),
        )
        .route(
            "/v1/signal/{connector_id}/poll",
            post(poll_signal_connector_route),
        )
        .route(
            "/v1/signal/{connector_id}/send",
            post(send_signal_message_route),
        )
        .route(
            "/v1/gmail",
            get(list_gmail_connectors).post(upsert_gmail_connector),
        )
        .route(
            "/v1/gmail/{connector_id}",
            get(get_gmail_connector).delete(delete_gmail_connector),
        )
        .route(
            "/v1/gmail/{connector_id}/poll",
            post(poll_gmail_connector_route),
        )
        .route(
            "/v1/gmail/{connector_id}/send",
            post(send_gmail_message_route),
        )
        .route(
            "/v1/brave",
            get(list_brave_connectors).post(upsert_brave_connector),
        )
        .route(
            "/v1/brave/{connector_id}",
            get(get_brave_connector).delete(delete_brave_connector),
        )
        .route("/v1/connector-approvals", get(list_connector_approvals))
        .route(
            "/v1/connector-approvals/{approval_id}/approve",
            post(approve_connector_approval),
        )
        .route(
            "/v1/connector-approvals/{approval_id}/reject",
            post(reject_connector_approval),
        )
        .route(
            "/v1/skills",
            get(list_enabled_skills).put(update_enabled_skills),
        )
        .route("/v1/skills/drafts", get(list_skill_drafts))
        .route("/v1/skills/drafts/{draft_id}", get(get_skill_draft))
        .route(
            "/v1/skills/drafts/{draft_id}/publish",
            post(publish_skill_draft),
        )
        .route(
            "/v1/skills/drafts/{draft_id}/reject",
            post(reject_skill_draft),
        )
        .route("/v1/missions", get(list_missions).post(add_mission))
        .route("/v1/missions/{mission_id}", get(get_mission))
        .route("/v1/missions/{mission_id}/pause", post(pause_mission))
        .route("/v1/missions/{mission_id}/resume", post(resume_mission))
        .route("/v1/missions/{mission_id}/cancel", post(cancel_mission))
        .route(
            "/v1/missions/{mission_id}/checkpoints",
            get(list_mission_checkpoints),
        )
        .route("/v1/memory", get(list_memories).post(upsert_memory))
        .route("/v1/memory/profile", get(list_profile_memories))
        .route("/v1/memory/review", get(list_memory_review_queue))
        .route("/v1/memory/search", post(search_memory))
        .route("/v1/memory/{memory_id}/approve", post(approve_memory))
        .route("/v1/memory/{memory_id}/reject", post(reject_memory))
        .route("/v1/memory/{memory_id}", delete(forget_memory))
        .route("/v1/logs", get(list_logs))
        .route("/v1/events", get(list_events))
        .route("/v1/doctor", get(doctor))
        .route("/v1/run", post(run_task))
        .route("/v1/batch", post(run_batch))
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/{session_id}", get(get_session))
        .route("/v1/sessions/{session_id}/title", put(rename_session))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ))
        .with_state(state)
}

pub(crate) fn build_public_routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(dashboard_root))
        .route("/ui", get(dashboard_index))
        .route("/dashboard", get(dashboard_index))
        .route("/dashboard.css", get(dashboard_css))
        .route("/dashboard.js", get(dashboard_js))
        .route(
            "/auth/dashboard/session",
            post(create_dashboard_session).delete(clear_dashboard_session),
        )
        .route("/auth/provider/callback", get(provider_browser_auth_callback))
        .route("/auth/provider/complete", get(provider_browser_auth_complete))
        .route("/v1/hooks/{connector_id}", post(receive_webhook_event))
        .with_state(state)
}

async fn create_dashboard_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DashboardSessionRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let expected = {
        let config = state.config.read().await;
        config.daemon.token.clone()
    };
    if payload.token.trim() != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let session_id = uuid::Uuid::new_v4().to_string();
    let mut sessions = state.dashboard_sessions.write().await;
    prune_dashboard_sessions(&mut sessions);
    sessions.insert(session_id.clone(), Utc::now());
    let cookie = dashboard_session_cookie(&session_id, request_is_https(&headers));
    Ok((
        StatusCode::NO_CONTENT,
        [
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, "no-store, max-age=0".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
    ))
}

async fn clear_dashboard_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(session_id) = dashboard_session_cookie_id(&headers) {
        let mut sessions = state.dashboard_sessions.write().await;
        sessions.remove(&session_id);
    }
    (
        StatusCode::NO_CONTENT,
        [
            (
                header::SET_COOKIE,
                clear_dashboard_session_cookie(request_is_https(&headers)),
            ),
            (header::CACHE_CONTROL, "no-store, max-age=0".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
    )
}

async fn require_bearer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let expected = {
        let config = state.config.read().await;
        format!("Bearer {}", config.daemon.token)
    };

    let header_matches = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);

    if !header_matches && !authorize_dashboard_session(&state, request.headers()).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}

async fn authorize_dashboard_session(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(session_id) = dashboard_session_cookie_id(headers) else {
        return false;
    };
    let mut sessions = state.dashboard_sessions.write().await;
    prune_dashboard_sessions(&mut sessions);
    let Some(last_seen) = sessions.get_mut(&session_id) else {
        return false;
    };
    *last_seen = Utc::now();
    true
}

fn prune_dashboard_sessions(
    sessions: &mut std::collections::HashMap<String, chrono::DateTime<Utc>>,
) {
    let now = Utc::now();
    sessions.retain(|_, last_seen| now - *last_seen <= Duration::seconds(DASHBOARD_SESSION_TTL_SECS));
}

fn dashboard_session_cookie(session_id: &str, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{DASHBOARD_SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Strict{secure_attr}"
    )
}

fn clear_dashboard_session_cookie(secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{DASHBOARD_SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT{secure_attr}"
    )
}

fn dashboard_session_cookie_id(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|segment| {
        let mut parts = segment.trim().splitn(2, '=');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();
        if name == DASHBOARD_SESSION_COOKIE_NAME && !value.is_empty() {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn request_is_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
        || headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .is_some_and(|value| value.starts_with("https://"))
}

async fn run_task(
    State(state): State<AppState>,
    Json(payload): Json<RunTaskRequest>,
) -> Result<Json<RunTaskResponse>, ApiError> {
    let (alias, provider) = resolve_alias_and_provider(&state, payload.alias.as_deref()).await?;
    let response = execute_task_request(
        &state,
        &alias,
        &provider,
        TaskRequestInput {
            prompt: payload.prompt,
            requested_model: payload.requested_model,
            session_id: payload.session_id,
            cwd: payload.cwd,
            thinking_level: payload.thinking_level,
            attachments: payload.attachments,
            permission_preset: payload.permission_preset,
            output_schema_json: payload.output_schema_json,
            persist: !payload.ephemeral,
            background: false,
            delegation_depth: 0,
        },
    )
    .await?;
    Ok(Json(response))
}

async fn run_batch(
    State(state): State<AppState>,
    Json(payload): Json<BatchTaskRequest>,
) -> Result<Json<BatchTaskResponse>, ApiError> {
    let response = execute_batch_request(
        &state,
        payload,
        DelegationExecutionOptions {
            background: false,
            permission_preset: None,
            delegation_depth: 0,
        },
    )
    .await?;
    crate::append_log(
        &state,
        "info",
        "batch",
        format!("executed {} subagent tasks", response.results.len()),
    )?;
    Ok(Json(response))
}
