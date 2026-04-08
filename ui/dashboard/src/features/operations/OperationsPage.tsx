import { FormEvent, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useDashboardData } from "../../app/useDashboardData";
import {
  approveConnectorApproval,
  approveMemory,
  cancelMission,
  fetchMission,
  listConnectorApprovals,
  listEvents,
  listLogs,
  listMemories,
  listMemoryReviewQueue,
  listMissionCheckpoints,
  listMissions,
  rebuildMemory,
  rejectConnectorApproval,
  rejectMemory,
  saveMemory,
  saveMission,
  searchMemory
} from "../../api/client";
import type { MemoryRecord, Mission, MissionStatus, SessionTranscriptHit } from "../../api/types";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { WorkbenchTabs } from "../../components/WorkbenchTabs";
import { fmtDate, startCase } from "../../utils/format";
import shellStyles from "../shared/Workbench.module.css";

const OPERATIONS_TABS = [
  { id: "missions", label: "Missions", description: "Queue, schedule, and inspect mission state" },
  { id: "memory", label: "Memory", description: "Rebuild, search, review, and create durable context" },
  { id: "approvals", label: "Approvals", description: "Connector and moderation decisions" },
  { id: "events", label: "Events", description: "Daemon events and operational logs" }
] as const;

type OperationsTabId = (typeof OPERATIONS_TABS)[number]["id"];

function newMission(): Mission {
  const timestamp = new Date().toISOString();
  return {
    id: globalThis.crypto?.randomUUID?.() ?? `mission-${Date.now()}`,
    title: "",
    details: "",
    status: "queued" as MissionStatus,
    created_at: timestamp,
    updated_at: timestamp,
    alias: null,
    requested_model: null,
    session_id: null,
    phase: null,
    handoff_summary: null,
    workspace_key: null,
    watch_path: null,
    watch_recursive: false,
    watch_fingerprint: null,
    wake_trigger: null,
    wake_at: null,
    scheduled_for_at: null,
    repeat_interval_seconds: null,
    repeat_anchor_at: null,
    last_error: null,
    retries: 0,
    max_retries: 3,
    evolve: false
  };
}

function upsertMemoryRecord(current: MemoryRecord[] | undefined, next: MemoryRecord): MemoryRecord[] {
  const records = current ?? [];
  const remaining = records.filter((memory) => memory.id !== next.id);
  return [next, ...remaining];
}

