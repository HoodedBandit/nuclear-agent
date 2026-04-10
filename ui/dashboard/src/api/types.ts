export type PersistenceMode = "on_demand" | "always_on";
export type PermissionPreset = "suggest" | "auto_edit" | "full_auto";
export type ProviderKind =
  | "open_ai_compatible"
  | "chat_gpt_codex"
  | "anthropic"
  | "ollama";
export type AuthMode = "none" | "api_key" | "oauth";
export type ThinkingLevel =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh";
export type TaskMode = "build" | "daily";
export type AttachmentKind = "image" | "file";
export type RemoteContentPolicy = "allow" | "warn_only" | "block_high_risk";
export type MemoryKind =
  | "preference"
  | "project_fact"
  | "workflow"
  | "constraint"
  | "task"
  | "note";
export type MemoryScope = "global" | "workspace" | "session" | "provider";
export type MemoryReviewStatus = "accepted" | "candidate" | "rejected";
export type MissionStatus =
  | "queued"
  | "running"
  | "waiting"
  | "scheduled"
  | "blocked"
  | "completed"
  | "failed"
  | "cancelled";
export type ConnectorApprovalStatus = "pending" | "approved" | "rejected";
export type AutonomyState = "disabled" | "enabled" | "paused";
export type AutonomyMode = "assisted" | "free_thinking" | "evolve";
export type AutopilotState = "disabled" | "enabled" | "paused";
export type EvolveState =
  | "disabled"
  | "running"
  | "paused"
  | "completed"
  | "failed";
export type BrowserProviderAuthSessionStatus =
  | "pending"
  | "completed"
  | "failed";
export type ConnectorKind =
  | "app"
  | "webhook"
  | "inbox"
  | "telegram"
  | "discord"
  | "slack"
  | "home_assistant"
  | "signal"
  | "gmail"
  | "brave";

export interface KeyValuePair {
  key: string;
  value: string;
}

export interface OAuthConfig {
  client_id: string;
  authorization_url: string;
  token_url: string;
  scopes?: string[];
  extra_authorize_params?: KeyValuePair[];
  extra_token_params?: KeyValuePair[];
}

export interface OAuthToken {
  access_token: string;
  refresh_token?: string | null;
  expires_at?: string | null;
  token_type?: string | null;
  scopes?: string[];
}

export interface ProviderConfig {
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string;
  auth_mode: AuthMode;
  default_model?: string | null;
  keychain_account?: string | null;
  oauth?: OAuthConfig | null;
  local: boolean;
}

export interface ModelAlias {
  alias: string;
  provider_id: string;
  model: string;
  description?: string | null;
}

export interface DelegationLimit {
  mode: "limited" | "unlimited";
  value?: number;
}

export interface DelegationConfig {
  max_depth: DelegationLimit;
  max_parallel_subagents: DelegationLimit;
  disabled_provider_ids: string[];
}

export interface DelegationTarget {
  alias: string;
  provider_id: string;
  provider_display_name: string;
  model: string;
  target_names?: string[];
  primary?: boolean;
}

export interface ProviderCapabilitySummary {
  provider_id: string;
  model: string;
  capabilities: Record<string, boolean>;
}

export interface MainTargetSummary {
  alias: string;
  provider_id: string;
  provider_display_name: string;
  model: string;
}

export interface InputAttachment {
  kind: AttachmentKind;
  path: string;
}

export interface AutonomyProfile {
  state: AutonomyState;
  mode: AutonomyMode;
  unlimited_usage: boolean;
  full_network: boolean;
  allow_self_edit: boolean;
  consented_at?: string | null;
}

export interface EvolveConfig {
  state: EvolveState;
  stop_policy: string;
  whole_machine_scope: boolean;
  test_gated: boolean;
  stage_and_restart: boolean;
  unlimited_recursion: boolean;
  current_mission_id?: string | null;
  alias?: string | null;
  requested_model?: string | null;
  iteration: number;
  last_goal?: string | null;
  last_summary?: string | null;
  last_verified_at?: string | null;
  pending_restart: boolean;
  diff_review_required: boolean;
}

export interface AutopilotConfig {
  state: AutopilotState;
  max_concurrent_missions: number;
  wake_interval_seconds: number;
  allow_background_shell: boolean;
  allow_background_network: boolean;
  allow_background_self_edit: boolean;
}

export interface TrustPolicy {
  trusted_paths: string[];
  allow_shell: boolean;
  allow_network: boolean;
  allow_full_disk: boolean;
  allow_self_edit: boolean;
}

