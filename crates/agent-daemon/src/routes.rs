use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{Duration, Utc};
use futures::stream;
use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use agent_core::{
    BatchTaskRequest, BatchTaskResponse, DashboardLaunchResponse, RunTaskRequest, RunTaskResponse,
    RunTaskStreamEvent,
};

use crate::{
    add_mission, approve_connector_approval, approve_memory, autonomy_status, autopilot_status,
    call_home_assistant_service_route, cancel_mission, clear_provider_credentials, compact_session,
    control_socket::control_socket_route,
    create_support_bundle,
    dashboard::{add_dashboard_asset_routes, dashboard_index, dashboard_root},
    dashboard_bootstrap, delegation_status, delete_alias, delete_app_connector,
    delete_brave_connector, delete_discord_connector, delete_gmail_connector,
    delete_home_assistant_connector, delete_inbox_connector, delete_mcp_server, delete_plugin,
    delete_provider, delete_signal_connector, delete_slack_connector, delete_telegram_connector,
    delete_webhook_connector, doctor, enable_autonomy, evolve_status, export_config, forget_memory,
    fork_session, get_brave_connector, get_discord_connector, get_gmail_connector,
    get_home_assistant_connector, get_home_assistant_entity_state_route, get_inbox_connector,
    get_mission, get_permission_preset, get_plugin, get_plugin_doctor_report,
    get_provider_browser_auth_status, get_session, get_session_resume_packet, get_signal_connector,
    get_skill_draft, get_slack_connector, get_telegram_connector, get_trust, get_webhook_connector,
    import_config, inspect_workspace_route, install_plugin, list_aliases, list_app_connectors,
    list_brave_connectors, list_connector_approvals, list_delegation_targets,
    list_discord_connectors, list_enabled_skills, list_events, list_gmail_connectors,
    list_home_assistant_connectors, list_inbox_connectors, list_logs, list_mcp_servers,
    list_memories, list_memory_review_queue, list_mission_checkpoints, list_missions,
    list_plugin_doctor_reports, list_plugins, list_profile_memories,
    list_provider_model_descriptors, list_provider_models, list_providers, list_sessions,
    list_signal_connectors, list_skill_drafts, list_slack_connectors, list_telegram_connectors,
    list_webhook_connectors, pause_autonomy, pause_evolve_mode, pause_mission,
    poll_discord_connector_route, poll_gmail_connector_route, poll_home_assistant_connector_route,
    poll_inbox_connector_route, poll_signal_connector_route, poll_slack_connector_route,
    poll_telegram_connector_route, provider_browser_auth_callback, provider_browser_auth_complete,
    publish_skill_draft, rebuild_memory, receive_webhook_event, reject_connector_approval,
    reject_memory, reject_skill_draft, rename_session, reset_onboarding,
    resolve_alias_and_provider, resume_autonomy, resume_evolve_mode, resume_mission, run_update,
    search_memory, send_discord_message_route, send_gmail_message_route, send_signal_message_route,
    send_slack_message_route, send_telegram_message_route, shutdown, start_evolve_mode,
    start_provider_browser_auth, status, stop_evolve_mode, suggest_provider_defaults,
    update_autopilot, update_daemon_config, update_delegation_config, update_enabled_skills,
    update_main_alias, update_permission_preset, update_plugin, update_plugin_state, update_status,
    update_trust, upsert_alias, upsert_app_connector, upsert_brave_connector,
    upsert_discord_connector, upsert_gmail_connector, upsert_home_assistant_connector,
    upsert_inbox_connector, upsert_mcp_server, upsert_memory, upsert_provider,
    upsert_signal_connector, upsert_slack_connector, upsert_telegram_connector,
    upsert_webhook_connector, workspace_diff_route, workspace_init_agents_route,
    workspace_shell_route, ApiError, AppState,
};
use crate::{
    execute_batch_request, execute_task_request, execute_task_request_with_events,
    DelegationExecutionOptions, TaskRequestInput,
};

const DASHBOARD_SESSION_COOKIE_NAME: &str = "agent_dashboard_session";
const DASHBOARD_SESSION_TTL_SECS: i64 = 12 * 60 * 60;
const DASHBOARD_LAUNCH_TTL_SECS: i64 = 5 * 60;

#[derive(serde::Deserialize)]
struct DashboardSessionRequest {
    token: String,
}

