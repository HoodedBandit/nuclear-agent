import { useEffect, useMemo, useRef, useState } from "react";
import clsx from "clsx";
import {
  NavLink,
  Outlet,
  useLocation,
  useNavigate
} from "react-router-dom";
import { useDashboardData } from "../../app/dashboard-data";
import {
  NAV_GROUPS,
  NAV_ITEMS,
  itemForPath,
  type DashboardNavGroupKey,
  type DashboardNavItem
} from "./navigation";
import { ParityGapsDrawer } from "./ParityGapsDrawer";

function RouteGlyph(props: { icon: DashboardNavItem["icon"] }) {
  switch (props.icon) {
    case "chat":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 5h16v10H8l-4 4V5Z" />
        </svg>
      );
    case "overview":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 18h16M6 16V9m6 7V5m6 11v-4" />
        </svg>
      );
    case "channels":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M5 5h14v9H8l-3 3V5Zm3 14h11" />
        </svg>
      );
    case "sessions":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 4h10v16H7zM9 8h6M9 12h6M9 16h4" />
        </svg>
      );
    case "logs":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M6 4h12v16H6zM9 8h6M9 12h6M9 16h6" />
        </svg>
      );
    case "automation":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 2v4m0 12v4M4 12h4m12 0h4M6.5 6.5l2.8 2.8m5.4 5.4 2.8 2.8m0-11-2.8 2.8m-5.4 5.4-2.8 2.8" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      );
    case "skills":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="m12 3 2.6 5.2 5.7.8-4.1 4 1 5.7L12 16l-5.2 2.7 1-5.7-4.1-4 5.7-.8Z" />
        </svg>
      );
    case "infrastructure":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 7h16M4 12h16M4 17h16M7 4v16M17 4v16" />
        </svg>
      );
    case "config":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 3v3m0 12v3M5.6 5.6 7.8 7.8m8.4 8.4 2.2 2.2M3 12h3m12 0h3M5.6 18.4l2.2-2.2m8.4-8.4 2.2-2.2" />
          <circle cx="12" cy="12" r="4" />
        </svg>
      );
    case "debug":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M8 6h8v8H8zM10 2h4M6 10H3m18 0h-3M7 17l-2 3m12-3 2 3" />
        </svg>
      );
    default:
      return null;
  }
}

function UtilityGlyph(props: { kind: "search" | "focus" | "dock" | "gaps" | "logout" }) {
  switch (props.kind) {
    case "search":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle cx="11" cy="11" r="6" />
          <path d="m16 16 4 4" />
        </svg>
      );
    case "focus":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
        </svg>
      );
    case "dock":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M4 5h5v14H4zM11 5h9v14h-9z" />
        </svg>
      );
    case "gaps":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 6h10M7 12h6M7 18h10" />
          <circle cx="17" cy="12" r="2" />
        </svg>
      );
    case "logout":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M15 7V4H5v16h10v-3" />
          <path d="m10 12 9 0m0 0-3-3m3 3-3 3" />
        </svg>
      );
    default:
      return null;
  }
}

function ChevronGlyph(props: { collapsed?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d={props.collapsed ? "m9 6 6 6-6 6" : "m15 6-6 6 6 6"} />
    </svg>
  );
}

function SectionChevron() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function createGroupState() {
  return Object.fromEntries(NAV_GROUPS.map((group) => [group.key, false])) as Record<
    DashboardNavGroupKey,
    boolean
  >;
}

