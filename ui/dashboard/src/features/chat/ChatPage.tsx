import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useDashboardData } from "../../app/useDashboardData";
import {
  compactSession,
  fetchSessionResumePacket,
  fetchSessionTranscript,
  forkSession,
  listSessions,
  renameSession,
  updateMainAlias
} from "../../api/client";
import { controlRunTask } from "../../api/controlSocket";
import type {
  InputAttachment,
  MemoryRecord,
  PermissionPreset,
  RemoteContentArtifact,
  RunTaskStreamEvent,
  SessionMessage,
  SessionSummary
} from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { WorkbenchTabs } from "../../components/WorkbenchTabs";
import { useDashboardStore } from "../../store/dashboardStore";
import { fmtDate, startCase } from "../../utils/format";
import shellStyles from "../shared/Workbench.module.css";
import { type ChatConsoleEntry, executeChatConsoleInput } from "./chatConsole";
import styles from "./ChatPage.module.css";

const CHAT_TABS = [
  { id: "run", label: "Run", description: "Compose, stream, and inspect live tasks" },
  { id: "sessions", label: "Sessions", description: "Resume, fork, compact, and rename history" },
  { id: "context", label: "Context", description: "Resume packets, memories, and remote artifacts" }
] as const;

type ChatTabId = (typeof CHAT_TABS)[number]["id"];

function inferAttachmentKind(path: string): InputAttachment["kind"] {
  return /\.(png|jpe?g|gif|webp|bmp|svg)$/i.test(path) ? "image" : "file";
}

function runResultFromMessages(messages: SessionMessage[]) {
  return [...messages]
    .reverse()
    .find((message) => message.role === "assistant" && message.content.trim().length > 0)?.content;
}

function remoteArtifactTitle(artifact: RemoteContentArtifact) {
  return artifact.source.label ?? artifact.title ?? artifact.source.url ?? artifact.source.kind;
}

function remoteArtifactSubtitle(artifact: RemoteContentArtifact) {
  const parts = [
    artifact.title && artifact.title !== artifact.source.label ? artifact.title : null,
    artifact.source.host,
    artifact.source.url
  ].filter((value): value is string => Boolean(value && value.trim().length > 0));
  return parts.join(" · ");
}

function formatMemoryEvidence(memory: MemoryRecord, limit = 2) {
  const refs = Array.isArray(memory.evidence_refs) ? memory.evidence_refs : [];
  if (!refs.length) {
    return memory.source_session_id ? `source session ${memory.source_session_id}` : "no evidence refs";
  }
  const parts = refs.slice(0, limit).map((ref) => {
    const pieces = [];
    if (ref.session_id) {
      pieces.push(`session ${ref.session_id}`);
    }
    if (ref.message_id) {
      pieces.push(`msg ${ref.message_id}`);
    }
    if (ref.summary) {
      pieces.push(ref.summary);
    }
    return pieces.join(" / ") || "evidence";
  });
  if (refs.length > limit) {
    parts.push(`+${refs.length - limit} more`);
  }
  return parts.join(" | ");
}

function renderMemoryProvenance(memory: MemoryRecord) {
  const items = [];
  if (memory.observation_source) {
    items.push(`observation: ${memory.observation_source}`);
  }
  if (memory.source_session_id) {
    items.push(`session: ${memory.source_session_id}`);
  }
  if (memory.source_message_id) {
    items.push(`message: ${memory.source_message_id}`);
  }
  items.push(`evidence: ${formatMemoryEvidence(memory)}`);
  return items;
}

