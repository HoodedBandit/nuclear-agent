import type { UpdateStatusResponse } from "../../../api/types";
import { Panel } from "../../../components/Panel";

interface UpdatesTabProps {
  status: UpdateStatusResponse | null;
  busy: boolean;
  error?: string | null;
  onCheck: () => void;
  onRun: () => void;
}

function renderValue(value?: string | null) {
  return value && value.trim() ? value : "-";
}

export function UpdatesTab(props: UpdatesTabProps) {
  const { status, busy, onCheck, onRun } = props;
  const { error = null } = props;
  const actionable = status?.availability === "available";

  return (
    <div className="split-panels">
      <Panel eyebrow="Updates" title="Remote release control">
        <div className="stack-list" id="update-status-panel">
          <div className="stack-card">
            <div className="stack-card__title">
              <strong>Install surface</strong>
              <span>{status?.install.kind ?? "manual"}</span>
            </div>
            <div className="stack-card__copy mono">
              {status?.install.executable_path ?? "No update check has been run yet."}
            </div>
          </div>
          <div className="button-row">
            <button id="update-check-button" type="button" onClick={onCheck} disabled={busy}>
              {busy ? "Checking..." : "Check for updates"}
            </button>
            <button
              id="update-run-button"
              type="button"
              onClick={onRun}
              disabled={busy || !actionable}
            >
              {status?.availability === "in_progress" ? "Applying..." : "Update now"}
            </button>
          </div>
          {error ? (
            <article className="stack-card">
              <div className="stack-card__title">
                <strong>Request failed</strong>
                <span>attention</span>
              </div>
              <p className="stack-card__copy">{error}</p>
            </article>
          ) : null}
          {status ? (
            <div className="stack-list" id="update-status-body">
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Availability</strong>
                  <span>{status.availability}</span>
                </div>
                <p className="stack-card__copy">{renderValue(status.detail)}</p>
              </article>
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Current build</strong>
                  <span>{status.current_version}</span>
                </div>
                <p className="stack-card__copy mono">
                  commit {renderValue(status.current_commit)}
                </p>
              </article>
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Candidate</strong>
                  <span>{renderValue(status.candidate_version)}</span>
                </div>
                <p className="stack-card__copy mono">
                  tag {renderValue(status.candidate_tag)} commit{" "}
                  {renderValue(status.candidate_commit)}
                </p>
              </article>
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Execution</strong>
                  <span>{renderValue(status.step)}</span>
                </div>
                <p className="stack-card__copy">checked {status.checked_at}</p>
              </article>
              {status.last_run ? (
                <article className="stack-card" id="update-last-run">
                  <div className="stack-card__title">
                    <strong>Last run</strong>
                    <span>{status.last_run.state}</span>
                  </div>
                  <p className="stack-card__copy">
                    {renderValue(status.last_run.detail)} from{" "}
                    {renderValue(status.last_run.from_version)} to{" "}
                    {renderValue(status.last_run.to_version)}
                  </p>
                </article>
              ) : null}
            </div>
          ) : null}
        </div>
      </Panel>
    </div>
  );
}
