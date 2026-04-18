use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Duration,
};

#[cfg(test)]
use agent_core::AppConnectorConfig;
use agent_core::{
    parse_plugin_provider_id, project_plugin_provider_config, AppConfig, ConversationMessage,
    InstalledPluginConfig, Mission, PluginConnectorManifest, PluginConnectorPollRequest,
    PluginConnectorPollResponse, PluginDoctorReport, PluginInstallRequest, PluginPermissions,
    PluginProviderAdapterManifest, PluginProviderAdapterRequest, PluginProviderAdapterResponse,
    PluginStateUpdateRequest, PluginToolManifest, PluginUpdateRequest, ProviderConfig,
    ProviderHealth, ProviderReply, ThinkingLevel, ToolDefinition, PLUGIN_HOST_VERSION,
};
use agent_storage::plugins as storage_plugins;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use tokio::{process::Command, time::timeout};

use crate::{append_log, ApiError, AppState};

#[derive(Debug, Clone)]
pub(crate) struct HostedPluginTool {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub install_dir: PathBuf,
    pub command: String,
    pub args: Vec<String>,
    pub tool_name: String,
    pub description: String,
    pub input_schema_json: String,
    pub cwd: Option<PathBuf>,
    pub permissions: PluginPermissions,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct HostedPluginConnector {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub install_dir: PathBuf,
    pub command: String,
    pub args: Vec<String>,
    pub connector_id: String,
    pub connector_kind: agent_core::ConnectorKind,
    pub description: String,
    pub cwd: Option<PathBuf>,
    pub permissions: PluginPermissions,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct HostedPluginProviderAdapter {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub install_dir: PathBuf,
    pub command: String,
    pub args: Vec<String>,
    pub adapter_id: String,
    pub description: String,
    pub provider: ProviderConfig,
    pub cwd: Option<PathBuf>,
    pub permissions: PluginPermissions,
    pub timeout_seconds: Option<u64>,
}

pub(crate) async fn list_plugins(
    State(state): State<AppState>,
) -> Result<Json<Vec<InstalledPluginConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.plugins.clone()))
}

pub(crate) async fn get_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<InstalledPluginConfig>, ApiError> {
    let config = state.config.read().await;
    let plugin = config
        .get_plugin(&plugin_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown plugin"))?;
    Ok(Json(plugin))
}

pub(crate) async fn install_plugin(
    State(state): State<AppState>,
    Json(payload): Json<PluginInstallRequest>,
) -> Result<Json<InstalledPluginConfig>, ApiError> {
    let resolved = storage_plugins::resolve_plugin_install_request(state.storage.paths(), &payload)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    let existing = {
        let config = state.config.read().await;
        config.get_plugin(&resolved.manifest.id).cloned()
    };
    let installed =
        storage_plugins::install_plugin_package(state.storage.paths(), &payload, existing.as_ref())
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;

    {
        let mut config = state.config.write().await;
        config.upsert_plugin(installed.clone());
        state.storage.save_config(&config)?;
    }

    append_log(
        &state,
        "info",
        "plugins",
        format!(
            "plugin '{}' installed from '{}'",
            installed.id, installed.source_reference
        ),
    )?;
    Ok(Json(installed))
}

pub(crate) async fn update_plugin_state(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(payload): Json<PluginStateUpdateRequest>,
) -> Result<Json<InstalledPluginConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        let plugin = config
            .plugins
            .iter_mut()
            .find(|plugin| plugin.id == plugin_id)
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown plugin"))?;
        if let Some(enabled) = payload.enabled {
            plugin.enabled = enabled;
        }
        if let Some(granted_permissions) = payload.granted_permissions.as_ref() {
            if !plugin.trusted && payload.trusted != Some(true) {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "permission grants require an explicit trust review",
                ));
            }
            plugin.granted_permissions =
                granted_permissions.intersection(&plugin.declared_permissions());
        }
        if let Some(trusted) = payload.trusted {
            plugin.trusted = trusted;
            if trusted {
                plugin.reviewed_integrity_sha256 = plugin.integrity_sha256.clone();
                plugin.reviewed_at = Some(chrono::Utc::now());
                plugin.granted_permissions = plugin
                    .granted_permissions
                    .intersection(&plugin.declared_permissions());
            } else {
                plugin.granted_permissions = PluginPermissions::default();
                plugin.reviewed_integrity_sha256.clear();
                plugin.reviewed_at = None;
            }
        }
        if let Some(pinned) = payload.pinned {
            plugin.pinned = pinned;
        }
        if payload.granted_permissions.is_some() && plugin.trusted {
            plugin.reviewed_integrity_sha256 = plugin.integrity_sha256.clone();
            plugin.reviewed_at = Some(chrono::Utc::now());
        }
        plugin.updated_at = chrono::Utc::now();
        let plugin = plugin.clone();
        state.storage.save_config(&config)?;
        plugin
    };

    append_log(
        &state,
        "info",
        "plugins",
        format!(
            "plugin '{}' state updated (enabled={}, trusted={}, pinned={}, grants=shell:{} network:{} full_disk:{})",
            updated.id,
            updated.enabled,
            updated.trusted,
            updated.pinned,
            updated.granted_permissions.shell,
            updated.granted_permissions.network,
            updated.granted_permissions.full_disk
        ),
    )?;
    Ok(Json(updated))
}

