import type { FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { fetchDoctor, postJson } from "../../api/client";
import { useOverviewBootstrap } from "../../app/dashboard-selectors";
import { EmptyState } from "../../components/EmptyState";
import { MetricCard } from "../../components/MetricCard";
import { Panel } from "../../components/Panel";

function fmtDate(value?: string | null) {
  if (!value) {
    return "-";
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function OverviewPage() {
  const { status, events, sessions } = useOverviewBootstrap();
  const [workspaceReport, setWorkspaceReport] = useState<{
    workspace_root?: string;
    manifests?: string[];
    language_breakdown?: Array<{ label: string; files: number }>;
    focus_paths?: Array<{ path: string; source_files: number }>;
    recent_commits?: string[];
  } | null>(null);
  const doctorQuery = useQuery({
    queryKey: ["doctor"],
    queryFn: fetchDoctor,
    refetchInterval: 15000
  });

  async function inspectWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const requestedPath = String(form.get("path") || "").trim();
    const response = await postJson<{
      workspace_root?: string;
      manifests?: string[];
      language_breakdown?: Array<{ label: string; files: number }>;
      focus_paths?: Array<{ path: string; source_files: number }>;
      recent_commits?: string[];
    }>("/v1/workspace/inspect", { path: requestedPath || undefined });
    setWorkspaceReport(response);
  }

  return (
    <>
      <Panel
        eyebrow="Overview"
        title="Runtime posture"
        meta={`Started ${fmtDate(status.started_at)}`}
      >
        <div className="metric-grid" data-testid="modern-overview-page">
          <MetricCard label="Providers" value={status.providers} />
          <MetricCard label="Aliases" value={status.aliases} />
          <MetricCard
            label="Missions"
            value={status.active_missions}
            detail={`${status.missions} total`}
            tone={status.active_missions > 0 ? "warn" : "neutral"}
          />
          <MetricCard
            label="Memory reviews"
            value={status.pending_memory_reviews}
            tone={status.pending_memory_reviews > 0 ? "warn" : "good"}
          />
        </div>
      </Panel>

      <div className="split-panels">
        <Panel eyebrow="Health" title="Doctor summary">
          {doctorQuery.data ? (
            <div className="stack-list" id="doctor-summary">
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Daemon health</strong>
                  <span>{doctorQuery.data.daemon_running ? "running" : "offline"}</span>
                </div>
                <p className="stack-card__copy">
                  Config path: <span className="mono">{doctorQuery.data.config_path}</span>
                </p>
              </article>
              {doctorQuery.data.providers.map((provider) => (
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
            <EmptyState title="Doctor pending" copy="Waiting for the health report." />
          )}
        </Panel>

        <Panel eyebrow="Workspace" title="Inspect workspace">
          <form className="stack-list" id="workspace-inspect-form" onSubmit={inspectWorkspace}>
            <label className="field">
              <span>Workspace path</span>
              <input
                id="workspace-inspect-path"
                name="path"
                placeholder="Leave blank for daemon cwd"
              />
            </label>
            <div className="button-row">
              <button id="workspace-inspect-submit" type="submit">
                Inspect workspace
              </button>
            </div>
          </form>
          {workspaceReport ? (
            <div className="stack-list" id="workspace-overview">
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Workspace root</strong>
                  <span>{workspaceReport.language_breakdown?.length || 0} languages</span>
                </div>
                <p className="stack-card__copy mono">
                  {workspaceReport.workspace_root || "-"}
                </p>
              </article>
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Manifest files</strong>
                  <span>{workspaceReport.manifests?.length || 0}</span>
                </div>
                <p className="stack-card__copy">
                  {(workspaceReport.manifests || []).join(", ") || "No manifests reported."}
                </p>
              </article>
              <article className="stack-card">
                <div className="stack-card__title">
                  <strong>Focus paths</strong>
                  <span>{workspaceReport.focus_paths?.length || 0}</span>
                </div>
                <p className="stack-card__copy">
                  {(workspaceReport.focus_paths || []).map((entry) => entry.path).join(", ") ||
                    "No focus paths reported."}
                </p>
              </article>
            </div>
          ) : (
            <EmptyState
              title="Workspace summary pending"
              copy="Run an inspection to render manifests, languages, and hotspots."
            />
          )}
        </Panel>
      </div>

      <div className="split-panels">
        <Panel eyebrow="Recent" title="Event ribbon">
          <div className="stack-list" id="overview-events">
            {events.length ? (
              events.slice(0, 8).map((entry) => (
                <article key={entry.id} className="stack-card">
                  <div className="stack-card__title">
                    <strong>{entry.target}</strong>
                    <span>{fmtDate(entry.created_at)}</span>
                  </div>
                  <p className="stack-card__copy">{entry.message}</p>
                </article>
              ))
            ) : (
              <EmptyState title="No events yet" copy="Live daemon activity will appear here." />
            )}
          </div>
        </Panel>

        <Panel eyebrow="Sessions" title="Recent sessions">
          <div className="stack-list" id="overview-sessions">
            {sessions.length ? (
              sessions.slice(0, 8).map((session) => (
                <article key={session.id} className="stack-card">
                  <div className="stack-card__title">
                    <strong>{session.title || session.alias}</strong>
                    <span>{session.task_mode || "default"}</span>
                  </div>
                  <p className="stack-card__subtitle">
                    {session.alias} {"->"} {session.provider_id} / {session.model}
                  </p>
                  <p className="stack-card__copy">{fmtDate(session.updated_at)}</p>
                </article>
              ))
            ) : (
              <EmptyState title="No sessions yet" copy="Run a task to seed the session ledger." />
            )}
          </div>
        </Panel>
      </div>
    </>
  );
}
