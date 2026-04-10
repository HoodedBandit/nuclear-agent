import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  createSupportBundle,
  fetchDoctor,
  getJson,
  postJson,
  putJson
} from "../../api/client";
import type { PermissionPreset, SupportBundleResponse } from "../../api/types";
import { useDashboardData } from "../../app/dashboard-data";
import { Panel } from "../../components/Panel";
import { AdvancedTab } from "./tabs/AdvancedTab";
import { DaemonTab } from "./tabs/DaemonTab";
import { DiagnosticsTab } from "./tabs/DiagnosticsTab";
import { PolicyTab } from "./tabs/PolicyTab";

type SystemTab = "policy" | "daemon" | "diagnostics" | "advanced";

export function SystemPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<SystemTab>("policy");
  const [supportBundle, setSupportBundle] = useState<SupportBundleResponse | null>(null);
  const logsQuery = useQuery({
    queryKey: ["logs"],
    queryFn: () => getJson<typeof bootstrap.events>("/v1/logs?limit=100"),
    refetchInterval: 15000
  });
  const doctorQuery = useQuery({
    queryKey: ["doctor-system"],
    queryFn: fetchDoctor,
    refetchInterval: 15000
  });
  const configQuery = useQuery({
    queryKey: ["advanced-config"],
    queryFn: () => getJson<Record<string, unknown>>("/v1/config")
  });

  async function refreshSystem() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
      queryClient.invalidateQueries({ queryKey: ["logs"] }),
      queryClient.invalidateQueries({ queryKey: ["doctor-system"] }),
      queryClient.invalidateQueries({ queryKey: ["advanced-config"] })
    ]);
  }

  async function updatePermission(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const preset = new FormData(event.currentTarget).get("permission_preset") as PermissionPreset;
    await putJson("/v1/permissions", { permission_preset: preset });
    await refreshSystem();
  }

  async function updateTrust(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/trust", {
      trusted_path: String(form.get("trusted_path") || "").trim() || null,
      allow_shell: Boolean(form.get("allow_shell")),
      allow_network: Boolean(form.get("allow_network")),
      allow_full_disk: Boolean(form.get("allow_full_disk")),
      allow_self_edit: Boolean(form.get("allow_self_edit"))
    });
    await refreshSystem();
  }

  async function updateDaemon(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/daemon/config", {
      persistence_mode: form.get("persistence_mode"),
      auto_start: Boolean(form.get("auto_start"))
    });
    await refreshSystem();
  }

  async function updateAutonomy(mode: "enable" | "pause" | "resume") {
    const path =
      mode === "enable" ? "/v1/autonomy/enable" : `/v1/autonomy/${mode}`;
    await postMode(path);
  }

  async function updateEvolve(mode: "start" | "pause" | "resume" | "stop") {
    const payload = mode === "start" ? { alias: null, requested_model: null } : {};
    await putOrPost(`/v1/evolve/${mode}`, payload);
  }

  async function updateAutopilot(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/autopilot/status", {
      state: form.get("state"),
      max_concurrent_missions: Number(form.get("max_concurrent_missions") || 1),
      wake_interval_seconds: Number(form.get("wake_interval_seconds") || 30),
      allow_background_shell: Boolean(form.get("allow_background_shell")),
      allow_background_network: Boolean(form.get("allow_background_network")),
      allow_background_self_edit: Boolean(form.get("allow_background_self_edit"))
    });
    await refreshSystem();
  }

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

  async function saveConfig(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const raw = new FormData(event.currentTarget).get("config")?.toString() || "{}";
    await putJson("/v1/config", JSON.parse(raw));
    await refreshSystem();
  }

  return (
    <Panel eyebrow="System" title="Policy, daemon, and diagnostics">
      <div className="toolbar">
        <div className="toolbar__title">
          <strong>System workbench</strong>
          <span>Lock policy, daemon state, diagnostics, and advanced configuration.</span>
        </div>
        <div className="subtabs">
          {(["policy", "daemon", "diagnostics", "advanced"] as SystemTab[]).map((tab) => (
            <button
              key={tab}
              type="button"
              className={activeTab === tab ? "is-active" : undefined}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {activeTab === "policy" ? (
        <PolicyTab
          permissions={bootstrap.permissions}
          trust={bootstrap.trust}
          onUpdatePermission={updatePermission}
          onUpdateTrust={updateTrust}
        />
      ) : null}

      {activeTab === "daemon" ? (
        <DaemonTab
          status={bootstrap.status}
          onUpdateDaemon={updateDaemon}
          onUpdateAutonomy={(mode) => {
            void updateAutonomy(mode);
          }}
          onUpdateEvolve={(mode) => {
            void updateEvolve(mode);
          }}
          onUpdateAutopilot={updateAutopilot}
        />
      ) : null}

      {activeTab === "diagnostics" ? (
        <DiagnosticsTab
          doctor={doctorQuery.data}
          logs={logsQuery.data}
          supportBundle={supportBundle}
          onCreateBundle={createBundle}
        />
      ) : null}

      {activeTab === "advanced" ? (
        <AdvancedTab config={configQuery.data} onSaveConfig={saveConfig} />
      ) : null}
    </Panel>
  );
}

async function postMode(path: string) {
  await putOrPost(path, { allow_self_edit: false, mode: "assisted" });
}

async function putOrPost(path: string, payload: Record<string, unknown>) {
  const method = path.includes("/status") ? putJson : postJson;
  await method(path, payload);
}