export interface DaemonStatus {
  pid: number;
  started_at: string;
  persistence_mode: PersistenceMode;
  auto_start: boolean;
  main_agent_alias?: string | null;
  main_target?: MainTargetSummary | null;
  onboarding_complete: boolean;
  autonomy: AutonomyProfile;
  evolve: EvolveConfig;
  autopilot: AutopilotConfig;
  delegation: DelegationConfig;
  providers: number;
  aliases: number;
  plugins: number;
  delegation_targets: number;
  webhook_connectors: number;
  inbox_connectors: number;
  telegram_connectors: number;
  discord_connectors: number;
  slack_connectors: number;
  home_assistant_connectors: number;
  signal_connectors: number;
  gmail_connectors: number;
  brave_connectors: number;
  pending_connector_approvals: number;
  missions: number;
  active_missions: number;
  memories: number;
  pending_memory_reviews: number;
  skill_drafts: number;
  published_skills: number;
}

export interface ConnectorBase {
  id: string;
  name: string;
  description: string;
  enabled?: boolean;
  alias?: string | null;
  requested_model?: string | null;
  cwd?: string | null;
}

export interface AppConnectorConfig extends ConnectorBase {
  command: string;
  args?: string[];
  tool_name: string;
  input_schema_json: string;
}

export type McpServerConfig = AppConnectorConfig;

export interface WebhookConnectorConfig extends ConnectorBase {
  prompt_template: string;
  token_sha256?: string | null;
}

export interface InboxConnectorConfig extends ConnectorBase {
  path: string;
  delete_after_read?: boolean;
}

export interface TelegramConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  allowed_chat_ids?: number[];
  allowed_user_ids?: number[];
  last_update_id?: number | null;
}

export interface DiscordConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids?: string[];
  allowed_channel_ids?: string[];
  allowed_user_ids?: string[];
}

export interface SlackConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids?: string[];
  allowed_channel_ids?: string[];
  allowed_user_ids?: string[];
}

export interface SignalConnectorConfig extends ConnectorBase {
  account: string;
  cli_path?: string | null;
  require_pairing_approval?: boolean;
  monitored_group_ids?: string[];
  allowed_group_ids?: string[];
  allowed_user_ids?: string[];
}

export interface HomeAssistantConnectorConfig extends ConnectorBase {
  base_url: string;
  access_token_keychain_account?: string | null;
  monitored_entity_ids?: string[];
  allowed_service_domains?: string[];
  allowed_service_entity_ids?: string[];
}

export interface GmailConnectorConfig extends ConnectorBase {
  oauth_keychain_account?: string | null;
  allowed_senders?: string[];
  require_pairing_approval?: boolean;
}

export interface BraveConnectorConfig extends ConnectorBase {
  api_key_keychain_account?: string | null;
}

export interface InstalledPluginConfig {
  id: string;
  enabled: boolean;
  trusted: boolean;
  pinned: boolean;
  source_kind?: string | null;
  source_reference?: string | null;
  source_path: string;
  install_dir: string;
  integrity_sha256?: string | null;
  reviewed_integrity_sha256?: string | null;
  reviewed_at?: string | null;
  granted_permissions?: Record<string, boolean>;
  manifest: {
    name: string;
    version: string;
    description: string;
    tools: unknown[];
    connectors: unknown[];
    provider_adapters: unknown[];
  };
}

export type PluginSourceKind = "local_path" | "git_repo" | "marketplace";

export interface PluginPermissions {
  shell: boolean;
  network: boolean;
  full_disk: boolean;
}

export interface PluginDoctorReport {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  trusted: boolean;
  runtime_ready: boolean;
  ok: boolean;
  detail: string;
  tools: number;
  connectors: number;
  provider_adapters: number;
  integrity_sha256: string;
  source_kind: PluginSourceKind;
  declared_permissions: PluginPermissions;
  granted_permissions: PluginPermissions;
  reviewed_at?: string | null;
}

export interface SessionSummary {
  id: string;
  title?: string | null;
  alias: string;
  provider_id: string;
  model: string;
  cwd?: string | null;
  task_mode?: TaskMode | null;
  created_at: string;
  updated_at: string;
}

export interface SessionMessage {
  id: string;
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  created_at: string;
}

export interface SessionTranscript {
  session: SessionSummary;
  messages: SessionMessage[];
}

