import { useQuery } from "@tanstack/react-query";
import { getJson } from "../../api/client";
import type { LogEntry } from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Panel } from "../../components/Panel";

function fmtDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function LogsPage() {
  const logsQuery = useQuery({
    queryKey: ["logs-page"],
    queryFn: () => getJson<LogEntry[]>("/v1/logs?limit=120"),
    refetchInterval: 10000
  });
  const eventsQuery = useQuery({
    queryKey: ["events-page"],
    queryFn: () => getJson<LogEntry[]>("/v1/events?limit=120"),
    refetchInterval: 10000
  });

  return (
    <div className="split-panels">
      <Panel eyebrow="Daemon" title="Logs">
        <div className="stack-list" id="logs-feed">
          {logsQuery.data?.length ? (
            logsQuery.data.map((entry) => (
              <article key={entry.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{entry.target}</strong>
                  <span>{fmtDate(entry.created_at)}</span>
                </div>
                <p className="stack-card__copy">{entry.message}</p>
              </article>
            ))
          ) : (
            <EmptyState title="No logs" copy="Log feed idle." />
          )}
        </div>
      </Panel>
      <Panel eyebrow="Events" title="Recent activity">
        <div className="stack-list" id="events-feed">
          {eventsQuery.data?.length ? (
            eventsQuery.data.map((entry) => (
              <article key={entry.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{entry.target}</strong>
                  <span>{fmtDate(entry.created_at)}</span>
                </div>
                <p className="stack-card__copy">{entry.message}</p>
              </article>
            ))
          ) : (
            <EmptyState title="No events" copy="Event feed idle." />
          )}
        </div>
      </Panel>
    </div>
  );
}
