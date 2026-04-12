use agent_core::{
    DaemonConfigUpdateRequest, DelegationConfig, DelegationConfigUpdateRequest, DelegationTarget,
    McpServerConfig, McpServerUpsertRequest, PermissionPreset, PermissionUpdateRequest,
    SkillUpdateRequest, TrustUpdateRequest,
};
use agent_policy::permission_summary;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tracing::warn;

use crate::{
    append_log, delegation_targets_from_config, normalize_delegation_limit, ApiError, AppState,
};

use super::sync_daemon_autostart_setting;

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
