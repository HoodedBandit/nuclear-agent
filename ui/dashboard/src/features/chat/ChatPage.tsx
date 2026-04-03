import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { useDashboardData } from "../../app/useDashboardData";
import {
  fetchSessionResumePacket,
  fetchSessionTranscript,
  listSessions,
  streamRunTask
} from "../../api/client";
import type {
  RemoteContentArtifact,
  RunTaskStreamEvent,
  SessionMessage
} from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { useDashboardStore } from "../../store/dashboardStore";
import { fmtDate, startCase } from "../../utils/format";
import styles from "./ChatPage.module.css";

export function ChatPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const { selectedSessionId, setSelectedSessionId } = useDashboardStore();
  const [prompt, setPrompt] = useState("");
  const [alias, setAlias] = useState(
    bootstrap.status.main_agent_alias ?? bootstrap.aliases[0]?.alias ?? ""
  );
  const [taskMode, setTaskMode] = useState<"" | "build" | "daily">("");
  const [streamMessages, setStreamMessages] = useState<SessionMessage[]>([]);
  const [remoteArtifacts, setRemoteArtifacts] = useState<RemoteContentArtifact[]>([]);
  const [lastRunMeta, setLastRunMeta] = useState<string | null>(null);

  const sessionsQuery = useQuery({
    queryKey: ["sessions"],
    queryFn: listSessions,
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

  const runTaskMutation = useMutation({
    mutationFn: async () => {
      setStreamMessages([]);
      setRemoteArtifacts([]);
      setLastRunMeta(null);
      return streamRunTask(
        {
          prompt,
          alias: alias || undefined,
          session_id: selectedSessionId ?? undefined,
          task_mode: taskMode || undefined
        },
        (event) => {
          handleStreamEvent(event);
        }
      );
    },
    onSuccess: async (response) => {
      setSelectedSessionId(response.session_id);
      setLastRunMeta(`${response.alias} - ${response.model}`);
      setPrompt("");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["sessions"] }),
        queryClient.invalidateQueries({ queryKey: ["session", response.session_id] }),
        queryClient.invalidateQueries({ queryKey: ["resume-packet", response.session_id] })
      ]);
    }
  });

  function handleStreamEvent(event: RunTaskStreamEvent) {
    if (event.type === "message") {
      setStreamMessages((current) => [...current, event.message]);
    }
    if (event.type === "remote_content") {
      setRemoteArtifacts((current) => [...current, event.artifact]);
    }
    if (event.type === "session_started") {
      setSelectedSessionId(event.session_id);
      setLastRunMeta(`${event.alias} - ${event.model}`);
    }
  }

  const mergedMessages = useMemo(() => {
    const persisted = transcriptQuery.data?.messages ?? [];
    const deduped = new Map<string, SessionMessage>();
    for (const message of [...persisted, ...streamMessages]) {
      deduped.set(message.id, message);
    }
    return Array.from(deduped.values());
  }, [transcriptQuery.data?.messages, streamMessages]);

  return (
    <div className={styles.page} data-testid="modern-chat-page">
      <section className={styles.layout}>
        <Surface
          eyebrow="Sessions"
          title="Conversation rail"
          className={styles.sessionsSurface}
        >
          <div className={styles.sessionsList}>
            {sessionsQuery.data?.map((session) => (
              <button
                key={session.id}
                type="button"
                className={
                  session.id === selectedSessionId
                    ? `${styles.sessionButton} ${styles.sessionButtonActive}`
                    : styles.sessionButton
                }
                onClick={() => setSelectedSessionId(session.id)}
                data-testid={`modern-session-${session.id}`}
              >
                <strong>{session.title ?? "Untitled session"}</strong>
                <span>{session.alias} - {session.model}</span>
                <span>{fmtDate(session.updated_at)}</span>
              </button>
            ))}
          </div>
        </Surface>

        <div className={styles.centerColumn}>
          <Surface
            eyebrow="Operator chat"
            title="Run tasks and inspect streamed output"
            emphasis="accent"
          >
            <form
              className={styles.composer}
              onSubmit={(event) => {
                event.preventDefault();
                void runTaskMutation.mutateAsync();
              }}
            >
              <div className={styles.formRow}>
                <label>
                  Alias
                  <select
                    value={alias}
                    onChange={(event) => setAlias(event.target.value)}
                    data-testid="modern-chat-alias"
                  >
                    {bootstrap.aliases.map((item) => (
                      <option key={item.alias} value={item.alias}>
                        {item.alias} - {item.model}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Task mode
                  <select
                    value={taskMode}
                    onChange={(event) =>
                      setTaskMode(event.target.value as "" | "build" | "daily")
                    }
                  >
                    <option value="">Standard</option>
                    <option value="build">Build</option>
                    <option value="daily">Daily</option>
                  </select>
                </label>
              </div>
              <label className={styles.promptLabel}>
                Prompt
                <textarea
                  value={prompt}
                  onChange={(event) => setPrompt(event.target.value)}
                  placeholder="Run a task against the current target."
                  rows={5}
                  id="modern-chat-prompt"
                />
              </label>
              <div className={styles.composerActions}>
                <button
                  type="button"
                  className={styles.secondaryButton}
                  onClick={() => {
                    setSelectedSessionId(null);
                    setStreamMessages([]);
                    setRemoteArtifacts([]);
                    setLastRunMeta(null);
                  }}
                >
                  New session
                </button>
                <button
                  type="submit"
                  className={styles.primaryButton}
                  disabled={runTaskMutation.isPending || prompt.trim().length === 0}
                  data-testid="modern-chat-submit"
                >
                  {runTaskMutation.isPending ? "Running..." : "Run task"}
                </button>
              </div>
              {runTaskMutation.error ? (
                <p className={styles.errorCopy}>
                  {runTaskMutation.error instanceof Error
                    ? runTaskMutation.error.message
                    : "Task failed."}
                </p>
              ) : null}
            </form>
          </Surface>

          <Surface eyebrow="Transcript" title={lastRunMeta ?? "Session output"}>
            {mergedMessages.length === 0 ? (
              <EmptyState
                title="No transcript yet"
                body="Run a task or select a previous session to inspect the conversation, tool events, and remote-content signals."
              />
            ) : (
              <div className={styles.transcript} data-testid="modern-chat-transcript">
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
                      <span>{fmtDate(message.created_at)}</span>
                    </div>
                    <pre className={styles.messageBody}>{message.content}</pre>
                  </article>
                ))}
              </div>
            )}
          </Surface>
        </div>

        <div className={styles.sideColumn}>
          <Surface eyebrow="Remote content" title="Safety stream">
            {remoteArtifacts.length === 0 ? (
              <p className={styles.helpCopy}>
                No remote-content events were emitted for this run.
              </p>
            ) : (
              <div className={styles.artifactList}>
                {remoteArtifacts.map((artifact, index) => (
                  <article
                    key={`${
                      artifact.source.url ?? artifact.source.label ?? "artifact"
                    }-${index}`}
                    className={styles.artifactCard}
                  >
                    <div className={styles.messageMeta}>
                      <Pill
                        tone={
                          artifact.risk === "high"
                            ? "danger"
                            : artifact.risk === "medium"
                              ? "warn"
                              : "good"
                        }
                      >
                        {artifact.risk}
                      </Pill>
                      <Pill tone={artifact.allowed ? "good" : "danger"}>
                        {artifact.allowed ? "allowed" : "blocked"}
                      </Pill>
                    </div>
                    <strong>
                      {artifact.source.label ??
                        artifact.source.url ??
                        artifact.source.kind}
                    </strong>
                    {artifact.excerpt ? (
                      <p className={styles.helpCopy}>{artifact.excerpt}</p>
                    ) : null}
                  </article>
                ))}
              </div>
            )}
          </Surface>

          <Surface eyebrow="Resume packet" title="Session context">
            {resumePacketQuery.data ? (
              <div className={styles.resumeList}>
                {resumePacketQuery.data.related_transcript_hits
                  .slice(0, 4)
                  .map((hit) => (
                    <article key={hit.message_id} className={styles.hitCard}>
                      <div className={styles.metaRow}>
                        <span>{fmtDate(hit.created_at)}</span>
                      </div>
                      <p>{hit.preview}</p>
                    </article>
                  ))}
              </div>
            ) : (
              <p className={styles.helpCopy}>
                Select a session to inspect its saved resume packet.
              </p>
            )}
          </Surface>
        </div>
      </section>
    </div>
  );
}
