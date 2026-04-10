import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { deleteJson, getJson, postJson, putJson } from "../../api/client";
import type {
  ConnectorApprovalRecord,
  LogEntry,
  MemoryRecord,
  MemoryRebuildResponse,
  MemorySearchResponse,
  Mission,
  SkillDraft
} from "../../api/types";
import { Panel } from "../../components/Panel";
import { ApprovalsTab } from "./tabs/ApprovalsTab";
import { EventsTab } from "./tabs/EventsTab";
import { MemoryTab } from "./tabs/MemoryTab";
import { MissionsTab } from "./tabs/MissionsTab";
import { SkillsTab } from "./tabs/SkillsTab";

type OpsTab = "missions" | "approvals" | "memory" | "skills" | "events";

export function OperationsPage() {
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<OpsTab>("missions");
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
  const skillDraftsQuery = useQuery({
    queryKey: ["skill-drafts"],
    queryFn: () => getJson<SkillDraft[]>("/v1/skills/drafts")
  });
  const skillsQuery = useQuery({
    queryKey: ["enabled-skills"],
    queryFn: () => getJson<string[]>("/v1/skills")
  });
  const eventsQuery = useQuery({
    queryKey: ["events"],
    queryFn: () => getJson<LogEntry[]>("/v1/events?limit=80")
  });

  async function refreshOps() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["missions"] }),
      queryClient.invalidateQueries({ queryKey: ["connector-approvals"] }),
      queryClient.invalidateQueries({ queryKey: ["memory-review"] }),
      queryClient.invalidateQueries({ queryKey: ["profile-memory"] }),
      queryClient.invalidateQueries({ queryKey: ["skill-drafts"] }),
      queryClient.invalidateQueries({ queryKey: ["enabled-skills"] }),
      queryClient.invalidateQueries({ queryKey: ["events"] })
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
    await refreshOps();
  }

  async function updateMission(missionId: string, action: "pause" | "resume" | "cancel") {
    await postJson(`/v1/missions/${encodeURIComponent(missionId)}/${action}`, {
      note: `dashboard ${action}`
    });
    await refreshOps();
  }

  async function decideApproval(approvalId: string, action: "approve" | "reject") {
    await postJson(`/v1/connector-approvals/${encodeURIComponent(approvalId)}/${action}`, {
      note: `dashboard ${action}`
    });
    await refreshOps();
  }

  async function decideMemory(memoryId: string, action: "approve" | "reject") {
    await postJson(`/v1/memory/${encodeURIComponent(memoryId)}/${action}`, {
      status: action === "approve" ? "accepted" : "rejected",
      note: `dashboard ${action}`
    });
    await refreshOps();
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
    await refreshOps();
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
      `Rebuilt memory from ${result.sessions_scanned} session(s), refreshed ${result.embeddings_refreshed} embedding set(s).`
    );
    await refreshOps();
  }

  async function publishDraft(draftId: string, action: "publish" | "reject") {
    await postJson(`/v1/skills/drafts/${encodeURIComponent(draftId)}/${action}`, {});
    await refreshOps();
  }

  async function updateEnabledSkills(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const raw = new FormData(event.currentTarget)
      .get("skills")
      ?.toString()
      .split(",")
      .map((entry) => entry.trim())
      .filter(Boolean);
    await putJson("/v1/skills", { enabled_skills: raw || [] });
    await refreshOps();
  }

  return (
    <>
      <Panel eyebrow="Operations" title="Mission control deck">
        <div className="toolbar">
          <div className="toolbar__title">
            <strong>Execution workbench</strong>
            <span>Manage missions, approvals, memory review, skills, and event flow.</span>
          </div>
          <div className="subtabs">
            {(["missions", "approvals", "memory", "skills", "events"] as OpsTab[]).map(
              (tab) => (
                <button
                  key={tab}
                  type="button"
                  className={activeTab === tab ? "is-active" : undefined}
                  onClick={() => setActiveTab(tab)}
                >
                  {tab}
                </button>
              )
            )}
          </div>
        </div>

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
              void deleteJson(`/v1/memory/${encodeURIComponent(memoryId)}`).then(refreshOps);
            }}
          />
        ) : null}

        {activeTab === "skills" ? (
          <SkillsTab
            enabledSkills={skillsQuery.data}
            profileMemories={profileQuery.data}
            skillDrafts={skillDraftsQuery.data}
            onUpdateSkills={updateEnabledSkills}
            onPublishDraft={(draftId, action) => {
              void publishDraft(draftId, action);
            }}
          />
        ) : null}

        {activeTab === "events" ? (
          <EventsTab events={eventsQuery.data} />
        ) : null}
      </Panel>
    </>
  );
}
