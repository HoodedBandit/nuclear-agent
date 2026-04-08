import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  DashboardBootstrapResponse,
  PermissionPreset,
  RemoteContentPolicy
} from "../../api/types";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import { IntegrationsPage } from "./IntegrationsPage";

const listProviders = vi.fn().mockResolvedValue([]);
const listAliases = vi.fn().mockResolvedValue([]);
const listAppConnectors = vi.fn().mockResolvedValue([]);
const listInboxConnectors = vi.fn().mockResolvedValue([]);
const listTelegramConnectors = vi.fn().mockResolvedValue([]);
const listDiscordConnectors = vi.fn().mockResolvedValue([]);
const listSlackConnectors = vi.fn().mockResolvedValue([]);
const listHomeAssistantConnectors = vi.fn().mockResolvedValue([]);
const listSignalConnectors = vi.fn().mockResolvedValue([]);
const listGmailConnectors = vi.fn().mockResolvedValue([]);
const listBraveConnectors = vi.fn().mockResolvedValue([]);
const listWebhookConnectors = vi.fn().mockResolvedValue([]);
const listPlugins = vi.fn().mockResolvedValue([]);
const listPluginDoctorReports = vi.fn().mockResolvedValue([]);
const listMcpServers = vi.fn().mockResolvedValue([]);
const saveProvider = vi.fn().mockResolvedValue(undefined);
const saveAlias = vi.fn().mockResolvedValue(undefined);
const discoverProvider = vi.fn().mockResolvedValue({
  models: [{ id: "kimi-k2.5" }, { id: "kimi-k2" }],
  recommended_model: "kimi-k2.5",
  warnings: [],
  readiness: null
});
const validateProvider = vi.fn().mockResolvedValue({
  ok: true,
  model: "kimi-k2.5",
  detail: "completion and tool schema validation succeeded"
});
const clearProviderCredentials = vi.fn().mockResolvedValue(undefined);
const deleteProvider = vi.fn().mockResolvedValue(undefined);
const startProviderBrowserAuth = vi.fn().mockResolvedValue({ session_id: "session", status: "pending" });
const fetchProviderBrowserAuthStatus = vi.fn().mockResolvedValue({ session_id: "session", kind: "codex", provider_id: "codex", display_name: "Codex", status: "pending" });
const saveAppConnector = vi.fn().mockResolvedValue(undefined);
const saveInboxConnector = vi.fn().mockResolvedValue(undefined);
const saveTelegramConnector = vi.fn().mockResolvedValue(undefined);
const saveDiscordConnector = vi.fn().mockResolvedValue(undefined);
const saveSlackConnector = vi.fn().mockResolvedValue(undefined);
const saveHomeAssistantConnector = vi.fn().mockResolvedValue(undefined);
const saveSignalConnector = vi.fn().mockResolvedValue(undefined);
const saveGmailConnector = vi.fn().mockResolvedValue(undefined);
const saveBraveConnector = vi.fn().mockResolvedValue(undefined);
const saveWebhookConnector = vi.fn().mockResolvedValue(undefined);
const deleteAppConnector = vi.fn().mockResolvedValue(undefined);
const deleteInboxConnector = vi.fn().mockResolvedValue(undefined);
const deleteTelegramConnector = vi.fn().mockResolvedValue(undefined);
const deleteDiscordConnector = vi.fn().mockResolvedValue(undefined);
const deleteSlackConnector = vi.fn().mockResolvedValue(undefined);
const deleteHomeAssistantConnector = vi.fn().mockResolvedValue(undefined);
const deleteSignalConnector = vi.fn().mockResolvedValue(undefined);
const deleteGmailConnector = vi.fn().mockResolvedValue(undefined);
const deleteBraveConnector = vi.fn().mockResolvedValue(undefined);
const deleteWebhookConnector = vi.fn().mockResolvedValue(undefined);
const installPlugin = vi.fn().mockResolvedValue(undefined);
const updatePlugin = vi.fn().mockResolvedValue(undefined);
const updatePluginState = vi.fn().mockResolvedValue(undefined);
const deletePlugin = vi.fn().mockResolvedValue(undefined);
const saveMcpServer = vi.fn().mockResolvedValue(undefined);
const deleteMcpServer = vi.fn().mockResolvedValue(undefined);
const updateMainAlias = vi.fn().mockResolvedValue(undefined);
const deleteAlias = vi.fn().mockResolvedValue(undefined);