pub(crate) async fn update_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
    Json(payload): Json<PluginUpdateRequest>,
) -> Result<Json<InstalledPluginConfig>, ApiError> {
    let existing = {
        let config = state.config.read().await;
        config
            .get_plugin(&plugin_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown plugin"))?
    };
    let updated =
        storage_plugins::update_plugin_package(state.storage.paths(), &existing, &payload)
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;

    {
        let mut config = state.config.write().await;
        config.upsert_plugin(updated.clone());
        state.storage.save_config(&config)?;
    }

    append_log(
        &state,
        "info",
        "plugins",
        format!(
            "plugin '{}' updated from '{}'",
            updated.id, updated.source_reference
        ),
    )?;
    Ok(Json(updated))
}

pub(crate) async fn delete_plugin(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let plugin = config
            .get_plugin(&plugin_id)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown plugin"))?;
        config.remove_plugin(&plugin_id);
        state.storage.save_config(&config)?;
        plugin
    };

    storage_plugins::uninstall_plugin_package(&removed)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
    append_log(
        &state,
        "info",
        "plugins",
        format!("plugin '{}' removed", plugin_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_plugin_doctor_reports(
    State(state): State<AppState>,
) -> Result<Json<Vec<PluginDoctorReport>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(collect_plugin_doctor_reports(&config)))
}

pub(crate) async fn get_plugin_doctor_report(
    State(state): State<AppState>,
    Path(plugin_id): Path<String>,
) -> Result<Json<PluginDoctorReport>, ApiError> {
    let config = state.config.read().await;
    let report = collect_plugin_doctor_reports(&config)
        .into_iter()
        .find(|report| report.id == plugin_id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown plugin"))?;
    Ok(Json(report))
}

pub(crate) fn collect_plugin_doctor_reports(config: &AppConfig) -> Vec<PluginDoctorReport> {
    let conflicts = plugin_tool_conflicts(config);
    let mut reports = config
        .plugins
        .iter()
        .map(storage_plugins::doctor_plugin)
        .collect::<Vec<_>>();

    for report in &mut reports {
        let Some(plugin) = config.get_plugin(&report.id) else {
            continue;
        };
        let declared_permissions = plugin.declared_permissions();
        for tool in &plugin.manifest.tools {
            if let Some(owners) = conflicts.get(&tool.name) {
                append_report_detail(
                    report,
                    format!("tool '{}' conflicts with {}", tool.name, owners.join(", ")),
                );
                report.ok = false;
            }
        }
        if declared_permissions.shell && !config.trust_policy.allow_shell {
            append_report_detail(
                report,
                "host trust policy currently blocks plugin shell access".to_string(),
            );
            report.ok = false;
            report.runtime_ready = false;
        }
        if declared_permissions.network && !config.trust_policy.allow_network {
            append_report_detail(
                report,
                "host trust policy currently blocks plugin network access".to_string(),
            );
            report.ok = false;
            report.runtime_ready = false;
        }
        if declared_permissions.full_disk && !config.trust_policy.allow_full_disk {
            append_report_detail(
                report,
                "host trust policy currently blocks plugin full-disk access".to_string(),
            );
            report.ok = false;
            report.runtime_ready = false;
        }
    }

    reports
}

pub(crate) fn collect_hosted_plugin_tools(config: &AppConfig) -> Vec<HostedPluginTool> {
    let conflicts = plugin_tool_conflicts(config);
    let mut seen_tool_names = config
        .mcp_servers
        .iter()
        .map(|server| server.tool_name.clone())
        .chain(
            config
                .app_connectors
                .iter()
                .map(|connector| connector.tool_name.clone()),
        )
        .collect::<BTreeSet<_>>();
    let mut tools = Vec::new();

    for plugin in config
        .plugins
        .iter()
        .filter(|plugin| plugin.runtime_projection_ready())
    {
        for tool in &plugin.manifest.tools {
            let permissions =
                capability_permissions(&plugin.manifest.permissions, &tool.permissions);
            if conflicts.contains_key(&tool.name)
                || !seen_tool_names.insert(tool.name.clone())
                || !plugin.permissions_granted(&permissions)
            {
                continue;
            }
            tools.push(build_hosted_plugin_tool(plugin, tool));
        }
    }

    tools
}

pub(crate) fn collect_hosted_plugin_connectors(config: &AppConfig) -> Vec<HostedPluginConnector> {
    config
        .plugins
        .iter()
        .filter(|plugin| plugin.runtime_projection_ready())
        .flat_map(|plugin| {
            plugin
                .manifest
                .connectors
                .iter()
                .filter(|connector| {
                    let permissions = capability_permissions(
                        &plugin.manifest.permissions,
                        &connector.permissions,
                    );
                    plugin.permissions_granted(&permissions)
                })
                .map(|connector| build_hosted_plugin_connector(plugin, connector))
        })
        .collect()
}

pub(crate) fn collect_hosted_plugin_provider_adapters(
    config: &AppConfig,
) -> Vec<HostedPluginProviderAdapter> {
    config
        .plugins
        .iter()
        .filter(|plugin| plugin.runtime_projection_ready())
        .flat_map(|plugin| {
            plugin
                .manifest
                .provider_adapters
                .iter()
                .filter(|adapter| {
                    let permissions =
                        capability_permissions(&plugin.manifest.permissions, &adapter.permissions);
                    plugin.permissions_granted(&permissions)
                })
                .map(|adapter| build_hosted_plugin_provider_adapter(plugin, adapter))
        })
        .collect()
}

pub(crate) fn resolve_hosted_plugin_provider_adapter(
    config: &AppConfig,
    provider_id: &str,
) -> Option<HostedPluginProviderAdapter> {
    let (plugin_id, adapter_id) = parse_plugin_provider_id(provider_id)?;
    collect_hosted_plugin_provider_adapters(config)
        .into_iter()
        .find(|adapter| adapter.plugin_id == plugin_id && adapter.adapter_id == adapter_id)
}

pub(crate) async fn poll_hosted_plugin_connectors(state: &AppState) -> Result<usize, ApiError> {
    let config = state.config.read().await.clone();
    let connectors = collect_hosted_plugin_connectors(&config);
    let mut queued = 0usize;

    for connector in connectors {
        ensure_host_allows_plugin_permissions(
            &config.trust_policy,
            &connector.permissions,
            &format!(
                "plugin '{}' connector '{}'",
                connector.plugin_id, connector.connector_id
            ),
        )?;
        let response = run_hosted_plugin_connector(&connector).await?;
        if !response.ok {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!(
                    "plugin '{}' connector '{}' failed: {}",
                    connector.plugin_id, connector.connector_id, response.detail
                ),
            ));
        }

        for mission in response.missions {
            let mut queued_mission = Mission::new(mission.title.clone(), mission.prompt.clone());
            queued_mission.alias = mission.alias.clone();
            queued_mission.requested_model = mission.requested_model.clone();
            if let Some(cwd) = mission.cwd.as_ref() {
                let resolved = storage_plugins::resolve_plugin_path(&connector.install_dir, cwd);
                queued_mission.workspace_key = Some(resolved.display().to_string());
            }
            state.storage.upsert_mission(&queued_mission)?;
            append_log(
                state,
                "info",
                "plugins",
                format!(
                    "plugin connector '{}' queued mission '{}' ({})",
                    connector.connector_id, queued_mission.title, queued_mission.id
                ),
            )?;
            queued += 1;
        }
    }

    Ok(queued)
}

