import { FormEvent, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import { useDashboardData } from "../../app/useDashboardData";
import {
  clearProviderCredentials,
  deleteAlias,
  deleteAppConnector,
  deleteBraveConnector,
  deleteDiscordConnector,
  deleteGmailConnector,
  deleteHomeAssistantConnector,
  deleteInboxConnector,
  deleteMcpServer,
  deletePlugin,
  deleteProvider,
  deleteSignalConnector,
  deleteSlackConnector,
  deleteTelegramConnector,
  deleteWebhookConnector,
  discoverProvider,
  fetchProviderBrowserAuthStatus,
  installPlugin,
  listAliases,
  listAppConnectors,
  listBraveConnectors,
  listDiscordConnectors,
  listGmailConnectors,
  listHomeAssistantConnectors,
  listInboxConnectors,
  listMcpServers,
  listPluginDoctorReports,
  listPlugins,
  listProviders,
  listSignalConnectors,
  listSlackConnectors,
  listTelegramConnectors,
  listWebhookConnectors,
  saveAlias,
  saveAppConnector,
  saveBraveConnector,
  saveDiscordConnector,
  saveGmailConnector,
  saveHomeAssistantConnector,
  saveInboxConnector,
  saveMcpServer,
  saveProvider,
  saveSignalConnector,
  saveSlackConnector,
  saveTelegramConnector,
  saveWebhookConnector,
  startProviderBrowserAuth,
  updateMainAlias,
  updatePlugin,
  updatePluginState,
  validateProvider
} from "../../api/client";
import type {
  AuthMode,
  ConnectorKind,
  InstalledPluginConfig,
  ModelAlias,
  PluginPermissions,
  ProviderCapabilitySummary,
  ProviderConfig,
  ProviderKind,
  ProviderProfile,
  ProviderReadinessResult,
  ProviderUpsertRequest
} from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { WorkbenchTabs } from "../../components/WorkbenchTabs";
import { startCase } from "../../utils/format";
import shellStyles from "../shared/Workbench.module.css";
import styles from "./IntegrationsPage.module.css";

const INTEGRATIONS_TABS = [
  { id: "providers", label: "Providers", description: "Auth, readiness, aliases, and routing" },
  { id: "connectors", label: "Connectors", description: "External inboxes, bots, hooks, and local apps" },
  { id: "plugins", label: "Plugins", description: "Install, trust, update, and diagnose local extensions" },
  { id: "mcp", label: "MCP", description: "Managed MCP server roster and command wiring" }
] as const;

type IntegrationsTabId = (typeof INTEGRATIONS_TABS)[number]["id"];
type ProviderPresetId =
  | "openai"
  | "codex"
  | "moonshot"
  | "openrouter"
  | "venice"
  | "anthropic"
  | "ollama"
  | "local_openai";

interface ProviderPreset {
  id: ProviderPresetId;
  label: string;
  providerKind: ProviderKind;
  providerProfile: ProviderProfile;
  displayName: string;
  providerId: string;
  baseUrl: string;
  authMode: AuthMode;
  local: boolean;
  apiKeyLabel?: string;
  apiKeyPlaceholder?: string;
  defaultModelPlaceholder: string;
  browserAuthKind?: "codex";
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "openai",
    label: "OpenAI API",
    providerKind: "open_ai_compatible",
    providerProfile: "open_ai",
    displayName: "OpenAI",
    providerId: "openai",
    baseUrl: "https://api.openai.com/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "OpenAI API key",
    apiKeyPlaceholder: "sk-...",
    defaultModelPlaceholder: "gpt-5.4"
  },
  {
    id: "codex",
    label: "ChatGPT / Codex browser session",
    providerKind: "chat_gpt_codex",
    providerProfile: "open_ai",
    displayName: "Codex",
    providerId: "codex",
    baseUrl: "https://chatgpt.com/backend-api/codex",
    authMode: "oauth",
    local: false,
    defaultModelPlaceholder: "gpt-5-codex",
    browserAuthKind: "codex"
  },
  {
    id: "moonshot",
    label: "Moonshot (Kimi)",
    providerKind: "open_ai_compatible",
    providerProfile: "moonshot",
    displayName: "Moonshot",
    providerId: "moonshot",
    baseUrl: "https://api.moonshot.ai/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Moonshot API key",
    apiKeyPlaceholder: "sk-...",
    defaultModelPlaceholder: "kimi-k2.5"
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    providerKind: "open_ai_compatible",
    providerProfile: "open_router",
    displayName: "OpenRouter",
    providerId: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "OpenRouter API key",
    apiKeyPlaceholder: "sk-or-...",
    defaultModelPlaceholder: "openai/gpt-5.4"
  },
  {
    id: "venice",
    label: "Venice",
    providerKind: "open_ai_compatible",
    providerProfile: "venice",
    displayName: "Venice AI",
    providerId: "venice",
    baseUrl: "https://api.venice.ai/api/v1",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Venice API key",
    apiKeyPlaceholder: "venice-...",
    defaultModelPlaceholder: "venice-uncensored"
  },
  {
    id: "anthropic",
    label: "Anthropic",
    providerKind: "anthropic",
    providerProfile: "anthropic",
    displayName: "Anthropic",
    providerId: "anthropic",
    baseUrl: "https://api.anthropic.com",
    authMode: "api_key",
    local: false,
    apiKeyLabel: "Anthropic API key",
    apiKeyPlaceholder: "sk-ant-...",
    defaultModelPlaceholder: "claude-sonnet-4-5"
  },
  {
    id: "ollama",
    label: "Ollama",
    providerKind: "ollama",
    providerProfile: "ollama",
    displayName: "Ollama local",
    providerId: "ollama-local",
    baseUrl: "http://127.0.0.1:11434",
    authMode: "none",
    local: true,
    defaultModelPlaceholder: "qwen2.5-coder:7b"
  },
  {
    id: "local_openai",
    label: "Local OpenAI-compatible",
    providerKind: "open_ai_compatible",
    providerProfile: "local_open_ai_compatible",
    displayName: "Local OpenAI-compatible",
    providerId: "local-openai",
    baseUrl: "http://127.0.0.1:5001/v1",
    authMode: "none",
    local: true,
    defaultModelPlaceholder: "custom-model"
  }
];

interface ProviderFormState {
  presetId: ProviderPresetId;
  id: string;
  display_name: string;
  kind: ProviderKind;
  base_url: string;
  provider_profile: ProviderProfile;
  auth_mode: AuthMode;
  default_model: string;
  api_key: string;
  local: boolean;
  alias_name: string;
  alias_description: string;
  set_as_main: boolean;
}

interface ConnectorFormState {
  kind: ConnectorKind;
  id: string;
  name: string;
  description: string;
  alias: string;
  requested_model: string;
  cwd: string;
  enabled: boolean;
  prompt_template: string;
  webhook_token: string;
  path: string;
  delete_after_read: boolean;
  bot_token: string;
  require_pairing_approval: boolean;
  allowed_chat_ids: string;
  allowed_user_ids: string;
  monitored_channel_ids: string;
  allowed_channel_ids: string;
  monitored_group_ids: string;
  allowed_group_ids: string;
  base_url: string;
  access_token: string;
  monitored_entity_ids: string;
  allowed_service_domains: string;
  allowed_service_entity_ids: string;
  account: string;
  cli_path: string;
  oauth_token: string;
  api_key: string;
  label_filter: string;
  command: string;
  args: string;
  tool_name: string;
  input_schema_json: string;
}

interface PluginInstallFormState {
  source_path: string;
  enabled: boolean;
  trusted: boolean;
  pinned: boolean;
  permissions: PluginPermissions;
}

interface McpFormState {
  id: string;
  name: string;
  description: string;
  command: string;
  args: string;
  tool_name: string;
  input_schema_json: string;
  cwd: string;
  enabled: boolean;
}

function presetById(id: ProviderPresetId): ProviderPreset {
  return PROVIDER_PRESETS.find((preset) => preset.id === id) ?? PROVIDER_PRESETS[0];
}

function providerPresetFromConfig(provider: ProviderConfig): ProviderPresetId {
  if (provider.kind === "chat_gpt_codex") {
    return "codex";
  }
  switch (provider.provider_profile) {
    case "moonshot":
      return "moonshot";
    case "open_router":
      return "openrouter";
    case "venice":
      return "venice";
    case "anthropic":
      return "anthropic";
    case "ollama":
      return "ollama";
    case "local_open_ai_compatible":
      return "local_openai";
    default:
      return "openai";
  }
}

function buildInitialProviderFormState(presetId: ProviderPresetId = "codex"): ProviderFormState {
  const preset = presetById(presetId);
  return {
    presetId: preset.id,
    id: preset.providerId,
    display_name: preset.displayName,
    kind: preset.providerKind,
    base_url: preset.baseUrl,
    provider_profile: preset.providerProfile,
    auth_mode: preset.authMode,
    default_model: "",
    api_key: "",
    local: preset.local,
    alias_name: "",
    alias_description: "",
    set_as_main: false
  };
}