export function ChatPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const { selectedSessionId, setSelectedSessionId, setActiveRunSessionId } = useDashboardStore();
  const [activeTab, setActiveTab] = useState<ChatTabId>("run");
  const [prompt, setPrompt] = useState("");
  const [alias, setAlias] = useState(
    bootstrap.status.main_agent_alias ?? bootstrap.aliases[0]?.alias ?? ""
  );
  const [requestedModel, setRequestedModel] = useState("");
  const [taskMode, setTaskMode] = useState<"" | "build" | "daily">("");
  const [permissionPreset, setPermissionPreset] = useState<"" | PermissionPreset>("");
  const [attachmentPath, setAttachmentPath] = useState("");
  const [attachments, setAttachments] = useState<InputAttachment[]>([]);
  const [streamMessages, setStreamMessages] = useState<SessionMessage[]>([]);
  const [remoteArtifacts, setRemoteArtifacts] = useState<RemoteContentArtifact[]>([]);
  const [chatCwd, setChatCwd] = useState("");
  const [consoleEntries, setConsoleEntries] = useState<ChatConsoleEntry[]>([]);
  const [commandResult, setCommandResult] = useState<string | null>(null);
  const [lastRunMeta, setLastRunMeta] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: () => listSessions(50),
    initialData: bootstrap.sessions
  });

  const transcriptQuery = useQuery({
    queryKey: ["session", selectedSessionId],
    queryFn: () => fetchSessionTranscript(selectedSessionId!),
    enabled: Boolean(selectedSessionId)
  });

  const resumePacketQuery = useQuery({
    queryKey: ["resume-packet", selectedSessionId],
    queryFn: () => fetchSessionResumePacket(selectedSessionId!),
    enabled: Boolean(selectedSessionId)
  });

  useEffect(() => {
    if (!selectedSessionId) {
      return;
    }
    const selected = sessionsQuery.data?.find((session) => session.id === selectedSessionId);
    if (selected?.task_mode) {
      setTaskMode(selected.task_mode);
    }
    if (selected?.alias) {
      setAlias(selected.alias);
    }
  }, [selectedSessionId, sessionsQuery.data]);

  const selectedSession = useMemo(
    () => sessionsQuery.data?.find((session) => session.id === selectedSessionId) ?? null,
    [selectedSessionId, sessionsQuery.data]
  );

  const mergedMessages = useMemo(() => {
    const persisted = transcriptQuery.data?.messages ?? [];
    const deduped = new Map<string, SessionMessage>();
    for (const message of [...persisted, ...streamMessages]) {
      deduped.set(message.id, message);
    }
    return Array.from(deduped.values()).sort((left, right) => left.created_at.localeCompare(right.created_at));
  }, [transcriptQuery.data?.messages, streamMessages]);

  const runResult = useMemo(() => runResultFromMessages(mergedMessages), [mergedMessages]);

  function pushConsoleEntry(entry: ChatConsoleEntry) {
    setConsoleEntries((current) => [entry, ...current].slice(0, 24));
    setCommandResult(entry.body);
  }

  function startNewSession() {
    setSelectedSessionId(null);
    setActiveRunSessionId(null);
    setStreamMessages([]);
    setRemoteArtifacts([]);
    setLastRunMeta(null);
    setRunError(null);
  }

  function handleStreamEvent(event: RunTaskStreamEvent) {
    if (event.type === "message") {
      setStreamMessages((current) => {
        const next = new Map(current.map((message) => [message.id, message]));
        next.set(event.message.id, event.message);
        return Array.from(next.values());
      });
    }
    if (event.type === "remote_content") {
      setRemoteArtifacts((current) => [...current, event.artifact]);
    }
    if (event.type === "session_started") {
      setSelectedSessionId(event.session_id);
      setActiveRunSessionId(event.session_id);
      setLastRunMeta(`${event.alias} · ${event.model}`);
    }
    if (event.type === "error") {
      setRunError(event.message);
    }
  }

  useEffect(() => {
    const debugWindow = window as Window & {
      nuclearDashboardDebug?: {
        emitChatStreamEvent?: (event: RunTaskStreamEvent) => void;
      };
    };
    debugWindow.nuclearDashboardDebug = {
      ...(debugWindow.nuclearDashboardDebug ?? {}),
      emitChatStreamEvent: handleStreamEvent
    };
  });

  const runTaskMutation = useMutation({
    mutationFn: async () => {
      setStreamMessages([]);
      setRemoteArtifacts([]);
      setLastRunMeta(null);
      setRunError(null);
      setCommandResult(null);
      const response = await controlRunTask(
        {
          prompt,
          alias: alias || undefined,
          requested_model: requestedModel || undefined,
          session_id: selectedSessionId ?? undefined,
          cwd: chatCwd || undefined,
          task_mode: taskMode || undefined,
          permission_preset: permissionPreset || undefined,
          attachments
        },
        handleStreamEvent
      );
      return response;
    },
    onSuccess: async (response) => {
      setSelectedSessionId(response.session_id);
      setActiveRunSessionId(response.session_id);
      setLastRunMeta(`${response.alias} · ${response.model}`);
      setPrompt("");
      setAttachments([]);
      setAttachmentPath("");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["sessions"] }),
        queryClient.invalidateQueries({ queryKey: ["session", response.session_id] }),
        queryClient.invalidateQueries({ queryKey: ["resume-packet", response.session_id] })
      ]);
    },
    onError: (error) => {
      setRunError(error instanceof Error ? error.message : "Task failed.");
    }
  });

  const renameMutation = useMutation({
    mutationFn: async () => {
      if (!selectedSessionId) {
        throw new Error("Select a session first.");
      }
      return renameSession(selectedSessionId, { title: renameValue });
    },
    onSuccess: async () => {
      setRenameValue("");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["sessions"] }),
        queryClient.invalidateQueries({ queryKey: ["session", selectedSessionId] })
      ]);
    }
  });

  const forkMutation = useMutation({
    mutationFn: async () => {
      if (!selectedSessionId) {
        throw new Error("Select a session first.");
      }
      return forkSession(selectedSessionId, {});
    },
    onSuccess: async (response) => {
      setSelectedSessionId(response.session.id);
      setActiveTab("sessions");
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    }
  });

  const compactMutation = useMutation({
    mutationFn: async () => {
      if (!selectedSessionId) {
        throw new Error("Select a session first.");
      }
      return compactSession(selectedSessionId, {
        alias: alias || undefined,
        task_mode: taskMode || undefined
      });
    },
    onSuccess: async (response) => {
      setSelectedSessionId(response.session.id);
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    }
  });

  const makeMainMutation = useMutation({
    mutationFn: async () => updateMainAlias(alias),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
    }
  });

  function addAttachment() {
    const path = attachmentPath.trim();
    addAttachmentPath(path);
    setAttachmentPath("");
  }

  function addAttachmentPath(path: string) {
    if (!path) {
      return;
    }
    setAttachments((current) => [
      ...current,
      {
        kind: inferAttachmentKind(path),
        path
      }
    ]);
  }

  function selectSession(session: SessionSummary) {
    setSelectedSessionId(session.id);
    setActiveRunSessionId(session.id);
    setRenameValue(session.title ?? "");
    void Promise.all([
      queryClient.invalidateQueries({ queryKey: ["session", session.id] }),
      queryClient.invalidateQueries({ queryKey: ["resume-packet", session.id] })
    ]);
  }

  async function handleConsoleInput(input: string) {
    const result = await executeChatConsoleInput(input, {
      state: {
        bootstrap,
        sessions: sessionsQuery.data ?? [],
        selectedSession,
        alias,
        requestedModel,
        taskMode,
        permissionPreset,
        attachments,
        cwd: chatCwd
      },
      actions: {
        setAlias,
        setRequestedModel,
        setTaskMode,
        setPermissionPreset,
        addAttachment: addAttachmentPath,
        clearAttachments: () => setAttachments([]),
        startNewSession,
        setCwd: setChatCwd
      }
    });
    if (result.handled && result.entry) {
      pushConsoleEntry(result.entry);
      setPrompt("");
    }
    return result.handled;
  }

  return (
    <div className={shellStyles.page} data-testid="modern-chat-page">
      <section className={shellStyles.hero}>
        <div className={shellStyles.heroBlock}>
          <div className={shellStyles.heroEyebrow}>Chat</div>
          <h2 className={shellStyles.heroTitle}>Flagship operator surface for live task execution and session control.</h2>
          <p className={shellStyles.heroCopy}>
            Run tasks through the live control socket, inspect streamed output, and manage saved
            sessions without leaving the cockpit.
          </p>
        </div>
        <div className={shellStyles.heroActions}>
          <Pill tone="accent">{alias || "No alias selected"}</Pill>
          <Pill tone="neutral">{taskMode ? startCase(taskMode) : "Standard mode"}</Pill>
          <Pill tone={selectedSessionId ? "good" : "warn"}>
            {selectedSessionId ? "Session attached" : "New session"}
          </Pill>
        </div>
      </section>

      <WorkbenchTabs
        tabs={CHAT_TABS.map((tab) => ({ ...tab }))}
        activeTab={activeTab}
        onChange={(tabId) => setActiveTab(tabId as ChatTabId)}
        testIdPrefix="modern-chat-tab"
      />

      {activeTab === "run" ? (
        <div className={styles.runGrid}>
          <Surface eyebrow="Sessions" title="Conversation rail">
            <div className={styles.sessionRail} data-testid="modern-chat-sessions">
              {(sessionsQuery.data ?? []).map((session) => (
                <button
                  key={session.id}
                  type="button"
                  className={
                    session.id === selectedSessionId
                      ? `${shellStyles.listButton} ${shellStyles.listButtonActive}`
                      : shellStyles.listButton
                  }
                  onClick={() => selectSession(session)}
                  data-testid={`modern-session-${session.id}`}
                >
                  <strong>{session.title ?? "Untitled session"}</strong>
                  <div className={shellStyles.meta}>
                    {session.alias} · {session.model}
                  </div>
                  <div className={shellStyles.meta}>
                    {session.task_mode ? `${startCase(session.task_mode)} · ` : ""}
                    {fmtDate(session.updated_at)}
                  </div>
                </button>
              ))}
            </div>
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Run task" title={lastRunMeta ?? "Prompt composer"} emphasis="accent">
              <form
                className={shellStyles.stack}
                onSubmit={async (event) => {
                  event.preventDefault();
                  const input = prompt.trim();
                  if (!input) {
                    return;
                  }
                  try {
                    const handled = await handleConsoleInput(input);
                    if (!handled) {
                      void runTaskMutation.mutateAsync();
                    }
                  } catch (error) {
                    setRunError(error instanceof Error ? error.message : "Command failed.");
                  }
                }}
              >
                <div className={shellStyles.formGrid}>
                  <label className={shellStyles.field}>
                    Alias
                    <select
                      className={shellStyles.select}
                      value={alias}
                      onChange={(event) => setAlias(event.target.value)}
                      data-testid="modern-chat-alias"
                      id="run-task-alias"
                    >
                      {bootstrap.aliases.map((item) => (
                        <option key={item.alias} value={item.alias}>
                          {item.alias} · {item.model}
                        </option>
                      ))}
                    </select>
                  </label>

                  <label className={shellStyles.field}>
                    Model override
                    <input
                      className={shellStyles.input}
                      value={requestedModel}
                      onChange={(event) => setRequestedModel(event.target.value)}
                      placeholder="Optional explicit model"
                      id="run-task-model"
                    />
                  </label>

                  <label className={shellStyles.field}>
                    Task mode
                    <select
                      className={shellStyles.select}
                      value={taskMode}
                      onChange={(event) => setTaskMode(event.target.value as "" | "build" | "daily")}
                      data-testid="modern-chat-mode"
                      id="run-task-mode"
                    >
                      <option value="">Standard</option>
                      <option value="build">Build</option>
                      <option value="daily">Daily</option>
                    </select>
                  </label>

                  <label className={shellStyles.field}>
                    Permission preset
                    <select
                      className={shellStyles.select}
                      value={permissionPreset}
                      onChange={(event) =>
                        setPermissionPreset(event.target.value as "" | PermissionPreset)
                      }
                      id="run-task-permission"
                    >
                      <option value="">Default</option>
                      <option value="suggest">Suggest</option>
                      <option value="auto_edit">Auto edit</option>
                      <option value="full_auto">Full auto</option>
                    </select>
                  </label>
                </div>

                <div className={shellStyles.buttonRow}>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => void makeMainMutation.mutateAsync()}
                    disabled={makeMainMutation.isPending || alias.trim().length === 0}
                    data-testid="modern-chat-make-main"
                    id="chat-make-main-button"
                  >
                    {makeMainMutation.isPending ? "Saving main…" : "Make selected alias main"}
                  </button>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={startNewSession}
                    data-testid="modern-chat-new-session"
                    id="chat-new-session"
                  >
                    New session
                  </button>
                </div>

                <label className={shellStyles.fieldWide}>
                  Prompt
                  <textarea
                    className={shellStyles.textarea}
                    value={prompt}
                    onChange={(event) => setPrompt(event.target.value)}
                    placeholder="Run a task, slash command, or shell command against the current alias."
                    rows={6}
                    id="modern-chat-prompt"
                    data-testid="run-task-prompt"
                  />
                </label>

                <div className={styles.attachmentRow}>
                  <label className={shellStyles.fieldWide}>
                    Attachment path
                    <div className={styles.attachmentInputRow}>
                      <input
                        className={shellStyles.input}
                        value={attachmentPath}
                        onChange={(event) => setAttachmentPath(event.target.value)}
                        placeholder="J:\\path\\to\\artifact.png"
                        data-testid="modern-chat-attachment-path"
                        id="chat-attachment-path"
                      />
                      <button
                        type="button"
                        className={shellStyles.secondaryButton}
                        onClick={addAttachment}
                        data-testid="modern-chat-add-attachment"
                        id="chat-attachment-add"
                      >
                        Add
                      </button>
                    </div>
                  </label>
                  {attachments.length > 0 ? (
                    <div className={styles.attachmentList} data-testid="modern-chat-attachments" id="chat-attachments">
                      {attachments.map((attachment) => (
                        <article key={`${attachment.kind}-${attachment.path}`} className={shellStyles.listCard}>
                          <strong>{attachment.path.split(/[\\/]/).pop()}</strong>
                          <div className={shellStyles.meta}>
                            {attachment.kind} · {attachment.path}
                          </div>
                        </article>
                      ))}
                    </div>
                  ) : null}
                </div>

                <div className={shellStyles.buttonRow}>
                  <button
                    type="submit"
                    className={shellStyles.primaryButton}
                    disabled={runTaskMutation.isPending || prompt.trim().length === 0}
                    data-testid="modern-chat-submit"
                    id="run-task-submit"
                  >
                    {runTaskMutation.isPending ? "Running…" : "Run task"}
                  </button>
                </div>
              </form>

              {runError ? <p className={shellStyles.bannerError} id="run-task-error">{runError}</p> : null}
              {!runError && (commandResult || runResult) ? (
                <p className={shellStyles.bannerSuccess} data-testid="modern-chat-result" id="run-task-result">
                  {commandResult || runResult}
                </p>
              ) : null}
            </Surface>

            <Surface eyebrow="Transcript" title={selectedSession?.title ?? "Conversation stream"}>
              {mergedMessages.length === 0 ? (
                <EmptyState
                  title="No transcript yet"
                  body="Run a task or select a prior session to inspect the saved conversation, tool events, and resume packet."
                />
              ) : (
                <div className={styles.transcript} data-testid="modern-chat-transcript" id="chat-transcript">
                  {mergedMessages.map((message) => (
                    <article key={message.id} className={styles.messageCard}>
                      <div className={styles.messageMeta}>
                        <Pill
                          tone={
                            message.role === "assistant"
                              ? "accent"
                              : message.role === "tool"
                                ? "warn"
                                : "neutral"
                          }
                        >
                          {startCase(message.role)}
                        </Pill>
                        <span className={shellStyles.meta}>{fmtDate(message.created_at)}</span>
                      </div>
                      <pre className={styles.messageBody}>{message.content}</pre>
                    </article>
                  ))}
                </div>
              )}
            </Surface>
          </div>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Remote content" title="Safety stream">
              {remoteArtifacts.length === 0 ? (
                <p className={shellStyles.empty}>No remote-content events in the current run.</p>
              ) : (
                <div className={shellStyles.list} id="chat-remote-content">
                  {remoteArtifacts.map((artifact) => (
                    <article key={artifact.id ?? `${artifact.source.url ?? artifact.source.label ?? "artifact"}-${artifact.excerpt ?? ""}`} className={shellStyles.listCard}>
                      <div className={shellStyles.pillRow}>
                        <Pill
                          tone={
                            artifact.assessment.risk === "high"
                              ? "danger"
                              : artifact.assessment.risk === "medium"
                                ? "warn"
                                : "good"
                          }
                        >
                          {artifact.assessment.risk}
                        </Pill>
                        <Pill tone={artifact.assessment.blocked ? "danger" : "good"}>
                          {artifact.assessment.blocked ? "blocked" : "allowed"}
                        </Pill>
                      </div>
                      <strong>{remoteArtifactTitle(artifact)}</strong>
                      {remoteArtifactSubtitle(artifact) ? (
                        <div className={shellStyles.meta}>{remoteArtifactSubtitle(artifact)}</div>
                      ) : null}
                      {artifact.excerpt ? <p className={styles.sideCopy}>{artifact.excerpt}</p> : null}
                      {artifact.assessment.warnings.length > 0 ? (
                        <div className={shellStyles.meta}>
                          Warnings: {artifact.assessment.warnings.join("; ")}
                        </div>
                      ) : null}
                    </article>
                  ))}
                </div>
              )}
            </Surface>

            <Surface eyebrow="Resume packet" title="Current context">
              {resumePacketQuery.data ? (
                <div className={shellStyles.list}>
                  {resumePacketQuery.data.recent_messages.slice(-4).map((message) => (
                    <article key={message.id} className={shellStyles.listCard}>
                      <strong>{startCase(message.role)}</strong>
                      <div className={shellStyles.meta}>{message.content}</div>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>Select a session to inspect its resume packet.</p>
              )}
            </Surface>

            <Surface eyebrow="Command console" title="Slash commands and shell output">
              {consoleEntries.length > 0 ? (
                <div className={shellStyles.list} id="chat-console">
                  {consoleEntries.map((entry) => (
                    <article key={entry.id} className={shellStyles.listCard}>
                      <div className={shellStyles.pillRow}>
                        <Pill
                          tone={
                            entry.tone === "good"
                              ? "good"
                              : entry.tone === "warn"
                                ? "warn"
                                : entry.tone === "danger"
                                  ? "danger"
                                  : "neutral"
                          }
                        >
                          {entry.title}
                        </Pill>
                      </div>
                      <pre className={styles.consoleBody}>{entry.body}</pre>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>
                  Slash commands, shell output, and local operator shortcuts appear here.
                </p>
              )}
            </Surface>
          </div>
        </div>
      ) : null}

      {activeTab === "sessions" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Saved sessions" title="Resume and manage history">
            <div className={shellStyles.tableWrap}>
              <table className={shellStyles.table} data-testid="modern-session-table" id="sessions-body">
                <thead>
                  <tr>
                    <th>Title</th>
                    <th>Alias</th>
                    <th>Model</th>
                    <th>Updated</th>
                    <th>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {(sessionsQuery.data ?? []).map((session) => (
                    <tr key={session.id}>
                      <td>{session.title ?? "Untitled session"}</td>
                      <td>{session.alias}</td>
                      <td>{session.model}</td>
                      <td>{fmtDate(session.updated_at)}</td>
                      <td>
                        <div className={shellStyles.buttonRow}>
                          <button
                            type="button"
                            className={shellStyles.secondaryButton}
                            onClick={() => selectSession(session)}
                            data-testid={`modern-session-view-${session.id}`}
                            data-session-id={session.id}
                          >
                            View
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Selected session" title={selectedSession?.title ?? "No session selected"}>
              {selectedSession ? (
                <div className={shellStyles.stack} id="session-detail">
                  <div className={shellStyles.kvGrid}>
                    <div className={shellStyles.kvRow}>
                      <span className={shellStyles.kvLabel}>Alias</span>
                      <strong className={shellStyles.kvValue}>{selectedSession.alias}</strong>
                    </div>
                    <div className={shellStyles.kvRow}>
                      <span className={shellStyles.kvLabel}>Model</span>
                      <strong className={shellStyles.kvValue}>{selectedSession.model}</strong>
                    </div>
                    <div className={shellStyles.kvRow}>
                      <span className={shellStyles.kvLabel}>Task mode</span>
                      <strong className={shellStyles.kvValue}>
                        {selectedSession.task_mode ? startCase(selectedSession.task_mode) : "Standard"}
                      </strong>
                    </div>
                  </div>
                  <div className={shellStyles.meta} id="chat-session-meta">
                    {selectedSession.task_mode ? startCase(selectedSession.task_mode) : "Standard"}
                  </div>

                  <label className={shellStyles.field}>
                    Rename session
                    <input
                      className={shellStyles.input}
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                      placeholder={selectedSession.title ?? "Untitled session"}
                    />
                  </label>

                  <div className={shellStyles.buttonRow}>
                    <button
                      type="button"
                      className={shellStyles.secondaryButton}
                      onClick={() => void renameMutation.mutateAsync()}
                      disabled={renameMutation.isPending || renameValue.trim().length === 0}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      className={shellStyles.secondaryButton}
                      onClick={() => void forkMutation.mutateAsync()}
                      disabled={forkMutation.isPending}
                    >
                      Fork
                    </button>
                    <button
                      type="button"
                      className={shellStyles.secondaryButton}
                      onClick={() => void compactMutation.mutateAsync()}
                      disabled={compactMutation.isPending}
                    >
                      Compact
                    </button>
                  </div>

                  {renameMutation.error ? (
                    <p className={shellStyles.bannerError}>
                      {renameMutation.error instanceof Error ? renameMutation.error.message : "Rename failed."}
                    </p>
                  ) : null}
                </div>
              ) : (
                <p className={shellStyles.empty}>Select a session from the table to manage it.</p>
              )}
            </Surface>

            <Surface eyebrow="Resume packet" title="Linked context">
              {resumePacketQuery.data ? (
                <div className={shellStyles.stack}>
                  <div className={shellStyles.stack}>
                    <h3 className={styles.sectionTitle}>Recent messages</h3>
                    <div className={shellStyles.list}>
                      {resumePacketQuery.data.recent_messages.length > 0 ? (
                        resumePacketQuery.data.recent_messages.slice(-4).map((message) => (
                          <article key={message.id} className={shellStyles.listCard}>
                            <strong>{startCase(message.role)}</strong>
                            <div className={shellStyles.meta}>{message.content}</div>
                          </article>
                        ))
                      ) : (
                        <p className={shellStyles.empty}>No recent messages available.</p>
                      )}
                    </div>
                  </div>

                  <div className={shellStyles.stack}>
                    <h3 className={styles.sectionTitle}>Linked memories</h3>
                    <div className={shellStyles.list}>
                      {resumePacketQuery.data.linked_memories.length > 0 ? (
                        resumePacketQuery.data.linked_memories.slice(0, 6).map((memory) => (
                          <article key={memory.id} className={shellStyles.listCard}>
                            <strong>{memory.subject}</strong>
                            <div className={shellStyles.meta}>{memory.content}</div>
                            {renderMemoryProvenance(memory).map((item) => (
                              <div key={`${memory.id}-${item}`} className={shellStyles.meta}>
                                {item}
                              </div>
                            ))}
                          </article>
                        ))
                      ) : (
                        <p className={shellStyles.empty}>No linked memories.</p>
                      )}
                    </div>
                  </div>
                </div>
              ) : (
                <p className={shellStyles.empty}>Select a session to inspect linked memories.</p>
              )}
            </Surface>
          </div>
        </div>
      ) : null}

      {activeTab === "context" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Resume packet" title="Related transcript hits">
            {resumePacketQuery.data ? (
              <div className={shellStyles.list}>
                {resumePacketQuery.data.related_transcript_hits.length > 0 ? (
                  resumePacketQuery.data.related_transcript_hits.map((hit) => (
                    <article key={hit.message_id} className={shellStyles.listCard}>
                      <strong>{fmtDate(hit.created_at)}</strong>
                      <div className={shellStyles.meta}>{hit.preview}</div>
                    </article>
                  ))
                ) : (
                  <p className={shellStyles.empty}>No related transcript hits for this session.</p>
                )}
              </div>
            ) : (
              <p className={shellStyles.empty}>Select a session to inspect its resume packet.</p>
            )}
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Remote artifacts" title="Current run evidence">
              {remoteArtifacts.length > 0 ? (
                <div className={shellStyles.list}>
                  {remoteArtifacts.map((artifact) => (
                    <article key={artifact.id ?? `${artifact.source.label ?? artifact.source.url ?? "artifact"}-${artifact.excerpt ?? ""}`} className={shellStyles.listCard}>
                      <strong>{remoteArtifactTitle(artifact)}</strong>
                      {remoteArtifactSubtitle(artifact) ? (
                        <div className={shellStyles.meta}>{remoteArtifactSubtitle(artifact)}</div>
                      ) : null}
                      <div className={shellStyles.meta}>
                        {artifact.assessment.reasons.join("; ") || "No explicit reasons recorded."}
                      </div>
                    </article>
                  ))}
                </div>
              ) : (
                <p className={shellStyles.empty}>Run a task with remote content to capture safety evidence here.</p>
              )}
            </Surface>

            <Surface eyebrow="Selected session" title="Task metadata">
              {selectedSession ? (
                <div className={shellStyles.kvGrid}>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Session ID</span>
                    <strong className={shellStyles.kvValue}>{selectedSession.id}</strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Messages</span>
                    <strong className={shellStyles.kvValue}>{selectedSession.message_count}</strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Workspace</span>
                    <strong className={shellStyles.kvValue}>{selectedSession.cwd ?? "Not recorded"}</strong>
                  </div>
                </div>
              ) : (
                <p className={shellStyles.empty}>Select a session to inspect its metadata.</p>
              )}
            </Surface>
          </div>
        </div>
      ) : null}
    </div>
  );
}