pub(crate) async fn plugin_provider_models(
    config: &AppConfig,
    provider_id: &str,
) -> Option<Result<Vec<String>, ApiError>> {
    let adapter = resolve_hosted_plugin_provider_adapter(config, provider_id)?;
    if let Err(error) = ensure_host_allows_plugin_permissions(
        &config.trust_policy,
        &adapter.permissions,
        &format!(
            "plugin '{}' provider adapter '{}'",
            adapter.plugin_id, adapter.adapter_id
        ),
    ) {
        return Some(Err(error));
    }
    Some(run_hosted_plugin_provider_list_models(&adapter).await)
}

pub(crate) async fn plugin_provider_prompt(
    config: &AppConfig,
    provider_id: &str,
    messages: &[ConversationMessage],
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Option<Result<ProviderReply, ApiError>> {
    let adapter = resolve_hosted_plugin_provider_adapter(config, provider_id)?;
    if let Err(error) = ensure_host_allows_plugin_permissions(
        &config.trust_policy,
        &adapter.permissions,
        &format!(
            "plugin '{}' provider adapter '{}'",
            adapter.plugin_id, adapter.adapter_id
        ),
    ) {
        return Some(Err(error));
    }
    Some(
        run_hosted_plugin_provider_prompt(
            &adapter,
            messages,
            requested_model,
            session_id,
            thinking_level,
            tools,
        )
        .await,
    )
}

pub(crate) async fn provider_health(state: &AppState, provider: &ProviderConfig) -> ProviderHealth {
    let config = state.config.read().await.clone();
    if let Some(result) = plugin_provider_models(&config, &provider.id).await {
        return match result {
            Ok(models) => validate_provider_health(provider, &models),
            Err(error) => ProviderHealth {
                id: provider.id.clone(),
                ok: false,
                detail: error.message,
            },
        };
    }

    agent_providers::health_check(&state.http_client, provider).await
}

fn validate_provider_health(provider: &ProviderConfig, models: &[String]) -> ProviderHealth {
    if let Some(default_model) = provider.default_model.as_deref() {
        if !models.iter().any(|model| model == default_model) {
            return ProviderHealth {
                id: provider.id.clone(),
                ok: false,
                detail: format!(
                    "default model '{}' was not returned by provider",
                    default_model
                ),
            };
        }
    }

    ProviderHealth {
        id: provider.id.clone(),
        ok: true,
        detail: format!("{} model(s) reachable", models.len()),
    }
}

#[cfg(test)]
pub(crate) fn project_plugin_tools(config: &AppConfig) -> Vec<AppConnectorConfig> {
    collect_hosted_plugin_tools(config)
        .into_iter()
        .map(|tool| AppConnectorConfig {
            id: format!(
                "plugin-{}-{}",
                slugify_identifier(&tool.plugin_id),
                slugify_identifier(&tool.tool_name)
            ),
            name: format!("{} / {}", tool.plugin_name, tool.tool_name),
            description: tool.description,
            command: tool.command,
            args: tool.args,
            tool_name: tool.tool_name,
            input_schema_json: tool.input_schema_json,
            enabled: true,
            cwd: tool.cwd,
        })
        .collect()
}

fn capability_permissions(
    manifest_permissions: &PluginPermissions,
    capability_permissions: &PluginPermissions,
) -> PluginPermissions {
    manifest_permissions.union(capability_permissions)
}

fn ensure_host_allows_plugin_permissions(
    trust_policy: &agent_core::TrustPolicy,
    permissions: &PluginPermissions,
    label: &str,
) -> Result<(), ApiError> {
    if permissions.shell && !trust_policy.allow_shell {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("{label} requires shell permission, but shell access is disabled"),
        ));
    }
    if permissions.network && !trust_policy.allow_network {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("{label} requires network permission, but network access is disabled"),
        ));
    }
    if permissions.full_disk && !trust_policy.allow_full_disk {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!("{label} requires full-disk permission, but full-disk access is disabled"),
        ));
    }
    Ok(())
}

