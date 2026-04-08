import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  DashboardBootstrapResponse,
  PermissionPreset,
  RemoteContentPolicy
} from "../../api/types";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import { IntegrationsPage } from "./IntegrationsPage";

const listProviders = vi.fn().mockResolvedValue([]);
const saveProvider = vi.fn().mockResolvedValue(undefined);
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

vi.mock("../../api/client", () => ({
  listProviders: (...args: unknown[]) => listProviders(...args),
  saveProvider: (...args: unknown[]) => saveProvider(...args),
  discoverProvider: (...args: unknown[]) => discoverProvider(...args),
  validateProvider: (...args: unknown[]) => validateProvider(...args)
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
      <DashboardDataProvider value={value}>
        <IntegrationsPage />
      </DashboardDataProvider>
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