export interface MemoryRecord {
  id: string;
  kind: MemoryKind;
  scope: MemoryScope;
  subject: string;
  content: string;
  confidence: number;
  review_status: MemoryReviewStatus;
  tags?: string[];
  workspace_key?: string | null;
  provider_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface SessionSearchHit {
  session_id: string;
  score: number;
  title?: string | null;
  snippet: string;
}

export interface SessionResumePacket {
  session: SessionSummary;
  generated_at: string;
  recent_messages: SessionMessage[];
  linked_memories: MemoryRecord[];
  related_transcript_hits: SessionSearchHit[];
}

export interface LogEntry {
  id: string;
  level: string;
  target: string;
  message: string;
  created_at: string;
}

export interface Mission {
  id: string;
  title: string;
  details: string;
  status: MissionStatus;
  created_at: string;
  updated_at: string;
  alias?: string | null;
  requested_model?: string | null;
  session_id?: string | null;
  phase?: string | null;
  handoff_summary?: string | null;
  workspace_key?: string | null;
  watch_path?: string | null;
  watch_recursive?: boolean;
  wake_at?: string | null;
  repeat_interval_seconds?: number | null;
  wake_trigger?: string | null;
  retries?: number;
  max_retries?: number;
}

export interface MissionCheckpoint {
  mission_id: string;
  status: MissionStatus;
  summary: string;
  created_at: string;
  session_id?: string | null;
}

export interface ConnectorApprovalRecord {
  id: string;
  connector_kind: ConnectorKind;
  connector_id: string;
  connector_name: string;
  status: ConnectorApprovalStatus;
  title: string;
  details: string;
  source_key: string;
  message_preview?: string | null;
  queued_mission_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface SkillDraft {
  id: string;
  title: string;
  content: string;
  status: "draft" | "published" | "rejected";
  created_at: string;
  updated_at: string;
}

export interface HealthReport {
  daemon_running: boolean;
  config_path: string;
  data_path: string;
  keyring_ok: boolean;
  providers: Array<{ id: string; ok: boolean; detail: string }>;
  plugins: Array<{
    id: string;
    name: string;
    version: string;
    ok: boolean;
    enabled: boolean;
    trusted: boolean;
    detail: string;
    runtime_ready?: boolean;
    declared_permissions?: Record<string, boolean>;
    granted_permissions?: Record<string, boolean>;
  }>;
  remote_content_policy?: string;
  provider_capabilities?: ProviderCapabilitySummary[];
}

export interface DashboardBootstrapResponse {
  status: DaemonStatus;
  providers: ProviderConfig[];
  aliases: ModelAlias[];
  delegation_targets: DelegationTarget[];
  telegram_connectors: TelegramConnectorConfig[];
  discord_connectors: DiscordConnectorConfig[];
  slack_connectors: SlackConnectorConfig[];
  signal_connectors: SignalConnectorConfig[];
  home_assistant_connectors: HomeAssistantConnectorConfig[];
  webhook_connectors: WebhookConnectorConfig[];
  inbox_connectors: InboxConnectorConfig[];
  gmail_connectors: GmailConnectorConfig[];
  brave_connectors: BraveConnectorConfig[];
  plugins: InstalledPluginConfig[];
  sessions: SessionSummary[];
  events: LogEntry[];
  permissions: PermissionPreset;
  trust: TrustPolicy;
  delegation_config: DelegationConfig;
  provider_capabilities: ProviderCapabilitySummary[];
  remote_content_policy: string;
}

export interface BrowserProviderAuthStartResponse {
  session_id: string;
  status: BrowserProviderAuthSessionStatus;
  authorization_url?: string | null;
}

export interface BrowserProviderAuthStatusResponse {
  session_id: string;
  kind: "codex" | "claude";
  provider_id: string;
  display_name: string;
  status: BrowserProviderAuthSessionStatus;
  error?: string | null;
}

export interface RunTaskResponse {
  session_id: string;
  alias: string;
  provider_id: string;
  model: string;
  response: string;
  tool_events?: Array<{
    call_id: string;
    name: string;
    arguments: string;
    outcome: string;
    output: string;
  }>;
  structured_output_json?: string | null;
}

export interface MemorySearchResponse {
  memories: MemoryRecord[];
  transcript_hits: SessionSearchHit[];
}

export interface MemoryRebuildResponse {
  generated_at: string;
  sessions_scanned: number;
  observations_scanned: number;
  memories_upserted: number;
  embeddings_refreshed: number;
}

export interface SupportBundleResponse {
  bundle_dir: string;
  generated_at: string;
  files: string[];
}
