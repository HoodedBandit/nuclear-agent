import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { useDashboardData } from "../../app/useDashboardData";
import { listEvents, inspectWorkspace } from "../../api/client";
import { Pill } from "../../components/Pill";
import { StatCard } from "../../components/StatCard";
import { Surface } from "../../components/Surface";
import { WorkbenchTabs } from "../../components/WorkbenchTabs";
import { fmtCount, fmtDate, fmtDurationFrom, startCase } from "../../utils/format";
import shellStyles from "../shared/Workbench.module.css";
import styles from "./OverviewPage.module.css";

const OVERVIEW_TABS = [
  { id: "summary", label: "Summary", description: "Health, targeting, and posture" },
  { id: "activity", label: "Activity", description: "Recent logs and daemon movement" },
  { id: "workspace", label: "Workspace", description: "Repo and workspace inspection" }
] as const;

type OverviewTabId = (typeof OVERVIEW_TABS)[number]["id"];

export function OverviewPage() {
  const { bootstrap } = useDashboardData();
  const navigate = useNavigate();
  const [activeTab, setActiveTab] = useState<OverviewTabId>("summary");
  const connectorCount =
    bootstrap.telegram_connectors.length +
    bootstrap.discord_connectors.length +
    bootstrap.slack_connectors.length +
    bootstrap.signal_connectors.length +
    bootstrap.home_assistant_connectors.length +
    bootstrap.webhook_connectors.length +
    bootstrap.inbox_connectors.length +
    bootstrap.gmail_connectors.length +
    bootstrap.brave_connectors.length;

  const workspaceQuery = useQuery({
    queryKey: ["workspace-inspect", bootstrap.status.main_target?.alias ?? "default"],
    queryFn: () => inspectWorkspace({}),
    staleTime: 30_000
  });

  const eventsQuery = useQuery({
    queryKey: ["events"],
    queryFn: () => listEvents(60),
    initialData: bootstrap.events
  });

  const recentEvents = useMemo(
    () => (eventsQuery.data ?? []).slice().sort((left, right) => right.created_at.localeCompare(left.created_at)),
    [eventsQuery.data]
  );

  return (
    <div className={shellStyles.page} data-testid="modern-overview-page">
      <section className={shellStyles.hero}>
        <div className={shellStyles.heroBlock}>
          <div className={shellStyles.heroEyebrow}>Overview</div>
          <h2 className={shellStyles.heroTitle}>Operational launchpad for the live daemon and its workspace.</h2>
          <p className={shellStyles.heroCopy}>
            Inspect the current target, recent system movement, and the workspace the agent is
            actively operating inside.
          </p>
        </div>
        <div className={shellStyles.heroActions}>
          <Pill tone="accent">{startCase(bootstrap.permissions)}</Pill>
          <Pill tone={bootstrap.status.autonomy.allow_self_edit ? "warn" : "neutral"}>
            Self-edit {bootstrap.status.autonomy.allow_self_edit ? "enabled" : "off"}
          </Pill>
          <Pill tone={bootstrap.remote_content_policy === "block_high_risk" ? "good" : "warn"}>
            {startCase(bootstrap.remote_content_policy)}
          </Pill>
        </div>
      </section>

      <WorkbenchTabs
        tabs={OVERVIEW_TABS.map((tab) => ({ ...tab }))}
        activeTab={activeTab}
        onChange={(tabId) => setActiveTab(tabId as OverviewTabId)}
        testIdPrefix="modern-overview-tab"
      />

      {activeTab === "summary" ? (
        <>
          <section className={styles.statsGrid}>
            <StatCard
              label="Providers"
              value={fmtCount(bootstrap.providers.length)}
              detail={`${bootstrap.provider_capabilities.length} capability summaries tracked`}
            />
            <StatCard
              label="Aliases"
              value={fmtCount(bootstrap.aliases.length)}
              detail={`Main alias ${bootstrap.status.main_agent_alias ?? "is not configured"}`}
            />
            <StatCard
              label="Connectors"
              value={fmtCount(connectorCount)}
              detail={`${fmtCount(bootstrap.plugins.length)} plugins installed`}
            />
            <StatCard
              label="Sessions"
              value={fmtCount(bootstrap.sessions.length)}
              detail={`Daemon started ${fmtDate(bootstrap.status.started_at)}`}
            />
          </section>

          <div className={shellStyles.gridTwo}>
            <div className={shellStyles.stack}>
              <Surface eyebrow="Main route" title="Target and policy" emphasis="accent">
                <div className={shellStyles.kvGrid}>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Alias</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.status.main_target?.alias ?? bootstrap.status.main_agent_alias ?? "Unassigned"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Provider</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.status.main_target?.provider_id ?? "Unavailable"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Model</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.status.main_target?.model ?? "Unavailable"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Autonomy</span>
                    <strong className={shellStyles.kvValue}>{startCase(bootstrap.status.autonomy.state)}</strong>
                  </div>
                </div>
              </Surface>

              <Surface eyebrow="Provider matrix" title="Configured providers">
                <div className={shellStyles.list}>
                  {bootstrap.providers.map((provider) => (
                    <article key={provider.id} className={shellStyles.listCard}>
                      <strong>{provider.display_name}</strong>
                      <div className={shellStyles.meta}>{provider.id}</div>
                      <div className={shellStyles.meta}>{provider.base_url}</div>
                      <div className={shellStyles.pillRow}>
                        <Pill tone={provider.local ? "good" : "accent"}>
                          {provider.local ? "Local" : "Hosted"}
                        </Pill>
                        <Pill tone="neutral">{startCase(provider.auth_mode)}</Pill>
                      </div>
                    </article>
                  ))}
                </div>
              </Surface>
            </div>

            <div className={shellStyles.stack}>
              <Surface eyebrow="Recent sessions" title="Latest conversation history">
                <div className={shellStyles.list}>
                  {bootstrap.sessions.slice(0, 8).map((session) => (
                    <article key={session.id} className={shellStyles.listCard}>
                      <strong>{session.title ?? "Untitled session"}</strong>
                      <div className={shellStyles.meta}>
                        {session.alias} · {session.model}
                      </div>
                      <div className={shellStyles.meta}>{fmtDate(session.updated_at)}</div>
                    </article>
                  ))}
                </div>
              </Surface>

              <Surface eyebrow="Quick launch" title="Open the right workbench fast">
                <div className={shellStyles.buttonRow} id="setup-checklist">
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => navigate("/integrations?tab=providers")}
                  >
                    Providers
                  </button>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => navigate("/integrations?tab=connectors&connector=telegram")}
                  >
                    Telegram
                  </button>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => navigate("/integrations?tab=plugins")}
                  >
                    Plugins
                  </button>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => navigate("/system?tab=trust&focus=permissions")}
                  >
                    Permissions
                  </button>
                  <button
                    type="button"
                    className={shellStyles.secondaryButton}
                    onClick={() => navigate("/system?tab=config")}
                  >
                    Config
                  </button>
                </div>
              </Surface>

              <Surface eyebrow="Trust posture" title="Execution boundaries">
                <div className={shellStyles.kvGrid}>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Shell</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.trust.allow_shell ? "Allowed" : "Guarded"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Network</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.trust.allow_network ? "Allowed" : "Guarded"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Full disk</span>
                    <strong className={shellStyles.kvValue}>
                      {bootstrap.trust.allow_full_disk ? "Allowed" : "Guarded"}
                    </strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Trusted paths</span>
                    <strong className={shellStyles.kvValue}>{bootstrap.trust.trusted_paths.length}</strong>
                  </div>
                </div>
              </Surface>
            </div>
          </div>
        </>
      ) : null}

      {activeTab === "activity" ? (
        <div className={shellStyles.gridTwo}>
          <Surface eyebrow="Recent daemon events" title="Activity stream" className={styles.fullHeightSurface}>
            <div className={shellStyles.scrollArea}>
              <div className={shellStyles.list}>
                {recentEvents.map((event) => (
                  <article key={event.id} className={shellStyles.listCard}>
                    <div className={styles.eventHeader}>
                      <div className={shellStyles.pillRow}>
                        <Pill tone="neutral">{event.level}</Pill>
                        <Pill tone="accent">{event.scope}</Pill>
                      </div>
                      <span className={shellStyles.meta}>{fmtDurationFrom(event.created_at)}</span>
                    </div>
                    <p className={styles.eventMessage}>{event.message}</p>
                  </article>
                ))}
              </div>
            </div>
          </Surface>

          <Surface eyebrow="System summary" title="Current state">
            <div className={shellStyles.kvGrid}>
              <div className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>PID</span>
                <strong className={shellStyles.kvValue}>{bootstrap.status.pid}</strong>
              </div>
              <div className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Persistence</span>
                <strong className={shellStyles.kvValue}>{startCase(bootstrap.status.persistence_mode)}</strong>
              </div>
              <div className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Auto start</span>
                <strong className={shellStyles.kvValue}>{bootstrap.status.auto_start ? "Enabled" : "Disabled"}</strong>
              </div>
              <div className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Autopilot</span>
                <strong className={shellStyles.kvValue}>{startCase(bootstrap.status.autopilot.state)}</strong>
              </div>
              <div className={shellStyles.kvRow}>
                <span className={shellStyles.kvLabel}>Missions</span>
                <strong className={shellStyles.kvValue}>{bootstrap.status.missions ?? 0}</strong>
              </div>
            </div>
          </Surface>
        </div>
      ) : null}

      {activeTab === "workspace" ? (
        <div className={shellStyles.gridTwo}>
          <div className={shellStyles.stack}>
            <Surface eyebrow="Workspace root" title="Inspection summary" emphasis="accent" className={styles.fullHeightSurface}>
              {workspaceQuery.data ? (
                <div className={shellStyles.kvGrid} id="workspace-summary-cards">
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Workspace root</span>
                    <strong className={shellStyles.kvValue}>{workspaceQuery.data.workspace_root}</strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Git branch</span>
                    <strong className={shellStyles.kvValue}>{workspaceQuery.data.git_branch ?? "Unavailable"}</strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Dirty files</span>
                    <strong className={shellStyles.kvValue}>{workspaceQuery.data.dirty_files}</strong>
                  </div>
                  <div className={shellStyles.kvRow}>
                    <span className={shellStyles.kvLabel}>Untracked files</span>
                    <strong className={shellStyles.kvValue}>{workspaceQuery.data.untracked_files}</strong>
                  </div>
                </div>
              ) : (
                <p className={shellStyles.empty}>Inspecting the current workspace…</p>
              )}
            </Surface>

            <Surface eyebrow="Source distribution" title="Focus paths">
              <div id="workspace">
                <div className={shellStyles.tableWrap}>
                  <table className={shellStyles.table}>
                    <thead>
                      <tr>
                        <th>Path</th>
                        <th>Source files</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(workspaceQuery.data?.focus_paths ?? []).map((item) => (
                        <tr key={item.path}>
                          <td className={shellStyles.code}>{item.path}</td>
                          <td>{item.source_files}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </Surface>
          </div>

          <div className={shellStyles.stack}>
            <Surface eyebrow="Large source files" title="Highest line counts">
              <div className={shellStyles.tableWrap}>
                <table className={shellStyles.table} id="workspace-overview">
                  <thead>
                    <tr>
                      <th>File</th>
                      <th>Lines</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(workspaceQuery.data?.large_source_files ?? []).map((item) => (
                      <tr key={item.path}>
                        <td className={shellStyles.code}>{item.path}</td>
                        <td>{item.lines}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Surface>

            <Surface eyebrow="Manifests and languages" title="Repository makeup">
              <div className={shellStyles.stack}>
                <div>
                  <h3 className={styles.subheading}>Manifests</h3>
                  <div className={shellStyles.list}>
                    {(workspaceQuery.data?.manifests ?? []).map((manifest) => (
                      <article key={manifest} className={shellStyles.listCard}>
                        <span className={shellStyles.code}>{manifest}</span>
                      </article>
                    ))}
                  </div>
                </div>
                <div>
                  <h3 className={styles.subheading}>Languages</h3>
                  <div className={shellStyles.list}>
                    {(workspaceQuery.data?.language_breakdown ?? []).map((item) => (
                      <article key={item.label} className={shellStyles.listCard}>
                        <strong>{item.label}</strong>
                        <div className={shellStyles.meta}>{item.files} files</div>
                      </article>
                    ))}
                  </div>
                </div>
              </div>
            </Surface>
          </div>
        </div>
      ) : null}
    </div>
  );
}
