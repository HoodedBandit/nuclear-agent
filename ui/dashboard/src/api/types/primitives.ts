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

export interface InputAttachment {
  kind: AttachmentKind;
  path: string;
}
