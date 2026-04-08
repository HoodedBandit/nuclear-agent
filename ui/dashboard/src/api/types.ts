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

export interface MainTargetSummary {
  alias: string;
  provider_id: string;
  model: string;
  description?: string | null;
}

export interface DaemonStatus {
  pid: number;
  started_at: string;
  persistence_mode: string;
  auto_start: boolean;
  main_agent_alias?: string | null;
  main_target?: MainTargetSummary | null;
  onboarding_complete: boolean;
  autonomy: {
    state: string;
    mode: string;
    unlimited_usage: boolean;
    full_network: boolean;
    allow_self_edit: boolean;
    consented_at?: string | null;
  };
  evolve: {
    state: string;
  };
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

export interface RemoteContentArtifact {
  source: {
    kind: string;
    label?: string | null;
    url?: string | null;
  };
  risk: "low" | "medium" | "high";
  policy: RemoteContentPolicy;
  allowed: boolean;
  reasons: string[];
  excerpt?: string | null;
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
  provider_output_items?: ProviderOutputItem[];
}

export interface SessionTranscript {
  session: SessionSummary;
  messages: SessionMessage[];
}

export interface SessionResumePacket {
  session: SessionSummary;
  generated_at: string;
  recent_messages: SessionMessage[];
  related_transcript_hits: Array<{
    message_id: string;
    preview: string;
    created_at: string;
  }>;
}

export interface LogEntry {
  id: string;
  level: string;
  scope: string;
  message: string;
  created_at: string;
}

export interface ConnectorSummary {
  id: string;
  name: string;
  enabled?: boolean;
  alias?: string | null;
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

export interface DashboardBootstrapResponse {
  status: DaemonStatus;
  providers: ProviderConfig[];
  aliases: ModelAlias[];
  delegation_targets: Array<{ alias: string; provider_id: string; model: string }>;
  telegram_connectors: ConnectorSummary[];
  discord_connectors: ConnectorSummary[];
  slack_connectors: ConnectorSummary[];
  signal_connectors: ConnectorSummary[];
  home_assistant_connectors: ConnectorSummary[];
  webhook_connectors: ConnectorSummary[];
  inbox_connectors: ConnectorSummary[];
  gmail_connectors: ConnectorSummary[];
  brave_connectors: ConnectorSummary[];
  plugins: Array<{ id: string; name: string; enabled?: boolean }>;
  sessions: SessionSummary[];
  events: LogEntry[];
  permissions: PermissionPreset;
  trust: {
    trusted_paths: string[];
    allow_shell: boolean;
    allow_network: boolean;
    allow_full_disk: boolean;
    allow_self_edit: boolean;
  };
  delegation_config: {
    max_depth: string;
    max_parallel_subagents: string;
    disabled_provider_ids: string[];
  };
  provider_capabilities: ProviderCapabilitySummary[];
  remote_content_policy: RemoteContentPolicy;
}

export interface RunTaskRequest {
  prompt: string;
  alias?: string | null;
  session_id?: string | null;
  task_mode?: TaskMode | null;
  permission_preset?: PermissionPreset | null;
  remote_content_policy_override?: RemoteContentPolicy | null;
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

export interface DashboardSessionResponse {
  ok: boolean;
}
