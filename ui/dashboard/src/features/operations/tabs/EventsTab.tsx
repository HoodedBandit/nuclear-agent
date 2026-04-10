import type { LogEntry } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { fmtDate } from "../format";

interface EventsTabProps {
  events?: LogEntry[];
}

export function EventsTab(props: EventsTabProps) {
  const { events } = props;

  return (
    <div className="stack-list">
      {events?.length ? (
        events.map((entry) => (
          <article key={entry.id} className="stack-card">
            <div className="stack-card__title">
              <strong>{entry.target}</strong>
              <span>{fmtDate(entry.created_at)}</span>
            </div>
            <p className="stack-card__copy">{entry.message}</p>
          </article>
        ))
      ) : (
        <EmptyState title="No events" copy="Recent daemon events will appear here." />
      )}
    </div>
  );
}
