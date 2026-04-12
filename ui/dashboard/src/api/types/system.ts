import type {
  AutonomyMode,
  AutonomyState,
  AutopilotState,
  EvolveState,
  PersistenceMode
} from "./primitives";
import type {
  DelegationConfig,
  MainTargetSummary,
  ProviderCapabilitySummary
} from "./providers";

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

export interface LogEntry {
  id: string;
  level: string;
  target: string;
  message: string;
  created_at: string;
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

export interface SupportBundleResponse {
  bundle_dir: string;
  generated_at: string;
  files: string[];
}