fn add_system_api_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/v1/status", get(status))
        .route("/v1/dashboard/bootstrap", get(dashboard_bootstrap))
        .route("/v1/workspace/inspect", post(inspect_workspace_route))
        .route("/v1/workspace/diff", post(workspace_diff_route))
        .route("/v1/workspace/init", post(workspace_init_agents_route))
        .route("/v1/workspace/shell", post(workspace_shell_route))
        .route("/v1/onboarding/reset", post(reset_onboarding))
        .route("/v1/shutdown", post(shutdown))
        .route("/v1/config", get(export_config).put(import_config))
        .route("/v1/logs", get(list_logs))
        .route("/v1/events", get(list_events))
        .route("/v1/ws", get(control_socket_route))
        .route("/v1/doctor", get(doctor))
        .route("/v1/update/status", get(update_status))
        .route("/v1/update/run", post(run_update))
        .route("/v1/support-bundle", post(create_support_bundle))
        .route("/v1/dashboard/launch", post(create_dashboard_launch))
}

fn add_provider_api_routes(router: Router<AppState>) -> Router<AppState> {
    router
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
        .route(
            "/v1/providers/{provider_id}/model-descriptors",
            get(list_provider_model_descriptors),
        )
        .route("/v1/aliases", get(list_aliases).post(upsert_alias))
        .route("/v1/main-alias", put(update_main_alias))
        .route("/v1/aliases/{alias_name}", delete(delete_alias))
        .route("/v1/trust", get(get_trust).put(update_trust))
        .route(
            "/v1/permissions",
            get(get_permission_preset).put(update_permission_preset),
        )
}

fn add_autonomy_api_routes(router: Router<AppState>) -> Router<AppState> {
    router
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
        .route("/v1/missions", get(list_missions).post(add_mission))
        .route("/v1/missions/{mission_id}", get(get_mission))
        .route("/v1/missions/{mission_id}/pause", post(pause_mission))
        .route("/v1/missions/{mission_id}/resume", post(resume_mission))
        .route("/v1/missions/{mission_id}/cancel", post(cancel_mission))
        .route(
            "/v1/missions/{mission_id}/checkpoints",
            get(list_mission_checkpoints),
        )
}

fn add_integration_api_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/v1/mcp", get(list_mcp_servers).post(upsert_mcp_server))
        .route("/v1/mcp/{server_id}", delete(delete_mcp_server))
        .route("/v1/plugins", get(list_plugins))
        .route("/v1/plugins/install", post(install_plugin))
        .route("/v1/plugins/doctor", get(list_plugin_doctor_reports))
        .route(
            "/v1/plugins/{plugin_id}",
            get(get_plugin)
                .put(update_plugin_state)
                .delete(delete_plugin),
        )
        .route("/v1/plugins/{plugin_id}/update", post(update_plugin))
        .route(
            "/v1/plugins/{plugin_id}/doctor",
            get(get_plugin_doctor_report),
        )
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
}

fn add_memory_and_session_api_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/v1/memory", get(list_memories).post(upsert_memory))
        .route("/v1/memory/profile", get(list_profile_memories))
        .route("/v1/memory/review", get(list_memory_review_queue))
        .route("/v1/memory/search", post(search_memory))
        .route("/v1/memory/rebuild", post(rebuild_memory))
        .route("/v1/memory/{memory_id}/approve", post(approve_memory))
        .route("/v1/memory/{memory_id}/reject", post(reject_memory))
        .route("/v1/memory/{memory_id}", delete(forget_memory))
        .route("/v1/run", post(run_task))
        .route("/v1/run/stream", post(run_task_stream))
        .route("/v1/batch", post(run_batch))
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/{session_id}", get(get_session))
        .route(
            "/v1/sessions/{session_id}/resume-packet",
            get(get_session_resume_packet),
        )
        .route("/v1/sessions/{session_id}/title", put(rename_session))
        .route("/v1/sessions/{session_id}/fork", post(fork_session))
        .route("/v1/sessions/{session_id}/compact", post(compact_session))
}

pub(crate) fn build_protected_routes(state: AppState) -> Router {
    add_memory_and_session_api_routes(add_integration_api_routes(add_autonomy_api_routes(
        add_provider_api_routes(add_system_api_routes(Router::new())),
    )))
    .layer(middleware::from_fn_with_state(
        state.clone(),
        require_bearer,
    ))
    .with_state(state)
}

