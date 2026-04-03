#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum BrowserLoginResult {
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
pub(crate) struct Cli {
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    cwd: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
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
pub(crate) enum DaemonCommands {
    Start,
    Stop,
    Status,
    Config(DaemonConfigArgs),
}

#[derive(Args)]
pub(crate) struct DaemonConfigArgs {
    #[arg(long, value_enum)]
    mode: Option<PersistenceModeArg>,
    #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
    auto_start: Option<bool>,
}

#[derive(Subcommand)]
pub(crate) enum ProviderCommands {
    Add(ProviderAddArgs),
    AddLocal(LocalProviderAddArgs),
    List,
}

#[derive(Subcommand)]
pub(crate) enum TelegramCommands {
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
pub(crate) enum TelegramApprovalCommands {
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
pub(crate) enum WebhookCommands {
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
pub(crate) enum DiscordCommands {
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
pub(crate) enum DiscordApprovalCommands {
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
pub(crate) enum SlackCommands {
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
pub(crate) enum SlackApprovalCommands {
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
pub(crate) enum HomeAssistantCommands {
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
pub(crate) enum SignalCommands {
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
pub(crate) enum SignalApprovalCommands {
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
pub(crate) enum InboxCommands {
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
pub(crate) enum SkillCommands {
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
pub(crate) struct ProviderAddArgs {
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
pub(crate) struct LocalProviderAddArgs {
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
pub(crate) enum ModelCommands {
    List {
        #[arg(long)]
        provider: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum AliasCommands {
    Add(AliasAddArgs),
    List,
}

#[derive(Args)]
pub(crate) struct AliasAddArgs {
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
pub(crate) struct TrustArgs {
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
pub(crate) struct RunArgs {
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
pub(crate) struct ChatArgs {
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
pub(crate) struct PermissionsArgs {
    #[arg(value_enum)]
    preset: Option<PermissionPresetArg>,
}

#[derive(Args)]
pub(crate) struct WebhookAddArgs {
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
pub(crate) struct WebhookDeliverArgs {
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
pub(crate) struct TelegramAddArgs {
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
pub(crate) struct TelegramSendArgs {
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
pub(crate) struct DiscordAddArgs {
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
pub(crate) struct DiscordSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long = "channel-id")]
    channel_id: String,
    #[arg(long)]
    content: String,
}

#[derive(Args)]
pub(crate) struct SlackAddArgs {
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
pub(crate) struct SlackSendArgs {
    #[arg(long)]
    id: String,
    #[arg(long = "channel-id")]
    channel_id: String,
    #[arg(long)]
    text: String,
}

#[derive(Args)]
pub(crate) struct SignalAddArgs {
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
pub(crate) struct SignalSendArgs {
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
pub(crate) struct HomeAssistantAddArgs {
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
pub(crate) struct HomeAssistantServiceArgs {
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
pub(crate) struct DashboardArgs {
    #[arg(long, default_value_t = false)]
    print_url: bool,
    #[arg(long, default_value_t = false)]
    no_open: bool,
}

#[derive(Args)]
pub(crate) struct SupportBundleArgs {
    #[arg(long = "output-dir")]
    output_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 200)]
    log_limit: usize,
    #[arg(long, default_value_t = 25)]
    session_limit: usize,
}

#[derive(Args)]
pub(crate) struct InboxAddArgs {
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
pub(crate) struct ReviewArgs {
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
pub(crate) struct ResumeArgs {
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
pub(crate) struct ForkArgs {
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
pub(crate) struct CompletionArgs {
    #[arg(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Args)]
pub(crate) struct LogoutArgs {
    #[arg(long)]
    provider: Option<String>,

    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Args)]
pub(crate) struct ResetArgs {
    #[arg(long, short = 'y', default_value_t = false)]
    yes: bool,
}

#[derive(Subcommand)]
pub(crate) enum SessionCommands {
    List,
    Resume { id: String },
    ResumePacket { id: String },
    Rename { id: String, title: String },
}

#[derive(Subcommand)]
pub(crate) enum AutonomyCommands {
    Enable {
        #[arg(long, value_enum, default_value_t = AutonomyModeArg::FreeThinking)]
        mode: AutonomyModeArg,
        #[arg(long, num_args = 1, value_parser = clap::value_parser!(bool))]
        allow_self_edit: Option<bool>,
    },
    Pause,
    Resume,
    Status,
}

#[derive(Subcommand)]
pub(crate) enum EvolveCommands {
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
pub(crate) enum AutopilotCommands {
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
pub(crate) enum MissionCommands {
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
pub(crate) enum MemoryCommands {
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
pub(crate) struct LoginArgs {
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
pub(crate) enum HostedKindArg {
    #[value(name = "openai", alias = "openai-compatible")]
    OpenaiCompatible,
    Anthropic,
    Moonshot,
    Openrouter,
    Venice,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum LocalKindArg {
    Ollama,
    OpenaiCompatible,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PersistenceModeArg {
    OnDemand,
    AlwaysOn,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PermissionPresetArg {
    Suggest,
    AutoEdit,
    FullAuto,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TaskModeArg {
    Build,
    Daily,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum AutonomyModeArg {
    Assisted,
    FreeThinking,
    Evolve,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum AuthMethodArg {
    Browser,
    ApiKey,
    Oauth,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ThinkingLevelArg {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum MemoryKindArg {
    Preference,
    ProjectFact,
    Workflow,
    Constraint,
    Task,
    Note,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum MemoryScopeArg {
    Global,
    Workspace,
    Session,
    Provider,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum SkillDraftStatusArg {
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
