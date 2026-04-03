use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Duration,
};

use agent_core::{
    AliasUpsertRequest, AppConfig, AutonomyEnableRequest, AutonomyMode, AutonomyState,
    AutopilotConfig, AutopilotState, AutopilotUpdateRequest, DaemonConfigUpdateRequest,
    DaemonStatus, DashboardBootstrapResponse, DelegationConfig, DelegationConfigUpdateRequest,
    DelegationTarget, HealthReport, LogEntry, MainAliasUpdateRequest, MainTargetSummary,
    McpServerConfig, McpServerUpsertRequest, MemoryReviewStatus, ModelAlias, ModelDescriptor,
    PermissionPreset, PermissionUpdateRequest, ProviderCapabilitySummary, ProviderConfig,
    ProviderSuggestionRequest, ProviderSuggestionResponse, ProviderUpsertRequest, SkillDraftStatus,
    SkillUpdateRequest, TrustUpdateRequest, CONFIG_VERSION, INTERNAL_DAEMON_ARG,
};
use agent_policy::{autonomy_warning, permission_summary};
use agent_providers::{delete_secret, store_api_key, store_oauth_token};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::warn;

use crate::{
    append_log, collect_plugin_doctor_reports, delegation_targets_from_config,
    normalize_delegation_limit, runtime::provider_has_runnable_access, ApiError, AppState,
    LimitQuery,
};
use crate::{
    collect_hosted_plugin_tools,
    tools::{effective_tool_definitions, remote_content::RemoteContentRuntimeState, ToolContext},
};
use agent_core::{HostedToolKind, ModelToolCapabilities, ToolBackend, ToolDefinition};

fn redact_provider_secret_metadata(mut provider: ProviderConfig) -> ProviderConfig {
    provider.keychain_account = None;
    provider
}

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
    config: &agent_core::AppConfig,
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
        provider_capabilities: provider_capability_summaries(state, config)?,
        remote_content_policy: config.remote_content_policy,
    })
}

pub(crate) async fn export_config(
    State(state): State<AppState>,
) -> Result<Json<agent_core::AppConfig>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(config))
}

pub(crate) async fn import_config(
    State(state): State<AppState>,
    Json(mut payload): Json<agent_core::AppConfig>,
) -> Result<Json<agent_core::AppConfig>, ApiError> {
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
    config: &agent_core::AppConfig,
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

fn config_has_runnable_main_alias(config: &agent_core::AppConfig) -> bool {
    config
        .main_alias()
        .ok()
        .and_then(|alias| config.resolve_provider(&alias.provider_id))
        .is_some_and(|provider| provider_has_runnable_access(&provider))
}

fn runnable_main_target_summary(config: &agent_core::AppConfig) -> Option<MainTargetSummary> {
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

pub(crate) async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(
        config
            .all_providers()
            .into_iter()
            .map(redact_provider_secret_metadata)
            .collect(),
    ))
}

pub(crate) async fn suggest_provider_defaults(
    State(state): State<AppState>,
    Json(payload): Json<ProviderSuggestionRequest>,
) -> Result<Json<ProviderSuggestionResponse>, ApiError> {
    let preferred_provider_id = payload.preferred_provider_id.trim();
    if preferred_provider_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "preferred_provider_id must not be empty",
        ));
    }

    let config = state.config.read().await;
    let editing_provider_id = payload
        .editing_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let provider_id =
        config.next_available_provider_id_excluding(preferred_provider_id, editing_provider_id);

    let default_model = payload
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let editing_alias_name = payload
        .editing_alias_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let alias_name = default_model.as_deref().map(|default_model| {
        let preferred_alias_name = payload
            .preferred_alias_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| config.default_alias_name_for(&provider_id, default_model));
        config.next_available_alias_name_excluding(&preferred_alias_name, editing_alias_name)
    });

    Ok(Json(ProviderSuggestionResponse {
        provider_id,
        alias_name,
        alias_model: default_model,
        would_be_first_main: !config_has_runnable_main_alias(&config),
    }))
}

