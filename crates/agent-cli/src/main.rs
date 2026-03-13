use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io::IsTerminal,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

mod tui;

use agent_core::{
    AliasUpsertRequest, AppConfig, AppConnectorConfig, AppConnectorUpsertRequest, AuthMode,
    AutonomyEnableRequest, AutonomyMode, AutopilotConfig, AutopilotState, AutopilotUpdateRequest,
    BatchTaskRequest, BatchTaskResponse, ConnectorApprovalRecord, ConnectorApprovalStatus,
    ConnectorApprovalUpdateRequest, ConnectorKind, DaemonConfigUpdateRequest, DaemonStatus,
    DiscordChannelCursor, DiscordConnectorConfig, DiscordConnectorUpsertRequest,
    DiscordPollResponse, DiscordSendRequest, DiscordSendResponse, EvolveConfig, EvolveStartRequest,
    HealthReport, HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest,
    HomeAssistantEntityState, HomeAssistantPollResponse, HomeAssistantServiceCallRequest,
    HomeAssistantServiceCallResponse, InboxConnectorConfig, InboxConnectorUpsertRequest,
    InboxPollResponse, InputAttachment, KeyValuePair, McpServerConfig, McpServerUpsertRequest,
    MemoryKind, MemoryRecord, MemoryReviewStatus, MemoryReviewUpdateRequest, MemoryScope,
    MemorySearchQuery, MemorySearchResponse, MemoryUpsertRequest, MessageRole, Mission,
    MissionCheckpoint, MissionControlRequest, MissionStatus, ModelAlias, OAuthConfig, OAuthToken,
    PermissionPreset, PermissionUpdateRequest, PersistenceMode, ProviderConfig, ProviderKind,
    ProviderUpsertRequest, RunTaskRequest, RunTaskResponse, SessionTranscript,
    SignalConnectorConfig, SignalConnectorUpsertRequest, SignalPollResponse, SignalSendRequest,
    SignalSendResponse, SkillDraft, SkillDraftStatus, SkillUpdateRequest, SlackChannelCursor,
    SlackConnectorConfig, SlackConnectorUpsertRequest, SlackPollResponse, SlackSendRequest,
    SlackSendResponse, SubAgentTask, TelegramConnectorConfig, TelegramConnectorUpsertRequest,
    TelegramPollResponse, TelegramSendRequest, TelegramSendResponse, ThinkingLevel, TrustPolicy,
    TrustUpdateRequest, WakeTrigger, WebhookConnectorConfig, WebhookConnectorUpsertRequest,
    WebhookEventRequest, WebhookEventResponse, DEFAULT_ANTHROPIC_URL, DEFAULT_CHATGPT_CODEX_URL,
    DEFAULT_LOCAL_OPENAI_URL, DEFAULT_MOONSHOT_URL, DEFAULT_OLLAMA_URL, DEFAULT_OPENAI_URL,
    DEFAULT_OPENROUTER_URL, DEFAULT_VENICE_URL, INTERNAL_DAEMON_ARG,
};
use agent_policy::{
    allow_shell, autonomy_summary, autonomy_warning, permission_summary, trust_summary,
};
use agent_providers::{
    build_oauth_authorization_url, delete_secret, exchange_oauth_code, health_check,
    keyring_available, list_model_descriptors, list_models as provider_list_models,
    list_models_with_overrides as provider_list_models_with_overrides, store_api_key,
    store_oauth_token,
};
use agent_storage::Storage;
use anyhow::{anyhow, bail, Context, Result};
use arboard::Clipboard;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect, Input, Password, Select};
use reqwest::{Client, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    name = "autism",
    bin_name = "autism",
    version,
    about = "Persistent local coding agent CLI",
    subcommand_negates_reqs = true,
    override_usage = "autism [OPTIONS] [PROMPT]\n       autism [OPTIONS] <COMMAND> [ARGS]"
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
enum McpCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(McpAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
}

#[derive(Subcommand)]
enum AppCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(AppAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
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
struct McpAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    command: String,
    #[arg(long = "arg")]
    args: Vec<String>,
    #[arg(long = "tool-name")]
    tool_name: String,
    #[arg(long = "schema-file")]
    schema_file: PathBuf,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
}

#[derive(Args)]
struct AppAddArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    command: String,
    #[arg(long = "arg")]
    args: Vec<String>,
    #[arg(long = "tool-name")]
    tool_name: String,
    #[arg(long = "schema-file")]
    schema_file: PathBuf,
    #[arg(long)]
    cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    enabled: bool,
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
        println!("A main model must be configured before autism can start.");
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
    !config.onboarding_complete || config.providers.is_empty() || !has_usable_main_alias(config)
}

fn has_usable_main_alias(config: &AppConfig) -> bool {
    config
        .main_alias()
        .ok()
        .is_some_and(|alias| config.get_provider(&alias.provider_id).is_some())
}

async fn ensure_onboarded(storage: &Storage) -> Result<()> {
    let config = storage.load_config()?;
    if !needs_onboarding(&config) {
        return Ok(());
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("no completed setup found; run `autism setup` in an interactive terminal first");
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
        format!(" >_ Autism CLI (v{version})"),
        String::new(),
        format!(" model:     {model_label}"),
        format!(" directory: {}", directory.display()),
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0) + 2;
    println!("╭{}╮", "─".repeat(width));
    for line in lines {
        println!("│ {:width$} │", line, width = width.saturating_sub(1));
    }
    println!("╰{}╯", "─".repeat(width));
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
        format!(" >_ Autism CLI (v{version})"),
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

async fn mcp_command(storage: &Storage, command: McpCommands) -> Result<()> {
    match command {
        McpCommands::List { json } => {
            let servers = load_mcp_servers(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&servers)?);
            } else {
                for server in servers {
                    println!(
                        "{} [{}] tool={} enabled={} cmd={} {}",
                        server.id,
                        server.name,
                        server.tool_name,
                        server.enabled,
                        server.command,
                        server.args.join(" ")
                    );
                }
            }
        }
        McpCommands::Get { id, json } => {
            let server = load_mcp_servers(storage)
                .await?
                .into_iter()
                .find(|server| server.id == id)
                .ok_or_else(|| anyhow!("unknown MCP server '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&server)?);
            } else {
                println!("id={}", server.id);
                println!("name={}", server.name);
                println!("tool_name={}", server.tool_name);
                println!("enabled={}", server.enabled);
                println!("command={} {}", server.command, server.args.join(" "));
                println!("schema={}", server.input_schema_json);
            }
        }
        McpCommands::Add(args) => {
            let server = McpServerConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                command: args.command,
                args: args.args,
                tool_name: args.tool_name,
                input_schema_json: load_schema_file(&args.schema_file)?,
                enabled: args.enabled,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: McpServerConfig = client
                    .post(
                        "/v1/mcp",
                        &McpServerUpsertRequest {
                            server: server.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_mcp_server(server.clone());
                storage.save_config(&config)?;
            }
            println!("mcp_server='{}' configured", args.id);
        }
        McpCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/mcp/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_mcp_server(&id) {
                    bail!("unknown MCP server '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("mcp_server='{}' removed", id);
        }
        McpCommands::Enable { id } => {
            set_mcp_enabled(storage, &id, true).await?;
            println!("mcp_server='{}' enabled", id);
        }
        McpCommands::Disable { id } => {
            set_mcp_enabled(storage, &id, false).await?;
            println!("mcp_server='{}' disabled", id);
        }
    }
    Ok(())
}

async fn app_command(storage: &Storage, command: AppCommands) -> Result<()> {
    match command {
        AppCommands::List { json } => {
            let connectors = load_app_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] tool={} enabled={} cmd={} {}",
                        connector.id,
                        connector.name,
                        connector.tool_name,
                        connector.enabled,
                        connector.command,
                        connector.args.join(" ")
                    );
                }
            }
        }
        AppCommands::Get { id, json } => {
            let connector = load_app_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown app connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("tool_name={}", connector.tool_name);
                println!("enabled={}", connector.enabled);
                println!("command={} {}", connector.command, connector.args.join(" "));
                println!("schema={}", connector.input_schema_json);
            }
        }
        AppCommands::Add(args) => {
            let connector = AppConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                command: args.command,
                args: args.args,
                tool_name: args.tool_name,
                input_schema_json: load_schema_file(&args.schema_file)?,
                enabled: args.enabled,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: AppConnectorConfig = client
                    .post(
                        "/v1/apps",
                        &AppConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_app_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!("app_connector='{}' configured", args.id);
        }
        AppCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/apps/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_app_connector(&id) {
                    bail!("unknown app connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("app_connector='{}' removed", id);
        }
        AppCommands::Enable { id } => {
            set_app_enabled(storage, &id, true).await?;
            println!("app_connector='{}' enabled", id);
        }
        AppCommands::Disable { id } => {
            set_app_enabled(storage, &id, false).await?;
            println!("app_connector='{}' disabled", id);
        }
    }
    Ok(())
}

