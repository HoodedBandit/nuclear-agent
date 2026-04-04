import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type {
  DashboardBootstrapResponse,
  PermissionPreset,
  RemoteContentPolicy
} from "../../api/types";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import { IntegrationsPage } from "./IntegrationsPage";

const listProviders = vi.fn().mockResolvedValue([]);
const saveProvider = vi.fn().mockResolvedValue(undefined);
const discoverProviderModels = vi.fn().mockResolvedValue(["kimi-k2.5", "kimi-k2"]);

vi.mock("../../api/client", () => ({
  listProviders: (...args: unknown[]) => listProviders(...args),
  saveProvider: (...args: unknown[]) => saveProvider(...args),
  discoverProviderModels: (...args: unknown[]) => discoverProviderModels(...args)
}));

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
  it("stores API-key providers with the provided credential and discovered model", async () => {
    renderPage();

    fireEvent.change(screen.getByLabelText(/provider preset/i), {
      target: { value: "moonshot" }
    });
    fireEvent.change(screen.getByLabelText(/moonshot api key/i), {
      target: { value: "sk-moonshot-test" }
    });

    await waitFor(() => {
      expect(discoverProviderModels).toHaveBeenCalled();
    });

    expect(await screen.findByText(/loaded 2 models/i)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText(/default model/i), {
      target: { value: "kimi-k2.5" }
    });

    fireEvent.click(screen.getByTestId("modern-provider-save"));

    await waitFor(() => {
      expect(saveProvider).toHaveBeenCalledWith({
        provider: {
          id: "moonshot",
          display_name: "Moonshot",
          kind: "open_ai_compatible",
          base_url: "https://api.moonshot.ai/v1",
          auth_mode: "api_key",
          default_model: "kimi-k2.5",
          keychain_account: null,
          local: false
        },
        api_key: "sk-moonshot-test",
        oauth_token: null
      });
    });
  });
});