pub(crate) async fn upsert_provider(
    State(state): State<AppState>,
    Json(mut payload): Json<ProviderUpsertRequest>,
) -> Result<Json<ProviderConfig>, ApiError> {
    let existing_provider = {
        let config = state.config.read().await;
        if config.is_projected_plugin_provider(&payload.provider.id) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "plugin-backed providers are managed by their plugin package",
            ));
        }
        config.get_provider(&payload.provider.id).cloned()
    };

    if let Some(api_key) = payload.api_key.take() {
        let account = store_api_key(&payload.provider.id, &api_key)?;
        payload.provider.keychain_account = Some(account);
    }

    if let Some(token) = payload.oauth_token.take() {
        let account = store_oauth_token(&payload.provider.id, &token)?;
        payload.provider.keychain_account = Some(account);
    }

    if payload.provider.keychain_account.is_none()
        && !matches!(payload.provider.auth_mode, agent_core::AuthMode::None)
    {
        if let Some(existing) =
            existing_provider.filter(|existing| existing.auth_mode == payload.provider.auth_mode)
        {
            payload.provider.keychain_account = existing.keychain_account.clone();
        }
    }

    {
        let mut config = state.config.write().await;
        config.upsert_provider(payload.provider.clone());
        state.storage.save_config(&config)?;
    }

    append_log(
        &state,
        "info",
        "providers",
        format!("provider '{}' updated", payload.provider.id),
    )?;
    Ok(Json(redact_provider_secret_metadata(payload.provider)))
}

pub(crate) async fn delete_provider(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed, removed_aliases, secret_account) = {
        let mut config = state.config.write().await;
        let secret_account = config
            .get_provider(&provider_id)
            .and_then(|provider| provider.keychain_account.clone());
        let aliases_before = config.aliases.len();
        let removed = config.remove_provider(&provider_id);
        let removed_aliases = aliases_before.saturating_sub(config.aliases.len());
        if removed {
            state.storage.save_config(&config)?;
        }
        (removed, removed_aliases, secret_account)
    };

    if !removed {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown provider"));
    }

    if let Some(account) = secret_account {
        delete_secret(&account)?;
    }

    append_log(
        &state,
        "warn",
        "providers",
        format!(
            "provider '{}' removed ({} alias{})",
            provider_id,
            removed_aliases,
            if removed_aliases == 1 { "" } else { "es" }
        ),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_provider_models(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let provider = {
        let config = state.config.read().await;
        config
            .resolve_provider(&provider_id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown provider"))?
    };

    let config = state.config.read().await.clone();
    let models =
        if let Some(result) = crate::plugins::plugin_provider_models(&config, &provider_id).await {
            result?
        } else {
            agent_providers::list_models(&state.http_client, &provider).await?
        };
    Ok(Json(models))
}

pub(crate) async fn list_provider_model_descriptors(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<Vec<ModelDescriptor>>, ApiError> {
    let provider = {
        let config = state.config.read().await;
        config
            .resolve_provider(&provider_id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown provider"))?
    };

    let descriptors =
        agent_providers::list_model_descriptors(&state.http_client, &provider).await?;
    Ok(Json(descriptors))
}

pub(crate) async fn clear_provider_credentials(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<ProviderConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if config.is_projected_plugin_provider(&provider_id) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "plugin-backed providers are managed by their plugin package",
            ));
        }
        let provider = config
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown provider"))?;
        if let Some(account) = provider.keychain_account.take() {
            delete_secret(&account)?;
        }
        let updated = provider.clone();
        state.storage.save_config(&config)?;
        updated
    };

    append_log(
        &state,
        "warn",
        "providers",
        format!("provider '{}' credentials cleared", updated.id),
    )?;
    Ok(Json(updated))
}

pub(crate) async fn list_aliases(
    State(state): State<AppState>,
) -> Result<Json<Vec<ModelAlias>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.aliases.clone()))
}