function buildProviderPayload(formState: ProviderFormState): ProviderUpsertRequest {
  return {
    provider: {
      id: formState.id.trim(),
      display_name: formState.display_name.trim(),
      kind: formState.kind,
      base_url: formState.base_url.trim(),
      provider_profile: formState.provider_profile,
      auth_mode: formState.auth_mode,
      default_model: formState.default_model.trim() || null,
      keychain_account: null,
      local: formState.local
    },
    api_key: formState.auth_mode === "api_key" ? formState.api_key : null,
    oauth_token: null
  };
}

function buildProviderFormFromExisting(provider: ProviderConfig, aliases: ModelAlias[]): ProviderFormState {
  const alias = aliases.find((item) => item.provider_id === provider.id);
  return {
    presetId: providerPresetFromConfig(provider),
    id: provider.id,
    display_name: provider.display_name,
    kind: provider.kind,
    base_url: provider.base_url,
    provider_profile: provider.provider_profile ?? "generic_open_ai_compatible",
    auth_mode: provider.auth_mode,
    default_model: provider.default_model ?? "",
    api_key: "",
    local: provider.local,
    alias_name: alias?.alias ?? "",
    alias_description: alias?.description ?? "",
    set_as_main: false
  };
}

function buildInitialConnectorFormState(kind: ConnectorKind = "inbox"): ConnectorFormState {
  return {
    kind,
    id: kind,
    name: startCase(kind),
    description: "",
    alias: "",
    requested_model: "",
    cwd: "",
    enabled: true,
    prompt_template: "Summarize the incoming event and route it to the right task.",
    webhook_token: "",
    path: "",
    delete_after_read: false,
    bot_token: "",
    require_pairing_approval: false,
    allowed_chat_ids: "",
    allowed_user_ids: "",
    monitored_channel_ids: "",
    allowed_channel_ids: "",
    monitored_group_ids: "",
    allowed_group_ids: "",
    base_url: "http://127.0.0.1:8123",
    access_token: "",
    monitored_entity_ids: "",
    allowed_service_domains: "",
    allowed_service_entity_ids: "",
    account: "",
    cli_path: "",
    oauth_token: "",
    api_key: "",
    label_filter: "",
    command: "",
    args: "",
    tool_name: "",
    input_schema_json: "{\n  \"type\": \"object\",\n  \"properties\": {}\n}"
  };
}

function buildInitialPluginFormState(): PluginInstallFormState {
  return {
    source_path: "",
    enabled: true,
    trusted: false,
    pinned: false,
    permissions: {
      shell: false,
      network: false,
      full_disk: false
    }
  };
}

function buildInitialMcpFormState(): McpFormState {
  return {
    id: "local-mcp",
    name: "Local MCP",
    description: "",
    command: "",
    args: "",
    tool_name: "local_mcp",
    input_schema_json: "{\n  \"type\": \"object\",\n  \"properties\": {}\n}",
    cwd: "",
    enabled: true
  };
}

function discoveryKeyForSecret(secret: string): string {
  let hash = 0;
  for (let index = 0; index < secret.length; index += 1) {
    hash = (hash * 31 + secret.charCodeAt(index)) >>> 0;
  }
  return `${secret.length}:${hash.toString(16)}`;
}

function readinessFromError(model: string, error: unknown): ProviderReadinessResult {
  return {
    ok: false,
    model,
    detail: error instanceof Error ? error.message : "Readiness check failed."
  };
}

