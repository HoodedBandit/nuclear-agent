export type PermissionPreset = "suggest" | "auto_edit" | "full_auto";
export type RemoteContentPolicy = "allow" | "warn_only" | "block_high_risk";
export type TaskMode = "build" | "daily";
export type MessageRole = "system" | "user" | "assistant" | "tool";
export type ProviderKind =
  | "open_ai_compatible"
  | "chat_gpt_codex"
  | "anthropic"
  | "ollama";
export type ProviderProfile =
  | "open_ai"
  | "open_router"
  | "moonshot"
  | "venice"
  | "anthropic"
  | "ollama"
  | "local_open_ai_compatible"
  | "generic_open_ai_compatible";
export type AuthMode = "none" | "api_key" | "oauth";
export type AttachmentKind = "image" | "file";
export type BrowserProviderAuthKind = "codex";
export type BrowserProviderAuthSessionStatus = "pending" | "completed" | "failed";
export type ConnectorApprovalStatus = "pending" | "approved" | "rejected";
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
export type MissionStatus =
  | "queued"
  | "running"
  | "blocked"
  | "waiting"
  | "scheduled"
  | "completed"
  | "failed"
  | "cancelled";

export interface MainTargetSummary {
  alias: string;
  provider_id: string;
  provider_display_name?: string;
  model: string;
  description?: string | null;
}

export interface AutonomyProfile {
  state: string;
  mode: string;
  unlimited_usage: boolean;
  full_network: boolean;
  allow_self_edit: boolean;
  consented_at?: string | null;
}

export interface EvolveStatus {
  state: string;
  current_mission_id?: string | null;
  iteration?: number;
  pending_restart?: boolean;
}

export interface AutopilotConfig {
  state: string;
  max_concurrent_missions: number;
  wake_interval_seconds: number;
  allow_background_shell: boolean;
  allow_background_network: boolean;
  allow_background_self_edit: boolean;
}

export interface DelegationConfig {
  max_depth: string;
  max_parallel_subagents: string;
  disabled_provider_ids: string[];
}

export interface DaemonStatus {
  pid: number;
  started_at: string;
  persistence_mode: string;
  auto_start: boolean;
  main_agent_alias?: string | null;
  main_target?: MainTargetSummary | null;
  onboarding_complete: boolean;
  autonomy: AutonomyProfile;
  evolve: EvolveStatus;
  autopilot: AutopilotConfig;
  delegation: DelegationConfig;
  providers?: number;
  aliases?: number;
  plugins?: number;
  delegation_targets?: number;
  webhook_connectors?: number;
  inbox_connectors?: number;
  telegram_connectors?: number;
  discord_connectors?: number;
  slack_connectors?: number;
  home_assistant_connectors?: number;
  signal_connectors?: number;
  gmail_connectors?: number;
  brave_connectors?: number;
  pending_connector_approvals?: number;
  missions?: number;
  active_missions?: number;
  memories?: number;
  pending_memory_reviews?: number;
  skill_drafts?: number;
  published_skills?: number;
}

export interface ProviderConfig {
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string;
  provider_profile?: ProviderProfile | null;
  auth_mode: AuthMode;
  default_model?: string | null;
  keychain_account?: string | null;
  local: boolean;
}

export interface ProviderUpsertRequest {
  provider: ProviderConfig;
  api_key?: string | null;
  oauth_token?: unknown | null;
}

export interface ModelAlias {
  alias: string;
  provider_id: string;
  model: string;
  description?: string | null;
}

export interface AliasUpsertRequest {
  alias: ModelAlias;
  set_as_main: boolean;
}

export interface ProviderSuggestionRequest {
  preferred_provider_id: string;
  preferred_alias_name?: string | null;
  default_model?: string | null;
  editing_provider_id?: string | null;
  editing_alias_name?: string | null;
}

export interface ProviderSuggestionResponse {
  provider_id: string;
  alias_name?: string | null;
  alias_model?: string | null;
  would_be_first_main: boolean;
}

export interface SessionSummary {
  id: string;
  title?: string | null;
  alias: string;
  provider_id: string;
  model: string;
  task_mode?: TaskMode | null;
  message_count: number;
  cwd?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments: string;
}

export interface InputAttachment {
  kind: AttachmentKind;
  path: string;
}

export interface RemoteContentSource {
  kind: string;
  label?: string | null;
  url?: string | null;
  host?: string | null;
}

export interface RemoteContentAssessment {
  risk: "low" | "medium" | "high";
  blocked: boolean;
  reasons: string[];
  warnings: string[];
}