fn build_hosted_plugin_tool(
    plugin: &InstalledPluginConfig,
    tool: &PluginToolManifest,
) -> HostedPluginTool {
    HostedPluginTool {
        plugin_id: plugin.id.clone(),
        plugin_name: plugin.manifest.name.clone(),
        plugin_version: plugin.manifest.version.clone(),
        install_dir: plugin.install_dir.clone(),
        command: storage_plugins::resolve_plugin_command(&plugin.install_dir, &tool.command),
        args: tool.args.clone(),
        tool_name: tool.name.clone(),
        description: format!(
            "{} [plugin {} {}]",
            tool.description, plugin.manifest.name, plugin.manifest.version
        ),
        input_schema_json: tool.input_schema_json.clone(),
        cwd: tool
            .cwd
            .as_ref()
            .map(|cwd| storage_plugins::resolve_plugin_path(&plugin.install_dir, cwd))
            .or_else(|| Some(plugin.install_dir.clone())),
        permissions: capability_permissions(&plugin.manifest.permissions, &tool.permissions),
        timeout_seconds: tool.timeout_seconds,
    }
}

fn build_hosted_plugin_connector(
    plugin: &InstalledPluginConfig,
    connector: &PluginConnectorManifest,
) -> HostedPluginConnector {
    HostedPluginConnector {
        plugin_id: plugin.id.clone(),
        plugin_name: plugin.manifest.name.clone(),
        plugin_version: plugin.manifest.version.clone(),
        install_dir: plugin.install_dir.clone(),
        command: storage_plugins::resolve_plugin_command(&plugin.install_dir, &connector.command),
        args: connector.args.clone(),
        connector_id: connector.id.clone(),
        connector_kind: connector.kind,
        description: connector.description.clone(),
        cwd: connector
            .cwd
            .as_ref()
            .map(|cwd| storage_plugins::resolve_plugin_path(&plugin.install_dir, cwd))
            .or_else(|| Some(plugin.install_dir.clone())),
        permissions: capability_permissions(&plugin.manifest.permissions, &connector.permissions),
        timeout_seconds: connector.timeout_seconds,
    }
}

fn build_hosted_plugin_provider_adapter(
    plugin: &InstalledPluginConfig,
    adapter: &PluginProviderAdapterManifest,
) -> HostedPluginProviderAdapter {
    HostedPluginProviderAdapter {
        plugin_id: plugin.id.clone(),
        plugin_name: plugin.manifest.name.clone(),
        plugin_version: plugin.manifest.version.clone(),
        install_dir: plugin.install_dir.clone(),
        command: storage_plugins::resolve_plugin_command(&plugin.install_dir, &adapter.command),
        args: adapter.args.clone(),
        adapter_id: adapter.id.clone(),
        description: adapter.description.clone(),
        provider: project_plugin_provider_config(plugin, adapter),
        cwd: adapter
            .cwd
            .as_ref()
            .map(|cwd| storage_plugins::resolve_plugin_path(&plugin.install_dir, cwd))
            .or_else(|| Some(plugin.install_dir.clone())),
        permissions: capability_permissions(&plugin.manifest.permissions, &adapter.permissions),
        timeout_seconds: adapter.timeout_seconds,
    }
}

async fn run_hosted_plugin_connector(
    connector: &HostedPluginConnector,
) -> Result<PluginConnectorPollResponse, ApiError> {
    let request = PluginConnectorPollRequest {
        host_version: PLUGIN_HOST_VERSION,
        plugin_id: connector.plugin_id.clone(),
        plugin_name: connector.plugin_name.clone(),
        plugin_version: connector.plugin_version.clone(),
        connector_id: connector.connector_id.clone(),
        connector_kind: connector.connector_kind,
    };
    let output = run_hosted_plugin_process(
        &connector.command,
        &connector.args,
        connector
            .cwd
            .as_deref()
            .unwrap_or(connector.install_dir.as_path()),
        connector.timeout_seconds,
        &request,
        |process| {
            process.env("AGENT_PLUGIN_ID", &connector.plugin_id);
            process.env("AGENT_PLUGIN_NAME", &connector.plugin_name);
            process.env("AGENT_PLUGIN_VERSION", &connector.plugin_version);
            process.env("AGENT_PLUGIN_CONNECTOR_ID", &connector.connector_id);
            process.env("AGENT_PLUGIN_CONNECTOR_DESCRIPTION", &connector.description);
        },
    )
    .await?;
    parse_plugin_json_response(
        output,
        |stdout| serde_json::from_str::<PluginConnectorPollResponse>(stdout.trim()),
        format!(
            "plugin '{}' connector '{}'",
            connector.plugin_id, connector.connector_id
        ),
    )
}

