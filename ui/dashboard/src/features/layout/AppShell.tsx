import clsx from "clsx";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useDashboardData } from "../../app/dashboard-data";
import { Badge } from "../../components/Badge";

const NAV_ITEMS = [
  {
    to: "/",
    label: "Overview",
    eyebrow: "Status, workspace, live posture"
  },
  {
    to: "/chat",
    label: "Chat",
    eyebrow: "Sessions, prompts, transcripts"
  },
  {
    to: "/operations",
    label: "Operations",
    eyebrow: "Missions, approvals, memory"
  },
  {
    to: "/integrations",
    label: "Integrations",
    eyebrow: "Providers, connectors, plugins"
  },
  {
    to: "/system",
    label: "System",
    eyebrow: "Policy, logs, config, doctor"
  }
];

function navTone(pathname: string) {
  if (pathname === "/chat") {
    return "good";
  }
  if (pathname === "/operations") {
    return "warn";
  }
  return "info";
}

export function AppShell() {
  const { bootstrap, onLogout } = useDashboardData();
  const location = useLocation();
  const mainTarget = bootstrap.status.main_target;

  return (
    <div className="app-shell">
      <aside className="nav-rail" data-testid="modern-nav-rail">
        <section className="brand-block">
          <div className="brand-block__mark">N</div>
          <div>
            <p className="eyebrow">Parallel cockpit build</p>
            <h1>Nuclear Agent</h1>
            <p className="brand-block__meta">
              Premium tactical dashboard on the safe replacement route.
            </p>
          </div>
        </section>

        <section className="nav-block">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) =>
                clsx("nav-link", isActive && "nav-link--active")
              }
              data-testid={`nav-${item.label.toLowerCase()}`}
            >
              <strong>{item.label}</strong>
              <small>{item.eyebrow}</small>
            </NavLink>
          ))}
        </section>

        <section className="rail-card">
          <div className="rail-card__row">
            <span>Main target</span>
            <strong>{mainTarget?.alias || "Not configured"}</strong>
          </div>
          <div className="rail-card__row">
            <span>Providers</span>
            <strong>{bootstrap.status.providers}</strong>
          </div>
          <div className="rail-card__row">
            <span>Connectors</span>
            <strong>
              {bootstrap.status.telegram_connectors +
                bootstrap.status.discord_connectors +
                bootstrap.status.slack_connectors +
                bootstrap.status.signal_connectors +
                bootstrap.status.home_assistant_connectors +
                bootstrap.status.inbox_connectors +
                bootstrap.status.webhook_connectors +
                bootstrap.status.gmail_connectors +
                bootstrap.status.brave_connectors}
            </strong>
          </div>
        </section>
      </aside>

      <main className="workspace-shell" data-testid="modern-main-workspace">
        <header className="top-command-deck">
          <div>
            <p className="eyebrow">Resident control plane</p>
            <h2>
              {location.pathname === "/"
                ? "Operator overview"
                : location.pathname.replace("/", "").replace("-", " ")}
            </h2>
            <p className="deck-copy">
              Maintain full control of the daemon without dropping into generic admin
              dashboard patterns.
            </p>
          </div>
          <div className="deck-status">
            <Badge tone={navTone(location.pathname)}>
              {bootstrap.status.autonomy.state}
            </Badge>
            <Badge tone={bootstrap.status.active_missions > 0 ? "warn" : "neutral"}>
              {bootstrap.status.active_missions} active missions
            </Badge>
            <Badge tone={bootstrap.status.pending_memory_reviews > 0 ? "warn" : "good"}>
              {bootstrap.status.pending_memory_reviews} memory reviews
            </Badge>
            <button
              className="button--ghost"
              type="button"
              onClick={() => {
                void onLogout();
              }}
            >
              Logout
            </button>
          </div>
        </header>

        <div className="content-grid">
          <div className="content-grid__main">
            <Outlet />
          </div>
          <aside className="inspector-rail" data-testid="modern-right-inspector">
            <section className="inspector-card">
              <p className="eyebrow">Primary target</p>
              <h3>{mainTarget?.provider_display_name || "Awaiting setup"}</h3>
              <p>
                {mainTarget
                  ? `${mainTarget.alias} -> ${mainTarget.model}`
                  : "Create a provider and alias to arm the cockpit."}
              </p>
            </section>
            <section className="inspector-card">
              <p className="eyebrow">Policy posture</p>
              <ul className="micro-list">
                <li>Permissions preset: {bootstrap.permissions}</li>
                <li>
                  Shell trust: {bootstrap.trust.allow_shell ? "allowed" : "blocked"}
                </li>
                <li>
                  Network trust: {bootstrap.trust.allow_network ? "allowed" : "blocked"}
                </li>
                <li>
                  Delegation depth:{" "}
                  {bootstrap.delegation_config.max_depth.mode === "limited"
                    ? bootstrap.delegation_config.max_depth.value
                    : "unlimited"}
                </li>
              </ul>
            </section>
            <section className="inspector-card">
              <p className="eyebrow">Recent event feed</p>
              <div className="event-stack">
                {bootstrap.events.slice(0, 4).map((entry) => (
                  <article key={entry.id} className="event-line">
                    <strong>{entry.target}</strong>
                    <span>{entry.message}</span>
                  </article>
                ))}
              </div>
            </section>
          </aside>
        </div>
      </main>
    </div>
  );
}
