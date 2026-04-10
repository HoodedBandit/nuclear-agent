import type { FormEvent } from "react";
import type { HealthReport, LogEntry, SupportBundleResponse } from "../../../api/types";
import { EmptyState } from "../../../components/EmptyState";
import { Panel } from "../../../components/Panel";
import { fmtDate } from "../format";

interface DiagnosticsTabProps {
  doctor?: HealthReport;
  logs?: LogEntry[];
  supportBundle: SupportBundleResponse | null;
  onCreateBundle: (event: FormEvent<HTMLFormElement>) => void;
}

export function DiagnosticsTab(props: DiagnosticsTabProps) {
  const { doctor, logs, supportBundle, onCreateBundle } = props;

  return (
    <div className="split-panels">
      <Panel eyebrow="Doctor" title="Health report">
        {doctor ? (
          <div className="stack-list" id="system-doctor">
            {doctor.providers.map((provider) => (
              <article key={provider.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{provider.id}</strong>
                  <span>{provider.ok ? "ok" : "attention"}</span>
                </div>
                <p className="stack-card__copy">{provider.detail}</p>
              </article>
            ))}
          </div>
        ) : (
          <EmptyState title="Doctor pending" copy="Health report not loaded yet." />
        )}
      </Panel>

      <Panel eyebrow="Logs" title="Daemon log feed">
        <div className="stack-list" id="system-logs">
          {logs?.length ? (
            logs.map((entry) => (
              <article key={entry.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{entry.target}</strong>
                  <span>{fmtDate(entry.created_at)}</span>
                </div>
                <p className="stack-card__copy">{entry.message}</p>
              </article>
            ))
          ) : (
            <EmptyState title="No logs" copy="Recent daemon logs will appear here." />
          )}
        </div>
      </Panel>

      <Panel eyebrow="Support" title="Support bundle">
        <form className="stack-list" id="support-bundle-form" onSubmit={onCreateBundle}>
          <label className="field">
            <span>Output directory</span>
            <input
              id="support-bundle-output"
              name="output_dir"
              placeholder="Leave blank for daemon data dir"
            />
          </label>
          <div className="grid-three">
            <label className="field">
              <span>Log limit</span>
              <input id="support-bundle-log-limit" type="number" name="log_limit" defaultValue="200" />
            </label>
            <label className="field">
              <span>Session limit</span>
              <input
                id="support-bundle-session-limit"
                type="number"
                name="session_limit"
                defaultValue="25"
              />
            </label>
          </div>
          <button id="support-bundle-submit" type="submit">
            Create support bundle
          </button>
        </form>
        {supportBundle ? (
          <article className="stack-card" id="support-bundle-result">
            <div className="stack-card__title">
              <strong>Bundle ready</strong>
              <span>{fmtDate(supportBundle.generated_at)}</span>
            </div>
            <p className="stack-card__copy mono">{supportBundle.bundle_dir}</p>
          </article>
        ) : null}
      </Panel>
    </div>
  );
}
