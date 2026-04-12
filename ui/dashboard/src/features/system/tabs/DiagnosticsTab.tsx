import type { FormEvent } from "react";
import { useState } from "react";
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
  const [activeView, setActiveView] = useState<"logs" | "support">("logs");

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

      <Panel eyebrow="Diagnostics" title="Logs and support bundle">
        <div className="stack-list">
          <div className="subtabs subtabs--panel" role="tablist" aria-label="Diagnostics tools">
            {(["logs", "support"] as const).map((view) => (
              <button
                key={view}
                type="button"
                className={activeView === view ? "is-active" : undefined}
                onClick={() => setActiveView(view)}
              >
                {view === "logs" ? "log feed" : "support bundle"}
              </button>
            ))}
          </div>

          {activeView === "logs" ? (
            <div className="stack-list scroll-stack" id="system-logs">
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
          ) : null}

          {activeView === "support" ? (
            <>
              <form
                className="stack-card stack-list"
                id="support-bundle-form"
                onSubmit={onCreateBundle}
              >
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
                    <input
                      id="support-bundle-log-limit"
                      type="number"
                      name="log_limit"
                      defaultValue="200"
                    />
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
              ) : (
                <EmptyState
                  title="No bundle yet"
                  copy="Create a support bundle only when you need an exportable diagnostic package."
                />
              )}
            </>
          ) : null}
        </div>
      </Panel>
    </div>
  );
}
