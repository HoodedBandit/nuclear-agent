import type { SessionResumePacket } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";

interface ResumePacketPanelProps {
  resumePacket: SessionResumePacket | null;
}

export function ResumePacketPanel({ resumePacket }: ResumePacketPanelProps) {
  return (
    <Panel eyebrow="Resume packet" title="Linked context">
      {resumePacket ? (
        <div className="stack-list" id="session-detail">
          <article className="stack-card">
            <div className="stack-card__title">
              <strong>Recent messages</strong>
              <span>{resumePacket.recent_messages.length}</span>
            </div>
            <p className="stack-card__copy">
              Session {resumePacket.session.alias} / {resumePacket.session.model}
            </p>
          </article>
          <article className="stack-card">
            <div className="stack-card__title">
              <strong>Linked memories</strong>
              <span>{resumePacket.linked_memories.length}</span>
            </div>
            <p className="stack-card__copy">
              {resumePacket.linked_memories
                .slice(0, 3)
                .map((memory) => memory.subject)
                .join(", ") || "No linked memories."}
            </p>
          </article>
          <article className="stack-card">
            <div className="stack-card__title">
              <strong>Transcript hits</strong>
              <span>{resumePacket.related_transcript_hits.length}</span>
            </div>
            <p className="stack-card__copy">
              {resumePacket.related_transcript_hits
                .slice(0, 3)
                .map((hit) => hit.snippet)
                .join(" | ") || "No related hits."}
            </p>
          </article>
        </div>
      ) : (
        <EmptyState title="No resume packet" copy="Open a session to inspect its linked context." />
      )}
    </Panel>
  );
}
