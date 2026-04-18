import { fireEvent, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router-dom";
import { describe, expect, it } from "vitest";
import type { DashboardBootstrapResponse } from "../../api/types";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import { AppShell } from "./AppShell";

function bootstrapFixture(): DashboardBootstrapResponse {
  return {
    status: {
      pid: 1000,
      started_at: "2026-04-09T00:00:00Z",
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
      delegation_targets: 1,
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
      missions: 1,
      active_missions: 1,
      memories: 2,
      pending_memory_reviews: 1,
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
    events: [
      {
        id: "evt-1",
        level: "info",
        target: "daemon",
        message: "daemon ready",
        created_at: "2026-04-09T00:01:00Z"
      }
    ],
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

function renderShell(initialEntries: string[] = ["/chat"]) {
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: (
          <DashboardDataProvider
            bootstrap={bootstrapFixture()}
            onLogout={async () => undefined}
          >
            <AppShell />
          </DashboardDataProvider>
        ),
        children: [
          { index: true, element: <div>overview-body</div> },
          { path: "chat", element: <div>chat-body</div> },
          { path: "config", element: <div>config-body</div> }
        ]
      }
    ],
    { initialEntries }
  );

  render(<RouterProvider router={router} />);
}

describe("AppShell", () => {
  it("renders the OpenClaw-style shell and parity drawer", () => {
    renderShell();

    expect(screen.getAllByText("Nuclear").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Control").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Chat").length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Search pages" })).toBeInTheDocument();
    expect(screen.getByText("Runtime")).toBeInTheDocument();
    expect(screen.getByText("main · shell")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Open parity gaps" }));

    expect(screen.getByText("OpenClaw delta")).toBeInTheDocument();
    expect(
      screen.getByText("Backed by Nuclear run and session APIs.")
    ).toBeInTheDocument();
    expect(screen.getByText("chat-body")).toBeInTheDocument();
  });

  it("opens quick nav and filters results", () => {
    renderShell(["/chat"]);

    fireEvent.click(screen.getByRole("button", { name: "Search pages" }));

    expect(screen.getByRole("dialog", { name: "Search pages" })).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Search chat, config, logs..."), {
      target: { value: "config" }
    });

    expect(screen.getByRole("button", { name: /config/i })).toBeInTheDocument();
  });
});
