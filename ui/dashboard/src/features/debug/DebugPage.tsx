import type { FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { createSupportBundle, fetchDoctor, getJson, postJson } from "../../api/client";
import type { LogEntry, SupportBundleResponse } from "../../api/types";
import { EmptyState } from "../../components/EmptyState";
import { Panel } from "../../components/Panel";
import { DiagnosticsTab } from "../system/tabs/DiagnosticsTab";

export function DebugPage() {
  const [supportBundle, setSupportBundle] = useState<SupportBundleResponse | null>(null);
  const [workspaceReport, setWorkspaceReport] = useState<{
    workspace_root?: string;
    manifests?: string[];
    focus_paths?: Array<{ path: string; source_files: number }>;
    recent_commits?: string[];
  } | null>(null);
  const logsQuery = useQuery({
    queryKey: ["logs"],
    queryFn: () => getJson<LogEntry[]>("/v1/logs?limit=100"),
    refetchInterval: 15000
  });
  const doctorQuery = useQuery({
    queryKey: ["doctor-debug"],
    queryFn: fetchDoctor,
    refetchInterval: 15000
  });

  async function createBundle(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setSupportBundle(
      await createSupportBundle({
        output_dir: String(form.get("output_dir") || "").trim() || null,
        log_limit: Number(form.get("log_limit") || 200),
        session_limit: Number(form.get("session_limit") || 25)
      })
    );
  }

  async function inspectWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const requestedPath = String(form.get("path") || "").trim();
    const response = await postJson<{
      workspace_root?: string;
      manifests?: string[];
      focus_paths?: Array<{ path: string; source_files: number }>;
      recent_commits?: string[];
    }>("/v1/workspace/inspect", { path: requestedPath || undefined });
    setWorkspaceReport(response);
  }

  return (
    <div className="page-stack">
      <DiagnosticsTab
        doctor={doctorQuery.data}
        logs={logsQuery.data}
        supportBundle={supportBundle}
        onCreateBundle={createBundle}
      />
      <Panel eyebrow="Workspace" title="Inspect">
        <form className="stack-list" id="workspace-inspect-form" onSubmit={inspectWorkspace}>
          <label className="field">
            <span>Path</span>
            <input
              id="workspace-inspect-path"
              name="path"
              placeholder="Leave blank for daemon cwd"
            />
          </label>
          <div className="button-row">
            <button id="workspace-inspect-submit" type="submit">
              Inspect
            </button>
          </div>
        </form>
        {workspaceReport ? (
          <div className="stack-list" id="workspace-overview">
            <article className="stack-card">
              <div className="stack-card__title">
                <strong>Root</strong>
                <span>{workspaceReport.manifests?.length || 0} manifests</span>
              </div>
              <p className="stack-card__copy mono">{workspaceReport.workspace_root || "-"}</p>
            </article>
            <article className="stack-card">
              <div className="stack-card__title">
                <strong>Focus</strong>
                <span>{workspaceReport.focus_paths?.length || 0}</span>
              </div>
              <p className="stack-card__copy">
                {(workspaceReport.focus_paths || []).map((entry) => entry.path).join(", ") ||
                  "No focus paths."}
              </p>
            </article>
          </div>
        ) : (
          <EmptyState title="No workspace scan" copy="Run inspect." />
        )}
      </Panel>
    </div>
  );
}
