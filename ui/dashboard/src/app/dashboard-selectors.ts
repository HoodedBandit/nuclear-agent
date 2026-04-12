import type {
  DashboardBootstrapResponse,
  DelegationConfig,
  DiscordConnectorConfig,
  GmailConnectorConfig,
  HomeAssistantConnectorConfig,
  InboxConnectorConfig,
  LogEntry,
  ModelAlias,
  PermissionPreset,
  ProviderConfig,
  SessionSummary,
  DaemonStatus,
  SlackConnectorConfig,
  SignalConnectorConfig,
  TelegramConnectorConfig,
  TrustPolicy,
  WebhookConnectorConfig,
  BraveConnectorConfig,
  InstalledPluginConfig
} from "../api/types";
import { useDashboardData } from "./dashboard-data";

function useBootstrapSlice<T>(
  selector: (bootstrap: DashboardBootstrapResponse) => T
): T {
  return selector(useDashboardData().bootstrap);
}

export function useShellBootstrap(): {
  status: DaemonStatus;
  sessions: SessionSummary[];
  events: LogEntry[];
  permissions: PermissionPreset;
  trust: TrustPolicy;
  delegationConfig: DelegationConfig;
} {
  return useBootstrapSlice((bootstrap) => ({
    status: bootstrap.status,
    sessions: bootstrap.sessions,
    events: bootstrap.events,
    permissions: bootstrap.permissions,
    trust: bootstrap.trust,
    delegationConfig: bootstrap.delegation_config
  }));
}

export function useOverviewBootstrap(): {
  status: DaemonStatus;
  sessions: SessionSummary[];
  events: LogEntry[];
} {
  return useBootstrapSlice((bootstrap) => ({
    status: bootstrap.status,
    sessions: bootstrap.sessions,
    events: bootstrap.events
  }));
}

export function useChatBootstrap(): {
  aliases: ModelAlias[];
  sessions: SessionSummary[];
  mainAgentAlias: string | null;
} {
  return useBootstrapSlice((bootstrap) => ({
    aliases: bootstrap.aliases,
    sessions: bootstrap.sessions,
    mainAgentAlias: bootstrap.status.main_agent_alias ?? null
  }));
}

export function useProviderBootstrap(): {
  providers: ProviderConfig[];
  aliases: ModelAlias[];
} {
  return useBootstrapSlice((bootstrap) => ({
    providers: bootstrap.providers,
    aliases: bootstrap.aliases
  }));
}

export function useConnectorBootstrap(): {
  webhookConnectors: WebhookConnectorConfig[];
  inboxConnectors: InboxConnectorConfig[];
  telegramConnectors: TelegramConnectorConfig[];
  discordConnectors: DiscordConnectorConfig[];
  slackConnectors: SlackConnectorConfig[];
  signalConnectors: SignalConnectorConfig[];
  homeAssistantConnectors: HomeAssistantConnectorConfig[];
  gmailConnectors: GmailConnectorConfig[];
  braveConnectors: BraveConnectorConfig[];
} {
  return useBootstrapSlice((bootstrap) => ({
    webhookConnectors: bootstrap.webhook_connectors,
    inboxConnectors: bootstrap.inbox_connectors,
    telegramConnectors: bootstrap.telegram_connectors,
    discordConnectors: bootstrap.discord_connectors,
    slackConnectors: bootstrap.slack_connectors,
    signalConnectors: bootstrap.signal_connectors,
    homeAssistantConnectors: bootstrap.home_assistant_connectors,
    gmailConnectors: bootstrap.gmail_connectors,
    braveConnectors: bootstrap.brave_connectors
  }));
}

export function useDelegationBootstrap(): {
  delegationConfig: DelegationConfig;
} {
  return useBootstrapSlice((bootstrap) => ({
    delegationConfig: bootstrap.delegation_config
  }));
}

export function usePluginBootstrap(): {
  plugins: InstalledPluginConfig[];
} {
  return useBootstrapSlice((bootstrap) => ({
    plugins: bootstrap.plugins
  }));
}

export function useSystemBootstrap(): {
  status: DaemonStatus;
  permissions: PermissionPreset;
  trust: TrustPolicy;
} {
  return useBootstrapSlice((bootstrap) => ({
    status: bootstrap.status,
    permissions: bootstrap.permissions,
    trust: bootstrap.trust
  }));
}
