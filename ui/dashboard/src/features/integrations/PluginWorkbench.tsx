import type { FormEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { PluginDoctorReport } from "../../api/types";
import { deleteJson, getJson, postJson, putJson } from "../../api/client";
import { useDashboardData } from "../../app/dashboard-data";
import { EmptyState } from "../../components/EmptyState";
import { Panel } from "../../components/Panel";

export function PluginWorkbench() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const pluginsDoctorQuery = useQuery({
    queryKey: ["plugins-doctor"],
    queryFn: () => getJson<PluginDoctorReport[]>("/v1/plugins/doctor")
  });

  async function refresh() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
      queryClient.invalidateQueries({ queryKey: ["plugins-doctor"] })
    ]);
  }

  async function installPlugin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const formElement = event.currentTarget;
    const form = new FormData(formElement);
    await postJson("/v1/plugins/install", {
      source_path: String(form.get("source_path") || "").trim(),
      enabled: Boolean(form.get("enabled")),
      trusted: Boolean(form.get("trusted")),
      pinned: Boolean(form.get("pinned")),
      granted_permissions: {
        shell: Boolean(form.get("grant_shell")),
        network: Boolean(form.get("grant_network")),
        full_disk: Boolean(form.get("grant_full_disk"))
      }
    });
    formElement.reset();
    await refresh();
  }

  async function updatePlugin(pluginId: string, payload: Record<string, unknown> | null, action?: "remove" | "update") {
    const path = `/v1/plugins/${encodeURIComponent(pluginId)}`;
    if (action === "remove") {
      await deleteJson(path);
    } else if (action === "update") {
      await postJson(`${path}/update`, {});
    } else {
      await putJson(path, payload || {});
    }
    await refresh();
  }

  return (
    <div className="split-panels">
      <Panel eyebrow="Install" title="Plugin packages">
        <form className="stack-list" id="plugin-install-form" onSubmit={installPlugin}>
          <label className="field"><span>Source path</span><input id="plugin-install-path" name="source_path" required /></label>
          <label className="field"><span><input type="checkbox" name="enabled" defaultChecked /> Enabled</span></label>
          <label className="field"><span><input id="plugin-install-trusted" type="checkbox" name="trusted" /> Trusted</span></label>
          <label className="field"><span><input type="checkbox" name="pinned" /> Pinned</span></label>
          <label className="field"><span><input type="checkbox" name="grant_shell" /> Grant shell</span></label>
          <label className="field"><span><input type="checkbox" name="grant_network" /> Grant network</span></label>
          <label className="field"><span><input type="checkbox" name="grant_full_disk" /> Grant full disk</span></label>
          <button id="plugin-install-submit" type="submit">Install plugin</button>
        </form>
      </Panel>
      <Panel eyebrow="Installed" title="Plugin roster">
        <div className="stack-list" id="plugins-list">
          {bootstrap.plugins.length ? (
            bootstrap.plugins.map((plugin) => (
              <article key={plugin.id} className="stack-card">
                <div className="stack-card__title">
                  <strong>{plugin.manifest.name}</strong>
                  <span>{plugin.enabled ? "enabled" : "disabled"}</span>
                </div>
                <p className="stack-card__subtitle">{plugin.id} | {plugin.manifest.version}</p>
                <p className="stack-card__copy">{plugin.manifest.description}</p>
                <div className="button-row">
                  <button type="button" onClick={() => void updatePlugin(plugin.id, { enabled: !plugin.enabled })}>
                    {plugin.enabled ? "Disable" : "Enable"}
                  </button>
                  <button type="button" onClick={() => void updatePlugin(plugin.id, { trusted: !plugin.trusted })}>
                    {plugin.trusted ? "Untrust" : "Trust"}
                  </button>
                  <button type="button" onClick={() => void updatePlugin(plugin.id, { pinned: !plugin.pinned })}>
                    {plugin.pinned ? "Unpin" : "Pin"}
                  </button>
                  <button type="button" onClick={() => void updatePlugin(plugin.id, null, "update")}>
                    Update
                  </button>
                  <button type="button" onClick={() => void updatePlugin(plugin.id, null, "remove")}>
                    Remove
                  </button>
                </div>
              </article>
            ))
          ) : (
            <EmptyState title="No plugins" copy="Installed plugin packages appear here." />
          )}
        </div>
        <div className="stack-list" id="plugins-health">
          {pluginsDoctorQuery.data?.map((report) => (
            <article key={`doctor-${report.id}`} className="stack-card">
              <div className="stack-card__title">
                <strong>{report.name}</strong>
                <span>{report.ok ? "ready" : "attention"}</span>
              </div>
              <p className="stack-card__copy">{report.detail}</p>
            </article>
          ))}
        </div>
      </Panel>
    </div>
  );
}
