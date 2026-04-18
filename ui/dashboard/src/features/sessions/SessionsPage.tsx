import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { getJson, postJson, putJson } from "../../api/client";
import type {
  SessionResumePacket,
  SessionSummary,
  SessionTranscript
} from "../../api/types";
import { useChatBootstrap } from "../../app/dashboard-selectors";
import { Panel } from "../../components/Panel";
import { RecentSessionsPanel } from "../chat/panels/RecentSessionsPanel";
import { ResumePacketPanel } from "../chat/panels/ResumePacketPanel";
import { TranscriptPanel } from "../chat/panels/TranscriptPanel";

export function SessionsPage() {
  const { sessions } = useChatBootstrap();
  const queryClient = useQueryClient();
  const [session, setSession] = useState<SessionTranscript | null>(null);
  const [resumePacket, setResumePacket] = useState<SessionResumePacket | null>(null);

  async function loadSessionContext(sessionId: string) {
    const [transcript, resume] = await Promise.all([
      getJson<SessionTranscript>(`/v1/sessions/${encodeURIComponent(sessionId)}`),
      getJson<SessionResumePacket>(
        `/v1/sessions/${encodeURIComponent(sessionId)}/resume-packet`
      )
    ]);
    setSession(transcript);
    setResumePacket(resume);
  }

  async function renameSession() {
    if (!session) {
      return;
    }
    const title = window.prompt("Rename session", session.session.title || "");
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

  return (
    <div className="page-stack">
      <Panel eyebrow="Session" title={session?.session.title || "Inspector"}>
        <div className="button-row">
          <button type="button" onClick={() => void renameSession()} disabled={!session}>
            Rename
          </button>
          <button type="button" onClick={() => void forkSession()} disabled={!session}>
            Fork
          </button>
          <button type="button" onClick={() => void compactSession()} disabled={!session}>
            Compact
          </button>
        </div>
      </Panel>
      <div className="split-panels">
        <TranscriptPanel session={session} response={null} />
        <RecentSessionsPanel
          sessions={sessions}
          onOpenSession={(entry: SessionSummary) => {
            void loadSessionContext(entry.id);
          }}
          onUseTarget={(entry) => {
            void loadSessionContext(entry.id);
          }}
        />
      </div>
      <ResumePacketPanel resumePacket={resumePacket} />
    </div>
  );
}
