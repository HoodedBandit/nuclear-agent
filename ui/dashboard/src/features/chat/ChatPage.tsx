import type { FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { getJson, postJson, putJson } from "../../api/client";
import type {
  InputAttachment,
  PermissionPreset,
  RemoteContentPolicy,
  RunTaskResponse,
  SessionResumePacket,
  SessionSummary,
  SessionTranscript,
  TaskMode,
  ThinkingLevel
} from "../../api/types";
import { useChatBootstrap } from "../../app/dashboard-selectors";
import { RecentSessionsPanel } from "./panels/RecentSessionsPanel";
import { ResumePacketPanel } from "./panels/ResumePacketPanel";
import { RunTaskPanel } from "./panels/RunTaskPanel";
import { TranscriptPanel } from "./panels/TranscriptPanel";

export function ChatPage() {
  const { aliases, sessions, mainAgentAlias } = useChatBootstrap();
  const queryClient = useQueryClient();
  const [prompt, setPrompt] = useState("");
  const [alias, setAlias] = useState(mainAgentAlias || "main");
  const [thinking, setThinking] = useState<ThinkingLevel>("medium");
  const [cwd, setCwd] = useState("");
  const [taskMode, setTaskMode] = useState<TaskMode | "">("");
  const [permissionPreset, setPermissionPreset] = useState<PermissionPreset | "">("");
  const [ephemeral, setEphemeral] = useState(false);
  const [remoteContentPolicy, setRemoteContentPolicy] = useState<RemoteContentPolicy | "">("");
  const [attachmentPath, setAttachmentPath] = useState("");
  const [attachmentKind, setAttachmentKind] = useState<InputAttachment["kind"]>("file");
  const [attachments, setAttachments] = useState<InputAttachment[]>([]);
  const [response, setResponse] = useState<RunTaskResponse | null>(null);
  const [session, setSession] = useState<SessionTranscript | null>(null);
  const [resumePacket, setResumePacket] = useState<SessionResumePacket | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function resetDraftRunState() {
    setAttachments([]);
    setAttachmentPath("");
    setAttachmentKind("file");
    setEphemeral(false);
    setError(null);
  }

  async function loadSessionContext(sessionId: string) {
    const [transcript, resume] = await Promise.all([
      getJson<SessionTranscript>(`/v1/sessions/${encodeURIComponent(sessionId)}`),
      getJson<SessionResumePacket>(
        `/v1/sessions/${encodeURIComponent(sessionId)}/resume-packet`
      )
    ]);
    setSession(transcript);
    setResumePacket(resume);
    return transcript;
  }

  async function runTask(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const result = await postJson<RunTaskResponse>("/v1/run", {
        prompt,
        alias,
        requested_model: null,
        session_id: session?.session.id || null,
        cwd: cwd || null,
        thinking_level: thinking,
        attachments,
        permission_preset: permissionPreset || null,
        task_mode: taskMode || null,
        ephemeral,
        remote_content_policy_override: remoteContentPolicy || null
      });
      setResponse(result);
      setPrompt("");
      await loadSessionContext(result.session_id);
      await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Task failed.");
    } finally {
      setBusy(false);
    }
  }

  async function openSession(sessionSummary: SessionSummary) {
    resetDraftRunState();
    await loadSessionContext(sessionSummary.id);
    setResponse(null);
    setAlias(sessionSummary.alias);
    setTaskMode(sessionSummary.task_mode || "");
    setCwd(sessionSummary.cwd || "");
  }

  async function renameSession() {
    if (!session) {
      return;
    }
    const title = window.prompt("Rename chat", session.session.title || "");
    if (!title || !title.trim()) {
      return;
    }
    await putJson(`/v1/sessions/${encodeURIComponent(session.session.id)}/title`, {
      title: title.trim()
    });
    await loadSessionContext(session.session.id);
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  async function forkSession() {
    if (!session) {
      return;
    }
    const forked = await postJson<SessionTranscript>(
      `/v1/sessions/${encodeURIComponent(session.session.id)}/fork`,
      {}
    );
    await loadSessionContext(forked.session.id);
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  async function compactSession() {
    if (!session) {
      return;
    }
    const compacted = await postJson<SessionTranscript>(
      `/v1/sessions/${encodeURIComponent(session.session.id)}/compact`,
      {}
    );
    await loadSessionContext(compacted.session.id);
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  async function makeMain() {
    await putJson("/v1/main-alias", { alias });
    await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
  }

  function clearSession() {
    resetDraftRunState();
    setSession(null);
    setResumePacket(null);
    setResponse(null);
    setTaskMode("");
  }

  function addAttachment() {
    const path = attachmentPath.trim();
    if (!path) {
      return;
    }
    setAttachments((current) => [...current, { kind: attachmentKind, path }]);
    setAttachmentPath("");
  }

  function removeAttachment(index: number) {
    setAttachments((current) => current.filter((_, currentIndex) => currentIndex !== index));
  }

  return (
    <>
      <RunTaskPanel
        aliases={aliases}
        sessionId={session?.session.id || null}
        prompt={prompt}
        alias={alias}
        thinking={thinking}
        cwd={cwd}
        taskMode={taskMode}
        permissionPreset={permissionPreset}
        ephemeral={ephemeral}
        remoteContentPolicy={remoteContentPolicy}
        attachmentPath={attachmentPath}
        attachmentKind={attachmentKind}
        attachments={attachments}
        busy={busy}
        error={error}
        onPromptChange={setPrompt}
        onAliasChange={setAlias}
        onThinkingChange={setThinking}
        onCwdChange={setCwd}
        onTaskModeChange={setTaskMode}
        onPermissionPresetChange={setPermissionPreset}
        onEphemeralChange={setEphemeral}
        onRemoteContentPolicyChange={setRemoteContentPolicy}
        onAttachmentPathChange={setAttachmentPath}
        onAttachmentKindChange={setAttachmentKind}
        onSubmit={runTask}
        onAddAttachment={addAttachment}
        onRemoveAttachment={removeAttachment}
        onMakeMain={() => {
          void makeMain();
        }}
        onClearSession={clearSession}
        onRenameSession={() => {
          void renameSession();
        }}
        onForkSession={() => {
          void forkSession();
        }}
        onCompactSession={() => {
          void compactSession();
        }}
      />

      <div className="split-panels">
        <TranscriptPanel session={session} response={response} />
        <RecentSessionsPanel
          sessions={sessions}
          onOpenSession={(entry) => {
            void openSession(entry);
          }}
          onUseTarget={(entry) => {
            setAlias(entry.alias);
            setCwd(entry.cwd || "");
          }}
        />
      </div>

      <ResumePacketPanel resumePacket={resumePacket} />
    </>
  );
}
