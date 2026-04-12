import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { DashboardDataProvider } from "../../app/DashboardDataContext";
import type {
  DashboardBootstrapResponse,
  SessionResumePacket,
  SessionTranscript
} from "../../api/types";
import { ChatPage } from "./ChatPage";

const getJsonMock = vi.fn();
const postJsonMock = vi.fn();
const putJsonMock = vi.fn();

vi.mock("../../api/client", () => ({
  getJson: (...args: unknown[]) => getJsonMock(...args),
  postJson: (...args: unknown[]) => postJsonMock(...args),
  putJson: (...args: unknown[]) => putJsonMock(...args)
}));

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
    aliases: [
      {
        alias: "main",
        provider_id: "codex",
        model: "gpt-5.4",
        description: "Primary target"
      }
    ],
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
    sessions: [
      {
        id: "session-1",
        title: "Saved session",
        alias: "main",
        provider_id: "codex",
        model: "gpt-5.4",
        cwd: "J:\\repo",
        task_mode: "daily",
        created_at: "2026-04-09T00:00:00Z",
        updated_at: "2026-04-09T00:01:00Z"
      }
    ],
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

function transcriptFixture(): SessionTranscript {
  return {
    session: bootstrapFixture().sessions[0],
    messages: [
      {
        id: "msg-1",
        role: "user",
        content: "Saved context",
        created_at: "2026-04-09T00:00:30Z"
      }
    ]
  };
}

function resumePacketFixture(): SessionResumePacket {
  return {
    session: bootstrapFixture().sessions[0],
    generated_at: "2026-04-09T00:01:10Z",
    recent_messages: transcriptFixture().messages,
    linked_memories: [],
    related_transcript_hits: []
  };
}

function renderChatPage() {
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
        <ChatPage />
      </DashboardDataProvider>
    </QueryClientProvider>
  );
}

describe("ChatPage", () => {
  beforeEach(() => {
    getJsonMock.mockReset();
    postJsonMock.mockReset();
    putJsonMock.mockReset();
    getJsonMock.mockImplementation(async (path: string) => {
      if (path.includes("/resume-packet")) {
        return resumePacketFixture();
      }
      return transcriptFixture();
    });
  });

  it("clears staged draft-only run state when opening an existing session", async () => {
    renderChatPage();

    fireEvent.click(screen.getByText("Stage files and images into the next run"));
    fireEvent.change(screen.getByLabelText("Attachment path"), {
      target: { value: "J:\\assets\\reference.png" }
    });
    fireEvent.click(screen.getByRole("button", { name: "Add attachment" }));

    fireEvent.click(
      screen.getByText("Working directory, task mode, permissions, and remote access")
    );
    fireEvent.click(screen.getByLabelText("Ephemeral run"));

    fireEvent.click(screen.getByRole("button", { name: "Open" }));

    await waitFor(() => {
      expect(screen.getByText("Saved context")).toBeInTheDocument();
    });

    expect(screen.getByLabelText("Attachment path")).toHaveValue("");
    expect(screen.getByText("No attachments")).toBeInTheDocument();
    expect(screen.getByLabelText("Ephemeral run")).not.toBeChecked();
    expect(screen.getByLabelText("Attachment kind")).toHaveValue("file");
  });

  it("clears staged draft-only run state when starting a new session", async () => {
    renderChatPage();

    fireEvent.click(screen.getByText("Stage files and images into the next run"));
    fireEvent.change(screen.getByLabelText("Attachment kind"), {
      target: { value: "image" }
    });
    fireEvent.change(screen.getByLabelText("Attachment path"), {
      target: { value: "J:\\assets\\reference.png" }
    });
    fireEvent.click(screen.getByRole("button", { name: "Add attachment" }));

    fireEvent.click(
      screen.getByText("Working directory, task mode, permissions, and remote access")
    );
    fireEvent.click(screen.getByLabelText("Ephemeral run"));

    fireEvent.click(screen.getByRole("button", { name: "New session" }));

    expect(screen.getByText("No attachments")).toBeInTheDocument();
    expect(screen.getByLabelText("Attachment path")).toHaveValue("");
    expect(screen.getByLabelText("Ephemeral run")).not.toBeChecked();
    expect(screen.getByLabelText("Attachment kind")).toHaveValue("file");
  });
});
