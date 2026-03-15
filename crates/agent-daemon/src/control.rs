use std::time::Duration;

use agent_core::{
    AliasUpsertRequest, AutonomyEnableRequest, AutonomyMode, AutonomyState, AutopilotConfig,
    AutopilotState, AutopilotUpdateRequest, DaemonConfigUpdateRequest, DaemonStatus,
    DashboardBootstrapResponse, DelegationConfig, DelegationConfigUpdateRequest,
    DelegationTarget, HealthReport, LogEntry, MainAliasUpdateRequest, MainTargetSummary,
    McpServerConfig, McpServerUpsertRequest, MemoryReviewStatus, ModelAlias, PermissionPreset,
    PermissionUpdateRequest, ProviderConfig, ProviderSuggestionRequest,
    ProviderSuggestionResponse, ProviderUpsertRequest,
    SkillDraftStatus, SkillUpdateRequest, TrustUpdateRequest, INTERNAL_DAEMON_ARG,
};
use agent_policy::{autonomy_warning, permission_summary};
use agent_providers::{delete_secret, health_check, store_api_key, store_oauth_token};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::Deserialize;
use tokio::time::timeout;
use tracing::warn;

use crate::{
    append_log, delegation_targets_from_config, normalize_delegation_limit,
    runtime::provider_has_runnable_access, ApiError, AppState, LimitQuery,
};

fn redact_provider_secret_metadata(mut provider: ProviderConfig) -> ProviderConfig {
    provider.keychain_account = None;
    provider
}

#[derive(Deserialize)]
pub(crate) struct EventQuery {
    pub(crate) after: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) wait_seconds: Option<u64>,
}

pub(crate) async fn status(State(state): State<AppState>) -> Result<Json<DaemonStatus>, ApiError> {
    let config = state.config.read().await.clone();
    Ok(Json(build_daemon_status(&state, &config)?))
}

pub(crate) async fn dashboard_bootstrap(
    State(state): State<AppState>,
) -> Result<Json<DashboardBootstrapResponse>, ApiError> {
    let config = state.config.read().await.clone();
    let status = build_daemon_status(&state, &config)?;
    let sessions = state.storage.list_sessions(25)?;
    let events = load_events(&state, None, 40)?;
    let delegation_targets = delegation_targets_from_config(&config, None);
    Ok(Json(DashboardBootstrapResponse {
        status,
        providers: config
            .providers
            .iter()
            .cloned()
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
        sessions,
        events,
        permissions: config.permission_preset,
        trust: config.trust_policy.clone(),
        delegation_config: config.delegation.clone(),
    }))
}

fn build_daemon_status(state: &AppState, config: &agent_core::AppConfig) -> Result<DaemonStatus, ApiError> {
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
        providers: config.providers.len(),
        aliases: config.aliases.len(),
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
        .and_then(|alias| config.get_provider(&alias.provider_id))
        .is_some_and(provider_has_runnable_access)
}

fn runnable_main_target_summary(
    config: &agent_core::AppConfig,
) -> Option<MainTargetSummary> {
    let summary = config.main_target_summary()?;
    let provider = config.get_provider(&summary.provider_id)?;
    provider_has_runnable_access(provider).then_some(summary)
}

pub(crate) async fn shutdown(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    append_log(&state, "info", "daemon", "shutdown requested")?;
    let _ = state.shutdown.send(());
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(
        config
            .providers
            .iter()
            .cloned()
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
        if let Some(existing) = existing_provider.filter(|existing| {
            existing.auth_mode == payload.provider.auth_mode
        }) {
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
            .get_provider(&provider_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown provider"))?
    };

    let models = agent_providers::list_models(&state.http_client, &provider).await?;
    Ok(Json(models))
}

pub(crate) async fn clear_provider_credentials(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<ProviderConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
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
        if config.get_provider(&payload.alias.provider_id).is_none() {
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
        let summary = config
            .alias_target_summary(alias_name)
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "alias must reference a known provider"))?;
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

    let daemon_path = std::env::current_exe()
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    if let Err(error) =
        state
            .storage
            .sync_autostart(&daemon_path, &[INTERNAL_DAEMON_ARG], updated.auto_start)
    {
        warn!("failed to update auto-start: {error}");
    }

    append_log(&state, "info", "daemon", "daemon config updated")?;
    Ok(Json(updated))
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
                .filter(|provider_id| config.get_provider(provider_id).is_some())
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

    let mut events = load_events(&state, after, limit)?;
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
            .providers
            .iter()
            .map(|provider| health_check(&state.http_client, provider)),
    )
    .await;

    Ok(Json(HealthReport {
        daemon_running: true,
        config_path: state.storage.paths().config_path.display().to_string(),
        data_path: state.storage.paths().data_dir.display().to_string(),
        keyring_ok: agent_providers::keyring_available(),
        providers: checks,
    }))
}

fn load_events(
    state: &AppState,
    after: Option<DateTime<Utc>>,
    limit: usize,
) -> Result<Vec<LogEntry>, ApiError> {
    let events = match after {
        Some(cursor) => state.storage.list_logs_after(cursor, limit)?,
        None => {
            let mut logs = state.storage.list_logs(limit)?;
            logs.reverse();
            logs
        }
    };
    Ok(events)
}

fn parse_event_cursor(value: &str) -> Result<DateTime<Utc>, ApiError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}
