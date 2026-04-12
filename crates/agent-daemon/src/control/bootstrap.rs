use agent_core::{
    AppConfig, DaemonStatus, DashboardBootstrapResponse, MainTargetSummary, MemoryReviewStatus,
    SkillDraftStatus, CONFIG_VERSION,
};
use agent_providers::delete_secret;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    append_log, delegation_targets_from_config, runtime::provider_has_runnable_access, ApiError,
    AppState,
};

use super::{
    configured_keychain_accounts, load_events, provider_capability_summaries,
    redact_provider_secret_metadata, sync_daemon_autostart_setting,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct OnboardingResetRequest {
    #[serde(default)]
    pub(crate) confirmed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct OnboardingResetResponse {
    pub(crate) removed_credentials: usize,
    #[serde(default)]
    pub(crate) credential_warnings: Vec<String>,
    pub(crate) daemon_token: String,
}

pub(crate) async fn status(State(state): State<AppState>) -> Result<Json<DaemonStatus>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(build_daemon_status(&state, &config)?))
}

pub(crate) async fn dashboard_bootstrap(
    State(state): State<AppState>,
) -> Result<Json<DashboardBootstrapResponse>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(build_dashboard_bootstrap_response(&state, &config)?))
}

pub(crate) fn build_dashboard_bootstrap_response(
    state: &AppState,
    config: &AppConfig,
) -> Result<DashboardBootstrapResponse, ApiError> {
    let status = build_daemon_status(state, config)?;
    let sessions = state.storage.list_sessions(25)?;
    let events = load_events(state, None, 40)?;
    let delegation_targets = delegation_targets_from_config(config, None);
    Ok(DashboardBootstrapResponse {
        status,
        providers: config
            .all_providers()
            .into_iter()
            .map(redact_provider_secret_metadata)
            .collect(),
        aliases: config.aliases.clone(),
        delegation_targets,
        telegram_connectors: config.telegram_connectors.clone(),
        discord_connectors: config.discord_connectors.clone(),
        slack_connectors: config.slack_connectors.clone(),
        signal_connectors: config.signal_connectors.clone(),
        home_assistant_connectors: config.home_assistant_connectors.clone(),
        webhook_connectors: config.webhook_connectors.clone(),
        inbox_connectors: config.inbox_connectors.clone(),
        gmail_connectors: config.gmail_connectors.clone(),
        brave_connectors: config.brave_connectors.clone(),
        plugins: config.plugins.clone(),
        sessions,
        events,
        permissions: config.permission_preset,
        trust: config.trust_policy.clone(),
        delegation_config: config.delegation.clone(),
        provider_capabilities: provider_capability_summaries(config),
        remote_content_policy: config.remote_content_policy,
    })
}

pub(crate) async fn export_config(
    State(state): State<AppState>,
) -> Result<Json<AppConfig>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(config))
}

pub(crate) async fn import_config(
    State(state): State<AppState>,
    Json(mut payload): Json<AppConfig>,
) -> Result<Json<AppConfig>, ApiError> {
    payload.version = CONFIG_VERSION;
    payload
        .validate_dashboard_mutation()
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;

    {
        let mut config = state.config.write().await;
        *config = payload.clone();
        state.storage.save_config(&config)?;
    }

    if let Err(error) = sync_daemon_autostart_setting(&state, payload.daemon.auto_start) {
        warn!(
            "failed to update auto-start after config import: {:?}",
            error
        );
    }

    append_log(&state, "info", "daemon", "full dashboard config updated")?;
    Ok(Json(payload))
}

pub(crate) fn build_daemon_status(
    state: &AppState,
    config: &AppConfig,
) -> Result<DaemonStatus, ApiError> {
    let mission_count = state.storage.count_missions()?;
    let active_missions = state.storage.count_active_missions()?;
    let delegation_targets = delegation_targets_from_config(config, None).len();
    Ok(DaemonStatus {
        pid: std::process::id(),
        started_at: state.started_at,
        persistence_mode: config.daemon.persistence_mode.clone(),
        auto_start: config.daemon.auto_start,
        main_agent_alias: config.main_agent_alias.clone(),
        main_target: runnable_main_target_summary(config),
        onboarding_complete: config.onboarding_complete,
        autonomy: config.autonomy.clone(),
        evolve: config.evolve.clone(),
        autopilot: config.autopilot.clone(),
        delegation: config.delegation.clone(),
        providers: config.all_providers().len(),
        aliases: config.aliases.len(),
        plugins: config.plugins.len(),
        delegation_targets,
        webhook_connectors: config.webhook_connectors.len(),
        inbox_connectors: config.inbox_connectors.len(),
        telegram_connectors: config.telegram_connectors.len(),
        discord_connectors: config.discord_connectors.len(),
        slack_connectors: config.slack_connectors.len(),
        home_assistant_connectors: config.home_assistant_connectors.len(),
        signal_connectors: config.signal_connectors.len(),
        gmail_connectors: config.gmail_connectors.len(),
        brave_connectors: config.brave_connectors.len(),
        pending_connector_approvals: state.storage.count_pending_connector_approvals()?,
        missions: mission_count,
        active_missions,
        memories: state.storage.count_memories()?,
        pending_memory_reviews: state
            .storage
            .count_memories_by_review_status(MemoryReviewStatus::Candidate)?,
        skill_drafts: state.storage.count_skill_drafts()?,
        published_skills: state
            .storage
            .count_skill_drafts_by_status(SkillDraftStatus::Published)?,
    })
}

fn runnable_main_target_summary(config: &AppConfig) -> Option<MainTargetSummary> {
    let summary = config.main_target_summary()?;
    let provider = config.resolve_provider(&summary.provider_id)?;
    provider_has_runnable_access(&provider).then_some(summary)
}

pub(crate) async fn shutdown(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    append_log(&state, "info", "daemon", "shutdown requested")?;
    let _ = state.shutdown.send(());
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn reset_onboarding(
    State(state): State<AppState>,
    Json(payload): Json<OnboardingResetRequest>,
) -> Result<Json<OnboardingResetResponse>, ApiError> {
    if !payload.confirmed {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "onboarding reset requires confirmed=true",
        ));
    }

    let config_before = state.config.read().await.clone();
    let keychain_accounts = configured_keychain_accounts(&config_before);
    if let Err(error) = sync_daemon_autostart_setting(&state, false) {
        warn!(
            "failed to disable auto-start during onboarding reset: {:?}",
            error
        );
    }

    let mut removed_credentials = 0usize;
    let mut credential_warnings = Vec::new();
    for account in keychain_accounts {
        match delete_secret(&account) {
            Ok(()) => removed_credentials += 1,
            Err(error) => credential_warnings.push(format!("{account}: {error}")),
        }
    }

    state.storage.reset_all()?;
    let reset_config = state.storage.load_config()?;
    {
        let mut config = state.config.write().await;
        *config = reset_config.clone();
    }
    state.browser_auth_sessions.write().await.clear();
    state.dashboard_launches.write().await.clear();
    state
        .mission_cancellations
        .lock()
        .expect("mission cancellation lock poisoned")
        .clear();
    state.autopilot_wake.notify_waiters();
    state.log_wake.notify_waiters();

    append_log(
        &state,
        "warn",
        "daemon",
        format!(
            "dashboard onboarding reset completed; removed {removed_credentials} credential(s)"
        ),
    )?;

    Ok(Json(OnboardingResetResponse {
        removed_credentials,
        credential_warnings,
        daemon_token: reset_config.daemon.token,
    }))
}