pub(crate) async fn upsert_alias(
    State(state): State<AppState>,
    Json(payload): Json<AliasUpsertRequest>,
) -> Result<Json<ModelAlias>, ApiError> {
    {
        let mut config = state.config.write().await;
        if config
            .resolve_provider(&payload.alias.provider_id)
            .is_none()
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "alias references unknown provider",
            ));
        }
        if payload.set_as_main {
            config.main_agent_alias = Some(payload.alias.alias.clone());
        }
        config.upsert_alias(payload.alias.clone());
        state.storage.save_config(&config)?;
    }

    append_log(
        &state,
        "info",
        "aliases",
        format!(
            "alias '{}' points to {}{}",
            payload.alias.alias,
            payload.alias.provider_id,
            if payload.set_as_main { " (main)" } else { "" }
        ),
    )?;
    Ok(Json(payload.alias))
}

pub(crate) async fn delete_alias(
    State(state): State<AppState>,
    Path(alias_name): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config.remove_alias(&alias_name);
        if removed {
            state.storage.save_config(&config)?;
        }
        removed
    };

    if !removed {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown alias"));
    }

    append_log(
        &state,
        "info",
        "aliases",
        format!("alias '{}' removed", alias_name),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn update_main_alias(
    State(state): State<AppState>,
    Json(payload): Json<MainAliasUpdateRequest>,
) -> Result<Json<MainTargetSummary>, ApiError> {
    let alias_name = payload.alias.trim();
    if alias_name.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "alias must not be empty",
        ));
    }

    let summary = {
        let mut config = state.config.write().await;
        let summary = config.alias_target_summary(alias_name).ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "alias must reference a known provider",
            )
        })?;
        config.main_agent_alias = Some(summary.alias.clone());
        state.storage.save_config(&config)?;
        summary
    };

    append_log(
        &state,
        "info",
        "aliases",
        format!(
            "main alias set to '{}' ({}/{})",
            summary.alias, summary.provider_id, summary.model
        ),
    )?;
    Ok(Json(summary))
}

pub(crate) async fn get_trust(
    State(state): State<AppState>,
) -> Result<Json<agent_core::TrustPolicy>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.trust_policy.clone()))
}

pub(crate) async fn update_trust(
    State(state): State<AppState>,
    Json(payload): Json<TrustUpdateRequest>,
) -> Result<Json<agent_core::TrustPolicy>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if let Some(allow_shell) = payload.allow_shell {
            config.trust_policy.allow_shell = allow_shell;
        }
        if let Some(allow_network) = payload.allow_network {
            config.trust_policy.allow_network = allow_network;
        }
        if let Some(allow_full_disk) = payload.allow_full_disk {
            config.trust_policy.allow_full_disk = allow_full_disk;
        }
        if let Some(allow_self_edit) = payload.allow_self_edit {
            config.trust_policy.allow_self_edit = allow_self_edit;
        }

        if let Some(path) = payload.trusted_path {
            if !config.trust_policy.trusted_paths.contains(&path) {
                config.trust_policy.trusted_paths.push(path);
            }
        }

        state.storage.save_config(&config)?;
        config.trust_policy.clone()
    };

    append_log(&state, "warn", "trust", "trust policy updated")?;
    Ok(Json(updated))
}

pub(crate) async fn get_permission_preset(
    State(state): State<AppState>,
) -> Result<Json<PermissionPreset>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.permission_preset))
}

pub(crate) async fn update_permission_preset(
    State(state): State<AppState>,
    Json(payload): Json<PermissionUpdateRequest>,
) -> Result<Json<PermissionPreset>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.permission_preset = payload.permission_preset;
        state.storage.save_config(&config)?;
        config.permission_preset
    };
    append_log(
        &state,
        "info",
        "permissions",
        format!("permission preset set to {}", permission_summary(updated)),
    )?;
    Ok(Json(updated))
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
        config.autonomy.allow_self_edit = payload
            .allow_self_edit
            .unwrap_or(config.autonomy.allow_self_edit);
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

