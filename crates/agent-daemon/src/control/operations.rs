use std::time::Duration;

use agent_core::{
    AutonomyEnableRequest, AutonomyMode, AutonomyState, AutopilotConfig, AutopilotState,
    AutopilotUpdateRequest, HealthReport, LogEntry, ProviderCapabilitySummary,
};
use agent_policy::autonomy_warning;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::Deserialize;
use tokio::time::timeout;

use crate::{append_log, collect_plugin_doctor_reports, ApiError, AppState, LimitQuery};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventCursor {
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct EventQuery {
    pub(crate) after: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) wait_seconds: Option<u64>,
}

pub(crate) async fn autonomy_status(
    State(state): State<AppState>,
) -> Result<Json<agent_core::AutonomyProfile>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.autonomy.clone()))
}

pub(crate) async fn enable_autonomy(
    State(state): State<AppState>,
    Json(payload): Json<AutonomyEnableRequest>,
) -> Result<Json<agent_core::AutonomyProfile>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        let mode = payload.mode.unwrap_or(AutonomyMode::FreeThinking);
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = mode.clone();
        config.autonomy.unlimited_usage = true;
        config.autonomy.full_network = true;
        config.autonomy.allow_self_edit =
            payload.allow_self_edit || !matches!(mode, AutonomyMode::Assisted);
        config.autonomy.consented_at = Some(Utc::now());
        state.storage.save_config(&config)?;
        config.autonomy.clone()
    };

    append_log(
        &state,
        "warn",
        "autonomy",
        format!("autonomy enabled: {}", autonomy_warning()),
    )?;
    state.autopilot_wake.notify_waiters();
    Ok(Json(updated))
}

pub(crate) async fn pause_autonomy(
    State(state): State<AppState>,
) -> Result<Json<agent_core::AutonomyProfile>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.autonomy.state = AutonomyState::Paused;
        state.storage.save_config(&config)?;
        config.autonomy.clone()
    };
    append_log(&state, "warn", "autonomy", "autonomy paused")?;
    state.autopilot_wake.notify_waiters();
    Ok(Json(updated))
}

pub(crate) async fn resume_autonomy(
    State(state): State<AppState>,
) -> Result<Json<agent_core::AutonomyProfile>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.autonomy.state = AutonomyState::Enabled;
        state.storage.save_config(&config)?;
        config.autonomy.clone()
    };
    append_log(&state, "warn", "autonomy", "autonomy resumed")?;
    state.autopilot_wake.notify_waiters();
    Ok(Json(updated))
}

pub(crate) async fn autopilot_status(
    State(state): State<AppState>,
) -> Result<Json<AutopilotConfig>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.autopilot.clone()))
}

pub(crate) async fn update_autopilot(
    State(state): State<AppState>,
    Json(payload): Json<AutopilotUpdateRequest>,
) -> Result<Json<AutopilotConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if let Some(state_value) = payload.state {
            config.autopilot.state = state_value;
        }
        if let Some(max_concurrent_missions) = payload.max_concurrent_missions {
            config.autopilot.max_concurrent_missions = max_concurrent_missions.max(1);
        }
        if let Some(wake_interval_seconds) = payload.wake_interval_seconds {
            config.autopilot.wake_interval_seconds = wake_interval_seconds.max(5);
        }
        if let Some(value) = payload.allow_background_shell {
            config.autopilot.allow_background_shell = value;
        }
        if let Some(value) = payload.allow_background_network {
            config.autopilot.allow_background_network = value;
        }
        if let Some(value) = payload.allow_background_self_edit {
            config.autopilot.allow_background_self_edit = value;
        }
        state.storage.save_config(&config)?;
        config.autopilot.clone()
    };
    append_log(
        &state,
        "info",
        "autopilot",
        format!(
            "autopilot={} interval={}s concurrency={}",
            match updated.state {
                AutopilotState::Disabled => "disabled",
                AutopilotState::Enabled => "enabled",
                AutopilotState::Paused => "paused",
            },
            updated.wake_interval_seconds,
            updated.max_concurrent_missions
        ),
    )?;
    state.autopilot_wake.notify_waiters();
    Ok(Json(updated))
}