async fn telegram_command(storage: &Storage, command: TelegramCommands) -> Result<()> {
    match command {
        TelegramCommands::List { json } => {
            let connectors = load_telegram_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} chats={} users={} alias={} model={} last_update_id={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
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
        TelegramCommands::Get { id, json } => {
            let connector = load_telegram_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!("chat_ids={}", format_i64_list(&connector.allowed_chat_ids));
                println!("user_ids={}", format_i64_list(&connector.allowed_user_ids));
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "last_update_id={}",
                    connector
                        .last_update_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        TelegramCommands::Add(args) => {
            let existing = load_telegram_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            let bot_token_keychain_account = match args.bot_token {
                Some(bot_token) => Some(store_api_key(
                    &format!("connector:telegram:{}", args.id),
                    &bot_token,
                )?),
                None => existing
                    .as_ref()
                    .and_then(|connector| connector.bot_token_keychain_account.clone()),
            };
            if bot_token_keychain_account.is_none() {
                bail!("--bot-token is required for new telegram connectors");
            }
            let connector = TelegramConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                allowed_chat_ids: args.chat_ids,
                allowed_user_ids: args.user_ids,
                last_update_id: existing.and_then(|connector| connector.last_update_id),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: TelegramConnectorConfig = client
                    .post(
                        "/v1/telegram",
                        &TelegramConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_telegram_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "telegram='{}' configured require_pairing_approval={} chats={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_i64_list(&connector.allowed_chat_ids),
                format_i64_list(&connector.allowed_user_ids)
            );
        }
        TelegramCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/telegram/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .telegram_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
                config.remove_telegram_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("telegram='{}' removed", id);
        }
        TelegramCommands::Enable { id } => {
            set_telegram_enabled(storage, &id, true).await?;
            println!("telegram='{}' enabled", id);
        }
        TelegramCommands::Disable { id } => {
            set_telegram_enabled(storage, &id, false).await?;
            println!("telegram='{}' disabled", id);
        }
        TelegramCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: TelegramPollResponse = client
                .post(&format!("/v1/telegram/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "telegram='{}' processed_updates={} queued_missions={} pending_approvals={} last_update_id={}",
                response.connector_id,
                response.processed_updates,
                response.queued_missions,
                response.pending_approvals,
                response
                    .last_update_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        TelegramCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: TelegramSendResponse = client
                .post(
                    &format!("/v1/telegram/{}/send", args.id),
                    &TelegramSendRequest {
                        chat_id: args.chat_id,
                        text: args.text,
                        disable_notification: args.disable_notification,
                    },
                )
                .await?;
            println!(
                "telegram='{}' sent message to chat={} message_id={}",
                response.connector_id,
                response.chat_id,
                response
                    .message_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        TelegramCommands::Approvals { command } => match command {
            TelegramApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Telegram, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            TelegramApprovalCommands::Approve { id, note } => {
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
            TelegramApprovalCommands::Reject { id, note } => {
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
        },
    }
    Ok(())
}

async fn discord_command(storage: &Storage, command: DiscordCommands) -> Result<()> {
    match command {
        DiscordCommands::List { json } => {
            let connectors = load_discord_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
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
        DiscordCommands::Get { id, json } => {
            let connector = load_discord_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_channel_ids={}",
                    format_string_list(&connector.monitored_channel_ids)
                );
                println!(
                    "allowed_channel_ids={}",
                    format_string_list(&connector.allowed_channel_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!(
                    "channel_cursors={}",
                    format_discord_channel_cursors(&connector.channel_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        DiscordCommands::Add(args) => {
            let existing = load_discord_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            let bot_token_keychain_account = match args.bot_token {
                Some(bot_token) => Some(store_api_key(
                    &format!("connector:discord:{}", args.id),
                    &bot_token,
                )?),
                None => existing
                    .as_ref()
                    .and_then(|connector| connector.bot_token_keychain_account.clone()),
            };
            if bot_token_keychain_account.is_none() {
                bail!("--bot-token is required for new discord connectors");
            }
            let connector = DiscordConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                monitored_channel_ids: args.monitored_channel_ids,
                allowed_channel_ids: args.allowed_channel_ids,
                allowed_user_ids: args.user_ids,
                channel_cursors: existing
                    .map(|connector| connector.channel_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: DiscordConnectorConfig = client
                    .post(
                        "/v1/discord",
                        &DiscordConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_discord_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "discord='{}' configured require_pairing_approval={} monitored_channels={} allowed_channels={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_string_list(&connector.monitored_channel_ids),
                format_string_list(&connector.allowed_channel_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        DiscordCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/discord/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .discord_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
                config.remove_discord_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("discord='{}' removed", id);
        }
        DiscordCommands::Enable { id } => {
            set_discord_enabled(storage, &id, true).await?;
            println!("discord='{}' enabled", id);
        }
        DiscordCommands::Disable { id } => {
            set_discord_enabled(storage, &id, false).await?;
            println!("discord='{}' disabled", id);
        }
        DiscordCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: DiscordPollResponse = client
                .post(&format!("/v1/discord/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "discord='{}' processed_messages={} queued_missions={} pending_approvals={} updated_channels={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals,
                response.updated_channels
            );
        }
        DiscordCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: DiscordSendResponse = client
                .post(
                    &format!("/v1/discord/{}/send", args.id),
                    &DiscordSendRequest {
                        channel_id: args.channel_id.clone(),
                        content: args.content.clone(),
                    },
                )
                .await?;
            println!(
                "discord='{}' sent message to channel={} message_id={}",
                response.connector_id,
                response.channel_id,
                response.message_id.as_deref().unwrap_or("-")
            );
        }
        DiscordCommands::Approvals { command } => match command {
            DiscordApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Discord, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            DiscordApprovalCommands::Approve { id, note } => {
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
            DiscordApprovalCommands::Reject { id, note } => {
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
        },
    }
    Ok(())
}

async fn slack_command(storage: &Storage, command: SlackCommands) -> Result<()> {
    match command {
        SlackCommands::List { json } => {
            let connectors = load_slack_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
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
        SlackCommands::Get { id, json } => {
            let connector = load_slack_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_channel_ids={}",
                    format_string_list(&connector.monitored_channel_ids)
                );
                println!(
                    "allowed_channel_ids={}",
                    format_string_list(&connector.allowed_channel_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!(
                    "channel_cursors={}",
                    format_slack_channel_cursors(&connector.channel_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        SlackCommands::Add(args) => {
            let existing = load_slack_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            let bot_token_keychain_account = match args.bot_token {
                Some(bot_token) => Some(store_api_key(
                    &format!("connector:slack:{}", args.id),
                    &bot_token,
                )?),
                None => existing
                    .as_ref()
                    .and_then(|connector| connector.bot_token_keychain_account.clone()),
            };
            if bot_token_keychain_account.is_none() {
                bail!("--bot-token is required for new slack connectors");
            }
            let connector = SlackConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                monitored_channel_ids: args.monitored_channel_ids,
                allowed_channel_ids: args.allowed_channel_ids,
                allowed_user_ids: args.user_ids,
                channel_cursors: existing
                    .map(|connector| connector.channel_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: SlackConnectorConfig = client
                    .post(
                        "/v1/slack",
                        &SlackConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_slack_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "slack='{}' configured require_pairing_approval={} monitored_channels={} allowed_channels={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_string_list(&connector.monitored_channel_ids),
                format_string_list(&connector.allowed_channel_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        SlackCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/slack/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .slack_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
                config.remove_slack_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("slack='{}' removed", id);
        }
        SlackCommands::Enable { id } => {
            set_slack_enabled(storage, &id, true).await?;
            println!("slack='{}' enabled", id);
        }
        SlackCommands::Disable { id } => {
            set_slack_enabled(storage, &id, false).await?;
            println!("slack='{}' disabled", id);
        }
        SlackCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: SlackPollResponse = client
                .post(&format!("/v1/slack/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "slack='{}' processed_messages={} queued_missions={} pending_approvals={} updated_channels={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals,
                response.updated_channels
            );
        }
        SlackCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: SlackSendResponse = client
                .post(
                    &format!("/v1/slack/{}/send", args.id),
                    &SlackSendRequest {
                        channel_id: args.channel_id.clone(),
                        text: args.text.clone(),
                    },
                )
                .await?;
            println!(
                "slack='{}' sent message to channel={} ts={}",
                response.connector_id,
                response.channel_id,
                response.message_ts.as_deref().unwrap_or("-")
            );
        }
        SlackCommands::Approvals { command } => match command {
            SlackApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Slack, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            SlackApprovalCommands::Approve { id, note } => {
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
            SlackApprovalCommands::Reject { id, note } => {
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
        },
    }
    Ok(())
}

async fn signal_command(storage: &Storage, command: SignalCommands) -> Result<()> {
    match command {
        SignalCommands::List { json } => {
            let connectors = load_signal_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} account={} cli_path={} require_pairing_approval={} monitored_groups={} allowed_groups={} users={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.account,
                        connector
                            .cli_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "signal-cli".to_string()),
                        connector.require_pairing_approval,
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
        SignalCommands::Get { id, json } => {
            let connector = load_signal_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown signal connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!("account={}", connector.account);
                println!(
                    "cli_path={}",
                    connector
                        .cli_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "signal-cli".to_string())
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_group_ids={}",
                    format_string_list(&connector.monitored_group_ids)
                );
                println!(
                    "allowed_group_ids={}",
                    format_string_list(&connector.allowed_group_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        SignalCommands::Add(args) => {
            let connector = SignalConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                account: args.account,
                cli_path: args.cli_path,
                require_pairing_approval: args.require_pairing_approval,
                monitored_group_ids: args.monitored_group_ids,
                allowed_group_ids: args.allowed_group_ids,
                allowed_user_ids: args.user_ids,
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: SignalConnectorConfig = client
                    .post(
                        "/v1/signal",
                        &SignalConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_signal_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "signal='{}' configured account={} monitored_groups={} allowed_groups={} users={} auto_poll=daemon",
                connector.id,
                connector.account,
                format_string_list(&connector.monitored_group_ids),
                format_string_list(&connector.allowed_group_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        SignalCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/signal/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let exists = config
                    .signal_connectors
                    .iter()
                    .any(|connector| connector.id == id);
                if !exists {
                    bail!("unknown signal connector '{id}'");
                }
                config.remove_signal_connector(&id);
                storage.save_config(&config)?;
            }
            println!("signal='{}' removed", id);
        }
        SignalCommands::Enable { id } => {
            set_signal_enabled(storage, &id, true).await?;
            println!("signal='{}' enabled", id);
        }
        SignalCommands::Disable { id } => {
            set_signal_enabled(storage, &id, false).await?;
            println!("signal='{}' disabled", id);
        }
        SignalCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: SignalPollResponse = client
                .post(&format!("/v1/signal/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "signal='{}' processed_messages={} queued_missions={} pending_approvals={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals
            );
        }
        SignalCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: SignalSendResponse = client
                .post(
                    &format!("/v1/signal/{}/send", args.id),
                    &SignalSendRequest {
                        recipient: args.recipient.clone(),
                        group_id: args.group_id.clone(),
                        text: args.text.clone(),
                    },
                )
                .await?;
            println!(
                "signal='{}' sent message to {}",
                response.connector_id, response.target
            );
        }
        SignalCommands::Approvals { command } => match command {
            SignalApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Signal, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            SignalApprovalCommands::Approve { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Approved,
                    note,
                )
                .await?;
                println!(
                    "approved signal pairing={} connector={} conversation={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
            SignalApprovalCommands::Reject { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Rejected,
                    note,
                )
                .await?;
                println!(
                    "rejected signal pairing={} connector={} conversation={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
        },
    }
    Ok(())
}

async fn home_assistant_command(storage: &Storage, command: HomeAssistantCommands) -> Result<()> {
    match command {
        HomeAssistantCommands::List { json } => {
            let connectors = load_home_assistant_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
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
        HomeAssistantCommands::Get { id, json } => {
            let connector = load_home_assistant_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "access_token_configured={}",
                    connector.access_token_keychain_account.is_some()
                );
                println!("base_url={}", connector.base_url);
                println!(
                    "monitored_entity_ids={}",
                    format_string_list(&connector.monitored_entity_ids)
                );
                println!(
                    "allowed_service_domains={}",
                    format_string_list(&connector.allowed_service_domains)
                );
                println!(
                    "allowed_service_entity_ids={}",
                    format_string_list(&connector.allowed_service_entity_ids)
                );
                println!(
                    "tracked_entities={}",
                    format_home_assistant_entity_cursors(&connector.entity_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        HomeAssistantCommands::Add(args) => {
            let existing = load_home_assistant_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            let access_token_keychain_account = match args.access_token {
                Some(access_token) => Some(store_api_key(
                    &format!("connector:home-assistant:{}", args.id),
                    &access_token,
                )?),
                None => existing
                    .as_ref()
                    .and_then(|connector| connector.access_token_keychain_account.clone()),
            };
            if access_token_keychain_account.is_none() {
                bail!("--access-token is required for new home assistant connectors");
            }
            let connector = HomeAssistantConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                base_url: args.base_url.trim_end_matches('/').to_string(),
                access_token_keychain_account,
                monitored_entity_ids: args.monitored_entity_ids,
                allowed_service_domains: args.allowed_service_domains,
                allowed_service_entity_ids: args.allowed_service_entity_ids,
                entity_cursors: existing
                    .map(|connector| connector.entity_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: HomeAssistantConnectorConfig = client
                    .post(
                        "/v1/home-assistant",
                        &HomeAssistantConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_home_assistant_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "home_assistant='{}' configured base_url={} monitored_entities={} service_domains={} service_entities={} auto_poll=daemon",
                args.id,
                connector.base_url,
                format_string_list(&connector.monitored_entity_ids),
                format_string_list(&connector.allowed_service_domains),
                format_string_list(&connector.allowed_service_entity_ids)
            );
        }
        HomeAssistantCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value =
                    client.delete(&format!("/v1/home-assistant/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .home_assistant_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
                config.remove_home_assistant_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.access_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("home_assistant='{}' removed", id);
        }
        HomeAssistantCommands::Enable { id } => {
            set_home_assistant_enabled(storage, &id, true).await?;
            println!("home_assistant='{}' enabled", id);
        }
        HomeAssistantCommands::Disable { id } => {
            set_home_assistant_enabled(storage, &id, false).await?;
            println!("home_assistant='{}' disabled", id);
        }
        HomeAssistantCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: HomeAssistantPollResponse = client
                .post(
                    &format!("/v1/home-assistant/{id}/poll"),
                    &serde_json::json!({}),
                )
                .await?;
            println!(
                "home_assistant='{}' processed_entities={} queued_missions={} updated_entities={}",
                response.connector_id,
                response.processed_entities,
                response.queued_missions,
                response.updated_entities
            );
        }
        HomeAssistantCommands::State { id, entity_id } => {
            let client = ensure_daemon(storage).await?;
            let state: HomeAssistantEntityState = client
                .get(&format!("/v1/home-assistant/{id}/entities/{entity_id}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        HomeAssistantCommands::CallService(args) => {
            let client = ensure_daemon(storage).await?;
            let service_data = args
                .service_data_json
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()
                .context("--service-data-json must be valid JSON")?;
            let response: HomeAssistantServiceCallResponse = client
                .post(
                    &format!("/v1/home-assistant/{}/services", args.id),
                    &HomeAssistantServiceCallRequest {
                        domain: args.domain.clone(),
                        service: args.service.clone(),
                        entity_id: args.entity_id.clone(),
                        service_data,
                    },
                )
                .await?;
            println!(
                "home_assistant='{}' called {}.{} changed_entities={}",
                response.connector_id, response.domain, response.service, response.changed_entities
            );
        }
    }
    Ok(())
}

async fn webhook_command(storage: &Storage, command: WebhookCommands) -> Result<()> {
    match command {
        WebhookCommands::List { json } => {
            let connectors = load_webhook_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} alias={} model={} token={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.alias.as_deref().unwrap_or("-"),
                        connector.requested_model.as_deref().unwrap_or("-"),
                        connector.token_sha256.is_some(),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        WebhookCommands::Get { id, json } => {
            let connector = load_webhook_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown webhook connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
                println!("token_configured={}", connector.token_sha256.is_some());
                println!("prompt_template={}", connector.prompt_template);
            }
        }
        WebhookCommands::Add(args) => {
            let prompt_template =
                load_prompt_template(args.prompt_template.as_deref(), args.prompt_file.as_ref())?;
            let token = args.token.unwrap_or_else(|| Uuid::new_v4().to_string());
            let connector = WebhookConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                prompt_template,
                enabled: args.enabled,
                token_sha256: Some(hash_webhook_token_local(&token)),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: WebhookConnectorConfig = client
                    .post(
                        "/v1/webhooks",
                        &WebhookConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_webhook_connector(connector.clone());
                storage.save_config(&config)?;
            }
            let config = storage.load_config()?;
            println!("webhook='{}' configured", args.id);
            println!(
                "url=http://{}:{}/v1/hooks/{}",
                config.daemon.host, config.daemon.port, args.id
            );
            println!("token={token}");
        }
        WebhookCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/webhooks/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_webhook_connector(&id) {
                    bail!("unknown webhook connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("webhook='{}' removed", id);
        }
        WebhookCommands::Enable { id } => {
            set_webhook_enabled(storage, &id, true).await?;
            println!("webhook='{}' enabled", id);
        }
        WebhookCommands::Disable { id } => {
            set_webhook_enabled(storage, &id, false).await?;
            println!("webhook='{}' disabled", id);
        }
        WebhookCommands::Deliver(args) => {
            let config = storage.load_config()?;
            let base_url = format!("http://{}:{}", config.daemon.host, config.daemon.port);
            let mut request = build_http_client()
                .post(format!("{base_url}/v1/hooks/{}", args.id))
                .json(&WebhookEventRequest {
                    summary: args.summary,
                    prompt: args.prompt,
                    details: args.details,
                    payload: match args.payload_file {
                        Some(path) => Some(load_json_file(&path)?),
                        None => None,
                    },
                });
            if let Some(token) = args.token {
                request = request.header("x-agent-webhook-token", token);
            }
            let response = request.send().await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                bail!("webhook delivery failed: {} {}", status, body);
            }
            let parsed: WebhookEventResponse =
                serde_json::from_str(&body).context("failed to parse webhook response")?;
            println!(
                "queued webhook mission={} title={} status={:?}",
                parsed.mission_id, parsed.title, parsed.status
            );
        }
    }
    Ok(())
}

async fn inbox_command(storage: &Storage, command: InboxCommands) -> Result<()> {
    match command {
        InboxCommands::List { json } => {
            let connectors = load_inbox_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
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
        InboxCommands::Get { id, json } => {
            let connector = load_inbox_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown inbox connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!("delete_after_read={}", connector.delete_after_read);
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!("path={}", connector.path.display());
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        InboxCommands::Add(args) => {
            let connector = InboxConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                path: args.path,
                enabled: args.enabled,
                delete_after_read: args.delete_after_read,
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: InboxConnectorConfig = client
                    .post(
                        "/v1/inboxes",
                        &InboxConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_inbox_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "inbox='{}' configured path={}",
                args.id,
                connector.path.display()
            );
        }
        InboxCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/inboxes/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_inbox_connector(&id) {
                    bail!("unknown inbox connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("inbox='{}' removed", id);
        }
        InboxCommands::Enable { id } => {
            set_inbox_enabled(storage, &id, true).await?;
            println!("inbox='{}' enabled", id);
        }
        InboxCommands::Disable { id } => {
            set_inbox_enabled(storage, &id, false).await?;
            println!("inbox='{}' disabled", id);
        }
        InboxCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: InboxPollResponse = client
                .post(&format!("/v1/inboxes/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "polled inbox={} processed_files={} queued_missions={}",
                response.connector_id, response.processed_files, response.queued_missions
            );
        }
    }
    Ok(())
}

async fn skills_command(storage: &Storage, command: SkillCommands) -> Result<()> {
    match command {
        SkillCommands::List => {
            let enabled = load_enabled_skills(storage).await?;
            for skill in discover_skills()? {
                let marker = if enabled.contains(&skill.name) {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{} {} - {} ({})",
                    marker,
                    skill.name,
                    skill.description,
                    skill.path.display()
                );
            }
        }
        SkillCommands::Enable { name } => {
            update_enabled_skill(storage, &name, true).await?;
            println!("skill='{}' enabled", name);
        }
        SkillCommands::Disable { name } => {
            update_enabled_skill(storage, &name, false).await?;
            println!("skill='{}' disabled", name);
        }
        SkillCommands::Drafts { limit, status } => {
            let drafts = load_skill_drafts(storage, limit, status.map(Into::into)).await?;
            if drafts.is_empty() {
                println!("no skill drafts");
            } else {
                for draft in drafts {
                    println!(
                        "{} [{:?}] uses={} provider={} workspace={}",
                        draft.id,
                        draft.status,
                        draft.usage_count,
                        draft.provider_id.as_deref().unwrap_or("-"),
                        draft.workspace_key.as_deref().unwrap_or("-")
                    );
                    println!("  {}", draft.title);
                    println!("  {}", draft.summary);
                }
            }
        }
        SkillCommands::Publish { id } => {
            let draft =
                update_skill_draft_status(storage, &id, SkillDraftStatus::Published).await?;
            println!("published skill draft={} title={}", draft.id, draft.title);
        }
        SkillCommands::Reject { id } => {
            let draft = update_skill_draft_status(storage, &id, SkillDraftStatus::Rejected).await?;
            println!("rejected skill draft={} title={}", draft.id, draft.title);
        }
    }
    Ok(())
}

async fn load_mcp_servers(storage: &Storage) -> Result<Vec<McpServerConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/mcp").await
    } else {
        Ok(storage.load_config()?.mcp_servers)
    }
}

async fn load_app_connectors(storage: &Storage) -> Result<Vec<AppConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/apps").await
    } else {
        Ok(storage.load_config()?.app_connectors)
    }
}

async fn load_telegram_connectors(storage: &Storage) -> Result<Vec<TelegramConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/telegram").await
    } else {
        Ok(storage.load_config()?.telegram_connectors)
    }
}

async fn load_discord_connectors(storage: &Storage) -> Result<Vec<DiscordConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/discord").await
    } else {
        Ok(storage.load_config()?.discord_connectors)
    }
}

async fn load_slack_connectors(storage: &Storage) -> Result<Vec<SlackConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/slack").await
    } else {
        Ok(storage.load_config()?.slack_connectors)
    }
}

async fn load_home_assistant_connectors(
    storage: &Storage,
) -> Result<Vec<HomeAssistantConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/home-assistant").await
    } else {
        Ok(storage.load_config()?.home_assistant_connectors)
    }
}

async fn load_signal_connectors(storage: &Storage) -> Result<Vec<SignalConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/signal").await
    } else {
        Ok(storage.load_config()?.signal_connectors)
    }
}

async fn load_webhook_connectors(storage: &Storage) -> Result<Vec<WebhookConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/webhooks").await
    } else {
        Ok(storage.load_config()?.webhook_connectors)
    }
}

async fn load_inbox_connectors(storage: &Storage) -> Result<Vec<InboxConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/inboxes").await
    } else {
        Ok(storage.load_config()?.inbox_connectors)
    }
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
            format!(
                "{} [{:?}/{:?}] {}{}{}\n  {}{}",
                memory.id,
                memory.kind,
                memory.scope,
                memory.subject,
                tags,
                review,
                memory.content,
                note
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

async fn set_mcp_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut servers = load_mcp_servers(storage).await?;
    let server = servers
        .iter_mut()
        .find(|server| server.id == id)
        .ok_or_else(|| anyhow!("unknown MCP server '{id}'"))?;
    server.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: McpServerConfig = client
            .post(
                "/v1/mcp",
                &McpServerUpsertRequest {
                    server: server.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_mcp_server(server.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_app_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_app_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown app connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: AppConnectorConfig = client
            .post(
                "/v1/apps",
                &AppConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_app_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_telegram_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_telegram_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: TelegramConnectorConfig = client
            .post(
                "/v1/telegram",
                &TelegramConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_telegram_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_discord_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_discord_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: DiscordConnectorConfig = client
            .post(
                "/v1/discord",
                &DiscordConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_discord_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_slack_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_slack_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: SlackConnectorConfig = client
            .post(
                "/v1/slack",
                &SlackConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_slack_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_home_assistant_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_home_assistant_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: HomeAssistantConnectorConfig = client
            .post(
                "/v1/home-assistant",
                &HomeAssistantConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_home_assistant_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_signal_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_signal_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown signal connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: SignalConnectorConfig = client
            .post(
                "/v1/signal",
                &SignalConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_signal_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_webhook_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_webhook_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown webhook connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: WebhookConnectorConfig = client
            .post(
                "/v1/webhooks",
                &WebhookConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_webhook_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_inbox_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_inbox_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown inbox connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: InboxConnectorConfig = client
            .post(
                "/v1/inboxes",
                &InboxConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_inbox_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
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

fn hash_webhook_token_local(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn format_i64_list(values: &[i64]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(",")
    }
}

fn format_discord_channel_cursors(values: &[DiscordChannelCursor]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| match cursor.last_message_id.as_deref() {
                Some(last_message_id) => format!("{}:{last_message_id}", cursor.channel_id),
                None => format!("{}:-", cursor.channel_id),
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn format_slack_channel_cursors(values: &[SlackChannelCursor]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| match cursor.last_message_ts.as_deref() {
                Some(last_message_ts) => format!("{}:{last_message_ts}", cursor.channel_id),
                None => format!("{}:-", cursor.channel_id),
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn format_home_assistant_entity_cursors(
    values: &[agent_core::HomeAssistantEntityCursor],
) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| {
                format!(
                    "{}:{}@{}",
                    cursor.entity_id,
                    cursor.last_state.as_deref().unwrap_or("-"),
                    cursor.last_changed.as_deref().unwrap_or("-")
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
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
                if config.get_provider(&payload.alias.provider_id).is_none() {
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
        attachments,
        args.permissions.map(Into::into),
        args.no_tui,
    )
    .await
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveCommand {
    Exit,
    Help,
    Status,
    ConfigShow,
    DashboardOpen,
    TelegramsShow,
    DiscordsShow,
    SlacksShow,
    SignalsShow,
    HomeAssistantsShow,
    TelegramApprovalsShow,
    TelegramApprove { id: String, note: Option<String> },
    TelegramReject { id: String, note: Option<String> },
    DiscordApprovalsShow,
    DiscordApprove { id: String, note: Option<String> },
    DiscordReject { id: String, note: Option<String> },
    SlackApprovalsShow,
    SlackApprove { id: String, note: Option<String> },
    SlackReject { id: String, note: Option<String> },
    WebhooksShow,
    InboxesShow,
    AutopilotShow,
    AutopilotEnable,
    AutopilotPause,
    AutopilotResume,
    MissionsShow,
    EventsShow(usize),
    Schedule { after_seconds: u64, title: String },
    Repeat { every_seconds: u64, title: String },
    Watch { path: PathBuf, title: String },
    ProfileShow,
    MemoryShow(Option<String>),
    MemoryReviewShow,
    MemoryApprove { id: String, note: Option<String> },
    MemoryReject { id: String, note: Option<String> },
    Remember(String),
    Forget(String),
    Skills(InteractiveSkillCommand),
    PermissionsShow,
    PermissionsSet(Option<PermissionPreset>),
    Attach(PathBuf),
    AttachmentsShow,
    AttachmentsClear,
    New,
    Clear,
    Diff,
    Copy,
    Compact,
    Init,
    ModelShow,
    ModelSet(String),
    ThinkingShow,
    ThinkingSet(Option<ThinkingLevel>),
    Fast,
    Rename(Option<String>),
    Review(Option<String>),
    Resume(Option<String>),
    Fork(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveSkillCommand {
    Show(Option<SkillDraftStatus>),
    Publish(String),
    Reject(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveModelSelection {
    Alias(String),
    Explicit(String),
}

async fn interactive_session(
    storage: &Storage,
    mut alias: Option<String>,
    mut session_id: Option<String>,
    initial_prompt: Option<String>,
    mut thinking_level: Option<ThinkingLevel>,
    mut attachments: Vec<InputAttachment>,
    mut permission_preset: Option<PermissionPreset>,
) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    let mut cwd =
        load_session_cwd(storage, session_id.as_deref())?.unwrap_or(current_request_cwd()?);
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
        "Interactive chat. Use /help for commands, /model to switch aliases or provider models, and /thinking to adjust reasoning."
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
                                    permission_preset,
                                    &attachments,
                                    cwd.as_path(),
                                )
                                .await?;
                            }
                            InteractiveCommand::ConfigShow => {
                                println!(
                                    "Settings:\n  /config opens the categorized settings menu in the TUI.\n  /dashboard opens the localhost web control room.\n  /model, /thinking, and /permissions are the quick line-mode shortcuts."
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
                                    for memory in result.memories {
                                        println!(
                                            "{} [{:?}/{:?}] {}",
                                            memory.id, memory.kind, memory.scope, memory.subject
                                        );
                                        println!("  {}", memory.content);
                                    }
                                    for hit in result.transcript_hits {
                                        println!(
                                            "session={} [{:?}] {}",
                                            hit.session_id, hit.role, hit.preview
                                        );
                                    }
                                } else {
                                    let memories: Vec<MemoryRecord> =
                                        client.get("/v1/memory?limit=10").await?;
                                    for memory in memories {
                                        println!(
                                            "{} [{:?}/{:?}] {}",
                                            memory.id, memory.kind, memory.scope, memory.subject
                                        );
                                        println!("  {}", memory.content);
                                    }
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
                            InteractiveCommand::ThinkingShow => {
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::ThinkingSet(new_level) => {
                                thinking_level = new_level;
                                persist_thinking_level(storage, thinking_level)?;
                                println!("thinking={}", thinking_level_label(thinking_level));
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
                                    "Resumed session={} title={} alias={} provider={} model={}",
                                    transcript.session.id,
                                    transcript.session.title.as_deref().unwrap_or("(untitled)"),
                                    transcript.session.alias,
                                    transcript.session.provider_id,
                                    transcript.session.model
                                );
                                last_output = latest_assistant_output_from_transcript(&transcript);
                                alias = Some(transcript.session.alias.clone());
                                session_id = Some(transcript.session.id.clone());
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &transcript.session.model,
                                )?;
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
        "Resuming session={} title={} alias={} provider={} model={}",
        transcript.session.id,
        transcript.session.title.as_deref().unwrap_or("(untitled)"),
        transcript.session.alias,
        transcript.session.provider_id,
        transcript.session.model
    );
    launch_chat_session(
        storage,
        Some(transcript.session.alias),
        Some(transcript.session.id),
        args.prompt,
        resolve_thinking_level(storage, args.thinking)?,
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
        Vec::new(),
        None,
        false,
    )
    .await
}

fn completion_command(args: CompletionArgs) {
    let mut command = Cli::command();
    generate(args.shell, &mut command, "autism", &mut io::stdout());
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

async fn reset_command(storage: &Storage, args: ResetArgs) -> Result<()> {
    if !args.yes {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            bail!(
                "reset is destructive; rerun with `autism reset --yes` in an interactive terminal"
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
        println!("Run `autism setup` in an interactive terminal to complete onboarding again.");
        return Ok(());
    }

    println!();
    println!("Restarting onboarding.");
    setup(storage).await
}

async fn session_command(storage: &Storage, command: SessionCommands) -> Result<()> {
    match command {
        SessionCommands::List => {
            for session in storage.list_sessions(50)? {
                println!(
                    "{} {} {} {} {} {} {}",
                    session.id,
                    session.title.as_deref().unwrap_or("(untitled)"),
                    session.alias,
                    session.provider_id,
                    session.model,
                    session
                        .cwd
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    session.updated_at
                );
            }
        }
        SessionCommands::Resume { id } => {
            let session = storage
                .get_session(&id)?
                .ok_or_else(|| anyhow!("unknown session"))?;
            let transcript = SessionTranscript {
                session,
                messages: storage.list_session_messages(&id)?,
            };
            println!(
                "session={} title={} alias={} provider={} model={}",
                transcript.session.id,
                transcript.session.title.as_deref().unwrap_or("(untitled)"),
                transcript.session.alias,
                transcript.session.provider_id,
                transcript.session.model
            );
            for message in transcript.messages {
                println!(
                    "[{:?}] {}",
                    message.role,
                    format_session_message_for_display(&message)
                );
            }
        }
        SessionCommands::Rename { id, title } => {
            let title = title.trim();
            if title.is_empty() {
                bail!("session title cannot be empty");
            }
            storage.rename_session(&id, title)?;
            println!("renamed session={} title={}", id, title);
        }
    }
    Ok(())
}

async fn autonomy_command(storage: &Storage, command: AutonomyCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        AutonomyCommands::Enable {
            mode,
            allow_self_edit,
        } => {
            let theme = ColorfulTheme::default();
            println!("{}", autonomy_warning());
            let first = Confirm::with_theme(&theme)
                .with_prompt("Enable Think For Yourself mode?")
                .default(false)
                .interact()?;
            if !first {
                bail!("autonomy enable cancelled");
            }
            let second = Confirm::with_theme(&theme)
                .with_prompt("Confirm that this mode can damage the system and burn API bandwidth without limits")
                .default(false)
                .interact()?;
            if !second {
                bail!("autonomy enable cancelled");
            }
            let status: agent_core::AutonomyProfile = client
                .post(
                    "/v1/autonomy/enable",
                    &AutonomyEnableRequest {
                        mode: Some(mode.into()),
                        allow_self_edit,
                    },
                )
                .await?;
            println!(
                "autonomy={} mode={} unlimited_usage={} full_network={} self_edit={}",
                autonomy_summary(status.state),
                agent_policy::autonomy_mode_summary(status.mode),
                status.unlimited_usage,
                status.full_network,
                status.allow_self_edit
            );
        }
        AutonomyCommands::Pause => {
            let status: agent_core::AutonomyProfile = client
                .post("/v1/autonomy/pause", &serde_json::json!({}))
                .await?;
            println!("autonomy={}", autonomy_summary(status.state));
        }
        AutonomyCommands::Resume => {
            let status: agent_core::AutonomyProfile = client
                .post("/v1/autonomy/resume", &serde_json::json!({}))
                .await?;
            println!("autonomy={}", autonomy_summary(status.state));
        }
        AutonomyCommands::Status => {
            let status: agent_core::AutonomyProfile = client.get("/v1/autonomy/status").await?;
            println!(
                "state={} mode={} unlimited_usage={} full_network={} self_edit={} consented_at={:?}",
                autonomy_summary(status.state),
                agent_policy::autonomy_mode_summary(status.mode),
                status.unlimited_usage,
                status.full_network,
                status.allow_self_edit,
                status.consented_at
            );
        }
    }
    Ok(())
}

async fn evolve_command(storage: &Storage, command: EvolveCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        EvolveCommands::Start {
            alias,
            model,
            budget_friendly,
        } => {
            let theme = ColorfulTheme::default();
            println!(
                "Evolve mode will let the agent methodically improve its own code with free thinking, self-edit, background shell/network access, and test-gated iterative changes."
            );
            let confirmed = Confirm::with_theme(&theme)
                .with_prompt("Start evolve mode?")
                .default(false)
                .interact()?;
            if !confirmed {
                bail!("evolve start cancelled");
            }
            let status: EvolveConfig = client
                .post(
                    "/v1/evolve/start",
                    &EvolveStartRequest {
                        alias,
                        requested_model: model,
                        budget_friendly: Some(budget_friendly),
                    },
                )
                .await?;
            println!(
                "evolve state={} mission={} iteration={} stop_policy={:?}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-"),
                status.iteration,
                status.stop_policy
            );
        }
        EvolveCommands::Pause => {
            let status: EvolveConfig = client
                .post("/v1/evolve/pause", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} mission={}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Resume => {
            let status: EvolveConfig = client
                .post("/v1/evolve/resume", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} mission={}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Stop => {
            let status: EvolveConfig = client
                .post("/v1/evolve/stop", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} last_summary={}",
                serde_json::to_value(&status.state)?,
                status.last_summary.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Status => {
            let status: EvolveConfig = client.get("/v1/evolve/status").await?;
            println!(
                "state={} stop_policy={:?} mission={} iteration={} pending_restart={} alias={} model={} last_goal={} last_summary={}",
                serde_json::to_value(&status.state)?,
                status.stop_policy,
                status.current_mission_id.as_deref().unwrap_or("-"),
                status.iteration,
                status.pending_restart,
                status.alias.as_deref().unwrap_or("-"),
                status.requested_model.as_deref().unwrap_or("-"),
                status.last_goal.as_deref().unwrap_or("-"),
                status.last_summary.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

async fn autopilot_command(storage: &Storage, command: AutopilotCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        AutopilotCommands::Enable => {
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
        AutopilotCommands::Pause => {
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
        AutopilotCommands::Resume => {
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
        AutopilotCommands::Status => {
            let status: AutopilotConfig = client.get("/v1/autopilot/status").await?;
            println!("{}", autopilot_summary(&status));
        }
        AutopilotCommands::Config {
            interval_seconds,
            max_concurrent,
            allow_shell,
            allow_network,
            allow_self_edit,
        } => {
            let status: AutopilotConfig = client
                .put(
                    "/v1/autopilot/status",
                    &AutopilotUpdateRequest {
                        state: None,
                        max_concurrent_missions: max_concurrent,
                        wake_interval_seconds: interval_seconds,
                        allow_background_shell: allow_shell,
                        allow_background_network: allow_network,
                        allow_background_self_edit: allow_self_edit,
                    },
                )
                .await?;
            println!("{}", autopilot_summary(&status));
        }
    }
    Ok(())
}

async fn mission_command(storage: &Storage, command: MissionCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        MissionCommands::Add {
            title,
            details,
            alias,
            model,
            after_seconds,
            every_seconds,
            at,
            watch,
            watch_nonrecursive,
        } => {
            let cwd = current_request_cwd()?;
            let watch_path = resolve_watch_path(watch.as_deref(), &cwd)?;
            if watch_path.is_some()
                && (after_seconds.is_some() || at.is_some() || every_seconds.is_some())
            {
                bail!("use either --watch or timer settings (--after-seconds/--at/--every-seconds), not both");
            }
            let mut mission = Mission::new(title, details);
            mission.alias = alias;
            mission.requested_model = model;
            mission.repeat_interval_seconds = every_seconds.filter(|seconds| *seconds > 0);
            mission.wake_at = resolve_mission_wake_at(after_seconds, at.as_deref())?;
            if mission.wake_at.is_some() || mission.repeat_interval_seconds.is_some() {
                mission.status = MissionStatus::Scheduled;
                mission.wake_trigger = Some(WakeTrigger::Timer);
            }
            mission.workspace_key = Some(cwd.display().to_string());
            mission.watch_path = watch_path;
            mission.watch_recursive = mission.watch_path.is_some() && !watch_nonrecursive;
            if mission.watch_path.is_some() {
                mission.status = MissionStatus::Waiting;
                mission.wake_trigger = Some(WakeTrigger::FileChange);
                mission.wake_at = None;
            }
            let mission: Mission = client.post("/v1/missions", &mission).await?;
            println!(
                "mission={} status={:?} created_at={} wake_at={} repeat={} watch={}",
                mission.id,
                mission.status,
                mission.created_at,
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
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        MissionCommands::List => {
            for mission in client.get::<Vec<Mission>>("/v1/missions").await? {
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
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    mission.retries,
                    mission.max_retries
                );
                if !mission.details.is_empty() {
                    println!("  {}", mission.details);
                }
            }
        }
        MissionCommands::Pause { id, note } => {
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/pause"),
                    &MissionControlRequest {
                        wake_at: None,
                        clear_wake_at: false,
                        repeat_interval_seconds: None,
                        clear_repeat_interval_seconds: false,
                        watch_path: None,
                        clear_watch_path: false,
                        watch_recursive: None,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Resume {
            id,
            after_seconds,
            every_seconds,
            at,
            watch,
            watch_nonrecursive,
            note,
        } => {
            let cwd = current_request_cwd()?;
            let watch_path = resolve_watch_path(watch.as_deref(), &cwd)?;
            if watch_path.is_some()
                && (after_seconds.is_some() || at.is_some() || every_seconds.is_some())
            {
                bail!("use either --watch or timer settings (--after-seconds/--at/--every-seconds), not both");
            }
            let wake_at = resolve_mission_wake_at(after_seconds, at.as_deref())?;
            let watch_recursive = watch_path.as_ref().map(|_| !watch_nonrecursive);
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/resume"),
                    &MissionControlRequest {
                        wake_at,
                        clear_wake_at: false,
                        repeat_interval_seconds: every_seconds,
                        clear_repeat_interval_seconds: false,
                        watch_path,
                        clear_watch_path: false,
                        watch_recursive,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Cancel { id, note } => {
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/cancel"),
                    &MissionControlRequest {
                        wake_at: None,
                        clear_wake_at: false,
                        repeat_interval_seconds: None,
                        clear_repeat_interval_seconds: false,
                        watch_path: None,
                        clear_watch_path: false,
                        watch_recursive: None,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Checkpoints { id, limit } => {
            let checkpoints: Vec<MissionCheckpoint> = client
                .get(&format!("/v1/missions/{id}/checkpoints?limit={limit}"))
                .await?;
            for checkpoint in checkpoints.into_iter().rev() {
                println!(
                    "{} [{:?}] {}",
                    checkpoint.created_at, checkpoint.status, checkpoint.summary
                );
                if let Some(session_id) = checkpoint.session_id {
                    println!("  session={}", session_id);
                }
            }
        }
    }
    Ok(())
}

async fn memory_command(storage: &Storage, command: MemoryCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        MemoryCommands::List { limit } => {
            let memories: Vec<MemoryRecord> =
                client.get(&format!("/v1/memory?limit={limit}")).await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Review { limit } => {
            let memories: Vec<MemoryRecord> = client
                .get(&format!("/v1/memory/review?limit={limit}"))
                .await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Approve { id, note } => {
            let memory: MemoryRecord = client
                .post(
                    &format!("/v1/memory/{id}/approve"),
                    &MemoryReviewUpdateRequest {
                        status: MemoryReviewStatus::Accepted,
                        note,
                    },
                )
                .await?;
            println!("approved memory={} subject={}", memory.id, memory.subject);
        }
        MemoryCommands::Reject { id, note } => {
            let memory: MemoryRecord = client
                .post(
                    &format!("/v1/memory/{id}/reject"),
                    &MemoryReviewUpdateRequest {
                        status: MemoryReviewStatus::Rejected,
                        note,
                    },
                )
                .await?;
            println!("rejected memory={} subject={}", memory.id, memory.subject);
        }
        MemoryCommands::Profile { limit } => {
            let memories = load_profile_memories(storage, limit).await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Search { query, limit } => {
            let response: MemorySearchResponse = client
                .post(
                    "/v1/memory/search",
                    &MemorySearchQuery {
                        query,
                        limit: Some(limit),
                        workspace_key: Some(current_request_cwd()?.display().to_string()),
                        provider_id: None,
                        review_statuses: Vec::new(),
                        include_superseded: false,
                    },
                )
                .await?;
            for memory in response.memories {
                println!(
                    "{} [{} {:?}/{:?}] {}",
                    memory.id, memory.confidence, memory.kind, memory.scope, memory.subject
                );
                println!("  {}", memory.content);
            }
            for hit in response.transcript_hits {
                println!(
                    "session={} [{:?}] {}",
                    hit.session_id, hit.role, hit.preview
                );
            }
        }
        MemoryCommands::Remember {
            subject,
            content,
            kind,
            scope,
        } => {
            let memory: MemoryRecord = client
                .post(
                    "/v1/memory",
                    &MemoryUpsertRequest {
                        kind: kind.into(),
                        scope: scope.into(),
                        subject,
                        content,
                        confidence: Some(100),
                        source_session_id: None,
                        source_message_id: None,
                        provider_id: None,
                        workspace_key: Some(current_request_cwd()?.display().to_string()),
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
        MemoryCommands::Forget { id } => {
            let _: serde_json::Value = client.delete(&format!("/v1/memory/{id}")).await?;
            println!("forgot memory={}", id);
        }
    }
    Ok(())
}

async fn logs_command(storage: &Storage, limit: usize, follow: bool) -> Result<()> {
    if follow {
        return follow_events_command(storage, limit).await;
    }

    for entry in storage.list_logs(limit)?.into_iter().rev() {
        print_log_entry(&entry);
    }
    Ok(())
}

async fn follow_events_command(storage: &Storage, limit: usize) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    let mut seen = HashSet::new();
    let mut cursor = None;

    loop {
        let path = event_feed_path(cursor.as_ref(), limit, 30);
        let events: Vec<agent_core::LogEntry> = client.get(&path).await?;
        if events.is_empty() {
            continue;
        }

        for entry in events {
            if !seen.insert(entry.id.clone()) {
                continue;
            }
            cursor = Some(entry.created_at);
            print_log_entry(&entry);
        }
        io::stdout().flush().ok();
    }
}

async fn dashboard_command(storage: &Storage, args: DashboardArgs) -> Result<()> {
    let _ = ensure_daemon(storage).await?;
    let config = storage.load_config()?;
    let url = format!(
        "http://{}:{}/ui?token={}",
        config.daemon.host, config.daemon.port, config.daemon.token
    );

    if args.print_url || args.no_open {
        println!("{url}");
    }

    if !args.no_open {
        match webbrowser::open(&url) {
            Ok(_) => {
                if !args.print_url {
                    println!("{url}");
                }
            }
            Err(error) => {
                println!("{url}");
                return Err(anyhow!("failed to open dashboard in browser: {error}"));
            }
        }
    }

    Ok(())
}

async fn doctor_command(storage: &Storage) -> Result<()> {
    let config = storage.load_config()?;
    let client = build_http_client();
    let providers = futures::future::join_all(
        config
            .providers
            .iter()
            .map(|provider| health_check(&client, provider)),
    )
    .await;
    let report = HealthReport {
        daemon_running: try_daemon(storage).await?.is_some(),
        config_path: storage.paths().config_path.display().to_string(),
        data_path: storage.paths().data_dir.display().to_string(),
        keyring_ok: keyring_available(),
        providers,
    };
    println!("daemon_running={}", report.daemon_running);
    println!("config_path={}", report.config_path);
    println!("data_path={}", report.data_path);
    println!("keyring_ok={}", report.keyring_ok);
    for provider in report.providers {
        println!(
            "{} ok={} detail={}",
            provider.id, provider.ok, provider.detail
        );
    }
    Ok(())
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

    bail!("daemon did not stop in time; run `autism daemon stop` and retry reset")
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

fn parse_interactive_command(line: &str) -> Result<Option<InteractiveCommand>> {
    if !line.starts_with('/') {
        return Ok(None);
    }

    let body = &line[1..];
    let mut parts = body.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let args = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let parsed = match command {
        "exit" | "quit" => InteractiveCommand::Exit,
        "help" => InteractiveCommand::Help,
        "status" => InteractiveCommand::Status,
        "config" | "settings" => InteractiveCommand::ConfigShow,
        "dashboard" | "ui" => InteractiveCommand::DashboardOpen,
        "telegram" | "telegrams" => parse_telegram_interactive_command(args)?,
        "discord" | "discords" => parse_discord_interactive_command(args)?,
        "slack" | "slacks" => parse_slack_interactive_command(args)?,
        "signal" | "signals" => parse_signal_interactive_command(args)?,
        "home-assistant" | "home-assistants" | "homeassistant" | "homeassistants" | "ha" => {
            parse_home_assistant_interactive_command(args)?
        }
        "webhooks" => InteractiveCommand::WebhooksShow,
        "inboxes" => InteractiveCommand::InboxesShow,
        "autopilot" => match args.map(|value| value.to_ascii_lowercase()) {
            Some(value) if value == "on" || value == "enable" => {
                InteractiveCommand::AutopilotEnable
            }
            Some(value) if value == "pause" => InteractiveCommand::AutopilotPause,
            Some(value) if value == "resume" => InteractiveCommand::AutopilotResume,
            Some(value) if value == "status" => InteractiveCommand::AutopilotShow,
            Some(_) | None => InteractiveCommand::AutopilotShow,
        },
        "missions" => InteractiveCommand::MissionsShow,
        "events" => InteractiveCommand::EventsShow(parse_optional_limit(args, 10)?),
        "schedule" => {
            let (after_seconds, title) = parse_schedule_command_args(args)?;
            InteractiveCommand::Schedule {
                after_seconds,
                title,
            }
        }
        "repeat" => {
            let (every_seconds, title) = parse_repeat_command_args(args)?;
            InteractiveCommand::Repeat {
                every_seconds,
                title,
            }
        }
        "watch" => {
            let (path, title) = parse_watch_command_args(args)?;
            InteractiveCommand::Watch { path, title }
        }
        "profile" => InteractiveCommand::ProfileShow,
        "memory" => parse_memory_interactive_command(args)?,
        "remember" => InteractiveCommand::Remember(
            args.ok_or_else(|| anyhow!("usage: /remember <text>"))?
                .to_string(),
        ),
        "forget" => InteractiveCommand::Forget(
            args.ok_or_else(|| anyhow!("usage: /forget <memory-id>"))?
                .to_string(),
        ),
        "skills" => InteractiveCommand::Skills(parse_interactive_skill_command(args)?),
        "permissions" | "approvals" => match args {
            Some(value) => {
                InteractiveCommand::PermissionsSet(Some(parse_permission_preset(value)?))
            }
            None => InteractiveCommand::PermissionsShow,
        },
        "attach" => InteractiveCommand::Attach(PathBuf::from(
            args.ok_or_else(|| anyhow!("usage: /attach <path>"))?,
        )),
        "attachments" => InteractiveCommand::AttachmentsShow,
        "detach" | "attachments-clear" => InteractiveCommand::AttachmentsClear,
        "new" => InteractiveCommand::New,
        "clear" => InteractiveCommand::Clear,
        "diff" => InteractiveCommand::Diff,
        "copy" => InteractiveCommand::Copy,
        "compact" => InteractiveCommand::Compact,
        "init" => InteractiveCommand::Init,
        "alias" | "model" => match args {
            Some(value) => InteractiveCommand::ModelSet(value.to_string()),
            None => InteractiveCommand::ModelShow,
        },
        "thinking" => match args {
            Some(value) => InteractiveCommand::ThinkingSet(parse_thinking_setting(value)?),
            None => InteractiveCommand::ThinkingShow,
        },
        "fast" => InteractiveCommand::Fast,
        "rename" => InteractiveCommand::Rename(args.map(ToOwned::to_owned)),
        "review" => InteractiveCommand::Review(args.map(ToOwned::to_owned)),
        "resume" => InteractiveCommand::Resume(args.map(ToOwned::to_owned)),
        "fork" => InteractiveCommand::Fork(args.map(ToOwned::to_owned)),
        other => bail!("unknown slash command '/{other}'. Use /help to list commands."),
    };

    Ok(Some(parsed))
}

fn parse_telegram_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::TelegramsShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::TelegramsShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::TelegramApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /telegram approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::TelegramApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /telegram reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::TelegramReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::TelegramsShow),
    }
}

fn parse_discord_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::DiscordsShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::DiscordsShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::DiscordApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /discord approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::DiscordApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /discord reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::DiscordReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::DiscordsShow),
    }
}

fn parse_slack_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::SlacksShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::SlacksShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::SlackApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /slack approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::SlackApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /slack reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::SlackReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::SlacksShow),
    }
}

fn parse_home_assistant_interactive_command(_args: Option<&str>) -> Result<InteractiveCommand> {
    Ok(InteractiveCommand::HomeAssistantsShow)
}

fn parse_signal_interactive_command(_args: Option<&str>) -> Result<InteractiveCommand> {
    Ok(InteractiveCommand::SignalsShow)
}

fn parse_memory_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::MemoryShow(None));
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::MemoryShow(None));
    }
    match action.to_ascii_lowercase().as_str() {
        "review" => Ok(InteractiveCommand::MemoryReviewShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /memory approve <memory-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::MemoryApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /memory reject <memory-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::MemoryReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::MemoryShow(Some(args.to_string()))),
    }
}

fn parse_interactive_skill_command(args: Option<&str>) -> Result<InteractiveSkillCommand> {
    let Some(args) = args else {
        return Ok(InteractiveSkillCommand::Show(None));
    };
    let mut parts = args.splitn(2, char::is_whitespace);
    let action = parts.next().unwrap_or_default().to_ascii_lowercase();
    let remainder = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match action.as_str() {
        "drafts" | "list" => Ok(InteractiveSkillCommand::Show(None)),
        "published" => Ok(InteractiveSkillCommand::Show(Some(
            SkillDraftStatus::Published,
        ))),
        "rejected" => Ok(InteractiveSkillCommand::Show(Some(
            SkillDraftStatus::Rejected,
        ))),
        "publish" => Ok(InteractiveSkillCommand::Publish(
            remainder
                .ok_or_else(|| anyhow!("usage: /skills publish <draft-id>"))?
                .to_string(),
        )),
        "reject" => Ok(InteractiveSkillCommand::Reject(
            remainder
                .ok_or_else(|| anyhow!("usage: /skills reject <draft-id>"))?
                .to_string(),
        )),
        _ => Ok(InteractiveSkillCommand::Show(None)),
    }
}

fn parse_thinking_setting(value: &str) -> Result<Option<ThinkingLevel>> {
    if value.eq_ignore_ascii_case("default") {
        return Ok(None);
    }

    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "none" => Ok(Some(ThinkingLevel::None)),
        "minimal" => Ok(Some(ThinkingLevel::Minimal)),
        "low" => Ok(Some(ThinkingLevel::Low)),
        "medium" => Ok(Some(ThinkingLevel::Medium)),
        "high" => Ok(Some(ThinkingLevel::High)),
        "xhigh" | "x-high" | "extra-high" => Ok(Some(ThinkingLevel::XHigh)),
        _ => bail!("unknown thinking level '{value}'"),
    }
}

fn parse_optional_limit(value: Option<&str>, default: usize) -> Result<usize> {
    match value {
        Some(value) => value
            .parse::<usize>()
            .with_context(|| format!("invalid limit '{value}'")),
        None => Ok(default),
    }
}

fn parse_schedule_command_args(args: Option<&str>) -> Result<(u64, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let delay = parts
        .next()
        .ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let after_seconds = delay
        .parse::<u64>()
        .with_context(|| format!("invalid schedule delay '{delay}'"))?;
    Ok((after_seconds, title.to_string()))
}

fn parse_repeat_command_args(args: Option<&str>) -> Result<(u64, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let interval = parts
        .next()
        .ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let every_seconds = interval
        .parse::<u64>()
        .with_context(|| format!("invalid repeat interval '{interval}'"))?;
    Ok((every_seconds, title.to_string()))
}

fn parse_watch_command_args(args: Option<&str>) -> Result<(PathBuf, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let path = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    Ok((PathBuf::from(path), title.to_string()))
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

fn print_interactive_help() {
    println!("Available commands:");
    println!("/help                     show this help");
    println!("/config                   open the categorized settings menu");
    println!("/dashboard                open the localhost web control room");
    println!("/telegrams                list configured Telegram connectors");
    println!("/discords                 list configured Discord connectors");
    println!("/slacks                   list configured Slack connectors");
    println!("/signals                  list configured Signal connectors");
    println!("/home-assistant           list configured Home Assistant connectors");
    println!("/telegram approvals       list pending Telegram pairing approvals");
    println!("/telegram approve <id>    approve a Telegram pairing request");
    println!("/telegram reject <id>     reject a Telegram pairing request");
    println!("/discord approvals        list pending Discord pairing approvals");
    println!("/discord approve <id>     approve a Discord pairing request");
    println!("/discord reject <id>      reject a Discord pairing request");
    println!("/slack approvals          list pending Slack pairing approvals");
    println!("/slack approve <id>       approve a Slack pairing request");
    println!("/slack reject <id>        reject a Slack pairing request");
    println!("/webhooks                 list configured webhook connectors");
    println!("/inboxes                  list configured inbox connectors");
    println!("/autopilot [on|pause|resume|status] control the background mission runner");
    println!("/missions                 list background missions");
    println!("/events [limit]           show recent daemon events");
    println!("/schedule <seconds> <title> create a scheduled background mission");
    println!("/repeat <seconds> <title> create a recurring background mission");
    println!("/watch <path> <title>     create a filesystem-triggered background mission");
    println!("/profile                  show learned resident profile memory");
    println!("/memory [query]           list recent memory or search memory/transcripts");
    println!("/memory review            show candidate memories awaiting review");
    println!("/memory approve <id>      approve a candidate memory");
    println!("/memory reject <id>       reject a candidate memory");
    println!("/remember <text>          store a manual long-term memory note");
    println!("/forget <memory-id>       delete a stored memory");
    println!("/skills [drafts|published|rejected] list learned skill drafts");
    println!("/skills publish <id>      approve a learned skill draft");
    println!("/skills reject <id>       discard a learned skill draft");
    println!("/model [name]             show provider models or switch alias/model");
    println!("/fast                     set thinking to minimal");
    println!("/thinking [level]         open the thinking picker or set thinking: default, none, minimal, low, medium, high, xhigh");
    println!("/status                   show session, model, daemon, and thinking state");
    println!("/permissions [preset]     open the permissions picker or set permissions: suggest, auto-edit, full-auto");
    println!("/attach <path>            attach an image to the next prompt(s)");
    println!("/attachments              list current image attachments");
    println!("/detach                   clear current image attachments");
    println!("/copy                     copy the latest assistant output to the clipboard");
    println!("/compact                  summarize the current session into a smaller fork");
    println!("/init                     create an AGENTS.md starter file in the current directory");
    println!("/rename [title]           rename the current session");
    println!("/review [instructions]    review current uncommitted changes");
    println!("/diff                     print the current uncommitted git diff");
    println!("/resume [last|session]    resume another recorded session");
    println!("/fork [last|session]      fork the current or selected session");
    println!("/new                      start a new chat");
    println!("/clear                    clear the terminal and start a new chat");
    println!("!<command>                run a local shell command in the current directory");
    println!("/exit                     quit the interactive session");
}

fn clear_terminal() {
    print!("\x1B[2J\x1B[H");
    let _ = io::stdout().flush();
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

fn resolve_requested_model_override(
    storage: &Storage,
    alias: Option<&str>,
    actual_model: &str,
) -> Result<Option<String>> {
    let config = storage.load_config()?;
    let active_alias = resolve_active_alias(&config, alias)?;
    Ok((actual_model != active_alias.model).then(|| actual_model.to_string()))
}

fn resolve_session_model_override(
    storage: &Storage,
    session_id: Option<&str>,
    alias: Option<&str>,
) -> Result<Option<String>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let Some(session) = storage.get_session(session_id)? else {
        return Ok(None);
    };
    resolve_requested_model_override(storage, alias, &session.model)
}

async fn interactive_model_choices_text(
    storage: &Storage,
    current_alias: Option<&str>,
    requested_model: Option<&str>,
) -> Result<String> {
    let config = storage.load_config()?;
    let active_alias = resolve_active_alias(&config, current_alias)?;
    let provider = config
        .get_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let selected_model = resolved_requested_model(active_alias, requested_model);
    let mut lines = vec![
        format!("current alias: {}", active_alias.alias),
        format!("provider: {}", provider.display_name),
        format!("selected model: {}", selected_model),
    ];

    match timeout(
        Duration::from_secs(3),
        list_model_descriptors(&build_http_client(), provider),
    )
    .await
    {
        Ok(Ok(models)) if !models.is_empty() => {
            lines.push(String::new());
            lines.push("provider models:".to_string());
            for model in models {
                let marker = if model.id == selected_model { "*" } else { " " };
                let display_name = model.display_name.as_deref().unwrap_or(model.id.as_str());
                let suffix = match (model.context_window, model.effective_context_window_percent) {
                    (Some(window), Some(percent)) => {
                        format!(" | ctx {} @ {}%", format_tokens_compact(window), percent)
                    }
                    (Some(window), None) => {
                        format!(" | ctx {}", format_tokens_compact(window))
                    }
                    _ => String::new(),
                };

                if display_name == model.id {
                    lines.push(format!("{marker} {}{}", model.id, suffix));
                } else {
                    lines.push(format!(
                        "{marker} {} ({}){}",
                        display_name, model.id, suffix
                    ));
                }
            }
        }
        Ok(Ok(_)) => {
            lines.push(String::new());
            lines.push("provider models: (none returned)".to_string());
        }
        Ok(Err(error)) => {
            lines.push(String::new());
            lines.push(format!("provider models unavailable: {error:#}"));
        }
        Err(_) => {
            lines.push(String::new());
            lines.push("provider models unavailable: request timed out".to_string());
        }
    }

    if !config.aliases.is_empty() {
        lines.push(String::new());
        lines.push("configured aliases:".to_string());
        for alias in &config.aliases {
            let marker = if current_alias == Some(alias.alias.as_str()) && requested_model.is_none()
            {
                "*"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {} -> {} / {}",
                alias.alias, alias.provider_id, alias.model
            ));
        }
    }

    Ok(lines.join("\n"))
}

async fn resolve_interactive_model_selection(
    storage: &Storage,
    current_alias: Option<&str>,
    value: &str,
) -> Result<InteractiveModelSelection> {
    let config = storage.load_config()?;
    if config.get_alias(value).is_some() {
        return Ok(InteractiveModelSelection::Alias(value.to_string()));
    }

    let active_alias = resolve_active_alias(&config, current_alias)?;
    let provider = config
        .get_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let normalized = normalize_model_selection_value(value);

    let resolved_model = match timeout(
        Duration::from_secs(3),
        list_model_descriptors(&build_http_client(), provider),
    )
    .await
    {
        Ok(Ok(models)) => models
            .into_iter()
            .find(|model| {
                model.id.eq_ignore_ascii_case(value)
                    || normalize_model_selection_value(&model.id) == normalized
                    || model.display_name.as_deref().is_some_and(|name| {
                        name.eq_ignore_ascii_case(value)
                            || normalize_model_selection_value(name) == normalized
                    })
            })
            .map(|model| model.id)
            .unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    };

    Ok(InteractiveModelSelection::Explicit(resolved_model))
}

fn normalize_model_selection_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn format_tokens_compact(value: i64) -> String {
    let value = value.max(0);
    if value == 0 {
        return "0".to_string();
    }
    if value < 1_000 {
        return value.to_string();
    }

    let value_f64 = value as f64;
    let (scaled, suffix) = if value >= 1_000_000_000_000 {
        (value_f64 / 1_000_000_000_000.0, "T")
    } else if value >= 1_000_000_000 {
        (value_f64 / 1_000_000_000.0, "B")
    } else if value >= 1_000_000 {
        (value_f64 / 1_000_000.0, "M")
    } else {
        (value_f64 / 1_000.0, "K")
    };

    let decimals = if scaled < 10.0 {
        2
    } else if scaled < 100.0 {
        1
    } else {
        0
    };

    let mut formatted = format!("{scaled:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    format!("{formatted}{suffix}")
}

fn build_uncommitted_review_prompt(custom_prompt: Option<String>) -> Result<String> {
    build_review_prompt(&ReviewArgs {
        uncommitted: true,
        base: None,
        commit: None,
        commit_title: None,
        prompt: custom_prompt,
        thinking: None,
    })
}

fn build_uncommitted_diff() -> Result<String> {
    collect_review_target(&ReviewArgs {
        uncommitted: true,
        base: None,
        commit: None,
        commit_title: None,
        prompt: None,
        thinking: None,
    })
}

fn load_transcript_for_interactive_resume(
    storage: &Storage,
    target: Option<&str>,
) -> Result<SessionTranscript> {
    match target {
        Some("last") => load_session_for_command(storage, None, true, false),
        Some(session_id) => {
            load_session_for_command(storage, Some(session_id.to_string()), false, false)
        }
        None => load_session_for_command(storage, None, false, true),
    }
}

fn load_transcript_for_interactive_fork(
    storage: &Storage,
    current_session_id: Option<&str>,
    target: Option<&str>,
) -> Result<SessionTranscript> {
    match target {
        Some("last") => load_session_for_command(storage, None, true, false),
        Some(session_id) => {
            load_session_for_command(storage, Some(session_id.to_string()), false, false)
        }
        None => {
            if let Some(current_session_id) = current_session_id {
                load_session_for_command(
                    storage,
                    Some(current_session_id.to_string()),
                    false,
                    false,
                )
            } else {
                load_session_for_command(storage, None, false, true)
            }
        }
    }
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
    permission_preset: Option<PermissionPreset>,
    attachments: &[InputAttachment],
    cwd: &Path,
) -> Result<()> {
    let config = storage.load_config()?;
    let current_session = session_id.and_then(|id| storage.get_session(id).ok().flatten());
    let active_alias = resolve_active_alias(&config, alias)?;
    let provider = config
        .get_provider(&active_alias.provider_id)
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
    println!("thinking={}", thinking_level_label(thinking_level));
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

fn load_last_assistant_output(
    storage: &Storage,
    session_id: Option<&str>,
) -> Result<Option<String>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let messages = storage.list_session_messages(session_id)?;
    Ok(latest_nonempty_assistant_message(messages.iter()))
}

fn latest_assistant_output_from_transcript(transcript: &SessionTranscript) -> Option<String> {
    latest_nonempty_assistant_message(transcript.messages.iter())
}

fn latest_nonempty_assistant_message<'a>(
    messages: impl DoubleEndedIterator<Item = &'a agent_core::SessionMessage>,
) -> Option<String> {
    let mut fallback = None;
    for message in messages.rev() {
        if message.role != MessageRole::Assistant {
            continue;
        }
        if !message.content.trim().is_empty() {
            return Some(message.content.clone());
        }
        if fallback.is_none() {
            fallback = Some(message.content.clone());
        }
    }
    fallback
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("failed to access system clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("failed to write to system clipboard")
}

fn init_agents_file(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(path, build_agents_template(path.parent()))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

fn build_agents_template(parent: Option<&Path>) -> String {
    let location = parent
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    format!(
        "# AGENTS.md\n\n## Project Guidance\n- Describe what lives under {location}.\n- List the most important build, test, and run commands.\n- Call out code style, review expectations, and risky areas.\n\n## Guardrails\n- Document paths or systems the agent should avoid editing.\n- Note approval expectations for destructive changes.\n\n## Verification\n- List the commands the agent should run before considering work complete.\n"
    )
}

fn build_compact_prompt(transcript: &SessionTranscript) -> Result<String> {
    let mut history = String::new();
    for message in &transcript.messages {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        history.push_str(&format!(
            "[{role}]\n{}\n\n",
            format_session_message_for_display(message)
        ));
    }

    if history.trim().is_empty() {
        bail!("current session has no transcript to compact");
    }

    let history = truncate_for_prompt(history, 100_000);
    Ok(format!(
        "Summarize this coding session so a fresh agent can continue it with minimal context loss. Preserve the user's goals, decisions, current status, important files, unresolved questions, and any concrete next steps. Be concise but complete.\n\nTranscript:\n```text\n{history}\n```"
    ))
}

fn format_session_message_for_display(message: &agent_core::SessionMessage) -> String {
    let mut lines = Vec::new();
    if message.role == MessageRole::Tool {
        let label = match (&message.tool_name, &message.tool_call_id) {
            (Some(name), Some(call_id)) => format!("[tool:{name} id={call_id}]"),
            (Some(name), None) => format!("[tool:{name}]"),
            (None, Some(call_id)) => format!("[tool id={call_id}]"),
            (None, None) => "[tool]".to_string(),
        };
        lines.push(label);
    }
    if !message.content.trim().is_empty() {
        lines.push(message.content.trim().to_string());
    }
    for tool_call in &message.tool_calls {
        lines.push(format!(
            "[tool_call:{} id={}]",
            tool_call.name, tool_call.id
        ));
    }
    if !message.attachments.is_empty() {
        let attachments = message
            .attachments
            .iter()
            .map(|attachment| attachment.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("[images: {attachments}]"));
    }
    if lines.is_empty() {
        "(empty)".to_string()
    } else {
        lines.join("\n")
    }
}

fn compact_session(
    storage: &Storage,
    transcript: &SessionTranscript,
    summary: &str,
) -> Result<String> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        transcript.session.cwd.as_deref(),
    )?;
    storage.append_message(&agent_core::SessionMessage::new(
        new_session_id.clone(),
        MessageRole::User,
        format!(
            "This session is a compacted continuation of session {}.\nUse the following summary as prior context:\n\n{}",
            transcript.session.id,
            summary.trim()
        ),
        Some(transcript.session.provider_id.clone()),
        Some(transcript.session.model.clone()),
    ))?;
    Ok(new_session_id)
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

fn build_review_prompt(args: &ReviewArgs) -> Result<String> {
    let review_target = collect_review_target(args)?;
    let custom_prompt = normalize_prompt_input(args.prompt.clone())?;
    let instructions = custom_prompt.unwrap_or_else(|| {
        "Review these code changes. Focus on bugs, regressions, security issues, and missing tests. Put findings first, ordered by severity, and be concise.".to_string()
    });
    Ok(format!(
        "{instructions}\n\nReview target:\n```\n{review_target}\n```"
    ))
}

fn collect_review_target(args: &ReviewArgs) -> Result<String> {
    if let Some(base) = &args.base {
        return capture_git_output(
            &[
                "diff",
                "--no-ext-diff",
                "--stat",
                "--patch",
                &format!("{base}...HEAD"),
            ],
            120_000,
        );
    }
    if let Some(commit) = &args.commit {
        let mut output = capture_git_output(
            &["show", "--stat", "--patch", "--format=medium", commit],
            120_000,
        )?;
        if let Some(title) = &args.commit_title {
            output = format!("Commit title: {title}\n\n{output}");
        }
        return Ok(output);
    }

    let staged = capture_git_output(&["diff", "--no-ext-diff", "--cached"], 60_000)
        .unwrap_or_else(|_| String::new());
    let unstaged =
        capture_git_output(&["diff", "--no-ext-diff"], 60_000).unwrap_or_else(|_| String::new());
    let untracked = capture_git_output(&["ls-files", "--others", "--exclude-standard"], 10_000)
        .unwrap_or_else(|_| String::new());

    let combined = format!(
        "Staged changes:\n{staged}\n\nUnstaged changes:\n{unstaged}\n\nUntracked files:\n{untracked}"
    );
    if combined.trim().is_empty() {
        bail!("no reviewable git changes found");
    }
    Ok(truncate_for_prompt(combined, 120_000))
}

fn capture_git_output(args: &[&str], max_len: usize) -> Result<String> {
    async fn run_git_capture(args: Vec<String>) -> Result<std::process::Output> {
        let mut command = TokioCommand::new("git");
        command.kill_on_drop(true);
        command.args(&args);
        timeout(DEFAULT_GIT_CAPTURE_TIMEOUT, command.output())
            .await
            .with_context(|| format!("git {} timed out", args.join(" ")))?
            .with_context(|| format!("failed to run git {}", args.join(" ")))
    }

    let args_vec = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(run_git_capture(args_vec))),
        Err(_) => tokio::runtime::Runtime::new()
            .context("failed to create runtime for git capture")?
            .block_on(run_git_capture(args_vec)),
    }?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(truncate_for_prompt(
        String::from_utf8_lossy(&output.stdout).to_string(),
        max_len,
    ))
}

fn truncate_for_prompt(mut text: String, max_len: usize) -> String {
    if text.len() <= max_len {
        return text;
    }
    text.truncate(max_len);
    text.push_str("\n\n[truncated]");
    text
}

fn load_session_for_command(
    storage: &Storage,
    requested_id: Option<String>,
    last: bool,
    show_all: bool,
) -> Result<SessionTranscript> {
    const SESSION_PICKER_LIMIT: usize = 5_000;
    let session = if let Some(session_id) = requested_id {
        storage
            .get_session(&session_id)?
            .ok_or_else(|| anyhow!("unknown session"))?
    } else if last {
        storage
            .list_sessions(1)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no recorded sessions found"))?
    } else {
        let sessions =
            rank_sessions_for_picker(storage.list_sessions(SESSION_PICKER_LIMIT)?, show_all)?;
        if sessions.is_empty() {
            bail!("no recorded sessions found");
        }
        select_session_interactively(&sessions, show_all)?
    };

    Ok(SessionTranscript {
        messages: storage.list_session_messages(&session.id)?,
        session,
    })
}

fn select_session_interactively(
    sessions: &[agent_core::SessionSummary],
    show_all: bool,
) -> Result<agent_core::SessionSummary> {
    let theme = ColorfulTheme::default();
    let items = sessions
        .iter()
        .map(|session| {
            let title = session.title.as_deref().unwrap_or("(untitled)");
            let cwd = session
                .cwd
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            if show_all {
                format!(
                    "{} | {} | {} | {} | {} | {}",
                    session.id, title, session.alias, session.provider_id, cwd, session.updated_at
                )
            } else {
                format!(
                    "{} | {} | {} | {} | {}",
                    session.id, title, session.alias, cwd, session.updated_at
                )
            }
        })
        .collect::<Vec<_>>();
    let choice = if items.len() > 8 {
        FuzzySelect::with_theme(&theme)
            .with_prompt("Select a session")
            .items(&items)
            .default(0)
            .interact()?
    } else {
        Select::with_theme(&theme)
            .with_prompt("Select a session")
            .items(&items)
            .default(0)
            .interact()?
    };
    Ok(sessions[choice].clone())
}

fn rank_sessions_for_picker(
    mut sessions: Vec<agent_core::SessionSummary>,
    show_all: bool,
) -> Result<Vec<agent_core::SessionSummary>> {
    if show_all {
        return Ok(sessions);
    }
    let cwd = current_request_cwd().ok();
    if let Some(cwd) = cwd {
        let matching = sessions
            .iter()
            .filter(|session| {
                session
                    .cwd
                    .as_deref()
                    .is_some_and(|value| value.starts_with(&cwd) || cwd.starts_with(value))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !matching.is_empty() {
            return Ok(matching);
        }
    }
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(sessions)
}

fn fork_session(storage: &Storage, transcript: &SessionTranscript) -> Result<String> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        transcript.session.cwd.as_deref(),
    )?;
    for message in &transcript.messages {
        storage.append_message(&message.fork_to_session(new_session_id.clone()))?;
    }
    Ok(new_session_id)
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

async fn interactive_provider_setup(
    theme: &ColorfulTheme,
    config: &AppConfig,
) -> Result<(ProviderUpsertRequest, ModelAlias)> {
    let choice = Select::with_theme(theme)
        .with_prompt("Choose a provider type")
        .items([
            "OpenAI hosted",
            "Anthropic hosted",
            "Moonshot hosted",
            "OpenRouter hosted",
            "Venice AI hosted",
            "Ollama local",
            "Local OpenAI-compatible endpoint (Kobold-style)",
        ])
        .default(0)
        .interact()?;

    let (default_id, default_name) = match choice {
        0 => ("openai", "OpenAI"),
        1 => ("anthropic", "Anthropic"),
        2 => ("moonshot", "Moonshot"),
        3 => ("openrouter", "OpenRouter"),
        4 => ("venice", "Venice AI"),
        5 => ("ollama-local", "Ollama"),
        6 => ("local-openai", "Local OpenAI-compatible"),
        _ => unreachable!("invalid provider selection"),
    };

    let id = next_available_provider_id(config, default_id);
    let name = default_name.to_string();

    let (request, model) = match choice {
        0 => {
            interactive_hosted_provider_request(
                theme,
                id.clone(),
                name,
                HostedKindArg::OpenaiCompatible,
            )
            .await?
        }
        1 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Anthropic)
                .await?
        }
        2 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Moonshot)
                .await?
        }
        3 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Openrouter)
                .await?
        }
        4 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Venice)
                .await?
        }
        5 => {
            let base_url = ask_url(theme, DEFAULT_OLLAMA_URL)?;
            let mut provider = ProviderConfig {
                id: id.clone(),
                display_name: name,
                kind: ProviderKind::Ollama,
                base_url,
                auth_mode: AuthMode::None,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: true,
            };
            let model = determine_local_model(&provider, None, Some(theme)).await?;
            provider.default_model = Some(model.clone());
            (
                ProviderUpsertRequest {
                    provider,
                    api_key: None,
                    oauth_token: None,
                },
                model,
            )
        }
        6 => {
            let requires_key = Confirm::with_theme(theme)
                .with_prompt("Does the local endpoint require an API key?")
                .default(false)
                .interact()?;
            let base_url = ask_url(theme, DEFAULT_LOCAL_OPENAI_URL)?;
            if requires_key {
                let api_key = Password::with_theme(theme)
                    .with_prompt("API key")
                    .allow_empty_password(false)
                    .interact()?;
                let mut request = ProviderUpsertRequest {
                    provider: ProviderConfig {
                        id: id.clone(),
                        display_name: name,
                        kind: ProviderKind::OpenAiCompatible,
                        base_url,
                        auth_mode: AuthMode::ApiKey,
                        default_model: None,
                        keychain_account: None,
                        oauth: None,
                        local: true,
                    },
                    api_key: Some(api_key),
                    oauth_token: None,
                };
                let model = resolve_hosted_model_after_auth(theme, &request, None).await?;
                request.provider.default_model = Some(model.clone());
                (request, model)
            } else {
                let mut provider = ProviderConfig {
                    id: id.clone(),
                    display_name: name,
                    kind: ProviderKind::OpenAiCompatible,
                    base_url,
                    auth_mode: AuthMode::None,
                    default_model: None,
                    keychain_account: None,
                    oauth: None,
                    local: true,
                };
                let model = determine_local_model(&provider, None, Some(theme)).await?;
                provider.default_model = Some(model.clone());
                (
                    ProviderUpsertRequest {
                        provider,
                        api_key: None,
                        oauth_token: None,
                    },
                    model,
                )
            }
        }
        _ => unreachable!("invalid provider selection"),
    };

    let alias_name: String = Input::with_theme(theme)
        .with_prompt("Alias for this model")
        .with_initial_text(default_alias_name(config, &request.provider, &model))
        .interact_text()?;

    let alias = ModelAlias {
        alias: alias_name,
        provider_id: id,
        model,
        description: None,
    };
    Ok((request, alias))
}

fn ask_url(theme: &ColorfulTheme, default_url: &str) -> Result<String> {
    Ok(Input::with_theme(theme)
        .with_prompt("Base URL")
        .with_initial_text(default_url)
        .interact_text()?)
}

fn prompt_for_model(theme: &ColorfulTheme) -> Result<String> {
    Ok(Input::with_theme(theme)
        .with_prompt("Default model")
        .interact_text()?)
}

async fn interactive_hosted_provider_request(
    theme: &ColorfulTheme,
    id: String,
    name: String,
    kind: HostedKindArg,
) -> Result<(ProviderUpsertRequest, String)> {
    let auth_method = select_auth_method(theme, kind)?;
    let base_url = match auth_method {
        AuthMethodArg::Browser => default_browser_hosted_url(kind).to_string(),
        AuthMethodArg::ApiKey | AuthMethodArg::Oauth => default_hosted_url(kind).to_string(),
    };
    let mut request = match auth_method {
        AuthMethodArg::Browser => match complete_browser_login(kind, &name).await? {
            BrowserLoginResult::ApiKey(api_key) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id,
                    display_name: name,
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
                    id,
                    display_name: name,
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
                id,
                display_name: name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                auth_mode: AuthMode::ApiKey,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: false,
            },
            api_key: Some(
                Password::with_theme(theme)
                    .with_prompt("API key")
                    .allow_empty_password(false)
                    .interact()?,
            ),
            oauth_token: None,
        },
        AuthMethodArg::Oauth => {
            let provider = build_oauth_provider(
                theme,
                id,
                name,
                hosted_kind_to_provider_kind(kind),
                &base_url,
            )?;
            let token = complete_oauth_login(&provider).await?;
            ProviderUpsertRequest {
                provider,
                api_key: None,
                oauth_token: Some(token),
            }
        }
    };

    let model = resolve_hosted_model_after_auth(theme, &request, None).await?;
    request.provider.default_model = Some(model.clone());

    Ok((request, model))
}

fn hosted_kind_defaults(kind: HostedKindArg) -> (&'static str, &'static str) {
    match kind {
        HostedKindArg::OpenaiCompatible => ("openai", "OpenAI"),
        HostedKindArg::Anthropic => ("anthropic", "Anthropic"),
        HostedKindArg::Moonshot => ("moonshot", "Moonshot"),
        HostedKindArg::Openrouter => ("openrouter", "OpenRouter"),
        HostedKindArg::Venice => ("venice", "Venice AI"),
    }
}

fn next_available_provider_id(config: &AppConfig, preferred: &str) -> String {
    if config.get_provider(preferred).is_none() {
        return preferred.to_string();
    }

    let mut index = 2;
    loop {
        let candidate = format!("{preferred}-{index}");
        if config.get_provider(&candidate).is_none() {
            return candidate;
        }
        index += 1;
    }
}

fn default_alias_name(config: &AppConfig, provider: &ProviderConfig, model: &str) -> String {
    let preferred = if config.main_agent_alias.is_none() && config.aliases.is_empty() {
        "main".to_string()
    } else {
        let model_slug = model
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' => ch.to_ascii_lowercase(),
                _ => '-',
            })
            .collect::<String>()
            .split('-')
            .filter(|segment| !segment.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("-");
        if model_slug.is_empty() {
            provider.id.clone()
        } else {
            format!("{}-{}", provider.id, model_slug)
        }
    };

    if config.get_alias(&preferred).is_none() {
        return preferred;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{preferred}-{index}");
        if config.get_alias(&candidate).is_none() {
            return candidate;
        }
        index += 1;
    }
}

async fn resolve_hosted_model_after_auth(
    theme: &ColorfulTheme,
    request: &ProviderUpsertRequest,
    provided: Option<String>,
) -> Result<String> {
    let discovered = provider_list_models_with_overrides(
        &build_http_client(),
        &request.provider,
        request.api_key.as_deref(),
        request.oauth_token.as_ref(),
    )
    .await;

    if let Some(model) = provided {
        if let Ok(models) = &discovered {
            if !models.is_empty() && !models.iter().any(|candidate| candidate == &model) {
                bail!(
                    "model '{}' is not available for provider '{}'",
                    model,
                    request.provider.id
                );
            }
        }
        return Ok(model);
    }

    match discovered {
        Ok(models) if !models.is_empty() => {
            if models.len() == 1 {
                println!("Detected model '{}'.", models[0]);
                return Ok(models[0].clone());
            }
            let selection = FuzzySelect::with_theme(theme)
                .with_prompt("Choose a model")
                .items(&models)
                .default(0)
                .interact()?;
            Ok(models[selection].clone())
        }
        Ok(_) => {
            println!("No models were returned automatically for this provider.");
            prompt_for_model(theme)
        }
        Err(error) => {
            if should_abort_after_auth_discovery_error(request, &error) {
                return Err(error);
            }
            println!("Could not load models automatically after authentication: {error}");
            prompt_for_model(theme)
        }
    }
}

fn should_abort_after_auth_discovery_error(
    request: &ProviderUpsertRequest,
    error: &anyhow::Error,
) -> bool {
    request.provider.auth_mode == AuthMode::OAuth
        && request
            .provider
            .oauth
            .as_ref()
            .is_some_and(|oauth| oauth.authorization_url.contains(OPENAI_BROWSER_AUTH_ISSUER))
        && error
            .to_string()
            .contains("missing the organization access required to mint a platform API key")
}

async fn determine_local_model(
    provider: &ProviderConfig,
    provided: Option<String>,
    theme: Option<&ColorfulTheme>,
) -> Result<String> {
    if let Some(model) = provided {
        return Ok(model);
    }

    match provider_list_models(&build_http_client(), provider).await {
        Ok(models) if !models.is_empty() => {
            if let Some(theme) = theme {
                if models.len() == 1 {
                    println!("Detected local model '{}'.", models[0]);
                    return Ok(models[0].clone());
                }
                let index = Select::with_theme(theme)
                    .with_prompt("Choose a model")
                    .items(&models)
                    .default(0)
                    .interact()?;
                return Ok(models[index].clone());
            }

            println!("Detected local model '{}'.", models[0]);
            Ok(models[0].clone())
        }
        Ok(_) => {
            if let Some(theme) = theme {
                prompt_for_model(theme)
            } else {
                bail!("local provider returned no models; pass --model explicitly")
            }
        }
        Err(error) => {
            if let Some(theme) = theme {
                println!("Could not detect models automatically: {error}");
                prompt_for_model(theme)
            } else {
                Err(error).context("could not detect a local model; pass --model explicitly")
            }
        }
    }
}

fn build_oauth_provider(
    theme: &ColorfulTheme,
    id: String,
    name: String,
    kind: ProviderKind,
    default_url: &str,
) -> Result<ProviderConfig> {
    let client_id = Input::with_theme(theme)
        .with_prompt("OAuth client id")
        .interact_text()?;
    let authorization_url = Input::with_theme(theme)
        .with_prompt("OAuth authorization URL")
        .interact_text()?;
    let token_url = Input::with_theme(theme)
        .with_prompt("OAuth token URL")
        .interact_text()?;
    let scopes_input: String = Input::with_theme(theme)
        .with_prompt("Scopes (space or comma separated, optional)")
        .allow_empty(true)
        .interact_text()?;
    let auth_params_input: String = Input::with_theme(theme)
        .with_prompt("Extra authorization params k=v,k=v (optional)")
        .allow_empty(true)
        .interact_text()?;
    let token_params_input: String = Input::with_theme(theme)
        .with_prompt("Extra token params k=v,k=v (optional)")
        .allow_empty(true)
        .interact_text()?;

    Ok(ProviderConfig {
        id,
        display_name: name,
        kind,
        base_url: ask_url(theme, default_url)?,
        auth_mode: AuthMode::OAuth,
        default_model: None,
        keychain_account: None,
        oauth: Some(OAuthConfig {
            client_id,
            authorization_url,
            token_url,
            scopes: split_scopes(&scopes_input),
            extra_authorize_params: parse_key_value_list(&auth_params_input)?,
            extra_token_params: parse_key_value_list(&token_params_input)?,
        }),
        local: false,
    })
}

async fn complete_browser_login(
    kind: HostedKindArg,
    provider_name: &str,
) -> Result<BrowserLoginResult> {
    match kind {
        HostedKindArg::OpenaiCompatible => Ok(BrowserLoginResult::OAuthToken(
            complete_openai_browser_login().await?,
        )),
        HostedKindArg::Openrouter => Ok(BrowserLoginResult::ApiKey(
            complete_openrouter_browser_login().await?,
        )),
        HostedKindArg::Anthropic => Ok(BrowserLoginResult::ApiKey(
            complete_claude_browser_login().await?,
        )),
        HostedKindArg::Moonshot | HostedKindArg::Venice => Ok(BrowserLoginResult::ApiKey(
            capture_browser_api_key(kind, provider_name).await?,
        )),
    }
}

fn openai_browser_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: OPENAI_BROWSER_CLIENT_ID.to_string(),
        authorization_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/authorize"),
        token_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/token"),
        scopes: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "offline_access".to_string(),
            "api.connectors.read".to_string(),
            "api.connectors.invoke".to_string(),
        ],
        extra_authorize_params: vec![
            KeyValuePair {
                key: "id_token_add_organizations".to_string(),
                value: "true".to_string(),
            },
            KeyValuePair {
                key: "codex_cli_simplified_flow".to_string(),
                value: "true".to_string(),
            },
            KeyValuePair {
                key: "originator".to_string(),
                value: OPENAI_BROWSER_ORIGINATOR.to_string(),
            },
        ],
        extra_token_params: Vec::new(),
    }
}

async fn complete_openai_browser_login() -> Result<OAuthToken> {
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
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let listener = bind_openai_browser_listener(OPENAI_BROWSER_CALLBACK_PORT).await?;
    let redirect_uri = format!(
        "http://localhost:{}{OPENAI_BROWSER_CALLBACK_PATH}",
        listener
            .local_addr()
            .context("failed to inspect OpenAI browser callback listener")?
            .port()
    );
    let authorization_url =
        build_oauth_authorization_url(&provider, &redirect_uri, &state, &challenge)?;

    match webbrowser::open(&authorization_url) {
        Ok(_) => println!("Opened browser for OpenAI sign-in."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{authorization_url}\n");

    timeout(
        OAUTH_TIMEOUT,
        run_openai_browser_callback_loop(
            &client,
            &provider,
            listener,
            &state,
            &verifier,
            &redirect_uri,
        ),
    )
    .await
    .context("timed out waiting for OpenAI browser callback")?
}

async fn bind_openai_browser_listener(port: u16) -> Result<TcpListener> {
    let bind_address = format!("127.0.0.1:{port}");
    let mut cancel_attempted = false;

    for _ in 0..10 {
        match TcpListener::bind(&bind_address).await {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if !cancel_attempted {
                    cancel_attempted = true;
                    if let Err(cancel_error) = send_openai_browser_cancel_request(port) {
                        eprintln!(
                            "Failed to cancel previous OpenAI browser login server: {cancel_error}"
                        );
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }
            Err(error) => {
                return Err(error).context("failed to bind OpenAI browser callback server");
            }
        }
    }

    bail!("OpenAI browser callback port {bind_address} is already in use")
}

fn send_openai_browser_cancel_request(port: u16) -> Result<()> {
    let address = format!("127.0.0.1:{port}");
    let mut stream = std::net::TcpStream::connect(&address).with_context(|| {
        format!("failed to connect to existing OpenAI browser callback server at {address}")
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("failed to set OpenAI browser callback read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("failed to set OpenAI browser callback write timeout")?;
    stream
        .write_all(
            format!(
                "GET {OPENAI_BROWSER_CANCEL_PATH} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .context("failed to send OpenAI browser cancel request")?;
    let mut buffer = [0_u8; 64];
    let _ = stream.read(&mut buffer);
    Ok(())
}

async fn run_openai_browser_callback_loop(
    client: &Client,
    provider: &ProviderConfig,
    listener: TcpListener,
    expected_state: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    let success_url = format!(
        "http://localhost:{}{OPENAI_BROWSER_SUCCESS_PATH}",
        listener
            .local_addr()
            .context("failed to inspect OpenAI browser callback listener")?
            .port()
    );
    let mut pending_token = None;

    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .context("failed to accept OpenAI browser callback connection")?;
        let request = read_local_http_request(&mut stream).await?;
        let url = parse_callback_request_url(&request, "OpenAI browser callback")?;

        match url.path() {
            OPENAI_BROWSER_CALLBACK_PATH => {
                let code = match parse_openai_browser_callback(&url, expected_state) {
                    Ok(code) => code,
                    Err(error) => {
                        write_html_response(
                            &mut stream,
                            "400 Bad Request",
                            &render_openai_browser_error_page(&error.to_string()),
                        )
                        .await?;
                        return Err(error);
                    }
                };

                let token = match exchange_oauth_code(
                    client,
                    provider,
                    &code,
                    verifier,
                    redirect_uri,
                )
                .await
                {
                    Ok(token) => token,
                    Err(error) => {
                        write_html_response(
                            &mut stream,
                            "500 Internal Server Error",
                            &render_openai_browser_error_page(&error.to_string()),
                        )
                        .await?;
                        return Err(
                            error.context("OpenAI browser sign-in failed during token exchange")
                        );
                    }
                };

                write_redirect_response(&mut stream, &success_url).await?;
                pending_token = Some(token);
            }
            OPENAI_BROWSER_SUCCESS_PATH => {
                write_html_response(&mut stream, "200 OK", &render_openai_browser_success_page())
                    .await?;

                if let Some(token) = pending_token.take() {
                    return Ok(token);
                }
            }
            OPENAI_BROWSER_CANCEL_PATH => {
                write_html_response(
                    &mut stream,
                    "200 OK",
                    "<html><body><h1>Login cancelled</h1><p>You can return to the terminal.</p></body></html>",
                )
                .await?;
                bail!("OpenAI browser sign-in was cancelled");
            }
            _ => {
                write_html_response(
                    &mut stream,
                    "404 Not Found",
                    "<html><body><h1>Not found</h1></body></html>",
                )
                .await?;
            }
        }
    }
}

fn parse_openai_browser_callback(url: &Url, expected_state: &str) -> Result<String> {
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    if state.as_deref() != Some(expected_state) {
        bail!("OpenAI browser callback state did not match expected login state");
    }

    if let Some(error_code) = error {
        bail!(
            "{}",
            oauth_callback_error_message(&error_code, error_description.as_deref())
        );
    }

    code.ok_or_else(|| anyhow!("OpenAI browser callback missing authorization code"))
}

fn render_openai_browser_success_page() -> String {
    "<html><body><h1>Signed in to OpenAI</h1><p>You can return to the terminal.</p></body></html>"
        .to_string()
}

fn render_openai_browser_error_page(message: &str) -> String {
    format!(
        "<html><body><h1>OpenAI sign-in failed</h1><p>{}</p></body></html>",
        escape_html(message)
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeSettingsFile {
    #[serde(default)]
    primary_api_key: Option<String>,
    #[serde(default)]
    oauth_account: Option<ClaudeOauthAccount>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauthAccount {
    #[serde(default)]
    email_address: Option<String>,
    #[serde(default)]
    organization_uuid: Option<String>,
    #[serde(default)]
    organization_name: Option<String>,
}

struct ClaudeBrowserCredentials {
    api_key: String,
    email: Option<String>,
    org_id: Option<String>,
    org_name: Option<String>,
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeBrowserTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<serde_json::Value>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeBrowserApiKeyResponse {
    #[serde(default)]
    raw_key: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeBrowserRolesResponse {
    #[serde(default)]
    organization_name: Option<String>,
}

async fn complete_claude_browser_login() -> Result<String> {
    if let Some(credentials) = try_load_claude_browser_credentials().await? {
        print_claude_browser_credentials(&credentials, true);
        return Ok(credentials.api_key);
    }

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
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let listener =
        bind_preferred_callback_listener(CLAUDE_BROWSER_CALLBACK_PORT, "Claude browser callback")
            .await?;
    let redirect_uri = format!(
        "http://localhost:{}{CLAUDE_BROWSER_CALLBACK_PATH}",
        listener
            .local_addr()
            .context("failed to inspect Claude browser callback listener")?
            .port()
    );
    let authorization_url =
        build_oauth_authorization_url(&provider, &redirect_uri, &state, &challenge)?;

    let callback_task = tokio::spawn(wait_for_oauth_callback(listener));
    match webbrowser::open(&authorization_url) {
        Ok(_) => println!("Opened browser for Claude sign-in."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{authorization_url}\n");

    let callback = timeout(OAUTH_TIMEOUT, callback_task)
        .await
        .context("timed out waiting for Claude browser callback")?
        .context("Claude browser callback task failed")??;
    if callback.state != state {
        bail!("Claude browser callback state did not match expected login state");
    }

    let token = exchange_claude_browser_code(
        &client,
        &callback.code,
        &callback.state,
        &verifier,
        &redirect_uri,
    )
    .await?;
    let api_key = create_claude_browser_api_key(&client, &token.access_token).await?;
    let roles = fetch_claude_browser_roles(&client, &token.access_token)
        .await
        .ok();
    let credentials = ClaudeBrowserCredentials {
        api_key,
        email: token.display_email,
        org_id: token.org_id,
        org_name: roles
            .as_ref()
            .and_then(|roles| roles.organization_name.clone()),
        subscription_type: token.subscription_type,
    };
    print_claude_browser_credentials(&credentials, false);
    Ok(credentials.api_key)
}

async fn try_load_claude_browser_credentials() -> Result<Option<ClaudeBrowserCredentials>> {
    let settings_path = claude_settings_path()
        .ok_or_else(|| anyhow!("failed to resolve home directory for Claude settings"))?;
    if !settings_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&settings_path)
        .with_context(|| format!("failed to read {}", settings_path.display()))?;
    parse_claude_browser_credentials_from_settings(&raw)
}

fn parse_claude_browser_credentials_from_settings(
    raw: &str,
) -> Result<Option<ClaudeBrowserCredentials>> {
    let settings: ClaudeSettingsFile =
        serde_json::from_str(raw).context("failed to parse Claude settings file")?;
    let Some(api_key) = settings
        .primary_api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let oauth_account = settings.oauth_account;
    Ok(Some(ClaudeBrowserCredentials {
        api_key,
        email: oauth_account
            .as_ref()
            .and_then(|account| account.email_address.clone()),
        org_id: oauth_account
            .as_ref()
            .and_then(|account| account.organization_uuid.clone()),
        org_name: oauth_account
            .as_ref()
            .and_then(|account| account.organization_name.clone()),
        subscription_type: None,
    }))
}

fn claude_browser_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: CLAUDE_BROWSER_CLIENT_ID.to_string(),
        authorization_url: CLAUDE_BROWSER_AUTHORIZE_URL.to_string(),
        token_url: CLAUDE_BROWSER_TOKEN_URL.to_string(),
        scopes: CLAUDE_BROWSER_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect(),
        extra_authorize_params: vec![KeyValuePair {
            key: "code".to_string(),
            value: "true".to_string(),
        }],
        extra_token_params: Vec::new(),
    }
}

async fn bind_preferred_callback_listener(preferred_port: u16, label: &str) -> Result<TcpListener> {
    match TcpListener::bind(("127.0.0.1", preferred_port)).await {
        Ok(listener) => Ok(listener),
        Err(error) => {
            println!(
                "{label} could not bind port {preferred_port} ({error}); falling back to an ephemeral local port."
            );
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .with_context(|| format!("failed to bind local {label} listener"))
        }
    }
}

async fn exchange_claude_browser_code(
    client: &Client,
    code: &str,
    state: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    let response = client
        .post(CLAUDE_BROWSER_TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": redirect_uri,
            "client_id": CLAUDE_BROWSER_CLIENT_ID,
            "code_verifier": code_verifier,
            "state": state,
        }))
        .send()
        .await
        .context("failed to exchange Claude browser authorization code")?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .context("failed to read Claude token response")?;
    if !status.is_success() {
        bail!(
            "Claude browser token exchange failed: {}",
            parse_service_error_text(&raw)
        );
    }

    let token: ClaudeBrowserTokenResponse =
        serde_json::from_str(&raw).context("failed to parse Claude token response")?;
    Ok(OAuthToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: parse_optional_expires_at(token.expires_in.as_ref())?,
        token_type: token.token_type,
        scopes: token
            .scope
            .map(|scope| split_scopes(&scope))
            .unwrap_or_else(|| {
                CLAUDE_BROWSER_SCOPES
                    .iter()
                    .map(|scope| (*scope).to_string())
                    .collect()
            }),
        id_token: token.id_token,
        account_id: None,
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    })
}

async fn create_claude_browser_api_key(client: &Client, access_token: &str) -> Result<String> {
    let response = client
        .post(CLAUDE_BROWSER_API_KEY_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .context("failed to mint Claude managed API key")?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .context("failed to read Claude managed key response")?;
    if !status.is_success() {
        bail!(
            "Claude browser API key mint failed: {}",
            parse_service_error_text(&raw)
        );
    }

    let body: ClaudeBrowserApiKeyResponse =
        serde_json::from_str(&raw).context("failed to parse Claude managed key response")?;
    body.raw_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Claude browser login returned no managed API key"))
}

async fn fetch_claude_browser_roles(
    client: &Client,
    access_token: &str,
) -> Result<ClaudeBrowserRolesResponse> {
    let response = client
        .get(CLAUDE_BROWSER_ROLES_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .context("failed to fetch Claude organization metadata")?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .context("failed to read Claude organization metadata response")?;
    if !status.is_success() {
        bail!(
            "Claude browser org metadata request failed: {}",
            parse_service_error_text(&raw)
        );
    }
    serde_json::from_str(&raw).context("failed to parse Claude organization metadata")
}

fn parse_optional_expires_at(
    value: Option<&serde_json::Value>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let seconds = match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| anyhow!("expires_in was not an integer"))?,
        serde_json::Value::String(text) => text
            .parse::<i64>()
            .with_context(|| format!("invalid expires_in value '{text}'"))?,
        _ => bail!("expires_in was not a string or integer"),
    };
    Ok(chrono::Utc::now().checked_add_signed(chrono::Duration::seconds(seconds)))
}

fn parse_service_error_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "unknown authentication error".to_string();
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        for candidate in [
            value.get("error_description"),
            value.get("detail"),
            value.get("message"),
            value
                .get("error")
                .and_then(|error| error.as_str().map(|_| error)),
        ] {
            if let Some(text) = candidate.and_then(serde_json::Value::as_str) {
                let text = text.trim();
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
        if let Some(error) = value.get("error") {
            if let Some(text) = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                return text.to_string();
            }
        }
    }

    trimmed.to_string()
}

fn claude_settings_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".claude.json"))
}

fn print_claude_browser_credentials(credentials: &ClaudeBrowserCredentials, reused: bool) {
    if reused {
        println!("Using existing Claude credentials from ~/.claude.json.");
    } else {
        println!("Created a Claude managed API key from the browser session.");
    }
    if let Some(email) = credentials.email.as_deref() {
        println!("Claude account: {email}");
    }
    if let Some(subscription_type) = credentials.subscription_type.as_deref() {
        println!("Claude plan: {subscription_type}");
    }
    if let Some(org_name) = credentials.org_name.as_deref() {
        println!("Claude org: {org_name}");
    } else if let Some(org_id) = credentials.org_id.as_deref() {
        println!("Claude org id: {org_id}");
    }
}

#[cfg(test)]
fn jwt_expiry(token: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value = serde_json::from_slice::<serde_json::Value>(&decoded).ok()?;
    let exp = value.get("exp")?.as_i64()?;
    chrono::DateTime::<chrono::Utc>::from_timestamp(exp, 0)
}

async fn complete_openrouter_browser_login() -> Result<String> {
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local OpenRouter callback server")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .context("failed to inspect OpenRouter callback listener")?
            .port()
    );

    let mut authorization_url =
        Url::parse("https://openrouter.ai/auth").context("failed to parse OpenRouter auth URL")?;
    {
        let mut query = authorization_url.query_pairs_mut();
        query.append_pair("callback_url", &redirect_uri);
        query.append_pair("code_challenge", &challenge);
        query.append_pair("code_challenge_method", "S256");
    }

    let callback_task = tokio::spawn(wait_for_code_callback(listener));
    match webbrowser::open(authorization_url.as_str()) {
        Ok(_) => println!("Opened browser for OpenRouter login."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!(
        "If needed, open this URL manually:\n{}\n",
        authorization_url.as_str()
    );

    let callback = timeout(OAUTH_TIMEOUT, callback_task)
        .await
        .context("timed out waiting for OpenRouter callback")?
        .context("OpenRouter callback task failed")??;

    let response = client
        .post("https://openrouter.ai/api/v1/auth/keys")
        .json(&serde_json::json!({
            "code": callback.code,
            "code_verifier": verifier,
            "code_challenge_method": "S256"
        }))
        .send()
        .await
        .context("failed to exchange OpenRouter browser code for an API key")?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse OpenRouter browser login response")?;
    if !status.is_success() {
        bail!(
            "OpenRouter browser login failed: {}",
            body.get("error")
                .and_then(serde_json::Value::as_str)
                .or_else(|| body.get("message").and_then(serde_json::Value::as_str))
                .unwrap_or("unknown error")
        );
    }

    body.get("key")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("OpenRouter browser login response did not contain an API key"))
}

async fn capture_browser_api_key(kind: HostedKindArg, provider_name: &str) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local browser capture server")?;
    let helper_url = format!(
        "http://127.0.0.1:{}/",
        listener
            .local_addr()
            .context("failed to inspect browser capture listener")?
            .port()
    );

    let capture_task = tokio::spawn(wait_for_browser_api_key_submission(
        listener,
        kind,
        provider_name.to_string(),
    ));
    match webbrowser::open(&helper_url) {
        Ok(_) => println!("Opened browser helper for {} login.", provider_name),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{helper_url}\n");

    timeout(OAUTH_TIMEOUT, capture_task)
        .await
        .context("timed out waiting for browser credential submission")?
        .context("browser credential capture task failed")?
}

async fn wait_for_browser_api_key_submission(
    listener: TcpListener,
    kind: HostedKindArg,
    provider_name: String,
) -> Result<String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .context("failed to accept browser credential connection")?;
        let request = read_local_http_request(&mut stream).await?;
        let Some(first_line) = request.lines().next() else {
            write_html_response(
                &mut stream,
                "400 Bad Request",
                "<html><body><h1>Bad request</h1></body></html>",
            )
            .await?;
            continue;
        };
        let mut parts = first_line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or("/");

        match (method, target) {
            ("GET", "/") => {
                let html = browser_capture_page(kind, &provider_name);
                write_html_response(&mut stream, "200 OK", &html).await?;
            }
            ("GET", "/favicon.ico") => {
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .context("failed to write favicon response")?;
            }
            ("POST", "/submit") => {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                let fields = url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect::<Vec<_>>();
                let credential = fields
                    .iter()
                    .find(|(key, _)| key == "credential")
                    .map(|(_, value)| value.trim().to_string())
                    .unwrap_or_default();
                if credential.is_empty() {
                    write_html_response(
                        &mut stream,
                        "400 Bad Request",
                        "<html><body><h1>Missing credential</h1><p>Return to the previous tab and paste the credential before submitting.</p></body></html>",
                    )
                    .await?;
                    continue;
                }

                write_html_response(
                    &mut stream,
                    "200 OK",
                    "<html><body><h1>Credential captured</h1><p>You can close this tab and return to the terminal.</p></body></html>",
                )
                .await?;
                return Ok(credential);
            }
            _ => {
                write_html_response(
                    &mut stream,
                    "404 Not Found",
                    "<html><body><h1>Not Found</h1></body></html>",
                )
                .await?;
            }
        }
    }
}

fn browser_capture_page(kind: HostedKindArg, provider_name: &str) -> String {
    let title = escape_html(provider_name);
    let portal_url = escape_html(hosted_kind_browser_portal_url(kind));
    let instructions = escape_html(hosted_kind_browser_instructions(kind));
    format!(
        "<html><body style=\"font-family: sans-serif; max-width: 760px; margin: 40px auto; line-height: 1.5;\">\
         <h1>{title} browser setup</h1>\
         <p>{instructions}</p>\
         <p><a href=\"{portal_url}\" target=\"_blank\" rel=\"noreferrer\">Open {title}</a></p>\
         <form method=\"POST\" action=\"/submit\">\
         <label for=\"credential\"><strong>Paste credential</strong></label><br/>\
         <input id=\"credential\" name=\"credential\" type=\"password\" style=\"width: 100%; padding: 10px; margin: 12px 0;\" autofocus />\
         <button type=\"submit\" style=\"padding: 10px 18px;\">Save credential</button>\
         </form>\
         <p>This sends the credential only to the local CLI callback on this machine.</p>\
         </body></html>"
    )
}

fn hosted_kind_browser_portal_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => "https://platform.openai.com/",
        HostedKindArg::Anthropic => "https://console.anthropic.com/",
        HostedKindArg::Moonshot => "https://platform.moonshot.ai/",
        HostedKindArg::Openrouter => "https://openrouter.ai/",
        HostedKindArg::Venice => "https://venice.ai/",
    }
}

fn hosted_kind_browser_instructions(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => {
            "Sign in to the OpenAI platform in another tab, generate or copy an API key, then paste it below."
        }
        HostedKindArg::Anthropic => {
            "Sign in to Anthropic Console in another tab, create or copy an API key, then paste it below."
        }
        HostedKindArg::Moonshot => {
            "Sign in to the Moonshot platform in another tab, create or copy an API key, then paste it below."
        }
        HostedKindArg::Openrouter => {
            "OpenRouter browser login is automatic and should not use the manual browser helper."
        }
        HostedKindArg::Venice => {
            "Sign in to Venice in another tab, create or copy an API key, then paste it below."
        }
    }
}

async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buffer = vec![0_u8; 16_384];
    let bytes_read = timeout(OAUTH_TIMEOUT, stream.read(&mut buffer))
        .await
        .context("timed out reading local browser callback")?
        .context("failed to read local browser callback")?;
    Ok(String::from_utf8_lossy(&buffer[..bytes_read]).to_string())
}

async fn write_html_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    body: &str,
) -> Result<()> {
    let body_len = body.len();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_len,
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser response")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

fn parse_callback_request_url(request: &str, label: &str) -> Result<Url> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("{label} contained no request line"))?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("{label} request line was invalid"))?;
    Url::parse(&format!("http://127.0.0.1{path}"))
        .with_context(|| format!("failed to parse {label} URL"))
}

async fn write_redirect_response(stream: &mut tokio::net::TcpStream, location: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser redirect")
}

fn is_missing_codex_entitlement_error(error_code: &str, error_description: Option<&str>) -> bool {
    error_code == "access_denied"
        && error_description.is_some_and(|description| {
            description
                .to_ascii_lowercase()
                .contains("missing_codex_entitlement")
        })
}

fn oauth_callback_error_message(error_code: &str, error_description: Option<&str>) -> String {
    if is_missing_codex_entitlement_error(error_code, error_description) {
        return "OpenAI browser sign-in is not enabled for this workspace account yet.".to_string();
    }

    if let Some(description) = error_description {
        if !description.trim().is_empty() {
            return format!("Sign-in failed: {description}");
        }
    }

    format!("Sign-in failed: {error_code}")
}

async fn complete_oauth_login(provider: &ProviderConfig) -> Result<OAuthToken> {
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local OAuth callback server")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .context("failed to inspect OAuth callback listener")?
            .port()
    );
    let authorization_url =
        build_oauth_authorization_url(provider, &redirect_uri, &state, &challenge)?;

    let callback_task = tokio::spawn(wait_for_oauth_callback(listener));
    match webbrowser::open(&authorization_url) {
        Ok(_) => println!("Opened browser for OAuth login."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{authorization_url}\n");

    let callback = timeout(OAUTH_TIMEOUT, callback_task)
        .await
        .context("timed out waiting for OAuth callback")?
        .context("OAuth callback task failed")??;
    if callback.state != state {
        bail!("OAuth callback state did not match expected login state");
    }

    exchange_oauth_code(&client, provider, &callback.code, &verifier, &redirect_uri).await
}

async fn wait_for_code_callback(listener: TcpListener) -> Result<BrowserCodeCallback> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept browser code callback connection")?;
    let request = read_local_http_request(&mut stream).await?;
    let url = parse_callback_request_url(&request, "browser callback")?;

    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }

    let body = if let Some(error) = error.clone() {
        format!(
            "<html><body><h1>Browser login failed</h1><p>{}</p></body></html>",
            html_escape(&error)
        )
    } else {
        "<html><body><h1>Login complete</h1><p>You can return to the terminal.</p></body></html>"
            .to_string()
    };
    let status = if error.is_some() {
        "400 Bad Request"
    } else {
        "200 OK"
    };
    write_html_response(&mut stream, status, &body).await?;

    if let Some(error) = error {
        bail!("browser login failed: {error}");
    }

    Ok(BrowserCodeCallback {
        code: code.ok_or_else(|| anyhow!("browser callback missing authorization code"))?,
    })
}

async fn wait_for_oauth_callback(listener: TcpListener) -> Result<OAuthCallback> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept OAuth callback connection")?;
    let mut buffer = vec![0_u8; 8192];
    let bytes_read = timeout(OAUTH_TIMEOUT, stream.read(&mut buffer))
        .await
        .context("timed out reading OAuth callback")?
        .context("failed to read OAuth callback request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let url = parse_callback_request_url(&request, "OAuth callback")?;

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    let (status_line, body) = if let Some(error) = error.clone() {
        let message = oauth_callback_error_message(&error, error_description.as_deref());
        (
            "HTTP/1.1 400 Bad Request",
            format!(
                "<html><body><h1>OAuth login failed</h1><p>{}</p></body></html>",
                html_escape(&message)
            ),
        )
    } else {
        (
            "HTTP/1.1 200 OK",
            "<html><body><h1>Login complete</h1><p>You can return to the terminal.</p></body></html>"
                .to_string(),
        )
    };
    let response = format!(
        "{status_line}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write OAuth callback response")?;

    if let Some(error) = error {
        bail!(
            "{}",
            oauth_callback_error_message(&error, error_description.as_deref())
        );
    }

    Ok(OAuthCallback {
        code: code.ok_or_else(|| anyhow!("OAuth callback missing authorization code"))?,
        state: state.ok_or_else(|| anyhow!("OAuth callback missing state"))?,
    })
}

fn generate_code_verifier() -> String {
    let mut verifier = String::new();
    while verifier.len() < 64 {
        verifier.push_str(&Uuid::new_v4().simple().to_string());
    }
    verifier.truncate(96);
    verifier
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn prompt_or_value(
    theme: &ColorfulTheme,
    prompt: &str,
    current: Option<String>,
    initial_text: Option<String>,
) -> Result<String> {
    if let Some(current) = current {
        return Ok(current);
    }

    let mut input = Input::with_theme(theme).with_prompt(prompt);
    if let Some(initial_text) = initial_text {
        input = input.with_initial_text(initial_text);
    }
    Ok(input.interact_text()?)
}

fn select_hosted_kind(theme: &ColorfulTheme) -> Result<HostedKindArg> {
    Ok(
        match Select::with_theme(theme)
            .with_prompt("Provider type")
            .items(["OpenAI", "Anthropic", "Moonshot", "OpenRouter", "Venice AI"])
            .default(0)
            .interact()?
        {
            0 => HostedKindArg::OpenaiCompatible,
            1 => HostedKindArg::Anthropic,
            2 => HostedKindArg::Moonshot,
            3 => HostedKindArg::Openrouter,
            _ => HostedKindArg::Venice,
        },
    )
}

fn select_auth_method(theme: &ColorfulTheme, kind: HostedKindArg) -> Result<AuthMethodArg> {
    let browser_label = if hosted_kind_supports_automatic_browser_capture(kind) {
        match kind {
            HostedKindArg::OpenaiCompatible => {
                "Browser sign-in (use your OpenAI account, Recommended)"
            }
            HostedKindArg::Anthropic => "Browser sign-in (use your Claude account, Recommended)",
            HostedKindArg::Openrouter => "Browser sign-in (automatic capture, Recommended)",
            HostedKindArg::Moonshot | HostedKindArg::Venice => {
                unreachable!("non-native browser login provider was routed incorrectly")
            }
        }
    } else {
        "Browser portal (open provider site, then paste credential)"
    };

    Ok(
        match Select::with_theme(theme)
            .with_prompt("Authentication method")
            .items([browser_label, "OAuth (advanced custom flow)", "API key"])
            .default(0)
            .interact()?
        {
            0 => AuthMethodArg::Browser,
            1 => AuthMethodArg::Oauth,
            _ => AuthMethodArg::ApiKey,
        },
    )
}

fn hosted_kind_to_provider_kind(kind: HostedKindArg) -> ProviderKind {
    match kind {
        HostedKindArg::OpenaiCompatible
        | HostedKindArg::Moonshot
        | HostedKindArg::Openrouter
        | HostedKindArg::Venice => ProviderKind::OpenAiCompatible,
        HostedKindArg::Anthropic => ProviderKind::Anthropic,
    }
}

fn browser_hosted_kind_to_provider_kind(kind: HostedKindArg) -> ProviderKind {
    match kind {
        HostedKindArg::OpenaiCompatible => ProviderKind::ChatGptCodex,
        _ => hosted_kind_to_provider_kind(kind),
    }
}

fn default_hosted_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => DEFAULT_OPENAI_URL,
        HostedKindArg::Anthropic => DEFAULT_ANTHROPIC_URL,
        HostedKindArg::Moonshot => DEFAULT_MOONSHOT_URL,
        HostedKindArg::Openrouter => DEFAULT_OPENROUTER_URL,
        HostedKindArg::Venice => DEFAULT_VENICE_URL,
    }
}

fn default_browser_hosted_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => DEFAULT_CHATGPT_CODEX_URL,
        _ => default_hosted_url(kind),
    }
}

fn hosted_kind_supports_automatic_browser_capture(kind: HostedKindArg) -> bool {
    matches!(
        kind,
        HostedKindArg::Anthropic | HostedKindArg::Openrouter | HostedKindArg::OpenaiCompatible
    )
}

fn collect_scopes(theme: &ColorfulTheme, scopes: Vec<String>) -> Result<Vec<String>> {
    if !scopes.is_empty() {
        return Ok(scopes);
    }
    let input: String = Input::with_theme(theme)
        .with_prompt("Scopes (space or comma separated, optional)")
        .allow_empty(true)
        .interact_text()?;
    Ok(split_scopes(&input))
}

fn split_scopes(input: &str) -> Vec<String> {
    input
        .replace(',', " ")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn collect_key_value_params(
    theme: &ColorfulTheme,
    prompt: &str,
    params: Vec<String>,
) -> Result<Vec<KeyValuePair>> {
    if !params.is_empty() {
        let pairs = params
            .into_iter()
            .map(parse_key_value_pair)
            .collect::<Result<Vec<_>>>()?;
        reject_plaintext_oauth_secrets(&pairs)?;
        return Ok(pairs);
    }
    let input: String = Input::with_theme(theme)
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()?;
    let pairs = parse_key_value_list(&input)?;
    reject_plaintext_oauth_secrets(&pairs)?;
    Ok(pairs)
}

fn parse_key_value_list(input: &str) -> Result<Vec<KeyValuePair>> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    input
        .split(',')
        .map(|entry| parse_key_value_pair(entry.trim().to_string()))
        .collect()
}

fn parse_key_value_pair(value: String) -> Result<KeyValuePair> {
    let (key, value) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("expected key=value"))?;
    Ok(KeyValuePair {
        key: key.trim().to_string(),
        value: value.trim().to_string(),
    })
}

fn reject_plaintext_oauth_secrets(params: &[KeyValuePair]) -> Result<()> {
    let Some(secret_key) = params.iter().find_map(|param| {
        let key = param.key.trim().to_ascii_lowercase();
        ["secret", "password", "private_key", "api_key"]
            .iter()
            .any(|fragment| key.contains(fragment))
            .then_some(param.key.as_str())
    }) else {
        return Ok(());
    };
    bail!(
        "OAuth parameter '{}' looks secret and would be stored in plaintext config; browser/API-key flows are supported, but secret OAuth params are not",
        secret_key
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::AppConfig;
    use clap::Parser;

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
        let cli = Cli::parse_from(["autism", "write a summary"]);
        assert_eq!(cli.prompt.as_deref(), Some("write a summary"));
        assert!(cli.command.is_none());
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
            "autism",
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
        let cli = Cli::parse_from(["autism", "resume", "--last"]);
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
            "autism",
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
            "autism",
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
            "autism",
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
    fn parse_interactive_command_supports_model_and_thinking() {
        assert_eq!(
            parse_interactive_command("/model claude").unwrap(),
            Some(InteractiveCommand::ModelSet("claude".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/thinking high").unwrap(),
            Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
        );
        assert_eq!(
            parse_interactive_command("/thinking default").unwrap(),
            Some(InteractiveCommand::ThinkingSet(None))
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
            "autism",
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
            "autism",
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
    fn cli_parses_skill_draft_listing() {
        let cli = Cli::parse_from([
            "autism",
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
        let cli = Cli::parse_from(["autism", "memory", "profile", "--limit", "7"]);
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
    fn cli_parses_telegram_add_command() {
        let cli = Cli::parse_from([
            "autism",
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
            "autism",
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
            "autism",
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
            "autism",
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
            ..AppConfig::default()
        };

        let accounts = configured_keychain_accounts(&config);

        assert_eq!(accounts.len(), 3);
        assert!(accounts.contains("shared-account"));
        assert!(accounts.contains("telegram-account"));
        assert!(accounts.contains("discord-account"));
    }

    #[test]
    fn cli_parses_reset_yes() {
        let cli = Cli::parse_from(["autism", "reset", "--yes"]);

        match cli.command {
            Some(Commands::Reset(args)) => assert!(args.yes),
            _ => panic!("expected reset command"),
        }
    }

    #[test]
    fn cli_parses_openrouter_provider_add() {
        let cli = Cli::parse_from([
            "autism",
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
            "autism",
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
            "autism",
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
                kind: ProviderKind::OpenAiCompatible,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                auth_mode: AuthMode::ApiKey,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: None,
                local: false,
            }],
            ..AppConfig::default()
        };

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
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                auth_mode: AuthMode::OAuth,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: Some("provider:openai".to_string()),
                oauth: Some(openai_browser_oauth_config()),
                local: false,
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
}
