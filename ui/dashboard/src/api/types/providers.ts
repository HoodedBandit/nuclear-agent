import type {
  AuthMode,
  BrowserProviderAuthSessionStatus,
  OAuthConfig,
  ProviderKind
} from "./primitives";

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
