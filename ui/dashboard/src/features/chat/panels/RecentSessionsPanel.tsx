import type { SessionSummary } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";

interface RecentSessionsPanelProps {
  sessions: SessionSummary[];
  onOpenSession: (session: SessionSummary) => void;
  onUseTarget: (session: SessionSummary) => void;
}

export function RecentSessionsPanel({
  sessions,
  onOpenSession,
  onUseTarget
}: RecentSessionsPanelProps) {
  return (
    <Panel eyebrow="Sessions" title="Recent chats">
      <div className="stack-list" id="sessions-body">
        {sessions.length ? (
          sessions.map((entry) => (
            <article key={entry.id} className="stack-card" data-session-id={entry.id}>
              <div className="stack-card__title">
                <strong>{entry.title || entry.alias}</strong>
                <span>{entry.task_mode || "default"}</span>
              </div>
              <p className="stack-card__subtitle">
                {entry.alias} {"->"} {entry.provider_id} / {entry.model}
              </p>
              <div className="button-row">
                <button type="button" onClick={() => onOpenSession(entry)}>
                  Open
                </button>
                <button type="button" onClick={() => onUseTarget(entry)}>
                  Use target
                </button>
              </div>
            </article>
          ))
        ) : (
          <EmptyState title="No saved sessions" copy="Chats will accumulate here." />
        )}
      </div>
    </Panel>
  );
}