export function AppShell() {
  const { onLogout, bootstrap } = useDashboardData();
  const location = useLocation();
  const navigate = useNavigate();
  const { status, trust } = bootstrap;
  const mainTarget = status.main_target;
  const [navCollapsed, setNavCollapsed] = useState(false);
  const [navDrawerOpen, setNavDrawerOpen] = useState(false);
  const [gapDrawerOpen, setGapDrawerOpen] = useState(false);
  const [chatFocus, setChatFocus] = useState(false);
  const [commandOpen, setCommandOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [groupCollapsed, setGroupCollapsed] = useState(createGroupState);
  const commandInputRef = useRef<HTMLInputElement | null>(null);
  const activeNav = useMemo(() => itemForPath(location.pathname), [location.pathname]);

  useEffect(() => {
    setNavDrawerOpen(false);
    if (activeNav.key !== "chat") {
      setChatFocus(false);
    }
    setGroupCollapsed((current) =>
      current[activeNav.group] ? { ...current, [activeNav.group]: false } : current
    );
  }, [activeNav.group, activeNav.key]);

  useEffect(() => {
    if (!commandOpen) {
      setCommandQuery("");
      return;
    }
    const timer = window.setTimeout(() => {
      commandInputRef.current?.focus();
      commandInputRef.current?.select();
    }, 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [commandOpen]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandOpen(true);
        return;
      }
      if (event.key === "Escape") {
        setCommandOpen(false);
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  const navGroups = useMemo(
    () =>
      NAV_GROUPS.map((group) => ({
        ...group,
        items: NAV_ITEMS.filter((item) => item.group === group.key)
      })),
    []
  );

  const commandMatches = useMemo(() => {
    const query = commandQuery.trim().toLowerCase();
    if (!query) {
      return NAV_ITEMS;
    }
    return NAV_ITEMS.filter((item) =>
      `${item.label} ${item.group} ${item.subtitle}`.toLowerCase().includes(query)
    );
  }, [commandQuery]);

  const shellClasses = clsx(
    "shell",
    navCollapsed && "shell--nav-collapsed",
    navDrawerOpen && "shell--nav-drawer-open",
    activeNav.key === "chat" && chatFocus && "shell--chat-focus"
  );
  const policyLabel = trust.allow_shell
    ? trust.allow_network
      ? "shell + net"
      : "shell"
    : "read only";
  const footerMeta = `${mainTarget?.alias || "No target"} · ${policyLabel}`;

  function isActive(item: DashboardNavItem) {
    return [item.path, ...(item.aliases || [])].includes(location.pathname);
  }

  function toggleGroup(groupKey: DashboardNavGroupKey) {
    setGroupCollapsed((current) => ({
      ...current,
      [groupKey]: !current[groupKey]
    }));
  }

  function closeCommandPalette() {
    setCommandOpen(false);
    setCommandQuery("");
  }

  function openCommandPalette() {
    setCommandOpen(true);
  }

  function navigateToItem(item: DashboardNavItem) {
    navigate(item.path);
    closeCommandPalette();
  }

  async function handleLogout() {
    closeCommandPalette();
    await onLogout();
  }

  return (
    <>
      <div className={shellClasses}>
        <header className="topbar">
          <div className="topnav-shell">
            <button
              type="button"
              className="topbar-nav-toggle icon-button"
              aria-label="Toggle navigation"
              aria-expanded={navDrawerOpen}
              onClick={() => setNavDrawerOpen((current) => !current)}
            >
              <span />
              <span />
              <span />
            </button>
            <div className="topnav-shell__content">
              <div className="dashboard-header">
                <div className="dashboard-header__breadcrumb">
                  <span className="dashboard-header__breadcrumb-link">Nuclear</span>
                  <span className="dashboard-header__breadcrumb-sep">›</span>
                  <span className="dashboard-header__breadcrumb-current">
                    {activeNav.label}
                  </span>
                </div>
              </div>
            </div>
            <div className="topnav-shell__actions">
              <button
                type="button"
                className="topbar-search"
                onClick={openCommandPalette}
                aria-label="Search pages"
              >
                <span className="topbar-search__icon">
                  <UtilityGlyph kind="search" />
                </span>
                <span className="topbar-search__label">Search</span>
                <span className="topbar-search__kbd">Ctrl+K</span>
              </button>
              <div className="topbar-actions">
                {activeNav.key === "chat" ? (
                  <button
                    type="button"
                    className="topbar-utility"
                    onClick={() => setChatFocus((current) => !current)}
                    aria-label={chatFocus ? "Restore navigation" : "Focus chat"}
                    title={chatFocus ? "Restore navigation" : "Focus chat"}
                  >
                    <UtilityGlyph kind={chatFocus ? "dock" : "focus"} />
                  </button>
                ) : null}
                <button
                  type="button"
                  className="topbar-utility"
                  onClick={() => setGapDrawerOpen(true)}
                  aria-label="Open parity gaps"
                  title="Open parity gaps"
                >
                  <UtilityGlyph kind="gaps" />
                </button>
                <button
                  type="button"
                  className="topbar-utility"
                  onClick={() => {
                    void handleLogout();
                  }}
                  aria-label="Logout"
                  title="Logout"
                >
                  <UtilityGlyph kind="logout" />
                </button>
              </div>
            </div>
          </div>
        </header>

        <aside className="shell-nav" data-testid="modern-nav-rail">
          <div className={clsx("sidebar", navCollapsed && "sidebar--collapsed")}>
            <div className="sidebar-shell">
              <div className="sidebar-shell__header">
                <div className="sidebar-brand">
                  <div className="sidebar-brand__logo">N</div>
                  {!navCollapsed ? (
                    <div className="sidebar-brand__copy">
                      <span className="sidebar-brand__eyebrow">Control</span>
                      <strong className="sidebar-brand__title">Nuclear</strong>
                    </div>
                  ) : null}
                </div>
                <button
                  type="button"
                  className="nav-collapse-toggle icon-button"
                  aria-label={navCollapsed ? "Expand navigation" : "Collapse navigation"}
                  onClick={() => setNavCollapsed((current) => !current)}
                >
                  <ChevronGlyph collapsed={navCollapsed} />
                </button>
              </div>

              <div className="sidebar-shell__body sidebar-nav">
                {navGroups.map((group) => {
                  const sectionCollapsed = !navCollapsed && groupCollapsed[group.key];
                  return (
                    <section
                      key={group.key}
                      className={clsx(
                        "nav-section",
                        sectionCollapsed && "nav-section--collapsed"
                      )}
                    >
                      {!navCollapsed ? (
                        <button
                          type="button"
                          className="nav-section__label"
                          onClick={() => toggleGroup(group.key)}
                          aria-expanded={!sectionCollapsed}
                        >
                          <span className="nav-section__label-text">{group.label}</span>
                          <span className="nav-section__chevron">
                            <SectionChevron />
                          </span>
                        </button>
                      ) : null}
                      <div className="nav-section__items">
                        {group.items.map((item) => (
                          <NavLink
                            key={item.key}
                            to={item.path}
                            className={clsx("nav-item", isActive(item) && "nav-item--active")}
                            data-testid={`nav-${item.key}`}
                          >
                            <span className="nav-item__icon">
                              <RouteGlyph icon={item.icon} />
                            </span>
                            {!navCollapsed ? (
                              <span className="nav-item__text">{item.label}</span>
                            ) : null}
                          </NavLink>
                        ))}
                      </div>
                    </section>
                  );
                })}
              </div>

              <div className="sidebar-shell__footer">
                <div className="sidebar-utility-group">
                  <button
                    type="button"
                    className="nav-item sidebar-utility-link"
                    onClick={() => setGapDrawerOpen(true)}
                  >
                    <span className="nav-item__icon">
                      <UtilityGlyph kind="gaps" />
                    </span>
                    <span className="nav-item__text">Parity gaps</span>
                  </button>
                  <button
                    type="button"
                    className="nav-item sidebar-utility-link"
                    onClick={() => {
                      void handleLogout();
                    }}
                  >
                    <span className="nav-item__icon">
                      <UtilityGlyph kind="logout" />
                    </span>
                    <span className="nav-item__text">Sign out</span>
                  </button>
                </div>
                <div className="sidebar-version">
                  {!navCollapsed ? (
                    <div className="sidebar-version__copy">
                      <span className="sidebar-version__label">Runtime</span>
                      <span className="sidebar-version__text">{footerMeta}</span>
                    </div>
                  ) : null}
                  <span className="sidebar-version__status" aria-label="Connected" />
                </div>
              </div>
            </div>
          </div>
        </aside>

        <button
          type="button"
          className="shell-nav-backdrop"
          aria-label="Close navigation"
          onClick={() => setNavDrawerOpen(false)}
        />

        <main className="content" data-testid="modern-main-workspace">
          <div className="content__inner">
            <Outlet />
          </div>
        </main>
      </div>

      <button
        type="button"
        className={clsx("drawer-backdrop", commandOpen && "drawer-backdrop--open")}
        aria-label="Close search"
        onClick={closeCommandPalette}
      />
      <aside
        className={clsx("command-palette", commandOpen && "command-palette--open")}
        role="dialog"
        aria-modal="true"
        aria-hidden={!commandOpen}
        aria-label="Search pages"
      >
        <div className="command-palette__header">
          <div>
            <p className="section-label">Quick nav</p>
            <h2>Search pages</h2>
          </div>
          <button type="button" className="icon-button" onClick={closeCommandPalette}>
            Close
          </button>
        </div>
        <div className="command-palette__body">
          <label className="command-palette__field">
            <span className="sr-only">Search pages</span>
            <input
              ref={commandInputRef}
              value={commandQuery}
              onChange={(event) => setCommandQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && commandMatches[0]) {
                  event.preventDefault();
                  navigateToItem(commandMatches[0]);
                }
              }}
              placeholder="Search chat, config, logs..."
            />
          </label>
          <div className="command-palette__list">
            {commandMatches.length > 0 ? (
              commandMatches.map((item) => (
                <button
                  key={item.key}
                  type="button"
                  className="command-palette__result"
                  onClick={() => navigateToItem(item)}
                >
                  <span className="command-palette__result-icon">
                    <RouteGlyph icon={item.icon} />
                  </span>
                  <span className="command-palette__result-copy">
                    <strong>{item.label}</strong>
                    <span>{item.group}</span>
                  </span>
                </button>
              ))
            ) : (
              <div className="empty-state">
                <h3>No matching page</h3>
                <p>Try a route name such as config, sessions, or infrastructure.</p>
              </div>
            )}
          </div>
        </div>
      </aside>

      <ParityGapsDrawer open={gapDrawerOpen} onClose={() => setGapDrawerOpen(false)} />
    </>
  );
}
