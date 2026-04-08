import { NavLink, Outlet, useLocation } from "react-router-dom";

import { useDashboardData } from "../../app/useDashboardData";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { fmtDate, fmtDurationFrom, startCase } from "../../utils/format";
import styles from "./CockpitLayout.module.css";

const NAV_ITEMS = [
  { to: "/", label: "Overview", title: "Operational summary" },
  { to: "/chat", label: "Chat", title: "Task and session cockpit" },
  { to: "/integrations", label: "Integrations", title: "Providers, connectors, plugins, and MCP" },
  { to: "/operations", label: "Operations", title: "Missions, memory, approvals, and events" },
  { to: "/system", label: "System", title: "Trust, daemon, config, and diagnostics" }
];

function routeTitle(pathname: string) {
  const match = NAV_ITEMS.find((item) => item.to === pathname || (item.to === "/" && pathname === "/"));
  return match ?? NAV_ITEMS[0];
}

export function CockpitLayout() {
  const location = useLocation();
  const { bootstrap, onLogout } = useDashboardData();
  const mainTarget = bootstrap.status.main_target;
  const current = routeTitle(location.pathname);
  const lastEvent = bootstrap.events[0];

  return (
    <div className={styles.shell} data-testid="modern-dashboard-shell">
      <aside className={styles.navRail} data-testid="modern-nav-rail">
        <div className={styles.brand}>
          <div className={styles.brandMark}>N</div>
          <div className={styles.brandCopy}>
            <div className={styles.brandEyebrow}>Nuclear Agent</div>
            <div className={styles.brandTitle}>Operator Cockpit</div>
            <div className={styles.brandMeta}>
              {bootstrap.status.main_agent_alias ?? "No main alias"} · {bootstrap.status.persistence_mode}
            </div>
          </div>
        </div>

        <nav className={styles.nav}>
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) =>
                isActive ? `${styles.navLink} ${styles.navLinkActive}` : styles.navLink
              }
              data-testid={`nav-${item.label.toLowerCase()}`}
            >
              <span>{item.label}</span>
              <small>{item.title}</small>
            </NavLink>
          ))}
        </nav>

        <Surface eyebrow="Current target" title="Active main route" className={styles.railCard}>
          <div className={styles.railMeta}>
            <div>
              <span>Alias</span>
              <strong>{mainTarget?.alias ?? bootstrap.status.main_agent_alias ?? "Unassigned"}</strong>
            </div>
            <div>
              <span>Provider</span>
              <strong>{mainTarget?.provider_id ?? "Unavailable"}</strong>
            </div>
            <div>
              <span>Model</span>
              <strong>{mainTarget?.model ?? "Unavailable"}</strong>
            </div>
          </div>
        </Surface>
      </aside>

      <div className={styles.main}>
        <header className={styles.topBar}>
          <div className={styles.headlineBlock}>
            <div className={styles.topEyebrow}>{current.label}</div>
            <h1 className={styles.headline}>{current.title}</h1>
            <p className={styles.headlineCopy}>
              Live operator view for a local daemon, persistent sessions, provider routing, safety
              controls, and automated workflows.
            </p>
          </div>

          <div className={styles.statusCluster}>
            <Pill tone="accent">{startCase(bootstrap.permissions)}</Pill>
            <Pill tone={bootstrap.trust.allow_shell ? "good" : "warn"}>
              Shell {bootstrap.trust.allow_shell ? "enabled" : "guarded"}
            </Pill>
            <Pill tone={bootstrap.status.autonomy.allow_self_edit ? "warn" : "neutral"}>
              Self-edit {bootstrap.status.autonomy.allow_self_edit ? "on" : "off"}
            </Pill>
            <Pill tone={bootstrap.remote_content_policy === "block_high_risk" ? "good" : "warn"}>
              {startCase(bootstrap.remote_content_policy)}
            </Pill>
            <button className={styles.logoutButton} type="button" onClick={() => void onLogout()}>
              Sign out
            </button>
          </div>
        </header>

        <div className={styles.contentGrid}>
          <main className={styles.workspace} data-testid="modern-main-workspace">
            <Outlet />
          </main>

          <aside className={styles.inspector} data-testid="modern-right-inspector">
            <Surface eyebrow="Daemon state" title="Live posture">
              <div className={styles.railMeta}>
                <div>
                  <span>Autonomy</span>
                  <strong>{startCase(bootstrap.status.autonomy.state)}</strong>
                </div>
                <div>
                  <span>Evolve</span>
                  <strong>{startCase(bootstrap.status.evolve.state)}</strong>
                </div>
                <div>
                  <span>Started</span>
                  <strong>{fmtDate(bootstrap.status.started_at)}</strong>
                </div>
              </div>
            </Surface>

            <Surface eyebrow="Latest event" title={lastEvent?.scope ?? "No recent activity"}>
              {lastEvent ? (
                <div className={styles.eventCard}>
                  <div className={styles.eventMeta}>
                    <Pill tone="neutral">{lastEvent.level}</Pill>
                    <span>{fmtDurationFrom(lastEvent.created_at)}</span>
                  </div>
                  <p className={styles.eventMessage}>{lastEvent.message}</p>
                </div>
              ) : (
                <p className={styles.emptyCopy}>No recent daemon events.</p>
              )}
            </Surface>
          </aside>
        </div>
      </div>
    </div>
  );
}
