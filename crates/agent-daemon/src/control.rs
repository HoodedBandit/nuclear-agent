use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Duration,
};

use agent_core::{
    AliasUpsertRequest, AppConfig, AutonomyEnableRequest, AutonomyMode, AutonomyState,
    AutopilotConfig, AutopilotState, AutopilotUpdateRequest, ConversationMessage,
    DaemonConfigUpdateRequest, DaemonStatus, DashboardBootstrapResponse, DelegationConfig,
    DelegationConfigUpdateRequest, DelegationTarget, HealthReport, LogEntry,
    MainAliasUpdateRequest, MainTargetSummary, McpServerConfig, McpServerUpsertRequest,
    MemoryReviewStatus, MessageRole, ModelAlias, ModelDescriptor, PermissionPreset,
    PermissionUpdateRequest, ProviderCapabilitySummary, ProviderConfig, ProviderDiscoveryResponse,
    ProviderProfile, ProviderReadinessResult, ProviderSuggestionRequest,
    ProviderSuggestionResponse, ProviderUpsertRequest, SkillDraftStatus, SkillUpdateRequest,
    TrustUpdateRequest, CONFIG_VERSION, INTERNAL_DAEMON_ARG,
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
    provider.provider_profile = Some(provider.effective_profile());
    provider
}

fn normalize_provider_upsert_payload(
    mut payload: ProviderUpsertRequest,
) -> Result<ProviderUpsertRequest, ApiError> {
    payload.provider = payload.provider.with_inferred_profile();
    if payload.provider.id.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider.id must not be empty",
        ));
    }
    if payload.provider.base_url.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider.base_url must not be empty",
        ));
    }
    if let Some(error) = payload.provider.explicit_profile_compatibility_error() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, error));
    }
    if payload.provider.effective_profile() == ProviderProfile::Anthropic
        && payload.provider.auth_mode != agent_core::AuthMode::ApiKey
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Anthropic third-party access requires an API key",
        ));
    }
    if payload.provider.auth_mode == agent_core::AuthMode::ApiKey
        && payload
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        && payload.provider.keychain_account.is_none()
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "api_key is required for API-key providers",
        ));
    }
    Ok(payload)
}

fn recommended_provider_model(
    provider: &ProviderConfig,
    descriptors: &[ModelDescriptor],
) -> Option<String> {
    if let Some(model) = provider
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    {
        return Some(model);
    }

    let profile = provider.effective_profile();
    let ranked_match = |candidates: &[&str]| {
        descriptors.iter().find(|descriptor| {
            let id = descriptor.id.to_ascii_lowercase();
            candidates.iter().any(|candidate| id.contains(candidate))
        })
    };

    if let Some(descriptor) = descriptors
        .iter()
        .find(|descriptor| descriptor.supports_parallel_tool_calls)
    {
        return Some(descriptor.id.clone());
    }

    let preferred = match profile {
        ProviderProfile::Moonshot => ranked_match(&["kimi-k2.5", "kimi-k2"]),
        ProviderProfile::Anthropic => ranked_match(&["sonnet", "claude"]),
        ProviderProfile::OpenAi | ProviderProfile::OpenRouter => {
            ranked_match(&["gpt-5", "gpt-4.1", "claude"])
        }
        ProviderProfile::Venice => ranked_match(&["venice"]),
        _ => None,
    };
    preferred
        .map(|descriptor| descriptor.id.clone())
        .or_else(|| descriptors.first().map(|descriptor| descriptor.id.clone()))
}

fn provider_discovery_warnings(
    provider: &ProviderConfig,
    descriptors: &[ModelDescriptor],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if descriptors.is_empty() {
        warnings.push(
            "The provider did not return a model list. You can still enter a model manually if the endpoint accepts it."
                .to_string(),
        );
    }
    if matches!(
        provider.effective_profile(),
        ProviderProfile::OpenRouter | ProviderProfile::Venice
    ) && !descriptors
        .iter()
        .any(|descriptor| descriptor.supports_parallel_tool_calls)
    {
        warnings.push(
            "No discovered models advertised tool-calling support. Agent mode may fail until you pick a compatible model."
                .to_string(),
        );
    }
    warnings
}

