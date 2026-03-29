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

#[cfg(test)]
use agent_core::{DiscordChannelCursor, MessageRole, SessionResumePacket};
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
    logs_command, memory_command, mission_command, session_command,
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
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, Select};
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
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
const CLAUDE_BROWSER_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_BROWSER_AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
const CLAUDE_BROWSER_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_BROWSER_API_KEY_URL: &str =
    "https://api.anthropic.com/api/oauth/claude_cli/create_api_key";
const CLAUDE_BROWSER_ROLES_URL: &str = "https://api.anthropic.com/api/oauth/claude_cli/roles";
const CLAUDE_BROWSER_CALLBACK_PORT: u16 = 45454;
const CLAUDE_BROWSER_CALLBACK_PATH: &str = "/callback";
const CLAUDE_BROWSER_SCOPES: &[&str] = &[
    "org:create_api_key",
    "user:profile",
    "user:inference",
    "user:sessions:claude_code",
    "user:mcp_servers",
];
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn build_http_client() -> Client {
    Client::builder()
        .timeout(DEFAULT_HTTP_TIMEOUT)
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum BrowserLoginResult {
    ApiKey(String),
    OAuthToken(OAuthToken),
}

#[derive(Parser)]
#[command(
    name = "nuclear",
    bin_name = "nuclear",
    version,
    about = "Persistent local work agent CLI for Nuclear Agent",
    subcommand_negates_reqs = true,
    override_usage = "nuclear [OPTIONS] [PROMPT]\n       nuclear [OPTIONS] <COMMAND> [ARGS]"
)]
struct Cli {
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    cwd: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a prompt non-interactively.
    #[command(visible_alias = "e")]
    Exec(RunArgs),
    /// Run a code review prompt non-interactively.
    Review(ReviewArgs),
    /// Resume a previous interactive session.
    Resume(ResumeArgs),
    /// Fork a previous interactive session.
    Fork(ForkArgs),
    /// Generate shell completion scripts.
    Completion(CompletionArgs),
    /// Remove stored authentication credentials.
    Logout(LogoutArgs),
    /// Wipe saved state and restart onboarding.
    Reset(ResetArgs),
    Setup,
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    Login(LoginArgs),
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
    App {
        #[command(subcommand)]
        command: AppCommands,
    },
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },
    Telegram {
        #[command(subcommand)]
        command: TelegramCommands,
    },
    Discord {
        #[command(subcommand)]
        command: DiscordCommands,
    },
    Slack {
        #[command(subcommand)]
        command: SlackCommands,
    },
    Signal {
        #[command(subcommand)]
        command: SignalCommands,
    },
    HomeAssistant {
        #[command(subcommand)]
        command: HomeAssistantCommands,
    },
    Webhook {
        #[command(subcommand)]
        command: WebhookCommands,
    },
    Inbox {
        #[command(subcommand)]
        command: InboxCommands,
    },
    Skills {
        #[command(subcommand)]
        command: SkillCommands,
    },
    Model {
        #[command(subcommand)]
        command: ModelCommands,
    },
    Alias {
        #[command(subcommand)]
        command: AliasCommands,
    },
    Permissions(PermissionsArgs),
    Trust(TrustArgs),
    Run(RunArgs),
    Chat(ChatArgs),
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
    Autonomy {
        #[command(subcommand)]
        command: AutonomyCommands,
    },
    Evolve {
        #[command(subcommand)]
        command: EvolveCommands,
    },
    Autopilot {
        #[command(subcommand)]
        command: AutopilotCommands,
    },
    Mission {
        #[command(subcommand)]
        command: MissionCommands,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    Logs {
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        follow: bool,
    },
    Dashboard(DashboardArgs),
    Doctor,
    #[command(name = "__daemon", hide = true)]
    InternalDaemon,
}

#[derive(Subcommand)]
enum DaemonCommands {
    Start,
    Stop,
    Status,
    Config(DaemonConfigArgs),
}

#[derive(Args)]
struct DaemonConfigArgs {
    #[arg(long, value_enum)]
    mode: Option<PersistenceModeArg>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    auto_start: Option<bool>,
}

#[derive(Subcommand)]
enum ProviderCommands {
    Add(ProviderAddArgs),
    AddLocal(LocalProviderAddArgs),
    List,
}

#[derive(Subcommand)]
enum TelegramCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(TelegramAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
    Send(TelegramSendArgs),
    Approvals {
        #[command(subcommand)]
        command: TelegramApprovalCommands,
    },
}

#[derive(Subcommand)]
enum TelegramApprovalCommands {
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Approve {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reject {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum WebhookCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(WebhookAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Deliver(WebhookDeliverArgs),
}

#[derive(Subcommand)]
enum DiscordCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(DiscordAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
    Send(DiscordSendArgs),
    Approvals {
        #[command(subcommand)]
        command: DiscordApprovalCommands,
    },
}

#[derive(Subcommand)]
enum DiscordApprovalCommands {
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Approve {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reject {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum SlackCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(SlackAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
    Send(SlackSendArgs),
    Approvals {
        #[command(subcommand)]
        command: SlackApprovalCommands,
    },
}

#[derive(Subcommand)]
enum SlackApprovalCommands {
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Approve {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reject {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum HomeAssistantCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(HomeAssistantAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
    State {
        id: String,
        #[arg(long = "entity-id")]
        entity_id: String,
    },
    CallService(HomeAssistantServiceArgs),
}

#[derive(Subcommand)]
enum SignalCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(SignalAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
    Send(SignalSendArgs),
    Approvals {
        #[command(subcommand)]
        command: SignalApprovalCommands,
    },
}

#[derive(Subcommand)]
enum SignalApprovalCommands {
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Approve {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reject {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum InboxCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(InboxAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Poll {
        id: String,
    },
}

#[derive(Subcommand)]
enum SkillCommands {
    List,
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Drafts {
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, value_enum)]
        status: Option<SkillDraftStatusArg>,
    },
    Publish {
        id: String,
    },
    Reject {
        id: String,
    },
}

#[derive(Args)]
struct ProviderAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long, value_enum)]
    kind: HostedKindArg,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    model: String,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    main_alias: Option<String>,
}

#[derive(Args)]
struct LocalProviderAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long, value_enum)]
    kind: LocalKindArg,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    main_alias: Option<String>,
}

#[derive(Subcommand)]
enum ModelCommands {
    List {
        #[arg(long)]
        provider: String,
    },
}

#[derive(Subcommand)]
enum AliasCommands {
    Add(AliasAddArgs),
    List,
}

#[derive(Args)]
struct AliasAddArgs {
    #[arg(long)]
    alias: String,
    #[arg(long)]
    provider: String,
    #[arg(long)]
    model: String,
    #[arg(long)]
    description: Option<String>,
    #[arg(long, default_value_t = false)]
    main: bool,
}

#[derive(Args)]
struct TrustArgs {
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    allow_shell: Option<bool>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    allow_network: Option<bool>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    allow_full_disk: Option<bool>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    allow_self_edit: Option<bool>,
}

#[derive(Args)]
struct RunArgs {
    prompt: Option<String>,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long = "task")]
    tasks: Vec<String>,
    #[arg(long, value_enum)]
    thinking: Option<ThinkingLevelArg>,
    #[arg(long, value_enum)]
    mode: Option<TaskModeArg>,
    #[arg(long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
    #[arg(long = "output-schema", value_name = "FILE")]
    output_schema: Option<PathBuf>,
    #[arg(long = "output-last-message", value_name = "FILE")]
    output_last_message: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    json: bool,
    #[arg(long, default_value_t = false)]
    ephemeral: bool,
    #[arg(long, value_enum)]
    permissions: Option<PermissionPresetArg>,
}

#[derive(Args)]
struct ChatArgs {
    #[arg(long)]
    alias: Option<String>,
    #[arg(long, value_enum)]
    thinking: Option<ThinkingLevelArg>,
    #[arg(long, value_enum)]
    mode: Option<TaskModeArg>,
    #[arg(long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
    #[arg(long, value_enum)]
    permissions: Option<PermissionPresetArg>,
    #[arg(long, default_value_t = false)]
    no_tui: bool,
}

#[derive(Args)]
struct PermissionsArgs {
    #[arg(value_enum)]
    preset: Option<PermissionPresetArg>,
}

#[derive(Args)]
struct WebhookAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    prompt_template: Option<String>,
    #[arg(long = "prompt-file")]
    prompt_file: Option<PathBuf>,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long)]
    token: Option<String>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct WebhookDeliverArgs {
    id: String,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long)]
    prompt: Option<String>,
    #[arg(long)]
    details: Option<String>,
    #[arg(long = "payload-file")]
    payload_file: Option<PathBuf>,
    #[arg(long)]
    token: Option<String>,
}

#[derive(Args)]
struct TelegramAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long = "bot-token")]
    bot_token: Option<String>,
    #[arg(long = "chat-id")]
    chat_ids: Vec<i64>,
    #[arg(long = "user-id")]
    user_ids: Vec<i64>,
    #[arg(long, default_value_t = true)]
    require_pairing_approval: bool,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct TelegramSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long = "chat-id")]
    chat_id: i64,
    #[arg(long)]
    text: String,
    #[arg(long, default_value_t = false)]
    disable_notification: bool,
}

#[derive(Args)]
struct DiscordAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long = "bot-token")]
    bot_token: Option<String>,
    #[arg(long = "monitored-channel-id")]
    monitored_channel_ids: Vec<String>,
    #[arg(long = "allowed-channel-id")]
    allowed_channel_ids: Vec<String>,
    #[arg(long = "user-id")]
    user_ids: Vec<String>,
    #[arg(long, default_value_t = true)]
    require_pairing_approval: bool,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct DiscordSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long = "channel-id")]
    channel_id: String,
    #[arg(long)]
    content: String,
}

#[derive(Args)]
struct SlackAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long = "bot-token")]
    bot_token: Option<String>,
    #[arg(long = "monitored-channel-id")]
    monitored_channel_ids: Vec<String>,
    #[arg(long = "allowed-channel-id")]
    allowed_channel_ids: Vec<String>,
    #[arg(long = "user-id")]
    user_ids: Vec<String>,
    #[arg(long, default_value_t = true)]
    require_pairing_approval: bool,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct SlackSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long = "channel-id")]
    channel_id: String,
    #[arg(long)]
    text: String,
}

#[derive(Args)]
struct SignalAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    account: String,
    #[arg(long = "cli-path")]
    cli_path: Option<PathBuf>,
    #[arg(long = "monitored-group-id")]
    monitored_group_ids: Vec<String>,
    #[arg(long = "allowed-group-id")]
    allowed_group_ids: Vec<String>,
    #[arg(long = "user-id")]
    user_ids: Vec<String>,
    #[arg(long, default_value_t = true)]
    require_pairing_approval: bool,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct SignalSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    recipient: Option<String>,
    #[arg(long = "group-id")]
    group_id: Option<String>,
    #[arg(long)]
    text: String,
}

#[derive(Args)]
struct HomeAssistantAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long = "base-url")]
    base_url: String,
    #[arg(long = "access-token")]
    access_token: Option<String>,
    #[arg(long = "entity-id")]
    monitored_entity_ids: Vec<String>,
    #[arg(long = "service-domain")]
    allowed_service_domains: Vec<String>,
    #[arg(long = "service-entity-id")]
    allowed_service_entity_ids: Vec<String>,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct HomeAssistantServiceArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    domain: String,
    #[arg(long)]
    service: String,
    #[arg(long = "entity-id")]
    entity_id: Option<String>,
    #[arg(long = "service-data-json")]
    service_data_json: Option<String>,
}

#[derive(Args)]
struct DashboardArgs {
    #[arg(long, default_value_t = false)]
    print_url: bool,
    #[arg(long, default_value_t = false)]
    no_open: bool,
}

#[derive(Args)]
struct InboxAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    path: PathBuf,
    #[arg(long)]
    alias: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    delete_after_read: bool,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct ReviewArgs {
    #[arg(long, default_value_t = false, conflicts_with_all = ["base", "commit", "prompt"])]
    uncommitted: bool,

    #[arg(long, value_name = "BRANCH", conflicts_with_all = ["uncommitted", "commit", "prompt"])]
    base: Option<String>,

    #[arg(long, value_name = "SHA", conflicts_with_all = ["uncommitted", "base", "prompt"])]
    commit: Option<String>,

    #[arg(long, value_name = "TITLE", requires = "commit")]
    commit_title: Option<String>,

    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[arg(long, value_enum)]
    thinking: Option<ThinkingLevelArg>,
}

#[derive(Args)]
struct ResumeArgs {
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    #[arg(long = "last", default_value_t = false)]
    last: bool,

    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[arg(long, value_enum)]
    thinking: Option<ThinkingLevelArg>,
}

#[derive(Args)]
struct ForkArgs {
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    last: bool,

    #[arg(long = "all", default_value_t = false)]
    all: bool,

    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[arg(long, value_enum)]
    thinking: Option<ThinkingLevelArg>,
}

#[derive(Args)]
struct CompletionArgs {
    #[arg(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Args)]
struct LogoutArgs {
    #[arg(long)]
    provider: Option<String>,

    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Args)]
struct ResetArgs {
    #[arg(long, short = 'y', default_value_t = false)]
    yes: bool,
}

#[derive(Subcommand)]
enum SessionCommands {
    List,
    Resume { id: String },
    ResumePacket { id: String },
    Rename { id: String, title: String },
}

#[derive(Subcommand)]
enum AutonomyCommands {
    Enable {
        #[arg(long, value_enum, default_value_t = AutonomyModeArg::FreeThinking)]
        mode: AutonomyModeArg,
        #[arg(long, default_value_t = false)]
        allow_self_edit: bool,
    },
    Pause,
    Resume,
    Status,
}

#[derive(Subcommand)]
enum EvolveCommands {
    Start {
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = false)]
        budget_friendly: bool,
    },
    Pause,
    Resume,
    Stop,
    Status,
}

#[derive(Subcommand)]
enum AutopilotCommands {
    Enable,
    Pause,
    Resume,
    Status,
    Config {
        #[arg(long)]
        interval_seconds: Option<u64>,
        #[arg(long)]
        max_concurrent: Option<u8>,
        #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
        allow_shell: Option<bool>,
        #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
        allow_network: Option<bool>,
        #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
        allow_self_edit: Option<bool>,
    },
}

