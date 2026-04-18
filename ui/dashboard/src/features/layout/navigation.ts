export type DashboardNavGroupKey = "chat" | "control" | "agent" | "settings";

export type DashboardNavItemKey =
  | "chat"
  | "overview"
  | "channels"
  | "sessions"
  | "logs"
  | "automation"
  | "skills"
  | "infrastructure"
  | "config"
  | "debug";

export interface DashboardNavGroup {
  key: DashboardNavGroupKey;
  label: string;
}

export interface DashboardNavItem {
  key: DashboardNavItemKey;
  label: string;
  subtitle: string;
  group: DashboardNavGroupKey;
  path: string;
  icon: string;
  aliases?: string[];
}

export const NAV_GROUPS: DashboardNavGroup[] = [
  { key: "chat", label: "chat" },
  { key: "control", label: "control" },
  { key: "agent", label: "agent" },
  { key: "settings", label: "settings" }
];

export const NAV_ITEMS: DashboardNavItem[] = [
  {
    key: "chat",
    label: "Chat",
    subtitle: "Run tasks",
    group: "chat",
    path: "/chat",
    icon: "chat"
  },
  {
    key: "overview",
    label: "Overview",
    subtitle: "Health and repo",
    group: "control",
    path: "/overview",
    icon: "overview",
    aliases: ["/"]
  },
  {
    key: "channels",
    label: "Channels",
    subtitle: "Connectors",
    group: "control",
    path: "/channels",
    icon: "channels"
  },
  {
    key: "sessions",
    label: "Sessions",
    subtitle: "Resume and fork",
    group: "control",
    path: "/sessions",
    icon: "sessions"
  },
  {
    key: "automation",
    label: "Automation",
    subtitle: "Missions and memory",
    group: "agent",
    path: "/automation",
    icon: "automation",
    aliases: ["/operations"]
  },
  {
    key: "skills",
    label: "Skills",
    subtitle: "Drafts and enabled",
    group: "agent",
    path: "/skills",
    icon: "skills"
  },
  {
    key: "infrastructure",
    label: "Infrastructure",
    subtitle: "Providers and tools",
    group: "settings",
    path: "/infrastructure",
    icon: "infrastructure",
    aliases: ["/integrations"]
  },
  {
    key: "config",
    label: "Config",
    subtitle: "Policy and updates",
    group: "settings",
    path: "/config",
    icon: "config",
    aliases: ["/system"]
  },
  {
    key: "debug",
    label: "Debug",
    subtitle: "Doctor and bundle",
    group: "settings",
    path: "/debug",
    icon: "debug"
  },
  {
    key: "logs",
    label: "Logs",
    subtitle: "Event feed",
    group: "settings",
    path: "/logs",
    icon: "logs"
  }
];

export function itemForPath(pathname: string): DashboardNavItem {
  const normalized = pathname || "/";
  return (
    NAV_ITEMS.find((item) => {
      const candidates = [item.path, ...(item.aliases || [])];
      return candidates.includes(normalized);
    }) || NAV_ITEMS[0]
  );
}

export function groupLabelForKey(key: DashboardNavGroupKey): string {
  return NAV_GROUPS.find((group) => group.key === key)?.label || key;
}
