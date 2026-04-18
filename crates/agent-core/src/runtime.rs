use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubAgentTask {
    pub prompt: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub task_mode: Option<TaskMode>,
    #[serde(default)]
    pub output_schema_json: Option<String>,
    #[serde(default)]
    pub strategy: Option<SubAgentStrategy>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskMode {
    Build,
    Daily,
}

impl TaskMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Daily => "daily",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentStrategy {
    #[default]
    SingleBest,
    ParallelBestEffort,
    ParallelAll,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubAgentResult {
    pub alias: String,
    pub provider_id: String,
    pub model: String,
    pub success: bool,
    pub response: String,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub structured_output_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolExecutionRecord {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub outcome: ToolExecutionOutcome,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTaskRequest {
    pub prompt: String,
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    pub session_id: Option<String>,
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub attachments: Vec<InputAttachment>,
    #[serde(default)]
    pub permission_preset: Option<PermissionPreset>,
    #[serde(default)]
    pub task_mode: Option<TaskMode>,
    #[serde(default)]
    pub output_schema_json: Option<String>,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default)]
    pub remote_content_policy_override: Option<RemoteContentPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunTaskResponse {
    pub session_id: String,
    pub alias: String,
    pub provider_id: String,
    pub model: String,
    pub response: String,
    #[serde(default)]
    pub tool_events: Vec<ToolExecutionRecord>,
    #[serde(default)]
    pub structured_output_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunTaskStreamEvent {
    SessionStarted {
        session_id: String,
        alias: String,
        provider_id: String,
        model: String,
    },
    Message {
        message: SessionMessage,
    },
    RemoteContent {
        artifact: RemoteContentArtifact,
    },
    Completed {
        response: RunTaskResponse,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchTaskRequest {
    pub tasks: Vec<SubAgentTask>,
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub task_mode: Option<TaskMode>,
    #[serde(default)]
    pub strategy: Option<SubAgentStrategy>,
    #[serde(default)]
    pub parent_alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchTaskResponse {
    pub summary: String,
    pub results: Vec<SubAgentResult>,
    #[serde(default)]
    pub all_succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegationTarget {
    pub alias: String,
    pub provider_id: String,
    pub provider_display_name: String,
    pub model: String,
    #[serde(default)]
    pub target_names: Vec<String>,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AliasUpsertRequest {
    pub alias: ModelAlias,
    pub set_as_main: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRenameRequest {
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderUpsertRequest {
    pub provider: ProviderConfig,
    pub api_key: Option<String>,
    pub oauth_token: Option<OAuthToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSuggestionRequest {
    pub preferred_provider_id: String,
    #[serde(default)]
    pub preferred_alias_name: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub editing_provider_id: Option<String>,
    #[serde(default)]
    pub editing_alias_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSuggestionResponse {
    pub provider_id: String,
    #[serde(default)]
    pub alias_name: Option<String>,
    #[serde(default)]
    pub alias_model: Option<String>,
    pub would_be_first_main: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderAuthKind {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderAuthSessionStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserProviderAuthStartRequest {
    pub kind: BrowserProviderAuthKind,
    pub provider_id: String,
    pub display_name: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub alias_name: Option<String>,
    #[serde(default)]
    pub alias_model: Option<String>,
    #[serde(default)]
    pub alias_description: Option<String>,
    #[serde(default)]
    pub set_as_main: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserProviderAuthStartResponse {
    pub session_id: String,
    pub status: BrowserProviderAuthSessionStatus,
    #[serde(default)]
    pub authorization_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserProviderAuthStatusResponse {
    pub session_id: String,
    pub kind: BrowserProviderAuthKind,
    pub provider_id: String,
    pub display_name: String,
    pub status: BrowserProviderAuthSessionStatus,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustUpdateRequest {
    pub trusted_path: Option<PathBuf>,
    pub allow_shell: Option<bool>,
    pub allow_network: Option<bool>,
    pub allow_full_disk: Option<bool>,
    pub allow_self_edit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonConfigUpdateRequest {
    pub persistence_mode: Option<PersistenceMode>,
    pub auto_start: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegationConfigUpdateRequest {
    #[serde(default)]
    pub max_depth: Option<DelegationLimit>,
    #[serde(default)]
    pub max_parallel_subagents: Option<DelegationLimit>,
    #[serde(default)]
    pub disabled_provider_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionUpdateRequest {
    pub permission_preset: PermissionPreset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyEnableRequest {
    #[serde(default)]
    pub mode: Option<AutonomyMode>,
    pub allow_self_edit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvolveStartRequest {
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub budget_friendly: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutopilotUpdateRequest {
    #[serde(default)]
    pub state: Option<AutopilotState>,
    #[serde(default)]
    pub max_concurrent_missions: Option<u8>,
    #[serde(default)]
    pub wake_interval_seconds: Option<u64>,
    #[serde(default)]
    pub allow_background_shell: Option<bool>,
    #[serde(default)]
    pub allow_background_network: Option<bool>,
    #[serde(default)]
    pub allow_background_self_edit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionControlRequest {
    #[serde(default)]
    pub wake_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub clear_wake_at: bool,
    #[serde(default)]
    pub repeat_interval_seconds: Option<u64>,
    #[serde(default)]
    pub clear_repeat_interval_seconds: bool,
    #[serde(default)]
    pub watch_path: Option<PathBuf>,
    #[serde(default)]
    pub clear_watch_path: bool,
    #[serde(default)]
    pub watch_recursive: Option<bool>,
    #[serde(default)]
    pub clear_session_id: bool,
    #[serde(default)]
    pub clear_handoff_summary: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillUpdateRequest {
    pub enabled_skills: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallKind {
    Packaged,
    Source,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAvailabilityState {
    UpToDate,
    Available,
    Blocked,
    Unsupported,
    InProgress,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateOperationStep {
    Checking,
    Downloading,
    Verifying,
    Applying,
    Restarting,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateRunState {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateInstallTarget {
    pub kind: UpdateInstallKind,
    pub executable_path: String,
    #[serde(default)]
    pub install_dir: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub build_profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateRunSummary {
    pub state: UpdateRunState,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub to_version: Option<String>,
    #[serde(default)]
    pub from_commit: Option<String>,
    #[serde(default)]
    pub to_commit: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateStatusResponse {
    pub install: UpdateInstallTarget,
    pub current_version: String,
    #[serde(default)]
    pub current_commit: Option<String>,
    pub availability: UpdateAvailabilityState,
    pub checked_at: DateTime<Utc>,
    #[serde(default)]
    pub step: Option<UpdateOperationStep>,
    #[serde(default)]
    pub candidate_version: Option<String>,
    #[serde(default)]
    pub candidate_tag: Option<String>,
    #[serde(default)]
    pub candidate_commit: Option<String>,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub last_run: Option<UpdateRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UpdateRunRequest {
    #[serde(default)]
    pub wait_for_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatus {
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub persistence_mode: PersistenceMode,
    pub auto_start: bool,
    #[serde(default)]
    pub main_agent_alias: Option<String>,
    #[serde(default)]
    pub main_target: Option<MainTargetSummary>,
    #[serde(default)]
    pub onboarding_complete: bool,
    pub autonomy: AutonomyProfile,
    #[serde(default)]
    pub evolve: EvolveConfig,
    pub autopilot: AutopilotConfig,
    pub delegation: DelegationConfig,
    pub providers: usize,
    pub aliases: usize,
    #[serde(default)]
    pub plugins: usize,
    pub delegation_targets: usize,
    #[serde(default)]
    pub webhook_connectors: usize,
    #[serde(default)]
    pub inbox_connectors: usize,
    #[serde(default)]
    pub telegram_connectors: usize,
    #[serde(default)]
    pub discord_connectors: usize,
    #[serde(default)]
    pub slack_connectors: usize,
    #[serde(default)]
    pub home_assistant_connectors: usize,
    #[serde(default)]
    pub signal_connectors: usize,
    #[serde(default)]
    pub gmail_connectors: usize,
    #[serde(default)]
    pub brave_connectors: usize,
    #[serde(default)]
    pub pending_connector_approvals: usize,
    pub missions: usize,
    pub active_missions: usize,
    pub memories: usize,
    #[serde(default)]
    pub pending_memory_reviews: usize,
    #[serde(default)]
    pub skill_drafts: usize,
    #[serde(default)]
    pub published_skills: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MainTargetSummary {
    pub alias: String,
    pub provider_id: String,
    pub provider_display_name: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MainAliasUpdateRequest {
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardBootstrapResponse {
    pub status: DaemonStatus,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub aliases: Vec<ModelAlias>,
    #[serde(default)]
    pub delegation_targets: Vec<DelegationTarget>,
    #[serde(default)]
    pub telegram_connectors: Vec<TelegramConnectorConfig>,
    #[serde(default)]
    pub discord_connectors: Vec<DiscordConnectorConfig>,
    #[serde(default)]
    pub slack_connectors: Vec<SlackConnectorConfig>,
    #[serde(default)]
    pub signal_connectors: Vec<SignalConnectorConfig>,
    #[serde(default)]
    pub home_assistant_connectors: Vec<HomeAssistantConnectorConfig>,
    #[serde(default)]
    pub webhook_connectors: Vec<WebhookConnectorConfig>,
    #[serde(default)]
    pub inbox_connectors: Vec<InboxConnectorConfig>,
    #[serde(default)]
    pub gmail_connectors: Vec<GmailConnectorConfig>,
    #[serde(default)]
    pub brave_connectors: Vec<BraveConnectorConfig>,
    #[serde(default)]
    pub plugins: Vec<InstalledPluginConfig>,
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
    #[serde(default)]
    pub events: Vec<LogEntry>,
    pub permissions: PermissionPreset,
    pub trust: TrustPolicy,
    pub delegation_config: DelegationConfig,
    #[serde(default)]
    pub provider_capabilities: Vec<ProviderCapabilitySummary>,
    #[serde(default)]
    pub remote_content_policy: RemoteContentPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardLaunchResponse {
    pub launch_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderHealth {
    pub id: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthReport {
    pub daemon_running: bool,
    pub config_path: String,
    pub data_path: String,
    pub keyring_ok: bool,
    pub providers: Vec<ProviderHealth>,
    #[serde(default)]
    pub plugins: Vec<PluginDoctorReport>,
    #[serde(default)]
    pub remote_content_policy: RemoteContentPolicy,
    #[serde(default)]
    pub provider_capabilities: Vec<ProviderCapabilitySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionTranscript {
    pub session: SessionSummary,
    pub messages: Vec<SessionMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionResumePacket {
    pub session: SessionSummary,
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub recent_messages: Vec<SessionMessage>,
    #[serde(default)]
    pub linked_memories: Vec<MemoryRecord>,
    #[serde(default)]
    pub related_transcript_hits: Vec<SessionSearchHit>,
}