#[derive(Subcommand)]
enum MissionCommands {
    Add {
        title: String,
        #[arg(long, default_value = "")]
        details: String,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        after_seconds: Option<u64>,
        #[arg(long)]
        every_seconds: Option<u64>,
        #[arg(long, value_name = "RFC3339")]
        at: Option<String>,
        #[arg(long, value_name = "PATH")]
        watch: Option<PathBuf>,
        #[arg(long)]
        watch_nonrecursive: bool,
    },
    List,
    Pause {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Resume {
        id: String,
        #[arg(long)]
        after_seconds: Option<u64>,
        #[arg(long)]
        every_seconds: Option<u64>,
        #[arg(long, value_name = "RFC3339")]
        at: Option<String>,
        #[arg(long, value_name = "PATH")]
        watch: Option<PathBuf>,
        #[arg(long)]
        watch_nonrecursive: bool,
        #[arg(long)]
        note: Option<String>,
    },
    Cancel {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Checkpoints {
        id: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
    List {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    Review {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    Approve {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reject {
        id: String,
        #[arg(long)]
        note: Option<String>,
    },
    Profile {
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    Rebuild {
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long, default_value_t = false)]
        recompute_embeddings: bool,
    },
    Remember {
        subject: String,
        content: String,
        #[arg(long, value_enum, default_value_t = MemoryKindArg::Note)]
        kind: MemoryKindArg,
        #[arg(long, value_enum, default_value_t = MemoryScopeArg::Global)]
        scope: MemoryScopeArg,
    },
    Forget {
        id: String,
    },
}

#[derive(Args)]
struct LoginArgs {
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long, value_enum)]
    kind: Option<HostedKindArg>,
    #[arg(long, value_enum)]
    auth: Option<AuthMethodArg>,
    #[arg(long)]
    base_url: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    api_key: Option<String>,
    #[arg(long)]
    client_id: Option<String>,
    #[arg(long = "auth-url")]
    authorization_url: Option<String>,
    #[arg(long = "token-url")]
    token_url: Option<String>,
    #[arg(long = "scope")]
    scopes: Vec<String>,
    #[arg(long = "auth-param")]
    auth_params: Vec<String>,
    #[arg(long = "token-param")]
    token_params: Vec<String>,
    #[arg(long)]
    main_alias: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum HostedKindArg {
    #[value(name = "openai", alias = "openai-compatible")]
    OpenaiCompatible,
    Anthropic,
    Moonshot,
    Openrouter,
    Venice,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LocalKindArg {
    Ollama,
    OpenaiCompatible,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum PersistenceModeArg {
    OnDemand,
    AlwaysOn,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum PermissionPresetArg {
    Suggest,
    AutoEdit,
    FullAuto,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum TaskModeArg {
    Build,
    Daily,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AutonomyModeArg {
    Assisted,
    FreeThinking,
    Evolve,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum AuthMethodArg {
    Browser,
    ApiKey,
    Oauth,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum ThinkingLevelArg {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum MemoryKindArg {
    Preference,
    ProjectFact,
    Workflow,
    Constraint,
    Task,
    Note,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum MemoryScopeArg {
    Global,
    Workspace,
    Session,
    Provider,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum SkillDraftStatusArg {
    Draft,
    Published,
    Rejected,
}

impl From<PersistenceModeArg> for PersistenceMode {
    fn from(value: PersistenceModeArg) -> Self {
        match value {
            PersistenceModeArg::OnDemand => PersistenceMode::OnDemand,
            PersistenceModeArg::AlwaysOn => PersistenceMode::AlwaysOn,
        }
    }
}

impl From<ThinkingLevelArg> for ThinkingLevel {
    fn from(value: ThinkingLevelArg) -> Self {
        match value {
            ThinkingLevelArg::None => ThinkingLevel::None,
            ThinkingLevelArg::Minimal => ThinkingLevel::Minimal,
            ThinkingLevelArg::Low => ThinkingLevel::Low,
            ThinkingLevelArg::Medium => ThinkingLevel::Medium,
            ThinkingLevelArg::High => ThinkingLevel::High,
            ThinkingLevelArg::Xhigh => ThinkingLevel::XHigh,
        }
    }
}

impl From<PermissionPresetArg> for PermissionPreset {
    fn from(value: PermissionPresetArg) -> Self {
        match value {
            PermissionPresetArg::Suggest => PermissionPreset::Suggest,
            PermissionPresetArg::AutoEdit => PermissionPreset::AutoEdit,
            PermissionPresetArg::FullAuto => PermissionPreset::FullAuto,
        }
    }
}

impl From<TaskModeArg> for TaskMode {
    fn from(value: TaskModeArg) -> Self {
        match value {
            TaskModeArg::Build => TaskMode::Build,
            TaskModeArg::Daily => TaskMode::Daily,
        }
    }
}

impl From<AutonomyModeArg> for AutonomyMode {
    fn from(value: AutonomyModeArg) -> Self {
        match value {
            AutonomyModeArg::Assisted => AutonomyMode::Assisted,
            AutonomyModeArg::FreeThinking => AutonomyMode::FreeThinking,
            AutonomyModeArg::Evolve => AutonomyMode::Evolve,
        }
    }
}

impl From<MemoryKindArg> for MemoryKind {
    fn from(value: MemoryKindArg) -> Self {
        match value {
            MemoryKindArg::Preference => MemoryKind::Preference,
            MemoryKindArg::ProjectFact => MemoryKind::ProjectFact,
            MemoryKindArg::Workflow => MemoryKind::Workflow,
            MemoryKindArg::Constraint => MemoryKind::Constraint,
            MemoryKindArg::Task => MemoryKind::Task,
            MemoryKindArg::Note => MemoryKind::Note,
        }
    }
}

impl From<MemoryScopeArg> for MemoryScope {
    fn from(value: MemoryScopeArg) -> Self {
        match value {
            MemoryScopeArg::Global => MemoryScope::Global,
            MemoryScopeArg::Workspace => MemoryScope::Workspace,
            MemoryScopeArg::Session => MemoryScope::Session,
            MemoryScopeArg::Provider => MemoryScope::Provider,
        }
    }
}

impl From<SkillDraftStatusArg> for SkillDraftStatus {
    fn from(value: SkillDraftStatusArg) -> Self {
        match value {
            SkillDraftStatusArg::Draft => SkillDraftStatus::Draft,
            SkillDraftStatusArg::Published => SkillDraftStatus::Published,
            SkillDraftStatusArg::Rejected => SkillDraftStatus::Rejected,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    if matches!(cli.command, Some(Commands::InternalDaemon)) {
        return agent_daemon::run_daemon().await;
    }
    if let Some(cwd) = &cli.cwd {
        std::env::set_current_dir(cwd)
            .with_context(|| format!("failed to change directory to {}", cwd.display()))?;
    }
    let storage = Storage::open()?;

    match cli.command {
        None => {
            launch_chat_session(
                &storage,
                None,
                None,
                cli.prompt,
                None,
                None,
                Vec::new(),
                None,
                false,
            )
            .await?
        }
        Some(Commands::Exec(args)) => run_command(&storage, args).await?,
        Some(Commands::Review(args)) => review_command(&storage, args).await?,
        Some(Commands::Resume(args)) => resume_command(&storage, args).await?,
        Some(Commands::Fork(args)) => fork_command(&storage, args).await?,
        Some(Commands::Completion(args)) => completion_command(args),
        Some(Commands::Logout(args)) => logout_command(&storage, args).await?,
        Some(Commands::Reset(args)) => reset_command(&storage, args).await?,
        Some(Commands::Setup) => setup(&storage).await?,
        Some(Commands::Daemon { command }) => daemon_command(&storage, command).await?,
        Some(Commands::Login(args)) => login_command(&storage, args).await?,
        Some(Commands::Provider { command }) => provider_command(&storage, command).await?,
        Some(Commands::Mcp { command }) => mcp_command(&storage, command).await?,
        Some(Commands::App { command }) => app_command(&storage, command).await?,
        Some(Commands::Plugin { command }) => plugin_command(&storage, command).await?,
        Some(Commands::Repo { command }) => repo_command(&storage, command).await?,
        Some(Commands::Telegram { command }) => telegram_command(&storage, command).await?,
        Some(Commands::Discord { command }) => discord_command(&storage, command).await?,
        Some(Commands::Slack { command }) => slack_command(&storage, command).await?,
        Some(Commands::Signal { command }) => signal_command(&storage, command).await?,
        Some(Commands::HomeAssistant { command }) => {
            home_assistant_command(&storage, command).await?
        }
        Some(Commands::Webhook { command }) => webhook_command(&storage, command).await?,
        Some(Commands::Inbox { command }) => inbox_command(&storage, command).await?,
        Some(Commands::Skills { command }) => skills_command(&storage, command).await?,
        Some(Commands::Model { command }) => model_command(&storage, command).await?,
        Some(Commands::Alias { command }) => alias_command(&storage, command).await?,
        Some(Commands::Permissions(args)) => permissions_command(&storage, args).await?,
        Some(Commands::Trust(args)) => trust_command(&storage, args).await?,
        Some(Commands::Run(args)) => run_command(&storage, args).await?,
        Some(Commands::Chat(args)) => chat_command(&storage, args).await?,
        Some(Commands::Session { command }) => session_command(&storage, command).await?,
        Some(Commands::Autonomy { command }) => autonomy_command(&storage, command).await?,
        Some(Commands::Evolve { command }) => evolve_command(&storage, command).await?,
        Some(Commands::Autopilot { command }) => autopilot_command(&storage, command).await?,
        Some(Commands::Mission { command }) => mission_command(&storage, command).await?,
        Some(Commands::Memory { command }) => memory_command(&storage, command).await?,
        Some(Commands::Logs { limit, follow }) => logs_command(&storage, limit, follow).await?,
        Some(Commands::Dashboard(args)) => dashboard_command(&storage, args).await?,
        Some(Commands::Doctor) => doctor_command(&storage).await?,
        Some(Commands::InternalDaemon) => unreachable!(),
    }

    Ok(())
}

async fn setup(storage: &Storage) -> Result<()> {
    let theme = ColorfulTheme::default();
    let mut config = storage.load_config()?;
    print_onboarding_banner_clean(&config)?;
    let cwd = current_request_cwd()?;

    let mode_idx = Select::with_theme(&theme)
        .with_prompt("How should the agent daemon run?")
        .items([
            "On-demand (starts when you use the CLI)",
            "Always-on (persistent daemon)",
        ])
        .default(matches!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn) as usize)
        .interact()?;
    config.daemon.persistence_mode = if mode_idx == 0 {
        PersistenceMode::OnDemand
    } else {
        PersistenceMode::AlwaysOn
    };
    config.daemon.auto_start =
        if matches!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn) {
            Confirm::with_theme(&theme)
                .with_prompt("Enable auto-start on boot/login?")
                .default(config.daemon.auto_start)
                .interact()?
        } else {
            println!("Auto-start is only available in always-on mode.");
            false
        };

    if needs_onboarding(&config) {
        println!();
        println!(
            "A main model must be configured before {} can start.",
            PRIMARY_COMMAND_NAME
        );
        let (request, alias) = interactive_provider_setup(&theme, &config).await?;
        apply_provider_request_locally(&mut config, &request)?;
        config.main_agent_alias = Some(alias.alias.clone());
        config.upsert_alias(alias);
    } else if Confirm::with_theme(&theme)
        .with_prompt("Configure another provider now?")
        .default(false)
        .interact()?
    {
        let (request, alias) = interactive_provider_setup(&theme, &config).await?;
        apply_provider_request_locally(&mut config, &request)?;
        if config.main_agent_alias.is_none() {
            config.main_agent_alias = Some(alias.alias.clone());
        }
        config.upsert_alias(alias);
    }

    if Confirm::with_theme(&theme)
        .with_prompt(format!(
            "Trust the current directory for project file access? ({})",
            cwd.display()
        ))
        .default(
            config.trust_policy.trusted_paths.is_empty()
                || config
                    .trust_policy
                    .trusted_paths
                    .iter()
                    .any(|path| path == &cwd),
        )
        .interact()?
        && !config
            .trust_policy
            .trusted_paths
            .iter()
            .any(|path| path == &cwd)
    {
        config.trust_policy.trusted_paths.push(cwd.clone());
    }

    config.permission_preset = select_permission_preset(&theme, config.permission_preset)?;
    config.trust_policy.allow_shell = Confirm::with_theme(&theme)
        .with_prompt("Allow shell commands by default inside trusted workspaces?")
        .default(config.trust_policy.allow_shell)
        .interact()?;
    config.trust_policy.allow_network = Confirm::with_theme(&theme)
        .with_prompt("Allow general network tools by default?")
        .default(config.trust_policy.allow_network)
        .interact()?;

    if !has_usable_main_alias(&config) {
        bail!("setup was not completed with a usable main alias");
    }

    config.onboarding_complete = true;
    storage.save_config(&config)?;
    storage.sync_autostart(
        &current_executable_path()?,
        &[INTERNAL_DAEMON_ARG],
        config.daemon.auto_start,
    )?;

    println!(
        "Saved configuration to {}",
        storage.paths().config_path.display()
    );
    println!(
        "Persistence mode: {:?}, auto-start: {}",
        config.daemon.persistence_mode, config.daemon.auto_start
    );

    if Confirm::with_theme(&theme)
        .with_prompt("Start the daemon now?")
        .default(true)
        .interact()?
    {
        start_daemon_process()?;
        wait_for_daemon(&config).await?;
        println!("Daemon started.");
    }

    Ok(())
}

fn needs_onboarding(config: &AppConfig) -> bool {
    !config.onboarding_complete || !has_usable_main_alias(config)
}

fn has_usable_main_alias(config: &AppConfig) -> bool {
    config
        .main_alias()
        .ok()
        .and_then(|alias| config.resolve_provider(&alias.provider_id))
        .is_some_and(|provider| provider_has_saved_access(&provider))
}

async fn ensure_onboarded(storage: &Storage) -> Result<()> {
    let config = storage.load_config()?;
    if !needs_onboarding(&config) {
        return Ok(());
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "no completed setup found; run `{} setup` in an interactive terminal first",
            PRIMARY_COMMAND_NAME
        );
    }

    println!("No completed setup found. Launching onboarding.");
    setup(storage).await?;

    let updated = storage.load_config()?;
    if needs_onboarding(&updated) {
        bail!("onboarding did not finish with a usable main model");
    }

    Ok(())
}

#[allow(dead_code)]
fn print_onboarding_banner(config: &AppConfig) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let directory = current_request_cwd()?;
    let model_label = config
        .main_agent_alias
        .as_deref()
        .unwrap_or("not configured");
    let lines = [
        format!(" >_ {DISPLAY_APP_NAME} CLI (v{version})"),
        String::new(),
        format!(" model:     {model_label}"),
        format!(" directory: {}", directory.display()),
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0) + 2;
    println!("â•­{}â•®", "â”€".repeat(width));
    for line in lines {
        println!("â”‚ {:width$} â”‚", line, width = width.saturating_sub(1));
    }
    println!("â•°{}â•¯", "â”€".repeat(width));
    Ok(())
}

fn select_permission_preset(
    theme: &ColorfulTheme,
    current: PermissionPreset,
) -> Result<PermissionPreset> {
    let items = [
        "Suggest (read-only tools only)",
        "Auto-edit (edit files without shell/network)",
        "Full-auto (all tools enabled by default)",
    ];
    let default_index = match current {
        PermissionPreset::Suggest => 0,
        PermissionPreset::AutoEdit => 1,
        PermissionPreset::FullAuto => 2,
    };
    let selection = Select::with_theme(theme)
        .with_prompt("Choose the default permission preset")
        .items(items)
        .default(default_index)
        .interact()?;
    Ok(match selection {
        0 => PermissionPreset::Suggest,
        1 => PermissionPreset::AutoEdit,
        2 => PermissionPreset::FullAuto,
        _ => unreachable!("invalid permission selection"),
    })
}

fn print_onboarding_banner_clean(config: &AppConfig) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let directory = current_request_cwd()?;
    let model_label = config
        .main_agent_alias
        .as_deref()
        .unwrap_or("not configured");
    let lines = [
        format!(" >_ {DISPLAY_APP_NAME} CLI (v{version})"),
        String::new(),
        format!(" model:     {model_label}"),
        format!(" directory: {}", directory.display()),
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0) + 2;
    println!(".{}.", "-".repeat(width));
    for line in lines {
        println!("| {:width$} |", line, width = width.saturating_sub(1));
    }
    println!("'{}'", "-".repeat(width));
    Ok(())
}

async fn daemon_command(storage: &Storage, command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start => {
            if try_daemon(storage).await?.is_some() {
                println!("Daemon already running.");
                return Ok(());
            }
            start_daemon_process()?;
            let config = storage.load_config()?;
            wait_for_daemon(&config).await?;
            println!(
                "Daemon started at http://{}:{}",
                config.daemon.host, config.daemon.port
            );
        }
        DaemonCommands::Stop => {
            let Some(client) = try_daemon(storage).await? else {
                println!("Daemon is not running.");
                return Ok(());
            };
            let _: serde_json::Value = client.post("/v1/shutdown", &serde_json::json!({})).await?;
            println!("Daemon stop requested.");
        }
        DaemonCommands::Status => {
            let Some(client) = try_daemon(storage).await? else {
                let config = storage.load_config()?;
                println!("running: false");
                println!("persistence: {:?}", config.daemon.persistence_mode);
                println!("auto_start: {}", config.daemon.auto_start);
                return Ok(());
            };
            let status: DaemonStatus = client.get("/v1/status").await?;
            println!("running: true");
            println!("pid: {}", status.pid);
            println!("started_at: {}", status.started_at);
            println!("persistence: {:?}", status.persistence_mode);
            println!("auto_start: {}", status.auto_start);
            println!("autonomy: {}", autonomy_summary(status.autonomy.state));
            println!("providers: {}", status.providers);
            println!("aliases: {}", status.aliases);
            println!("plugins: {}", status.plugins);
            println!("webhooks: {}", status.webhook_connectors);
            println!("inboxes: {}", status.inbox_connectors);
            println!("telegram: {}", status.telegram_connectors);
            println!("discord: {}", status.discord_connectors);
            println!(
                "pending_connector_approvals: {}",
                status.pending_connector_approvals
            );
            println!("missions: {}", status.missions);
            println!("active_missions: {}", status.active_missions);
            println!("memories: {}", status.memories);
            println!("pending_memory_reviews: {}", status.pending_memory_reviews);
            println!("skill_drafts: {}", status.skill_drafts);
            println!("published_skills: {}", status.published_skills);
        }
        DaemonCommands::Config(args) => {
            if args.mode.is_none() && args.auto_start.is_none() {
                bail!("provide --mode and/or --auto-start");
            }
            let current_config = storage.load_config()?;
            let next_mode = args
                .mode
                .map(Into::into)
                .unwrap_or(current_config.daemon.persistence_mode);
            let next_auto_start = args.auto_start.unwrap_or(current_config.daemon.auto_start);
            if matches!(next_mode, PersistenceMode::OnDemand) && next_auto_start {
                bail!("auto-start requires always-on daemon mode");
            }
            if let Some(client) = try_daemon(storage).await? {
                let config: agent_core::DaemonConfig = client
                    .put(
                        "/v1/daemon/config",
                        &DaemonConfigUpdateRequest {
                            persistence_mode: args.mode.map(Into::into),
                            auto_start: args.auto_start,
                        },
                    )
                    .await?;
                println!(
                    "daemon config updated: mode={:?}, auto_start={}",
                    config.persistence_mode, config.auto_start
                );
            } else {
                let mut config = storage.load_config()?;
                if let Some(mode) = args.mode {
                    config.daemon.persistence_mode = mode.into();
                }
                if let Some(auto_start) = args.auto_start {
                    config.daemon.auto_start = auto_start;
                }
                storage.save_config(&config)?;
                storage.sync_autostart(
                    &current_executable_path()?,
                    &[INTERNAL_DAEMON_ARG],
                    config.daemon.auto_start,
                )?;
                println!(
                    "daemon config updated locally: mode={:?}, auto_start={}",
                    config.daemon.persistence_mode, config.daemon.auto_start
                );
            }
        }
    }

    Ok(())
}

async fn login_command(storage: &Storage, args: LoginArgs) -> Result<()> {
    let theme = ColorfulTheme::default();
    let kind = args.kind.unwrap_or(select_hosted_kind(&theme)?);
    let (default_provider_id, default_provider_name) = hosted_kind_defaults(kind);
    let provider_id = args.id.unwrap_or_else(|| default_provider_id.to_string());
    let provider_name = args
        .name
        .unwrap_or_else(|| default_provider_name.to_string());
    let auth_method = match args.auth {
        Some(auth) => auth,
        None => select_auth_method(&theme, kind)?,
    };
    let default_url = match auth_method {
        AuthMethodArg::Browser => default_browser_hosted_url(kind),
        AuthMethodArg::ApiKey | AuthMethodArg::Oauth => default_hosted_url(kind),
    };
    let base_url = args.base_url.unwrap_or_else(|| default_url.to_string());
    let main_alias = resolve_main_alias(storage, args.main_alias)?;

    let mut request = match auth_method {
        AuthMethodArg::Browser => match complete_browser_login(kind, &provider_name).await? {
            BrowserLoginResult::ApiKey(api_key) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: provider_id.clone(),
                    display_name: provider_name,
                    kind: hosted_kind_to_provider_kind(kind),
                    base_url,
                    auth_mode: AuthMode::ApiKey,
                    default_model: None,
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                api_key: Some(api_key),
                oauth_token: None,
            },
            BrowserLoginResult::OAuthToken(token) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: provider_id.clone(),
                    display_name: provider_name,
                    kind: browser_hosted_kind_to_provider_kind(kind),
                    base_url,
                    auth_mode: AuthMode::OAuth,
                    default_model: None,
                    keychain_account: None,
                    oauth: Some(openai_browser_oauth_config()),
                    local: false,
                },
                api_key: None,
                oauth_token: Some(token),
            },
        },
        AuthMethodArg::ApiKey => ProviderUpsertRequest {
            provider: ProviderConfig {
                id: provider_id.clone(),
                display_name: provider_name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                auth_mode: AuthMode::ApiKey,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: false,
            },
            api_key: Some(match args.api_key {
                Some(api_key) => api_key,
                None => Password::with_theme(&theme)
                    .with_prompt("API key")
                    .allow_empty_password(false)
                    .interact()?,
            }),
            oauth_token: None,
        },
        AuthMethodArg::Oauth => {
            let provider = ProviderConfig {
                id: provider_id.clone(),
                display_name: provider_name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                auth_mode: AuthMode::OAuth,
                default_model: None,
                keychain_account: None,
                oauth: Some(OAuthConfig {
                    client_id: prompt_or_value(&theme, "OAuth client id", args.client_id, None)?,
                    authorization_url: prompt_or_value(
                        &theme,
                        "OAuth authorization URL",
                        args.authorization_url,
                        None,
                    )?,
                    token_url: prompt_or_value(&theme, "OAuth token URL", args.token_url, None)?,
                    scopes: collect_scopes(&theme, args.scopes)?,
                    extra_authorize_params: collect_key_value_params(
                        &theme,
                        "Additional authorization params (k=v, comma separated)",
                        args.auth_params,
                    )?,
                    extra_token_params: collect_key_value_params(
                        &theme,
                        "Additional token params (k=v, comma separated)",
                        args.token_params,
                    )?,
                }),
                local: false,
            };
            let token = complete_oauth_login(&provider).await?;
            ProviderUpsertRequest {
                provider,
                api_key: None,
                oauth_token: Some(token),
            }
        }
    };

    let default_model = resolve_hosted_model_after_auth(&theme, &request, args.model).await?;
    request.provider.default_model = Some(default_model.clone());
    upsert_provider_with_optional_alias(storage, request, main_alias, default_model).await
}

async fn provider_command(storage: &Storage, command: ProviderCommands) -> Result<()> {
    match command {
        ProviderCommands::Add(args) => {
            let api_key = args
                .api_key
                .ok_or_else(|| anyhow!("--api-key is required for hosted provider add"))?;
            let provider = ProviderConfig {
                id: args.id,
                display_name: args.name,
                kind: hosted_kind_to_provider_kind(args.kind),
                base_url: args
                    .base_url
                    .unwrap_or_else(|| default_hosted_url(args.kind).to_string()),
                auth_mode: AuthMode::ApiKey,
                default_model: Some(args.model.clone()),
                keychain_account: None,
                oauth: None,
                local: false,
            };
            let request = ProviderUpsertRequest {
                provider,
                api_key: Some(api_key),
                oauth_token: None,
            };
            let main_alias = resolve_main_alias(storage, args.main_alias)?;
            upsert_provider_with_optional_alias(storage, request, main_alias, args.model).await?;
        }
        ProviderCommands::AddLocal(args) => {
            let base_url = args.base_url.unwrap_or_else(|| match args.kind {
                LocalKindArg::Ollama => DEFAULT_OLLAMA_URL.to_string(),
                LocalKindArg::OpenaiCompatible => DEFAULT_LOCAL_OPENAI_URL.to_string(),
            });
            let mut provider = ProviderConfig {
                id: args.id,
                display_name: args.name,
                kind: match args.kind {
                    LocalKindArg::Ollama => ProviderKind::Ollama,
                    LocalKindArg::OpenaiCompatible => ProviderKind::OpenAiCompatible,
                },
                base_url,
                auth_mode: if args.api_key.is_some() {
                    AuthMode::ApiKey
                } else {
                    AuthMode::None
                },
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: true,
            };
            let model = determine_local_model(&provider, args.model, None).await?;
            provider.default_model = Some(model.clone());
            let request = ProviderUpsertRequest {
                provider,
                api_key: args.api_key,
                oauth_token: None,
            };
            let main_alias = resolve_main_alias(storage, args.main_alias)?;
            upsert_provider_with_optional_alias(storage, request, main_alias, model).await?;
        }
        ProviderCommands::List => {
            let config = storage.load_config()?;
            for provider in config.providers {
                println!(
                    "{} [{}] auth={:?} model={} url={}",
                    provider.id,
                    if provider.local { "local" } else { "remote" },
                    provider.auth_mode,
                    provider.default_model.unwrap_or_else(|| "-".to_string()),
                    provider.base_url
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct SkillInfo {
    name: String,
    description: String,
    path: PathBuf,
}

async fn load_enabled_skills(storage: &Storage) -> Result<Vec<String>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/skills").await
    } else {
        Ok(storage.load_config()?.enabled_skills)
    }
}

async fn load_skill_drafts(
    storage: &Storage,
    limit: usize,
    status: Option<SkillDraftStatus>,
) -> Result<Vec<SkillDraft>> {
    if let Some(client) = try_daemon(storage).await? {
        let mut path = format!("/v1/skills/drafts?limit={limit}");
        if let Some(status) = status {
            path.push_str("&status=");
            path.push_str(match status {
                SkillDraftStatus::Draft => "draft",
                SkillDraftStatus::Published => "published",
                SkillDraftStatus::Rejected => "rejected",
            });
        }
        client.get(&path).await
    } else {
        storage.list_skill_drafts(limit, status, None, None)
    }
}

async fn load_profile_memories(storage: &Storage, limit: usize) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/profile?limit={limit}"))
            .await
    } else {
        let mut seen = HashSet::new();
        let mut memories = storage.list_memories_by_tag("system_profile", limit, None, None)?;
        memories.extend(storage.list_memories_by_tag("workspace_profile", limit, None, None)?);
        memories.retain(|memory| seen.insert(memory.id.clone()));
        memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        memories.truncate(limit);
        Ok(memories)
    }
}

async fn load_memory_review_queue(storage: &Storage, limit: usize) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/review?limit={limit}"))
            .await
    } else {
        storage.list_memories_by_review_status(MemoryReviewStatus::Candidate, limit)
    }
}

async fn load_connector_approvals(
    storage: &Storage,
    kind: ConnectorKind,
    limit: usize,
) -> Result<Vec<ConnectorApprovalRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!(
                "/v1/connector-approvals?kind={}&status=pending&limit={limit}",
                serde_json::to_string(&kind)?.trim_matches('"')
            ))
            .await
    } else {
        storage.list_connector_approvals(Some(kind), Some(ConnectorApprovalStatus::Pending), limit)
    }
}

async fn update_memory_review_status(
    storage: &Storage,
    id: &str,
    status: MemoryReviewStatus,
    note: Option<String>,
) -> Result<MemoryRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            MemoryReviewStatus::Accepted => format!("/v1/memory/{id}/approve"),
            MemoryReviewStatus::Rejected => format!("/v1/memory/{id}/reject"),
            MemoryReviewStatus::Candidate => {
                bail!("cannot set memory back to candidate from CLI")
            }
        };
        client
            .post(&path, &MemoryReviewUpdateRequest { status, note })
            .await
    } else {
        let updated = storage.update_memory_review_status(id, status, note.as_deref())?;
        if !updated {
            bail!("unknown memory '{id}'");
        }
        storage
            .get_memory(id)?
            .ok_or_else(|| anyhow!("unknown memory '{id}'"))
    }
}

