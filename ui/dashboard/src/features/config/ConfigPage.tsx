import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { fetchUpdateStatus, getJson, postJson, putJson, runUpdate } from "../../api/client";
import type { PermissionPreset, UpdateStatusResponse } from "../../api/types";
import { useSystemBootstrap } from "../../app/dashboard-selectors";
import { AdvancedTab } from "../system/tabs/AdvancedTab";
import { DaemonTab } from "../system/tabs/DaemonTab";
import { PolicyTab } from "../system/tabs/PolicyTab";
import { UpdatesTab } from "../system/tabs/UpdatesTab";
import { markPendingUpdate } from "../system/update-session";

type ConfigTab = "policy" | "runtime" | "updates" | "advanced";

export function ConfigPage() {
  const { status, permissions, trust } = useSystemBootstrap();
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<ConfigTab>("policy");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatusResponse | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const configQuery = useQuery({
    queryKey: ["advanced-config"],
    queryFn: () => getJson<Record<string, unknown>>("/v1/config")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
      queryClient.invalidateQueries({ queryKey: ["advanced-config"] })
    ]);
  }

  async function updatePermission(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const preset = new FormData(event.currentTarget).get("permission_preset") as PermissionPreset;
    await putJson("/v1/permissions", { permission_preset: preset });
    await refresh();
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
    await refresh();
  }

  async function updateDaemon(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    await putJson("/v1/daemon/config", {
      persistence_mode: form.get("persistence_mode"),
      auto_start: Boolean(form.get("auto_start"))
    });
    await refresh();
  }

  async function updateAutonomy(mode: "enable" | "pause" | "resume") {
    const path = mode === "enable" ? "/v1/autonomy/enable" : `/v1/autonomy/${mode}`;
    await postMode(path);
    await refresh();
  }

  async function updateEvolve(mode: "start" | "pause" | "resume" | "stop") {
    const payload = mode === "start" ? { alias: null, requested_model: null } : {};
    await putOrPost(`/v1/evolve/${mode}`, payload);
    await refresh();
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
    await refresh();
  }

  async function saveConfig(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const raw = new FormData(event.currentTarget).get("config")?.toString() || "{}";
    await putJson("/v1/config", JSON.parse(raw));
    await refresh();
  }

  async function checkForUpdates() {
    setUpdateBusy(true);
    setUpdateError(null);
    try {
      setUpdateStatus(await fetchUpdateStatus());
    } catch (error) {
      setUpdateError(error instanceof Error ? error.message : "Update check failed.");
    } finally {
      setUpdateBusy(false);
    }
  }

  async function applyUpdate() {
    setUpdateBusy(true);
    setUpdateError(null);
    try {
      const status = await runUpdate({});
      setUpdateStatus(status);
      if (status.availability === "in_progress") {
        markPendingUpdate();
      }
    } catch (error) {
      setUpdateError(error instanceof Error ? error.message : "Update failed.");
    } finally {
      setUpdateBusy(false);
    }
  }

  return (
    <div className="page-stack">
      <section className="route-tabs" aria-label="Config sections">
        {(["policy", "runtime", "updates", "advanced"] as ConfigTab[]).map((tab) => (
          <button
            key={tab}
            type="button"
            className={activeTab === tab ? "is-active" : undefined}
            onClick={() => setActiveTab(tab)}
          >
            {tab}
          </button>
        ))}
      </section>
      {activeTab === "policy" ? (
        <PolicyTab
          permissions={permissions}
          trust={trust}
          onUpdatePermission={updatePermission}
          onUpdateTrust={updateTrust}
        />
      ) : null}
      {activeTab === "runtime" ? (
        <DaemonTab
          status={status}
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
      {activeTab === "updates" ? (
        <UpdatesTab
          status={updateStatus}
          busy={updateBusy}
          error={updateError}
          onCheck={() => {
            void checkForUpdates();
          }}
          onRun={() => {
            void applyUpdate();
          }}
        />
      ) : null}
      {activeTab === "advanced" ? (
        <AdvancedTab config={configQuery.data} onSaveConfig={saveConfig} />
      ) : null}
    </div>
  );
}

async function postMode(path: string) {
  await putOrPost(path, { allow_self_edit: false, mode: "assisted" });
}

async function putOrPost(path: string, payload: Record<string, unknown>) {
  const method = path.includes("/status") ? putJson : postJson;
  await method(path, payload);
}