export interface RemoteContentArtifact {
  id?: string;
  source: RemoteContentSource;
  title?: string | null;
  mime_type?: string | null;
  excerpt?: string | null;
  content_sha256?: string | null;
  assessment: RemoteContentAssessment;
}

export interface ProviderOutputItem {
  type: string;
  role?: MessageRole;
  content?: string | null;
  name?: string | null;
  status?: string | null;
}

export interface SessionMessage {
  id: string;
  session_id: string;
  role: MessageRole;
  content: string;
  created_at: string;
  provider_id?: string | null;
  model?: string | null;
  tool_name?: string | null;
  tool_call_id?: string | null;
  tool_calls: ToolCall[];
  attachments?: InputAttachment[];
  provider_payload_json?: string | null;
  provider_output_items?: ProviderOutputItem[];
}

export interface SessionTranscriptHit {
  message_id: string;
  preview: string;
  created_at: string;
  session_id?: string;
  role?: MessageRole;
}

export interface MemoryEvidenceRef {
  session_id?: string | null;
  message_id?: string | null;
  summary: string;
}

export interface MemoryRecord {
  id: string;
  kind: string;
  scope: string;
  subject: string;
  content: string;
  confidence: number;
  identity_key?: string | null;
  observation_source?: string | null;
  created_at: string;
  updated_at: string;
  last_used_at?: string | null;
  source_session_id?: string | null;
  source_message_id?: string | null;
  provider_id?: string | null;
  workspace_key?: string | null;
  evidence_refs: MemoryEvidenceRef[];
  tags: string[];
  superseded_by?: string | null;
  review_status: "candidate" | "accepted" | "rejected";
  review_note?: string | null;
  reviewed_at?: string | null;
  supersedes?: string | null;
}

export interface SessionTranscript {
  session: SessionSummary;
  messages: SessionMessage[];
}

export interface SessionResumePacket {
  session: SessionSummary;
  generated_at: string;
  recent_messages: SessionMessage[];
  linked_memories: MemoryRecord[];
  related_transcript_hits: SessionTranscriptHit[];
}

export interface LogEntry {
  id: string;
  level: string;
  scope: string;
  message: string;
  created_at: string;
}

export interface ProviderCapabilitySummary {
  provider_id: string;
  model: string;
  capabilities: Record<string, boolean>;
}

export interface ModelDescriptor {
  id: string;
  display_name?: string | null;
  description?: string | null;
  context_window?: number | null;
  supports_parallel_tool_calls?: boolean;
  capabilities?: Record<string, boolean>;
}

export interface ProviderReadinessResult {
  ok: boolean;
  model: string;
  detail: string;
}

export interface ProviderDiscoveryResponse {
  models: ModelDescriptor[];
  recommended_model?: string | null;
  warnings: string[];
  readiness?: ProviderReadinessResult | null;
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

export interface AppConnectorConfig {
  id: string;
  name: string;
  description: string;
  command: string;
  args: string[];
  tool_name: string;
  input_schema_json: string;
  enabled?: boolean;
  cwd?: string | null;
}

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
  allowed_chat_ids: number[];
  allowed_user_ids: number[];
  last_update_id?: number | null;
}

export interface DiscordConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids: string[];
  allowed_channel_ids: string[];
  allowed_user_ids: string[];
}

export interface SlackConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids: string[];
  allowed_channel_ids: string[];
  allowed_user_ids: string[];
}

export interface HomeAssistantConnectorConfig extends ConnectorBase {
  base_url: string;
  access_token_keychain_account?: string | null;
  monitored_entity_ids: string[];
  allowed_service_domains: string[];
  allowed_service_entity_ids: string[];
}

export interface SignalConnectorConfig extends ConnectorBase {
  account: string;
  cli_path?: string | null;
  require_pairing_approval?: boolean;
  monitored_group_ids: string[];
  allowed_group_ids: string[];
  allowed_user_ids: string[];
}

export interface GmailConnectorConfig extends ConnectorBase {
  oauth_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  allowed_sender_addresses: string[];
  label_filter?: string | null;
  last_history_id?: string | null;
}

export interface BraveConnectorConfig extends ConnectorBase {
  api_key_keychain_account?: string | null;
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
  source_event_id?: string | null;
  external_chat_id?: string | null;
  external_chat_display?: string | null;
  external_user_id?: string | null;
  external_user_display?: string | null;
  message_preview?: string | null;
  queued_mission_id?: string | null;
  created_at: string;
  updated_at: string;
  note?: string | null;
}

export interface PluginPermissions {
  shell: boolean;
  network: boolean;
  full_disk: boolean;
}

export interface PluginToolManifest {
  name: string;
  description: string;
}