pub(crate) async fn list_logs(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<LogEntry>>, ApiError> {
    Ok(Json(state.storage.list_logs(query.limit.unwrap_or(50))?))
}

pub(crate) async fn list_events(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> Result<Json<Vec<LogEntry>>, ApiError> {
    let after = query.after.as_deref().map(parse_event_cursor).transpose()?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let wait_seconds = query.wait_seconds.unwrap_or(0).min(30);

    let mut events = load_events(&state, after.clone(), limit)?;
    if events.is_empty() && wait_seconds > 0 {
        let _ = timeout(Duration::from_secs(wait_seconds), state.log_wake.notified()).await;
        events = load_events(&state, after, limit)?;
    }

    Ok(Json(events))
}

pub(crate) async fn doctor(State(state): State<AppState>) -> Result<Json<HealthReport>, ApiError> {
    let config = state.config.read().await.clone();
    let checks = join_all(
        config
            .all_providers()
            .iter()
            .map(|provider| crate::plugins::provider_health(&state, provider)),
    )
    .await;

    Ok(Json(HealthReport {
        daemon_running: true,
        config_path: state.storage.paths().config_path.display().to_string(),
        data_path: state.storage.paths().data_dir.display().to_string(),
        keyring_ok: agent_providers::keyring_available(),
        providers: checks,
        plugins: collect_plugin_doctor_reports(&config),
        remote_content_policy: config.remote_content_policy,
        provider_capabilities: provider_capability_summaries(&config),
    }))
}

pub(crate) fn provider_capability_summaries(
    config: &agent_core::AppConfig,
) -> Vec<ProviderCapabilitySummary> {
    config
        .all_providers()
        .into_iter()
        .filter_map(|provider| {
            provider.default_model.as_ref().map(|model| {
                let descriptor = agent_providers::describe_model(&provider, model);
                ProviderCapabilitySummary {
                    provider_id: provider.id.clone(),
                    model: descriptor.id,
                    capabilities: descriptor.capabilities,
                }
            })
        })
        .collect()
}

pub(crate) fn load_events(
    state: &AppState,
    after: Option<EventCursor>,
    limit: usize,
) -> Result<Vec<LogEntry>, ApiError> {
    let events = match after {
        Some(cursor) => {
            state
                .storage
                .list_logs_after_cursor(cursor.created_at, cursor.id.as_deref(), limit)?
        }
        None => {
            let mut logs = state.storage.list_logs(limit)?;
            logs.reverse();
            logs
        }
    };
    Ok(events)
}

pub(crate) fn next_event_cursor(entries: &[LogEntry]) -> Option<String> {
    entries.last().map(event_cursor_from_entry)
}

pub(crate) fn format_event_cursor(cursor: &EventCursor) -> String {
    match cursor.id.as_deref() {
        Some(id) if !id.trim().is_empty() => format!("{}|{}", cursor.created_at.to_rfc3339(), id),
        _ => cursor.created_at.to_rfc3339(),
    }
}

pub(crate) fn event_cursor_from_entry(entry: &LogEntry) -> String {
    format!("{}|{}", entry.created_at.to_rfc3339(), entry.id)
}

pub(crate) fn parse_event_cursor(value: &str) -> Result<EventCursor, ApiError> {
    let (created_at, id) = match value.split_once('|') {
        Some((created_at, id)) if !id.trim().is_empty() => {
            (created_at, Some(id.trim().to_string()))
        }
        _ => (value, None),
    };
    DateTime::parse_from_rfc3339(created_at)
        .map(|value| EventCursor {
            created_at: value.with_timezone(&Utc),
            id,
        })
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}