async fn run_hosted_plugin_provider_list_models(
    adapter: &HostedPluginProviderAdapter,
) -> Result<Vec<String>, ApiError> {
    let request = PluginProviderAdapterRequest::ListModels {
        host_version: PLUGIN_HOST_VERSION,
        plugin_id: adapter.plugin_id.clone(),
        plugin_name: adapter.plugin_name.clone(),
        plugin_version: adapter.plugin_version.clone(),
        adapter_id: adapter.adapter_id.clone(),
        provider_kind: adapter.provider.kind.clone(),
    };
    let output = run_hosted_plugin_process(
        &adapter.command,
        &adapter.args,
        adapter
            .cwd
            .as_deref()
            .unwrap_or(adapter.install_dir.as_path()),
        adapter.timeout_seconds,
        &request,
        |process| {
            process.env("AGENT_PLUGIN_ID", &adapter.plugin_id);
            process.env("AGENT_PLUGIN_NAME", &adapter.plugin_name);
            process.env("AGENT_PLUGIN_VERSION", &adapter.plugin_version);
            process.env("AGENT_PLUGIN_PROVIDER_ID", &adapter.provider.id);
            process.env("AGENT_PLUGIN_PROVIDER_ADAPTER_ID", &adapter.adapter_id);
            process.env("AGENT_PLUGIN_PROVIDER_DESCRIPTION", &adapter.description);
            process.env(
                "AGENT_PLUGIN_PROVIDER_KIND",
                format!("{:?}", adapter.provider.kind),
            );
        },
    )
    .await?;
    match parse_plugin_json_response(
        output,
        |stdout| serde_json::from_str::<PluginProviderAdapterResponse>(stdout.trim()),
        format!(
            "plugin '{}' provider adapter '{}'",
            adapter.plugin_id, adapter.adapter_id
        ),
    )? {
        PluginProviderAdapterResponse::ListModels { ok, models, detail } => {
            if ok {
                Ok(models)
            } else {
                Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "plugin '{}' provider adapter '{}' failed to list models: {}",
                        adapter.plugin_id, adapter.adapter_id, detail
                    ),
                ))
            }
        }
        PluginProviderAdapterResponse::RunPrompt { .. } => Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "plugin '{}' provider adapter '{}' returned the wrong response kind",
                adapter.plugin_id, adapter.adapter_id
            ),
        )),
    }
}

async fn run_hosted_plugin_provider_prompt(
    adapter: &HostedPluginProviderAdapter,
    messages: &[ConversationMessage],
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply, ApiError> {
    let request = PluginProviderAdapterRequest::RunPrompt {
        host_version: PLUGIN_HOST_VERSION,
        plugin_id: adapter.plugin_id.clone(),
        plugin_name: adapter.plugin_name.clone(),
        plugin_version: adapter.plugin_version.clone(),
        adapter_id: adapter.adapter_id.clone(),
        provider_kind: adapter.provider.kind.clone(),
        requested_model: requested_model.map(ToOwned::to_owned),
        session_id: session_id.map(ToOwned::to_owned),
        thinking_level,
        messages: messages.to_vec(),
        tools: tools.to_vec(),
    };
    let output = run_hosted_plugin_process(
        &adapter.command,
        &adapter.args,
        adapter
            .cwd
            .as_deref()
            .unwrap_or(adapter.install_dir.as_path()),
        adapter.timeout_seconds,
        &request,
        |process| {
            process.env("AGENT_PLUGIN_ID", &adapter.plugin_id);
            process.env("AGENT_PLUGIN_NAME", &adapter.plugin_name);
            process.env("AGENT_PLUGIN_VERSION", &adapter.plugin_version);
            process.env("AGENT_PLUGIN_PROVIDER_ID", &adapter.provider.id);
            process.env("AGENT_PLUGIN_PROVIDER_ADAPTER_ID", &adapter.adapter_id);
            process.env("AGENT_PLUGIN_PROVIDER_DESCRIPTION", &adapter.description);
            process.env(
                "AGENT_PLUGIN_PROVIDER_KIND",
                format!("{:?}", adapter.provider.kind),
            );
        },
    )
    .await?;
    match parse_plugin_json_response(
        output,
        |stdout| serde_json::from_str::<PluginProviderAdapterResponse>(stdout.trim()),
        format!(
            "plugin '{}' provider adapter '{}'",
            adapter.plugin_id, adapter.adapter_id
        ),
    )? {
        PluginProviderAdapterResponse::RunPrompt { ok, detail, reply } => {
            if !ok {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "plugin '{}' provider adapter '{}' failed to run prompt: {}",
                        adapter.plugin_id, adapter.adapter_id, detail
                    ),
                ));
            }
            let mut reply = reply.ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "plugin '{}' provider adapter '{}' returned no reply",
                        adapter.plugin_id, adapter.adapter_id
                    ),
                )
            })?;
            reply.provider_id = adapter.provider.id.clone();
            if reply.model.trim().is_empty() {
                return Err(ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    format!(
                        "plugin '{}' provider adapter '{}' returned an empty model",
                        adapter.plugin_id, adapter.adapter_id
                    ),
                ));
            }
            Ok(reply)
        }
        PluginProviderAdapterResponse::ListModels { .. } => Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "plugin '{}' provider adapter '{}' returned the wrong response kind",
                adapter.plugin_id, adapter.adapter_id
            ),
        )),
    }
}