export interface PluginConnectorManifest {
  id: string;
  kind: ConnectorKind;
  description: string;
}

export interface PluginProviderAdapterManifest {
  id: string;
  description: string;
  default_model?: string | null;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  tools: PluginToolManifest[];
  connectors: PluginConnectorManifest[];
  provider_adapters: PluginProviderAdapterManifest[];
}

export interface InstalledPluginConfig {
  id: string;
  manifest: PluginManifest;
  source_kind: string;
  install_dir: string;
  source_reference: string;
  source_path: string;
  integrity_sha256: string;
  enabled: boolean;
  trusted: boolean;
  granted_permissions: PluginPermissions;
  reviewed_integrity_sha256: string;
  reviewed_at?: string | null;
  pinned: boolean;
  installed_at: string;
  updated_at: string;
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
  source_kind: string;
  declared_permissions: PluginPermissions;
  granted_permissions: PluginPermissions;
  reviewed_at?: string | null;
}

export interface PluginInstallRequest {
  source?: string | null;
  source_path?: string | null;
  enabled?: boolean | null;
  trusted?: boolean | null;
  granted_permissions?: PluginPermissions | null;
  pinned?: boolean;
}

export interface PluginUpdateRequest {
  source?: string | null;
  source_path?: string | null;
}

export interface PluginStateUpdateRequest {
  enabled?: boolean | null;
  trusted?: boolean | null;
  granted_permissions?: PluginPermissions | null;
  pinned?: boolean | null;
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
  watch_fingerprint?: string | null;
  wake_trigger?: string | null;
  wake_at?: string | null;
  scheduled_for_at?: string | null;
  repeat_interval_seconds?: number | null;
  repeat_anchor_at?: string | null;
  last_error?: string | null;
  retries?: number;
  max_retries?: number;
  evolve?: boolean;
}

export interface MissionCheckpoint {
  id: string;
  mission_id: string;
  status: MissionStatus;
  summary: string;
  created_at: string;
  session_id?: string | null;
  phase?: string | null;
  handoff_summary?: string | null;
  response_excerpt?: string | null;
  next_wake_at?: string | null;
  scheduled_for_at?: string | null;
}

export interface MissionControlRequest {
  wake_at?: string | null;
  clear_wake_at?: boolean;
  repeat_interval_seconds?: number | null;
  clear_repeat_interval_seconds?: boolean;
  watch_path?: string | null;
  clear_watch_path?: boolean;
  watch_recursive?: boolean | null;
  clear_session_id?: boolean;
  clear_handoff_summary?: boolean;
  note?: string | null;
}

export interface WorkspacePathStat {
  path: string;
  source_files: number;
}

export interface WorkspaceLanguageStat {
  label: string;
  files: number;
}

export interface WorkspaceFileStat {
  path: string;
  lines: number;
}

export interface WorkspaceInspectResponse {
  requested_path: string;
  workspace_root: string;
  git_root?: string | null;
  git_branch?: string | null;
  git_commit?: string | null;
  staged_files: number;
  dirty_files: number;
  untracked_files: number;
  manifests: string[];
  focus_paths: WorkspacePathStat[];
  language_breakdown: WorkspaceLanguageStat[];
  large_source_files: WorkspaceFileStat[];
  recent_commits: string[];
  notes: string[];
}

export interface TrustPolicy {
  trusted_paths: string[];
  allow_shell: boolean;
  allow_network: boolean;
  allow_full_disk: boolean;
  allow_self_edit: boolean;
}

export interface HealthReport {
  daemon_running: boolean;
  config_path: string;
  data_path: string;
  keyring_ok: boolean;
  providers: Array<{ id: string; ok: boolean; detail: string }>;
  plugins: PluginDoctorReport[];
  remote_content_policy: RemoteContentPolicy;
  provider_capabilities: ProviderCapabilitySummary[];
}

export interface DashboardBootstrapResponse {
  status: DaemonStatus;
  providers: ProviderConfig[];
  aliases: ModelAlias[];
  delegation_targets: Array<{ alias: string; provider_id: string; model: string }>;
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
  remote_content_policy: RemoteContentPolicy;
}

export interface RunTaskRequest {
  prompt: string;
  alias?: string | null;
  requested_model?: string | null;
  session_id?: string | null;
  cwd?: string | null;
  attachments?: InputAttachment[];
  task_mode?: TaskMode | null;
  permission_preset?: PermissionPreset | null;
  remote_content_policy_override?: RemoteContentPolicy | null;
  ephemeral?: boolean;
}

export interface ToolExecutionRecord {
  call_id: string;
  name: string;
  arguments: string;
  outcome: "success" | "error";
  output: string;
}

