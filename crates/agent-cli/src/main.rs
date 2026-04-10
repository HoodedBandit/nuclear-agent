use std::{
    collections::BTreeSet,
    fs,
    io::IsTerminal,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

mod cli_support;
mod config_cli;
mod connector_cli;
mod connector_management_cli;
mod connector_support;
mod integrations_cli;
mod interactive_commands;
mod interactive_ui;
mod onboarding_cli;
mod operations_cli;
mod plugins_cli;
mod provider_auth;
mod repo_cli;
mod session_support;
mod skills_cli;
mod tui;

#[cfg(test)]
use agent_core::{DiscordChannelCursor, MessageRole, SessionResumePacket};
pub(crate) use cli_support::*;
use config_cli::{
    alias_command, daemon_command, login_command, logout_command, model_command,
    permissions_command, provider_command, reset_command, trust_command,
};
pub(crate) use config_cli::{
    apply_provider_request_locally, collect_image_attachments, load_json_file,
    load_prompt_template, load_schema_file, maybe_write_last_message, print_json_run_response,
    run_onboarding_reset,
};
#[cfg(test)]
use config_cli::{apply_trust_update, default_main_alias};
use connector_cli::{
    discord_command, home_assistant_command, signal_command, slack_command, telegram_command,
};
use connector_management_cli::{
    inbox_command, load_discord_connectors, load_home_assistant_connectors, load_inbox_connectors,
    load_signal_connectors, load_slack_connectors, load_telegram_connectors,
    load_webhook_connectors, skills_command, webhook_command,
};
pub(crate) use connector_support::{
    format_discord_channel_cursors, format_home_assistant_entity_cursors, format_i64_list,
    format_slack_channel_cursors, format_string_list, hash_webhook_token_local,
};
use futures::StreamExt;
use integrations_cli::{app_command, mcp_command, AppCommands, McpCommands};
#[cfg(test)]
use integrations_cli::{AppAddArgs, McpAddArgs};
use onboarding_cli::{ensure_onboarded, setup};
#[cfg(test)]
use onboarding_cli::{has_usable_main_alias, needs_onboarding};
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
    ConnectorApprovalStatus, ConnectorKind, DaemonConfigUpdateRequest, DaemonStatus,
    DashboardLaunchResponse, DiscordConnectorConfig, DiscordConnectorUpsertRequest,
    DiscordPollResponse, DiscordSendRequest, DiscordSendResponse, EvolveConfig, EvolveStartRequest,
    HealthReport, HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest,
    HomeAssistantEntityState, HomeAssistantPollResponse, HomeAssistantServiceCallRequest,
    HomeAssistantServiceCallResponse, InboxConnectorConfig, InboxConnectorUpsertRequest,
    InboxPollResponse, InputAttachment, KeyValuePair, MemoryKind, MemoryRebuildRequest,
    MemoryRebuildResponse, MemoryRecord, MemoryReviewStatus, MemoryReviewUpdateRequest,
    MemoryScope, MemorySearchQuery, MemorySearchResponse, MemoryUpsertRequest, Mission,
    MissionCheckpoint, MissionControlRequest, MissionStatus, ModelAlias, OAuthConfig, OAuthToken,
    PermissionPreset, PermissionUpdateRequest, PersistenceMode, ProviderConfig, ProviderKind,
    ProviderUpsertRequest, RunTaskRequest, RunTaskResponse, SessionTranscript,
    SignalConnectorConfig, SignalConnectorUpsertRequest, SignalPollResponse, SignalSendRequest,
    SignalSendResponse, SkillDraftStatus, SlackConnectorConfig, SlackConnectorUpsertRequest,
    SlackPollResponse, SlackSendRequest, SlackSendResponse, SubAgentTask, TaskMode,
    TelegramConnectorConfig, TelegramConnectorUpsertRequest, TelegramPollResponse,
    TelegramSendRequest, TelegramSendResponse, ThinkingLevel, TrustPolicy, TrustUpdateRequest,
    WakeTrigger, WebhookConnectorConfig, WebhookConnectorUpsertRequest, WebhookEventRequest,
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
pub(crate) use skills_cli::{
    discover_skills, format_connector_approvals, format_memory_records, format_skill_drafts,
    load_connector_approvals, load_enabled_skills, load_memory_review_queue, load_profile_memories,
    load_skill_drafts, update_connector_approval_status, update_enabled_skill,
    update_memory_review_status, update_skill_draft_status,
};
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
    #[command(name = "support-bundle")]
    SupportBundle(SupportBundleArgs),
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
struct SupportBundleArgs {
    #[arg(long = "output-dir")]
    output_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 200)]
    log_limit: usize,
    #[arg(long, default_value_t = 25)]
    session_limit: usize,
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
        Some(Commands::SupportBundle(args)) => support_bundle_command(&storage, args).await?,
        Some(Commands::InternalDaemon) => unreachable!(),
    }

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
mod tests;