async fn update_connector_approval_status(
    storage: &Storage,
    id: &str,
    status: ConnectorApprovalStatus,
    note: Option<String>,
) -> Result<ConnectorApprovalRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            ConnectorApprovalStatus::Pending => {
                bail!("cannot set connector approval back to pending from CLI")
            }
            ConnectorApprovalStatus::Approved => {
                format!("/v1/connector-approvals/{id}/approve")
            }
            ConnectorApprovalStatus::Rejected => {
                format!("/v1/connector-approvals/{id}/reject")
            }
        };
        client
            .post(&path, &ConnectorApprovalUpdateRequest { note })
            .await
    } else {
        let updated =
            storage.update_connector_approval_status(id, status, note.as_deref(), None)?;
        if !updated {
            bail!("unknown connector approval '{id}'");
        }
        storage
            .get_connector_approval(id)?
            .ok_or_else(|| anyhow!("unknown connector approval '{id}'"))
    }
}

async fn update_skill_draft_status(
    storage: &Storage,
    id: &str,
    status: SkillDraftStatus,
) -> Result<SkillDraft> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            SkillDraftStatus::Draft => bail!("cannot set skill draft back to draft from CLI"),
            SkillDraftStatus::Published => format!("/v1/skills/drafts/{id}/publish"),
            SkillDraftStatus::Rejected => format!("/v1/skills/drafts/{id}/reject"),
        };
        client.post(&path, &serde_json::json!({})).await
    } else {
        let mut draft = storage
            .get_skill_draft(id)?
            .ok_or_else(|| anyhow!("unknown skill draft '{id}'"))?;
        draft.status = status;
        draft.updated_at = chrono::Utc::now();
        storage.upsert_skill_draft(&draft)?;
        Ok(draft)
    }
}

