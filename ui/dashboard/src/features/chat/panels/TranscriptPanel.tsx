import type { RunTaskResponse, SessionTranscript } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";

interface TranscriptPanelProps {
  session: SessionTranscript | null;
  response: RunTaskResponse | null;
}

export function TranscriptPanel({ session, response }: TranscriptPanelProps) {
  return (
    <Panel eyebrow="Transcript" title="Active session">
      {session ? (
        <div className="stack-list" id="chat-transcript">
          {session.messages.map((message) => (
            <article key={message.id} className="stack-card">
              <div className="stack-card__title">
                <strong>{message.role}</strong>
                <span>{new Date(message.created_at).toLocaleString()}</span>
              </div>
              <p className="stack-card__copy">{message.content}</p>
            </article>
          ))}
        </div>
      ) : response ? (
        <article className="stack-card" id="run-task-result">
          <div className="stack-card__title">
            <strong>{response.alias}</strong>
            <span>{response.model}</span>
          </div>
          <p className="stack-card__copy">{response.response}</p>
          {response.tool_events?.length ? (
            <p className="stack-card__copy">
              Tools: {response.tool_events.map((entry) => entry.name).join(", ")}
            </p>
          ) : null}
        </article>
      ) : (
        <EmptyState title="No active transcript" copy="Run a task or open a prior session." />
      )}
    </Panel>
  );
}
