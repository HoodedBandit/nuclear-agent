import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import type { DashboardBootstrapResponse, UpdateStatusResponse } from "../../api/types";
import { SystemPage } from "./SystemPage";
import { hasPendingUpdate } from "./update-session";

const createSupportBundleMock = vi.fn();
const fetchDoctorMock = vi.fn();
const fetchUpdateStatusMock = vi.fn();
const getJsonMock = vi.fn();
const postJsonMock = vi.fn();
const putJsonMock = vi.fn();
const runUpdateMock = vi.fn();

vi.mock("../../api/client", () => ({
  createSupportBundle: (...args: unknown[]) => createSupportBundleMock(...args),
  fetchDoctor: (...args: unknown[]) => fetchDoctorMock(...args),
  fetchUpdateStatus: (...args: unknown[]) => fetchUpdateStatusMock(...args),
  getJson: (...args: unknown[]) => getJsonMock(...args),
  postJson: (...args: unknown[]) => postJsonMock(...args),
  putJson: (...args: unknown[]) => putJsonMock(...args),
  runUpdate: (...args: unknown[]) => runUpdateMock(...args)
}));

function bootstrapFixture(): DashboardBootstrapResponse {
  return {
    status: {
      pid: 1000,
      started_at: "2026-04-17T00:00:00Z",
      persistence_mode: "always_on",
      auto_start: true,
      main_agent_alias: "main",
      main_target: {
        alias: "main",
        provider_id: "codex",
        provider_display_name: "Codex",
        model: "gpt-5.4"
      },
      onboarding_complete: true,
      autonomy: {
        state: "enabled",
        mode: "assisted",
        unlimited_usage: false,
        full_network: false,
        allow_self_edit: false,
        consented_at: null
      },
      evolve: {
        state: "disabled",
        stop_policy: "manual",
        whole_machine_scope: false,
        test_gated: true,
        stage_and_restart: false,
        unlimited_recursion: false,
        current_mission_id: null,
        alias: null,
        requested_model: null,
        iteration: 0,
        last_goal: null,
        last_summary: null,
        last_verified_at: null,
        pending_restart: false,
        diff_review_required: true
      },
      autopilot: {
        state: "disabled",
        max_concurrent_missions: 0,
        wake_interval_seconds: 300,
        allow_background_shell: false,
        allow_background_network: false,
        allow_background_self_edit: false
      },
      delegation: {
        max_depth: { mode: "limited", value: 2 },
        max_parallel_subagents: { mode: "limited", value: 4 },
        disabled_provider_ids: []
      },
      providers: 1,
      aliases: 1,
      plugins: 0,
      delegation_targets: 0,
      webhook_connectors: 0,
      inbox_connectors: 0,
      telegram_connectors: 0,
      discord_connectors: 0,
      slack_connectors: 0,
      home_assistant_connectors: 0,
      signal_connectors: 0,
      gmail_connectors: 0,
      brave_connectors: 0,
      pending_connector_approvals: 0,
      missions: 0,
      active_missions: 0,
      memories: 0,
      pending_memory_reviews: 0,
      skill_drafts: 0,
      published_skills: 0
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
    permissions: "suggest",
    trust: {
      trusted_paths: [],
      allow_shell: true,
      allow_network: false,
      allow_full_disk: false,
      allow_self_edit: false
    },
    delegation_config: {
      max_depth: { mode: "limited", value: 2 },
      max_parallel_subagents: { mode: "limited", value: 4 },
      disabled_provider_ids: []
    },
    provider_capabilities: [],
    remote_content_policy: "allow"
  };
}

function updateFixture(availability: UpdateStatusResponse["availability"]): UpdateStatusResponse {
  return {
    install: {
      kind: "packaged",
      executable_path: "C:\\Nuclear\\nuclear.exe",
      install_dir: "C:\\Nuclear",
      repo_root: null,
      build_profile: null
    },
    current_version: "0.8.3",
    current_commit: null,
    availability,
    checked_at: "2026-04-17T00:05:00Z",
    step: availability === "in_progress" ? "applying" : null,
    candidate_version: "0.8.4",
    candidate_tag: "v0.8.4",
    candidate_commit: null,
    published_at: "2026-04-17T00:00:00Z",
    detail:
      availability === "available"
        ? "0.8.4 is available for windows-x64."
        : "Applying staged package and restarting the daemon.",
    last_run: null
  };
}

function renderSystemPage() {
  const client = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false
      }
    }
  });

  render(
    <QueryClientProvider client={client}>
      <DashboardDataProvider
        bootstrap={bootstrapFixture()}
        onLogout={async () => undefined}
      >
        <SystemPage />
      </DashboardDataProvider>
    </QueryClientProvider>
  );
}

describe("SystemPage updates", () => {
  beforeEach(() => {
    sessionStorage.clear();
    createSupportBundleMock.mockReset();
    fetchDoctorMock.mockReset();
    fetchUpdateStatusMock.mockReset();
    getJsonMock.mockReset();
    postJsonMock.mockReset();
    putJsonMock.mockReset();
    runUpdateMock.mockReset();
    fetchDoctorMock.mockResolvedValue({ providers: [], plugins: [] });
    getJsonMock.mockResolvedValue([]);
  });

  it("runs a manual update check and renders the candidate build", async () => {
    fetchUpdateStatusMock.mockResolvedValue(updateFixture("available"));

    renderSystemPage();
    fireEvent.click(screen.getByRole("button", { name: "updates" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));

    await waitFor(() => {
      expect(fetchUpdateStatusMock).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByText("0.8.4 is available for windows-x64.")).toBeInTheDocument();
    expect(screen.getByText(/tag v0\.8\.4 commit/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update now" })).toBeEnabled();
  });

  it("marks the session as pending when an update starts", async () => {
    fetchUpdateStatusMock.mockResolvedValue(updateFixture("available"));
    runUpdateMock.mockResolvedValue(updateFixture("in_progress"));

    renderSystemPage();
    fireEvent.click(screen.getByRole("button", { name: "updates" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await screen.findByText("0.8.4 is available for windows-x64.");

    fireEvent.click(screen.getByRole("button", { name: "Update now" }));

    await waitFor(() => {
      expect(runUpdateMock).toHaveBeenCalledWith({});
    });
    expect(hasPendingUpdate()).toBe(true);
    expect(screen.getByText("Applying staged package and restarting the daemon.")).toBeInTheDocument();
  });
});
