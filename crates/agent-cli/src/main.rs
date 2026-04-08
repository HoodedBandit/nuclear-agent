use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::IsTerminal,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

mod connector_cli;
mod connector_support;
mod integrations_cli;
mod interactive_commands;
mod interactive_ui;
mod operations_cli;
mod plugins_cli;
mod provider_auth;
mod repo_cli;
mod session_support;
mod tui;

use connector_cli::{
    discord_command, home_assistant_command, inbox_command, signal_command, skills_command,
    slack_command, telegram_command, webhook_command,
};
pub(crate) use connector_support::{
    format_discord_channel_cursors, format_home_assistant_entity_cursors, format_i64_list,
    format_slack_channel_cursors, format_string_list, hash_webhook_token_local,
};
use futures::StreamExt;
use integrations_cli::{app_command, mcp_command, AppCommands, McpCommands};
#[cfg(test)]
use integrations_cli::{AppAddArgs, McpAddArgs};
use operations_cli::{
    autonomy_command, autopilot_command, dashboard_command, doctor_command, evolve_command,
    logs_command, memory_command, mission_command, session_command, support_bundle_command,
};
use plugins_cli::{plugin_command, PluginCommands};
pub(crate) use repo_cli::{
    build_review_prompt, build_uncommitted_diff, build_uncommitted_review_prompt,
};
use repo_cli::{repo_command, RepoCommands};
pub(crate) use session_support::{
    build_compact_prompt, compact_session, copy_to_clipboard, fork_session,
    format_session_message_for_display, format_session_resume_packet, format_session_search_hits,
    init_agents_file, latest_assistant_output_from_transcript, load_last_assistant_output,
    load_session_for_command, load_session_resume_packet, load_transcript_for_interactive_fork,
    load_transcript_for_interactive_resume, rank_sessions_for_picker, SESSION_PICKER_LIMIT,
};

use agent_core::{
    AliasUpsertRequest, AppConfig, AuthMode, AutonomyEnableRequest, AutonomyMode, AutopilotConfig,
    AutopilotState, AutopilotUpdateRequest, BatchTaskRequest, BatchTaskResponse,
    ConnectorApprovalRecord, ConnectorApprovalStatus, ConnectorApprovalUpdateRequest,
    ConnectorKind, DaemonConfigUpdateRequest, DaemonStatus, DashboardLaunchResponse,
    DiscordConnectorConfig, DiscordConnectorUpsertRequest, DiscordPollResponse, DiscordSendRequest,
    DiscordSendResponse, EvolveConfig, EvolveStartRequest, HealthReport,
    HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest, HomeAssistantEntityState,
    HomeAssistantPollResponse, HomeAssistantServiceCallRequest, HomeAssistantServiceCallResponse,
    InboxConnectorConfig, InboxConnectorUpsertRequest, InboxPollResponse, InputAttachment,
    KeyValuePair, MemoryKind, MemoryRebuildRequest, MemoryRebuildResponse, MemoryRecord,
    MemoryReviewStatus, MemoryReviewUpdateRequest, MemoryScope, MemorySearchQuery,
    MemorySearchResponse, MemoryUpsertRequest, Mission, MissionCheckpoint, MissionControlRequest,
    MissionStatus, ModelAlias, OAuthConfig, OAuthToken, PermissionPreset, PermissionUpdateRequest,
    PersistenceMode, ProviderConfig, ProviderKind, ProviderUpsertRequest, RunTaskRequest,
    RunTaskResponse, SessionTranscript, SignalConnectorConfig, SignalConnectorUpsertRequest,
    SignalPollResponse, SignalSendRequest, SignalSendResponse, SkillDraft, SkillDraftStatus,
    SkillUpdateRequest, SlackConnectorConfig, SlackConnectorUpsertRequest, SlackPollResponse,
    SlackSendRequest, SlackSendResponse, SubAgentTask, TaskMode, TelegramConnectorConfig,
    TelegramConnectorUpsertRequest, TelegramPollResponse, TelegramSendRequest,
    TelegramSendResponse, ThinkingLevel, TrustPolicy, TrustUpdateRequest, WakeTrigger,
    WebhookConnectorConfig, WebhookConnectorUpsertRequest, WebhookEventRequest,
    WebhookEventResponse, DEFAULT_ANTHROPIC_URL, DEFAULT_CHATGPT_CODEX_URL,
    DEFAULT_LOCAL_OPENAI_URL, DEFAULT_MOONSHOT_URL, DEFAULT_OLLAMA_URL, DEFAULT_OPENAI_URL,
    DEFAULT_OPENROUTER_URL, DEFAULT_VENICE_URL, DISPLAY_APP_NAME, INTERNAL_DAEMON_ARG,
    PRIMARY_COMMAND_NAME,
};
use agent_policy::{
    allow_shell, autonomy_summary, autonomy_warning, permission_summary, trust_summary,
};
use agent_providers::{
    build_oauth_authorization_url, delete_secret, exchange_oauth_code, health_check,
    keyring_available, list_models as provider_list_models,
    list_models_with_overrides as provider_list_models_with_overrides, store_api_key,
    store_oauth_token,
};
use agent_storage::{plugins as storage_plugins, Storage};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
pub(crate) use connector_cli::{
    load_discord_connectors, load_home_assistant_connectors, load_inbox_connectors,
    load_signal_connectors, load_slack_connectors, load_telegram_connectors,
    load_webhook_connectors,
};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
pub(crate) use interactive_commands::{
    parse_interactive_command, InteractiveCommand, InteractiveModelSelection,
    InteractiveSkillCommand,
};
#[cfg(test)]
use interactive_ui::normalize_model_selection_value;
pub(crate) use interactive_ui::{
    clear_terminal, interactive_model_choices_text, interactive_provider_choices_text,
    print_interactive_help, provider_has_saved_access, resolve_interactive_model_selection,
    resolve_interactive_provider_selection, resolve_requested_model_override,
    resolve_session_model_override,
};
pub(crate) use operations_cli::{dashboard_launch_url, dashboard_ui_url};
use provider_auth::*;
pub(crate) use provider_auth::{
    browser_hosted_kind_to_provider_kind, complete_browser_login, complete_oauth_login,
    default_browser_hosted_url, default_hosted_url, hosted_kind_to_provider_kind,
    interactive_provider_setup, openai_browser_oauth_config,
};
use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    process::Command as TokioCommand,
    time::{sleep, timeout},
};
use url::{form_urlencoded, Url};
use uuid::Uuid;

const OAUTH_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_GIT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const OPENAI_BROWSER_AUTH_ISSUER: &str = "https://auth.openai.com";
const OPENAI_BROWSER_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_BROWSER_ORIGINATOR: &str = "codex_cli_rs";
const OPENAI_BROWSER_CALLBACK_PORT: u16 = 1455;
const OPENAI_BROWSER_CALLBACK_PATH: &str = "/auth/callback";
const OPENAI_BROWSER_SUCCESS_PATH: &str = "/success";
const OPENAI_BROWSER_CANCEL_PATH: &str = "/cancel";
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn build_http_client() -> Client {
    Client::builder()
        .timeout(DEFAULT_HTTP_TIMEOUT)
        .build()
        .unwrap_or_else(|_| Client::new())
}

include!("cli/types.rs");

#[tokio::main]
async fn main() -> Result<()> {
    run(Cli::parse()).await
}

include!("cli/runtime.rs");