pub(crate) fn format_memory_records(records: &[MemoryRecord]) -> String {
    if records.is_empty() {
        return "No stored memory.".to_string();
    }

    records
        .iter()
        .map(|memory| {
            let tags = if memory.tags.is_empty() {
                String::new()
            } else {
                format!(" tags={}", memory.tags.join(","))
            };
            let review = if matches!(memory.review_status, MemoryReviewStatus::Accepted) {
                String::new()
            } else {
                format!(" review={:?}", memory.review_status)
            };
            let note = memory
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            let source = match (
                memory.source_session_id.as_deref(),
                memory.source_message_id.as_deref(),
            ) {
                (Some(session_id), Some(message_id)) => {
                    format!("\n  source: session={session_id} message={message_id}")
                }
                (Some(session_id), None) => format!("\n  source: session={session_id}"),
                (None, Some(message_id)) => format!("\n  source: message={message_id}"),
                (None, None) => String::new(),
            };
            let evidence = if memory.evidence_refs.is_empty() {
                String::new()
            } else {
                let mut lines = memory
                    .evidence_refs
                    .iter()
                    .take(3)
                    .map(|evidence| {
                        let role = evidence
                            .role
                            .as_ref()
                            .map(|role| format!(" role={role:?}"))
                            .unwrap_or_default();
                        let message = evidence
                            .message_id
                            .as_deref()
                            .map(|value| format!(" message={value}"))
                            .unwrap_or_default();
                        let tool = match (
                            evidence.tool_name.as_deref(),
                            evidence.tool_call_id.as_deref(),
                        ) {
                            (Some(name), Some(call_id)) => format!(" tool={name}#{call_id}"),
                            (Some(name), None) => format!(" tool={name}"),
                            (None, Some(call_id)) => format!(" tool_call={call_id}"),
                            (None, None) => String::new(),
                        };
                        format!(
                            "\n    - session={}{}{}{} @ {}",
                            evidence.session_id, role, message, tool, evidence.created_at
                        )
                    })
                    .collect::<String>();
                if memory.evidence_refs.len() > 3 {
                    lines.push_str(&format!(
                        "\n    - ... {} more",
                        memory.evidence_refs.len() - 3
                    ));
                }
                format!("\n  evidence:{lines}")
            };
            format!(
                "{} [{:?}/{:?}] {}{}{}\n  {}{}{}{}",
                memory.id,
                memory.kind,
                memory.scope,
                memory.subject,
                tags,
                review,
                memory.content,
                source,
                note,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_connector_approvals(records: &[ConnectorApprovalRecord]) -> String {
    if records.is_empty() {
        return "No pending connector approvals.".to_string();
    }

    records
        .iter()
        .map(|approval| {
            let note = approval
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] {} chat={} user={}\n  {}\n  {}{}",
                approval.id,
                approval.status,
                approval.connector_name,
                approval.external_chat_display.as_deref().unwrap_or("-"),
                approval.external_user_display.as_deref().unwrap_or("-"),
                approval.title,
                approval
                    .message_preview
                    .as_deref()
                    .unwrap_or(approval.details.as_str()),
                note
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_skill_drafts(drafts: &[SkillDraft]) -> String {
    if drafts.is_empty() {
        return "No learned skill drafts.".to_string();
    }

    drafts
        .iter()
        .map(|draft| {
            let trigger = draft
                .trigger_hint
                .as_deref()
                .map(|value| format!(" trigger={value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] usage={}{}\n  {}\n  {}",
                draft.id, draft.status, draft.usage_count, trigger, draft.title, draft.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn load_prompt_template(inline: Option<&str>, file: Option<&PathBuf>) -> Result<String> {
    match (inline, file) {
        (Some(value), None) => Ok(value.to_string()),
        (None, Some(path)) => {
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
        }
        (Some(_), Some(_)) => bail!("specify either --prompt-template or --prompt-file"),
        (None, None) => bail!("one of --prompt-template or --prompt-file is required"),
    }
}

fn load_json_file(path: &Path) -> Result<serde_json::Value> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))
}

async fn update_enabled_skill(storage: &Storage, name: &str, enabled: bool) -> Result<()> {
    let available = discover_skills()?;
    if enabled && !available.iter().any(|skill| skill.name == name) {
        bail!("unknown skill '{name}'");
    }
    let mut enabled_skills = load_enabled_skills(storage).await?;
    if enabled {
        if !enabled_skills.iter().any(|skill| skill == name) {
            enabled_skills.push(name.to_string());
        }
    } else {
        enabled_skills.retain(|skill| skill != name);
    }
    if let Some(client) = try_daemon(storage).await? {
        let _: Vec<String> = client
            .put(
                "/v1/skills",
                &SkillUpdateRequest {
                    enabled_skills: enabled_skills.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.enabled_skills = enabled_skills;
        storage.save_config(&config)?;
    }
    Ok(())
}

fn discover_skills() -> Result<Vec<SkillInfo>> {
    let Some(root) = codex_skills_root() else {
        return Ok(Vec::new());
    };
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    discover_skills_in_dir(&root, &mut skills)?;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

fn discover_skills_in_dir(root: &Path, output: &mut Vec<SkillInfo>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            discover_skills_in_dir(&path, output)?;
            continue;
        }
        if entry.file_name().to_string_lossy() != "SKILL.md" {
            continue;
        }
        let name = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("failed to infer skill name from {}", path.display()))?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let description = extract_skill_description(&content);
        output.push(SkillInfo {
            name,
            description,
            path,
        });
    }
    Ok(())
}

fn codex_skills_root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".codex").join("skills"))
}

fn extract_skill_description(content: &str) -> String {
    let lines = content.lines().map(str::trim).collect::<Vec<_>>();
    if lines.first().copied() == Some("---") {
        let mut in_frontmatter = true;
        for line in &lines[1..] {
            if *line == "---" {
                in_frontmatter = false;
                continue;
            }
            if in_frontmatter {
                if let Some(value) = line.strip_prefix("description:") {
                    return value.trim().trim_matches('"').to_string();
                }
            }
        }
    }

    lines
        .into_iter()
        .find(|line| !line.is_empty() && !line.starts_with('#') && *line != "---" && *line != "```")
        .unwrap_or("No description available.")
        .to_string()
}

async fn model_command(storage: &Storage, command: ModelCommands) -> Result<()> {
    match command {
        ModelCommands::List { provider } => {
            let config = storage.load_config()?;
            let provider = config
                .get_provider(&provider)
                .cloned()
                .ok_or_else(|| anyhow!("unknown provider"))?;
            let models = provider_list_models(&build_http_client(), &provider).await?;
            for model in models {
                println!("{model}");
            }
        }
    }

    Ok(())
}

async fn alias_command(storage: &Storage, command: AliasCommands) -> Result<()> {
    match command {
        AliasCommands::Add(args) => {
            let payload = AliasUpsertRequest {
                alias: ModelAlias {
                    alias: args.alias.clone(),
                    provider_id: args.provider,
                    model: args.model,
                    description: args.description,
                },
                set_as_main: args.main,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: ModelAlias = client.post("/v1/aliases", &payload).await?;
            } else {
                let mut config = storage.load_config()?;
                if config
                    .resolve_provider(&payload.alias.provider_id)
                    .is_none()
                {
                    bail!("alias references unknown provider");
                }
                if payload.set_as_main {
                    config.main_agent_alias = Some(payload.alias.alias.clone());
                }
                config.upsert_alias(payload.alias.clone());
                storage.save_config(&config)?;
            }
            println!("alias '{}' configured", args.alias);
        }
        AliasCommands::List => {
            let config = storage.load_config()?;
            for alias in config.aliases {
                println!(
                    "{} -> {} / {}{}",
                    alias.alias,
                    alias.provider_id,
                    alias.model,
                    alias
                        .description
                        .as_deref()
                        .map(|text| format!(" ({text})"))
                        .unwrap_or_default()
                );
            }
        }
    }

    Ok(())
}

async fn trust_command(storage: &Storage, args: TrustArgs) -> Result<()> {
    let update = TrustUpdateRequest {
        trusted_path: args.path,
        allow_shell: args.allow_shell,
        allow_network: args.allow_network,
        allow_full_disk: args.allow_full_disk,
        allow_self_edit: args.allow_self_edit,
    };
    let policy: agent_core::TrustPolicy = if let Some(client) = try_daemon(storage).await? {
        client.put("/v1/trust", &update).await?
    } else {
        let mut config = storage.load_config()?;
        apply_trust_update(&mut config.trust_policy, &update);
        storage.save_config(&config)?;
        config.trust_policy
    };
    println!("{}", trust_summary(&policy));
    Ok(())
}

async fn permissions_command(storage: &Storage, args: PermissionsArgs) -> Result<()> {
    if let Some(preset) = args.preset {
        let preset: PermissionPreset = preset.into();
        let updated: PermissionPreset = if let Some(client) = try_daemon(storage).await? {
            client
                .put(
                    "/v1/permissions",
                    &PermissionUpdateRequest {
                        permission_preset: preset,
                    },
                )
                .await?
        } else {
            let mut config = storage.load_config()?;
            config.permission_preset = preset;
            storage.save_config(&config)?;
            config.permission_preset
        };
        println!("permission_preset={}", permission_summary(updated));
    } else {
        let preset = if let Some(client) = try_daemon(storage).await? {
            client.get::<PermissionPreset>("/v1/permissions").await?
        } else {
            storage.load_config()?.permission_preset
        };
        println!("permission_preset={}", permission_summary(preset));
    }
    Ok(())
}

fn load_schema_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn collect_image_attachments(base_cwd: &Path, paths: &[PathBuf]) -> Result<Vec<InputAttachment>> {
    paths
        .iter()
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                base_cwd.join(path)
            };
            let canonical = absolute
                .canonicalize()
                .with_context(|| format!("failed to access attachment {}", absolute.display()))?;
            if !canonical.is_file() {
                bail!("attachment {} is not a file", canonical.display());
            }
            Ok(InputAttachment {
                kind: agent_core::AttachmentKind::Image,
                path: canonical,
            })
        })
        .collect()
}

fn maybe_write_last_message(path: Option<&Path>, content: &str) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn print_json_run_response(response: &RunTaskResponse) -> Result<()> {
    for event in &response.tool_events {
        println!(
            "{}",
            serde_json::json!({
                "event": "tool",
                "call_id": event.call_id,
                "name": event.name,
                "arguments": event.arguments,
                "outcome": event.outcome,
                "output": event.output,
            })
        );
    }
    let structured_output = response
        .structured_output_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    println!(
        "{}",
        serde_json::json!({
            "event": "response",
            "session_id": response.session_id,
            "alias": response.alias,
            "provider_id": response.provider_id,
            "model": response.model,
            "response": response.response,
            "structured_output": structured_output,
        })
    );
    Ok(())
}

async fn run_command(storage: &Storage, args: RunArgs) -> Result<()> {
    ensure_onboarded(storage).await?;
    let client = ensure_daemon(storage).await?;
    let cwd = current_request_cwd()?;
    let thinking_level = resolve_thinking_level(storage, args.thinking)?;
    let task_mode = args.mode.map(Into::into);
    let attachments = collect_image_attachments(&cwd, &args.images)?;
    let output_schema_json = args
        .output_schema
        .as_deref()
        .map(load_schema_file)
        .transpose()?;
    if !args.tasks.is_empty() {
        let tasks = args
            .tasks
            .into_iter()
            .map(parse_task)
            .collect::<Result<Vec<_>>>()?;
        let response: BatchTaskResponse = client
            .post(
                "/v1/batch",
                &BatchTaskRequest {
                    tasks,
                    cwd: Some(cwd),
                    thinking_level,
                    task_mode,
                    strategy: None,
                    parent_alias: None,
                },
            )
            .await?;
        if !args.json && !response.summary.is_empty() {
            println!("{}\n", response.summary);
        }
        for result in response.results {
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": "batch_result",
                        "alias": result.alias,
                        "provider_id": result.provider_id,
                        "model": result.model,
                        "success": result.success,
                        "response": result.response,
                        "error": result.error,
                    })
                );
            } else if result.success {
                println!(
                    "[{} | {} | {}]\n{}\n",
                    result.alias, result.provider_id, result.model, result.response
                );
            } else {
                println!(
                    "[{} | {} | {} | error]\n{}\n",
                    result.alias,
                    result.provider_id,
                    result.model,
                    result
                        .error
                        .as_deref()
                        .unwrap_or("subagent task failed without an error message")
                );
            }
        }
        return Ok(());
    }

    let prompt = normalize_prompt_input(args.prompt)?
        .ok_or_else(|| anyhow!("prompt is required when --task is not used"))?;
    let response = execute_prompt(
        &client,
        prompt,
        args.alias,
        None,
        None,
        cwd,
        thinking_level,
        task_mode,
        attachments,
        args.permissions.map(Into::into),
        output_schema_json,
        args.ephemeral,
    )
    .await?;
    maybe_write_last_message(args.output_last_message.as_deref(), &response.response)?;
    if args.json {
        print_json_run_response(&response)?;
    } else {
        println!("{}", response.response);
        println!(
            "\nsession={} alias={} provider={} model={}",
            response.session_id, response.alias, response.provider_id, response.model
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn launch_chat_session(
    storage: &Storage,
    alias: Option<String>,
    session_id: Option<String>,
    initial_prompt: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    attachments: Vec<InputAttachment>,
    permission_preset: Option<PermissionPreset>,
    no_tui: bool,
) -> Result<()> {
    ensure_onboarded(storage).await?;
    if !no_tui && io::stdout().is_terminal() && io::stdin().is_terminal() {
        return tui::run_tui_session(
            storage,
            alias,
            session_id,
            initial_prompt,
            thinking_level,
            task_mode,
            attachments,
            permission_preset,
        )
        .await;
    }
    interactive_session(
        storage,
        alias,
        session_id,
        initial_prompt,
        thinking_level,
        task_mode,
        attachments,
        permission_preset,
    )
    .await
}

async fn chat_command(storage: &Storage, args: ChatArgs) -> Result<()> {
    let cwd = current_request_cwd()?;
    let attachments = collect_image_attachments(&cwd, &args.images)?;
    launch_chat_session(
        storage,
        args.alias,
        None,
        None,
        resolve_thinking_level(storage, args.thinking)?,
        args.mode.map(Into::into),
        attachments,
        args.permissions.map(Into::into),
        args.no_tui,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn interactive_session(
    storage: &Storage,
    mut alias: Option<String>,
    mut session_id: Option<String>,
    initial_prompt: Option<String>,
    mut thinking_level: Option<ThinkingLevel>,
    mut task_mode: Option<TaskMode>,
    mut attachments: Vec<InputAttachment>,
    mut permission_preset: Option<PermissionPreset>,
) -> Result<()> {
    let mut client = ensure_daemon(storage).await?;
    let mut cwd =
        load_session_cwd(storage, session_id.as_deref())?.unwrap_or(current_request_cwd()?);
    if task_mode.is_none() {
        task_mode = load_session_task_mode(storage, session_id.as_deref())?;
    }
    let mut last_output = load_last_assistant_output(storage, session_id.as_deref())?;
    let mut requested_model =
        resolve_session_model_override(storage, session_id.as_deref(), alias.as_deref())?;
    if thinking_level.is_none() {
        thinking_level = storage.load_config()?.thinking_level;
    }
    if permission_preset.is_none() {
        permission_preset = Some(storage.load_config()?.permission_preset);
    }
    println!(
        "Interactive chat. Use /help for commands, /model or Ctrl+P for alias/main switching, /provider for provider switching, /mode to switch between build and daily presets, /onboard for a fresh setup reset, and /thinking to adjust reasoning."
    );

    if let Some(prompt) = normalize_prompt_input(initial_prompt)? {
        let response = execute_prompt(
            &client,
            prompt,
            alias.clone(),
            requested_model.clone(),
            session_id.clone(),
            cwd.clone(),
            thinking_level,
            task_mode,
            attachments.clone(),
            permission_preset,
            None,
            false,
        )
        .await?;
        session_id = Some(response.session_id.clone());
        requested_model =
            resolve_requested_model_override(storage, alias.as_deref(), &response.model)?;
        last_output = Some(response.response.clone());
        println!("\n{}\n", response.response);
    }

    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(shell_command) = line.strip_prefix('!') {
            match run_bang_command(storage, shell_command.trim(), &mut cwd).await {
                Ok(output) => {
                    if !output.is_empty() {
                        println!("{output}");
                    }
                }
                Err(error) => println!("error: {error:#}"),
            }
            continue;
        }

        if line.starts_with('/') {
            match parse_interactive_command(line) {
                Ok(Some(InteractiveCommand::Exit)) => break,
                Ok(Some(command)) => {
                    let command_result: Result<()> = async {
                        match command {
                            InteractiveCommand::Exit => unreachable!(),
                            InteractiveCommand::Help => print_interactive_help(),
                            InteractiveCommand::Status => {
                                print_interactive_status(
                                    storage,
                                    &client,
                                    alias.as_deref(),
                                    requested_model.as_deref(),
                                    session_id.as_deref(),
                                    thinking_level,
                                    task_mode,
                                    permission_preset,
                                    &attachments,
                                    cwd.as_path(),
                                )
                                .await?;
                            }
                            InteractiveCommand::ConfigShow => {
                                println!(
                                    "Settings:\n  /config opens the categorized settings menu in the TUI.\n  /dashboard opens the localhost web control room.\n  /model opens the alias/main switcher, /provider switches logged-in providers, and /mode, /thinking, and /permissions remain quick shortcuts."
                                );
                            }
                            InteractiveCommand::DashboardOpen => {
                                dashboard_command(
                                    storage,
                                    DashboardArgs {
                                        no_open: false,
                                        print_url: true,
                                    },
                                )
                                .await?;
                            }
                            InteractiveCommand::TelegramsShow => {
                                let connectors = load_telegram_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No telegram connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} chats={} users={} alias={} model={} last_update_id={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_i64_list(&connector.allowed_chat_ids),
                                            format_i64_list(&connector.allowed_user_ids),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .last_update_id
                                                .map(|value| value.to_string())
                                                .unwrap_or_else(|| "-".to_string()),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::DiscordsShow => {
                                let connectors = load_discord_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No discord connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_string_list(&connector.monitored_channel_ids),
                                            format_string_list(&connector.allowed_channel_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.channel_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::SlacksShow => {
                                let connectors = load_slack_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No slack connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_string_list(&connector.monitored_channel_ids),
                                            format_string_list(&connector.allowed_channel_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.channel_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::SignalsShow => {
                                let connectors = load_signal_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No signal connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} account={} cli_path={} groups={} allowed_groups={} users={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            connector.account,
                                            connector
                                                .cli_path
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "signal-cli".to_string()),
                                            format_string_list(&connector.monitored_group_ids),
                                            format_string_list(&connector.allowed_group_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::HomeAssistantsShow => {
                                let connectors = load_home_assistant_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No Home Assistant connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} token={} base_url={} monitored_entities={} service_domains={} service_entities={} tracked_entities={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.access_token_keychain_account.is_some(),
                                            connector.base_url,
                                            format_string_list(&connector.monitored_entity_ids),
                                            format_string_list(&connector.allowed_service_domains),
                                            format_string_list(&connector.allowed_service_entity_ids),
                                            connector.entity_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::DiscordApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Discord, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::DiscordApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved discord pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::DiscordReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected discord pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::SlackApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Slack, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::SlackApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved slack pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::SlackReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected slack pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::TelegramApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Telegram, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::TelegramApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved telegram pairing={} connector={} chat={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::TelegramReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected telegram pairing={} connector={} chat={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::WebhooksShow => {
                                let webhooks = load_webhook_connectors(storage).await?;
                                if webhooks.is_empty() {
                                    println!("No webhook connectors configured.");
                                } else {
                                    for connector in webhooks {
                                        println!(
                                            "{} [{}] enabled={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::InboxesShow => {
                                let inboxes = load_inbox_connectors(storage).await?;
                                if inboxes.is_empty() {
                                    println!("No inbox connectors configured.");
                                } else {
                                    for connector in inboxes {
                                        println!(
                                            "{} [{}] enabled={} delete_after_read={} alias={} model={} path={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.delete_after_read,
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector.path.display(),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::AutopilotShow => {
                                let status: AutopilotConfig =
                                    client.get("/v1/autopilot/status").await?;
                                println!("{}", autopilot_summary(&status));
                            }
                            InteractiveCommand::AutopilotEnable => {
                                let status: AutopilotConfig = client
                                    .put(
                                        "/v1/autopilot/status",
                                        &AutopilotUpdateRequest {
                                            state: Some(AutopilotState::Enabled),
                                            max_concurrent_missions: None,
                                            wake_interval_seconds: None,
                                            allow_background_shell: None,
                                            allow_background_network: None,
                                            allow_background_self_edit: None,
                                        },
                                    )
                                    .await?;
                                println!("{}", autopilot_summary(&status));
                            }
                            InteractiveCommand::AutopilotPause => {
                                let status: AutopilotConfig = client
                                    .put(
                                        "/v1/autopilot/status",
                                        &AutopilotUpdateRequest {
                                            state: Some(AutopilotState::Paused),
                                            max_concurrent_missions: None,
                                            wake_interval_seconds: None,
                                            allow_background_shell: None,
                                            allow_background_network: None,
                                            allow_background_self_edit: None,
                                        },
                                    )
                                    .await?;
                                println!("{}", autopilot_summary(&status));
                            }
                            InteractiveCommand::AutopilotResume => {
                                let status: AutopilotConfig = client
                                    .put(
                                        "/v1/autopilot/status",
                                        &AutopilotUpdateRequest {
                                            state: Some(AutopilotState::Enabled),
                                            max_concurrent_missions: None,
                                            wake_interval_seconds: None,
                                            allow_background_shell: None,
                                            allow_background_network: None,
                                            allow_background_self_edit: None,
                                        },
                                    )
                                    .await?;
                                println!("{}", autopilot_summary(&status));
                            }
                            InteractiveCommand::MissionsShow => {
                                let missions: Vec<Mission> = client.get("/v1/missions").await?;
                                for mission in missions {
                                    println!(
                                        "{} [{:?}] {} wake_at={} repeat={} watch={} retries={}/{}",
                                        mission.id,
                                        mission.status,
                                        mission.title,
                                        mission
                                            .wake_at
                                            .map(|value| value.to_rfc3339())
                                            .unwrap_or_else(|| "-".to_string()),
                                        mission
                                            .repeat_interval_seconds
                                            .map(|value| format!("{value}s"))
                                            .unwrap_or_else(|| "-".to_string()),
                                        mission
                                            .watch_path
                                            .as_deref()
                                            .map(|value| value.display().to_string())
                                            .unwrap_or_else(|| "-".to_string()),
                                        mission.retries,
                                        mission.max_retries
                                    );
                                }
                            }
                            InteractiveCommand::EventsShow(limit) => {
                                let events: Vec<agent_core::LogEntry> = client
                                    .get(&format!("/v1/events?limit={limit}"))
                                    .await?;
                                for entry in events {
                                    print_log_entry(&entry);
                                }
                            }
                            InteractiveCommand::Schedule {
                                after_seconds,
                                title,
                            } => {
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Scheduled;
                                mission.wake_at = Some(
                                    chrono::Utc::now()
                                        + chrono::Duration::seconds(after_seconds as i64),
                                );
                                mission.wake_trigger = Some(agent_core::WakeTrigger::Timer);
                                mission.workspace_key =
                                    Some(cwd.display().to_string());
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} wake_at={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .wake_at
                                        .map(|value| value.to_rfc3339())
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::Repeat {
                                every_seconds,
                                title,
                            } => {
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Scheduled;
                                mission.wake_at = Some(
                                    chrono::Utc::now()
                                        + chrono::Duration::seconds(every_seconds as i64),
                                );
                                mission.repeat_interval_seconds = Some(every_seconds);
                                mission.wake_trigger = Some(agent_core::WakeTrigger::Timer);
                                mission.workspace_key =
                                    Some(cwd.display().to_string());
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} wake_at={} repeat={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .wake_at
                                        .map(|value| value.to_rfc3339())
                                        .unwrap_or_else(|| "-".to_string()),
                                    mission
                                        .repeat_interval_seconds
                                        .map(|value| format!("{value}s"))
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::Watch { path, title } => {
                                let watch_path = resolve_watch_path(Some(path.as_path()), &cwd)?
                                    .ok_or_else(|| anyhow!("watch path is required"))?;
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Waiting;
                                mission.wake_trigger = Some(WakeTrigger::FileChange);
                                mission.workspace_key = Some(cwd.display().to_string());
                                mission.watch_path = Some(watch_path);
                                mission.watch_recursive = true;
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} watch={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .watch_path
                                        .as_deref()
                                        .map(|value| value.display().to_string())
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::ProfileShow => {
                                let memories = load_profile_memories(storage, 20).await?;
                                println!("{}", format_memory_records(&memories));
                            }
                            InteractiveCommand::MemoryReviewShow => {
                                let memories = load_memory_review_queue(storage, 20).await?;
                                println!("{}", format_memory_records(&memories));
                            }
                            InteractiveCommand::MemoryRebuild { session_id } => {
                                let response: MemoryRebuildResponse = client
                                    .post(
                                        "/v1/memory/rebuild",
                                        &MemoryRebuildRequest {
                                            session_id,
                                            recompute_embeddings: false,
                                        },
                                    )
                                    .await?;
                                println!(
                                    "generated_at={} sessions_scanned={} observations_scanned={} memories_upserted={} embeddings_refreshed={}",
                                    response.generated_at,
                                    response.sessions_scanned,
                                    response.observations_scanned,
                                    response.memories_upserted,
                                    response.embeddings_refreshed
                                );
                            }
                            InteractiveCommand::MemoryShow(query) => {
                                if let Some(query) = query {
                                    let result: MemorySearchResponse = client
                                        .post(
                                            "/v1/memory/search",
                                            &MemorySearchQuery {
                                                query,
                                                limit: Some(10),
                                                workspace_key: Some(cwd.display().to_string()),
                                                provider_id: None,
                                                review_statuses: Vec::new(),
                                                include_superseded: false,
                                            },
                                        )
                                        .await?;
                                    if !result.memories.is_empty() {
                                        println!("{}", format_memory_records(&result.memories));
                                    }
                                    if !result.transcript_hits.is_empty() {
                                        if !result.memories.is_empty() {
                                            println!();
                                        }
                                        println!(
                                            "{}",
                                            format_session_search_hits(&result.transcript_hits)
                                        );
                                    }
                                } else {
                                    let memories: Vec<MemoryRecord> =
                                        client.get("/v1/memory?limit=10").await?;
                                    println!("{}", format_memory_records(&memories));
                                }
                            }
                            InteractiveCommand::MemoryApprove { id, note } => {
                                let memory = update_memory_review_status(
                                    storage,
                                    &id,
                                    MemoryReviewStatus::Accepted,
                                    note,
                                )
                                .await?;
                                println!("approved memory={} subject={}", memory.id, memory.subject);
                            }
                            InteractiveCommand::MemoryReject { id, note } => {
                                let memory = update_memory_review_status(
                                    storage,
                                    &id,
                                    MemoryReviewStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!("rejected memory={} subject={}", memory.id, memory.subject);
                            }
                            InteractiveCommand::Skills(command) => match command {
                                InteractiveSkillCommand::Show(status) => {
                                    let drafts = load_skill_drafts(storage, 20, status).await?;
                                    println!("{}", format_skill_drafts(&drafts));
                                }
                                InteractiveSkillCommand::Publish(id) => {
                                    let draft = update_skill_draft_status(
                                        storage,
                                        &id,
                                        SkillDraftStatus::Published,
                                    )
                                    .await?;
                                    println!(
                                        "published skill draft={} title={}",
                                        draft.id, draft.title
                                    );
                                }
                                InteractiveSkillCommand::Reject(id) => {
                                    let draft = update_skill_draft_status(
                                        storage,
                                        &id,
                                        SkillDraftStatus::Rejected,
                                    )
                                    .await?;
                                    println!(
                                        "rejected skill draft={} title={}",
                                        draft.id, draft.title
                                    );
                                }
                            },
                            InteractiveCommand::Remember(content) => {
                                let subject = manual_memory_subject(&content);
                                let memory: MemoryRecord = client
                                    .post(
                                        "/v1/memory",
                                        &MemoryUpsertRequest {
                                            kind: MemoryKind::Note,
                                            scope: MemoryScope::Global,
                                            subject,
                                            content,
                                            confidence: Some(100),
                                            source_session_id: session_id.clone(),
                                            source_message_id: None,
                                            provider_id: None,
                                            workspace_key: Some(cwd.display().to_string()),
                                            evidence_refs: Vec::new(),
                                            tags: vec!["manual".to_string()],
                                            identity_key: None,
                                            observation_source: None,
                                            review_status: Some(MemoryReviewStatus::Accepted),
                                            review_note: None,
                                            reviewed_at: None,
                                            supersedes: None,
                                        },
                                    )
                                    .await?;
                                println!("memory={} subject={}", memory.id, memory.subject);
                            }
                            InteractiveCommand::Forget(id) => {
                                let _: serde_json::Value =
                                    client.delete(&format!("/v1/memory/{id}")).await?;
                                println!("forgot memory={id}");
                            }
                            InteractiveCommand::PermissionsShow => {
                                print_permissions_status(storage, &client).await?;
                            }
                            InteractiveCommand::PermissionsSet(new_preset) => {
                                let next_preset = match new_preset {
                                    Some(preset) => preset,
                                    None => storage.load_config()?.permission_preset,
                                };
                                let updated: PermissionPreset = client
                                    .put(
                                        "/v1/permissions",
                                        &PermissionUpdateRequest {
                                            permission_preset: next_preset,
                                        },
                                    )
                                    .await?;
                                permission_preset = Some(updated);
                                println!("permission_preset={}", permission_summary(updated));
                            }
                            InteractiveCommand::Attach(path) => {
                                let mut new = collect_image_attachments(&cwd, &[path])?;
                                attachments.append(&mut new);
                                println!("attachments={}", attachments.len());
                            }
                            InteractiveCommand::AttachmentsShow => {
                                if attachments.is_empty() {
                                    println!("attachments=(none)");
                                } else {
                                    for attachment in &attachments {
                                        println!("{}", attachment.path.display());
                                    }
                                }
                            }
                            InteractiveCommand::AttachmentsClear => {
                                attachments.clear();
                                println!("attachments cleared");
                            }
                            InteractiveCommand::New => {
                                session_id = None;
                                last_output = None;
                                requested_model = None;
                                println!("Started a new chat session.");
                            }
                            InteractiveCommand::Clear => {
                                clear_terminal();
                                session_id = None;
                                last_output = None;
                                requested_model = None;
                                println!("Started a new chat session.");
                            }
                            InteractiveCommand::Diff => {
                                println!("{}", build_uncommitted_diff()?);
                            }
                            InteractiveCommand::Copy => {
                                let text = last_output.as_deref().ok_or_else(|| {
                                    anyhow!("no assistant output available to copy")
                                })?;
                                copy_to_clipboard(text)?;
                                println!("Copied the latest assistant output to the clipboard.");
                            }
                            InteractiveCommand::Compact => {
                                let current_session = session_id
                                    .as_deref()
                                    .ok_or_else(|| anyhow!("no active session to compact"))?;
                                let transcript = load_session_for_command(
                                    storage,
                                    Some(current_session.to_string()),
                                    false,
                                    false,
                                )?;
                                let prompt = build_compact_prompt(&transcript)?;
                                let response = execute_prompt(
                                    &client,
                                    prompt,
                                    alias.clone(),
                                    requested_model.clone(),
                                    None,
                                    cwd.clone(),
                                    thinking_level,
                                    task_mode,
                                    Vec::new(),
                                    permission_preset,
                                    None,
                                    true,
                                )
                                .await?;
                                let new_session_id =
                                    compact_session(storage, &transcript, &response.response)?;
                                session_id = Some(new_session_id.clone());
                                println!(
                                    "Compacted session {} -> {}",
                                    transcript.session.id, new_session_id
                                );
                            }
                            InteractiveCommand::Init => {
                                let path = cwd.join("AGENTS.md");
                                if init_agents_file(&path)? {
                                    println!("Initialized {}", path.display());
                                } else {
                                    println!(
                                        "{} already exists; leaving it unchanged.",
                                        path.display()
                                    );
                                }
                            }
                            InteractiveCommand::Onboard => {
                                run_onboarding_reset(storage, true).await?;
                                client = ensure_daemon(storage).await?;
                                let config = storage.load_config()?;
                                alias = config.main_agent_alias.clone();
                                session_id = None;
                                requested_model = None;
                                thinking_level = config.thinking_level;
                                task_mode = None;
                                permission_preset = Some(config.permission_preset);
                                attachments.clear();
                                last_output = None;
                                cwd = current_request_cwd()?;
                                println!(
                                    "Onboarding reset complete. Started fresh setup with main alias {}.",
                                    config.main_agent_alias.as_deref().unwrap_or("(not configured)")
                                );
                            }
                            InteractiveCommand::ModelShow => {
                                println!(
                                    "{}",
                                    interactive_model_choices_text(
                                        storage,
                                        alias.as_deref(),
                                        requested_model.as_deref(),
                                    )
                                    .await?
                                );
                            }
                            InteractiveCommand::ProviderShow => {
                                println!(
                                    "{}",
                                    interactive_provider_choices_text(storage, alias.as_deref())?
                                );
                            }
                            InteractiveCommand::ModelSet(selection) => {
                                match resolve_interactive_model_selection(
                                    storage,
                                    alias.as_deref(),
                                    &selection,
                                )
                                .await?
                                {
                                    InteractiveModelSelection::Alias(new_alias) => {
                                        alias = Some(new_alias.clone());
                                        requested_model = None;
                                        println!("model alias set to {new_alias}");
                                    }
                                    InteractiveModelSelection::Explicit(model_id) => {
                                        requested_model = Some(model_id.clone());
                                        println!("model override set to {model_id}");
                                    }
                                }
                            }
                            InteractiveCommand::ProviderSet(selection) => {
                                let new_alias = resolve_interactive_provider_selection(
                                    storage,
                                    alias.as_deref(),
                                    &selection,
                                )?;
                                let config = storage.load_config()?;
                                let summary = config
                                    .alias_target_summary(&new_alias)
                                    .ok_or_else(|| anyhow!("unknown alias '{new_alias}'"))?;
                                alias = Some(new_alias.clone());
                                requested_model = None;
                                println!(
                                    "provider set to {} via alias {} ({})",
                                    summary.provider_display_name, summary.alias, summary.model
                                );
                            }
                            InteractiveCommand::ThinkingShow => {
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::ModeShow => {
                                println!("mode={}", task_mode_label(task_mode));
                            }
                            InteractiveCommand::ThinkingSet(new_level) => {
                                thinking_level = new_level;
                                persist_thinking_level(storage, thinking_level)?;
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::ModeSet(new_mode) => {
                                task_mode = new_mode;
                                println!("mode={}", task_mode_label(task_mode));
                            }
                            InteractiveCommand::Fast => {
                                thinking_level = Some(ThinkingLevel::Minimal);
                                persist_thinking_level(storage, thinking_level)?;
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::Rename(new_title) => {
                                let current_session = session_id
                                    .as_deref()
                                    .ok_or_else(|| anyhow!("no active session to rename"))?;
                                let title = match new_title {
                                    Some(title) => title,
                                    None => Input::<String>::with_theme(&ColorfulTheme::default())
                                        .with_prompt("Session title")
                                        .interact_text()?,
                                };
                                let title = title.trim();
                                if title.is_empty() {
                                    bail!("session title cannot be empty");
                                }
                                storage.rename_session(current_session, title)?;
                                println!("renamed session={} title={}", current_session, title);
                            }
                            InteractiveCommand::Review(custom_prompt) => {
                                let prompt = build_uncommitted_review_prompt(custom_prompt)?;
                                let response = execute_prompt(
                                    &client,
                                    prompt,
                                    alias.clone(),
                                    requested_model.clone(),
                                    session_id.clone(),
                                    cwd.clone(),
                                    thinking_level,
                                    Some(TaskMode::Build),
                                    attachments.clone(),
                                    permission_preset,
                                    None,
                                    false,
                                )
                                .await?;
                                session_id = Some(response.session_id.clone());
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &response.model,
                                )?;
                                last_output = Some(response.response.clone());
                                println!("\n{}\n", response.response);
                            }
                            InteractiveCommand::Resume(target) => {
                                let transcript = load_transcript_for_interactive_resume(
                                    storage,
                                    target.as_deref(),
                                )?;
                                println!(
                                    "Resumed session={} title={} alias={} provider={} model={} mode={}",
                                    transcript.session.id,
                                    transcript.session.title.as_deref().unwrap_or("(untitled)"),
                                    transcript.session.alias,
                                    transcript.session.provider_id,
                                    transcript.session.model,
                                    task_mode_label(transcript.session.task_mode),
                                );
                                last_output = latest_assistant_output_from_transcript(&transcript);
                                alias = Some(transcript.session.alias.clone());
                                session_id = Some(transcript.session.id.clone());
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &transcript.session.model,
                                )?;
                                task_mode = transcript.session.task_mode;
                                cwd = transcript
                                    .session
                                    .cwd
                                    .clone()
                                    .unwrap_or_else(|| cwd.clone());
                                attachments.clear();
                            }
                            InteractiveCommand::Fork(target) => {
                                let transcript = load_transcript_for_interactive_fork(
                                    storage,
                                    session_id.as_deref(),
                                    target.as_deref(),
                                )?;
                                let new_session_id = fork_session(storage, &transcript)?;
                                println!(
                                    "Forked session {} ({}) -> {}",
                                    transcript.session.id,
                                    transcript.session.title.as_deref().unwrap_or("(untitled)"),
                                    new_session_id
                                );
                                last_output = latest_assistant_output_from_transcript(&transcript);
                                alias = Some(transcript.session.alias.clone());
                                session_id = Some(new_session_id);
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &transcript.session.model,
                                )?;
                                task_mode = transcript.session.task_mode;
                                cwd = transcript
                                    .session
                                    .cwd
                                    .clone()
                                    .unwrap_or_else(|| cwd.clone());
                            }
                        }
                        Ok(())
                    }
                    .await;
                    if let Err(error) = command_result {
                        println!("error: {error:#}");
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    println!("error: {error:#}");
                    continue;
                }
            }
        }

        let response = execute_prompt(
            &client,
            line.to_string(),
            alias.clone(),
            requested_model.clone(),
            session_id.clone(),
            cwd.clone(),
            thinking_level,
            task_mode,
            attachments.clone(),
            permission_preset,
            None,
            false,
        )
        .await?;
        session_id = Some(response.session_id.clone());
        requested_model =
            resolve_requested_model_override(storage, alias.as_deref(), &response.model)?;
        last_output = Some(response.response.clone());
        println!("\n{}\n", response.response);
    }

    Ok(())
}

async fn review_command(storage: &Storage, args: ReviewArgs) -> Result<()> {
    ensure_onboarded(storage).await?;
    let prompt = build_review_prompt(&args)?;
    let client = ensure_daemon(storage).await?;
    let thinking_level = resolve_thinking_level(storage, args.thinking)?;
    let response = execute_prompt(
        &client,
        prompt,
        None,
        None,
        None,
        current_request_cwd()?,
        thinking_level,
        Some(TaskMode::Build),
        Vec::new(),
        None,
        None,
        false,
    )
    .await?;
    println!("{}", response.response);
    println!(
        "\nsession={} alias={} provider={} model={}",
        response.session_id, response.alias, response.provider_id, response.model
    );
    Ok(())
}

async fn resume_command(storage: &Storage, args: ResumeArgs) -> Result<()> {
    let transcript = load_session_for_command(storage, args.session_id, args.last, args.all)?;
    println!(
        "Resuming session={} title={} alias={} provider={} model={} mode={}",
        transcript.session.id,
        transcript.session.title.as_deref().unwrap_or("(untitled)"),
        transcript.session.alias,
        transcript.session.provider_id,
        transcript.session.model,
        task_mode_label(transcript.session.task_mode),
    );
    launch_chat_session(
        storage,
        Some(transcript.session.alias),
        Some(transcript.session.id),
        args.prompt,
        resolve_thinking_level(storage, args.thinking)?,
        transcript.session.task_mode,
        Vec::new(),
        None,
        false,
    )
    .await
}

async fn fork_command(storage: &Storage, args: ForkArgs) -> Result<()> {
    let transcript = load_session_for_command(storage, args.session_id, args.last, args.all)?;
    let new_session_id = fork_session(storage, &transcript)?;
    println!(
        "Forked session {} ({}) -> {}",
        transcript.session.id,
        transcript.session.title.as_deref().unwrap_or("(untitled)"),
        new_session_id
    );
    launch_chat_session(
        storage,
        Some(transcript.session.alias),
        Some(new_session_id),
        args.prompt,
        resolve_thinking_level(storage, args.thinking)?,
        transcript.session.task_mode,
        Vec::new(),
        None,
        false,
    )
    .await
}

fn completion_command(args: CompletionArgs) {
    let mut command = Cli::command();
    generate(
        args.shell,
        &mut command,
        PRIMARY_COMMAND_NAME,
        &mut io::stdout(),
    );
}

async fn logout_command(storage: &Storage, args: LogoutArgs) -> Result<()> {
    let mut config = storage.load_config()?;
    let provider_ids = determine_logout_targets(&config, &args)?;
    let mut removed = 0usize;

    for provider_id in provider_ids {
        let Some(provider) = config
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        else {
            continue;
        };

        if let Some(account) = provider.keychain_account.take() {
            delete_secret(&account)?;
            removed += 1;
        }
    }

    storage.save_config(&config)?;
    println!("Removed stored credentials for {} provider(s).", removed);
    Ok(())
}

pub(crate) async fn run_onboarding_reset(
    storage: &Storage,
    require_confirmation: bool,
) -> Result<()> {
    if require_confirmation {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            bail!(
                "reset is destructive; rerun with `{} reset --yes` in an interactive terminal",
                PRIMARY_COMMAND_NAME
            );
        }

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("This will wipe saved config, sessions, logs, and credentials. Continue?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Reset cancelled.");
            return Ok(());
        }
    }

    stop_daemon_for_reset(storage).await?;

    let config = storage.load_config()?;
    storage.sync_autostart(&current_executable_path()?, &[INTERNAL_DAEMON_ARG], false)?;

    let keychain_accounts = configured_keychain_accounts(&config);
    let mut removed_credentials = 0usize;
    let mut credential_warnings = Vec::new();
    for account in keychain_accounts {
        match delete_secret(&account) {
            Ok(()) => removed_credentials += 1,
            Err(error) => credential_warnings.push(format!("{account}: {error}")),
        }
    }

    storage.reset_all()?;

    println!(
        "Reset complete. Cleared configuration, sessions, logs, and {} credential entry(s).",
        removed_credentials
    );
    for warning in credential_warnings {
        println!("warning: failed to delete keychain entry {warning}");
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!(
            "Run `{} setup` in an interactive terminal to complete onboarding again.",
            PRIMARY_COMMAND_NAME
        );
        return Ok(());
    }

    println!();
    println!("Restarting onboarding.");
    setup(storage).await
}

async fn reset_command(storage: &Storage, args: ResetArgs) -> Result<()> {
    run_onboarding_reset(storage, !args.yes).await
}

async fn ensure_daemon(storage: &Storage) -> Result<DaemonClient> {
    if let Some(client) = try_daemon(storage).await? {
        return Ok(client);
    }

    start_daemon_process()?;
    let config = storage.load_config()?;
    wait_for_daemon(&config).await?;
    Ok(DaemonClient::new(&config))
}

async fn stop_daemon_for_reset(storage: &Storage) -> Result<()> {
    let Some(client) = try_daemon(storage).await? else {
        return Ok(());
    };

    let _: serde_json::Value = client.post("/v1/shutdown", &serde_json::json!({})).await?;
    for _ in 0..20 {
        if try_daemon(storage).await?.is_none() {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }

    bail!(
        "daemon did not stop in time; run `{} daemon stop` and retry reset",
        PRIMARY_COMMAND_NAME
    )
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

async fn try_daemon(storage: &Storage) -> Result<Option<DaemonClient>> {
    let config = storage.load_config()?;
    let client = DaemonClient::new(&config);
    if client.get::<DaemonStatus>("/v1/status").await.is_ok() {
        Ok(Some(client))
    } else {
        Ok(None)
    }
}

async fn wait_for_daemon(config: &AppConfig) -> Result<()> {
    let client = DaemonClient::new(config);
    for _ in 0..20 {
        if client.get::<DaemonStatus>("/v1/status").await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(300)).await;
    }

    bail!("daemon did not become ready in time")
}

fn start_daemon_process() -> Result<()> {
    let current_exe = current_executable_path()?;
    let mut command = Command::new(&current_exe);
    command
        .arg(INTERNAL_DAEMON_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().with_context(|| {
        format!(
            "failed to start daemon using {} {}",
            current_exe.display(),
            INTERNAL_DAEMON_ARG
        )
    })?;
    Ok(())
}

fn current_executable_path() -> Result<PathBuf> {
    std::env::current_exe().context("failed to locate current executable")
}

#[allow(clippy::too_many_arguments)]
async fn execute_prompt(
    client: &DaemonClient,
    prompt: String,
    alias: Option<String>,
    requested_model: Option<String>,
    session_id: Option<String>,
    cwd: PathBuf,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    attachments: Vec<InputAttachment>,
    permission_preset: Option<PermissionPreset>,
    output_schema_json: Option<String>,
    ephemeral: bool,
) -> Result<RunTaskResponse> {
    client
        .post(
            "/v1/run",
            &RunTaskRequest {
                prompt,
                alias,
                requested_model,
                session_id,
                cwd: Some(cwd),
                thinking_level,
                task_mode,
                attachments,
                permission_preset,
                output_schema_json,
                ephemeral,
            },
        )
        .await
}

fn current_request_cwd() -> Result<PathBuf> {
    std::env::current_dir().context("failed to resolve current working directory")
}

fn load_session_cwd(storage: &Storage, session_id: Option<&str>) -> Result<Option<PathBuf>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    Ok(storage
        .get_session(session_id)?
        .and_then(|session| session.cwd))
}

fn load_session_task_mode(storage: &Storage, session_id: Option<&str>) -> Result<Option<TaskMode>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    Ok(storage
        .get_session(session_id)?
        .and_then(|session| session.task_mode))
}

fn resolve_thinking_level(
    storage: &Storage,
    thinking: Option<ThinkingLevelArg>,
) -> Result<Option<ThinkingLevel>> {
    match thinking {
        Some(thinking) => Ok(Some(thinking.into())),
        None => Ok(storage.load_config()?.thinking_level),
    }
}

fn persist_thinking_level(storage: &Storage, thinking_level: Option<ThinkingLevel>) -> Result<()> {
    let mut config = storage.load_config()?;
    config.thinking_level = thinking_level;
    storage.save_config(&config)
}

fn resolve_mission_wake_at(
    after_seconds: Option<u64>,
    at: Option<&str>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    if after_seconds.is_some() && at.is_some() {
        bail!("use either --after-seconds or --at, not both");
    }

    if let Some(seconds) = after_seconds {
        return Ok(Some(
            chrono::Utc::now() + chrono::Duration::seconds(seconds as i64),
        ));
    }

    let Some(at) = at else {
        return Ok(None);
    };

    chrono::DateTime::parse_from_rfc3339(at)
        .map(|value| value.with_timezone(&chrono::Utc))
        .with_context(|| format!("invalid RFC3339 timestamp '{at}'"))
        .map(Some)
}

fn resolve_watch_path(path: Option<&Path>, cwd: &Path) -> Result<Option<PathBuf>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    Ok(Some(absolute))
}

fn event_feed_path(
    cursor: Option<&chrono::DateTime<chrono::Utc>>,
    limit: usize,
    wait_seconds: u64,
) -> String {
    let mut path = format!("/v1/events?limit={limit}&wait_seconds={wait_seconds}");
    if let Some(cursor) = cursor {
        let encoded: String =
            form_urlencoded::byte_serialize(cursor.to_rfc3339().as_bytes()).collect();
        path.push_str("&after=");
        path.push_str(&encoded);
    }
    path
}

fn print_log_entry(entry: &agent_core::LogEntry) {
    println!(
        "{} [{}] {} {}",
        entry.created_at, entry.level, entry.scope, entry.message
    );
}

fn thinking_level_label(level: Option<ThinkingLevel>) -> &'static str {
    match level {
        None => "default",
        Some(level) => level.as_str(),
    }
}

fn task_mode_label(mode: Option<TaskMode>) -> &'static str {
    match mode {
        None => "default",
        Some(mode) => mode.as_str(),
    }
}

fn autopilot_summary(config: &AutopilotConfig) -> String {
    format!(
        "autopilot={} interval={}s concurrency={} shell={} network={} self_edit={}",
        match config.state {
            AutopilotState::Disabled => "disabled",
            AutopilotState::Enabled => "enabled",
            AutopilotState::Paused => "paused",
        },
        config.wake_interval_seconds,
        config.max_concurrent_missions,
        config.allow_background_shell,
        config.allow_background_network,
        config.allow_background_self_edit
    )
}

fn manual_memory_subject(content: &str) -> String {
    let slug = content
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "memory".to_string()
    } else {
        format!("memory:{slug}")
    }
}

fn parse_permission_preset(value: &str) -> Result<PermissionPreset> {
    match value.to_ascii_lowercase().as_str() {
        "suggest" => Ok(PermissionPreset::Suggest),
        "auto-edit" | "auto_edit" | "autoedit" => Ok(PermissionPreset::AutoEdit),
        "full-auto" | "full_auto" | "fullauto" => Ok(PermissionPreset::FullAuto),
        _ => bail!("unknown permission preset '{value}'"),
    }
}

fn resolve_active_alias<'a>(
    config: &'a AppConfig,
    alias: Option<&'a str>,
) -> Result<&'a ModelAlias> {
    if let Some(alias) = alias {
        return config
            .get_alias(alias)
            .ok_or_else(|| anyhow!("unknown alias '{alias}'"));
    }
    config.main_alias()
}

fn resolved_requested_model<'a>(
    active_alias: &'a ModelAlias,
    requested_model: Option<&'a str>,
) -> &'a str {
    requested_model.unwrap_or(active_alias.model.as_str())
}

async fn print_permissions_status(storage: &Storage, client: &DaemonClient) -> Result<()> {
    let config = storage.load_config()?;
    let autonomy: agent_core::AutonomyProfile = client.get("/v1/autonomy/status").await?;
    let preset: PermissionPreset = client.get("/v1/permissions").await?;
    println!("{}", trust_summary(&config.trust_policy));
    println!("permission_preset={}", permission_summary(preset));
    println!("autonomy={}", autonomy_summary(autonomy.state));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn print_interactive_status(
    storage: &Storage,
    client: &DaemonClient,
    alias: Option<&str>,
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    permission_preset: Option<PermissionPreset>,
    attachments: &[InputAttachment],
    cwd: &Path,
) -> Result<()> {
    let config = storage.load_config()?;
    let current_session = session_id.and_then(|id| storage.get_session(id).ok().flatten());
    let active_alias = resolve_active_alias(&config, alias)?;
    let provider = config
        .resolve_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let selected_model = resolved_requested_model(active_alias, requested_model);
    let daemon_status: DaemonStatus = client.get("/v1/status").await?;
    println!("session={}", session_id.unwrap_or("(new)"));
    if let Some(session) = current_session {
        println!("title={}", session.title.as_deref().unwrap_or("(untitled)"));
    }
    println!("alias={}", active_alias.alias);
    println!("provider={}", provider.id);
    println!("model={}", selected_model);
    if let Some(main_target) = daemon_status.main_target.as_ref() {
        println!(
            "main={} ({}/{})",
            main_target.alias, main_target.provider_id, main_target.model
        );
    }
    println!("thinking={}", thinking_level_label(thinking_level));
    println!("mode={}", task_mode_label(task_mode));
    println!(
        "permission_preset={}",
        permission_summary(permission_preset.unwrap_or(config.permission_preset))
    );
    println!("attachments={}", attachments.len());
    println!("cwd={}", cwd.display());
    println!(
        "daemon={} auto_start={} autonomy={} autopilot={} active_missions={} memories={}",
        match daemon_status.persistence_mode {
            PersistenceMode::OnDemand => "on-demand",
            PersistenceMode::AlwaysOn => "always-on",
        },
        daemon_status.auto_start,
        autonomy_summary(daemon_status.autonomy.state),
        match daemon_status.autopilot.state {
            AutopilotState::Disabled => "disabled",
            AutopilotState::Enabled => "enabled",
            AutopilotState::Paused => "paused",
        },
        daemon_status.active_missions,
        daemon_status.memories
    );
    println!("{}", trust_summary(&config.trust_policy));
    Ok(())
}

async fn run_bang_command(storage: &Storage, command: &str, cwd: &mut PathBuf) -> Result<String> {
    if command.is_empty() {
        bail!("shell command is empty");
    }
    if command == "cd" {
        *cwd = home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        return Ok(format!("cwd={}", cwd.display()));
    }
    if let Some(target) = command.strip_prefix("cd ") {
        let target = target.trim();
        if target.is_empty() {
            bail!("cd target is empty");
        }
        let next = resolve_shell_cd_target(cwd, target)?;
        *cwd = next;
        return Ok(format!("cwd={}", cwd.display()));
    }

    let config = storage.load_config()?;
    if !allow_shell(&config.trust_policy, &config.autonomy) {
        bail!("shell access is disabled by the current trust policy");
    }
    execute_local_shell_command(command, cwd).await
}

fn resolve_shell_cd_target(current: &Path, target: &str) -> Result<PathBuf> {
    let expanded = if target == "~" || target.starts_with("~/") || target.starts_with("~\\") {
        let home = home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        if target.len() == 1 {
            home
        } else {
            home.join(&target[2..])
        }
    } else {
        PathBuf::from(target)
    };

    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        current.join(expanded)
    };
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("failed to access {}", resolved.display()))?;
    if !canonical.is_dir() {
        bail!("{} is not a directory", canonical.display());
    }
    Ok(canonical)
}

async fn execute_local_shell_command(command: &str, cwd: &Path) -> Result<String> {
    let mut process = if cfg!(windows) {
        let mut command_process = TokioCommand::new("powershell");
        command_process
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command);
        command_process
    } else {
        let mut command_process = TokioCommand::new("sh");
        command_process.arg("-lc").arg(command);
        command_process
    };
    process.kill_on_drop(true);
    process.current_dir(cwd);

    let output = timeout(Duration::from_secs(60), process.output())
        .await
        .context("shell command timed out")?
        .with_context(|| format!("failed to run shell command '{command}'"))?;

    let mut text = String::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        text.push_str(stdout.trim_end());
    }
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(stderr.trim_end());
    }
    if text.is_empty() {
        text = format!("exit={}", output.status);
    } else if !output.status.success() {
        text.push_str(&format!("\nexit={}", output.status));
    }
    Ok(truncate_for_prompt(text, 20_000))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn normalize_prompt_input(prompt: Option<String>) -> Result<Option<String>> {
    let Some(prompt) = prompt else {
        return Ok(None);
    };
    if prompt != "-" {
        return Ok(Some(prompt));
    }

    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read prompt from stdin")?;
    let prompt = buffer.trim().to_string();
    if prompt.is_empty() {
        bail!("no prompt provided via stdin");
    }
    Ok(Some(prompt))
}

fn truncate_for_prompt(mut text: String, max_len: usize) -> String {
    if text.len() <= max_len {
        return text;
    }
    text.truncate(max_len);
    text.push_str("\n\n[truncated]");
    text
}

fn determine_logout_targets(config: &AppConfig, args: &LogoutArgs) -> Result<Vec<String>> {
    if args.all {
        return Ok(config
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect());
    }
    if let Some(provider) = &args.provider {
        return Ok(vec![provider.clone()]);
    }
    if let Some(alias_name) = config.main_agent_alias.as_deref() {
        if let Some(alias) = config.get_alias(alias_name) {
            return Ok(vec![alias.provider_id.clone()]);
        }
    }
    bail!("no provider specified and no main provider configured")
}

fn parse_task(value: String) -> Result<SubAgentTask> {
    let (target, prompt) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("task must use target=prompt format"))?;
    Ok(SubAgentTask {
        prompt: prompt.trim().to_string(),
        target: Some(target.trim().to_string()),
        alias: None,
        provider_id: None,
        requested_model: None,
        cwd: None,
        thinking_level: None,
        task_mode: None,
        output_schema_json: None,
        strategy: None,
    })
}

async fn upsert_provider_with_optional_alias(
    storage: &Storage,
    request: ProviderUpsertRequest,
    main_alias: Option<String>,
    model: String,
) -> Result<()> {
    let provider_id = request.provider.id.clone();
    if let Some(client) = try_daemon(storage).await? {
        let _: ProviderConfig = client.post("/v1/providers", &request).await?;
        if let Some(alias) = main_alias {
            set_alias(&client, alias, provider_id.clone(), model, true).await?;
        }
    } else {
        let mut config = storage.load_config()?;
        apply_provider_request_locally(&mut config, &request)?;
        if let Some(alias) = main_alias {
            config.main_agent_alias = Some(alias.clone());
            config.upsert_alias(ModelAlias {
                alias,
                provider_id: provider_id.clone(),
                model,
                description: None,
            });
        }
        storage.save_config(&config)?;
    }

    println!("provider '{}' configured", provider_id);
    Ok(())
}

fn apply_provider_request_locally(
    config: &mut AppConfig,
    request: &ProviderUpsertRequest,
) -> Result<()> {
    let mut provider = request.provider.clone();
    if let Some(api_key) = &request.api_key {
        provider.keychain_account = Some(store_api_key(&provider.id, api_key)?);
    }
    if let Some(token) = &request.oauth_token {
        provider.keychain_account = Some(store_oauth_token(&provider.id, token)?);
    }
    config.upsert_provider(provider);
    Ok(())
}

fn apply_trust_update(policy: &mut TrustPolicy, update: &TrustUpdateRequest) {
    if let Some(allow_shell) = update.allow_shell {
        policy.allow_shell = allow_shell;
    }
    if let Some(allow_network) = update.allow_network {
        policy.allow_network = allow_network;
    }
    if let Some(allow_full_disk) = update.allow_full_disk {
        policy.allow_full_disk = allow_full_disk;
    }
    if let Some(allow_self_edit) = update.allow_self_edit {
        policy.allow_self_edit = allow_self_edit;
    }
    if let Some(path) = &update.trusted_path {
        if !policy.trusted_paths.contains(path) {
            policy.trusted_paths.push(path.clone());
        }
    }
}

fn resolve_main_alias(storage: &Storage, requested: Option<String>) -> Result<Option<String>> {
    let config = storage.load_config()?;
    Ok(default_main_alias(&config, requested))
}

fn default_main_alias(config: &AppConfig, requested: Option<String>) -> Option<String> {
    requested.or_else(|| {
        if config.main_agent_alias.is_none() && config.aliases.is_empty() {
            Some("main".to_string())
        } else {
            None
        }
    })
}

struct OAuthCallback {
    code: String,
    state: String,
}

struct BrowserCodeCallback {
    code: String,
}

async fn set_alias(
    client: &DaemonClient,
    alias: String,
    provider: String,
    model: String,
    set_main: bool,
) -> Result<()> {
    let payload = AliasUpsertRequest {
        alias: ModelAlias {
            alias: alias.clone(),
            provider_id: provider,
            model,
            description: None,
        },
        set_as_main: set_main,
    };
    let _: ModelAlias = client.post("/v1/aliases", &payload).await?;

    println!("alias '{}' configured", alias);
    Ok(())
}

#[derive(Clone)]
struct DaemonClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl DaemonClient {
    fn new(config: &AppConfig) -> Self {
        Self {
            base_url: format!("http://{}:{}", config.daemon.host, config.daemon.port),
            token: config.daemon.token.clone(),
            http: build_http_client(),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, Option::<&()>::None).await
    }

    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        self.request(Method::POST, path, Some(body)).await
    }

    async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        self.request(Method::PUT, path, Some(body)).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::DELETE, path, Option::<&()>::None)
            .await
    }

    async fn post_stream<B, T, F>(&self, path: &str, body: &B, mut on_event: F) -> Result<()>
    where
        B: Serialize,
        T: DeserializeOwned,
        F: FnMut(T) -> Result<()>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .request(Method::POST, &url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("daemon returned {}: {}", status, body);
        }
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.with_context(|| format!("failed to read streamed response from {url}"))?;
            buffer.extend_from_slice(&chunk);
            drain_ndjson_buffer(&mut buffer, false, &mut on_event)
                .with_context(|| format!("failed to parse streamed daemon response from {url}"))?;
        }
        drain_ndjson_buffer(&mut buffer, true, &mut on_event)
            .with_context(|| format!("failed to parse streamed daemon response from {url}"))?;
        Ok(())
    }

    async fn request<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.request(method, &url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("daemon returned {}: {}", status, body);
        }
        response
            .json::<T>()
            .await
            .with_context(|| format!("failed to parse daemon response from {url}"))
    }
}

fn drain_ndjson_buffer<T, F>(
    buffer: &mut Vec<u8>,
    flush_trailing: bool,
    on_event: &mut F,
) -> Result<()>
where
    T: DeserializeOwned,
    F: FnMut(T) -> Result<()>,
{
    while let Some(newline_index) = buffer.iter().position(|byte| *byte == b'\n') {
        let line_bytes = buffer[..newline_index].to_vec();
        buffer.drain(..=newline_index);
        let line = std::str::from_utf8(&line_bytes)?.trim();
        if line.is_empty() {
            continue;
        }
        let value = serde_json::from_str::<T>(line)?;
        on_event(value)?;
    }

    if flush_trailing {
        if buffer.iter().all(|byte| byte.is_ascii_whitespace()) {
            buffer.clear();
            return Ok(());
        }
        let trailing = std::str::from_utf8(buffer)?.trim();
        if trailing.is_empty() {
            buffer.clear();
            return Ok(());
        }
        let value = serde_json::from_str::<T>(trailing)?;
        on_event(value)?;
        buffer.clear();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        plugin_provider_id, AppConfig, AutonomyProfile, BraveConnectorConfig,
        DashboardLaunchResponse, DelegationConfig, DiscordSendResponse, GmailConnectorConfig,
        HomeAssistantServiceCallResponse, InstalledPluginConfig, MainTargetSummary,
        MemorySearchResponse, PermissionPreset, PluginCompatibility, PluginManifest,
        PluginPermissions, PluginProviderAdapterManifest, PluginSourceKind, ProviderKind,
        RunTaskResponse, SessionMessage, SessionSummary, SignalSendResponse, SlackSendResponse,
        PLUGIN_SCHEMA_VERSION,
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

    fn temp_storage() -> Storage {
        Storage::open_at(std::env::temp_dir().join(format!("nuclear-cli-test-{}", Uuid::new_v4())))
            .unwrap()
    }

    fn session_transcript_with_mode(storage: &Storage, task_mode: TaskMode) -> SessionTranscript {
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "local".to_string(),
            model: "qwen".to_string(),
            description: None,
        };
        storage
            .ensure_session("session-1", &alias, "local", "qwen", Some(task_mode))
            .unwrap();
        storage
            .append_message(&SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                "hello".to_string(),
                Some("local".to_string()),
                Some("qwen".to_string()),
            ))
            .unwrap();
        SessionTranscript {
            session: storage.get_session("session-1").unwrap().unwrap(),
            messages: storage.list_session_messages("session-1").unwrap(),
        }
    }

    #[test]
    fn load_session_task_mode_returns_persisted_mode() {
        let storage = temp_storage();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "local".to_string(),
            model: "qwen".to_string(),
            description: None,
        };
        storage
            .ensure_session("session-1", &alias, "local", "qwen", Some(TaskMode::Daily))
            .unwrap();

        assert_eq!(
            load_session_task_mode(&storage, Some("session-1")).unwrap(),
            Some(TaskMode::Daily)
        );
    }

    #[test]
    fn compact_session_preserves_task_mode() {
        let storage = temp_storage();
        let transcript = session_transcript_with_mode(&storage, TaskMode::Daily);

        let compacted_id = compact_session(&storage, &transcript, "Carry this forward").unwrap();
        let compacted = storage.get_session(&compacted_id).unwrap().unwrap();

        assert_eq!(compacted.task_mode, Some(TaskMode::Daily));
    }

    #[test]
    fn fork_session_preserves_task_mode() {
        let storage = temp_storage();
        let transcript = session_transcript_with_mode(&storage, TaskMode::Build);

        let forked_id = fork_session(&storage, &transcript).unwrap();
        let forked = storage.get_session(&forked_id).unwrap().unwrap();

        assert_eq!(forked.task_mode, Some(TaskMode::Build));
    }

    #[derive(Debug, Clone)]
    struct CapturedHttpRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: String,
    }

    #[derive(Debug, Clone)]
    struct MockHttpExpectation {
        method: &'static str,
        path: String,
        response_body: String,
        status_line: &'static str,
        content_type: &'static str,
    }

    impl MockHttpExpectation {
        fn json<T: Serialize>(method: &'static str, path: impl Into<String>, response: &T) -> Self {
            Self {
                method,
                path: path.into(),
                response_body: serde_json::to_string(response).unwrap(),
                status_line: "200 OK",
                content_type: "application/json",
            }
        }
    }

    struct MockHttpServer {
        origin: String,
        requests: Arc<Mutex<Vec<CapturedHttpRequest>>>,
        handle: JoinHandle<Result<()>>,
    }

    impl MockHttpServer {
        async fn finish(self) -> Result<Vec<CapturedHttpRequest>> {
            self.handle.await??;
            Ok(self.requests.lock().unwrap().clone())
        }
    }

    async fn spawn_mock_http_server(
        expectations: Vec<MockHttpExpectation>,
        expected_auth: Option<String>,
    ) -> MockHttpServer {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_clone = Arc::clone(&requests);
        let expected_auth_clone = expected_auth.clone();
        let mut queue = VecDeque::from(expectations);
        let handle = tokio::spawn(async move {
            while let Some(expected) = queue.pop_front() {
                let (mut stream, _) = listener.accept().await?;
                let raw = read_local_http_request(&mut stream).await?;
                let captured = parse_http_request(&raw);
                assert_eq!(captured.method, expected.method);
                assert_eq!(captured.path, expected.path);
                if let Some(expected_auth) = expected_auth_clone.as_deref() {
                    assert_eq!(
                        captured.headers.get("authorization").map(String::as_str),
                        Some(expected_auth)
                    );
                }
                requests_clone.lock().unwrap().push(captured);
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    expected.status_line,
                    expected.content_type,
                    expected.response_body.len(),
                    expected.response_body
                );
                stream.write_all(response.as_bytes()).await?;
            }
            Ok(())
        });

        MockHttpServer {
            origin: format!("http://{addr}"),
            requests,
            handle,
        }
    }

    async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
        let mut buffer = Vec::new();
        let mut header_end = None;
        let mut content_length = 0usize;
        loop {
            let mut chunk = [0u8; 1024];
            let bytes = stream.read(&mut chunk).await?;
            if bytes == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..bytes]);
            if header_end.is_none() {
                if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(index + 4);
                    let headers = String::from_utf8_lossy(&buffer[..index + 4]);
                    for line in headers.lines() {
                        if let Some((name, value)) = line.split_once(':') {
                            if name.eq_ignore_ascii_case("content-length") {
                                content_length = value.trim().parse::<usize>().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            if let Some(end) = header_end {
                if buffer.len() >= end + content_length {
                    break;
                }
            }
        }
        Ok(String::from_utf8(buffer)?)
    }

    fn parse_http_request(raw: &str) -> CapturedHttpRequest {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts.next().unwrap_or_default().to_string();
        let path = request_parts.next().unwrap_or_default().to_string();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        CapturedHttpRequest {
            method,
            path,
            headers,
            body: body.to_string(),
        }
    }

    fn daemon_status_fixture() -> DaemonStatus {
        DaemonStatus {
            pid: 4242,
            started_at: chrono::Utc::now(),
            persistence_mode: PersistenceMode::OnDemand,
            auto_start: false,
            main_agent_alias: Some("main".to_string()),
            main_target: Some(MainTargetSummary {
                alias: "main".to_string(),
                provider_id: "local".to_string(),
                provider_display_name: "Local".to_string(),
                model: "qwen".to_string(),
            }),
            onboarding_complete: true,
            autonomy: AutonomyProfile::default(),
            evolve: EvolveConfig::default(),
            autopilot: AutopilotConfig::default(),
            delegation: DelegationConfig::default(),
            providers: 1,
            aliases: 1,
            plugins: 0,
            delegation_targets: 1,
            webhook_connectors: 0,
            inbox_connectors: 0,
            telegram_connectors: 0,
            discord_connectors: 0,
            slack_connectors: 0,
            home_assistant_connectors: 0,
            signal_connectors: 0,
            gmail_connectors: 0,
            brave_connectors: 0,
            pending_connector_approvals: 0,
            missions: 0,
            active_missions: 0,
            memories: 0,
            pending_memory_reviews: 0,
            skill_drafts: 0,
            published_skills: 0,
        }
    }

    fn save_daemon_config(storage: &Storage, origin: &str, token: &str) {
        let parsed = Url::parse(origin).unwrap();
        let mut config = storage.load_config().unwrap();
        config.daemon.host = parsed.host_str().unwrap().to_string();
        config.daemon.port = parsed.port().unwrap();
        config.daemon.token = token.to_string();
        storage.save_config(&config).unwrap();
    }

    fn sample_remote_provider(id: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: id.to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENAI_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: Some(model.to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        }
    }

    fn sample_local_provider(origin: &str, id: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: id.to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: origin.to_string(),
            auth_mode: AuthMode::None,
            default_model: Some(model.to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        }
    }

    fn sample_alias(alias: &str, provider_id: &str, model: &str) -> ModelAlias {
        ModelAlias {
            alias: alias.to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            description: None,
        }
    }

    fn local_onboarded_config(provider: ProviderConfig, alias: ModelAlias) -> AppConfig {
        AppConfig {
            onboarding_complete: true,
            providers: vec![provider],
            aliases: vec![alias.clone()],
            main_agent_alias: Some(alias.alias),
            ..AppConfig::default()
        }
    }

    fn projected_plugin_config(alias: &str, adapter_id: &str, model: &str) -> AppConfig {
        let plugin = InstalledPluginConfig {
            id: "echo-toolkit".to_string(),
            manifest: PluginManifest {
                schema_version: PLUGIN_SCHEMA_VERSION,
                id: "echo-toolkit".to_string(),
                name: "Echo Toolkit".to_string(),
                version: "0.1.0".to_string(),
                description: "test plugin".to_string(),
                homepage: None,
                compatibility: PluginCompatibility::default(),
                permissions: PluginPermissions::default(),
                tools: Vec::new(),
                connectors: Vec::new(),
                provider_adapters: vec![PluginProviderAdapterManifest {
                    id: adapter_id.to_string(),
                    provider_kind: ProviderKind::OpenAiCompatible,
                    description: "projected provider".to_string(),
                    command: "plugin-host".to_string(),
                    args: Vec::new(),
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    default_model: Some(model.to_string()),
                    timeout_seconds: None,
                }],
            },
            source_kind: PluginSourceKind::LocalPath,
            install_dir: std::env::temp_dir().join(format!("plugin-install-{}", Uuid::new_v4())),
            source_reference: String::new(),
            source_path: std::env::temp_dir().join(format!("plugin-source-{}", Uuid::new_v4())),
            integrity_sha256: "reviewed".to_string(),
            enabled: true,
            trusted: true,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: "reviewed".to_string(),
            reviewed_at: Some(chrono::Utc::now()),
            pinned: false,
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let provider_id = plugin_provider_id(&plugin.id, adapter_id);
        AppConfig {
            onboarding_complete: true,
            plugins: vec![plugin],
            aliases: vec![sample_alias(alias, &provider_id, model)],
            main_agent_alias: Some(alias.to_string()),
            ..AppConfig::default()
        }
    }

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{name}-{}", Uuid::new_v4()));
        fs::write(&path, content).unwrap();
        path
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct StreamFixture {
        value: String,
    }

    #[tokio::test]
    async fn dashboard_command_requests_launch_when_not_opening_browser() {
        let storage = temp_storage();
        let token = "test-dashboard-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/dashboard/launch",
                    &DashboardLaunchResponse {
                        launch_path: "/auth/dashboard/launch/mock".to_string(),
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        dashboard_command(
            &storage,
            DashboardArgs {
                print_url: true,
                no_open: true,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].path, "/v1/dashboard/launch");
    }

    #[tokio::test]
    async fn provider_add_posts_provider_and_alias() {
        let storage = temp_storage();
        let token = "test-login-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/providers",
                    &sample_remote_provider("openai", "gpt-5"),
                ),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/aliases",
                    &sample_alias("main", "openai", "gpt-5"),
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        provider_command(
            &storage,
            ProviderCommands::Add(ProviderAddArgs {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                kind: HostedKindArg::OpenaiCompatible,
                base_url: Some(server.origin.clone()),
                model: "gpt-5".to_string(),
                api_key: Some("secret-api-key".to_string()),
                main_alias: Some("main".to_string()),
            }),
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let provider_body: ProviderUpsertRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(provider_body.provider.id, "openai");
        assert_eq!(
            provider_body.provider.default_model.as_deref(),
            Some("gpt-5")
        );
        assert_eq!(provider_body.api_key.as_deref(), Some("secret-api-key"));

        let alias_body: AliasUpsertRequest = serde_json::from_str(&requests[2].body).unwrap();
        assert_eq!(alias_body.alias.alias, "main");
        assert_eq!(alias_body.alias.provider_id, "openai");
        assert_eq!(alias_body.alias.model, "gpt-5");
        assert!(alias_body.set_as_main);
    }

    #[tokio::test]
    async fn model_command_lists_models_for_local_provider() {
        let storage = temp_storage();
        let server = spawn_mock_http_server(
            vec![MockHttpExpectation::json(
                "GET",
                "/models",
                &json!({"data":[{"id":"qwen-coder"},{"id":"qwen-reasoner"}]}),
            )],
            None,
        )
        .await;

        let config = local_onboarded_config(
            sample_local_provider(&server.origin, "local", "qwen-coder"),
            sample_alias("main", "local", "qwen-coder"),
        );
        storage.save_config(&config).unwrap();

        model_command(
            &storage,
            ModelCommands::List {
                provider: "local".to_string(),
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/models");
    }

    #[tokio::test]
    async fn mcp_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let schema = temp_file("mcp-schema.json", "{\"type\":\"object\"}");

        mcp_command(
            &storage,
            McpCommands::Add(McpAddArgs {
                id: "filesystem".to_string(),
                name: "Filesystem".to_string(),
                description: "fs tools".to_string(),
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                tool_name: "fs_server".to_string(),
                schema_file: schema,
                cwd: None,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers[0].id, "filesystem");
    }

    #[tokio::test]
    async fn app_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let schema = temp_file("app-schema.json", "{\"type\":\"object\"}");

        app_command(
            &storage,
            AppCommands::Add(AppAddArgs {
                id: "github".to_string(),
                name: "GitHub".to_string(),
                description: "github tools".to_string(),
                command: "node".to_string(),
                args: vec!["github.js".to_string()],
                tool_name: "github_app".to_string(),
                schema_file: schema,
                cwd: None,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.app_connectors.len(), 1);
        assert_eq!(config.app_connectors[0].id, "github");
    }

    #[tokio::test]
    async fn alias_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let config = AppConfig {
            providers: vec![sample_local_provider(
                "http://localhost:11434",
                "local",
                "qwen",
            )],
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        alias_command(
            &storage,
            AliasCommands::Add(AliasAddArgs {
                alias: "main".to_string(),
                provider: "local".to_string(),
                model: "qwen".to_string(),
                description: Some("Primary".to_string()),
                main: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.main_agent_alias.as_deref(), Some("main"));
        assert_eq!(config.aliases.len(), 1);
        assert_eq!(config.aliases[0].description.as_deref(), Some("Primary"));
    }

    #[tokio::test]
    async fn alias_command_add_accepts_projected_plugin_provider_without_daemon() {
        let storage = temp_storage();
        let config = projected_plugin_config("main", "echo-provider", "echo-1");
        let provider_id = config.aliases[0].provider_id.clone();
        storage.save_config(&config).unwrap();

        alias_command(
            &storage,
            AliasCommands::Add(AliasAddArgs {
                alias: "assistant".to_string(),
                provider: provider_id,
                model: "echo-1".to_string(),
                description: Some("Plugin-backed".to_string()),
                main: false,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.aliases.len(), 2);
        assert_eq!(config.aliases[1].alias, "assistant");
    }

    #[tokio::test]
    async fn trust_command_updates_locally_without_daemon() {
        let storage = temp_storage();

        trust_command(
            &storage,
            TrustArgs {
                path: Some(PathBuf::from("C:\\workspace")),
                allow_shell: Some(true),
                allow_network: Some(true),
                allow_full_disk: None,
                allow_self_edit: Some(false),
            },
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert!(config.trust_policy.allow_shell);
        assert!(config.trust_policy.allow_network);
        assert!(!config.trust_policy.allow_self_edit);
        assert!(config
            .trust_policy
            .trusted_paths
            .contains(&PathBuf::from("C:\\workspace")));
    }

    #[tokio::test]
    async fn permissions_command_updates_locally_without_daemon() {
        let storage = temp_storage();

        permissions_command(
            &storage,
            PermissionsArgs {
                preset: Some(PermissionPresetArg::FullAuto),
            },
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.permission_preset, PermissionPreset::FullAuto);
    }

    #[tokio::test]
    async fn daemon_config_command_updates_local_config_without_daemon() {
        let storage = temp_storage();

        daemon_command(
            &storage,
            DaemonCommands::Config(DaemonConfigArgs {
                mode: Some(PersistenceModeArg::AlwaysOn),
                auto_start: Some(false),
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn);
        assert!(!config.daemon.auto_start);
    }

    #[tokio::test]
    async fn mission_command_add_posts_schedule_request() {
        let storage = temp_storage();
        let token = "test-mission-token";
        let mission = Mission::new("Ship it".to_string(), "details".to_string());
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("POST", "/v1/missions", &mission),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        mission_command(
            &storage,
            MissionCommands::Add {
                title: "Ship it".to_string(),
                details: "details".to_string(),
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                after_seconds: Some(60),
                every_seconds: None,
                at: None,
                watch: None,
                watch_nonrecursive: false,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: Mission = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.title, "Ship it");
        assert_eq!(body.alias.as_deref(), Some("main"));
        assert_eq!(body.requested_model.as_deref(), Some("gpt-5"));
        assert_eq!(body.status, MissionStatus::Scheduled);
        assert_eq!(body.wake_trigger, Some(WakeTrigger::Timer));
    }

    #[tokio::test]
    async fn memory_command_search_posts_workspace_scoped_query() {
        let storage = temp_storage();
        let token = "test-memory-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/memory/search",
                    &MemorySearchResponse {
                        memories: Vec::new(),
                        transcript_hits: Vec::new(),
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        memory_command(
            &storage,
            MemoryCommands::Search {
                query: "build output".to_string(),
                limit: 5,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: MemorySearchQuery = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.query, "build output");
        assert_eq!(body.limit, Some(5));
        assert!(body.workspace_key.is_some());
    }

    #[tokio::test]
    async fn memory_command_rebuild_posts_request() {
        let storage = temp_storage();
        let token = "test-memory-rebuild-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/memory/rebuild",
                    &MemoryRebuildResponse {
                        generated_at: chrono::Utc::now(),
                        session_id: Some("session-1".to_string()),
                        sessions_scanned: 1,
                        observations_scanned: 4,
                        memories_upserted: 2,
                        embeddings_refreshed: 2,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        memory_command(
            &storage,
            MemoryCommands::Rebuild {
                session_id: Some("session-1".to_string()),
                recompute_embeddings: true,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: MemoryRebuildRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.session_id.as_deref(), Some("session-1"));
        assert!(body.recompute_embeddings);
    }

    #[tokio::test]
    async fn session_resume_packet_command_requests_resume_packet_from_daemon() {
        let storage = temp_storage();
        let token = "test-session-resume-packet-token";
        let packet = SessionResumePacket {
            session: SessionSummary {
                id: "session-1".to_string(),
                title: Some("Resume me".to_string()),
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-5".to_string(),
                task_mode: Some(TaskMode::Daily),
                message_count: 2,
                cwd: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            generated_at: chrono::Utc::now(),
            recent_messages: Vec::new(),
            linked_memories: Vec::new(),
            related_transcript_hits: Vec::new(),
        };
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("GET", "/v1/sessions/session-1/resume-packet", &packet),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        session_command(
            &storage,
            SessionCommands::ResumePacket {
                id: "session-1".to_string(),
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests[1].path, "/v1/sessions/session-1/resume-packet");
    }

    #[tokio::test]
    async fn autonomy_evolve_and_autopilot_status_commands_hit_daemon() {
        let storage = temp_storage();
        let token = "test-status-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "GET",
                    "/v1/autonomy/status",
                    &AutonomyProfile::default(),
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("GET", "/v1/evolve/status", &EvolveConfig::default()),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "GET",
                    "/v1/autopilot/status",
                    &AutopilotConfig::default(),
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        autonomy_command(&storage, AutonomyCommands::Status)
            .await
            .unwrap();
        evolve_command(&storage, EvolveCommands::Status)
            .await
            .unwrap();
        autopilot_command(&storage, AutopilotCommands::Status)
            .await
            .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|req| req.path == "/v1/status")
                .count(),
            3
        );
        assert!(requests.iter().any(|req| req.path == "/v1/autonomy/status"));
        assert!(requests.iter().any(|req| req.path == "/v1/evolve/status"));
        assert!(requests
            .iter()
            .any(|req| req.path == "/v1/autopilot/status"));
    }

    #[tokio::test]
    async fn connector_command_paths_hit_daemon_routes() {
        let storage = temp_storage();
        let token = "test-connector-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/telegram/ops/poll",
                    &TelegramPollResponse {
                        connector_id: "ops".to_string(),
                        processed_updates: 1,
                        queued_missions: 0,
                        pending_approvals: 0,
                        last_update_id: Some(99),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/discord/ops/send",
                    &DiscordSendResponse {
                        connector_id: "ops".to_string(),
                        channel_id: "123".to_string(),
                        message_id: Some("m-1".to_string()),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/slack/ops/send",
                    &SlackSendResponse {
                        connector_id: "ops".to_string(),
                        channel_id: "C123".to_string(),
                        message_ts: Some("123.45".to_string()),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/signal/ops/send",
                    &SignalSendResponse {
                        connector_id: "ops".to_string(),
                        target: "group:team".to_string(),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/home-assistant/ops/services",
                    &HomeAssistantServiceCallResponse {
                        connector_id: "ops".to_string(),
                        domain: "light".to_string(),
                        service: "turn_on".to_string(),
                        changed_entities: 1,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        telegram_command(
            &storage,
            TelegramCommands::Poll {
                id: "ops".to_string(),
            },
        )
        .await
        .unwrap();
        discord_command(
            &storage,
            DiscordCommands::Send(DiscordSendArgs {
                id: "ops".to_string(),
                channel_id: "123".to_string(),
                content: "deploy now".to_string(),
            }),
        )
        .await
        .unwrap();
        slack_command(
            &storage,
            SlackCommands::Send(SlackSendArgs {
                id: "ops".to_string(),
                channel_id: "C123".to_string(),
                text: "ship it".to_string(),
            }),
        )
        .await
        .unwrap();
        signal_command(
            &storage,
            SignalCommands::Send(SignalSendArgs {
                id: "ops".to_string(),
                recipient: None,
                group_id: Some("team".to_string()),
                text: "hello".to_string(),
            }),
        )
        .await
        .unwrap();
        home_assistant_command(
            &storage,
            HomeAssistantCommands::CallService(HomeAssistantServiceArgs {
                id: "ops".to_string(),
                domain: "light".to_string(),
                service: "turn_on".to_string(),
                entity_id: Some("light.office".to_string()),
                service_data_json: Some("{\"brightness\":200}".to_string()),
            }),
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let discord_body: DiscordSendRequest = serde_json::from_str(&requests[3].body).unwrap();
        assert_eq!(discord_body.channel_id, "123");
        let slack_body: SlackSendRequest = serde_json::from_str(&requests[5].body).unwrap();
        assert_eq!(slack_body.text, "ship it");
        let signal_body: SignalSendRequest = serde_json::from_str(&requests[7].body).unwrap();
        assert_eq!(signal_body.group_id.as_deref(), Some("team"));
        let ha_body: HomeAssistantServiceCallRequest =
            serde_json::from_str(&requests[9].body).unwrap();
        assert_eq!(ha_body.domain, "light");
        assert_eq!(
            ha_body
                .service_data
                .as_ref()
                .and_then(|value| value.get("brightness"))
                .and_then(serde_json::Value::as_i64),
            Some(200)
        );
    }

    #[tokio::test]
    async fn webhook_and_inbox_add_commands_persist_locally_without_daemon() {
        let storage = temp_storage();
        let inbox_path = std::env::temp_dir().join(format!("nuclear-inbox-{}", Uuid::new_v4()));
        fs::create_dir_all(&inbox_path).unwrap();

        webhook_command(
            &storage,
            WebhookCommands::Add(WebhookAddArgs {
                id: "webhook".to_string(),
                name: "Webhook".to_string(),
                description: "Inbound webhook".to_string(),
                prompt_template: Some("Handle {{summary}}".to_string()),
                prompt_file: None,
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                cwd: None,
                token: Some("hook-token".to_string()),
                enabled: true,
            }),
        )
        .await
        .unwrap();

        inbox_command(
            &storage,
            InboxCommands::Add(InboxAddArgs {
                id: "inbox".to_string(),
                name: "Inbox".to_string(),
                description: "Watch a folder".to_string(),
                path: inbox_path.clone(),
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                cwd: None,
                delete_after_read: true,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.webhook_connectors.len(), 1);
        assert_eq!(config.inbox_connectors.len(), 1);
        assert_eq!(config.inbox_connectors[0].path, inbox_path);
        assert!(config.webhook_connectors[0].token_sha256.is_some());
    }

    #[tokio::test]
    async fn skills_enable_and_disable_update_local_config() {
        let storage = temp_storage();
        let home_dir = std::env::temp_dir().join(format!("nuclear-home-{}", Uuid::new_v4()));
        let skill_dir = home_dir.join(".codex").join("skills").join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Test Skill\nA skill used for tests.\n",
        )
        .unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home_dir);

        skills_command(
            &storage,
            SkillCommands::Enable {
                name: "test-skill".to_string(),
            },
        )
        .await
        .unwrap();

        let enabled = storage.load_config().unwrap().enabled_skills;
        assert_eq!(enabled, vec!["test-skill".to_string()]);

        skills_command(
            &storage,
            SkillCommands::Disable {
                name: "test-skill".to_string(),
            },
        )
        .await
        .unwrap();

        let enabled = storage.load_config().unwrap().enabled_skills;
        assert!(enabled.is_empty());

        if let Some(previous) = previous_home {
            std::env::set_var("HOME", previous);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[tokio::test]
    async fn session_command_rename_updates_stored_session() {
        let storage = temp_storage();
        let alias = sample_alias("main", "local", "qwen");
        storage
            .ensure_session("session-1", &alias, "local", "qwen", None)
            .unwrap();

        session_command(
            &storage,
            SessionCommands::Rename {
                id: "session-1".to_string(),
                title: "Renamed".to_string(),
            },
        )
        .await
        .unwrap();

        let session = storage.get_session("session-1").unwrap().unwrap();
        assert_eq!(session.title.as_deref(), Some("Renamed"));
    }

    #[tokio::test]
    async fn run_command_posts_request_when_onboarded() {
        let storage = temp_storage();
        let token = "test-run-token";
        let provider = sample_local_provider("http://127.0.0.1:11434", "local", "qwen");
        let alias = sample_alias("main", "local", "qwen");
        storage
            .save_config(&local_onboarded_config(provider, alias))
            .unwrap();

        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/run",
                    &RunTaskResponse {
                        session_id: "session-1".to_string(),
                        alias: "main".to_string(),
                        provider_id: "local".to_string(),
                        model: "qwen".to_string(),
                        response: "done".to_string(),
                        tool_events: Vec::new(),
                        structured_output_json: None,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        run_command(
            &storage,
            RunArgs {
                prompt: Some("hello".to_string()),
                alias: Some("main".to_string()),
                tasks: Vec::new(),
                thinking: None,
                mode: None,
                images: Vec::new(),
                output_schema: None,
                output_last_message: None,
                json: false,
                ephemeral: false,
                permissions: None,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: RunTaskRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.prompt, "hello");
        assert_eq!(body.alias.as_deref(), Some("main"));
        assert_eq!(body.requested_model, None);
    }

    #[test]
    fn drain_ndjson_buffer_handles_split_utf8_boundaries() {
        let mut buffer = Vec::new();
        let mut values = Vec::new();
        let first = b"{\"value\":\"snowman ".to_vec();
        let second = vec![0xE2, 0x98];
        let mut third = vec![0x83];
        third.extend_from_slice(b"\"}\n");

        buffer.extend_from_slice(&first);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, false, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();
        assert!(values.is_empty());

        buffer.extend_from_slice(&second);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, false, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();
        assert!(values.is_empty());

        buffer.extend_from_slice(&third);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, true, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            values,
            vec![StreamFixture {
                value: "snowman \u{2603}".to_string()
            }]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_ndjson_buffer_rejects_invalid_trailing_utf8() {
        let mut buffer = vec![0xE2, 0x98];
        let error = drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, true, &mut |_| Ok(()))
            .unwrap_err();
        assert!(error.to_string().contains("utf-8"));
    }

    #[test]
    fn parse_task_requires_alias_separator() {
        let task = parse_task("coder=write code".to_string()).unwrap();
        assert_eq!(task.target.as_deref(), Some("coder"));
        assert!(task.alias.is_none());
        assert!(parse_task("missing".to_string()).is_err());
    }

    #[test]
    fn parse_key_value_list_handles_empty_and_multiple_pairs() {
        assert!(parse_key_value_list("").unwrap().is_empty());
        let parsed = parse_key_value_list("a=1,b=2").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "a");
        assert_eq!(parsed[1].value, "2");
    }

    #[test]
    fn pkce_challenge_is_url_safe() {
        let challenge = pkce_challenge("abc123");
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
    }

    #[test]
    fn apply_trust_update_preserves_unspecified_values() {
        let mut policy = TrustPolicy {
            allow_shell: false,
            allow_network: false,
            ..TrustPolicy::default()
        };
        apply_trust_update(
            &mut policy,
            &TrustUpdateRequest {
                trusted_path: None,
                allow_shell: None,
                allow_network: Some(true),
                allow_full_disk: None,
                allow_self_edit: None,
            },
        );
        assert!(!policy.allow_shell);
        assert!(policy.allow_network);
    }

    #[test]
    fn first_provider_defaults_to_main_alias_only_once() {
        let config = AppConfig::default();
        assert_eq!(default_main_alias(&config, None), Some("main".to_string()));

        let configured = AppConfig {
            main_agent_alias: Some("claude".to_string()),
            ..AppConfig::default()
        };
        assert_eq!(default_main_alias(&configured, None), None);
        assert_eq!(
            default_main_alias(&configured, Some("writer".to_string())),
            Some("writer".to_string())
        );
    }

    #[test]
    fn next_available_provider_id_appends_suffix_when_needed() {
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("gpt-4.1".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "openai-2".to_string(),
                    display_name: "OpenAI 2".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("gpt-4.1-mini".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
            ],
            ..AppConfig::default()
        };

        assert_eq!(next_available_provider_id(&config, "openai"), "openai-3");
        assert_eq!(
            next_available_provider_id(&config, "anthropic"),
            "anthropic"
        );
    }

    #[test]
    fn default_alias_name_uses_main_then_model_slug() {
        let empty = AppConfig::default();
        let provider = ProviderConfig {
            id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENROUTER_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: Some("openai/gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        };
        assert_eq!(
            default_alias_name(&empty, &provider, "openai/gpt-4.1"),
            "main"
        );

        let configured = AppConfig {
            main_agent_alias: Some("main".to_string()),
            aliases: vec![
                ModelAlias {
                    alias: "main".to_string(),
                    provider_id: "openai".to_string(),
                    model: "gpt-4.1".to_string(),
                    description: None,
                },
                ModelAlias {
                    alias: "openrouter-openai-gpt".to_string(),
                    provider_id: "openrouter".to_string(),
                    model: "openai/gpt-4.1".to_string(),
                    description: None,
                },
            ],
            ..AppConfig::default()
        };
        assert_eq!(
            default_alias_name(&configured, &provider, "openai/gpt-4.1"),
            "openrouter-openai-gpt-4"
        );
    }

    #[test]
    fn moonshot_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Moonshot),
            DEFAULT_MOONSHOT_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Moonshot),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn openrouter_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Openrouter),
            DEFAULT_OPENROUTER_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Openrouter),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn venice_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Venice),
            DEFAULT_VENICE_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Venice),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn openai_browser_login_uses_chatgpt_codex_provider_defaults() {
        assert_eq!(
            browser_hosted_kind_to_provider_kind(HostedKindArg::OpenaiCompatible),
            ProviderKind::ChatGptCodex
        );
        assert_eq!(
            default_browser_hosted_url(HostedKindArg::OpenaiCompatible),
            DEFAULT_CHATGPT_CODEX_URL
        );
        assert_eq!(
            browser_hosted_kind_to_provider_kind(HostedKindArg::Anthropic),
            ProviderKind::Anthropic
        );
    }

    #[test]
    fn automatic_browser_capture_is_native_for_openai_anthropic_and_openrouter() {
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::OpenaiCompatible
        ));
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Anthropic
        ));
        assert!(!hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Moonshot
        ));
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Openrouter
        ));
        assert!(!hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Venice
        ));
    }

    #[test]
    fn openai_browser_oauth_config_requests_org_enriched_claims() {
        let config = openai_browser_oauth_config();
        assert!(config.scopes.iter().any(|scope| scope == "openid"));
        assert!(config.scopes.iter().any(|scope| scope == "offline_access"));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| { param.key == "id_token_add_organizations" && param.value == "true" }));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| { param.key == "codex_cli_simplified_flow" && param.value == "true" }));
    }

    #[test]
    fn openai_browser_authorization_url_uses_loopback_contract() {
        let provider = ProviderConfig {
            id: "openai-browser".to_string(),
            display_name: "OpenAI Browser Session".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(openai_browser_oauth_config()),
            local: false,
        };
        let redirect_uri = format!(
            "http://localhost:{OPENAI_BROWSER_CALLBACK_PORT}{OPENAI_BROWSER_CALLBACK_PATH}"
        );
        let authorization_url = build_oauth_authorization_url(
            &provider,
            &redirect_uri,
            "state-123",
            &pkce_challenge("verifier-123"),
        )
        .expect("authorization URL should build");
        let parsed = Url::parse(&authorization_url).expect("authorization URL should parse");
        let query = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(parsed.host_str(), Some("auth.openai.com"));
        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(redirect_uri.as_str())
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile email offline_access api.connectors.read api.connectors.invoke")
        );
        assert_eq!(
            query.get("originator").map(String::as_str),
            Some(OPENAI_BROWSER_ORIGINATOR)
        );
    }

    #[test]
    fn claude_browser_oauth_config_matches_packaged_claude_constants() {
        let config = claude_browser_oauth_config();
        assert_eq!(config.client_id, CLAUDE_BROWSER_CLIENT_ID);
        assert_eq!(config.authorization_url, CLAUDE_BROWSER_AUTHORIZE_URL);
        assert_eq!(config.token_url, CLAUDE_BROWSER_TOKEN_URL);
        assert!(config
            .scopes
            .iter()
            .any(|scope| scope == "org:create_api_key"));
        assert!(config.scopes.iter().any(|scope| scope == "user:inference"));
        assert!(config
            .scopes
            .iter()
            .any(|scope| scope == "user:sessions:claude_code"));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| param.key == "code" && param.value == "true"));
    }

    #[test]
    fn claude_browser_authorization_url_uses_loopback_contract() {
        let provider = ProviderConfig {
            id: "claude-browser".to_string(),
            display_name: "Claude Browser Session".to_string(),
            kind: ProviderKind::Anthropic,
            base_url: DEFAULT_ANTHROPIC_URL.to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(claude_browser_oauth_config()),
            local: false,
        };
        let redirect_uri = format!(
            "http://localhost:{CLAUDE_BROWSER_CALLBACK_PORT}{CLAUDE_BROWSER_CALLBACK_PATH}"
        );
        let authorization_url = build_oauth_authorization_url(
            &provider,
            &redirect_uri,
            "state-456",
            &pkce_challenge("verifier-456"),
        )
        .expect("authorization URL should build");
        let parsed = Url::parse(&authorization_url).expect("authorization URL should parse");
        let query = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(parsed.host_str(), Some("claude.ai"));
        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(redirect_uri.as_str())
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers")
        );
        assert_eq!(query.get("code").map(String::as_str), Some("true"));
    }

    #[test]
    fn claude_scope_error_triggers_oauth_fallback() {
        assert!(should_fallback_to_claude_browser_oauth(
            "Claude browser API key mint failed: OAuth token does not meet scope requirement org:create_api_key"
        ));
        assert!(!should_fallback_to_claude_browser_oauth(
            "Claude browser API key mint failed: service unavailable"
        ));
    }

    #[test]
    fn claude_settings_parser_reads_existing_browser_credentials() {
        let parsed = parse_claude_browser_credentials_from_settings(
            r#"{
                "primaryApiKey": " sk-ant-managed ",
                "oauthAccount": {
                    "emailAddress": "user@example.com",
                    "organizationUuid": "org_123",
                    "organizationName": "Acme"
                }
            }"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.api_key, "sk-ant-managed");
        assert_eq!(parsed.email.as_deref(), Some("user@example.com"));
        assert_eq!(parsed.org_id.as_deref(), Some("org_123"));
        assert_eq!(parsed.org_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn oauth_callback_error_message_prefers_description() {
        assert_eq!(
            oauth_callback_error_message("access_denied", Some("unknown authentication error")),
            "Sign-in failed: unknown authentication error"
        );
    }

    #[test]
    fn oauth_callback_error_message_maps_missing_codex_entitlement() {
        assert_eq!(
            oauth_callback_error_message(
                "access_denied",
                Some("missing_codex_entitlement for this workspace")
            ),
            "OpenAI browser sign-in is not enabled for this workspace account yet."
        );
    }

    #[test]
    fn rejects_plaintext_oauth_secret_params() {
        let error = reject_plaintext_oauth_secrets(&[KeyValuePair {
            key: "client_secret".to_string(),
            value: "secret".to_string(),
        }])
        .unwrap_err();
        assert!(error.to_string().contains("plaintext config"));
    }

    #[test]
    fn jwt_expiry_reads_exp_claim() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(br#"{"exp":4102444800}"#);
        let token = format!("{header}.{payload}.sig");
        let expiry = jwt_expiry(&token).unwrap();
        assert_eq!(expiry.timestamp(), 4_102_444_800);
    }

    #[test]
    fn cli_uses_default_prompt_without_subcommand() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "write a summary"]);
        assert_eq!(cli.prompt.as_deref(), Some("write a summary"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn legacy_command_alias_still_parses() {
        let cli = Cli::parse_from([agent_core::LEGACY_COMMAND_NAME, "resume", "--last"]);
        match cli.command {
            Some(Commands::Resume(args)) => {
                assert!(args.last);
                assert!(args.session_id.is_none());
            }
            _ => panic!("expected resume command"),
        }
    }

    #[test]
    fn hosted_kind_accepts_openai_alias() {
        assert_eq!(
            HostedKindArg::from_str("openai", true).unwrap(),
            HostedKindArg::OpenaiCompatible
        );
        assert_eq!(
            HostedKindArg::from_str("openai-compatible", true).unwrap(),
            HostedKindArg::OpenaiCompatible
        );
    }

    #[test]
    fn cli_parses_exec_subcommand() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "exec",
            "--alias",
            "claude",
            "--thinking",
            "high",
            "fix the bug",
        ]);
        match cli.command {
            Some(Commands::Exec(args)) => {
                assert_eq!(args.alias.as_deref(), Some("claude"));
                assert_eq!(args.thinking, Some(ThinkingLevelArg::High));
                assert_eq!(args.prompt.as_deref(), Some("fix the bug"));
            }
            _ => panic!("expected exec command"),
        }
    }

    #[test]
    fn cli_parses_resume_last() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "resume", "--last"]);
        match cli.command {
            Some(Commands::Resume(args)) => {
                assert!(args.last);
                assert!(args.session_id.is_none());
            }
            _ => panic!("expected resume command"),
        }
    }

    #[test]
    fn cli_parses_browser_auth_for_login() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "login",
            "--id",
            "openrouter",
            "--name",
            "OpenRouter",
            "--kind",
            "openrouter",
            "--auth",
            "browser",
            "--model",
            "openai/gpt-4.1",
        ]);
        match cli.command {
            Some(Commands::Login(args)) => {
                assert_eq!(args.kind, Some(HostedKindArg::Openrouter));
                assert_eq!(args.auth, Some(AuthMethodArg::Browser));
            }
            _ => panic!("expected login command"),
        }
    }

    #[test]
    fn cli_parses_daemon_config_bool_value() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "daemon",
            "config",
            "--mode",
            "always-on",
            "--auto-start",
            "true",
        ]);
        match cli.command {
            Some(Commands::Daemon {
                command: DaemonCommands::Config(args),
            }) => {
                assert_eq!(args.mode, Some(PersistenceModeArg::AlwaysOn));
                assert_eq!(args.auto_start, Some(true));
            }
            _ => panic!("expected daemon config command"),
        }
    }

    #[test]
    fn cli_parses_trust_bool_values() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "trust",
            "--allow-shell",
            "true",
            "--allow-network",
            "false",
        ]);
        match cli.command {
            Some(Commands::Trust(args)) => {
                assert_eq!(args.allow_shell, Some(true));
                assert_eq!(args.allow_network, Some(false));
            }
            _ => panic!("expected trust command"),
        }
    }

    #[test]
    fn parse_interactive_command_supports_model_mode_and_thinking() {
        assert_eq!(
            parse_interactive_command("/model claude").unwrap(),
            Some(InteractiveCommand::ModelSet("claude".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/provider anthropic").unwrap(),
            Some(InteractiveCommand::ProviderSet("anthropic".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/provider").unwrap(),
            Some(InteractiveCommand::ProviderShow)
        );
        assert_eq!(
            parse_interactive_command("/onboard").unwrap(),
            Some(InteractiveCommand::Onboard)
        );
        assert_eq!(
            parse_interactive_command("/thinking high").unwrap(),
            Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
        );
        assert_eq!(
            parse_interactive_command("/thinking default").unwrap(),
            Some(InteractiveCommand::ThinkingSet(None))
        );
        assert_eq!(
            parse_interactive_command("/mode daily").unwrap(),
            Some(InteractiveCommand::ModeSet(Some(TaskMode::Daily)))
        );
        assert_eq!(
            parse_interactive_command("/mode default").unwrap(),
            Some(InteractiveCommand::ModeSet(None))
        );
    }

    #[test]
    fn resolve_interactive_provider_selection_prefers_logged_in_provider_aliases() {
        let storage = temp_storage();
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    auth_mode: AuthMode::None,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: true,
                },
                ProviderConfig {
                    id: "anthropic".to_string(),
                    display_name: "Claude".to_string(),
                    kind: ProviderKind::Anthropic,
                    base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                    auth_mode: AuthMode::None,
                    default_model: Some("claude-sonnet".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: true,
                },
            ],
            aliases: vec![
                ModelAlias {
                    alias: "main".to_string(),
                    provider_id: "openai".to_string(),
                    model: "gpt-5".to_string(),
                    description: None,
                },
                ModelAlias {
                    alias: "claude".to_string(),
                    provider_id: "anthropic".to_string(),
                    model: "claude-sonnet".to_string(),
                    description: None,
                },
            ],
            main_agent_alias: Some("main".to_string()),
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "anthropic").unwrap(),
            "claude"
        );
        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "Claude").unwrap(),
            "claude"
        );
        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "claude").unwrap(),
            "claude"
        );
    }

    #[test]
    fn normalize_model_selection_value_ignores_punctuation() {
        assert_eq!(normalize_model_selection_value("gpt-5.4"), "gpt54");
        assert_eq!(normalize_model_selection_value("GPT 5_4"), "gpt54");
    }

    #[test]
    fn resolved_requested_model_prefers_explicit_override() {
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-5.2".to_string(),
            description: None,
        };

        assert_eq!(resolved_requested_model(&alias, Some("gpt-5.4")), "gpt-5.4");
        assert_eq!(resolved_requested_model(&alias, None), "gpt-5.2");
    }

    #[test]
    fn parse_interactive_command_supports_review_and_status() {
        assert_eq!(
            parse_interactive_command("/review focus on tests").unwrap(),
            Some(InteractiveCommand::Review(Some(
                "focus on tests".to_string()
            )))
        );
        assert_eq!(
            parse_interactive_command("/status").unwrap(),
            Some(InteractiveCommand::Status)
        );
        assert_eq!(
            parse_interactive_command("/config").unwrap(),
            Some(InteractiveCommand::ConfigShow)
        );
        assert_eq!(
            parse_interactive_command("/dashboard").unwrap(),
            Some(InteractiveCommand::DashboardOpen)
        );
        assert_eq!(
            parse_interactive_command("/telegrams").unwrap(),
            Some(InteractiveCommand::TelegramsShow)
        );
        assert_eq!(
            parse_interactive_command("/telegram approvals").unwrap(),
            Some(InteractiveCommand::TelegramApprovalsShow)
        );
        assert_eq!(
            parse_interactive_command("/discords").unwrap(),
            Some(InteractiveCommand::DiscordsShow)
        );
        assert_eq!(
            parse_interactive_command("/home-assistant").unwrap(),
            Some(InteractiveCommand::HomeAssistantsShow)
        );
        assert_eq!(
            parse_interactive_command("/telegram approve req-1 looks good").unwrap(),
            Some(InteractiveCommand::TelegramApprove {
                id: "req-1".to_string(),
                note: Some("looks good".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/webhooks").unwrap(),
            Some(InteractiveCommand::WebhooksShow)
        );
        assert_eq!(
            parse_interactive_command("/inboxes").unwrap(),
            Some(InteractiveCommand::InboxesShow)
        );
    }

    #[test]
    fn parse_interactive_command_supports_events_and_schedule() {
        assert_eq!(
            parse_interactive_command("/events 25").unwrap(),
            Some(InteractiveCommand::EventsShow(25))
        );
        assert_eq!(
            parse_interactive_command("/schedule 300 review auth flow").unwrap(),
            Some(InteractiveCommand::Schedule {
                after_seconds: 300,
                title: "review auth flow".to_string(),
            })
        );
        assert_eq!(
            parse_interactive_command("/repeat 600 weekly cleanup").unwrap(),
            Some(InteractiveCommand::Repeat {
                every_seconds: 600,
                title: "weekly cleanup".to_string(),
            })
        );
        assert_eq!(
            parse_interactive_command("/watch src watch auth changes").unwrap(),
            Some(InteractiveCommand::Watch {
                path: PathBuf::from("src"),
                title: "watch auth changes".to_string(),
            })
        );
    }

    #[test]
    fn parse_interactive_command_supports_profile_and_skills() {
        assert_eq!(
            parse_interactive_command("/profile").unwrap(),
            Some(InteractiveCommand::ProfileShow)
        );
        assert_eq!(
            parse_interactive_command("/skills published").unwrap(),
            Some(InteractiveCommand::Skills(InteractiveSkillCommand::Show(
                Some(SkillDraftStatus::Published)
            )))
        );
        assert_eq!(
            parse_interactive_command("/skills publish draft-1").unwrap(),
            Some(InteractiveCommand::Skills(
                InteractiveSkillCommand::Publish("draft-1".to_string())
            ))
        );
    }

    #[test]
    fn parse_interactive_command_supports_memory_review_actions() {
        assert_eq!(
            parse_interactive_command("/memory review").unwrap(),
            Some(InteractiveCommand::MemoryReviewShow)
        );
        assert_eq!(
            parse_interactive_command("/memory rebuild session-1").unwrap(),
            Some(InteractiveCommand::MemoryRebuild {
                session_id: Some("session-1".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/memory approve mem-1 looks good").unwrap(),
            Some(InteractiveCommand::MemoryApprove {
                id: "mem-1".to_string(),
                note: Some("looks good".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/memory reject mem-2 duplicate").unwrap(),
            Some(InteractiveCommand::MemoryReject {
                id: "mem-2".to_string(),
                note: Some("duplicate".to_string()),
            })
        );
    }

    #[test]
    fn parse_interactive_command_supports_discord_review_actions() {
        assert_eq!(
            parse_interactive_command("/discord approvals").unwrap(),
            Some(InteractiveCommand::DiscordApprovalsShow)
        );
        assert_eq!(
            parse_interactive_command("/discord approve appr-1 trusted").unwrap(),
            Some(InteractiveCommand::DiscordApprove {
                id: "appr-1".to_string(),
                note: Some("trusted".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/discord reject appr-2 spam").unwrap(),
            Some(InteractiveCommand::DiscordReject {
                id: "appr-2".to_string(),
                note: Some("spam".to_string()),
            })
        );
    }

    #[test]
    fn cli_parses_mission_schedule_flags() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "mission",
            "add",
            "Follow up",
            "--after-seconds",
            "120",
        ]);
        match cli.command {
            Some(Commands::Mission {
                command:
                    MissionCommands::Add {
                        title,
                        after_seconds,
                        at,
                        ..
                    },
            }) => {
                assert_eq!(title, "Follow up");
                assert_eq!(after_seconds, Some(120));
                assert_eq!(at, None);
            }
            _ => panic!("expected scheduled mission add command"),
        }
    }

    #[test]
    fn cli_parses_mission_watch_flags() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "mission",
            "add",
            "Watch repo",
            "--watch",
            "src",
            "--watch-nonrecursive",
        ]);
        match cli.command {
            Some(Commands::Mission {
                command:
                    MissionCommands::Add {
                        watch,
                        watch_nonrecursive,
                        ..
                    },
            }) => {
                assert_eq!(watch, Some(PathBuf::from("src")));
                assert!(watch_nonrecursive);
            }
            _ => panic!("expected watched mission add command"),
        }
    }

    #[test]
    fn cli_parses_run_and_chat_modes() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "run",
            "--mode",
            "daily",
            "plan my week",
        ]);
        match cli.command {
            Some(Commands::Run(args)) => {
                assert_eq!(args.mode, Some(TaskModeArg::Daily));
                assert_eq!(args.prompt.as_deref(), Some("plan my week"));
            }
            _ => panic!("expected run command"),
        }

        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "chat", "--mode", "build"]);
        match cli.command {
            Some(Commands::Chat(args)) => {
                assert_eq!(args.mode, Some(TaskModeArg::Build));
            }
            _ => panic!("expected chat command"),
        }
    }

    #[test]
    fn cli_parses_skill_draft_listing() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "skills",
            "drafts",
            "--status",
            "published",
            "--limit",
            "5",
        ]);
        match cli.command {
            Some(Commands::Skills {
                command: SkillCommands::Drafts { limit, status },
            }) => {
                assert_eq!(limit, 5);
                assert_eq!(status, Some(SkillDraftStatusArg::Published));
            }
            _ => panic!("expected skill drafts command"),
        }
    }

    #[test]
    fn cli_parses_memory_profile_command() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "memory", "profile", "--limit", "7"]);
        match cli.command {
            Some(Commands::Memory {
                command: MemoryCommands::Profile { limit },
            }) => {
                assert_eq!(limit, 7);
            }
            _ => panic!("expected memory profile command"),
        }
    }

    #[test]
    fn cli_parses_memory_rebuild_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "memory",
            "rebuild",
            "--session-id",
            "session-1",
            "--recompute-embeddings",
        ]);
        match cli.command {
            Some(Commands::Memory {
                command:
                    MemoryCommands::Rebuild {
                        session_id,
                        recompute_embeddings,
                    },
            }) => {
                assert_eq!(session_id.as_deref(), Some("session-1"));
                assert!(recompute_embeddings);
            }
            _ => panic!("expected memory rebuild command"),
        }
    }

    #[test]
    fn cli_parses_session_resume_packet_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "session",
            "resume-packet",
            "session-1",
        ]);
        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::ResumePacket { id },
            }) => {
                assert_eq!(id, "session-1");
            }
            _ => panic!("expected session resume-packet command"),
        }
    }

    #[test]
    fn cli_parses_telegram_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "telegram",
            "add",
            "--id",
            "ops-bot",
            "--name",
            "Ops Bot",
            "--description",
            "telegram resident connector",
            "--bot-token",
            "123:abc",
            "--chat-id",
            "42",
            "--user-id",
            "7",
        ]);
        match cli.command {
            Some(Commands::Telegram {
                command: TelegramCommands::Add(args),
            }) => {
                assert_eq!(args.id, "ops-bot");
                assert_eq!(args.chat_ids, vec![42]);
                assert_eq!(args.user_ids, vec![7]);
                assert!(args.require_pairing_approval);
            }
            _ => panic!("expected telegram add command"),
        }
    }

    #[test]
    fn cli_parses_webhook_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "webhook",
            "add",
            "--id",
            "alerts",
            "--name",
            "Alerts",
            "--description",
            "system alerts",
            "--prompt-template",
            "Check {payload_json}",
        ]);
        match cli.command {
            Some(Commands::Webhook {
                command: WebhookCommands::Add(args),
            }) => {
                assert_eq!(args.id, "alerts");
                assert_eq!(args.name, "Alerts");
            }
            _ => panic!("expected webhook add command"),
        }
    }

    #[test]
    fn cli_parses_discord_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "discord",
            "add",
            "--id",
            "ops-discord",
            "--name",
            "Ops Discord",
            "--description",
            "discord resident connector",
            "--bot-token",
            "discord-secret",
            "--monitored-channel-id",
            "123",
            "--allowed-channel-id",
            "456",
            "--user-id",
            "789",
        ]);
        match cli.command {
            Some(Commands::Discord {
                command: DiscordCommands::Add(args),
            }) => {
                assert_eq!(args.id, "ops-discord");
                assert_eq!(args.monitored_channel_ids, vec!["123".to_string()]);
                assert_eq!(args.allowed_channel_ids, vec!["456".to_string()]);
                assert_eq!(args.user_ids, vec!["789".to_string()]);
                assert!(args.require_pairing_approval);
            }
            _ => panic!("expected discord add command"),
        }
    }

    #[test]
    fn cli_parses_inbox_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "inbox",
            "add",
            "--id",
            "dropbox",
            "--name",
            "Drop Box",
            "--description",
            "local inbox",
            "--path",
            "tasks",
        ]);
        match cli.command {
            Some(Commands::Inbox {
                command: InboxCommands::Add(args),
            }) => {
                assert_eq!(args.id, "dropbox");
                assert_eq!(args.name, "Drop Box");
                assert_eq!(args.path, PathBuf::from("tasks"));
            }
            _ => panic!("expected inbox add command"),
        }
    }

    #[test]
    fn interactive_thinking_command_parses_default_and_levels() {
        assert_eq!(
            parse_interactive_command("/thinking").unwrap(),
            Some(InteractiveCommand::ThinkingShow)
        );
        assert_eq!(
            parse_interactive_command("/thinking high").unwrap(),
            Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
        );
        assert_eq!(
            parse_interactive_command("/thinking default").unwrap(),
            Some(InteractiveCommand::ThinkingSet(None))
        );
        assert_eq!(
            parse_interactive_command("/mode").unwrap(),
            Some(InteractiveCommand::ModeShow)
        );
    }

    #[test]
    fn interactive_model_and_review_commands_parse() {
        assert_eq!(
            parse_interactive_command("/model claude").unwrap(),
            Some(InteractiveCommand::ModelSet("claude".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/settings").unwrap(),
            Some(InteractiveCommand::ConfigShow)
        );
        assert_eq!(
            parse_interactive_command("/review focus on auth").unwrap(),
            Some(InteractiveCommand::Review(Some(
                "focus on auth".to_string()
            )))
        );
    }

    #[test]
    fn interactive_copy_compact_init_and_rename_commands_parse() {
        assert_eq!(
            parse_interactive_command("/copy").unwrap(),
            Some(InteractiveCommand::Copy)
        );
        assert_eq!(
            parse_interactive_command("/compact").unwrap(),
            Some(InteractiveCommand::Compact)
        );
        assert_eq!(
            parse_interactive_command("/init").unwrap(),
            Some(InteractiveCommand::Init)
        );
        assert_eq!(
            parse_interactive_command("/rename auth fixes").unwrap(),
            Some(InteractiveCommand::Rename(Some("auth fixes".to_string())))
        );
    }

    #[test]
    fn build_compact_prompt_includes_transcript_content() {
        let transcript = SessionTranscript {
            session: agent_core::SessionSummary {
                id: "session-1".to_string(),
                title: Some("Test".to_string()),
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                task_mode: None,
                message_count: 0,
                cwd: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            messages: vec![
                agent_core::SessionMessage::new(
                    "session-1".to_string(),
                    MessageRole::User,
                    "Fix auth".to_string(),
                    Some("openai".to_string()),
                    Some("gpt-4.1".to_string()),
                ),
                agent_core::SessionMessage::new(
                    "session-1".to_string(),
                    MessageRole::Assistant,
                    "Working on it".to_string(),
                    Some("openai".to_string()),
                    Some("gpt-4.1".to_string()),
                ),
            ],
        };

        let prompt = build_compact_prompt(&transcript).unwrap();
        assert!(prompt.contains("Fix auth"));
        assert!(prompt.contains("Working on it"));
    }

    #[test]
    fn determine_logout_targets_uses_main_alias_provider() {
        let config = AppConfig {
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "anthropic".to_string(),
                model: "claude-opus".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                kind: ProviderKind::Anthropic,
                base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                auth_mode: AuthMode::ApiKey,
                default_model: Some("claude-opus".to_string()),
                keychain_account: None,
                oauth: None,
                local: false,
            }],
            ..AppConfig::default()
        };

        let targets = determine_logout_targets(
            &config,
            &LogoutArgs {
                provider: None,
                all: false,
            },
        )
        .unwrap();

        assert_eq!(targets, vec!["anthropic".to_string()]);
    }

    #[test]
    fn configured_keychain_accounts_deduplicates_and_skips_missing() {
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::ChatGptCodex,
                    base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                    auth_mode: AuthMode::OAuth,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: Some("shared-account".to_string()),
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "openai-2".to_string(),
                    display_name: "OpenAI 2".to_string(),
                    kind: ProviderKind::ChatGptCodex,
                    base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                    auth_mode: AuthMode::OAuth,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: Some("shared-account".to_string()),
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "anthropic".to_string(),
                    display_name: "Anthropic".to_string(),
                    kind: ProviderKind::Anthropic,
                    base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("claude-opus".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
            ],
            telegram_connectors: vec![TelegramConnectorConfig {
                id: "ops-bot".to_string(),
                name: "Ops Bot".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("telegram-account".to_string()),
                require_pairing_approval: true,
                allowed_chat_ids: Vec::new(),
                allowed_user_ids: Vec::new(),
                last_update_id: None,
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            discord_connectors: vec![DiscordConnectorConfig {
                id: "ops-discord".to_string(),
                name: "Ops Discord".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("discord-account".to_string()),
                require_pairing_approval: true,
                monitored_channel_ids: vec!["123".to_string()],
                allowed_channel_ids: vec!["456".to_string()],
                allowed_user_ids: vec!["789".to_string()],
                channel_cursors: vec![DiscordChannelCursor {
                    channel_id: "123".to_string(),
                    last_message_id: Some("999".to_string()),
                }],
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            slack_connectors: vec![SlackConnectorConfig {
                id: "ops-slack".to_string(),
                name: "Ops Slack".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("slack-account".to_string()),
                require_pairing_approval: true,
                monitored_channel_ids: vec!["C123".to_string()],
                allowed_channel_ids: Vec::new(),
                allowed_user_ids: Vec::new(),
                channel_cursors: Vec::new(),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            home_assistant_connectors: vec![HomeAssistantConnectorConfig {
                id: "ops-home".to_string(),
                name: "Ops Home".to_string(),
                description: String::new(),
                enabled: true,
                base_url: "http://ha.local".to_string(),
                access_token_keychain_account: Some("home-account".to_string()),
                monitored_entity_ids: vec!["light.office".to_string()],
                allowed_service_domains: Vec::new(),
                allowed_service_entity_ids: Vec::new(),
                entity_cursors: Vec::new(),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            brave_connectors: vec![BraveConnectorConfig {
                id: "brave-search".to_string(),
                name: "Brave Search".to_string(),
                description: String::new(),
                enabled: true,
                api_key_keychain_account: Some("brave-account".to_string()),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            gmail_connectors: vec![GmailConnectorConfig {
                id: "gmail-ops".to_string(),
                name: "Ops Gmail".to_string(),
                description: String::new(),
                enabled: true,
                oauth_keychain_account: Some("gmail-account".to_string()),
                require_pairing_approval: true,
                allowed_sender_addresses: Vec::new(),
                label_filter: Some("INBOX".to_string()),
                last_history_id: None,
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            ..AppConfig::default()
        };

        let accounts = configured_keychain_accounts(&config);

        assert_eq!(accounts.len(), 7);
        assert!(accounts.contains("shared-account"));
        assert!(accounts.contains("telegram-account"));
        assert!(accounts.contains("discord-account"));
        assert!(accounts.contains("slack-account"));
        assert!(accounts.contains("home-account"));
        assert!(accounts.contains("brave-account"));
        assert!(accounts.contains("gmail-account"));
    }

    #[test]
    fn cli_parses_reset_yes() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "reset", "--yes"]);

        match cli.command {
            Some(Commands::Reset(args)) => assert!(args.yes),
            _ => panic!("expected reset command"),
        }
    }

    #[test]
    fn cli_parses_openrouter_provider_add() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "openrouter",
            "--name",
            "OpenRouter",
            "--kind",
            "openrouter",
            "--model",
            "openai/gpt-4.1",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::Openrouter);
                assert_eq!(args.model, "openai/gpt-4.1");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn cli_parses_openai_provider_add_alias() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "openai",
            "--name",
            "OpenAI",
            "--kind",
            "openai",
            "--model",
            "gpt-4.1",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::OpenaiCompatible);
                assert_eq!(args.model, "gpt-4.1");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn cli_parses_venice_provider_add() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "venice",
            "--name",
            "Venice",
            "--kind",
            "venice",
            "--model",
            "venice-large",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::Venice);
                assert_eq!(args.model, "venice-large");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn needs_onboarding_for_default_config() {
        assert!(needs_onboarding(&AppConfig::default()));
    }

    #[test]
    fn onboarding_not_needed_when_main_alias_resolves() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::Ollama,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                auth_mode: AuthMode::None,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            }],
            ..AppConfig::default()
        };

        assert!(!needs_onboarding(&config));
    }

    #[test]
    fn onboarding_not_needed_when_projected_plugin_provider_resolves() {
        let config = projected_plugin_config("main", "echo-provider", "echo-1");

        assert!(has_usable_main_alias(&config));
        assert!(!needs_onboarding(&config));
    }

    #[test]
    fn usable_main_alias_does_not_depend_on_onboarding_complete() {
        let config = AppConfig {
            onboarding_complete: false,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::Ollama,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                auth_mode: AuthMode::None,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            }],
            ..AppConfig::default()
        };

        assert!(has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_provider_is_missing() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "missing".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_provider_credentials_are_missing() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                auth_mode: AuthMode::OAuth,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: Some(openai_browser_oauth_config()),
                local: false,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_saved_credentials_are_unreadable() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                auth_mode: AuthMode::ApiKey,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: Some("missing-provider-account".to_string()),
                oauth: None,
                local: false,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }
}