vi.mock("../../api/client", () => ({
  clearProviderCredentials: (...args: unknown[]) => clearProviderCredentials(...args),
  deleteAlias: (...args: unknown[]) => deleteAlias(...args),
  deleteAppConnector: (...args: unknown[]) => deleteAppConnector(...args),
  deleteBraveConnector: (...args: unknown[]) => deleteBraveConnector(...args),
  deleteDiscordConnector: (...args: unknown[]) => deleteDiscordConnector(...args),
  deleteGmailConnector: (...args: unknown[]) => deleteGmailConnector(...args),
  deleteHomeAssistantConnector: (...args: unknown[]) => deleteHomeAssistantConnector(...args),
  deleteInboxConnector: (...args: unknown[]) => deleteInboxConnector(...args),
  deleteMcpServer: (...args: unknown[]) => deleteMcpServer(...args),
  deletePlugin: (...args: unknown[]) => deletePlugin(...args),
  deleteProvider: (...args: unknown[]) => deleteProvider(...args),
  deleteSignalConnector: (...args: unknown[]) => deleteSignalConnector(...args),
  deleteSlackConnector: (...args: unknown[]) => deleteSlackConnector(...args),
  deleteTelegramConnector: (...args: unknown[]) => deleteTelegramConnector(...args),
  deleteWebhookConnector: (...args: unknown[]) => deleteWebhookConnector(...args),
  listProviders: (...args: unknown[]) => listProviders(...args),
  listAliases: (...args: unknown[]) => listAliases(...args),
  listAppConnectors: (...args: unknown[]) => listAppConnectors(...args),
  listInboxConnectors: (...args: unknown[]) => listInboxConnectors(...args),
  listTelegramConnectors: (...args: unknown[]) => listTelegramConnectors(...args),
  listDiscordConnectors: (...args: unknown[]) => listDiscordConnectors(...args),
  listSlackConnectors: (...args: unknown[]) => listSlackConnectors(...args),
  listHomeAssistantConnectors: (...args: unknown[]) => listHomeAssistantConnectors(...args),
  listSignalConnectors: (...args: unknown[]) => listSignalConnectors(...args),
  listGmailConnectors: (...args: unknown[]) => listGmailConnectors(...args),
  listBraveConnectors: (...args: unknown[]) => listBraveConnectors(...args),
  listWebhookConnectors: (...args: unknown[]) => listWebhookConnectors(...args),
  listPlugins: (...args: unknown[]) => listPlugins(...args),
  listPluginDoctorReports: (...args: unknown[]) => listPluginDoctorReports(...args),
  listMcpServers: (...args: unknown[]) => listMcpServers(...args),
  saveProvider: (...args: unknown[]) => saveProvider(...args),
  saveAlias: (...args: unknown[]) => saveAlias(...args),
  saveAppConnector: (...args: unknown[]) => saveAppConnector(...args),
  saveInboxConnector: (...args: unknown[]) => saveInboxConnector(...args),
  saveTelegramConnector: (...args: unknown[]) => saveTelegramConnector(...args),
  saveDiscordConnector: (...args: unknown[]) => saveDiscordConnector(...args),
  saveSlackConnector: (...args: unknown[]) => saveSlackConnector(...args),
  saveHomeAssistantConnector: (...args: unknown[]) => saveHomeAssistantConnector(...args),
  saveSignalConnector: (...args: unknown[]) => saveSignalConnector(...args),
  saveGmailConnector: (...args: unknown[]) => saveGmailConnector(...args),
  saveBraveConnector: (...args: unknown[]) => saveBraveConnector(...args),
  saveWebhookConnector: (...args: unknown[]) => saveWebhookConnector(...args),
  saveMcpServer: (...args: unknown[]) => saveMcpServer(...args),
  discoverProvider: (...args: unknown[]) => discoverProvider(...args),
  validateProvider: (...args: unknown[]) => validateProvider(...args)
  ,
  installPlugin: (...args: unknown[]) => installPlugin(...args),
  updatePlugin: (...args: unknown[]) => updatePlugin(...args),
  updatePluginState: (...args: unknown[]) => updatePluginState(...args),
  startProviderBrowserAuth: (...args: unknown[]) => startProviderBrowserAuth(...args),
  fetchProviderBrowserAuthStatus: (...args: unknown[]) => fetchProviderBrowserAuthStatus(...args),
  updateMainAlias: (...args: unknown[]) => updateMainAlias(...args)
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderPage(bootstrap?: Partial<DashboardBootstrapResponse>) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false
      }
    }
  });

  const value = {
    bootstrap: {
      status: {
        pid: 1,
        started_at: "2026-04-03T00:00:00Z",
        persistence_mode: "full",
        auto_start: false,
        onboarding_complete: true,
        autonomy: {
          state: "disabled",
          mode: "guarded",
          unlimited_usage: false,
          full_network: false,
          allow_self_edit: false
        },
        autopilot: {
          state: "disabled",
          max_concurrent_missions: 1,
          wake_interval_seconds: 60,
          allow_background_shell: false,
          allow_background_network: false,
          allow_background_self_edit: false
        },
        delegation: {
          max_depth: "1",
          max_parallel_subagents: "1",
          disabled_provider_ids: []
        },
        evolve: {
          state: "disabled"
        },
        ...bootstrap?.status
      },
      providers: [],
      aliases: [],
      delegation_targets: [],
      telegram_connectors: [],
      discord_connectors: [],
      slack_connectors: [],
      signal_connectors: [],
      home_assistant_connectors: [],
      webhook_connectors: [],
      inbox_connectors: [],
      gmail_connectors: [],
      brave_connectors: [],
      plugins: [],
      sessions: [],
      events: [],
      permissions: "suggest" as PermissionPreset,
      trust: {
        trusted_paths: [],
        allow_shell: false,
        allow_network: true,
        allow_full_disk: false,
        allow_self_edit: false
      },
      delegation_config: {
        max_depth: "1",
        max_parallel_subagents: "1",
        disabled_provider_ids: []
      },
      provider_capabilities: [],
      remote_content_policy: "block_high_risk" as RemoteContentPolicy,
      ...bootstrap
    },
    onLogout: vi.fn()
  };

  return render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <DashboardDataProvider value={value}>
          <IntegrationsPage />
        </DashboardDataProvider>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe("IntegrationsPage", () => {
  it("lets the operator keep a manual model that is not in discovery results", async () => {
    renderPage();

    fireEvent.change(screen.getByLabelText(/provider preset/i), {
      target: { value: "openrouter" }
    });
    fireEvent.change(screen.getByLabelText(/openrouter api key/i), {
      target: { value: "sk-or-test" }
    });

    await waitFor(() => {
      expect(discoverProvider).toHaveBeenCalled();
    });

    fireEvent.change(screen.getByLabelText(/default model/i), {
      target: { value: "custom/provider-model" }
    });

    fireEvent.click(screen.getByTestId("modern-provider-save"));

    await waitFor(() => {
      expect(validateProvider).toHaveBeenCalledWith({
        provider: {
          id: "openrouter",
          display_name: "OpenRouter",
          kind: "open_ai_compatible",
          base_url: "https://openrouter.ai/api/v1",
          provider_profile: "open_router",
          auth_mode: "api_key",
          default_model: "custom/provider-model",
          keychain_account: null,
          local: false
        },
        api_key: "sk-or-test",
        oauth_token: null
      });
    });
  });

  it("stores API-key providers with the provided credential and discovered model", async () => {
    renderPage();

    fireEvent.change(screen.getByLabelText(/provider preset/i), {
      target: { value: "moonshot" }
    });
    fireEvent.change(screen.getByLabelText(/moonshot api key/i), {
      target: { value: "sk-moonshot-test" }
    });

    await waitFor(() => {
      expect(discoverProvider).toHaveBeenCalled();
    });

    expect(await screen.findByText(/loaded 2 models/i)).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByLabelText(/default model/i)).toHaveValue("kimi-k2.5");
    });

    fireEvent.click(screen.getByTestId("modern-provider-save"));

    await waitFor(() => {
      expect(saveProvider).toHaveBeenCalledWith({
        provider: {
          id: "moonshot",
          display_name: "Moonshot",
          kind: "open_ai_compatible",
          base_url: "https://api.moonshot.ai/v1",
          provider_profile: "moonshot",
          auth_mode: "api_key",
          default_model: "kimi-k2.5",
          keychain_account: null,
          local: false
        },
        api_key: "sk-moonshot-test",
        oauth_token: null
      });
      expect(validateProvider).toHaveBeenCalled();
    });
  });
});