fn provider_validation_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "validation_probe".to_string(),
        description: "Validation probe tool definition used to verify tool-schema compatibility."
            .to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "echo": { "type": "string" }
            },
            "required": ["echo"],
            "additionalProperties": false
        }),
        backend: ToolBackend::LocalFunction,
        hosted_kind: None,
        strict_schema: true,
    }]
}

async fn validate_provider_readiness(
    state: &AppState,
    payload: ProviderUpsertRequest,
) -> Result<ProviderReadinessResult, ApiError> {
    let payload = normalize_provider_upsert_payload(payload)?;
    let descriptors = agent_providers::list_model_descriptors_with_overrides(
        &state.http_client,
        &payload.provider,
        payload.api_key.as_deref(),
        payload.oauth_token.as_ref(),
    )
    .await?;
    let model = recommended_provider_model(&payload.provider, &descriptors).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider readiness requires a default model or at least one discovered model",
        )
    })?;
    if matches!(
        payload.provider.effective_profile(),
        ProviderProfile::OpenRouter | ProviderProfile::Venice
    ) && descriptors
        .iter()
        .find(|descriptor| descriptor.id == model)
        .is_some_and(|descriptor| !descriptor.supports_parallel_tool_calls)
    {
        return Ok(ProviderReadinessResult {
            ok: false,
            model,
            detail: "selected model does not advertise tool-calling support".to_string(),
        });
    }

    let validation_messages = [ConversationMessage {
        role: MessageRole::User,
        content: "Reply with the word ready.".to_string(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: Vec::new(),
        provider_output_items: Vec::new(),
    }];
    let validation_tools = provider_validation_tools();

    let readiness = agent_providers::run_prompt_with_overrides(
        &state.http_client,
        &payload.provider,
        agent_providers::PromptRunRequest {
            messages: &validation_messages,
            requested_model: Some(&model),
            session_id: None,
            thinking_level: None,
            tools: &validation_tools,
            auth_overrides: agent_providers::PromptAuthOverrides {
                api_key: payload.api_key.as_deref(),
                oauth_token: payload.oauth_token.as_ref(),
            },
        },
    )
    .await;

    match readiness {
        Ok(reply) => Ok(ProviderReadinessResult {
            ok: true,
            model,
            detail: if reply.tool_calls.is_empty() {
                "completion and tool schema validation succeeded".to_string()
            } else {
                "completion succeeded and the model accepted tool schemas".to_string()
            },
        }),
        Err(error) => Ok(ProviderReadinessResult {
            ok: false,
            model,
            detail: error.to_string(),
        }),
    }
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

    if payload.provider.keychain_account.is_none()
        && !matches!(payload.provider.auth_mode, agent_core::AuthMode::None)
        && payload
            .api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        && payload.oauth_token.is_none()
    {
        if let Some(existing) = existing_provider
            .as_ref()
            .filter(|existing| existing.auth_mode == payload.provider.auth_mode)
        {
            payload.provider.keychain_account = existing.keychain_account.clone();
        }
    }

    payload = normalize_provider_upsert_payload(payload)?;

    if let Some(api_key) = payload.api_key.take() {
        let account = store_api_key(&payload.provider.id, &api_key)?;
        payload.provider.keychain_account = Some(account);
    }

    if let Some(token) = payload.oauth_token.take() {
        let account = store_oauth_token(&payload.provider.id, &token)?;
        payload.provider.keychain_account = Some(account);
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

pub(crate) async fn discover_provider_models(
    State(state): State<AppState>,
    Json(payload): Json<ProviderUpsertRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let payload = normalize_provider_upsert_payload(payload)?;
    let provider = payload.provider;

    let models = agent_providers::list_models_with_overrides(
        &state.http_client,
        &provider,
        payload.api_key.as_deref(),
        payload.oauth_token.as_ref(),
    )
    .await?;
    Ok(Json(models))
}

pub(crate) async fn discover_provider(
    State(state): State<AppState>,
    Json(payload): Json<ProviderUpsertRequest>,
) -> Result<Json<ProviderDiscoveryResponse>, ApiError> {
    let payload = normalize_provider_upsert_payload(payload)?;
    let descriptors = agent_providers::list_model_descriptors_with_overrides(
        &state.http_client,
        &payload.provider,
        payload.api_key.as_deref(),
        payload.oauth_token.as_ref(),
    )
    .await?;
    let recommended_model = recommended_provider_model(&payload.provider, &descriptors);
    let warnings = provider_discovery_warnings(&payload.provider, &descriptors);
    Ok(Json(ProviderDiscoveryResponse {
        models: descriptors,
        recommended_model,
        warnings,
        readiness: None,
    }))
}

pub(crate) async fn validate_provider(
    State(state): State<AppState>,
    Json(payload): Json<ProviderUpsertRequest>,
) -> Result<Json<ProviderReadinessResult>, ApiError> {
    Ok(Json(validate_provider_readiness(&state, payload).await?))
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
        discover_provider_models, enable_autonomy, normalize_provider_upsert_payload,
        runtime_registered_model_capabilities, tool_registry_capabilities,
    };
    use agent_core::{
        AppConfig, AuthMode, AutonomyEnableRequest, AutonomyMode, AutonomyProfile, AutonomyState,
        HostedToolKind, ModelToolCapabilities, OAuthToken, PermissionPreset, ProviderConfig,
        ProviderKind, ProviderProfile, ProviderUpsertRequest, ToolBackend, ToolDefinition,
        TrustPolicy,
    };
    use axum::{extract::State, http::StatusCode, Json};
    use chrono::Utc;
    use reqwest::Client;
    use std::{
        collections::HashMap,
        sync::{atomic::AtomicBool, Arc},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::{mpsc, oneshot, Notify, RwLock},
    };

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

    #[derive(Debug)]
    struct CapturedHttpRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
    }

    fn parse_http_request(raw: &str) -> CapturedHttpRequest {
        let (head, _) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().expect("request line should be present");
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts
            .next()
            .expect("request method should be present")
            .to_string();
        let path = request_parts
            .next()
            .expect("request path should be present")
            .to_string();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        CapturedHttpRequest {
            method,
            path,
            headers,
        }
    }

    async fn read_local_http_request(
        stream: &mut tokio::net::TcpStream,
    ) -> std::io::Result<String> {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 1024];
        let mut header_end = None;
        let mut content_length = 0usize;

        loop {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if header_end.is_none() {
                if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(position + 4);
                    let headers = String::from_utf8_lossy(&buffer[..position + 4]);
                    content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':').and_then(|(name, value)| {
                                if name.eq_ignore_ascii_case("content-length") {
                                    value.trim().parse().ok()
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or(0);
                }
            }
            if let Some(end) = header_end {
                if buffer.len() >= end + content_length {
                    break;
                }
            }
        }

        Ok(String::from_utf8_lossy(&buffer).into_owned())
    }

    async fn spawn_json_response_server(
        status_line: &str,
        response_body: &str,
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test server should bind");
        let addr = listener
            .local_addr()
            .expect("listener should have an address");
        let status_line = status_line.to_string();
        let response_body = response_body.to_string();
        let (request_tx, request_rx) = oneshot::channel();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("server should accept");
            let request = read_local_http_request(&mut stream)
                .await
                .expect("server should read request");
            let _ = request_tx.send(request);
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("server should write response");
        });
        (format!("http://{addr}"), request_rx)
    }

    async fn spawn_json_response_sequence_server(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test server should bind");
        let addr = listener
            .local_addr()
            .expect("listener should have an address");
        let (request_tx, request_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            for (status_line, response_body) in responses {
                let (mut stream, _) = listener.accept().await.expect("server should accept");
                let request = read_local_http_request(&mut stream)
                    .await
                    .expect("server should read request");
                let _ = request_tx.send(request);
                let response = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("server should write response");
            }
        });
        (format!("http://{addr}"), request_rx)
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
            provider_profile: None,
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

    #[tokio::test]
    async fn discover_provider_models_uses_api_key_override_for_openai_compatible_hosts() {
        let (base_url, request_rx) = spawn_json_response_server(
            "200 OK",
            r#"{"data":[{"id":"kimi-k2.5"},{"id":"kimi-k2"}]}"#,
        )
        .await;
        let state = test_state(AppConfig::default());

        let Json(models) = discover_provider_models(
            State(state),
            Json(ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: "moonshot".to_string(),
                    display_name: "Moonshot".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url,
                    provider_profile: None,
                    auth_mode: AuthMode::ApiKey,
                    default_model: None,
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                api_key: Some("moonshot-test-key".to_string()),
                oauth_token: None,
            }),
        )
        .await
        .expect("model discovery should succeed");

        assert_eq!(models, vec!["kimi-k2.5".to_string(), "kimi-k2".to_string()]);
        let request = parse_http_request(&request_rx.await.expect("server should capture request"));
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/models");
        assert_eq!(
            request.headers.get("authorization").map(String::as_str),
            Some("Bearer moonshot-test-key")
        );
    }

    #[test]
    fn normalize_provider_upsert_payload_rejects_anthropic_non_api_auth() {
        let error = normalize_provider_upsert_payload(ProviderUpsertRequest {
            provider: ProviderConfig {
                id: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                kind: ProviderKind::Anthropic,
                base_url: "https://api.anthropic.com".to_string(),
                provider_profile: Some(ProviderProfile::Anthropic),
                auth_mode: AuthMode::OAuth,
                default_model: Some("claude-sonnet-4-20250514".to_string()),
                keychain_account: Some("anthropic-oauth".to_string()),
                oauth: None,
                local: false,
            },
            api_key: None,
            oauth_token: Some(OAuthToken {
                access_token: "token".to_string(),
                refresh_token: None,
                expires_at: None,
                scopes: Vec::new(),
                token_type: None,
                id_token: None,
                account_id: None,
                user_id: None,
                org_id: None,
                project_id: None,
                display_email: None,
                subscription_type: None,
            }),
        })
        .expect_err("anthropic oauth payload should be rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            error
                .message
                .contains("Anthropic third-party access requires an API key"),
            "unexpected error: {}",
            error.message
        );
    }

    #[test]
    fn normalize_provider_upsert_payload_rejects_incompatible_provider_profile() {
        let error = normalize_provider_upsert_payload(ProviderUpsertRequest {
            provider: ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: "https://api.openai.com/v1".to_string(),
                provider_profile: Some(ProviderProfile::Anthropic),
                auth_mode: AuthMode::ApiKey,
                default_model: Some("gpt-5".to_string()),
                keychain_account: Some("openai-key".to_string()),
                oauth: None,
                local: false,
            },
            api_key: None,
            oauth_token: None,
        })
        .expect_err("incompatible provider profile should be rejected");

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(
            error.message.contains("provider_profile is incompatible"),
            "unexpected error: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn validate_provider_accepts_manual_model_when_discovery_is_incomplete() {
        let (base_url, mut request_rx) = spawn_json_response_sequence_server(vec![
            ("200 OK", r#"{"data":[{"id":"kimi-k2.5"}]}"#),
            (
                "200 OK",
                r#"{"choices":[{"message":{"role":"assistant","content":"ready"}}]}"#,
            ),
        ])
        .await;
        let state = test_state(AppConfig::default());

        let readiness = super::validate_provider_readiness(
            &state,
            ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: "moonshot".to_string(),
                    display_name: "Moonshot".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url,
                    provider_profile: Some(ProviderProfile::Moonshot),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("manual-kimi-preview".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                api_key: Some("moonshot-test-key".to_string()),
                oauth_token: None,
            },
        )
        .await
        .expect("provider validation should allow manual model ids");

        assert!(readiness.ok);
        assert_eq!(readiness.model, "manual-kimi-preview");

        let discovery_request = parse_http_request(
            &request_rx
                .recv()
                .await
                .expect("discovery request should be captured"),
        );
        assert_eq!(discovery_request.method, "GET");
        assert_eq!(discovery_request.path, "/models");

        let completion_request = request_rx
            .recv()
            .await
            .expect("completion request should be captured");
        assert!(completion_request.starts_with("POST /chat/completions "));
        assert!(
            completion_request.contains("\"model\":\"manual-kimi-preview\""),
            "completion request should use the manual model: {completion_request}"
        );
    }
}