function parseCsvList(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function parseNumberList(value: string): number[] {
  return value
    .split(",")
    .map((item) => Number(item.trim()))
    .filter((item) => Number.isFinite(item));
}

function joinList(values: string[] | number[] | undefined) {
  return (values ?? []).join(", ");
}

function connectorRecordLabel(kind: ConnectorKind) {
  return startCase(kind);
}

function providerCapabilitiesFor(providerId: string, capabilities: ProviderCapabilitySummary[]) {
  return capabilities.filter((item) => item.provider_id === providerId).slice(0, 4);
}

function connectorSummaryLines(kind: ConnectorKind, item: Record<string, unknown>): string[] {
  const lines: string[] = [];

  const alias = typeof item.alias === "string" && item.alias.trim().length > 0 ? item.alias.trim() : null;
  const requestedModel =
    typeof item.requested_model === "string" && item.requested_model.trim().length > 0
      ? item.requested_model.trim()
      : null;
  const cwd = typeof item.cwd === "string" && item.cwd.trim().length > 0 ? item.cwd.trim() : null;

  if (alias || requestedModel) {
    lines.push([alias ? `Alias ${alias}` : null, requestedModel ? `Model ${requestedModel}` : null].filter(Boolean).join(" · "));
  }

  switch (kind) {
    case "app":
      if (typeof item.command === "string" && item.command.trim().length > 0) {
        lines.push(item.command.trim());
      }
      if (typeof item.tool_name === "string" && item.tool_name.trim().length > 0) {
        lines.push(`Tool ${item.tool_name.trim()}`);
      }
      break;
    case "inbox":
      if (typeof item.path === "string" && item.path.trim().length > 0) {
        lines.push(item.path.trim());
      }
      if (item.delete_after_read === true) {
        lines.push("Deletes files after read");
      }
      break;
    case "webhook":
      if (typeof item.prompt_template === "string" && item.prompt_template.trim().length > 0) {
        lines.push(item.prompt_template.trim());
      }
      break;
    case "home_assistant":
      if (typeof item.base_url === "string" && item.base_url.trim().length > 0) {
        lines.push(item.base_url.trim());
      }
      break;
    case "signal":
      if (typeof item.account === "string" && item.account.trim().length > 0) {
        lines.push(`Account ${item.account.trim()}`);
      }
      if (typeof item.cli_path === "string" && item.cli_path.trim().length > 0) {
        lines.push(item.cli_path.trim());
      }
      break;
    case "gmail":
      if (typeof item.label_filter === "string" && item.label_filter.trim().length > 0) {
        lines.push(`Label filter ${item.label_filter.trim()}`);
      }
      break;
    default:
      break;
  }

  if (cwd) {
    lines.push(`CWD ${cwd}`);
  }

  return lines;
}

export function IntegrationsPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedTab = searchParams.get("tab");
  const requestedConnectorKind = searchParams.get("connector");
  const initialTab = INTEGRATIONS_TABS.some((tab) => tab.id === requestedTab)
    ? (requestedTab as IntegrationsTabId)
    : "providers";
  const [activeTab, setActiveTab] = useState<IntegrationsTabId>(initialTab);
  const [providerForm, setProviderForm] = useState<ProviderFormState>(() => buildInitialProviderFormState());
  const [lastValidated, setLastValidated] = useState<ProviderReadinessResult | null>(null);
  const [connectorForm, setConnectorForm] = useState<ConnectorFormState>(() => buildInitialConnectorFormState());
  const [pluginForm, setPluginForm] = useState<PluginInstallFormState>(() => buildInitialPluginFormState());
  const [mcpForm, setMcpForm] = useState<McpFormState>(() => buildInitialMcpFormState());
  const [browserAuthSessionId, setBrowserAuthSessionId] = useState<string | null>(null);

  useEffect(() => {
    if (requestedTab && INTEGRATIONS_TABS.some((tab) => tab.id === requestedTab) && requestedTab !== activeTab) {
      setActiveTab(requestedTab as IntegrationsTabId);
    }
  }, [activeTab, requestedTab]);

  useEffect(() => {
    if (!requestedConnectorKind) {
      return;
    }
    const supportedKinds = [
      "app",
      "inbox",
      "telegram",
      "discord",
      "slack",
      "home_assistant",
      "signal",
      "gmail",
      "brave",
      "webhook"
    ] as ConnectorKind[];
    if (
      activeTab === "connectors" &&
      supportedKinds.includes(requestedConnectorKind as ConnectorKind) &&
      connectorForm.kind !== requestedConnectorKind
    ) {
      setConnectorForm(buildInitialConnectorFormState(requestedConnectorKind as ConnectorKind));
    }
  }, [activeTab, connectorForm.kind, requestedConnectorKind]);

  const providersQuery = useQuery({ queryKey: ["providers"], queryFn: listProviders, initialData: bootstrap.providers });
  const aliasesQuery = useQuery({ queryKey: ["aliases"], queryFn: listAliases, initialData: bootstrap.aliases });
  const appConnectorsQuery = useQuery({ queryKey: ["connectors", "app"], queryFn: listAppConnectors, initialData: [] });
  const inboxConnectorsQuery = useQuery({
    queryKey: ["connectors", "inbox"],
    queryFn: listInboxConnectors,
    initialData: bootstrap.inbox_connectors
  });
  const telegramConnectorsQuery = useQuery({
    queryKey: ["connectors", "telegram"],
    queryFn: listTelegramConnectors,
    initialData: bootstrap.telegram_connectors
  });
  const discordConnectorsQuery = useQuery({
    queryKey: ["connectors", "discord"],
    queryFn: listDiscordConnectors,
    initialData: bootstrap.discord_connectors
  });
  const slackConnectorsQuery = useQuery({
    queryKey: ["connectors", "slack"],
    queryFn: listSlackConnectors,
    initialData: bootstrap.slack_connectors
  });
  const homeAssistantConnectorsQuery = useQuery({
    queryKey: ["connectors", "home-assistant"],
    queryFn: listHomeAssistantConnectors,
    initialData: bootstrap.home_assistant_connectors
  });
  const signalConnectorsQuery = useQuery({
    queryKey: ["connectors", "signal"],
    queryFn: listSignalConnectors,
    initialData: bootstrap.signal_connectors
  });
  const gmailConnectorsQuery = useQuery({
    queryKey: ["connectors", "gmail"],
    queryFn: listGmailConnectors,
    initialData: bootstrap.gmail_connectors
  });
  const braveConnectorsQuery = useQuery({
    queryKey: ["connectors", "brave"],
    queryFn: listBraveConnectors,
    initialData: bootstrap.brave_connectors
  });
  const webhookConnectorsQuery = useQuery({
    queryKey: ["connectors", "webhook"],
    queryFn: listWebhookConnectors,
    initialData: bootstrap.webhook_connectors
  });
  const pluginsQuery = useQuery({ queryKey: ["plugins"], queryFn: listPlugins, initialData: bootstrap.plugins });
  const pluginDoctorQuery = useQuery({ queryKey: ["plugins", "doctor"], queryFn: listPluginDoctorReports });
  const mcpQuery = useQuery({ queryKey: ["mcp"], queryFn: listMcpServers, initialData: [] });
  const browserAuthStatusQuery = useQuery({
    queryKey: ["browser-auth-status", browserAuthSessionId],
    queryFn: () => fetchProviderBrowserAuthStatus(browserAuthSessionId!),
    enabled: browserAuthSessionId !== null,
    refetchInterval: (query) => (query.state.data?.status === "pending" ? 2_000 : false)
  });

  const selectedPreset = useMemo(() => presetById(providerForm.presetId), [providerForm.presetId]);
  const providerPayload = useMemo(() => buildProviderPayload(providerForm), [providerForm]);
  const secretDiscoveryKey = useMemo(() => discoveryKeyForSecret(providerForm.api_key), [providerForm.api_key]);
  const canDiscoverModels =
    providerPayload.provider.id.length > 0 &&
    providerPayload.provider.base_url.length > 0 &&
    (providerPayload.provider.auth_mode !== "api_key" || (providerPayload.api_key ?? "").length > 0);

  const modelDiscoveryQuery = useQuery({
    queryKey: [
      "provider-discovery",
      providerPayload.provider.provider_profile ?? "",
      providerPayload.provider.kind,
      providerPayload.provider.id,
      providerPayload.provider.base_url,
      providerPayload.provider.auth_mode,
      secretDiscoveryKey
    ],
    queryFn: async () => discoverProvider(providerPayload),
    enabled: canDiscoverModels,
    retry: false,
    staleTime: 30_000
  });

  useEffect(() => {
    const recommendedModel = modelDiscoveryQuery.data?.recommended_model?.trim();
    if (!recommendedModel || providerForm.default_model.trim().length > 0) {
      return;
    }
    setProviderForm((current) => ({ ...current, default_model: recommendedModel }));
  }, [modelDiscoveryQuery.data?.recommended_model, providerForm.default_model]);

  useEffect(() => {
    if (browserAuthStatusQuery.data?.status === "completed") {
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: ["aliases"] })
      ]);
    }
  }, [browserAuthStatusQuery.data?.status, queryClient]);

  useEffect(() => {
    setLastValidated(null);
  }, [providerForm]);

  const connectorRoster = useMemo(
    () => [
      { kind: "app" as const, label: "Apps", items: appConnectorsQuery.data ?? [] },
      { kind: "inbox" as const, label: "Inbox", items: inboxConnectorsQuery.data ?? [] },
      { kind: "telegram" as const, label: "Telegram", items: telegramConnectorsQuery.data ?? [] },
      { kind: "discord" as const, label: "Discord", items: discordConnectorsQuery.data ?? [] },
      { kind: "slack" as const, label: "Slack", items: slackConnectorsQuery.data ?? [] },
      { kind: "home_assistant" as const, label: "Home Assistant", items: homeAssistantConnectorsQuery.data ?? [] },
      { kind: "signal" as const, label: "Signal", items: signalConnectorsQuery.data ?? [] },
      { kind: "gmail" as const, label: "Gmail", items: gmailConnectorsQuery.data ?? [] },
      { kind: "brave" as const, label: "Brave", items: braveConnectorsQuery.data ?? [] },
      { kind: "webhook" as const, label: "Webhook", items: webhookConnectorsQuery.data ?? [] }
    ],
    [
      appConnectorsQuery.data,
      inboxConnectorsQuery.data,
      telegramConnectorsQuery.data,
      discordConnectorsQuery.data,
      slackConnectorsQuery.data,
      homeAssistantConnectorsQuery.data,
      signalConnectorsQuery.data,
      gmailConnectorsQuery.data,
      braveConnectorsQuery.data,
      webhookConnectorsQuery.data
    ]
  );

  const saveProviderMutation = useMutation({
    mutationFn: async () => {
      const savedProvider = await saveProvider(providerPayload);
      const aliasName = providerForm.alias_name.trim();
      if (aliasName) {
        await saveAlias({
          alias: {
            alias: aliasName,
            provider_id: savedProvider.id,
            model: providerForm.default_model.trim() || savedProvider.default_model || "",
            description: providerForm.alias_description.trim() || null
          },
          set_as_main: providerForm.set_as_main
        });
      } else if (providerForm.set_as_main) {
        const matchingAlias = aliasesQuery.data?.find((alias) => alias.provider_id === savedProvider.id);
        if (matchingAlias) {
          await updateMainAlias(matchingAlias.alias);
        }
      }

      const readinessModel = providerForm.default_model.trim() || savedProvider.default_model || "";
      try {
        const readiness = await validateProvider(providerPayload);
        setLastValidated(readiness);
      } catch (error) {
        setLastValidated(readinessFromError(readinessModel, error));
      }

      return savedProvider;
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: ["aliases"] })
      ]);
      setProviderForm(buildInitialProviderFormState(providerForm.presetId));
    }
  });

  const validateProviderMutation = useMutation({
    mutationFn: async () => validateProvider(providerPayload),
    onSuccess: (result) => setLastValidated(result)
  });

  const providerActionMutation = useMutation({
    mutationFn: async (action: { type: "delete" | "clear"; providerId: string }) => {
      if (action.type === "delete") {
        await deleteProvider(action.providerId);
      } else {
        await clearProviderCredentials(action.providerId);
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: ["aliases"] })
      ]);
    }
  });

  const aliasActionMutation = useMutation({
    mutationFn: async (action: { type: "main" | "delete"; alias: string }) => {
      if (action.type === "main") {
        await updateMainAlias(action.alias);
      } else {
        await deleteAlias(action.alias);
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["aliases"] })
      ]);
    }
  });

  const browserAuthMutation = useMutation({
    mutationFn: async () => {
      const preset = presetById(providerForm.presetId);
      if (!preset.browserAuthKind) {
        throw new Error("Browser auth is not available for this provider preset.");
      }
      return startProviderBrowserAuth({
        kind: preset.browserAuthKind,
        provider_id: providerForm.id.trim() || preset.providerId,
        display_name: providerForm.display_name.trim() || preset.displayName,
        default_model: providerForm.default_model.trim() || null,
        alias_name: providerForm.alias_name.trim() || null,
        alias_model: providerForm.default_model.trim() || null,
        alias_description: providerForm.alias_description.trim() || null,
        set_as_main: providerForm.set_as_main
      });
    },
    onSuccess: (response) => {
      setBrowserAuthSessionId(response.session_id);
      if (response.authorization_url) {
        window.open(response.authorization_url, "_blank", "noopener,noreferrer");
      }
    }
  });

  const connectorMutation = useMutation({
    mutationFn: async () => {
      switch (connectorForm.kind) {
        case "app":
          return saveAppConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              command: connectorForm.command.trim(),
              args: parseCsvList(connectorForm.args),
              tool_name: connectorForm.tool_name.trim(),
              input_schema_json: connectorForm.input_schema_json,
              enabled: connectorForm.enabled,
              cwd: connectorForm.cwd.trim() || null
            }
          });
        case "webhook":
          return saveWebhookConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              prompt_template: connectorForm.prompt_template.trim()
            },
            webhook_token: connectorForm.webhook_token.trim() || null
          });
        case "inbox":
          return saveInboxConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              path: connectorForm.path.trim(),
              delete_after_read: connectorForm.delete_after_read
            }
          });
        case "telegram":
          return saveTelegramConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              require_pairing_approval: connectorForm.require_pairing_approval,
              allowed_chat_ids: parseNumberList(connectorForm.allowed_chat_ids),
              allowed_user_ids: parseNumberList(connectorForm.allowed_user_ids)
            },
            bot_token: connectorForm.bot_token.trim() || null
          });
        case "discord":
          return saveDiscordConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              require_pairing_approval: connectorForm.require_pairing_approval,
              monitored_channel_ids: parseCsvList(connectorForm.monitored_channel_ids),
              allowed_channel_ids: parseCsvList(connectorForm.allowed_channel_ids),
              allowed_user_ids: parseCsvList(connectorForm.allowed_user_ids)
            },
            bot_token: connectorForm.bot_token.trim() || null
          });
        case "slack":
          return saveSlackConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              require_pairing_approval: connectorForm.require_pairing_approval,
              monitored_channel_ids: parseCsvList(connectorForm.monitored_channel_ids),
              allowed_channel_ids: parseCsvList(connectorForm.allowed_channel_ids),
              allowed_user_ids: parseCsvList(connectorForm.allowed_user_ids)
            },
            bot_token: connectorForm.bot_token.trim() || null
          });
        case "home_assistant":
          return saveHomeAssistantConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              base_url: connectorForm.base_url.trim(),
              monitored_entity_ids: parseCsvList(connectorForm.monitored_entity_ids),
              allowed_service_domains: parseCsvList(connectorForm.allowed_service_domains),
              allowed_service_entity_ids: parseCsvList(connectorForm.allowed_service_entity_ids)
            },
            access_token: connectorForm.access_token.trim() || null
          });
        case "signal":
          return saveSignalConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              account: connectorForm.account.trim(),
              cli_path: connectorForm.cli_path.trim() || null,
              require_pairing_approval: connectorForm.require_pairing_approval,
              monitored_group_ids: parseCsvList(connectorForm.monitored_group_ids),
              allowed_group_ids: parseCsvList(connectorForm.allowed_group_ids),
              allowed_user_ids: parseCsvList(connectorForm.allowed_user_ids)
            }
          });
        case "gmail":
          return saveGmailConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled,
              require_pairing_approval: connectorForm.require_pairing_approval,
              allowed_sender_addresses: parseCsvList(connectorForm.allowed_user_ids),
              label_filter: connectorForm.label_filter.trim() || null,
              last_history_id: null
            },
            oauth_token: connectorForm.oauth_token.trim() || null
          });
        case "brave":
          return saveBraveConnector({
            connector: {
              id: connectorForm.id.trim(),
              name: connectorForm.name.trim(),
              description: connectorForm.description.trim(),
              alias: connectorForm.alias.trim() || null,
              requested_model: connectorForm.requested_model.trim() || null,
              cwd: connectorForm.cwd.trim() || null,
              enabled: connectorForm.enabled
            },
            api_key: connectorForm.api_key.trim() || null
          });
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["connectors"] })
      ]);
      setConnectorForm(buildInitialConnectorFormState(connectorForm.kind));
    }
  });

  const connectorDeleteMutation = useMutation({
    mutationFn: async (action: { kind: ConnectorKind; id: string }) => {
      switch (action.kind) {
        case "app":
          return deleteAppConnector(action.id);
        case "webhook":
          return deleteWebhookConnector(action.id);
        case "inbox":
          return deleteInboxConnector(action.id);
        case "telegram":
          return deleteTelegramConnector(action.id);
        case "discord":
          return deleteDiscordConnector(action.id);
        case "slack":
          return deleteSlackConnector(action.id);
        case "home_assistant":
          return deleteHomeAssistantConnector(action.id);
        case "signal":
          return deleteSignalConnector(action.id);
        case "gmail":
          return deleteGmailConnector(action.id);
        case "brave":
          return deleteBraveConnector(action.id);
      }
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["connectors"] })
      ]);
    }
  });

  const pluginInstallMutation = useMutation({
    mutationFn: async () =>
      installPlugin({
        source_path: pluginForm.source_path.trim() || null,
        enabled: pluginForm.enabled,
        trusted: pluginForm.trusted,
        pinned: pluginForm.pinned,
        granted_permissions: pluginForm.permissions
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["plugins"] }),
        queryClient.invalidateQueries({ queryKey: ["plugins", "doctor"] })
      ]);
      setPluginForm(buildInitialPluginFormState());
    }
  });

  const pluginActionMutation = useMutation({
    mutationFn: async (
      action:
        | { kind: "toggle-enabled"; plugin: InstalledPluginConfig }
        | { kind: "toggle-trusted"; plugin: InstalledPluginConfig }
        | { kind: "update"; plugin: InstalledPluginConfig }
        | { kind: "remove"; plugin: InstalledPluginConfig }
    ) => {
      if (action.kind === "toggle-enabled") {
        return updatePluginState(action.plugin.id, { enabled: !action.plugin.enabled });
      }
      if (action.kind === "toggle-trusted") {
        return updatePluginState(action.plugin.id, { trusted: !action.plugin.trusted });
      }
      if (action.kind === "update") {
        return updatePlugin(action.plugin.id, { source_path: action.plugin.source_path });
      }
      return deletePlugin(action.plugin.id);
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["plugins"] }),
        queryClient.invalidateQueries({ queryKey: ["plugins", "doctor"] })
      ]);
    }
  });

  const mcpMutation = useMutation({
    mutationFn: async () =>
      saveMcpServer({
        server: {
          id: mcpForm.id.trim(),
          name: mcpForm.name.trim(),
          description: mcpForm.description.trim(),
          command: mcpForm.command.trim(),
          args: parseCsvList(mcpForm.args),
          tool_name: mcpForm.tool_name.trim(),
          input_schema_json: mcpForm.input_schema_json,
          enabled: mcpForm.enabled,
          cwd: mcpForm.cwd.trim() || null
        }
      }),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["mcp"] });
      setMcpForm(buildInitialMcpFormState());
    }
  });

  const mcpDeleteMutation = useMutation({
    mutationFn: async (serverId: string) => deleteMcpServer(serverId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["mcp"] });
    }
  });

  const discoveredModels = modelDiscoveryQuery.data?.models ?? [];
  const browserAuthStatus = browserAuthStatusQuery.data?.status ?? null;

  function handleProviderSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void saveProviderMutation.mutateAsync();
  }

  function updateIntegrationRoute(tabId: IntegrationsTabId, connectorKind?: ConnectorKind | null) {
    setActiveTab(tabId);
    const next = new URLSearchParams(searchParams);
    next.set("tab", tabId);
    if (tabId === "connectors") {
      const nextConnectorKind = connectorKind ?? connectorForm.kind;
      next.set("connector", nextConnectorKind);
    } else {
      next.delete("connector");
    }
    setSearchParams(next, { replace: true });
  }

  function handleConnectorSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void connectorMutation.mutateAsync();
  }

  function handlePluginSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void pluginInstallMutation.mutateAsync();
  }

  function handleMcpSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void mcpMutation.mutateAsync();
  }

  return (
    <div className={shellStyles.page} data-testid="modern-integrations-page">
      <section className={shellStyles.hero}>
        <div className={shellStyles.heroBlock}>
          <div className={shellStyles.heroEyebrow}>Integrations</div>
          <h2 className={shellStyles.heroTitle}>Providers, connectors, plugins, and MCP belong in one operator workbench.</h2>
          <p className={shellStyles.heroCopy}>
            Configure the runtime edge of the product without bouncing to a legacy dashboard.
            Auth, readiness, connectors, extensions, and managed servers all live here now.
          </p>
        </div>
        <div className={shellStyles.heroActions}>
          <Pill tone="accent">{providersQuery.data?.length ?? 0} providers</Pill>
          <Pill tone="neutral">
            {connectorRoster.reduce((sum, section) => sum + section.items.length, 0)} connectors
          </Pill>
          <Pill tone="good">{pluginsQuery.data?.length ?? 0} plugins</Pill>
          <Pill tone="neutral">{mcpQuery.data?.length ?? 0} MCP</Pill>
        </div>
      </section>

      <WorkbenchTabs
        tabs={INTEGRATIONS_TABS.map((tab) => ({ ...tab }))}
        activeTab={activeTab}
        onChange={(tabId) => updateIntegrationRoute(tabId as IntegrationsTabId)}
        testIdPrefix="modern-integrations-tab"
      />

      {activeTab === "providers" ? (
        <div className={shellStyles.gridTwo}>
          <div className={shellStyles.stack}>
            <Surface eyebrow="Configured providers" title="Runtime roster" emphasis="accent">
              {providersQuery.data && providersQuery.data.length > 0 ? (
                <div className={shellStyles.list} id="providers-list">
                  {providersQuery.data.map((provider) => {
                    const capabilityRows = providerCapabilitiesFor(provider.id, bootstrap.provider_capabilities);
                    return (
                      <article key={provider.id} className={shellStyles.listCard}>
                        <div className={styles.cardHeader}>
                          <div>
                            <strong>{provider.display_name}</strong>
                            <div className={shellStyles.meta}>{provider.id}</div>
                            <div className={shellStyles.meta}>{provider.base_url}</div>
                          </div>
                          <div className={shellStyles.pillRow}>
                            <Pill tone={provider.local ? "good" : "accent"}>
                              {provider.local ? "Local" : "Hosted"}
                            </Pill>
                            <Pill tone="neutral">{startCase(provider.auth_mode)}</Pill>
                          </div>
                        </div>

                        <div className={shellStyles.meta}>
                          Default model: {provider.default_model ?? "No default model selected"}
                        </div>

                        {capabilityRows.length > 0 ? (
                          <div className={styles.capabilityList}>
                            {capabilityRows.map((item) => (
                              <div key={`${item.provider_id}-${item.model}`} className={styles.capabilityCard}>
                                <strong>{item.model}</strong>
                                <div className={shellStyles.pillRow}>
                                  {Object.entries(item.capabilities)
                                    .filter(([, enabled]) => enabled)
                                    .slice(0, 5)
                                    .map(([capability]) => (
                                      <Pill key={capability} tone="neutral">
                                        {capability.replace(/_/g, " ")}
                                      </Pill>
                                    ))}
                                </div>
                              </div>
                            ))}
                          </div>
                        ) : null}

                        <div className={shellStyles.buttonRow}>
                          <button
                            type="button"
                            className={shellStyles.secondaryButton}
                            onClick={() => setProviderForm(buildProviderFormFromExisting(provider, aliasesQuery.data ?? []))}
                          >
                            Edit
                          </button>
                          {provider.auth_mode !== "none" ? (
                            <button
                              type="button"
                              className={shellStyles.secondaryButton}
                              onClick={() =>
                                void providerActionMutation.mutateAsync({
                                  type: "clear",
                                  providerId: provider.id
                                })
                              }
                            >
                              Clear credentials
                            </button>
                          ) : null}
                          <button
                            type="button"
                            className={shellStyles.dangerButton}
                            onClick={() =>
                              void providerActionMutation.mutateAsync({
                                type: "delete",
                                providerId: provider.id
                              })
                            }
                          >
                            Delete
                          </button>
                        </div>
                      </article>
                    );
                  })}
                </div>
              ) : (
                <EmptyState
                  title="No providers configured"
                  body="Add an API-key or browser-auth provider from the workbench on the right."
                />
              )}
            </Surface>

            <Surface eyebrow="Aliases" title="Switchable routes">
              {(aliasesQuery.data ?? []).length > 0 ? (
                <div className={shellStyles.list} id="aliases-list">
                  {(aliasesQuery.data ?? []).map((alias) => (
                    <article key={alias.alias} className={shellStyles.listCard}>
                      <strong>{alias.alias}</strong>
                      <div className={shellStyles.meta}>
                        {alias.provider_id} - {alias.model}
                      </div>
                      {alias.description ? <div className={shellStyles.meta}>{alias.description}</div> : null}
                      <div className={shellStyles.buttonRow}>
                        <button
                          type="button"
                          className={shellStyles.secondaryButton}
                          onClick={() =>
                            void aliasActionMutation.mutateAsync({ type: "main", alias: alias.alias })
                          }
                        >
                          Make main
                        </button>
                        <button
                          type="button"
                          className={shellStyles.dangerButton}
                          onClick={() =>
                            void aliasActionMutation.mutateAsync({ type: "delete", alias: alias.alias })
                          }
                        >
                          Remove
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>No aliases configured yet.</p>
              )}
            </Surface>

            <Surface eyebrow="Browser auth" title="Supported session-based sign-in">
              <p className={shellStyles.callout}>
                ChatGPT / Codex browser auth is supported here. Anthropic is API-key-only and
                hosted API-key providers stay in the standard provider form.
              </p>
              <div className={shellStyles.buttonRow}>
                <button
                  type="button"
                  className={shellStyles.primaryButton}
                  onClick={() => {
                    setProviderForm((current) => ({
                      ...buildInitialProviderFormState("codex"),
                      alias_name: current.alias_name,
                      set_as_main: current.set_as_main
                    }));
                    void browserAuthMutation.mutateAsync();
                  }}
                  disabled={browserAuthMutation.isPending}
                >
                  {browserAuthMutation.isPending ? "Starting auth..." : "Start Codex browser auth"}
                </button>
              </div>
              {browserAuthSessionId ? (
                <div className={styles.browserAuthStatus}>
                  <strong>Session</strong>
                  <div className={shellStyles.meta}>{browserAuthSessionId}</div>
                  <div className={shellStyles.pillRow}>
                    <Pill tone={browserAuthStatus === "completed" ? "good" : browserAuthStatus === "failed" ? "danger" : "warn"}>
                      {browserAuthStatus ?? "pending"}
                    </Pill>
                  </div>
                  {browserAuthStatusQuery.data?.error ? (
                    <p className={shellStyles.bannerError}>{browserAuthStatusQuery.data.error}</p>
                  ) : null}
                </div>
              ) : null}
            </Surface>
          </div>

          <Surface eyebrow="Provider workbench" title="Add or update a provider" className={styles.formSurface}>
            <form className={shellStyles.stack} onSubmit={handleProviderSubmit}>
              <div className={shellStyles.formGrid}>
                <label className={shellStyles.field}>
                  Provider preset
                  <select
                    className={shellStyles.select}
                    value={providerForm.presetId}
                    onChange={(event) =>
                      setProviderForm(buildInitialProviderFormState(event.target.value as ProviderPresetId))
                    }
                  >
                    {PROVIDER_PRESETS.map((preset) => (
                      <option key={preset.id} value={preset.id}>
                        {preset.label}
                      </option>
                    ))}
                  </select>
                </label>

                <label className={shellStyles.field}>
                  Provider ID
                  <input
                    className={shellStyles.input}
                    value={providerForm.id}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, id: event.target.value }))
                    }
                    placeholder={selectedPreset.providerId}
                    required
                  />
                </label>

                <label className={shellStyles.field}>
                  Display name
                  <input
                    className={shellStyles.input}
                    value={providerForm.display_name}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, display_name: event.target.value }))
                    }
                    placeholder={selectedPreset.displayName}
                    required
                  />
                </label>

                <label className={shellStyles.field}>
                  Base URL
                  <input
                    className={shellStyles.input}
                    value={providerForm.base_url}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, base_url: event.target.value }))
                    }
                    required
                  />
                </label>
              </div>

              <div className={styles.inlineDetails}>
                <Pill tone="neutral">{startCase(providerForm.kind)}</Pill>
                <Pill tone="neutral">{startCase(providerForm.provider_profile)}</Pill>
                <Pill tone={providerForm.local ? "good" : "accent"}>{providerForm.local ? "Local" : "Hosted"}</Pill>
              </div>

              {providerForm.auth_mode === "api_key" ? (
                <label className={shellStyles.fieldWide}>
                  {selectedPreset.apiKeyLabel ?? "API key"}
                  <input
                    className={shellStyles.input}
                    value={providerForm.api_key}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, api_key: event.target.value }))
                    }
                    placeholder={selectedPreset.apiKeyPlaceholder ?? "Paste API key"}
                    autoComplete="off"
                  />
                </label>
              ) : null}

              <label className={shellStyles.fieldWide}>
                Default model
                <input
                  className={shellStyles.input}
                  list="provider-discovered-models"
                  value={providerForm.default_model}
                  onChange={(event) =>
                    setProviderForm((current) => ({ ...current, default_model: event.target.value }))
                  }
                  placeholder={selectedPreset.defaultModelPlaceholder}
                />
                {discoveredModels.length > 0 ? (
                  <datalist id="provider-discovered-models">
                    {discoveredModels.map((model) => (
                      <option key={model.id} value={model.id} />
                    ))}
                  </datalist>
                ) : null}
              </label>

              <div className={shellStyles.formGrid}>
                <label className={shellStyles.field}>
                  Alias name
                  <input
                    className={shellStyles.input}
                    value={providerForm.alias_name}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, alias_name: event.target.value }))
                    }
                    placeholder="main"
                  />
                </label>

                <label className={shellStyles.field}>
                  Alias description
                  <input
                    className={shellStyles.input}
                    value={providerForm.alias_description}
                    onChange={(event) =>
                      setProviderForm((current) => ({ ...current, alias_description: event.target.value }))
                    }
                    placeholder="Primary coding route"
                  />
                </label>
              </div>

              <label className={styles.checkboxRow}>
                <input
                  type="checkbox"
                  checked={providerForm.set_as_main}
                  onChange={(event) =>
                    setProviderForm((current) => ({ ...current, set_as_main: event.target.checked }))
                  }
                />
                Make this alias the main route after save
              </label>

              <div className={styles.discoveryStatus}>
                {modelDiscoveryQuery.isPending ? <span className={shellStyles.meta}>Loading models...</span> : null}
                {modelDiscoveryQuery.data ? (
                  <span className={styles.successCopy}>
                    Loaded {modelDiscoveryQuery.data.models.length} model
                    {modelDiscoveryQuery.data.models.length === 1 ? "" : "s"}.
                  </span>
                ) : null}
                {modelDiscoveryQuery.data?.warnings.map((warning) => (
                  <span key={warning} className={shellStyles.meta}>{warning}</span>
                ))}
                {modelDiscoveryQuery.error ? (
                  <span className={styles.errorCopy}>
                    {modelDiscoveryQuery.error instanceof Error
                      ? `Could not load models automatically: ${modelDiscoveryQuery.error.message}`
                      : "Could not load models automatically."}
                  </span>
                ) : null}
                {lastValidated ? (
                  <span className={lastValidated.ok ? styles.successCopy : styles.errorCopy}>
                    {lastValidated.ok ? "Readiness passed:" : "Readiness failed:"} {lastValidated.detail}
                  </span>
                ) : null}
              </div>

              <div className={shellStyles.buttonRow}>
                <button
                  type="button"
                  className={shellStyles.secondaryButton}
                  onClick={() => void validateProviderMutation.mutateAsync()}
                  disabled={validateProviderMutation.isPending}
                >
                  {validateProviderMutation.isPending ? "Validating..." : "Validate now"}
                </button>
                <button
                  type="submit"
                  className={shellStyles.primaryButton}
                  data-testid="modern-provider-save"
                  disabled={saveProviderMutation.isPending}
                >
                  {saveProviderMutation.isPending ? "Saving..." : "Save provider"}
                </button>
                <button
                  type="button"
                  className={shellStyles.secondaryButton}
                  onClick={() => setProviderForm(buildInitialProviderFormState(providerForm.presetId))}
                >
                  Reset form
                </button>
              </div>

              {saveProviderMutation.error ? (
                <p className={shellStyles.bannerError}>
                  {saveProviderMutation.error instanceof Error
                    ? saveProviderMutation.error.message
                    : "Provider save failed."}
                </p>
              ) : null}
            </form>
          </Surface>
        </div>
      ) : null}

      {activeTab === "connectors" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Connector roster" title="Configured ingress and external tools" emphasis="accent">
            <div className={shellStyles.list} id="connector-roster">
              {connectorRoster.map((section) => (
                <article key={section.kind} className={shellStyles.listCard}>
                  <div className={styles.cardHeader}>
                    <div>
                      <strong>{section.label}</strong>
                      <div className={shellStyles.meta}>{section.items.length} configured</div>
                    </div>
                    <button
                      type="button"
                      className={shellStyles.secondaryButton}
                      onClick={() => {
                        setConnectorForm(buildInitialConnectorFormState(section.kind));
                        updateIntegrationRoute("connectors", section.kind);
                      }}
                    >
                      New {connectorRecordLabel(section.kind)}
                    </button>
                  </div>

                  {section.items.length > 0 ? (
                    <div className={shellStyles.list}>
                      {section.items.map((item) => (
                        <article key={item.id} className={styles.connectorCard}>
                          <div>
                            <strong>{item.name}</strong>
                            <div className={shellStyles.meta}>{item.id}</div>
                            {"description" in item && item.description ? (
                              <div className={shellStyles.meta}>{item.description}</div>
                            ) : null}
                            {connectorSummaryLines(section.kind, item as unknown as Record<string, unknown>).map((line) => (
                              <div key={`${item.id}-${line}`} className={shellStyles.meta}>
                                {line}
                              </div>
                            ))}
                          </div>
                          <div className={shellStyles.buttonRow}>
                            <button
                              type="button"
                              className={shellStyles.secondaryButton}
                              onClick={() =>
                                setConnectorForm((current) => ({
                                  ...buildInitialConnectorFormState(section.kind),
                                  id: item.id,
                                  name: item.name,
                                  description: "description" in item ? item.description : "",
                                  alias: "alias" in item ? item.alias ?? "" : "",
                                  requested_model: "requested_model" in item ? item.requested_model ?? "" : "",
                                  cwd: "cwd" in item ? item.cwd ?? "" : "",
                                  enabled: "enabled" in item ? item.enabled ?? true : true,
                                  path: "path" in item ? item.path : "",
                                  delete_after_read:
                                    "delete_after_read" in item ? item.delete_after_read ?? false : false,
                                  require_pairing_approval:
                                    "require_pairing_approval" in item
                                      ? item.require_pairing_approval ?? false
                                      : false,
                                  allowed_chat_ids:
                                    "allowed_chat_ids" in item ? joinList(item.allowed_chat_ids) : "",
                                  allowed_user_ids:
                                    "allowed_user_ids" in item ? joinList(item.allowed_user_ids) : "",
                                  monitored_channel_ids:
                                    "monitored_channel_ids" in item ? joinList(item.monitored_channel_ids) : "",
                                  allowed_channel_ids:
                                    "allowed_channel_ids" in item ? joinList(item.allowed_channel_ids) : "",
                                  monitored_group_ids:
                                    "monitored_group_ids" in item ? joinList(item.monitored_group_ids) : "",
                                  allowed_group_ids:
                                    "allowed_group_ids" in item ? joinList(item.allowed_group_ids) : "",
                                  base_url: "base_url" in item ? item.base_url : current.base_url,
                                  monitored_entity_ids:
                                    "monitored_entity_ids" in item ? joinList(item.monitored_entity_ids) : "",
                                  allowed_service_domains:
                                    "allowed_service_domains" in item ? joinList(item.allowed_service_domains) : "",
                                  allowed_service_entity_ids:
                                    "allowed_service_entity_ids" in item
                                      ? joinList(item.allowed_service_entity_ids)
                                      : "",
                                  account: "account" in item ? item.account : "",
                                  cli_path: "cli_path" in item ? item.cli_path ?? "" : "",
                                  label_filter: "label_filter" in item ? item.label_filter ?? "" : "",
                                  command: "command" in item ? item.command : "",
                                  args: "args" in item ? joinList(item.args) : "",
                                  tool_name: "tool_name" in item ? item.tool_name : "",
                                  input_schema_json:
                                    "input_schema_json" in item ? item.input_schema_json : current.input_schema_json,
                                  prompt_template:
                                    "prompt_template" in item ? item.prompt_template : current.prompt_template
                                }))
                              }
                            >
                              Edit
                            </button>
                            <button
                              type="button"
                              className={shellStyles.dangerButton}
                              onClick={() =>
                                void connectorDeleteMutation.mutateAsync({
                                  kind: section.kind,
                                  id: item.id
                                })
                              }
                            >
                              Remove
                            </button>
                          </div>
                        </article>
                      ))}
                    </div>
                  ) : (
                    <p className={shellStyles.empty}>No {section.label.toLowerCase()} connectors configured.</p>
                  )}
                </article>
              ))}
            </div>
          </Surface>

          <Surface eyebrow="Connector workbench" title={`${connectorRecordLabel(connectorForm.kind)} setup`} className={styles.formSurface}>
            <form id="connector-quick-form" className={shellStyles.stack} onSubmit={handleConnectorSubmit}>
              <div className={shellStyles.formGrid}>
                <label className={shellStyles.field}>
                  Connector type
                    <select
                      id="connector-kind"
                      className={shellStyles.select}
                      value={connectorForm.kind}
                      onChange={(event) => {
                        const kind = event.target.value as ConnectorKind;
                        setConnectorForm(buildInitialConnectorFormState(kind));
                        updateIntegrationRoute("connectors", kind);
                      }}
                    >
                    {(["app", "inbox", "telegram", "discord", "slack", "home_assistant", "signal", "gmail", "brave", "webhook"] as ConnectorKind[]).map((kind) => (
                      <option key={kind} value={kind}>
                        {connectorRecordLabel(kind)}
                      </option>
                    ))}
                  </select>
                </label>
                <label className={shellStyles.field}>
                  Connector ID
                  <input
                    id="connector-id"
                    className={shellStyles.input}
                    value={connectorForm.id}
                    onChange={(event) => setConnectorForm((current) => ({ ...current, id: event.target.value }))}
                  />
                </label>
                <label className={shellStyles.field}>
                  Name
                  <input
                    id="connector-name"
                    className={shellStyles.input}
                    value={connectorForm.name}
                    onChange={(event) => setConnectorForm((current) => ({ ...current, name: event.target.value }))}
                  />
                </label>
                <label className={shellStyles.field}>
                  Alias
                  <input
                    id="connector-alias"
                    className={shellStyles.input}
                    value={connectorForm.alias}
                    onChange={(event) => setConnectorForm((current) => ({ ...current, alias: event.target.value }))}
                    placeholder="main"
                  />
                </label>
              </div>

              <label className={shellStyles.fieldWide}>
                Description
                <textarea
                  id="connector-description"
                  className={shellStyles.textarea}
                  value={connectorForm.description}
                  onChange={(event) => setConnectorForm((current) => ({ ...current, description: event.target.value }))}
                  rows={3}
                />
              </label>

              {connectorForm.kind === "app" ? (
                <div className={shellStyles.stack}>
                  <div className={shellStyles.formGrid}>
                    <label className={shellStyles.field}>
                      Command
                      <input className={shellStyles.input} value={connectorForm.command} onChange={(event) => setConnectorForm((current) => ({ ...current, command: event.target.value }))} />
                    </label>
                    <label className={shellStyles.field}>
                      Args (comma separated)
                      <input className={shellStyles.input} value={connectorForm.args} onChange={(event) => setConnectorForm((current) => ({ ...current, args: event.target.value }))} />
                    </label>
                    <label className={shellStyles.field}>
                      Tool name
                      <input className={shellStyles.input} value={connectorForm.tool_name} onChange={(event) => setConnectorForm((current) => ({ ...current, tool_name: event.target.value }))} />
                    </label>
                    <label className={shellStyles.field}>
                      Working directory
                      <input className={shellStyles.input} value={connectorForm.cwd} onChange={(event) => setConnectorForm((current) => ({ ...current, cwd: event.target.value }))} />
                    </label>
                  </div>
                  <label className={shellStyles.fieldWide}>
                    Input schema JSON
                    <textarea className={shellStyles.textarea} value={connectorForm.input_schema_json} onChange={(event) => setConnectorForm((current) => ({ ...current, input_schema_json: event.target.value }))} rows={8} />
                  </label>
                </div>
              ) : null}

              {connectorForm.kind === "inbox" ? (
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    Inbox path
                    <input
                      id="connector-inbox-path"
                      className={shellStyles.input}
                      value={connectorForm.path}
                      onChange={(event) => setConnectorForm((current) => ({ ...current, path: event.target.value }))}
                    />
                  </label>
                  <label className={styles.checkboxRow}>
                    <input type="checkbox" checked={connectorForm.delete_after_read} onChange={(event) => setConnectorForm((current) => ({ ...current, delete_after_read: event.target.checked }))} />
                    Delete files after reading
                  </label>
                </div>
              ) : null}

              {["telegram", "discord", "slack"].includes(connectorForm.kind) ? (
                <div className={shellStyles.stack}>
                  <div className={shellStyles.formGrid}>
                    <label className={shellStyles.field}>
                      {connectorForm.kind === "telegram" ? "Bot token" : "Token"}
                      <input className={shellStyles.input} value={connectorForm.bot_token} onChange={(event) => setConnectorForm((current) => ({ ...current, bot_token: event.target.value }))} />
                    </label>
                    <label className={styles.checkboxRow}>
                      <input type="checkbox" checked={connectorForm.require_pairing_approval} onChange={(event) => setConnectorForm((current) => ({ ...current, require_pairing_approval: event.target.checked }))} />
                      Require pairing approval
                    </label>
                  </div>
                  {connectorForm.kind === "telegram" ? (
                    <div className={shellStyles.formGrid}>
                      <label className={shellStyles.field}>
                        Allowed chat ids
                        <input className={shellStyles.input} value={connectorForm.allowed_chat_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_chat_ids: event.target.value }))} />
                      </label>
                      <label className={shellStyles.field}>
                        Allowed user ids
                        <input className={shellStyles.input} value={connectorForm.allowed_user_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_user_ids: event.target.value }))} />
                      </label>
                    </div>
                  ) : (
                    <div className={shellStyles.formGrid}>
                      <label className={shellStyles.field}>
                        Monitored channels
                        <input className={shellStyles.input} value={connectorForm.monitored_channel_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, monitored_channel_ids: event.target.value }))} />
                      </label>
                      <label className={shellStyles.field}>
                        Allowed channels
                        <input className={shellStyles.input} value={connectorForm.allowed_channel_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_channel_ids: event.target.value }))} />
                      </label>
                      <label className={shellStyles.field}>
                        Allowed user ids
                        <input className={shellStyles.input} value={connectorForm.allowed_user_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_user_ids: event.target.value }))} />
                      </label>
                    </div>
                  )}
                </div>
              ) : null}

              {connectorForm.kind === "home_assistant" ? (
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    Base URL
                    <input className={shellStyles.input} value={connectorForm.base_url} onChange={(event) => setConnectorForm((current) => ({ ...current, base_url: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Access token
                    <input className={shellStyles.input} value={connectorForm.access_token} onChange={(event) => setConnectorForm((current) => ({ ...current, access_token: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Monitored entities
                    <input className={shellStyles.input} value={connectorForm.monitored_entity_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, monitored_entity_ids: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Allowed service domains
                    <input className={shellStyles.input} value={connectorForm.allowed_service_domains} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_service_domains: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Allowed service entities
                    <input className={shellStyles.input} value={connectorForm.allowed_service_entity_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_service_entity_ids: event.target.value }))} />
                  </label>
                </div>
              ) : null}

              {connectorForm.kind === "signal" ? (
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    Account
                    <input className={shellStyles.input} value={connectorForm.account} onChange={(event) => setConnectorForm((current) => ({ ...current, account: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    CLI path
                    <input className={shellStyles.input} value={connectorForm.cli_path} onChange={(event) => setConnectorForm((current) => ({ ...current, cli_path: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Monitored groups
                    <input className={shellStyles.input} value={connectorForm.monitored_group_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, monitored_group_ids: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Allowed groups
                    <input className={shellStyles.input} value={connectorForm.allowed_group_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_group_ids: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Allowed users
                    <input className={shellStyles.input} value={connectorForm.allowed_user_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_user_ids: event.target.value }))} />
                  </label>
                </div>
              ) : null}

              {connectorForm.kind === "gmail" ? (
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    OAuth token
                    <input className={shellStyles.input} value={connectorForm.oauth_token} onChange={(event) => setConnectorForm((current) => ({ ...current, oauth_token: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Allowed senders
                    <input className={shellStyles.input} value={connectorForm.allowed_user_ids} onChange={(event) => setConnectorForm((current) => ({ ...current, allowed_user_ids: event.target.value }))} />
                  </label>
                  <label className={shellStyles.field}>
                    Label filter
                    <input className={shellStyles.input} value={connectorForm.label_filter} onChange={(event) => setConnectorForm((current) => ({ ...current, label_filter: event.target.value }))} />
                  </label>
                </div>
              ) : null}

              {connectorForm.kind === "brave" ? (
                <label className={shellStyles.field}>
                  Brave API key
                  <input className={shellStyles.input} value={connectorForm.api_key} onChange={(event) => setConnectorForm((current) => ({ ...current, api_key: event.target.value }))} />
                </label>
              ) : null}

              {connectorForm.kind === "webhook" ? (
                <div className={shellStyles.stack}>
                  <label className={shellStyles.field}>
                    Webhook token
                    <input className={shellStyles.input} value={connectorForm.webhook_token} onChange={(event) => setConnectorForm((current) => ({ ...current, webhook_token: event.target.value }))} />
                  </label>
                  <label className={shellStyles.fieldWide}>
                    Prompt template
                    <textarea className={shellStyles.textarea} value={connectorForm.prompt_template} onChange={(event) => setConnectorForm((current) => ({ ...current, prompt_template: event.target.value }))} rows={5} />
                  </label>
                </div>
              ) : null}

              <div className={shellStyles.buttonRow}>
                <button
                  id="connector-save"
                  type="submit"
                  className={shellStyles.primaryButton}
                  disabled={connectorMutation.isPending}
                >
                  {connectorMutation.isPending ? "Saving..." : "Save connector"}
                </button>
                <button
                  id="connector-reset"
                  type="button"
                  className={shellStyles.secondaryButton}
                  onClick={() => setConnectorForm(buildInitialConnectorFormState(connectorForm.kind))}
                >
                  Reset form
                </button>
              </div>

              {connectorMutation.error ? (
                <p className={shellStyles.bannerError}>
                  {connectorMutation.error instanceof Error
                    ? connectorMutation.error.message
                    : "Connector save failed."}
                </p>
              ) : null}
            </form>
          </Surface>
        </div>
      ) : null}

      {activeTab === "plugins" ? (
        <div className={shellStyles.gridTwo}>
          <div className={shellStyles.stack}>
            <Surface eyebrow="Installed plugins" title="Extension lifecycle" emphasis="accent">
              {(pluginsQuery.data ?? []).length > 0 ? (
                <div className={shellStyles.list} id="plugins-list">
                  {(pluginsQuery.data ?? []).map((plugin) => (
                    <article key={plugin.id} className={shellStyles.listCard}>
                      <div className={styles.cardHeader}>
                        <div>
                          <strong>{plugin.manifest.name}</strong>
                          <div className={shellStyles.meta}>{plugin.id} - v{plugin.manifest.version}</div>
                          <div className={shellStyles.meta}>{plugin.source_path}</div>
                        </div>
                        <div className={shellStyles.pillRow}>
                          <Pill tone={plugin.enabled ? "good" : "neutral"}>{plugin.enabled ? "Enabled" : "Disabled"}</Pill>
                          <Pill tone={plugin.trusted ? "accent" : "warn"}>{plugin.trusted ? "Trusted" : "Review needed"}</Pill>
                        </div>
                      </div>
                      <div className={shellStyles.meta}>{plugin.manifest.description}</div>
                      <div className={shellStyles.buttonRow}>
                        <button
                          type="button"
                          className={shellStyles.secondaryButton}
                          onClick={() => void pluginActionMutation.mutateAsync({ kind: "update", plugin })}
                          disabled={pluginActionMutation.isPending}
                        >
                          Update
                        </button>
                        <button
                          type="button"
                          className={shellStyles.secondaryButton}
                          onClick={() => void pluginActionMutation.mutateAsync({ kind: "toggle-trusted", plugin })}
                          disabled={pluginActionMutation.isPending}
                        >
                          {plugin.trusted ? "Untrust" : "Trust"}
                        </button>
                        <button
                          type="button"
                          className={shellStyles.secondaryButton}
                          onClick={() => void pluginActionMutation.mutateAsync({ kind: "toggle-enabled", plugin })}
                          disabled={pluginActionMutation.isPending}
                        >
                          {plugin.enabled ? "Disable" : "Enable"}
                        </button>
                        <button
                          type="button"
                          className={shellStyles.dangerButton}
                          onClick={() => void pluginActionMutation.mutateAsync({ kind: "remove", plugin })}
                          disabled={pluginActionMutation.isPending}
                        >
                          Remove
                        </button>
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>No plugins installed yet.</p>
              )}
            </Surface>

            <Surface eyebrow="Doctor reports" title="Runtime readiness">
              {(pluginDoctorQuery.data ?? []).length > 0 ? (
                <div className={shellStyles.list} id="plugins-health">
                  {(pluginDoctorQuery.data ?? []).map((report) => (
                    <article key={report.id} className={shellStyles.listCard}>
                      <div className={styles.cardHeader}>
                        <strong>{report.name}</strong>
                        <Pill tone={report.ok ? "good" : "warn"}>{report.ok ? "ready" : "attention"}</Pill>
                      </div>
                      <div className={shellStyles.meta}>{report.detail}</div>
                      <div className={shellStyles.meta}>
                        tools {report.tools} - connectors {report.connectors} - adapters {report.provider_adapters}
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>No doctor reports available yet.</p>
              )}
            </Surface>
          </div>

          <Surface eyebrow="Install plugin" title="Trusted extension intake" className={styles.formSurface}>
            <form className={shellStyles.stack} onSubmit={handlePluginSubmit}>
              <label className={shellStyles.fieldWide}>
                Source path
                <input className={shellStyles.input} value={pluginForm.source_path} onChange={(event) => setPluginForm((current) => ({ ...current, source_path: event.target.value }))} placeholder="J:\\plugins\\echo-toolkit" />
              </label>

              <div className={shellStyles.formGrid}>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.enabled} onChange={(event) => setPluginForm((current) => ({ ...current, enabled: event.target.checked }))} />
                  Enable after install
                </label>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.trusted} onChange={(event) => setPluginForm((current) => ({ ...current, trusted: event.target.checked }))} />
                  Trust immediately
                </label>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.pinned} onChange={(event) => setPluginForm((current) => ({ ...current, pinned: event.target.checked }))} />
                  Pin current revision
                </label>
              </div>

              <div className={styles.permissionGrid}>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.permissions.shell} onChange={(event) => setPluginForm((current) => ({ ...current, permissions: { ...current.permissions, shell: event.target.checked } }))} />
                  Shell permission
                </label>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.permissions.network} onChange={(event) => setPluginForm((current) => ({ ...current, permissions: { ...current.permissions, network: event.target.checked } }))} />
                  Network permission
                </label>
                <label className={styles.checkboxRow}>
                  <input type="checkbox" checked={pluginForm.permissions.full_disk} onChange={(event) => setPluginForm((current) => ({ ...current, permissions: { ...current.permissions, full_disk: event.target.checked } }))} />
                  Full-disk permission
                </label>
              </div>

              <div className={shellStyles.buttonRow}>
                <button type="submit" className={shellStyles.primaryButton} disabled={pluginInstallMutation.isPending}>
                  {pluginInstallMutation.isPending ? "Installing..." : "Install plugin"}
                </button>
              </div>

              {pluginInstallMutation.error ? (
                <p className={shellStyles.bannerError}>
                  {pluginInstallMutation.error instanceof Error
                    ? pluginInstallMutation.error.message
                    : "Plugin install failed."}
                </p>
              ) : null}
            </form>
          </Surface>
        </div>
      ) : null}

      {activeTab === "mcp" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Managed servers" title="MCP roster" emphasis="accent">
            {(mcpQuery.data ?? []).length > 0 ? (
              <div className={shellStyles.list}>
                {(mcpQuery.data ?? []).map((server) => (
                  <article key={server.id} className={shellStyles.listCard}>
                    <div className={styles.cardHeader}>
                      <div>
                        <strong>{server.name}</strong>
                        <div className={shellStyles.meta}>{server.id}</div>
                        <div className={shellStyles.meta}>{server.command}</div>
                      </div>
                      <Pill tone={server.enabled ? "good" : "neutral"}>{server.enabled ? "Enabled" : "Disabled"}</Pill>
                    </div>
                    <div className={shellStyles.meta}>{server.description}</div>
                    <div className={shellStyles.buttonRow}>
                      <button type="button" className={shellStyles.secondaryButton} onClick={() => setMcpForm({ id: server.id, name: server.name, description: server.description, command: server.command, args: joinList(server.args), tool_name: server.tool_name, input_schema_json: server.input_schema_json, cwd: server.cwd ?? "", enabled: server.enabled ?? true })}>Edit</button>
                      <button type="button" className={shellStyles.dangerButton} onClick={() => void mcpDeleteMutation.mutateAsync(server.id)}>Remove</button>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <p className={shellStyles.empty}>No MCP servers configured.</p>
            )}
          </Surface>

          <Surface eyebrow="MCP workbench" title="Add or update server" className={styles.formSurface}>
            <form className={shellStyles.stack} onSubmit={handleMcpSubmit}>
              <div className={shellStyles.formGrid}>
                <label className={shellStyles.field}>
                  Server ID
                  <input className={shellStyles.input} value={mcpForm.id} onChange={(event) => setMcpForm((current) => ({ ...current, id: event.target.value }))} />
                </label>
                <label className={shellStyles.field}>
                  Name
                  <input className={shellStyles.input} value={mcpForm.name} onChange={(event) => setMcpForm((current) => ({ ...current, name: event.target.value }))} />
                </label>
                <label className={shellStyles.field}>
                  Command
                  <input className={shellStyles.input} value={mcpForm.command} onChange={(event) => setMcpForm((current) => ({ ...current, command: event.target.value }))} />
                </label>
                <label className={shellStyles.field}>
                  Args (comma separated)
                  <input className={shellStyles.input} value={mcpForm.args} onChange={(event) => setMcpForm((current) => ({ ...current, args: event.target.value }))} />
                </label>
                <label className={shellStyles.field}>
                  Tool name
                  <input className={shellStyles.input} value={mcpForm.tool_name} onChange={(event) => setMcpForm((current) => ({ ...current, tool_name: event.target.value }))} />
                </label>
                <label className={shellStyles.field}>
                  Working directory
                  <input className={shellStyles.input} value={mcpForm.cwd} onChange={(event) => setMcpForm((current) => ({ ...current, cwd: event.target.value }))} />
                </label>
              </div>

              <label className={shellStyles.fieldWide}>
                Description
                <textarea className={shellStyles.textarea} value={mcpForm.description} onChange={(event) => setMcpForm((current) => ({ ...current, description: event.target.value }))} rows={3} />
              </label>

              <label className={shellStyles.fieldWide}>
                Input schema JSON
                <textarea className={shellStyles.textarea} value={mcpForm.input_schema_json} onChange={(event) => setMcpForm((current) => ({ ...current, input_schema_json: event.target.value }))} rows={8} />
              </label>

              <label className={styles.checkboxRow}>
                <input type="checkbox" checked={mcpForm.enabled} onChange={(event) => setMcpForm((current) => ({ ...current, enabled: event.target.checked }))} />
                Enable after save
              </label>

              <div className={shellStyles.buttonRow}>
                <button type="submit" className={shellStyles.primaryButton} disabled={mcpMutation.isPending}>
                  {mcpMutation.isPending ? "Saving..." : "Save server"}
                </button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => setMcpForm(buildInitialMcpFormState())}>
                  Reset form
                </button>
              </div>

              {mcpMutation.error ? (
                <p className={shellStyles.bannerError}>
                  {mcpMutation.error instanceof Error ? mcpMutation.error.message : "MCP save failed."}
                </p>
              ) : null}
            </form>
          </Surface>
        </div>
      ) : null}
    </div>
  );
}
