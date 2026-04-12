import { useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { useDashboardData } from "../../app/dashboard-data";
import { useShellBootstrap } from "../../app/dashboard-selectors";
import { Badge } from "../../components/Badge";
import { DisclosureSection } from "../../components/DisclosureSection";

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
  const { onLogout } = useDashboardData();
  const { status, sessions, events, permissions, trust, delegationConfig } =
    useShellBootstrap();
  const location = useLocation();
  const mainTarget = status.main_target;
  const [inspectorOpen, setInspectorOpen] = useState(location.pathname === "/");
  const connectorCount =
    status.telegram_connectors +
    status.discord_connectors +
    status.slack_connectors +
    status.signal_connectors +
    status.home_assistant_connectors +
    status.inbox_connectors +
    status.webhook_connectors +
    status.gmail_connectors +
    status.brave_connectors;
  const activeNav = useMemo(
    () =>
      NAV_ITEMS.find((item) =>
        item.to === "/" ? location.pathname === "/" : location.pathname.startsWith(item.to)
      ) || NAV_ITEMS[0],
    [location.pathname]
  );

  useEffect(() => {
    setInspectorOpen(location.pathname === "/");
  }, [location.pathname]);

  return (
    <div className="app-shell">
      <aside className="nav-rail" data-testid="modern-nav-rail">
        <section className="brand-block">
          <div className="brand-block__mark">N</div>
          <div>
            <p className="eyebrow">Operator cockpit</p>
            <h1>Nuclear Agent</h1>
            <p className="brand-block__meta">
              Dense control surface with staged detail instead of always-open clutter.
            </p>
          </div>
        </section>

        <section className="rail-stats" aria-label="Cockpit summary">
          <article className="rail-stat">
            <span>Providers</span>
            <strong>{status.providers}</strong>
          </article>
          <article className="rail-stat">
            <span>Connectors</span>
            <strong>{connectorCount}</strong>
          </article>
          <article className="rail-stat">
            <span>Missions</span>
            <strong>{status.active_missions}</strong>
          </article>
          <article className="rail-stat">
            <span>Sessions</span>
            <strong>{sessions.length}</strong>
          </article>
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

        <section className="rail-note">
          <p className="eyebrow">Main target</p>
          <strong>{mainTarget?.alias || "Not configured"}</strong>
          <p>{mainTarget ? `${mainTarget.provider_display_name} -> ${mainTarget.model}` : "Arm a provider alias to establish a primary target."}</p>
        </section>
      </aside>

      <main className="workspace-shell" data-testid="modern-main-workspace">
        <header className="top-command-deck">
          <div className="deck-heading">
            <p className="eyebrow">Resident control plane</p>
            <h2>{activeNav.label}</h2>
            <p className="deck-copy">
              {activeNav.eyebrow}
            </p>
          </div>
          <div className="shell-actions">
            <div className="status-strip">
              <Badge tone={navTone(location.pathname)}>
                {status.autonomy.state}
              </Badge>
              <Badge tone={status.active_missions > 0 ? "warn" : "neutral"}>
                {status.active_missions} active missions
              </Badge>
              <Badge tone={status.pending_memory_reviews > 0 ? "warn" : "good"}>
                {status.pending_memory_reviews} memory reviews
              </Badge>
              <Badge tone={mainTarget ? "info" : "danger"}>
                {mainTarget?.alias || "no main target"}
              </Badge>
            </div>
            <div className="shell-actions__buttons">
              <button
                className="button--ghost"
                type="button"
                aria-expanded={inspectorOpen}
                onClick={() => setInspectorOpen((current) => !current)}
              >
                {inspectorOpen ? "Hide context" : "Open context"}
              </button>
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
          </div>
        </header>

        <div
          className={clsx("content-grid", inspectorOpen && "content-grid--with-inspector")}
        >
          <div className="content-grid__main">
            <Outlet />
          </div>
          {inspectorOpen ? (
            <aside className="inspector-rail" data-testid="modern-right-inspector">
              <section className="inspector-card context-frame">
                <div className="context-frame__header">
                  <p className="eyebrow">Context drawer</p>
                  <p className="context-frame__copy">
                    Keep secondary posture data nearby without pinning it onto every page.
                  </p>
                </div>
                <DisclosureSection
                  title="Primary target"
                  subtitle="Main alias, model, and provider"
                  meta={mainTarget?.alias || "awaiting setup"}
                  defaultOpen
                >
                  <div className="stack-list">
                    <div className="fact-grid">
                      <article className="fact-card">
                        <span>Provider</span>
                        <strong>{mainTarget?.provider_display_name || "Awaiting setup"}</strong>
                      </article>
                      <article className="fact-card">
                        <span>Model</span>
                        <strong>{mainTarget?.model || "No model selected"}</strong>
                      </article>
                    </div>
                    <p className="helper-copy mono">
                      {mainTarget
                        ? `${mainTarget.alias} -> ${mainTarget.model}`
                        : "Create a provider and alias to arm the cockpit."}
                    </p>
                  </div>
                </DisclosureSection>
                <DisclosureSection
                  title="Policy posture"
                  subtitle="Permissions, trust, and delegation depth"
                  meta={permissions}
                >
                  <ul className="micro-list">
                    <li>Permissions preset: {permissions}</li>
                    <li>Shell trust: {trust.allow_shell ? "allowed" : "blocked"}</li>
                    <li>Network trust: {trust.allow_network ? "allowed" : "blocked"}</li>
                    <li>
                      Delegation depth:{" "}
                      {delegationConfig.max_depth.mode === "limited"
                        ? delegationConfig.max_depth.value
                        : "unlimited"}
                    </li>
                  </ul>
                </DisclosureSection>
                <DisclosureSection
                  title="Recent event feed"
                  subtitle="Latest daemon activity"
                  meta={`${Math.min(events.length, 4)} visible`}
                >
                  <div className="event-stack scroll-stack">
                    {events.slice(0, 4).map((entry) => (
                      <article key={entry.id} className="event-line">
                        <strong>{entry.target}</strong>
                        <span>{entry.message}</span>
                      </article>
                    ))}
                  </div>
                </DisclosureSection>
              </section>
            </aside>
          ) : null}
        </div>
      </main>
    </div>
  );
}