export interface RunTaskResponse {
  session_id: string;
  alias: string;
  provider_id: string;
  model: string;
  response: string;
  tool_events: ToolExecutionRecord[];
}

export type RunTaskStreamEvent =
  | {
      type: "session_started";
      session_id: string;
      alias: string;
      provider_id: string;
      model: string;
    }
  | {
      type: "message";
      message: SessionMessage;
    }
  | {
      type: "remote_content";
      artifact: RemoteContentArtifact;
    }
  | {
      type: "completed";
      response: RunTaskResponse;
    }
  | {
      type: "error";
      message: string;
    };

export interface DashboardSessionRequest {
  token: string;
}

export interface BrowserProviderAuthStartRequest {
  kind: BrowserProviderAuthKind;
  provider_id: string;
  display_name: string;
  default_model?: string | null;
  alias_name?: string | null;
  alias_model?: string | null;
  alias_description?: string | null;
  set_as_main?: boolean;
}

export interface BrowserProviderAuthStartResponse {
  session_id: string;
  status: BrowserProviderAuthSessionStatus;
  authorization_url?: string | null;
}

export interface BrowserProviderAuthStatusResponse {
  session_id: string;
  kind: BrowserProviderAuthKind;
  provider_id: string;
  display_name: string;
  status: BrowserProviderAuthSessionStatus;
  error?: string | null;
}

export interface TrustUpdateRequest {
  trusted_path?: string | null;
  allow_shell?: boolean | null;
  allow_network?: boolean | null;
  allow_full_disk?: boolean | null;
  allow_self_edit?: boolean | null;
}

export interface PermissionUpdateRequest {
  permission_preset: PermissionPreset;
}

export interface DaemonConfigUpdateRequest {
  persistence_mode?: string | null;
  auto_start?: boolean | null;
}

export interface AutonomyEnableRequest {
  mode?: string | null;
  allow_self_edit?: boolean | null;
}

export interface AutopilotUpdateRequest {
  state?: string | null;
  max_concurrent_missions?: number | null;
  wake_interval_seconds?: number | null;
  allow_background_shell?: boolean | null;
  allow_background_network?: boolean | null;
  allow_background_self_edit?: boolean | null;
}

export interface McpServerConfig {
  id: string;
  name: string;
  description: string;
  command: string;
  args: string[];
  tool_name: string;
  input_schema_json: string;
  enabled?: boolean;
  cwd?: string | null;
}

export interface McpServerUpsertRequest {
  server: McpServerConfig;
}

export interface MemorySearchQuery {
  query: string;
  workspace_key?: string | null;
  provider_id?: string | null;
  review_statuses?: string[];
  include_superseded?: boolean;
  limit?: number | null;
}

export interface MemorySearchResponse {
  memories: MemoryRecord[];
  transcript_hits: SessionTranscriptHit[];
}

export interface MemoryUpsertRequest {
  kind: string;
  scope: string;
  subject: string;
  content: string;
  confidence?: number | null;
  source_session_id?: string | null;
  source_message_id?: string | null;
  provider_id?: string | null;
  workspace_key?: string | null;
  evidence_refs?: MemoryEvidenceRef[];
  tags?: string[];
  identity_key?: string | null;
  observation_source?: string | null;
  review_status?: "candidate" | "accepted" | "rejected";
  review_note?: string | null;
  reviewed_at?: string | null;
  supersedes?: string | null;
}

export interface MemoryReviewUpdateRequest {
  status: "accepted" | "rejected";
  note?: string | null;
}

export interface MemoryRebuildRequest {
  session_id?: string | null;
  recompute_embeddings?: boolean;
}

export interface MemoryRebuildResponse {
  generated_at: string;
  session_id?: string | null;
  sessions_scanned: number;
  observations_scanned: number;
  memories_upserted: number;
  embeddings_refreshed: number;
}

export interface SessionRenameRequest {
  title: string;
}

export interface SessionForkRequest {
  target_session_id?: string | null;
}

export interface SessionCompactRequest {
  alias?: string | null;
  requested_model?: string | null;
  cwd?: string | null;
  task_mode?: TaskMode | null;
}

export interface SessionMutationResponse {
  session: SessionSummary;
  messages: SessionMessage[];
}

export interface WorkspaceInspectRequest {
  path?: string | null;
}

export interface WorkspaceActionRequest {
  cwd?: string | null;
}

export interface WorkspaceDiffResponse {
  cwd: string;
  git_root: string;
  diff: string;
}

export interface WorkspaceInitResponse {
  cwd: string;
  path: string;
  created: boolean;
}

export interface WorkspaceShellResponse {
  cwd: string;
  output: string;
}
