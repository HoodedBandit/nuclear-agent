use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AuthMode, ConnectorKind, ConversationMessage, ProviderConfig, ProviderKind, ProviderReply,
    ThinkingLevel, ToolDefinition,
};

pub const PLUGIN_MANIFEST_FILE_NAME: &str = "agent-plugin.json";
pub const PLUGIN_SCHEMA_VERSION: u32 = 1;
pub const PLUGIN_HOST_VERSION: u32 = 1;
pub const PLUGIN_PROVIDER_ID_PREFIX: &str = "plugin.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginSourceKind {
    #[default]
    LocalPath,
    GitRepo,
    Marketplace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginCompatibility {
    #[serde(default)]
    pub min_host_version: Option<u32>,
    #[serde(default)]
    pub max_host_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginPermissions {
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub full_disk: bool,
}

impl PluginPermissions {
    pub fn is_empty(&self) -> bool {
        !self.shell && !self.network && !self.full_disk
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            shell: self.shell || other.shell,
            network: self.network || other.network,
            full_disk: self.full_disk || other.full_disk,
        }
    }

    pub fn intersection(&self, other: &Self) -> Self {
        Self {
            shell: self.shell && other.shell,
            network: self.network && other.network,
            full_disk: self.full_disk && other.full_disk,
        }
    }

    pub fn missing_from(&self, grants: &Self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.shell && !grants.shell {
            missing.push("shell");
        }
        if self.network && !grants.network {
            missing.push("network");
        }
        if self.full_disk && !grants.full_disk {
            missing.push("full_disk");
        }
        missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginToolManifest {
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub input_schema_json: String,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConnectorManifest {
    pub id: String,
    pub kind: ConnectorKind,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginProviderAdapterManifest {
    pub id: String,
    pub provider_kind: ProviderKind,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub compatibility: PluginCompatibility,
    #[serde(default)]
    pub permissions: PluginPermissions,
    #[serde(default)]
    pub tools: Vec<PluginToolManifest>,
    #[serde(default)]
    pub connectors: Vec<PluginConnectorManifest>,
    #[serde(default)]
    pub provider_adapters: Vec<PluginProviderAdapterManifest>,
}

impl PluginManifest {
    pub fn capability_count(&self) -> usize {
        self.tools.len() + self.connectors.len() + self.provider_adapters.len()
    }

    pub fn declared_permissions(&self) -> PluginPermissions {
        let mut permissions = self.permissions.clone();
        for tool in &self.tools {
            permissions = permissions.union(&tool.permissions);
        }
        for connector in &self.connectors {
            permissions = permissions.union(&connector.permissions);
        }
        for adapter in &self.provider_adapters {
            permissions = permissions.union(&adapter.permissions);
        }
        permissions
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPluginConfig {
    pub id: String,
    pub manifest: PluginManifest,
    #[serde(default)]
    pub source_kind: PluginSourceKind,
    pub install_dir: PathBuf,
    #[serde(default)]
    pub source_reference: String,
    pub source_path: PathBuf,
    #[serde(default)]
    pub integrity_sha256: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub granted_permissions: PluginPermissions,
    #[serde(default)]
    pub reviewed_integrity_sha256: String,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pinned: bool,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl InstalledPluginConfig {
    pub fn declared_permissions(&self) -> PluginPermissions {
        self.manifest.declared_permissions()
    }

    pub fn review_current(&self) -> bool {
        self.trusted
            && !self.reviewed_integrity_sha256.trim().is_empty()
            && self.reviewed_integrity_sha256 == self.integrity_sha256
    }

    pub fn runtime_projection_ready(&self) -> bool {
        self.enabled && self.review_current()
    }

    pub fn permissions_granted(&self, required: &PluginPermissions) -> bool {
        required.missing_from(&self.granted_permissions).is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginInstallRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_path: Option<PathBuf>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trusted: Option<bool>,
    #[serde(default)]
    pub granted_permissions: Option<PluginPermissions>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginUpdateRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginStateUpdateRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trusted: Option<bool>,
    #[serde(default)]
    pub granted_permissions: Option<PluginPermissions>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDoctorReport {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub trusted: bool,
    #[serde(default)]
    pub runtime_ready: bool,
    pub ok: bool,
    pub detail: String,
    #[serde(default)]
    pub tools: usize,
    #[serde(default)]
    pub connectors: usize,
    #[serde(default)]
    pub provider_adapters: usize,
    #[serde(default)]
    pub integrity_sha256: String,
    #[serde(default)]
    pub source_kind: PluginSourceKind,
    #[serde(default)]
    pub declared_permissions: PluginPermissions,
    #[serde(default)]
    pub granted_permissions: PluginPermissions,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginToolCallRequest {
    pub host_version: u32,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub tool_name: String,
    pub workspace_cwd: PathBuf,
    pub arguments: Value,
    pub shell_allowed: bool,
    pub network_allowed: bool,
    pub full_disk_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginToolCallResponse {
    pub ok: bool,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PluginProviderAdapterRequest {
    ListModels {
        host_version: u32,
        plugin_id: String,
        plugin_name: String,
        plugin_version: String,
        adapter_id: String,
        provider_kind: ProviderKind,
    },
    RunPrompt {
        host_version: u32,
        plugin_id: String,
        plugin_name: String,
        plugin_version: String,
        adapter_id: String,
        provider_kind: ProviderKind,
        requested_model: Option<String>,
        session_id: Option<String>,
        thinking_level: Option<ThinkingLevel>,
        messages: Vec<ConversationMessage>,
        tools: Vec<ToolDefinition>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PluginProviderAdapterResponse {
    ListModels {
        ok: bool,
        #[serde(default)]
        models: Vec<String>,
        #[serde(default)]
        detail: String,
    },
    RunPrompt {
        ok: bool,
        #[serde(default)]
        detail: String,
        #[serde(default)]
        reply: Option<ProviderReply>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConnectorPollRequest {
    pub host_version: u32,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub connector_id: String,
    pub connector_kind: ConnectorKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConnectorMission {
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginConnectorPollResponse {
    pub ok: bool,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub missions: Vec<PluginConnectorMission>,
}

impl PluginInstallRequest {
    pub fn source_reference(&self) -> Option<String> {
        self.source.clone().or_else(|| {
            self.source_path
                .as_ref()
                .map(|path| path.display().to_string())
        })
    }
}

impl PluginUpdateRequest {
    pub fn source_reference(&self) -> Option<String> {
        self.source.clone().or_else(|| {
            self.source_path
                .as_ref()
                .map(|path| path.display().to_string())
        })
    }
}

pub fn plugin_provider_id(plugin_id: &str, adapter_id: &str) -> String {
    format!("{PLUGIN_PROVIDER_ID_PREFIX}{plugin_id}.{adapter_id}")
}

pub fn parse_plugin_provider_id(provider_id: &str) -> Option<(String, String)> {
    let remainder = provider_id.strip_prefix(PLUGIN_PROVIDER_ID_PREFIX)?;
    let (plugin_id, adapter_id) = remainder.rsplit_once('.')?;
    if plugin_id.trim().is_empty() || adapter_id.trim().is_empty() {
        return None;
    }
    Some((plugin_id.to_string(), adapter_id.to_string()))
}

pub fn project_plugin_provider_config(
    plugin: &InstalledPluginConfig,
    adapter: &PluginProviderAdapterManifest,
) -> ProviderConfig {
    ProviderConfig {
        id: plugin_provider_id(&plugin.id, &adapter.id),
        display_name: format!("{} / {}", plugin.manifest.name, adapter.id),
        kind: adapter.provider_kind.clone(),
        base_url: format!("plugin://{}/{}", plugin.id, adapter.id),
        provider_profile: None,
        auth_mode: AuthMode::None,
        default_model: adapter.default_model.clone(),
        keychain_account: None,
        oauth: None,
        local: true,
    }
}

pub fn projected_plugin_providers(plugins: &[InstalledPluginConfig]) -> Vec<ProviderConfig> {
    plugins
        .iter()
        .filter(|plugin| plugin.runtime_projection_ready())
        .flat_map(|plugin| {
            plugin
                .manifest
                .provider_adapters
                .iter()
                .filter(|adapter| {
                    let required = plugin.manifest.permissions.union(&adapter.permissions);
                    plugin.permissions_granted(&required)
                })
                .map(|adapter| project_plugin_provider_config(plugin, adapter))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_manifest_counts_all_declared_capabilities() {
        let manifest = PluginManifest {
            schema_version: PLUGIN_SCHEMA_VERSION,
            id: "echo".to_string(),
            name: "Echo".to_string(),
            version: "0.8.0".to_string(),
            description: "test".to_string(),
            homepage: None,
            compatibility: PluginCompatibility::default(),
            permissions: PluginPermissions::default(),
            tools: vec![PluginToolManifest {
                name: "echo_tool".to_string(),
                description: "Echo input".to_string(),
                command: "python".to_string(),
                args: vec!["tool.py".to_string()],
                input_schema_json: "{\"type\":\"object\"}".to_string(),
                cwd: None,
                permissions: PluginPermissions::default(),
                timeout_seconds: None,
            }],
            connectors: vec![PluginConnectorManifest {
                id: "echo-connector".to_string(),
                kind: ConnectorKind::Webhook,
                description: "future connector".to_string(),
                command: "plugin-host".to_string(),
                args: Vec::new(),
                cwd: None,
                permissions: PluginPermissions::default(),
                timeout_seconds: None,
            }],
            provider_adapters: vec![PluginProviderAdapterManifest {
                id: "echo-provider".to_string(),
                provider_kind: ProviderKind::OpenAiCompatible,
                description: "future provider".to_string(),
                command: "plugin-host".to_string(),
                args: Vec::new(),
                cwd: None,
                permissions: PluginPermissions::default(),
                default_model: None,
                timeout_seconds: None,
            }],
        };

        assert_eq!(manifest.capability_count(), 3);
        assert!(manifest.declared_permissions().is_empty());
    }

    #[test]
    fn plugin_provider_ids_round_trip() {
        let provider_id = plugin_provider_id("echo-toolkit", "echo-provider");
        assert_eq!(provider_id, "plugin.echo-toolkit.echo-provider");
        assert_eq!(
            parse_plugin_provider_id(&provider_id),
            Some(("echo-toolkit".to_string(), "echo-provider".to_string()))
        );
    }
}