struct HostedPluginProcessOutput {
    stdout: String,
    stderr: String,
    status: std::process::ExitStatus,
}

async fn run_hosted_plugin_process<T, F>(
    command: &str,
    args: &[String],
    cwd: &std::path::Path,
    timeout_seconds: Option<u64>,
    request: &T,
    configure: F,
) -> Result<HostedPluginProcessOutput, ApiError>
where
    T: serde::Serialize,
    F: FnOnce(&mut Command),
{
    let mut process = Command::new(command);
    process.kill_on_drop(true);
    process.args(args);
    process.current_dir(cwd);
    process.stdin(std::process::Stdio::piped());
    process.stdout(std::process::Stdio::piped());
    process.stderr(std::process::Stdio::piped());
    process.env("AGENT_PLUGIN_HOST_VERSION", PLUGIN_HOST_VERSION.to_string());
    configure(&mut process);

    let mut child = process.spawn().map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "failed to start hosted plugin process '{}': {error}",
                command
            ),
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt as _;
        let body = serde_json::to_vec(request).map_err(ApiError::from)?;
        stdin.write_all(&body).await.map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!(
                    "failed to write hosted plugin request to '{}': {error}",
                    command
                ),
            )
        })?;
    }

    let timeout_seconds = timeout_seconds.unwrap_or(60).clamp(1, 600);
    let output = timeout(
        Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| ApiError::new(StatusCode::BAD_GATEWAY, "hosted plugin process timed out"))?
    .map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "failed while waiting for hosted plugin process '{}': {error}",
                command
            ),
        )
    })?;

    Ok(HostedPluginProcessOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status: output.status,
    })
}

fn parse_plugin_json_response<T, F>(
    output: HostedPluginProcessOutput,
    parse: F,
    label: String,
) -> Result<T, ApiError>
where
    F: FnOnce(&str) -> Result<T, serde_json::Error>,
{
    match parse(output.stdout.trim()) {
        Ok(response) if output.status.success() => Ok(response),
        Ok(_) => Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "{} exited unsuccessfully: {}",
                label,
                format_process_output(&output)
            ),
        )),
        Err(_) => Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "{} returned invalid JSON: {}",
                label,
                format_process_output(&output)
            ),
        )),
    }
}

fn format_process_output(output: &HostedPluginProcessOutput) -> String {
    let mut text = String::new();
    if !output.stdout.trim().is_empty() {
        text.push_str(output.stdout.trim());
    }
    if !output.stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(output.stderr.trim());
    }
    if text.is_empty() {
        output.status.to_string()
    } else {
        text
    }
}

fn append_report_detail(report: &mut PluginDoctorReport, detail: String) {
    if report.detail == "ready" {
        report.detail = detail;
    } else {
        report.detail = format!("{}; {}", report.detail, detail);
    }
}

fn plugin_tool_conflicts(config: &AppConfig) -> BTreeMap<String, Vec<String>> {
    let mut owners = BTreeMap::<String, Vec<String>>::new();

    for server in &config.mcp_servers {
        owners
            .entry(server.tool_name.clone())
            .or_default()
            .push(format!("mcp:{}", server.id));
    }
    for connector in &config.app_connectors {
        owners
            .entry(connector.tool_name.clone())
            .or_default()
            .push(format!("app:{}", connector.id));
    }
    for plugin in &config.plugins {
        for tool in &plugin.manifest.tools {
            owners
                .entry(tool.name.clone())
                .or_default()
                .push(format!("plugin:{}:{}", plugin.id, tool.name));
        }
    }

    owners.retain(|_, entries| entries.len() > 1);
    owners
}