pub(crate) async fn update_daemon_config(
    State(state): State<AppState>,
    Json(payload): Json<DaemonConfigUpdateRequest>,
) -> Result<Json<agent_core::DaemonConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if let Some(persistence_mode) = payload.persistence_mode {
            config.daemon.persistence_mode = persistence_mode;
        }
        if let Some(auto_start) = payload.auto_start {
            config.daemon.auto_start = auto_start;
        }
        state.storage.save_config(&config)?;
        config.daemon.clone()
    };

    if let Err(error) = sync_daemon_autostart_setting(&state, updated.auto_start) {
        warn!("failed to update auto-start: {:?}", error);
    }

    append_log(&state, "info", "daemon", "daemon config updated")?;
    Ok(Json(updated))
}

fn sync_daemon_autostart_setting(state: &AppState, auto_start: bool) -> Result<(), ApiError> {
    let daemon_path = std::env::current_exe()
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    state
        .storage
        .sync_autostart(&daemon_path, &[INTERNAL_DAEMON_ARG], auto_start)
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

fn configured_keychain_accounts(config: &AppConfig) -> BTreeSet<String> {
    let mut accounts = config
        .providers
        .iter()
        .filter_map(|provider| provider.keychain_account.clone())
        .collect::<BTreeSet<_>>();
    accounts.extend(
        config
            .telegram_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .discord_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .slack_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .home_assistant_connectors
            .iter()
            .filter_map(|connector| connector.access_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .brave_connectors
            .iter()
            .filter_map(|connector| connector.api_key_keychain_account.clone()),
    );
    accounts.extend(
        config
            .gmail_connectors
            .iter()
            .filter_map(|connector| connector.oauth_keychain_account.clone()),
    );
    accounts
}

pub(crate) async fn delegation_status(
    State(state): State<AppState>,
) -> Result<Json<DelegationConfig>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.delegation.clone()))
}

pub(crate) async fn update_delegation_config(
    State(state): State<AppState>,
    Json(payload): Json<DelegationConfigUpdateRequest>,
) -> Result<Json<DelegationConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if let Some(max_depth) = payload.max_depth {
            config.delegation.max_depth = normalize_delegation_limit(max_depth, 1)?;
        }
        if let Some(max_parallel_subagents) = payload.max_parallel_subagents {
            config.delegation.max_parallel_subagents =
                normalize_delegation_limit(max_parallel_subagents, 1)?;
        }
        if let Some(disabled_provider_ids) = payload.disabled_provider_ids {
            config.delegation.disabled_provider_ids = disabled_provider_ids
                .into_iter()
                .filter(|provider_id| config.resolve_provider(provider_id).is_some())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
        }
        state.storage.save_config(&config)?;
        config.delegation.clone()
    };
    append_log(&state, "info", "delegation", "delegation config updated")?;
    Ok(Json(updated))
}

pub(crate) async fn list_delegation_targets(
    State(state): State<AppState>,
) -> Result<Json<Vec<DelegationTarget>>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(delegation_targets_from_config(&config, None)))
}

pub(crate) async fn list_mcp_servers(
    State(state): State<AppState>,
) -> Result<Json<Vec<McpServerConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.mcp_servers.clone()))
}

pub(crate) async fn upsert_mcp_server(
    State(state): State<AppState>,
    Json(payload): Json<McpServerUpsertRequest>,
) -> Result<Json<McpServerConfig>, ApiError> {
    {
        let mut config = state.config.write().await;
        config.upsert_mcp_server(payload.server.clone());
        state.storage.save_config(&config)?;
    }
    append_log(
        &state,
        "info",
        "mcp",
        format!("mcp server '{}' updated", payload.server.id),
    )?;
    Ok(Json(payload.server))
}

pub(crate) async fn delete_mcp_server(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config.remove_mcp_server(&server_id);
        if removed {
            state.storage.save_config(&config)?;
        }
        removed
    };
    if !removed {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown MCP server"));
    }
    append_log(
        &state,
        "info",
        "mcp",
        format!("mcp server '{}' removed", server_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_enabled_skills(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.enabled_skills.clone()))
}

