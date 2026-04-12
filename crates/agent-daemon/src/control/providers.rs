use agent_core::{
    AliasUpsertRequest, MainAliasUpdateRequest, ModelAlias, ModelDescriptor, ProviderConfig,
    ProviderSuggestionRequest, ProviderSuggestionResponse, ProviderUpsertRequest,
};
use agent_providers::{delete_secret, store_api_key, store_oauth_token};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{append_log, runtime::provider_has_runnable_access, ApiError, AppState};

use super::redact_provider_secret_metadata;

fn config_has_runnable_main_alias(config: &agent_core::AppConfig) -> bool {
    config
        .main_alias()
        .ok()
        .and_then(|alias| config.resolve_provider(&alias.provider_id))
        .is_some_and(|provider| provider_has_runnable_access(&provider))
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
) -> Result<Json<agent_core::MainTargetSummary>, ApiError> {
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
