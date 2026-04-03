#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        plugin_provider_id, AppConfig, AutonomyProfile, BraveConnectorConfig,
        DashboardLaunchResponse, DelegationConfig, DiscordSendResponse, GmailConnectorConfig,
        HomeAssistantServiceCallResponse, InstalledPluginConfig, MainTargetSummary,
        MemorySearchResponse, MessageRole, PermissionPreset, PluginCompatibility,
        PluginManifest, PluginPermissions, PluginProviderAdapterManifest, PluginSourceKind,
        ProviderKind, RunTaskResponse, SessionMessage, SessionResumePacket, SessionSummary,
        SignalSendResponse, SlackSendResponse, DiscordChannelCursor, PLUGIN_SCHEMA_VERSION,
    };
    use clap::Parser;
    use serde::Deserialize;
    use serde_json::json;
    use std::{
        collections::{HashMap, VecDeque},
        sync::{Arc, Mutex},
    };
    use tokio::task::JoinHandle;
    use uuid::Uuid;

    include!("tests/cases_a.rs");

    include!("tests/cases_b.rs");

    include!("tests/cases_c.rs");
}
