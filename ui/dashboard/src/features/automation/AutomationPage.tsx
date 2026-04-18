import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteJson, getJson, postJson, putJson } from "../../api/client";
import type {
  ConnectorApprovalRecord,
  MemoryRecord,
  MemoryRebuildResponse,
  MemorySearchResponse,
  Mission
} from "../../api/types";
import { useSystemBootstrap } from "../../app/dashboard-selectors";
import { ApprovalsTab } from "../operations/tabs/ApprovalsTab";
import { MemoryTab } from "../operations/tabs/MemoryTab";
import { MissionsTab } from "../operations/tabs/MissionsTab";
import { DaemonTab } from "../system/tabs/DaemonTab";

type AutomationTab = "missions" | "approvals" | "memory" | "runtime";

export function AutomationPage() {
  const { status } = useSystemBootstrap();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<AutomationTab>("missions");
  const [memorySearchResults, setMemorySearchResults] = useState<MemorySearchResponse | null>(
    null
  );
  const missionsQuery = useQuery({
    queryKey: ["missions"],
    queryFn: () => getJson<Mission[]>("/v1/missions")
  });
  const approvalsQuery = useQuery({
    queryKey: ["connector-approvals"],
    queryFn: () => getJson<ConnectorApprovalRecord[]>("/v1/connector-approvals")
  });
  const memoryReviewQuery = useQuery({
    queryKey: ["memory-review"],
    queryFn: () => getJson<MemoryRecord[]>("/v1/memory/review?limit=50")
  });
  const profileQuery = useQuery({
    queryKey: ["profile-memory"],
    queryFn: () => getJson<MemoryRecord[]>("/v1/memory/profile?limit=25")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["missions"] }),
      queryClient.invalidateQueries({ queryKey: ["connector-approvals"] }),
      queryClient.invalidateQueries({ queryKey: ["memory-review"] }),
      queryClient.invalidateQueries({ queryKey: ["profile-memory"] }),
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] })
    ]);
  }

  async function addMission(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    const title = String(form.get("title") || "").trim();
    const details = String(form.get("details") || "").trim();
    const alias = String(form.get("alias") || "").trim();
    const requestedModel = String(form.get("model") || "").trim();
    const afterSeconds = Number(form.get("after_seconds") || 0);
    const everySeconds = Number(form.get("every_seconds") || 0);
    const watchPath = String(form.get("watch_path") || "").trim();
    const mission: Record<string, unknown> = {
      title,
      details,
      alias: alias || null,
      requested_model: requestedModel || null,
      workspace_key: null
    };
    if (watchPath) {
      mission.watch_path = watchPath;
      mission.watch_recursive = true;
      mission.status = "waiting";
      mission.wake_trigger = "file_change";
    } else if (afterSeconds > 0 || everySeconds > 0) {
      mission.status = "scheduled";
      mission.wake_trigger = "timer";
      if (afterSeconds > 0) {
        mission.wake_at = new Date(Date.now() + afterSeconds * 1000).toISOString();
      }
      if (everySeconds > 0) {
        mission.repeat_interval_seconds = everySeconds;
      }
    }
    await postJson("/v1/missions", mission);
    formElement.reset();
    await refresh();
  }

  async function updateMission(missionId: string, action: "pause" | "resume" | "cancel") {
    await postJson(`/v1/missions/${encodeURIComponent(missionId)}/${action}`, {
      note: `dashboard ${action}`
    });
    await refresh();
  }

  async function decideApproval(approvalId: string, action: "approve" | "reject") {
    await postJson(`/v1/connector-approvals/${encodeURIComponent(approvalId)}/${action}`, {
      note: `dashboard ${action}`
    });
    await refresh();
  }

  async function decideMemory(memoryId: string, action: "approve" | "reject") {
    await postJson(`/v1/memory/${encodeURIComponent(memoryId)}/${action}`, {
      status: action === "approve" ? "accepted" : "rejected",
      note: `dashboard ${action}`
    });
    await refresh();
  }

  async function createMemory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    await postJson("/v1/memory", {
      kind: String(form.get("kind") || "note"),
      scope: String(form.get("scope") || "workspace"),
      subject: String(form.get("subject") || "").trim(),
      content: String(form.get("content") || "").trim(),
      confidence: 100,
      workspace_key: null,
      evidence_refs: [],
      tags: ["dashboard", "manual"],
      review_status: "accepted"
    });
    formElement.reset();
    await refresh();
  }

  async function searchMemory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const query = String(form.get("query") || "").trim();
    setMemorySearchResults(
      await postJson<MemorySearchResponse>("/v1/memory/search", {
        query,
        limit: 20,
        workspace_key: null,
        provider_id: null,
        review_statuses: [],
        include_superseded: false
      })
    );
  }

  async function rebuildMemory(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const result = await postJson<MemoryRebuildResponse>("/v1/memory/rebuild", {
      session_id: String(form.get("session_id") || "").trim() || null,
      recompute_embeddings: Boolean(form.get("recompute_embeddings"))
    });
    window.alert(
      `Rebuilt ${result.sessions_scanned} sessions, refreshed ${result.embeddings_refreshed} embeddings.`
    );
    await refresh();
  }

  async function updateDaemon(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/daemon/config", {
      persistence_mode: form.get("persistence_mode"),
      auto_start: Boolean(form.get("auto_start"))
    });
    await refresh();
  }

  async function updateAutonomy(mode: "enable" | "pause" | "resume") {
    const path = mode === "enable" ? "/v1/autonomy/enable" : `/v1/autonomy/${mode}`;
    await postMode(path);
    await refresh();
  }

  async function updateEvolve(mode: "start" | "pause" | "resume" | "stop") {
    const payload = mode === "start" ? { alias: null, requested_model: null } : {};
    await putOrPost(`/v1/evolve/${mode}`, payload);
    await refresh();
  }

  async function updateAutopilot(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/autopilot/status", {
      state: form.get("state"),
      max_concurrent_missions: Number(form.get("max_concurrent_missions") || 1),
      wake_interval_seconds: Number(form.get("wake_interval_seconds") || 30),
      allow_background_shell: Boolean(form.get("allow_background_shell")),
      allow_background_network: Boolean(form.get("allow_background_network")),
      allow_background_self_edit: Boolean(form.get("allow_background_self_edit"))
    });
    await refresh();
  }

  return (
    <div className="page-stack">
      <section className="route-tabs" aria-label="Automation sections">
        {(["missions", "approvals", "memory", "runtime"] as AutomationTab[]).map((tab) => (
          <button
            key={tab}
            type="button"
            className={activeTab === tab ? "is-active" : undefined}
            onClick={() => setActiveTab(tab)}
          >
            {tab}
          </button>
        ))}
      </section>
      {activeTab === "missions" ? (
        <MissionsTab
          missions={missionsQuery.data}
          onSubmit={addMission}
          onUpdateMission={(missionId, action) => {
            void updateMission(missionId, action);
          }}
        />
      ) : null}
      {activeTab === "approvals" ? (
        <ApprovalsTab
          approvals={approvalsQuery.data}
          onDecide={(approvalId, action) => {
            void decideApproval(approvalId, action);
          }}
        />
      ) : null}
      {activeTab === "memory" ? (
        <>
          <MemoryTab
            reviewMemories={memoryReviewQuery.data}
            memorySearchResults={memorySearchResults}
            onDecideMemory={(memoryId, action) => {
              void decideMemory(memoryId, action);
            }}
            onSearch={searchMemory}
            onCreate={createMemory}
            onRebuild={rebuildMemory}
            onForget={(memoryId) => {
              void deleteJson(`/v1/memory/${encodeURIComponent(memoryId)}`).then(refresh);
            }}
          />
          <div className="memory-sideband">
            <span className="memory-sideband__label">profile memory</span>
            <span>{profileQuery.data?.length || 0}</span>
          </div>
        </>
      ) : null}
      {activeTab === "runtime" ? (
        <DaemonTab
          status={status}
          onUpdateDaemon={updateDaemon}
          onUpdateAutonomy={(mode) => {
            void updateAutonomy(mode);
          }}
          onUpdateEvolve={(mode) => {
            void updateEvolve(mode);
          }}
          onUpdateAutopilot={updateAutopilot}
        />
      ) : null}
    </div>
  );
}

async function postMode(path: string) {
  await putOrPost(path, { allow_self_edit: false, mode: "assisted" });
}

async function putOrPost(path: string, payload: Record<string, unknown>) {
  const method = path.includes("/status") ? putJson : postJson;
  await method(path, payload);
}
