import type {
  BraveConnectorConfig,
  DiscordConnectorConfig,
  GmailConnectorConfig,
  HomeAssistantConnectorConfig,
  InboxConnectorConfig,
  SignalConnectorConfig,
  SlackConnectorConfig,
  TelegramConnectorConfig,
  WebhookConnectorConfig
} from "./connectors";
import type { PermissionPreset, RemoteContentPolicy } from "./primitives";
import type { InstalledPluginConfig } from "./plugins";
import type {
  DelegationConfig,
  DelegationTarget,
  ModelAlias,
  ProviderCapabilitySummary,
  ProviderConfig
} from "./providers";
import type { SessionSummary } from "./sessions";
import type { DaemonStatus, LogEntry, TrustPolicy } from "./system";

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
  remote_content_policy: RemoteContentPolicy | string;
}