pub(crate) async fn update_enabled_skills(
    State(state): State<AppState>,
    Json(payload): Json<SkillUpdateRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.enabled_skills = payload.enabled_skills;
        state.storage.save_config(&config)?;
        config.enabled_skills.clone()
    };
    append_log(
        &state,
        "info",
        "skills",
        format!("enabled {} skill(s)", updated.len()),
    )?;
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
        provider_capabilities: provider_capability_summaries(&state, &config)?,
    }))
}

fn provider_capability_summaries(
    state: &AppState,
    config: &AppConfig,
) -> Result<Vec<ProviderCapabilitySummary>, ApiError> {
    config
        .all_providers()
        .into_iter()
        .filter_map(|provider| {
            provider
                .default_model
                .clone()
                .map(|model| (provider, model))
        })
        .map(|(provider, model)| {
            let descriptor = agent_providers::describe_model(&provider, &model);
            let capabilities = runtime_registered_model_capabilities(
                state,
                config,
                &provider,
                &model,
                descriptor.capabilities,
            )?;
            Ok(ProviderCapabilitySummary {
                provider_id: provider.id.clone(),
                model: descriptor.id,
                capabilities,
            })
        })
        .collect()
}

fn runtime_registered_model_capabilities(
    state: &AppState,
    config: &AppConfig,
    provider: &ProviderConfig,
    model: &str,
    model_capabilities: ModelToolCapabilities,
) -> Result<ModelToolCapabilities, ApiError> {
    let context = ToolContext {
        state: state.clone(),
        cwd: state.storage.paths().data_dir.clone(),
        trust_policy: config.trust_policy.clone(),
        autonomy: config.autonomy.clone(),
        permission_preset: config.permission_preset,
        http_client: state.http_client.clone(),
        mcp_servers: config.mcp_servers.clone(),
        app_connectors: config.app_connectors.clone(),
        plugin_tools: collect_hosted_plugin_tools(config),
        brave_connectors: config.brave_connectors.clone(),
        current_alias: None,
        default_thinking_level: config.thinking_level,
        task_mode: None,
        delegation: config.delegation.clone(),
        delegation_targets: delegation_targets_from_config(config, None),
        delegation_depth: 0,
        background: false,
        background_shell_allowed: true,
        background_network_allowed: true,
        background_self_edit_allowed: true,
        model_capabilities,
        remote_content_policy: config.remote_content_policy,
        remote_content_state: Arc::new(Mutex::new(RemoteContentRuntimeState::default())),
        allowed_direct_urls: Arc::new(HashSet::new()),
    };
    let tools = effective_tool_definitions(&context).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to resolve runtime tool capabilities for provider '{}' model '{}': {error}",
                provider.id, model
            ),
        )
    })?;
    Ok(tool_registry_capabilities(&tools))
}