fn add_dashboard_ui_routes(router: Router<AppState>) -> Router<AppState> {
    add_dashboard_asset_routes(
        router
            .route("/", get(dashboard_root))
            .route("/ui", get(dashboard_index))
            .route("/dashboard", get(dashboard_index)),
    )
}

fn add_public_auth_routes(router: Router<AppState>) -> Router<AppState> {
    router
        .route(
            "/auth/dashboard/session",
            post(create_dashboard_session).delete(clear_dashboard_session),
        )
        .route(
            "/auth/dashboard/launch/{launch_id}",
            get(consume_dashboard_launch),
        )
        .route(
            "/auth/provider/callback",
            get(provider_browser_auth_callback),
        )
        .route(
            "/auth/provider/complete",
            get(provider_browser_auth_complete),
        )
}

pub(crate) fn build_public_routes(state: AppState) -> Router {
    add_public_auth_routes(add_dashboard_ui_routes(
        Router::new().route("/v1/hooks/{connector_id}", post(receive_webhook_event)),
    ))
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

async fn create_dashboard_launch(
    State(state): State<AppState>,
) -> Result<Json<DashboardLaunchResponse>, ApiError> {
    let launch_id = uuid::Uuid::new_v4().to_string();
    let mut launches = state.dashboard_launches.write().await;
    prune_dashboard_launches(&mut launches);
    launches.insert(launch_id.clone(), Utc::now());
    Ok(Json(DashboardLaunchResponse {
        launch_path: format!("/auth/dashboard/launch/{launch_id}"),
    }))
}

async fn consume_dashboard_launch(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(launch_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut launches = state.dashboard_launches.write().await;
    prune_dashboard_launches(&mut launches);
    if launches.remove(&launch_id).is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let mut sessions = state.dashboard_sessions.write().await;
    prune_dashboard_sessions(&mut sessions);
    sessions.insert(session_id.clone(), Utc::now());
    let cookie = dashboard_session_cookie(&session_id, request_is_https(&headers));

    Ok((
        [
            (header::SET_COOKIE, cookie),
            (header::CACHE_CONTROL, "no-store, max-age=0".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        Redirect::to("/ui"),
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
    sessions
        .retain(|_, last_seen| now - *last_seen <= Duration::seconds(DASHBOARD_SESSION_TTL_SECS));
}

fn prune_dashboard_launches(
    launches: &mut std::collections::HashMap<String, chrono::DateTime<Utc>>,
) {
    let now = Utc::now();
    launches
        .retain(|_, created_at| now - *created_at <= Duration::seconds(DASHBOARD_LAUNCH_TTL_SECS));
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
            task_mode: payload.task_mode,
            output_schema_json: payload.output_schema_json,
            remote_content_policy_override: payload.remote_content_policy_override,
            persist: !payload.ephemeral,
            background: false,
            delegation_depth: 0,
            cancellation: None,
        },
    )
    .await?;
    Ok(Json(response))
}

async fn run_task_stream(
    State(state): State<AppState>,
    Json(payload): Json<RunTaskRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (alias, provider) = resolve_alias_and_provider(&state, payload.alias.as_deref()).await?;
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(32);
    let state = state.clone();
    let alias = alias.clone();
    let provider = provider.clone();

    tokio::spawn(async move {
        let mut emit = best_effort_stream_emitter(tx);
        if let Err(error) = execute_task_request_with_events(
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
                task_mode: payload.task_mode,
                output_schema_json: payload.output_schema_json,
                remote_content_policy_override: payload.remote_content_policy_override,
                persist: !payload.ephemeral,
                background: false,
                delegation_depth: 0,
                cancellation: None,
            },
            &mut emit,
        )
        .await
        {
            let _ = emit(RunTaskStreamEvent::Error {
                message: error.message,
            })
            .await;
        }
    });

    let response_stream = stream::unfold(rx, |mut rx| async {
        rx.recv()
            .await
            .map(|chunk| (Ok::<Bytes, Infallible>(chunk), rx))
    });

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson".to_string()),
            (header::CACHE_CONTROL, "no-store, max-age=0".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        Body::from_stream(response_stream),
    ))
}

fn best_effort_stream_emitter(
    tx: tokio::sync::mpsc::Sender<Bytes>,
) -> impl FnMut(RunTaskStreamEvent) -> Pin<Box<dyn Future<Output = bool> + Send>> {
    let connected = Arc::new(AtomicBool::new(true));
    move |event| {
        let tx = tx.clone();
        let connected = Arc::clone(&connected);
        Box::pin(async move {
            if !connected.load(Ordering::Relaxed) {
                return true;
            }

            let delivered = match serde_json::to_string(&event) {
                Ok(json) => tx.send(Bytes::from(format!("{json}\n"))).await.is_ok(),
                Err(_) => true,
            };
            if !delivered {
                connected.store(false, Ordering::Relaxed);
            }
            true
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, AppState, ProviderRateLimiter,
    };
    use agent_core::AppConfig;
    use agent_storage::Storage;
    use axum::response::IntoResponse;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    fn test_state() -> AppState {
        AppState {
            storage: Storage::open_at(
                std::env::temp_dir().join(format!("agent-daemon-routes-test-{}", Uuid::new_v4())),
            )
            .unwrap(),
            config: Arc::new(RwLock::new(AppConfig::default())),
            http_client: reqwest::Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: Utc::now(),
            shutdown: mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[tokio::test]
    async fn create_dashboard_session_accepts_token_and_sets_cookie() {
        let state = test_state();
        {
            let mut config = state.config.write().await;
            config.daemon.token = "dashboard-secret".to_string();
        }

        let response = create_dashboard_session(
            State(state.clone()),
            HeaderMap::new(),
            Json(DashboardSessionRequest {
                token: "dashboard-secret".to_string(),
            }),
        )
        .await
        .unwrap()
        .into_response();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.contains("agent_dashboard_session="));
        assert!(cookie.contains("HttpOnly"));
        assert_eq!(state.dashboard_sessions.read().await.len(), 1);
    }

    #[tokio::test]
    async fn create_dashboard_launch_and_consume_are_single_use() {
        let state = test_state();

        let Json(launch) = create_dashboard_launch(State(state.clone())).await.unwrap();
        assert!(launch.launch_path.starts_with("/auth/dashboard/launch/"));
        assert_eq!(state.dashboard_launches.read().await.len(), 1);

        let launch_id = launch.launch_path.rsplit('/').next().unwrap().to_string();
        let first = consume_dashboard_launch(
            State(state.clone()),
            HeaderMap::new(),
            axum::extract::Path(launch_id.clone()),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(first.status(), StatusCode::SEE_OTHER);
        assert_eq!(first.headers().get(header::LOCATION).unwrap(), "/ui");
        assert!(first.headers().contains_key(header::SET_COOKIE));

        let second = consume_dashboard_launch(
            State(state),
            HeaderMap::new(),
            axum::extract::Path(launch_id),
        )
        .await;
        match second {
            Err(status) => assert_eq!(status, StatusCode::UNAUTHORIZED),
            Ok(_) => panic!("expected second launch consumption to fail"),
        }
    }

    #[tokio::test]
    async fn onboarding_reset_replaces_config_and_returns_new_token() {
        let state = test_state();
        let old_token = {
            let mut config = state.config.write().await;
            config.daemon.token = "old-dashboard-token".to_string();
            config.onboarding_complete = true;
            state.storage.save_config(&config).unwrap();
            config.daemon.token.clone()
        };

        let Json(response) = reset_onboarding(
            State(state.clone()),
            Json(crate::control::OnboardingResetRequest { confirmed: true }),
        )
        .await
        .unwrap();

        let reloaded = state.storage.load_config().unwrap();
        let active = state.config.read().await.clone();
        assert_eq!(active, reloaded);
        assert_ne!(response.daemon_token, old_token);
        assert_eq!(response.daemon_token, active.daemon.token);
        assert!(!active.onboarding_complete);
        assert_eq!(response.removed_credentials, 0);
        assert!(response.credential_warnings.is_empty());
    }

    #[tokio::test]
    async fn best_effort_stream_emitter_ignores_disconnects() {
        let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);
        drop(rx);

        let mut emit = best_effort_stream_emitter(tx);
        assert!(
            emit(RunTaskStreamEvent::Error {
                message: "first".to_string(),
            })
            .await
        );
        assert!(
            emit(RunTaskStreamEvent::Error {
                message: "second".to_string(),
            })
            .await
        );
    }
}
