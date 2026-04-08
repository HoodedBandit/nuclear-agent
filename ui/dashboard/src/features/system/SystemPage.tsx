import { FormEvent, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import { useDashboardData } from "../../app/useDashboardData";
import {
  enableAutonomy,
  fetchConfig,
  fetchDoctorReport,
  getAutonomyStatus,
  getAutopilotStatus,
  getPermissionPreset,
  getTrust,
  pauseAutonomy,
  resumeAutonomy,
  saveConfig,
  updateAutopilot,
  updateDaemonConfig,
  updatePermissionPreset,
  updateTrust
} from "../../api/client";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { WorkbenchTabs } from "../../components/WorkbenchTabs";
import { fmtDate, startCase } from "../../utils/format";
import shellStyles from "../shared/Workbench.module.css";

const SYSTEM_TABS = [
  { id: "trust", label: "Trust", description: "Permissions, trust policy, and autonomy posture" },
  { id: "daemon", label: "Daemon", description: "Persistence, startup, and autopilot controls" },
  { id: "config", label: "Config", description: "Advanced JSON config editing" },
  { id: "diagnostics", label: "Diagnostics", description: "Doctor report, capability map, and environment" }
] as const;

type SystemTabId = (typeof SYSTEM_TABS)[number]["id"];

export function SystemPage() {
  const { bootstrap } = useDashboardData();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const requestedTab = searchParams.get("tab");
  const requestedFocus = searchParams.get("focus");
  const initialTab = SYSTEM_TABS.some((tab) => tab.id === requestedTab)
    ? (requestedTab as SystemTabId)
    : "trust";
  const [activeTab, setActiveTab] = useState<SystemTabId>(initialTab);
  const [trustedPath, setTrustedPath] = useState("");
  const [configEditor, setConfigEditor] = useState("");
  const [configSummary, setConfigSummary] = useState("Load the daemon config to inspect or edit it.");

  useEffect(() => {
    if (requestedTab && SYSTEM_TABS.some((tab) => tab.id === requestedTab) && requestedTab !== activeTab) {
      setActiveTab(requestedTab as SystemTabId);
    }
  }, [activeTab, requestedTab]);

  useEffect(() => {
    if (!requestedFocus || activeTab !== "trust") {
      return;
    }
    const target = document.getElementById(requestedFocus);
    if (!target) {
      return;
    }
    target.scrollIntoView({ block: "start", behavior: "auto" });
  }, [activeTab, requestedFocus]);

  const trustQuery = useQuery({ queryKey: ["trust"], queryFn: getTrust, initialData: bootstrap.trust });
  const permissionsQuery = useQuery({ queryKey: ["permissions"], queryFn: getPermissionPreset, initialData: bootstrap.permissions });
  const autonomyQuery = useQuery({ queryKey: ["autonomy"], queryFn: getAutonomyStatus, initialData: bootstrap.status.autonomy });
  const autopilotQuery = useQuery({ queryKey: ["autopilot"], queryFn: getAutopilotStatus, initialData: bootstrap.status.autopilot });
  const doctorQuery = useQuery({ queryKey: ["doctor"], queryFn: fetchDoctorReport });

  const trustMutation = useMutation({
    mutationFn: updateTrust,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["trust"] })
      ]);
      setTrustedPath("");
    }
  });
  const permissionsMutation = useMutation({
    mutationFn: updatePermissionPreset,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["permissions"] })
      ]);
    }
  });
  const autonomyMutation = useMutation({
    mutationFn: async (action: { kind: "enable" | "pause" | "resume"; mode?: string }) => {
      if (action.kind === "pause") {
        return pauseAutonomy();
      }
      if (action.kind === "resume") {
        return resumeAutonomy();
      }
      return enableAutonomy({
        mode: action.mode ?? "guarded",
        allow_self_edit: trustQuery.data.allow_self_edit
      });
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["autonomy"] })
      ]);
    }
  });
  const daemonMutation = useMutation({
    mutationFn: updateDaemonConfig,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["bootstrap"] });
    }
  });
  const autopilotMutation = useMutation({
    mutationFn: updateAutopilot,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["bootstrap"] }),
        queryClient.invalidateQueries({ queryKey: ["autopilot"] })
      ]);
    }
  });
  const loadConfigMutation = useMutation({
    mutationFn: fetchConfig,
    onSuccess: (config) => {
      setConfigEditor(JSON.stringify(config, null, 2));
      setConfigSummary("Loaded current daemon config.");
    }
  });
  const saveConfigMutation = useMutation({
    mutationFn: async () => {
      const parsed = JSON.parse(configEditor) as Record<string, unknown>;
      return saveConfig(parsed);
    },
    onSuccess: () => {
      setConfigSummary("Saved full config.");
    }
  });

  function updateSystemRoute(tabId: SystemTabId) {
    setActiveTab(tabId);
    const next = new URLSearchParams(searchParams);
    next.set("tab", tabId);
    if (tabId !== "trust") {
      next.delete("focus");
    }
    setSearchParams(next, { replace: true });
  }

  return (
    <div className={shellStyles.page} data-testid="modern-system-page">
      <section className={shellStyles.hero}>
        <div className={shellStyles.heroBlock}>
          <div className={shellStyles.heroEyebrow}>System</div>
          <h2 className={shellStyles.heroTitle}>Trust policy, daemon behavior, advanced config, and diagnostics in one admin workspace.</h2>
          <p className={shellStyles.heroCopy}>
            This is the control room for permissions, autonomy, persistence, startup, diagnostics,
            and safe expert-level configuration.
          </p>
        </div>
        <div className={shellStyles.heroActions}>
          <Pill tone="accent">{startCase(permissionsQuery.data)}</Pill>
          <Pill tone={trustQuery.data.allow_self_edit ? "warn" : "neutral"}>Self-edit {trustQuery.data.allow_self_edit ? "on" : "off"}</Pill>
          <Pill tone="good">{startCase(autonomyQuery.data.state)}</Pill>
        </div>
      </section>

      <WorkbenchTabs
        tabs={SYSTEM_TABS.map((tab) => ({ ...tab }))}
        activeTab={activeTab}
        onChange={(tabId) => updateSystemRoute(tabId as SystemTabId)}
        testIdPrefix="modern-system-tab"
      />

      {activeTab === "trust" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Trust policy" title="Filesystem, shell, network, and self-edit" emphasis="accent">
            <div className={shellStyles.kvGrid}>
              <label className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Shell</span>
                <input type="checkbox" checked={trustQuery.data.allow_shell} onChange={(event) => void trustMutation.mutateAsync({ allow_shell: event.target.checked })} />
              </label>
              <label className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Network</span>
                <input type="checkbox" checked={trustQuery.data.allow_network} onChange={(event) => void trustMutation.mutateAsync({ allow_network: event.target.checked })} />
              </label>
              <label className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Full disk</span>
                <input type="checkbox" checked={trustQuery.data.allow_full_disk} onChange={(event) => void trustMutation.mutateAsync({ allow_full_disk: event.target.checked })} />
              </label>
              <label className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Self-edit</span>
                <input data-trust-flag="allow_self_edit" type="checkbox" checked={trustQuery.data.allow_self_edit} onChange={(event) => void trustMutation.mutateAsync({ allow_self_edit: event.target.checked })} />
              </label>
            </div>

            <form className={shellStyles.stack} onSubmit={(event: FormEvent) => { event.preventDefault(); void trustMutation.mutateAsync({ trusted_path: trustedPath }); }}>
              <label className={shellStyles.field}>
                Add trusted path
                <input className={shellStyles.input} value={trustedPath} onChange={(event) => setTrustedPath(event.target.value)} placeholder="J:\\workspaces\\critical-repo" />
              </label>
              <div className={shellStyles.buttonRow}>
                <button type="submit" className={shellStyles.secondaryButton} disabled={trustedPath.trim().length === 0}>Add trusted path</button>
              </div>
            </form>

            <div className={shellStyles.list}>
              {trustQuery.data.trusted_paths.map((path) => (
                <article key={path} className={shellStyles.listCard}>
                  <span className={shellStyles.code}>{path}</span>
                </article>
              ))}
            </div>
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Permission preset" title="Operator default">
              <div className={shellStyles.buttonRow} id="permissions">
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void permissionsMutation.mutateAsync({ permission_preset: "suggest" })}>Suggest</button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void permissionsMutation.mutateAsync({ permission_preset: "auto_edit" })}>Auto edit</button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void permissionsMutation.mutateAsync({ permission_preset: "full_auto" })}>Full auto</button>
              </div>
            </Surface>

            <Surface eyebrow="Autonomy" title="Agent operating mode">
              <div id="control-summary" className={shellStyles.kvGrid}>
                <div className={shellStyles.kvRow}>
                  <span className={shellStyles.kvLabel}>State</span>{" "}
                  <strong id="autonomy-state" className={shellStyles.kvValue}>{startCase(autonomyQuery.data.state)}</strong>
                </div>
                <div className={shellStyles.kvRow}>
                  <span className={shellStyles.kvLabel}>Mode</span>{" "}
                  <strong id="autonomy-mode" className={shellStyles.kvValue}>{startCase(autonomyQuery.data.mode)}</strong>
                </div>
              </div>
              <div className={shellStyles.buttonRow}>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void autonomyMutation.mutateAsync({ kind: "enable", mode: "guarded" })}>Guarded</button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void autonomyMutation.mutateAsync({ kind: "enable", mode: "free_thinking" })}>Free thinking</button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void autonomyMutation.mutateAsync({ kind: "pause" })}>Pause</button>
                <button type="button" className={shellStyles.secondaryButton} onClick={() => void autonomyMutation.mutateAsync({ kind: "resume" })}>Resume</button>
              </div>
            </Surface>
          </div>
        </div>
      ) : null}

      {activeTab === "daemon" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Daemon" title="Runtime state" emphasis="accent">
            <div className={shellStyles.kvGrid}>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>PID</span><strong className={shellStyles.kvValue}>{bootstrap.status.pid}</strong></div>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Started</span><strong className={shellStyles.kvValue}>{fmtDate(bootstrap.status.started_at)}</strong></div>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Persistence</span><strong className={shellStyles.kvValue}>{bootstrap.status.persistence_mode}</strong></div>
            </div>
            <div className={shellStyles.buttonRow}>
              <button type="button" className={shellStyles.secondaryButton} onClick={() => void daemonMutation.mutateAsync({ auto_start: !bootstrap.status.auto_start })}>
                {bootstrap.status.auto_start ? "Disable auto start" : "Enable auto start"}
              </button>
              <button type="button" className={shellStyles.secondaryButton} onClick={() => void daemonMutation.mutateAsync({ persistence_mode: bootstrap.status.persistence_mode === "full" ? "minimal" : "full" })}>
                Toggle persistence
              </button>
            </div>
          </Surface>

          <Surface eyebrow="Autopilot" title="Background mission policy">
            <div className={shellStyles.kvGrid}>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>State</span><strong className={shellStyles.kvValue}>{startCase(autopilotQuery.data.state)}</strong></div>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Max concurrent</span><strong className={shellStyles.kvValue}>{autopilotQuery.data.max_concurrent_missions}</strong></div>
              <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Wake interval</span><strong className={shellStyles.kvValue}>{autopilotQuery.data.wake_interval_seconds}s</strong></div>
            </div>
            <div className={shellStyles.buttonRow}>
              <button type="button" className={shellStyles.secondaryButton} onClick={() => void autopilotMutation.mutateAsync({ state: autopilotQuery.data.state === "enabled" ? "disabled" : "enabled" })}>
                {autopilotQuery.data.state === "enabled" ? "Disable autopilot" : "Enable autopilot"}
              </button>
            </div>
          </Surface>
        </div>
      ) : null}

      {activeTab === "config" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Advanced config" title="Load, format, and save the full daemon config" emphasis="accent">
            <div className={shellStyles.buttonRow}>
              <button id="advanced-config-load" type="button" className={shellStyles.secondaryButton} onClick={() => void loadConfigMutation.mutateAsync()}>Load config</button>
              <button id="advanced-config-format" type="button" className={shellStyles.secondaryButton} onClick={() => setConfigEditor((current) => JSON.stringify(JSON.parse(current), null, 2))} disabled={configEditor.trim().length === 0}>Format JSON</button>
              <button id="advanced-config-save" type="button" className={shellStyles.primaryButton} onClick={() => void saveConfigMutation.mutateAsync()} disabled={configEditor.trim().length === 0}>Save config</button>
            </div>
            <textarea id="advanced-config-editor" className={shellStyles.textarea} value={configEditor} onChange={(event) => setConfigEditor(event.target.value)} rows={24} />
            <p id="advanced-config-summary" className={shellStyles.callout}>{configSummary}</p>
          </Surface>
          <Surface eyebrow="Safety note" title="Expert mode">
            <p className={shellStyles.callout}>
              This editor is intentionally raw. It is here for power users who need direct control,
              not as the primary way to configure the product.
            </p>
          </Surface>
        </div>
      ) : null}

      {activeTab === "diagnostics" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Doctor" title="Runtime health" emphasis="accent">
            {doctorQuery.data ? (
              <div className={shellStyles.kvGrid}>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Daemon running</span><strong className={shellStyles.kvValue}>{doctorQuery.data.daemon_running ? "Yes" : "No"}</strong></div>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Config path</span><strong className={shellStyles.kvValue}>{doctorQuery.data.config_path}</strong></div>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Data path</span><strong className={shellStyles.kvValue}>{doctorQuery.data.data_path}</strong></div>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Keyring</span><strong className={shellStyles.kvValue}>{doctorQuery.data.keyring_ok ? "Healthy" : "Warning"}</strong></div>
              </div>
            ) : (
              <p className={shellStyles.empty}>Loading doctor report...</p>
            )}
          </Surface>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Provider capability map" title="Runtime tool coverage">
              <div className={shellStyles.list}>
                {bootstrap.provider_capabilities.map((item) => (
                  <article key={`${item.provider_id}-${item.model}`} className={shellStyles.listCard}>
                    <strong>{item.provider_id}</strong>
                    <div className={shellStyles.meta}>{item.model}</div>
                    <div className={shellStyles.pillRow}>
                      {Object.entries(item.capabilities)
                        .filter(([, enabled]) => enabled)
                        .map(([capability]) => (
                          <Pill key={capability} tone="neutral">{capability.replace(/_/g, " ")}</Pill>
                        ))}
                    </div>
                  </article>
                ))}
              </div>
            </Surface>

            <Surface eyebrow="Environment" title="Current daemon snapshot">
              <div className={shellStyles.kvGrid}>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Providers</span><strong className={shellStyles.kvValue}>{bootstrap.providers.length}</strong></div>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Sessions</span><strong className={shellStyles.kvValue}>{bootstrap.sessions.length}</strong></div>
                <div className={shellStyles.kvRow}><span className={shellStyles.kvLabel}>Events</span><strong className={shellStyles.kvValue}>{bootstrap.events.length}</strong></div>
              </div>
            </Surface>
          </div>
        </div>
      ) : null}
    </div>
  );
}