fn tool_registry_capabilities(tools: &[ToolDefinition]) -> ModelToolCapabilities {
    let mut capabilities = ModelToolCapabilities::default();
    for tool in tools {
        match (tool.name.as_str(), tool.backend, tool.hosted_kind) {
            ("web_search", ToolBackend::ProviderBuiltin, Some(HostedToolKind::WebSearch)) => {
                capabilities.web_search = true;
            }
            ("run_shell", ToolBackend::LocalFunction, _) => {
                capabilities.shell = true;
                capabilities.local_shell = true;
            }
            ("apply_patch", ToolBackend::LocalFunction, _) => {
                capabilities.apply_patch = true;
            }
            _ => {}
        }
    }
    capabilities
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

#[cfg(test)]
mod tests {
    use super::{
        enable_autonomy, runtime_registered_model_capabilities, tool_registry_capabilities,
    };
    use agent_core::{
        AppConfig, AutonomyEnableRequest, AutonomyMode, AutonomyProfile, AutonomyState,
        HostedToolKind, ModelToolCapabilities, PermissionPreset, ProviderConfig, ProviderKind,
        ToolBackend, ToolDefinition, TrustPolicy,
    };
    use axum::{extract::State, Json};
    use chrono::Utc;
    use reqwest::Client;
    use std::sync::{atomic::AtomicBool, Arc};
    use tokio::sync::{mpsc, Notify, RwLock};

    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, AppState, ProviderRateLimiter,
    };

    fn test_state(config: AppConfig) -> AppState {
        let storage = agent_storage::Storage::open_at(
            std::env::temp_dir().join(format!("agent-control-test-{}", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let (shutdown_tx, _) = mpsc::unbounded_channel();
        AppState {
            storage,
            config: Arc::new(RwLock::new(config)),
            http_client: Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: Utc::now(),
            shutdown: shutdown_tx,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[test]
    fn tool_registry_capabilities_reflect_actual_registered_tools() {
        let tools = vec![
            ToolDefinition {
                name: "web_search".to_string(),
                description: "web".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                backend: ToolBackend::ProviderBuiltin,
                hosted_kind: Some(HostedToolKind::WebSearch),
                strict_schema: false,
            },
            ToolDefinition {
                name: "run_shell".to_string(),
                description: "shell".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                backend: ToolBackend::LocalFunction,
                hosted_kind: None,
                strict_schema: true,
            },
            ToolDefinition {
                name: "apply_patch".to_string(),
                description: "patch".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
                backend: ToolBackend::LocalFunction,
                hosted_kind: None,
                strict_schema: true,
            },
        ];

        let capabilities = tool_registry_capabilities(&tools);

        assert!(capabilities.web_search);
        assert!(capabilities.shell);
        assert!(capabilities.local_shell);
        assert!(capabilities.apply_patch);
        assert!(!capabilities.file_search);
        assert!(!capabilities.code_interpreter);
        assert!(!capabilities.remote_mcp);
        assert!(!capabilities.tool_search);
    }

    #[test]
    fn runtime_registered_model_capabilities_use_effective_tool_registry() {
        let mut config = AppConfig {
            permission_preset: PermissionPreset::FullAuto,
            trust_policy: TrustPolicy {
                trusted_paths: Vec::new(),
                allow_shell: true,
                allow_network: true,
                allow_full_disk: false,
                allow_self_edit: false,
            },
            autonomy: AutonomyProfile::default(),
            ..AppConfig::default()
        };
        config.providers.push(ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://example.com".to_string(),
            auth_mode: agent_core::AuthMode::None,
            default_model: Some("gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        });
        let state = test_state(config.clone());
        let provider = config.providers[0].clone();

        let capabilities = runtime_registered_model_capabilities(
            &state,
            &config,
            &provider,
            "gpt-4.1",
            ModelToolCapabilities {
                web_search: true,
                ..ModelToolCapabilities::default()
            },
        )
        .unwrap();

        assert!(capabilities.web_search);
        assert!(capabilities.shell);
        assert!(capabilities.apply_patch);
    }

    #[tokio::test]
    async fn enable_autonomy_preserves_self_edit_when_unspecified() {
        let mut config = AppConfig::default();
        config.autonomy.allow_self_edit = false;
        let state = test_state(config);

        let Json(profile) = enable_autonomy(
            State(state.clone()),
            Json(AutonomyEnableRequest {
                mode: Some(AutonomyMode::FreeThinking),
                allow_self_edit: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(profile.state, AutonomyState::Enabled);
        assert_eq!(profile.mode, AutonomyMode::FreeThinking);
        assert!(!profile.allow_self_edit);
        let saved = state.config.read().await;
        assert!(!saved.autonomy.allow_self_edit);
    }

    #[tokio::test]
    async fn enable_autonomy_allows_explicit_self_edit_override() {
        let state = test_state(AppConfig::default());

        let Json(profile) = enable_autonomy(
            State(state.clone()),
            Json(AutonomyEnableRequest {
                mode: Some(AutonomyMode::FreeThinking),
                allow_self_edit: Some(true),
            }),
        )
        .await
        .unwrap();

        assert!(profile.allow_self_edit);
        let saved = state.config.read().await;
        assert!(saved.autonomy.allow_self_edit);
    }
}
