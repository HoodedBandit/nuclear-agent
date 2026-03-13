use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};

use agent_core::{BatchTaskRequest, BatchTaskResponse, RunTaskRequest, RunTaskResponse};

use crate::{
    add_mission, approve_connector_approval, approve_memory, autonomy_status, autopilot_status,
    call_home_assistant_service_route, cancel_mission, clear_provider_credentials,
    dashboard::{dashboard_css, dashboard_index, dashboard_js, dashboard_root},
    delegation_status, delete_app_connector, delete_discord_connector,
    delete_home_assistant_connector, delete_inbox_connector, delete_mcp_server,
    delete_signal_connector, delete_slack_connector, delete_telegram_connector,
    delete_webhook_connector, doctor, enable_autonomy, evolve_status, forget_memory,
    get_discord_connector, get_home_assistant_connector, get_home_assistant_entity_state_route,
    get_inbox_connector, get_mission, get_permission_preset, get_session, get_signal_connector,
    get_skill_draft, get_slack_connector, get_telegram_connector, get_webhook_connector,
    list_aliases, list_app_connectors, list_connector_approvals, list_delegation_targets,
    list_discord_connectors, list_enabled_skills, list_events, list_home_assistant_connectors,
    list_inbox_connectors, list_logs, list_mcp_servers, list_memories, list_memory_review_queue,
    list_mission_checkpoints, list_missions, list_profile_memories, list_provider_models,
    list_providers, list_sessions, list_signal_connectors, list_skill_drafts,
    list_slack_connectors, list_telegram_connectors, list_webhook_connectors, pause_autonomy,
    pause_evolve_mode, pause_mission, poll_discord_connector_route,
    poll_home_assistant_connector_route, poll_inbox_connector_route, poll_signal_connector_route,
    poll_slack_connector_route, poll_telegram_connector_route, publish_skill_draft,
    receive_webhook_event, reject_connector_approval, reject_memory, reject_skill_draft,
    resolve_alias_and_provider, resume_autonomy, resume_evolve_mode, resume_mission, search_memory,
    send_discord_message_route, send_signal_message_route, send_slack_message_route,
    send_telegram_message_route, shutdown, start_evolve_mode, status, stop_evolve_mode,
    update_autopilot, update_daemon_config, update_delegation_config, update_enabled_skills,
    update_permission_preset, update_trust, upsert_alias, upsert_app_connector,
    upsert_discord_connector, upsert_home_assistant_connector, upsert_inbox_connector,
    upsert_mcp_server, upsert_memory, upsert_provider, upsert_signal_connector,
    upsert_slack_connector, upsert_telegram_connector, upsert_webhook_connector, ApiError,
    AppState,
};
use crate::{
    execute_batch_request, execute_task_request, DelegationExecutionOptions, TaskRequestInput,
};

pub(crate) fn build_protected_routes(state: AppState) -> Router {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/shutdown", post(shutdown))
        .route("/v1/providers", get(list_providers).post(upsert_provider))
        .route(
            "/v1/providers/{provider_id}/credentials",
            delete(clear_provider_credentials),
        )
        .route(
            "/v1/providers/{provider_id}/models",
            get(list_provider_models),
        )
        .route("/v1/aliases", get(list_aliases).post(upsert_alias))
        .route("/v1/trust", put(update_trust))
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
        .route("/v1/hooks/{connector_id}", post(receive_webhook_event))
        .with_state(state)
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

    let header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if header != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
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
