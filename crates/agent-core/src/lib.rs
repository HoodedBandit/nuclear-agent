use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod app_config;
mod control;
mod plugins;
mod workspace;

pub use control::*;
pub use plugins::*;
pub use workspace::*;

pub const APP_NAME: &str = "Agent Builder";
pub const APP_SLUG: &str = "agent-builder";
pub const DISPLAY_APP_NAME: &str = "Nuclear Agent";
pub const PRIMARY_COMMAND_NAME: &str = "nuclear";
pub const LEGACY_COMMAND_NAME: &str = "autism";
pub const CONFIG_VERSION: u32 = 2;
pub const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
pub const DEFAULT_DAEMON_PORT: u16 = 42690;
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_LOCAL_OPENAI_URL: &str = "http://127.0.0.1:5001/v1";
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_CHATGPT_CODEX_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MOONSHOT_URL: &str = "https://api.moonshot.ai/v1";
pub const DEFAULT_OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_VENICE_URL: &str = "https://api.venice.ai/api/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5";
pub const DEFAULT_CHATGPT_CODEX_MODEL: &str = "gpt-5-codex";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-4.1";
pub const DEFAULT_MOONSHOT_MODEL: &str = "kimi-k2";
pub const DEFAULT_VENICE_MODEL: &str = "venice-large";
pub const KEYCHAIN_SERVICE: &str = "agent-builder";
pub const INTERNAL_DAEMON_ARG: &str = "__daemon";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceMode {
    #[default]
    OnDemand,
    AlwaysOn,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionPreset {
    Suggest,
    #[default]
    AutoEdit,
    FullAuto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiCompatible,
    ChatGptCodex,
    Anthropic,
    Ollama,
}

impl ProviderKind {
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderKind::OpenAiCompatible => DEFAULT_OPENAI_URL,
            ProviderKind::ChatGptCodex => DEFAULT_CHATGPT_CODEX_URL,
            ProviderKind::Anthropic => DEFAULT_ANTHROPIC_URL,
            ProviderKind::Ollama => DEFAULT_OLLAMA_URL,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    ApiKey,
    #[serde(alias = "oauth_pending")]
    OAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OAuthConfig {
    pub client_id: String,
    pub authorization_url: String,
    pub token_url: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub extra_authorize_params: Vec<KeyValuePair>,
    #[serde(default)]
    pub extra_token_params: Vec<KeyValuePair>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_type: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub org_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub display_email: Option<String>,
    #[serde(default)]
    pub subscription_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyState {
    Disabled,
    Enabled,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyMode {
    #[default]
    Assisted,
    FreeThinking,
    Evolve,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutopilotState {
    #[default]
    Disabled,
    Enabled,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Queued,
    Running,
    Waiting,
    Scheduled,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

impl MissionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WakeTrigger {
    Manual,
    Timer,
    FollowUp,
    FileChange,
    UserMessage,
    Webhook,
    Inbox,
    Telegram,
    Discord,
    Slack,
    HomeAssistant,
    Signal,
    Gmail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionPhase {
    Planner,
    Executor,
    Reviewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    ProjectFact,
    Workflow,
    Constraint,
    Task,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    Global,
    Workspace,
    Session,
    Provider,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReviewStatus {
    #[default]
    Accepted,
    Candidate,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    App,
    Webhook,
    Inbox,
    Telegram,
    Discord,
    Slack,
    HomeAssistant,
    Signal,
    Gmail,
    Brave,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorApprovalStatus {
    #[default]
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputAttachment {
    pub kind: AttachmentKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub persistence_mode: PersistenceMode,
    pub auto_start: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_DAEMON_HOST.to_string(),
            port: DEFAULT_DAEMON_PORT,
            token: Uuid::new_v4().to_string(),
            persistence_mode: PersistenceMode::OnDemand,
            auto_start: false,
        }
    }
}

impl std::fmt::Display for PersistenceMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            PersistenceMode::OnDemand => "on-demand",
            PersistenceMode::AlwaysOn => "always-on",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    pub display_name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub auth_mode: AuthMode,
    pub default_model: Option<String>,
    pub keychain_account: Option<String>,
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
    pub local: bool,
}

impl ProviderConfig {
    pub fn has_saved_access_reference(&self) -> bool {
        matches!(self.auth_mode, AuthMode::None)
            || self
                .keychain_account
                .as_deref()
                .is_some_and(|account| !account.trim().is_empty())
    }

    #[deprecated(
        note = "metadata-only helper; use runtime credential validation for actual usability checks"
    )]
    pub fn has_usable_saved_access(&self) -> bool {
        self.has_saved_access_reference()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelAlias {
    pub alias: String,
    pub provider_id: String,
    pub model: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThinkingLevel {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ThinkingLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ThinkingLevel::None => "none",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::XHigh => "xhigh",
        }
    }
}

impl std::fmt::Display for ThinkingLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TrustPolicy {
    pub trusted_paths: Vec<PathBuf>,
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_full_disk: bool,
    pub allow_self_edit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyProfile {
    pub state: AutonomyState,
    #[serde(default)]
    pub mode: AutonomyMode,
    pub unlimited_usage: bool,
    pub full_network: bool,
    pub allow_self_edit: bool,
    pub consented_at: Option<DateTime<Utc>>,
}

impl Default for AutonomyProfile {
    fn default() -> Self {
        Self {
            state: AutonomyState::Disabled,
            mode: AutonomyMode::Assisted,
            unlimited_usage: false,
            full_network: false,
            allow_self_edit: false,
            consented_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvolveState {
    #[default]
    Disabled,
    Running,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvolveStopPolicy {
    #[default]
    AgentDecides,
    BudgetFriendly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvolveConfig {
    pub state: EvolveState,
    pub stop_policy: EvolveStopPolicy,
    pub whole_machine_scope: bool,
    pub test_gated: bool,
    pub stage_and_restart: bool,
    pub unlimited_recursion: bool,
    #[serde(default)]
    pub current_mission_id: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub iteration: u32,
    #[serde(default)]
    pub last_goal: Option<String>,
    #[serde(default)]
    pub last_summary: Option<String>,
    #[serde(default)]
    pub last_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pending_restart: bool,
    /// When true, evolve cycles must include a diff_summary reviewing all changes
    /// made during the cycle. Defaults to true.
    #[serde(default = "default_true")]
    pub diff_review_required: bool,
}

impl Default for EvolveConfig {
    fn default() -> Self {
        Self {
            state: EvolveState::Disabled,
            stop_policy: EvolveStopPolicy::AgentDecides,
            whole_machine_scope: true,
            test_gated: true,
            stage_and_restart: true,
            unlimited_recursion: true,
            current_mission_id: None,
            alias: None,
            requested_model: None,
            iteration: 0,
            last_goal: None,
            last_summary: None,
            last_verified_at: None,
            pending_restart: false,
            diff_review_required: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutopilotConfig {
    pub state: AutopilotState,
    pub max_concurrent_missions: u8,
    pub wake_interval_seconds: u64,
    pub allow_background_shell: bool,
    pub allow_background_network: bool,
    pub allow_background_self_edit: bool,
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            state: AutopilotState::Disabled,
            max_concurrent_missions: 1,
            wake_interval_seconds: 30,
            allow_background_shell: false,
            allow_background_network: false,
            allow_background_self_edit: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum DelegationLimit {
    Limited { value: u8 },
    Unlimited,
}

impl DelegationLimit {
    pub fn as_option(&self) -> Option<usize> {
        match self {
            DelegationLimit::Limited { value } => Some(usize::from(*value)),
            DelegationLimit::Unlimited => None,
        }
    }
}

impl std::fmt::Display for DelegationLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DelegationLimit::Limited { value } => write!(f, "{value}"),
            DelegationLimit::Unlimited => f.write_str("unlimited"),
        }
    }
}

fn default_delegation_depth() -> DelegationLimit {
    DelegationLimit::Limited { value: 1 }
}

fn default_delegation_parallel_limit() -> DelegationLimit {
    DelegationLimit::Limited { value: 8 }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegationConfig {
    #[serde(default = "default_delegation_depth")]
    pub max_depth: DelegationLimit,
    #[serde(default = "default_delegation_parallel_limit")]
    pub max_parallel_subagents: DelegationLimit,
    #[serde(default)]
    pub disabled_provider_ids: Vec<String>,
}

impl Default for DelegationConfig {
    fn default() -> Self {
        Self {
            max_depth: default_delegation_depth(),
            max_parallel_subagents: default_delegation_parallel_limit(),
            disabled_provider_ids: Vec::new(),
        }
    }
}

impl DelegationConfig {
    pub fn provider_enabled(&self, provider_id: &str) -> bool {
        !self
            .disabled_provider_ids
            .iter()
            .any(|id| id == provider_id)
    }
}

/// Configuration for embedding-based semantic memory search.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingConfig {
    /// Whether embedding-based semantic search is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// The provider ID to use for computing embeddings.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// The embedding model to use (e.g., "text-embedding-3-small").
    #[serde(default)]
    pub model: Option<String>,
    /// Number of dimensions for the embedding vector. 0 = use model default.
    #[serde(default)]
    pub dimensions: u32,
}

fn default_mission_max_retries() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mission {
    pub id: String,
    pub title: String,
    pub details: String,
    pub status: MissionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub phase: Option<MissionPhase>,
    #[serde(default)]
    pub handoff_summary: Option<String>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub watch_path: Option<PathBuf>,
    #[serde(default)]
    pub watch_recursive: bool,
    #[serde(default)]
    pub watch_fingerprint: Option<String>,
    #[serde(default)]
    pub wake_trigger: Option<WakeTrigger>,
    #[serde(default)]
    pub wake_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub scheduled_for_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub repeat_interval_seconds: Option<u64>,
    #[serde(default)]
    pub repeat_anchor_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub retries: u32,
    #[serde(default = "default_mission_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub evolve: bool,
}

impl Mission {
    pub fn new(title: String, details: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            details,
            status: MissionStatus::Queued,
            created_at: now,
            updated_at: now,
            alias: None,
            requested_model: None,
            session_id: None,
            phase: Some(MissionPhase::Planner),
            handoff_summary: None,
            workspace_key: None,
            watch_path: None,
            watch_recursive: false,
            watch_fingerprint: None,
            wake_trigger: Some(WakeTrigger::Manual),
            wake_at: None,
            scheduled_for_at: None,
            repeat_interval_seconds: None,
            repeat_anchor_at: None,
            last_error: None,
            retries: 0,
            max_retries: default_mission_max_retries(),
            evolve: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionCheckpoint {
    pub id: String,
    pub mission_id: String,
    pub status: MissionStatus,
    pub summary: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub phase: Option<MissionPhase>,
    #[serde(default)]
    pub handoff_summary: Option<String>,
    #[serde(default)]
    pub response_excerpt: Option<String>,
    #[serde(default)]
    pub next_wake_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub scheduled_for_at: Option<DateTime<Utc>>,
}

impl MissionCheckpoint {
    pub fn new(mission_id: String, status: MissionStatus, summary: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            mission_id,
            status,
            summary,
            created_at: Utc::now(),
            session_id: None,
            phase: None,
            handoff_summary: None,
            response_excerpt: None,
            next_wake_at: None,
            scheduled_for_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRecord {
    pub id: String,
    pub kind: MemoryKind,
    pub scope: MemoryScope,
    pub subject: String,
    pub content: String,
    pub confidence: u8,
    #[serde(default)]
    pub identity_key: Option<String>,
    #[serde(default)]
    pub observation_source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    #[serde(default)]
    pub review_status: MemoryReviewStatus,
    #[serde(default)]
    pub review_note: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub supersedes: Option<String>,
}

impl MemoryRecord {
    pub fn new(kind: MemoryKind, scope: MemoryScope, subject: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            kind,
            scope,
            subject,
            content,
            confidence: 100,
            identity_key: None,
            observation_source: None,
            created_at: now,
            updated_at: now,
            last_used_at: None,
            source_session_id: None,
            source_message_id: None,
            provider_id: None,
            workspace_key: None,
            evidence_refs: Vec::new(),
            tags: Vec::new(),
            superseded_by: None,
            review_status: MemoryReviewStatus::Accepted,
            review_note: None,
            reviewed_at: None,
            supersedes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEvidenceRef {
    pub session_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub role: Option<MessageRole>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryUpsertRequest {
    pub kind: MemoryKind,
    pub scope: MemoryScope,
    pub subject: String,
    pub content: String,
    #[serde(default)]
    pub confidence: Option<u8>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub identity_key: Option<String>,
    #[serde(default)]
    pub observation_source: Option<String>,
    #[serde(default)]
    pub review_status: Option<MemoryReviewStatus>,
    #[serde(default)]
    pub review_note: Option<String>,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryReviewUpdateRequest {
    pub status: MemoryReviewStatus,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemorySearchQuery {
    pub query: String,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub review_statuses: Vec<MemoryReviewStatus>,
    #[serde(default)]
    pub include_superseded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSearchHit {
    pub session_id: String,
    pub message_id: String,
    pub role: MessageRole,
    pub preview: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemorySearchResponse {
    #[serde(default)]
    pub memories: Vec<MemoryRecord>,
    #[serde(default)]
    pub transcript_hits: Vec<SessionSearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MemoryRebuildRequest {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub recompute_embeddings: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRebuildResponse {
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub sessions_scanned: usize,
    pub observations_scanned: usize,
    pub memories_upserted: usize,
    pub embeddings_refreshed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillDraftStatus {
    Draft,
    Published,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillDraft {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub instructions: String,
    #[serde(default)]
    pub trigger_hint: Option<String>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub source_session_id: Option<String>,
    #[serde(default)]
    pub source_message_ids: Vec<String>,
    #[serde(default)]
    pub usage_count: u32,
    pub status: SkillDraftStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_used_at: Option<DateTime<Utc>>,
}

impl SkillDraft {
    pub fn new(title: String, summary: String, instructions: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            summary,
            instructions,
            trigger_hint: None,
            workspace_key: None,
            provider_id: None,
            source_session_id: None,
            source_message_ids: Vec::new(),
            usage_count: 1,
            status: SkillDraftStatus::Draft,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    ToolSequence,
    ErrorRecovery,
    PreferredWorkflow,
    AvoidedAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsagePattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub description: String,
    #[serde(default)]
    pub trigger_hint: String,
    #[serde(default = "default_one_u32")]
    pub frequency: u32,
    #[serde(default = "default_fifty_u8")]
    pub confidence: u8,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub workspace_key: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
}

fn default_one_u32() -> u32 {
    1
}
fn default_fifty_u8() -> u8 {
    50
}

impl UsagePattern {
    pub fn new(pattern_type: PatternType, description: String, trigger_hint: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            pattern_type,
            description,
            trigger_hint,
            frequency: 1,
            confidence: 50,
            last_seen_at: now,
            created_at: now,
            workspace_key: None,
            provider_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub alias: String,
    pub provider_id: String,
    pub model: String,
    #[serde(default)]
    pub task_mode: Option<TaskMode>,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMessage {
    pub id: String,
    pub session_id: String,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub provider_payload_json: Option<String>,
    #[serde(default)]
    pub attachments: Vec<InputAttachment>,
}

impl SessionMessage {
    pub fn new(
        session_id: String,
        role: MessageRole,
        content: String,
        provider_id: Option<String>,
        model: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            role,
            content,
            created_at: Utc::now(),
            provider_id,
            model,
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
        }
    }

    pub fn with_attachments(mut self, attachments: Vec<InputAttachment>) -> Self {
        self.attachments = attachments;
        self
    }

    pub fn with_tool_metadata(
        mut self,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
    ) -> Self {
        self.tool_call_id = tool_call_id;
        self.tool_name = tool_name;
        self
    }

    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    pub fn with_provider_payload(mut self, provider_payload_json: Option<String>) -> Self {
        self.provider_payload_json = provider_payload_json;
        self
    }

    pub fn fork_to_session(&self, session_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            role: self.role.clone(),
            content: self.content.clone(),
            created_at: Utc::now(),
            provider_id: self.provider_id.clone(),
            model: self.model.clone(),
            tool_call_id: self.tool_call_id.clone(),
            tool_name: self.tool_name.clone(),
            tool_calls: self.tool_calls.clone(),
            provider_payload_json: self.provider_payload_json.clone(),
            attachments: self.attachments.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub id: String,
    pub level: String,
    pub scope: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

impl LogEntry {
    pub fn new(
        level: impl Into<String>,
        scope: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            level: level.into(),
            scope: scope.into(),
            message: message.into(),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub version: u32,
    pub daemon: DaemonConfig,
    pub main_agent_alias: Option<String>,
    pub providers: Vec<ProviderConfig>,
    pub aliases: Vec<ModelAlias>,
    #[serde(default)]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub permission_preset: PermissionPreset,
    pub trust_policy: TrustPolicy,
    pub autonomy: AutonomyProfile,
    #[serde(default)]
    pub evolve: EvolveConfig,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub app_connectors: Vec<AppConnectorConfig>,
    #[serde(default)]
    pub plugins: Vec<InstalledPluginConfig>,
    #[serde(default)]
    pub webhook_connectors: Vec<WebhookConnectorConfig>,
    #[serde(default)]
    pub inbox_connectors: Vec<InboxConnectorConfig>,
    #[serde(default)]
    pub telegram_connectors: Vec<TelegramConnectorConfig>,
    #[serde(default)]
    pub discord_connectors: Vec<DiscordConnectorConfig>,
    #[serde(default)]
    pub slack_connectors: Vec<SlackConnectorConfig>,
    #[serde(default)]
    pub home_assistant_connectors: Vec<HomeAssistantConnectorConfig>,
    #[serde(default)]
    pub signal_connectors: Vec<SignalConnectorConfig>,
    #[serde(default)]
    pub gmail_connectors: Vec<GmailConnectorConfig>,
    #[serde(default)]
    pub brave_connectors: Vec<BraveConnectorConfig>,
    #[serde(default)]
    pub enabled_skills: Vec<String>,
    #[serde(default)]
    pub autopilot: AutopilotConfig,
    #[serde(default)]
    pub delegation: DelegationConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    pub onboarding_complete: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            daemon: DaemonConfig::default(),
            main_agent_alias: None,
            providers: Vec::new(),
            aliases: Vec::new(),
            thinking_level: None,
            permission_preset: PermissionPreset::default(),
            trust_policy: TrustPolicy::default(),
            autonomy: AutonomyProfile::default(),
            evolve: EvolveConfig::default(),
            mcp_servers: Vec::new(),
            app_connectors: Vec::new(),
            plugins: Vec::new(),
            webhook_connectors: Vec::new(),
            inbox_connectors: Vec::new(),
            telegram_connectors: Vec::new(),
            discord_connectors: Vec::new(),
            slack_connectors: Vec::new(),
            home_assistant_connectors: Vec::new(),
            signal_connectors: Vec::new(),
            gmail_connectors: Vec::new(),
            brave_connectors: Vec::new(),
            enabled_skills: Vec::new(),
            autopilot: AutopilotConfig::default(),
            delegation: DelegationConfig::default(),
            embedding: EmbeddingConfig::default(),
            onboarding_complete: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderReply {
    pub provider_id: String,
    pub model: String,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub provider_payload_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub provider_payload_json: Option<String>,
    #[serde(default)]
    pub attachments: Vec<InputAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub tool_name: String,
    pub input_schema_json: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub tool_name: String,
    pub input_schema_json: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token_sha256: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub delete_after_read: bool,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
    #[serde(default)]
    pub allowed_user_ids: Vec<i64>,
    #[serde(default)]
    pub last_update_id: Option<i64>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordChannelCursor {
    pub channel_id: String,
    #[serde(default)]
    pub last_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub channel_cursors: Vec<DiscordChannelCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackChannelCursor {
    pub channel_id: String,
    #[serde(default)]
    pub last_message_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub channel_cursors: Vec<SlackChannelCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantEntityCursor {
    pub entity_id: String,
    #[serde(default)]
    pub last_state: Option<String>,
    #[serde(default)]
    pub last_changed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub base_url: String,
    #[serde(default)]
    pub access_token_keychain_account: Option<String>,
    #[serde(default)]
    pub monitored_entity_ids: Vec<String>,
    #[serde(default)]
    pub allowed_service_domains: Vec<String>,
    #[serde(default)]
    pub allowed_service_entity_ids: Vec<String>,
    #[serde(default)]
    pub entity_cursors: Vec<HomeAssistantEntityCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub account: String,
    #[serde(default)]
    pub cli_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_group_ids: Vec<String>,
    #[serde(default)]
    pub allowed_group_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BraveConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key_keychain_account: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorApprovalRecord {
    pub id: String,
    pub connector_kind: ConnectorKind,
    pub connector_id: String,
    pub connector_name: String,
    pub status: ConnectorApprovalStatus,
    pub title: String,
    pub details: String,
    pub source_key: String,
    #[serde(default)]
    pub source_event_id: Option<String>,
    #[serde(default)]
    pub external_chat_id: Option<String>,
    #[serde(default)]
    pub external_chat_display: Option<String>,
    #[serde(default)]
    pub external_user_id: Option<String>,
    #[serde(default)]
    pub external_user_display: Option<String>,
    #[serde(default)]
    pub message_preview: Option<String>,
    #[serde(default)]
    pub queued_mission_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub review_note: Option<String>,
}

impl ConnectorApprovalRecord {
    pub fn new(
        connector_kind: ConnectorKind,
        connector_id: String,
        connector_name: String,
        title: String,
        details: String,
        source_key: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            connector_kind,
            connector_id,
            connector_name,
            status: ConnectorApprovalStatus::Pending,
            title,
            details,
            source_key,
            source_event_id: None,
            external_chat_id: None,
            external_chat_display: None,
            external_user_id: None,
            external_user_display: None,
            message_preview: None,
            queued_mission_id: None,
            created_at: now,
            updated_at: now,
            reviewed_at: None,
            review_note: None,
        }
    }
}

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
pub struct McpServerUpsertRequest {
    pub server: McpServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConnectorUpsertRequest {
    pub connector: AppConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConnectorUpsertRequest {
    pub connector: WebhookConnectorConfig,
    #[serde(default)]
    pub webhook_token: Option<String>,
    #[serde(default)]
    pub clear_webhook_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxConnectorUpsertRequest {
    pub connector: InboxConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConnectorUpsertRequest {
    pub connector: TelegramConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConnectorUpsertRequest {
    pub connector: DiscordConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackConnectorUpsertRequest {
    pub connector: SlackConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantConnectorUpsertRequest {
    pub connector: HomeAssistantConnectorConfig,
    #[serde(default)]
    pub access_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalConnectorUpsertRequest {
    pub connector: SignalConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookEventRequest {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookEventResponse {
    pub connector_id: String,
    pub mission_id: String,
    pub title: String,
    pub status: MissionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxPollResponse {
    pub connector_id: String,
    pub processed_files: usize,
    pub queued_missions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramPollResponse {
    pub connector_id: String,
    pub processed_updates: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub last_update_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub updated_channels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub updated_channels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantPollResponse {
    pub connector_id: String,
    pub processed_entities: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub updated_entities: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorApprovalUpdateRequest {
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramSendRequest {
    pub chat_id: i64,
    pub text: String,
    #[serde(default)]
    pub disable_notification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramSendResponse {
    pub connector_id: String,
    pub chat_id: i64,
    #[serde(default)]
    pub message_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordSendRequest {
    pub channel_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordSendResponse {
    pub connector_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackSendRequest {
    pub channel_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackSendResponse {
    pub connector_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub message_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeAssistantEntityState {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub last_changed: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeAssistantServiceCallRequest {
    pub domain: String,
    pub service: String,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub service_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantServiceCallResponse {
    pub connector_id: String,
    pub domain: String,
    pub service: String,
    pub changed_entities: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalSendRequest {
    #[serde(default)]
    pub recipient: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalSendResponse {
    pub connector_id: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub oauth_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub allowed_sender_addresses: Vec<String>,
    #[serde(default)]
    pub label_filter: Option<String>,
    #[serde(default)]
    pub last_history_id: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailConnectorUpsertRequest {
    pub connector: GmailConnectorConfig,
    #[serde(default)]
    pub oauth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BraveConnectorUpsertRequest {
    pub connector: BraveConnectorConfig,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailSendRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailSendResponse {
    pub connector_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillUpdateRequest {
    pub enabled_skills: Vec<String>,
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