#[cfg(test)]
fn slugify_identifier(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch.to_ascii_lowercase(),
            _ => '-',
        })
        .collect::<String>();
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use agent_core::{
        project_plugin_provider_config, AppConfig, ConnectorKind, ConversationMessage, ModelAlias,
        PluginCompatibility, PluginConnectorManifest, PluginManifest, PluginPermissions,
        PluginProviderAdapterManifest, PluginSourceKind, PluginToolManifest, ProviderKind,
    };
    use agent_storage::Storage;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, ProviderRateLimiter,
    };

    use super::*;

    fn sample_plugin(
        id: &str,
        tool_name: &str,
        enabled: bool,
        trusted: bool,
    ) -> InstalledPluginConfig {
        InstalledPluginConfig {
            id: id.to_string(),
            manifest: PluginManifest {
                schema_version: agent_core::PLUGIN_SCHEMA_VERSION,
                id: id.to_string(),
                name: format!("{id} plugin"),
                version: "0.8.1".to_string(),
                description: "test".to_string(),
                homepage: None,
                compatibility: PluginCompatibility::default(),
                permissions: PluginPermissions::default(),
                tools: vec![PluginToolManifest {
                    name: tool_name.to_string(),
                    description: "tool".to_string(),
                    command: "python".to_string(),
                    args: vec!["tool.py".to_string()],
                    input_schema_json: "{\"type\":\"object\"}".to_string(),
                    cwd: Some(PathBuf::from("bin")),
                    permissions: PluginPermissions::default(),
                    timeout_seconds: Some(30),
                }],
                connectors: vec![PluginConnectorManifest {
                    id: format!("{id}-connector"),
                    kind: ConnectorKind::Webhook,
                    description: "future".to_string(),
                    command: "plugin-host".to_string(),
                    args: Vec::new(),
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    timeout_seconds: None,
                }],
                provider_adapters: vec![PluginProviderAdapterManifest {
                    id: format!("{id}-provider"),
                    provider_kind: ProviderKind::OpenAiCompatible,
                    description: "future".to_string(),
                    command: "plugin-host".to_string(),
                    args: Vec::new(),
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    default_model: None,
                    timeout_seconds: None,
                }],
            },
            source_kind: PluginSourceKind::LocalPath,
            install_dir: std::env::temp_dir().join(format!("plugin-{}", Uuid::new_v4())),
            source_reference: "plugin-source".to_string(),
            source_path: PathBuf::from("plugin-source"),
            integrity_sha256: "test-integrity".to_string(),
            enabled,
            trusted,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: if trusted {
                "test-integrity".to_string()
            } else {
                String::new()
            },
            reviewed_at: trusted.then(chrono::Utc::now),
            pinned: false,
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn runtime_plugin(
        root: &std::path::Path,
        provider_command: String,
        provider_args: Vec<String>,
        connector_command: String,
        connector_args: Vec<String>,
    ) -> InstalledPluginConfig {
        InstalledPluginConfig {
            id: "echo-toolkit".to_string(),
            manifest: PluginManifest {
                schema_version: agent_core::PLUGIN_SCHEMA_VERSION,
                id: "echo-toolkit".to_string(),
                name: "Echo Toolkit".to_string(),
                version: "0.8.1".to_string(),
                description: "runtime test".to_string(),
                homepage: None,
                compatibility: PluginCompatibility::default(),
                permissions: PluginPermissions::default(),
                tools: Vec::new(),
                connectors: vec![PluginConnectorManifest {
                    id: "echo-connector".to_string(),
                    kind: ConnectorKind::Webhook,
                    description: "runtime connector".to_string(),
                    command: connector_command,
                    args: connector_args,
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    timeout_seconds: Some(5),
                }],
                provider_adapters: vec![PluginProviderAdapterManifest {
                    id: "echo-provider".to_string(),
                    provider_kind: ProviderKind::OpenAiCompatible,
                    description: "runtime provider".to_string(),
                    command: provider_command,
                    args: provider_args,
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    default_model: Some("plugin-model".to_string()),
                    timeout_seconds: Some(5),
                }],
            },
            source_kind: PluginSourceKind::LocalPath,
            install_dir: root.to_path_buf(),
            source_reference: root.display().to_string(),
            source_path: root.to_path_buf(),
            integrity_sha256: "runtime-test".to_string(),
            enabled: true,
            trusted: true,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: "runtime-test".to_string(),
            reviewed_at: Some(chrono::Utc::now()),
            pinned: false,
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn temp_plugin_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("agent-plugin-runtime-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn provider_protocol_script(root: &std::path::Path) -> (String, Vec<String>) {
        if cfg!(windows) {
            let script = root.join("provider.ps1");
            fs::write(
                &script,
                "$payload = [Console]::In.ReadToEnd()\nif ($payload -like '*\"action\":\"list_models\"*') { [Console]::Out.Write('{\"action\":\"list_models\",\"ok\":true,\"models\":[\"plugin-model\"]}') } else { [Console]::Out.Write('{\"action\":\"run_prompt\",\"ok\":true,\"reply\":{\"provider_id\":\"ignored\",\"model\":\"plugin-model\",\"content\":\"plugin-reply\",\"tool_calls\":[]}}') }\n",
            )
            .unwrap();
            (
                "powershell".to_string(),
                vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script.display().to_string(),
                ],
            )
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let script = root.join("provider.sh");
                fs::write(
                    &script,
                    "#!/bin/sh\npayload=$(cat)\ncase \"$payload\" in\n  *'\"action\":\"list_models\"'*) printf '%s' '{\"action\":\"list_models\",\"ok\":true,\"models\":[\"plugin-model\"]}' ;;\n  *) printf '%s' '{\"action\":\"run_prompt\",\"ok\":true,\"reply\":{\"provider_id\":\"ignored\",\"model\":\"plugin-model\",\"content\":\"plugin-reply\",\"tool_calls\":[]}}' ;;\nesac\n",
                )
                .unwrap();
                let mut permissions = fs::metadata(&script).unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&script, permissions).unwrap();
                ("sh".to_string(), vec![script.display().to_string()])
            }
            #[cfg(not(unix))]
            unreachable!()
        }
    }

    fn connector_protocol_script(root: &std::path::Path) -> (String, Vec<String>) {
        if cfg!(windows) {
            let script = root.join("connector.ps1");
            fs::write(
                &script,
                "$null = [Console]::In.ReadToEnd()\n[Console]::Out.Write('{\"ok\":true,\"detail\":\"\",\"missions\":[{\"title\":\"Plugin Connector Mission\",\"prompt\":\"Handle plugin event\",\"alias\":\"main\"}]}')\n",
            )
            .unwrap();
            (
                "powershell".to_string(),
                vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script.display().to_string(),
                ],
            )
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let script = root.join("connector.sh");
                fs::write(
                    &script,
                    "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"ok\":true,\"detail\":\"\",\"missions\":[{\"title\":\"Plugin Connector Mission\",\"prompt\":\"Handle plugin event\",\"alias\":\"main\"}]}'\n",
                )
                .unwrap();
                let mut permissions = fs::metadata(&script).unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&script, permissions).unwrap();
                ("sh".to_string(), vec![script.display().to_string()])
            }
            #[cfg(not(unix))]
            unreachable!()
        }
    }

    fn test_state(config: AppConfig) -> AppState {
        let root =
            std::env::temp_dir().join(format!("agent-daemon-plugin-test-{}", Uuid::new_v4()));
        let storage = Storage::open_at(&root).unwrap();
        storage.save_config(&config).unwrap();
        let (shutdown_tx, _) = mpsc::unbounded_channel();
        AppState {
            storage,
            config: Arc::new(RwLock::new(config)),
            http_client: reqwest::Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: chrono::Utc::now(),
            shutdown: shutdown_tx,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[test]
    fn project_plugin_tools_skips_untrusted_and_conflicting_tools() {
        let mut config = AppConfig::default();
        config.app_connectors.push(AppConnectorConfig {
            id: "existing".to_string(),
            name: "Existing".to_string(),
            description: "existing".to_string(),
            command: "python".to_string(),
            args: Vec::new(),
            tool_name: "existing_tool".to_string(),
            input_schema_json: "{\"type\":\"object\"}".to_string(),
            enabled: true,
            cwd: None,
        });
        config
            .plugins
            .push(sample_plugin("one", "plugin_tool", true, true));
        config
            .plugins
            .push(sample_plugin("two", "existing_tool", true, true));
        config
            .plugins
            .push(sample_plugin("three", "shadowed_tool", true, false));

        let projected = project_plugin_tools(&config);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].tool_name, "plugin_tool");
    }

    #[test]
    fn project_plugin_tools_skip_capabilities_without_permission_grants() {
        let mut plugin = sample_plugin("secure", "network_tool", true, true);
        plugin.manifest.tools[0].permissions.network = true;
        let mut config = AppConfig::default();
        config.plugins.push(plugin.clone());

        assert!(project_plugin_tools(&config).is_empty());

        plugin.granted_permissions.network = true;
        config.plugins.clear();
        config.plugins.push(plugin);
        let projected = project_plugin_tools(&config);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].tool_name, "network_tool");
    }

    #[tokio::test]
    async fn list_plugin_doctor_reports_marks_tool_conflicts() {
        let mut config = AppConfig::default();
        config
            .plugins
            .push(sample_plugin("one", "dup_tool", true, true));
        config
            .plugins
            .push(sample_plugin("two", "dup_tool", true, true));
        let state = test_state(config);

        let Json(reports) = list_plugin_doctor_reports(State(state)).await.unwrap();
        assert_eq!(reports.len(), 2);
        assert!(reports.iter().all(|report| !report.ok));
        assert!(reports
            .iter()
            .all(|report| report.detail.contains("conflicts")));
    }

    #[tokio::test]
    async fn hosted_plugin_provider_adapter_handles_models_and_prompts() {
        let root = temp_plugin_root();
        let (provider_command, provider_args) = provider_protocol_script(&root);
        let (connector_command, connector_args) = connector_protocol_script(&root);
        let plugin = runtime_plugin(
            &root,
            provider_command,
            provider_args,
            connector_command,
            connector_args,
        );
        let provider =
            project_plugin_provider_config(&plugin, &plugin.manifest.provider_adapters[0]);
        let mut config = AppConfig::default();
        config.plugins.push(plugin);
        config.aliases.push(ModelAlias {
            alias: "main".to_string(),
            provider_id: provider.id.clone(),
            model: "plugin-model".to_string(),
            description: None,
        });
        config.main_agent_alias = Some("main".to_string());

        let models = plugin_provider_models(&config, &provider.id)
            .await
            .expect("plugin provider should resolve")
            .unwrap();
        assert_eq!(models, vec!["plugin-model".to_string()]);

        let reply = plugin_provider_prompt(
            &config,
            &provider.id,
            &[ConversationMessage {
                role: agent_core::MessageRole::User,
                content: "hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
            }],
            Some("plugin-model"),
            Some("session-1"),
            None,
            &[],
        )
        .await
        .expect("plugin provider should resolve")
        .unwrap();

        assert_eq!(reply.provider_id, provider.id);
        assert_eq!(reply.model, "plugin-model");
        assert_eq!(reply.content, "plugin-reply");
    }

    #[tokio::test]
    async fn hosted_plugin_connector_poll_queues_missions() {
        let root = temp_plugin_root();
        let (provider_command, provider_args) = provider_protocol_script(&root);
        let (connector_command, connector_args) = connector_protocol_script(&root);
        let plugin = runtime_plugin(
            &root,
            provider_command,
            provider_args,
            connector_command,
            connector_args,
        );
        let provider =
            project_plugin_provider_config(&plugin, &plugin.manifest.provider_adapters[0]);
        let mut config = AppConfig::default();
        config.plugins.push(plugin);
        config.aliases.push(ModelAlias {
            alias: "main".to_string(),
            provider_id: provider.id,
            model: "plugin-model".to_string(),
            description: None,
        });
        config.main_agent_alias = Some("main".to_string());
        let state = test_state(config);

        let queued = poll_hosted_plugin_connectors(&state).await.unwrap();
        let missions = state.storage.list_missions_limited(Some(10)).unwrap();

        assert_eq!(queued, 1);
        assert_eq!(missions.len(), 1);
        assert_eq!(missions[0].title, "Plugin Connector Mission");
        assert_eq!(missions[0].alias.as_deref(), Some("main"));
    }
}