export function OperationsPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<OperationsTabId>("missions");
  const [missionDraft, setMissionDraft] = useState<Mission>(() => newMission());
  const [selectedMissionId, setSelectedMissionId] = useState<string | null>(null);
  const [memoryQuery, setMemoryQuery] = useState("");
  const [rebuildSessionId, setRebuildSessionId] = useState("");
  const [memorySubject, setMemorySubject] = useState("");
  const [memoryContent, setMemoryContent] = useState("");
  const [approvalNote, setApprovalNote] = useState("");

  const missionsQuery = useQuery({
    queryKey: ["missions"],
    queryFn: () => listMissions(100),
    initialData: []
  });
  const selectedMissionQuery = useQuery({
    queryKey: ["mission", selectedMissionId],
    queryFn: () => fetchMission(selectedMissionId!),
    enabled: selectedMissionId !== null
  });
  const missionCheckpointsQuery = useQuery({
    queryKey: ["mission-checkpoints", selectedMissionId],
    queryFn: () => listMissionCheckpoints(selectedMissionId!, 25),
    enabled: selectedMissionId !== null
  });
  const memoriesQuery = useQuery({
    queryKey: ["memories"],
    queryFn: () => listMemories(50)
  });
  const memoryReviewQuery = useQuery({
    queryKey: ["memory-review"],
    queryFn: () => listMemoryReviewQueue(50)
  });
  const approvalsQuery = useQuery({
    queryKey: ["connector-approvals"],
    queryFn: () => listConnectorApprovals(50)
  });
  const eventsQuery = useQuery({
    queryKey: ["events"],
    queryFn: () => listEvents(100),
    initialData: bootstrap.events
  });
  const logsQuery = useQuery({
    queryKey: ["logs"],
    queryFn: () => listLogs(100)
  });

  const saveMissionMutation = useMutation({
    mutationFn: async () => saveMission(missionDraft),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["missions"] });
      setMissionDraft(newMission());
    }
  });
  const missionActionMutation = useMutation({
    mutationFn: async (action: { type: "cancel"; missionId: string }) => cancelMission(action.missionId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["missions"] });
      if (selectedMissionId) {
        await queryClient.invalidateQueries({ queryKey: ["mission", selectedMissionId] });
        await queryClient.invalidateQueries({ queryKey: ["mission-checkpoints", selectedMissionId] });
      }
    }
  });
  const rebuildMemoryMutation = useMutation({
    mutationFn: async () => rebuildMemory({ session_id: rebuildSessionId.trim() || null }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["memories"] }),
        queryClient.invalidateQueries({ queryKey: ["memory-review"] }),
        queryClient.invalidateQueries({ queryKey: ["resume-packet"] })
      ]);
    }
  });
  const memorySearchMutation = useMutation({
    mutationFn: async () => searchMemory({ query: memoryQuery, limit: 25 })
  });
  const saveMemoryMutation = useMutation({
    mutationFn: async () =>
      saveMemory({
        kind: "note",
        scope: "global",
        subject: memorySubject,
        content: memoryContent,
        confidence: 100,
        source_session_id: rebuildSessionId.trim() || undefined,
        tags: ["manual"],
        review_status: "accepted"
      }),
    onSuccess: async (memory) => {
      if (memory.review_status === "candidate") {
        queryClient.setQueryData<MemoryRecord[]>(["memory-review"], (current) =>
          upsertMemoryRecord(current, memory)
        );
      } else {
        queryClient.setQueryData<MemoryRecord[]>(["memories"], (current) =>
          upsertMemoryRecord(current, memory)
        );
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["memories"] }),
        queryClient.invalidateQueries({ queryKey: ["memory-review"] }),
        queryClient.invalidateQueries({ queryKey: ["resume-packet"] })
      ]);
      setMemorySubject("");
      setMemoryContent("");
    }
  });
  const reviewMemoryMutation = useMutation({
    mutationFn: async (action: { type: "approve" | "reject"; memoryId: string }) => {
      if (action.type === "approve") {
        return approveMemory(action.memoryId, { status: "accepted" });
      }
      return rejectMemory(action.memoryId, { status: "rejected" });
    },
    onSuccess: async (memory) => {
      queryClient.setQueryData<MemoryRecord[]>(["memory-review"], (current) =>
        (current ?? []).filter((item) => item.id !== memory.id)
      );
      if (memory.review_status === "accepted") {
        queryClient.setQueryData<MemoryRecord[]>(["memories"], (current) =>
          upsertMemoryRecord(current, memory)
        );
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["memories"] }),
        queryClient.invalidateQueries({ queryKey: ["memory-review"] }),
        queryClient.invalidateQueries({ queryKey: ["resume-packet"] })
      ]);
    }
  });
  const approvalMutation = useMutation({
    mutationFn: async (action: { type: "approve" | "reject"; approvalId: string }) => {
      if (action.type === "approve") {
        return approveConnectorApproval(action.approvalId, approvalNote.trim() || undefined);
      }
      return rejectConnectorApproval(action.approvalId, approvalNote.trim() || undefined);
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["connector-approvals"] });
      setApprovalNote("");
    }
  });

  const missionSummary = selectedMissionQuery.data;
  const searchTranscriptHits: SessionTranscriptHit[] = memorySearchMutation.data?.transcript_hits ?? [];
  const selectedMission = useMemo(
    () => missionsQuery.data?.find((mission) => mission.id === selectedMissionId) ?? null,
    [missionsQuery.data, selectedMissionId]
  );

  return (
    <div className={shellStyles.page} data-testid="modern-operations-page">
      <section className={shellStyles.hero}>
        <div className={shellStyles.heroBlock}>
          <div className={shellStyles.heroEyebrow}>Operations</div>
          <h2 className={shellStyles.heroTitle}>Queues, memory, approvals, and audit streams now live in the product cockpit.</h2>
          <p className={shellStyles.heroCopy}>
            This is the operating table for ongoing missions, durable memory, pending approvals,
            and the daemon event stream that used to stay trapped in the legacy browser UI.
          </p>
        </div>
        <div className={shellStyles.heroActions}>
          <Pill tone="accent">{missionsQuery.data?.length ?? 0} missions</Pill>
          <Pill tone="neutral">{memoryReviewQuery.data?.length ?? 0} pending memory reviews</Pill>
          <Pill tone="warn">{approvalsQuery.data?.length ?? 0} approvals</Pill>
        </div>
      </section>

      <WorkbenchTabs
        tabs={OPERATIONS_TABS.map((tab) => ({ ...tab }))}
        activeTab={activeTab}
        onChange={(tabId) => setActiveTab(tabId as OperationsTabId)}
        testIdPrefix="modern-operations-tab"
      />

      {activeTab === "missions" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Mission queue" title="Current missions" emphasis="accent">
            <div className={shellStyles.list}>
              {(missionsQuery.data ?? []).map((mission) => (
                <button
                  key={mission.id}
                  type="button"
                  className={mission.id === selectedMissionId ? `${shellStyles.listButton} ${shellStyles.listButtonActive}` : shellStyles.listButton}
                  onClick={() => setSelectedMissionId(mission.id)}
                >
                  <strong>{mission.title}</strong>
                  <div className={shellStyles.meta}>{startCase(mission.status)} - {mission.alias ?? "no alias"}</div>
                  <div className={shellStyles.meta}>{fmtDate(mission.updated_at)}</div>
                </button>
              ))}
            </div>
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Create mission" title="Queue new work">
              <form className={shellStyles.stack} onSubmit={(event: FormEvent) => { event.preventDefault(); void saveMissionMutation.mutateAsync(); }}>
                <label className={shellStyles.field}>
                  Title
                  <input className={shellStyles.input} value={missionDraft.title} onChange={(event) => setMissionDraft((current) => ({ ...current, title: event.target.value }))} />
                </label>
                <label className={shellStyles.fieldWide}>
                  Details
                  <textarea className={shellStyles.textarea} value={missionDraft.details} onChange={(event) => setMissionDraft((current) => ({ ...current, details: event.target.value }))} rows={5} />
                </label>
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    Alias
                    <input className={shellStyles.input} value={missionDraft.alias ?? ""} onChange={(event) => setMissionDraft((current) => ({ ...current, alias: event.target.value || null }))} placeholder="main" />
                  </label>
                  <label className={shellStyles.field}>
                    Requested model
                    <input className={shellStyles.input} value={missionDraft.requested_model ?? ""} onChange={(event) => setMissionDraft((current) => ({ ...current, requested_model: event.target.value || null }))} />
                  </label>
                </div>
                <div className={shellStyles.buttonRow}>
                  <button type="submit" className={shellStyles.primaryButton} disabled={saveMissionMutation.isPending || missionDraft.title.trim().length === 0}>
                    {saveMissionMutation.isPending ? "Queueing..." : "Queue mission"}
                  </button>
                </div>
              </form>
            </Surface>

            <Surface eyebrow="Selected mission" title={selectedMission?.title ?? "No mission selected"}>
              {missionSummary ? (
                <div className={shellStyles.stack}>
                  <div className={shellStyles.kvGrid}>
                    <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Status</span><strong className={shellStyles.kvValue}>{startCase(missionSummary.status)}</strong></div>
                    <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Alias</span><strong className={shellStyles.kvValue}>{missionSummary.alias ?? "Unassigned"}</strong></div>
                    <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Workspace</span><strong className={shellStyles.kvValue}>{missionSummary.workspace_key ?? "Default"}</strong></div>
                  </div>
                  <p className={shellStyles.callout}>{missionSummary.details}</p>
                  <div className={shellStyles.buttonRow}>
                    <button type="button" className={shellStyles.dangerButton} onClick={() => void missionActionMutation.mutateAsync({ type: "cancel", missionId: missionSummary.id })}>Cancel mission</button>
                  </div>
                  <div className={shellStyles.list}>
                    {(missionCheckpointsQuery.data ?? []).map((checkpoint) => (
                      <article key={checkpoint.id} className={shellStyles.listCard}>
                        <strong>{startCase(checkpoint.status)}</strong>
                        <div className={shellStyles.meta}>{checkpoint.summary}</div>
                        <div className={shellStyles.meta}>{fmtDate(checkpoint.created_at)}</div>
                      </article>
                    ))}
                  </div>
                </div>
              ) : (
                <p className={shellStyles.empty}>Select a mission to inspect checkpoints and status.</p>
              )}
            </Surface>
          </div>
        </div>
      ) : null}

      {activeTab === "memory" ? (
        <div className={shellStyles.gridTwo}>
          <div className={shellStyles.stack}>
            <Surface eyebrow="Search memory" title="Cross-session recall" emphasis="accent">
              <form id="memory-search-form" className={shellStyles.stack} onSubmit={(event: FormEvent) => { event.preventDefault(); void memorySearchMutation.mutateAsync(); }}>
                <label className={shellStyles.field}>
                  Query
                  <input id="memory-search-query" className={shellStyles.input} value={memoryQuery} onChange={(event) => setMemoryQuery(event.target.value)} />
                </label>
                <div className={shellStyles.buttonRow}>
                  <button type="submit" className={shellStyles.primaryButton} disabled={memorySearchMutation.isPending || memoryQuery.trim().length === 0}>
                    {memorySearchMutation.isPending ? "Searching..." : "Search"}
                  </button>
                </div>
              </form>
              <div className={shellStyles.list} id="memory-search-results">
                {(memorySearchMutation.data?.memories ?? memoriesQuery.data ?? []).map((memory) => (
                  <article key={memory.id} className={shellStyles.listCard}>
                    <strong>{memory.subject}</strong>
                    <div className={shellStyles.meta}>{memory.content}</div>
                    <div className={shellStyles.meta}>{startCase(memory.review_status)} - {memory.kind}</div>
                  </article>
                ))}
                {searchTranscriptHits.map((hit) => (
                  <article key={hit.message_id} className={shellStyles.listCard}>
                    <strong>Transcript hit</strong>
                    <div className={shellStyles.meta}>{hit.preview}</div>
                  </article>
                ))}
              </div>
            </Surface>

            <Surface eyebrow="Review queue" title="Pending memory approvals">
              <div className={shellStyles.list} id="memory-review-queue">
                {(memoryReviewQuery.data ?? []).map((memory) => (
                  <article key={memory.id} className={shellStyles.listCard}>
                    <strong>{memory.subject}</strong>
                    <div className={shellStyles.meta}>{memory.content}</div>
                    <div className={shellStyles.buttonRow}>
                      <button type="button" className={shellStyles.secondaryButton} onClick={() => void reviewMemoryMutation.mutateAsync({ type: "approve", memoryId: memory.id })}>Approve</button>
                      <button type="button" className={shellStyles.dangerButton} onClick={() => void reviewMemoryMutation.mutateAsync({ type: "reject", memoryId: memory.id })}>Reject</button>
                    </div>
                  </article>
                ))}
              </div>
            </Surface>
          </div>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Rebuild" title="Generate fresh memory from a session">
              <form id="memory-rebuild-form" className={shellStyles.stack} onSubmit={(event: FormEvent) => { event.preventDefault(); void rebuildMemoryMutation.mutateAsync(); }}>
                <label className={shellStyles.field}>
                  Session ID
                  <input id="memory-rebuild-session-id" className={shellStyles.input} value={rebuildSessionId} onChange={(event) => setRebuildSessionId(event.target.value)} />
                </label>
                <div className={shellStyles.buttonRow}>
                  <button type="submit" className={shellStyles.primaryButton} disabled={rebuildMemoryMutation.isPending}>
                    {rebuildMemoryMutation.isPending ? "Rebuilding..." : "Rebuild memory"}
                  </button>
                </div>
                {rebuildMemoryMutation.data ? (
                  <p className={shellStyles.bannerSuccess}>
                    Upserted {rebuildMemoryMutation.data.memories_upserted} memories from {rebuildMemoryMutation.data.sessions_scanned} sessions.
                  </p>
                ) : null}
              </form>
            </Surface>

            <Surface eyebrow="Create memory" title="Add an explicit durable fact">
              <form id="memory-create-form" className={shellStyles.stack} onSubmit={(event: FormEvent) => { event.preventDefault(); void saveMemoryMutation.mutateAsync(); }}>
                <label className={shellStyles.field}>
                  Subject
                  <input id="memory-create-subject" className={shellStyles.input} value={memorySubject} onChange={(event) => setMemorySubject(event.target.value)} />
                </label>
                <label className={shellStyles.fieldWide}>
                  Content
                  <textarea id="memory-create-content" className={shellStyles.textarea} value={memoryContent} onChange={(event) => setMemoryContent(event.target.value)} rows={5} />
                </label>
                <div className={shellStyles.buttonRow}>
                  <button type="submit" className={shellStyles.primaryButton} disabled={saveMemoryMutation.isPending || memorySubject.trim().length === 0 || memoryContent.trim().length === 0}>
                    {saveMemoryMutation.isPending ? "Saving..." : "Save memory"}
                  </button>
                </div>
                {saveMemoryMutation.data ? (
                  <p className={shellStyles.bannerSuccess}>
                    Saved memory "{saveMemoryMutation.data.subject}" as {startCase(saveMemoryMutation.data.review_status)}.
                  </p>
                ) : null}
                {saveMemoryMutation.error ? (
                  <p className={shellStyles.bannerError}>
                    {saveMemoryMutation.error instanceof Error ? saveMemoryMutation.error.message : "Memory save failed."}
                  </p>
                ) : null}
              </form>
            </Surface>
          </div>
        </div>
      ) : null}

      {activeTab === "approvals" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Pending approvals" title="Connector moderation" emphasis="accent">
            <div className={shellStyles.list}>
              {(approvalsQuery.data ?? []).map((approval) => (
                <article key={approval.id} className={shellStyles.listCard}>
                  <strong>{approval.title}</strong>
                  <div className={shellStyles.meta}>{approval.connector_name} - {approval.connector_kind}</div>
                  <div className={shellStyles.meta}>{approval.details}</div>
                  <div className={shellStyles.buttonRow}>
                    <button type="button" className={shellStyles.secondaryButton} onClick={() => void approvalMutation.mutateAsync({ type: "approve", approvalId: approval.id })}>Approve</button>
                    <button type="button" className={shellStyles.dangerButton} onClick={() => void approvalMutation.mutateAsync({ type: "reject", approvalId: approval.id })}>Reject</button>
                  </div>
                </article>
              ))}
            </div>
          </Surface>
          <Surface eyebrow="Moderator note" title="Decision note">
            <label className={shellStyles.fieldWide}>
              Note
              <textarea className={shellStyles.textarea} value={approvalNote} onChange={(event) => setApprovalNote(event.target.value)} rows={6} />
            </label>
          </Surface>
        </div>
      ) : null}

      {activeTab === "events" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Events" title="Daemon event stream" emphasis="accent">
            <div className={shellStyles.list}>
              {(eventsQuery.data ?? []).map((event) => (
                <article key={event.id} className={shellStyles.listCard}>
                  <strong>{event.scope}</strong>
                  <div className={shellStyles.meta}>{event.message}</div>
                  <div className={shellStyles.meta}>{fmtDate(event.created_at)}</div>
                </article>
              ))}
            </div>
          </Surface>
          <Surface eyebrow="Logs" title="Operational log stream">
            <div className={shellStyles.list}>
              {(logsQuery.data ?? []).map((entry) => (
                <article key={entry.id} className={shellStyles.listCard}>
                  <div className={shellStyles.pillRow}>
                    <Pill tone="neutral">{entry.level}</Pill>
                    <Pill tone="accent">{entry.scope}</Pill>
                  </div>
                  <div className={shellStyles.meta}>{entry.message}</div>
                  <div className={shellStyles.meta}>{fmtDate(entry.created_at)}</div>
                </article>
              ))}
            </div>
          </Surface>
        </div>
      ) : null}
    </div>
  );
}
