import { Link, NavLink, Outlet } from "react-router-dom";

import { useDashboardData } from "../../app/useDashboardData";
import { Pill } from "../../components/Pill";
import { Surface } from "../../components/Surface";
import { fmtDate, fmtDurationFrom, startCase } from "../../utils/format";
import styles from "./CockpitLayout.module.css";

const NAV_ITEMS = [
  { to: "/", label: "Overview" },
  { to: "/chat", label: "Chat" },
  { to: "/integrations", label: "Integrations" },
  { to: "/operations", label: "Operations" },
  { to: "/system", label: "System" }
];

export function CockpitLayout() {
  const { bootstrap, onLogout } = useDashboardData();
  const mainTarget = bootstrap.status.main_target;
  const lastEvent = bootstrap.events[0];

  return (
    <div className={styles.shell} data-testid="modern-dashboard-shell">
      <aside className={styles.navRail}>
        <div className={styles.brand}>
          <div className={styles.brandMark}>N</div>
          <div>
            <div className={styles.brandEyebrow}>Nuclear</div>
            <div className={styles.brandTitle}>Operator Cockpit</div>
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
              {item.label}
            </NavLink>
          ))}
        </nav>

        <Surface className={styles.legacySurface} eyebrow="Staged rollout" title="Classic fallback">
          <p className={styles.legacyText}>
            The new cockpit is active in parallel. The classic dashboard remains available until the
            migrated flows are feature-complete.
          </p>
          <div className={styles.legacyActions}>
            <a className={styles.linkButton} href="/dashboard-classic">
              Open classic
            </a>
            <a className={styles.linkButtonGhost} href="/dashboard">
              Stable route
            </a>
          </div>
        </Surface>
      </aside>

      <div className={styles.main}>
        <header className={styles.topBar}>
          <div className={styles.headlineBlock}>
            <div className={styles.topEyebrow}>Live operator view</div>
            <h1 className={styles.headline}>Modern dashboard rollout</h1>
            <p className={styles.headlineCopy}>
              Main target{" "}
              <strong>
                {mainTarget?.alias ?? bootstrap.status.main_agent_alias ?? "unassigned"}
              </strong>{" "}
              on {mainTarget?.model ?? "no model configured"}.
            </p>
          </div>

          <div className={styles.statusCluster}>
            <Pill tone="accent">{startCase(bootstrap.permissions)}</Pill>
            <Pill tone={bootstrap.trust.allow_shell ? "good" : "warn"}>
              Shell {bootstrap.trust.allow_shell ? "enabled" : "guarded"}
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
          <main className={styles.workspace}>
            <Outlet context={{ bootstrap }} />
          </main>

          <aside className={styles.inspector}>
            <Surface eyebrow="Current target" title="Session and policy">
              <div className={styles.metaList}>
                <div>
                  <span>Main alias</span>
                  <strong>{bootstrap.status.main_agent_alias ?? "Unassigned"}</strong>
                </div>
                <div>
                  <span>Persistence</span>
                  <strong>{startCase(bootstrap.status.persistence_mode)}</strong>
                </div>
                <div>
                  <span>Autonomy</span>
                  <strong>{startCase(bootstrap.status.autonomy.state)}</strong>
                </div>
                <div>
                  <span>Started</span>
                  <strong>{fmtDate(bootstrap.status.started_at)}</strong>
                </div>
              </div>
            </Surface>

            <Surface eyebrow="Recent activity" title="Last event">
              {lastEvent ? (
                <div className={styles.eventCard}>
                  <div className={styles.eventMeta}>
                    <Pill tone="neutral">{lastEvent.level}</Pill>
                    <span>{fmtDurationFrom(lastEvent.created_at)}</span>
                  </div>
                  <div className={styles.eventScope}>{lastEvent.scope}</div>
                  <p className={styles.eventMessage}>{lastEvent.message}</p>
                </div>
              ) : (
                <p className={styles.emptyCopy}>No recent daemon events.</p>
              )}
            </Surface>

            <Surface eyebrow="Escape hatch" title="Operator links">
              <div className={styles.linkStack}>
                <Link to="/integrations" className={styles.inlineLink}>
                  Review providers and connectors
                </Link>
                <Link to="/chat" className={styles.inlineLink}>
                  Open session cockpit
                </Link>
                <a href="/dashboard-classic" className={styles.inlineLink}>
                  Use classic dashboard tools
                </a>
              </div>
            </Surface>
          </aside>
        </div>
      </div>
    </div>
  );
}
