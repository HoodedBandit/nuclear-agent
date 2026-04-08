const state = {
  token: "",
  dashboardSessionAuthenticated: false,
  autoRefresh: true,
  refreshTimer: null,
  healthTimer: null,
  lastData: null,
  activeChatSessionId: null,
  pendingChatSessionId: null,
  activeTranscript: [],
  activeSessionResumePacket: null,
  sessionDetailPacket: null,
  sessionDetailSessionId: null,
  lazyObserver: null,
  loadedPanels: new Set(),
  renderCache: {},
  refreshInFlight: null,
  healthInFlight: null,
  panelInFlight: {},
  chatRunInFlight: false,
  controlSocket: null,
  controlSocketReady: false,
  controlSocketPromise: null,
  controlSocketRequests: new Map(),
  controlRequestCounter: 0,
  controlRefreshTimer: null,
  controlSocketClosing: false,
  activeDashboardTab: "overview",
  chatAttachments: [],
  chatCwd: "",
  consoleEntries: [],
};

const DASHBOARD_TAB_STORAGE_KEY = "dashboardActiveTab";
const DASHBOARD_TABS = [
  {
    id: "overview",
    label: "Overview",
    panels: ["overview", "workspace", "setup", "controls"],
  },
  {
    id: "chat",
    label: "Chat",
    panels: ["run-task", "sessions"],
  },
  {
    id: "operations",
    label: "Operations",
    panels: ["missions", "approvals", "memory", "skills", "profile", "memory-tools", "events"],
  },
  {
    id: "integrations",
    label: "Integrations",
    panels: ["connectors", "providers", "plugins", "delegation", "mcp"],
  },
  {
    id: "system",
    label: "System",
    panels: ["permissions", "logs", "daemon-config", "advanced"],
  },
];
const PANEL_TO_TAB = Object.fromEntries(
  DASHBOARD_TABS.flatMap((tab) => tab.panels.map((panelId) => [panelId, tab.id]))
);
const PANEL_LABELS = {
  overview: "Overview",
  workspace: "Workspace",
  setup: "Setup",
  controls: "Controls",
  missions: "Missions",
  approvals: "Approvals",
  memory: "Memory Review",
  skills: "Skills",
  profile: "Profile",
  "memory-tools": "Memory Tools",
  events: "Events",
  connectors: "Connectors",
  providers: "Providers",
  plugins: "Plugins",
  delegation: "Delegation",
  mcp: "MCP",
  permissions: "Permissions",
  logs: "Logs",
  "daemon-config": "Daemon Config",
  advanced: "Advanced",
  "run-task": "Chat",
  sessions: "Sessions",
};

const elements = {
  form: document.getElementById("auth-form"),
  tokenInput: document.getElementById("token-input"),
  refreshButton: document.getElementById("refresh-button"),
  clearButton: document.getElementById("clear-button"),
  autoRefreshInput: document.getElementById("autorefresh-input"),
  connectionStatus: document.getElementById("connection-status"),
  lastUpdated: document.getElementById("last-updated"),
  heroChips: document.getElementById("hero-chips"),
  statusCards: document.getElementById("status-cards"),
  statusDetails: document.getElementById("status-details"),
  healthSummary: document.getElementById("health-summary"),
  providerHealth: document.getElementById("provider-health"),
  controlSummary: document.getElementById("control-summary"),
  autopilotActions: document.getElementById("autopilot-actions"),
  autopilotDetails: document.getElementById("autopilot-details"),
  missionForm: document.getElementById("mission-form"),
  missionTitle: document.getElementById("mission-title"),
  missionDetails: document.getElementById("mission-details"),
  missionAlias: document.getElementById("mission-alias"),
  missionModel: document.getElementById("mission-model"),
  missionDelay: document.getElementById("mission-delay"),
  missionRepeat: document.getElementById("mission-repeat"),
  missionSchedule: document.getElementById("mission-schedule"),
  missionSummary: document.getElementById("mission-summary"),
  missionsBody: document.getElementById("missions-body"),
  approvalsSummary: document.getElementById("approvals-summary"),
  approvalsList: document.getElementById("approvals-list"),
  memorySummary: document.getElementById("memory-summary"),
  memoryList: document.getElementById("memory-list"),
  skillsSummary: document.getElementById("skills-summary"),
  skillsList: document.getElementById("skills-list"),
  profileSummary: document.getElementById("profile-summary"),
  profileList: document.getElementById("profile-list"),
  connectorSummary: document.getElementById("connector-summary"),
  connectorCards: document.getElementById("connector-cards"),
  delegationSummary: document.getElementById("delegation-summary"),
  delegationList: document.getElementById("delegation-list"),
  eventsSummary: document.getElementById("events-summary"),
  eventsList: document.getElementById("events-list"),
  memorySearchForm: document.getElementById("memory-search-form"),
  memorySearchQuery: document.getElementById("memory-search-query"),
  memorySearchResults: document.getElementById("memory-search-results"),
  memoryCreateForm: document.getElementById("memory-create-form"),
  memoryCreateKind: document.getElementById("memory-create-kind"),
  memoryCreateScope: document.getElementById("memory-create-scope"),
  memoryCreateSubject: document.getElementById("memory-create-subject"),
  memoryCreateContent: document.getElementById("memory-create-content"),
  memoryRebuildForm: document.getElementById("memory-rebuild-form"),
  memoryRebuildSessionId: document.getElementById("memory-rebuild-session-id"),
  memoryRebuildEmbeddings: document.getElementById("memory-rebuild-embeddings"),
  permissionsSummary: document.getElementById("permissions-summary"),
  permissionPresetActions: document.getElementById("permission-preset-actions"),
  permissionsDetails: document.getElementById("permissions-details"),
  trustToggles: document.getElementById("trust-toggles"),
  trustPathForm: document.getElementById("trust-path-form"),
  trustPathInput: document.getElementById("trust-path-input"),
  trustPaths: document.getElementById("trust-paths"),
  delegationForm: document.getElementById("delegation-form"),
  delegationMaxDepth: document.getElementById("delegation-max-depth"),
  delegationMaxParallel: document.getElementById("delegation-max-parallel"),
  delegationDisabledProviders: document.getElementById("delegation-disabled-providers"),
  runTaskForm: document.getElementById("run-task-form"),
  runTaskPrompt: document.getElementById("run-task-prompt"),
  runTaskAlias: document.getElementById("run-task-alias"),
  runTaskModel: document.getElementById("run-task-model"),
  runTaskThinking: document.getElementById("run-task-thinking"),
  runTaskMode: document.getElementById("run-task-mode"),
  runTaskPermission: document.getElementById("run-task-permission"),
  runTaskResult: document.getElementById("run-task-result"),
  chatSessionMeta: document.getElementById("chat-session-meta"),
  chatMainTarget: document.getElementById("chat-main-target"),
  chatTranscript: document.getElementById("chat-transcript"),
  chatMakeMainButton: document.getElementById("chat-make-main-button"),
  chatNewSession: document.getElementById("chat-new-session"),
  chatRenameButton: document.getElementById("chat-rename-button"),
  sessionsSummary: document.getElementById("sessions-summary"),
  sessionsBody: document.getElementById("sessions-body"),
  sessionDetail: document.getElementById("session-detail"),
  logsSummary: document.getElementById("logs-summary"),
  logsList: document.getElementById("logs-list"),
  mcpSummary: document.getElementById("mcp-summary"),
  mcpList: document.getElementById("mcp-list"),
  mcpAddForm: document.getElementById("mcp-add-form"),
  mcpAddId: document.getElementById("mcp-add-id"),
  mcpAddName: document.getElementById("mcp-add-name"),
  mcpAddCommand: document.getElementById("mcp-add-command"),
  mcpAddArgs: document.getElementById("mcp-add-args"),
  mcpAddEnabled: document.getElementById("mcp-add-enabled"),
  daemonConfigSummary: document.getElementById("daemon-config-summary"),
  daemonPersistenceActions: document.getElementById("daemon-persistence-actions"),
  daemonAutostartActions: document.getElementById("daemon-autostart-actions"),
  dashboardTabLinks: document.getElementById("dashboard-tab-links"),
  dashboardTabButtons: Array.from(document.querySelectorAll("[data-dashboard-tab-trigger]")),
  chatForkButton: document.getElementById("chat-fork-button"),
  chatCompactButton: document.getElementById("chat-compact-button"),
  chatStatusShortcut: document.getElementById("chat-status-shortcut"),
  chatDiffShortcut: document.getElementById("chat-diff-shortcut"),
  chatReviewShortcut: document.getElementById("chat-review-shortcut"),
  chatCopyShortcut: document.getElementById("chat-copy-shortcut"),
  chatCwd: document.getElementById("chat-cwd"),
  chatUseDaemonCwd: document.getElementById("chat-use-daemon-cwd"),
  chatUseSessionCwd: document.getElementById("chat-use-session-cwd"),
  chatUseWorkspaceCwd: document.getElementById("chat-use-workspace-cwd"),
  chatAttachmentPath: document.getElementById("chat-attachment-path"),
  chatAttachmentAdd: document.getElementById("chat-attachment-add"),
  chatAttachments: document.getElementById("chat-attachments"),
  chatAttachmentsClear: document.getElementById("chat-attachments-clear"),
};

function bearerHeaders() {
  return state.token ? { Authorization: `Bearer ${state.token}` } : {};
}

function hasDashboardAuth() {
  return state.dashboardSessionAuthenticated || !!state.token;
}

function isUnauthorizedError(error) {
  return error?.status === 401 || String(error?.message || "").startsWith("401 ");
}

async function apiRequest(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    credentials: "same-origin",
    headers: {
      ...(options.headers || {}),
      ...bearerHeaders(),
    },
  });
  if (!response.ok) {
    const text = await response.text();
    const error = new Error(
      `${response.status} ${response.statusText}${text ? `: ${text}` : ""}`
    );
    error.status = response.status;
    throw error;
  }
  if (response.status === 204) {
    return null;
  }
  const contentType = response.headers.get("content-type") || "";
  if (contentType.includes("application/json")) {
    return response.json();
  }
  return response.text();
}

function apiGet(path) {
  return apiRequest(path);
}

function apiPost(path, payload) {
  return apiRequest(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload ?? {}),
  });
}

function apiPut(path, payload) {
  return apiRequest(path, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload ?? {}),
  });
}

function apiDelete(path) {
  return apiRequest(path, { method: "DELETE" });
}

function saveActiveDashboardTab(tabId) {
  try {
    window.localStorage.setItem(DASHBOARD_TAB_STORAGE_KEY, tabId);
  } catch (_) {
  }
}

function loadActiveDashboardTab() {
  try {
    const saved = window.localStorage.getItem(DASHBOARD_TAB_STORAGE_KEY);
    return DASHBOARD_TABS.some((tab) => tab.id === saved) ? saved : "overview";
  } catch (_) {
    return "overview";
  }
}

function tabDefinition(tabId) {
  return DASHBOARD_TABS.find((tab) => tab.id === tabId) || DASHBOARD_TABS[0];
}

function panelElement(panelId) {
  return document.getElementById(panelId);
}

function updateDashboardTabButtons() {
  elements.dashboardTabButtons.forEach((button) => {
    const active = button.dataset.dashboardTabTrigger === state.activeDashboardTab;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  });
}

function renderDashboardTabLinks() {
  const container = elements.dashboardTabLinks;
  if (!container) {
    return;
  }
  const tab = tabDefinition(state.activeDashboardTab);
  container.innerHTML = tab.panels
    .map(
      (panelId) => `
        <a href="#${panelId}" class="${window.location.hash === `#${panelId}` ? "is-active" : ""}">${escapeHtml(
          PANEL_LABELS[panelId] || panelId
        )}</a>
      `
    )
    .join("");
}

function updateDashboardPanelVisibility() {
  document.querySelectorAll("[data-dashboard-tab]").forEach((panel) => {
    panel.hidden = panel.dataset.dashboardTab !== state.activeDashboardTab;
  });
}

function forcePanelsForTab(tabId) {
  return tabDefinition(tabId).panels.filter((panelId) => lazyPanelLoaders[panelId]);
}

function activateDashboardTab(tabId, { persist = true, scrollToTop = false } = {}) {
  const tab = tabDefinition(tabId);
  state.activeDashboardTab = tab.id;
  updateDashboardTabButtons();
  updateDashboardPanelVisibility();
  renderDashboardTabLinks();
  if (persist) {
    saveActiveDashboardTab(tab.id);
  }
  if (scrollToTop) {
    document.querySelector(".dashboard-nav")?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  if (hasDashboardAuth()) {
    refreshLoadedPanels(forcePanelsForTab(tab.id)).catch((error) => {
      setStatus(`Panel load failed: ${error.message}`, "warn");
    });
  }
}

function activateTabForPanel(panelId) {
  const tabId = PANEL_TO_TAB[panelId];
  if (tabId) {
    activateDashboardTab(tabId, { persist: true });
  }
}

window.dashboardApp = {
  apiDelete,
  apiGet,
  apiPost,
  apiPut,
  activateTab: activateDashboardTab,
  getLastData: () => state.lastData,
  hasDashboardAuth,
  focusSection,
  quickStartProvider,
  refreshDashboard,
  renderEmpty,
  setStatus,
  updateMainAlias,
};

function focusSection(id) {
  activateTabForPanel(id);
  renderDashboardTabLinks();
  ensurePanelLoaded(id).catch(() => {});
  document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

async function quickStartProvider(presetKey) {
  if (window.dashboardProviders?.quickStart) {
    await window.dashboardProviders.quickStart(presetKey);
    return;
  }
  focusSection("providers");
}

async function apiStream(path, payload, onEvent) {
  const response = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...bearerHeaders(),
    },
    body: JSON.stringify(payload ?? {}),
  });
  if (!response.ok) {
    const text = await response.text();
    const error = new Error(
      `${response.status} ${response.statusText}${text ? `: ${text}` : ""}`
    );
    error.status = response.status;
    throw error;
  }
  if (!response.body) {
    throw new Error("Streaming response body was not available.");
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    let newlineIndex = buffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const line = buffer.slice(0, newlineIndex).trim();
      buffer = buffer.slice(newlineIndex + 1);
      if (line) {
        onEvent(JSON.parse(line));
      }
      newlineIndex = buffer.indexOf("\n");
    }
  }
  const trailing = buffer.trim();
  if (trailing) {
    onEvent(JSON.parse(trailing));
  }
}

async function createDashboardSession(token) {
  const trimmed = token.trim();
  if (!trimmed) {
    throw new Error("A daemon token is required.");
  }
  const response = await fetch("/auth/dashboard/session", {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ token: trimmed }),
  });
  if (!response.ok) {
    const text = await response.text();
    const error = new Error(
      `${response.status} ${response.statusText}${text ? `: ${text}` : ""}`
    );
    error.status = response.status;
    throw error;
  }
  state.dashboardSessionAuthenticated = true;
  state.token = "";
  elements.tokenInput.value = "";
  closeControlSocket({ rejectMessage: "Dashboard session was replaced." });
}

async function clearDashboardSession() {
  closeControlSocket({ rejectMessage: "Dashboard session ended." });
  await fetch("/auth/dashboard/session", {
    method: "DELETE",
    credentials: "same-origin",
  });
  state.dashboardSessionAuthenticated = false;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function fmt(value) {
  if (value === null || value === undefined || value === "") {
    return "-";
  }
  return String(value);
}

function fmtList(values) {
  return Array.isArray(values) && values.length ? values.join(", ") : "-";
}

function fmtDate(value) {
  if (!value) {
    return "-";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return fmt(value);
  }
  return `${date.toLocaleDateString()} ${date.toLocaleTimeString()}`;
}

function fmtBoolean(value) {
  return value ? "yes" : "no";
}

function parseDelimitedList(value, mapper = (item) => item) {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean)
    .map(mapper);
}

function parseLimitInput(value, fallback) {
  const normalized = value.trim().toLowerCase();
  if (!normalized) {
    return fallback;
  }
  if (normalized === "unlimited") {
    return { mode: "unlimited" };
  }
  const parsed = Number.parseInt(normalized, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    throw new Error("Delegation limits must be a positive integer or 'unlimited'.");
  }
  return { mode: "limited", value: parsed };
}

function displayLimit(limit) {
  if (!limit) {
    return "-";
  }
  return limit.mode === "unlimited" ? "unlimited" : fmt(limit.value);
}

function setStatus(text, tone = "neutral") {
  elements.connectionStatus.textContent = text;
  elements.connectionStatus.dataset.tone = tone;
}

function badge(label, tone = "info") {
  return `<span class="badge" data-tone="${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
}

function heroChip(label) {
  return `<span class="health-pill">${escapeHtml(label)}</span>`;
}

const ACTION_DATASET_KEYS = new Set([
  "approvalApprove",
  "approvalReject",
  "memoryApprove",
  "memoryReject",
  "skillPublish",
  "skillReject",
  "missionPause",
  "missionResume",
  "missionDelay",
  "missionCancel",
  "autopilot",
  "autonomy",
  "evolve",
  "providerEdit",
  "providerModels",
  "providerClearCreds",
  "providerDelete",
  "aliasEdit",
  "aliasMakeMain",
  "aliasDelete",
  "permissionPreset",
  "trustFlag",
  "memoryDelete",
  "useModel",
  "sessionView",
  "sessionUse",
  "sessionFork",
  "sessionRename",
  "chatAttachmentRemove",
  "mcpDelete",
  "daemonPersistence",
  "daemonAutostart",
]);

function dataAttributeName(key) {
  return key.replace(/[A-Z]/g, (match) => `-${match.toLowerCase()}`);
}

function buttonHtml(label, datasetEntries, klass = "") {
  const attrs = Object.entries(datasetEntries)
    .map(
      ([key, value]) =>
        `data-${escapeHtml(dataAttributeName(key))}="${escapeHtml(value)}"`
    )
    .join(" ");
  return `<button class="button-small ${klass}" ${attrs}>${escapeHtml(label)}</button>`;
}

function hasActionDataset(element) {
  return Object.keys(element?.dataset || {}).some((key) =>
    ACTION_DATASET_KEYS.has(key)
  );
}

function findActionTarget(start) {
  let current =
    start instanceof HTMLElement
      ? start
      : start instanceof Node
        ? start.parentElement
        : null;
  while (current && current !== document.body) {
    if (hasActionDataset(current)) {
      return current;
    }
    current = current.parentElement;
  }
  if (current && hasActionDataset(current)) {
    return current;
  }
  return null;
}

function renderEmpty(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

function formatMemoryEvidence(memory, limit = 2) {
  const refs = Array.isArray(memory?.evidence_refs) ? memory.evidence_refs : [];
  if (!refs.length) {
    return memory?.source_session_id ? `source session ${memory.source_session_id}` : "no evidence refs";
  }
  const parts = refs.slice(0, limit).map((ref) => {
    const pieces = [];
    if (ref.session_id) {
      pieces.push(`session ${ref.session_id}`);
    }
    if (ref.role) {
      pieces.push(String(ref.role));
    }
    if (ref.tool_name) {
      pieces.push(`tool ${ref.tool_name}`);
    }
    if (ref.tool_call_id) {
      pieces.push(`call ${ref.tool_call_id}`);
    }
    if (ref.message_id) {
      pieces.push(`msg ${ref.message_id}`);
    }
    return pieces.join(" / ") || "evidence";
  });
  if (refs.length > limit) {
    parts.push(`+${refs.length - limit} more`);
  }
  return parts.join(" | ");
}

function renderMemoryProvenance(memory) {
  const items = [];
  if (memory.observation_source) {
    items.push(`observation: ${memory.observation_source}`);
  }
  if (memory.source_session_id) {
    items.push(`session: ${memory.source_session_id}`);
  }
  if (memory.source_message_id) {
    items.push(`message: ${memory.source_message_id}`);
  }
  items.push(`evidence: ${formatMemoryEvidence(memory)}`);
  return items;
}

function renderMemoryCard(memory, { actions = "", showReview = true } = {}) {
  const provenance = renderMemoryProvenance(memory);
  return `
    <article class="stack-card">
      <div class="card-title-row">
        <div>
          <h3>${escapeHtml(memory.subject)}</h3>
          <p class="card-subtitle">${escapeHtml(memory.kind)} · ${escapeHtml(memory.scope)}</p>
        </div>
        ${badge(`confidence ${fmt(memory.confidence)}`, memory.confidence >= 80 ? "good" : "warn")}
      </div>
      <p class="card-copy">${escapeHtml(memory.content)}</p>
      <ul class="micro-list">
        ${provenance.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}
        <li>workspace: ${escapeHtml(fmt(memory.workspace_key))}</li>
        <li>provider: ${escapeHtml(fmt(memory.provider_id))}</li>
        <li>updated: ${escapeHtml(fmtDate(memory.updated_at))}</li>
      </ul>
      ${
        showReview
          ? `<div class="inline-actions">
              ${actions}
            </div>`
          : ""
      }
    </article>
  `;
}

function renderSessionResumePacket(packet) {
  if (!packet) {
    return renderEmpty("No session selected.");
  }
  const linkedMemories = Array.isArray(packet.linked_memories) ? packet.linked_memories : [];
  const relatedHits = Array.isArray(packet.related_transcript_hits) ? packet.related_transcript_hits : [];
  return `
    <article class="stack-card stack-card--resume">
      <div class="card-title-row">
        <div>
          <h3>${escapeHtml(packet.session?.title || packet.session?.id || "Session")}</h3>
          <p class="card-subtitle">${escapeHtml(packet.session?.alias || "-")} · ${escapeHtml(packet.session?.model || "-")}</p>
        </div>
        ${badge(packet.session?.task_mode || "default", packet.session?.task_mode === "build" ? "good" : packet.session?.task_mode === "daily" ? "info" : "neutral")}
      </div>
      <ul class="micro-list">
        <li>session: ${escapeHtml(fmt(packet.session?.id))}</li>
        <li>provider: ${escapeHtml(fmt(packet.session?.provider_id))}</li>
        <li>cwd: ${escapeHtml(fmt(packet.session?.cwd))}</li>
        <li>messages: ${escapeHtml(fmt(packet.session?.message_count))}</li>
        <li>generated: ${escapeHtml(fmtDate(packet.generated_at))}</li>
      </ul>
      <div class="resume-section">
        <h4>Recent messages</h4>
        ${renderConversationThread(packet.recent_messages || [], "No recent messages available.")}
      </div>
      <div class="resume-section">
        <h4>Linked memories</h4>
        ${
          linkedMemories.length
            ? linkedMemories
                .map((memory) => renderMemoryCard(memory, { actions: "", showReview: false }))
                .join("")
            : renderEmpty("No linked memories.")
        }
      </div>
      <div class="resume-section">
        <h4>Related transcript hits</h4>
        ${
          relatedHits.length
            ? relatedHits
                .map(
                  (hit) => `
                    <article class="stack-card stack-card--compact">
                      <div class="card-title-row">
                        <div>
                          <h4>${escapeHtml(hit.session_id)}</h4>
                          <p class="card-subtitle">${escapeHtml(hit.role || "-")} · ${escapeHtml(fmtDate(hit.created_at))}</p>
                        </div>
                        ${badge("hit", "info")}
                      </div>
                      <p class="card-copy">${escapeHtml(hit.preview)}</p>
                    </article>
                  `
                )
                .join("")
            : renderEmpty("No related transcript hits.")
        }
      </div>
    </article>
  `;
}

function stableKey(value) {
  return JSON.stringify(value, (_key, current) => (current === undefined ? null : current));
}

function renderWhenChanged(key, value, render) {
  const next = stableKey(value);
  if (state.renderCache[key] === next) {
    return false;
  }
  state.renderCache[key] = next;
  render();
  return true;
}

function mergeLastData(patch) {
  state.lastData = {
    ...(state.lastData || {}),
    ...patch,
  };
  return state.lastData;
}

function panelElement(panelId) {
  return document.getElementById(panelId);
}

function isPanelNearViewport(panelId) {
  const panel = panelElement(panelId);
  if (!panel) {
    return false;
  }
  const rect = panel.getBoundingClientRect();
  return rect.top <= window.innerHeight * 1.2 && rect.bottom >= -window.innerHeight * 0.2;
}

function renderStatus(status) {
  const cards = [
    ["Providers", status.providers, `${status.aliases} aliases configured`],
    [
      "Delegation",
      status.delegation_targets,
      `${status.delegation.max_depth.mode === "unlimited" ? "unlimited" : status.delegation.max_depth.value} depth`,
    ],
    ["Missions", status.missions, `${status.active_missions} active`],
    [
      "Memories",
      status.memories,
      `${status.pending_memory_reviews} pending review${status.pending_memory_reviews === 1 ? "" : "s"}`,
    ],
    [
      "Skills",
      status.skill_drafts,
      `${status.published_skills} published`,
    ],
    ["Plugins", status.plugins || 0, "managed local packages"],
    ["Approvals", status.pending_connector_approvals, "connector gate queue"],
    [
      "Connectors",
      status.telegram_connectors +
        status.discord_connectors +
        status.slack_connectors +
        status.signal_connectors +
        status.home_assistant_connectors +
        status.webhook_connectors +
        status.inbox_connectors +
        (status.gmail_connectors || 0) +
        (status.brave_connectors || 0),
      "telegram, discord, slack, signal, home assistant, webhook, inbox, gmail, brave",
    ],
  ];
  elements.statusCards.innerHTML = cards
    .map(
      ([label, value, hint]) => `
        <article class="stat-card">
          <p class="stat-card__label">${escapeHtml(label)}</p>
          <p class="stat-card__value">${escapeHtml(value)}</p>
          <p class="stat-card__hint">${escapeHtml(hint)}</p>
        </article>
      `
    )
    .join("");

  const runtime = [
    ["PID", status.pid],
    ["Started", fmtDate(status.started_at)],
    ["Persistence", status.persistence_mode],
    ["Autonomy", `${status.autonomy.state} (${status.autonomy.mode})`],
    ["Evolve", status.evolve.state],
    ["Autopilot", status.autopilot.state],
    ["Wake interval", `${status.autopilot.wake_interval_seconds}s`],
    [
      "Delegation depth",
      status.delegation.max_depth.mode === "unlimited"
        ? "unlimited"
        : status.delegation.max_depth.value,
    ],
    [
      "Parallel subagents",
      status.delegation.max_parallel_subagents.mode === "unlimited"
        ? "unlimited"
        : status.delegation.max_parallel_subagents.value,
    ],
    ["Auto start", fmtBoolean(status.auto_start)],
  ];
  elements.statusDetails.innerHTML = runtime
    .map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(fmt(value))}</dd>`)
    .join("");

  elements.heroChips.innerHTML = [
    heroChip(`Autonomy: ${status.autonomy.state}/${status.autonomy.mode}`),
    heroChip(`Evolve: ${status.evolve.state}`),
    heroChip(`Autopilot: ${status.autopilot.state}`),
    heroChip(`Missions: ${status.active_missions}/${status.missions}`),
    heroChip(`Memories: ${status.memories}`),
    heroChip(`Memory review: ${status.pending_memory_reviews}`),
    heroChip(`Skills: ${status.skill_drafts}/${status.published_skills} published`),
    heroChip(`Plugins: ${status.plugins || 0}`),
    heroChip(`Approvals: ${status.pending_connector_approvals}`),
  ].join("");
}

function renderHealth(status, health) {
  if (!health) {
    elements.healthSummary.innerHTML = [
      heroChip("Health checks pending"),
      heroChip(`Config ${fmt(status?.persistence_mode)}`),
    ].join("");
    elements.providerHealth.innerHTML = renderEmpty(
      "Provider health checks run on a slower background cadence."
    );
    return;
  }

  elements.healthSummary.innerHTML = [
    heroChip(`Daemon ${health.daemon_running ? "running" : "down"}`),
    heroChip(`Keyring ${health.keyring_ok ? "ok" : "issue"}`),
    heroChip(`Config ${fmt(status?.persistence_mode)}`),
  ].join("");

  elements.providerHealth.innerHTML = health.providers.length
    ? health.providers
        .map(
          (provider) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h4>${escapeHtml(provider.id)}</h4>
                  <p class="card-subtitle">${escapeHtml(provider.detail)}</p>
                </div>
                ${badge(provider.ok ? "ok" : "issue", provider.ok ? "good" : "warn")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No provider health data.");
}

function renderAutopilot(status) {
  elements.controlSummary.textContent = `Autopilot ${status.autopilot.state} | autonomy ${status.autonomy.state}/${status.autonomy.mode} | evolve ${status.evolve.state}`;
  const autopilot = status.autopilot;
  const evolve = status.evolve;
  const actions = [];
  if (autopilot.state !== "enabled") {
    actions.push(buttonHtml("Enable", { autopilot: "enable" }));
  }
  if (autopilot.state === "enabled") {
    actions.push(buttonHtml("Pause", { autopilot: "pause" }, "button-muted"));
  }
  if (autopilot.state === "paused") {
    actions.push(buttonHtml("Resume", { autopilot: "resume" }));
  }
  actions.push(buttonHtml("Wake now", { autopilot: "wake" }, "button-ghost"));
  if (status.autonomy.state !== "enabled" || status.autonomy.mode !== "free_thinking") {
    actions.push(buttonHtml("Free thinking", { autonomy: "free-thinking" }, "button-ghost"));
  }
  if (evolve.state === "disabled" || evolve.state === "completed" || evolve.state === "failed") {
    actions.push(buttonHtml("Start evolve", { evolve: "start" }, "button-ghost"));
  } else if (evolve.state === "running") {
    actions.push(buttonHtml("Pause evolve", { evolve: "pause" }, "button-ghost"));
    actions.push(buttonHtml("Stop evolve", { evolve: "stop" }, "button-muted"));
  } else if (evolve.state === "paused") {
    actions.push(buttonHtml("Resume evolve", { evolve: "resume" }, "button-ghost"));
    actions.push(buttonHtml("Stop evolve", { evolve: "stop" }, "button-muted"));
  }
  elements.autopilotActions.innerHTML = actions.join("");

  const rows = [
    ["State", autopilot.state],
    ["Autonomy mode", status.autonomy.mode],
    ["Evolve state", evolve.state],
    ["Evolve mission", fmt(evolve.current_mission_id)],
    ["Evolve iteration", evolve.iteration],
    ["Pending restart", fmtBoolean(evolve.pending_restart)],
    ["Max concurrent missions", autopilot.max_concurrent_missions],
    ["Wake interval", `${autopilot.wake_interval_seconds}s`],
    ["Background shell", fmtBoolean(autopilot.allow_background_shell)],
    ["Background network", fmtBoolean(autopilot.allow_background_network)],
    ["Background self-edit", fmtBoolean(autopilot.allow_background_self_edit)],
  ];
  elements.autopilotDetails.innerHTML = rows
    .map(([label, value]) => `<dt>${escapeHtml(label)}</dt><dd>${escapeHtml(fmt(value))}</dd>`)
    .join("");
}

function renderMissions(missions) {
  elements.missionSummary.textContent = `${missions.length} mission(s) loaded`;
  elements.missionsBody.innerHTML = missions.length
    ? missions
        .map((mission) => {
          const canPause = !["blocked", "completed", "failed", "cancelled"].includes(mission.status);
          const canResume = ["blocked", "waiting", "scheduled"].includes(mission.status);
          const actions = [
            canPause ? buttonHtml("Pause", { missionPause: mission.id }) : "",
            canResume ? buttonHtml("Run now", { missionResume: mission.id }, "button-muted") : "",
            buttonHtml("Delay", { missionDelay: mission.id }, "button-ghost"),
            !["completed", "cancelled"].includes(mission.status)
              ? buttonHtml("Cancel", { missionCancel: mission.id }, "button-small--ghost")
              : "",
          ]
            .filter(Boolean)
            .join("");
          const wakeDisplay = mission.wake_at ? fmtDate(mission.wake_at) : fmt(mission.watch_path);
          const repeatDisplay = mission.repeat_interval_seconds
            ? `${mission.repeat_interval_seconds}s`
            : "-";
          return `
            <tr>
              <td>
                <strong>${escapeHtml(mission.title)}</strong>
                <div class="table-sub mono">${escapeHtml(mission.id)}</div>
              </td>
              <td>${badge(mission.status, mission.status === "failed" ? "danger" : mission.status === "blocked" ? "warn" : mission.status === "completed" ? "good" : "info")}</td>
              <td>${escapeHtml(wakeDisplay)}</td>
              <td>${escapeHtml(repeatDisplay)}</td>
              <td>${escapeHtml(fmtDate(mission.updated_at))}</td>
              <td>
                <div>${escapeHtml(fmt(mission.alias || mission.workspace_key))}</div>
                <div class="table-sub">${escapeHtml(fmt(mission.requested_model))}</div>
              </td>
              <td><div class="inline-actions">${actions}</div></td>
            </tr>
          `;
        })
        .join("")
    : `<tr><td colspan="7" class="empty-table">No missions yet.</td></tr>`;
}

function renderApprovals(approvals) {
  elements.approvalsSummary.textContent = `${approvals.length} pending approval(s)`;
  elements.approvalsList.innerHTML = approvals.length
    ? approvals
        .map(
          (approval) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h3>${escapeHtml(approval.title)}</h3>
                  <p class="card-subtitle">${escapeHtml(approval.connector_name)} · ${escapeHtml(approval.connector_kind)}</p>
                </div>
                ${badge(approval.status, "warn")}
              </div>
              <p class="card-copy">${escapeHtml(approval.details)}</p>
              <div class="badge-row">
                ${badge(`chat ${fmt(approval.external_chat_display || approval.external_chat_id)}`)}
                ${badge(`user ${fmt(approval.external_user_display || approval.external_user_id)}`)}
                ${badge(`created ${fmtDate(approval.created_at)}`)}
              </div>
              <div class="inline-actions">
                ${buttonHtml("Approve", { approvalApprove: approval.id })}
                ${buttonHtml("Reject", { approvalReject: approval.id }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No connector approvals pending.");
}

function renderMemory(memories) {
  elements.memorySummary.textContent = `${memories.length} memory candidate(s)`;
  elements.memoryList.innerHTML = memories.length
    ? memories
        .map(
          (memory) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h3>${escapeHtml(memory.subject)}</h3>
                  <p class="card-subtitle">${escapeHtml(memory.kind)} · ${escapeHtml(memory.scope)}</p>
                </div>
                ${badge(`confidence ${fmt(memory.confidence)}`, memory.confidence >= 80 ? "good" : "warn")}
              </div>
              <p class="card-copy">${escapeHtml(memory.content)}</p>
              <ul class="micro-list">
                <li>workspace: ${escapeHtml(fmt(memory.workspace_key))}</li>
                <li>provider: ${escapeHtml(fmt(memory.provider_id))}</li>
                <li>updated: ${escapeHtml(fmtDate(memory.updated_at))}</li>
              </ul>
              <div class="inline-actions">
                ${buttonHtml("Approve", { memoryApprove: memory.id })}
                ${buttonHtml("Reject", { memoryReject: memory.id }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No memory candidates pending review.");
}

function renderSkills(skills) {
  elements.skillsSummary.textContent = `${skills.length} draft(s) loaded`;
  elements.skillsList.innerHTML = skills.length
    ? skills
        .map(
          (skill) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h3>${escapeHtml(skill.title)}</h3>
                  <p class="card-subtitle">${escapeHtml(skill.summary)}</p>
                </div>
                ${badge(skill.status, skill.status === "published" ? "good" : skill.status === "rejected" ? "danger" : "warn")}
              </div>
              <p class="card-copy">${escapeHtml(skill.instructions)}</p>
              <ul class="micro-list">
                <li>trigger: ${escapeHtml(fmt(skill.trigger_hint))}</li>
                <li>workspace: ${escapeHtml(fmt(skill.workspace_key))}</li>
                <li>usage: ${escapeHtml(fmt(skill.usage_count))}</li>
              </ul>
              ${
                skill.status === "draft"
                  ? `<div class="inline-actions">
                      ${buttonHtml("Publish", { skillPublish: skill.id })}
                      ${buttonHtml("Reject", { skillReject: skill.id }, "button-small--ghost")}
                    </div>`
                  : ""
              }
            </article>
          `
        )
        .join("")
    : renderEmpty("No skill drafts available.");
}

function renderProfile(profileMemories) {
  elements.profileSummary.textContent = `${profileMemories.length} accepted profile fact(s)`;
  elements.profileList.innerHTML = profileMemories.length
    ? profileMemories
        .map(
          (memory) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h3>${escapeHtml(memory.subject)}</h3>
                  <p class="card-subtitle">${escapeHtml(memory.kind)} · ${escapeHtml(memory.scope)}</p>
                </div>
                ${badge(memory.review_status, memory.review_status === "accepted" ? "good" : "info")}
              </div>
              <p class="card-copy">${escapeHtml(memory.content)}</p>
              <ul class="micro-list">
                <li>provider: ${escapeHtml(fmt(memory.provider_id))}</li>
                <li>workspace: ${escapeHtml(fmt(memory.workspace_key))}</li>
                <li>updated: ${escapeHtml(fmtDate(memory.updated_at))}</li>
              </ul>
            </article>
          `
        )
        .join("")
    : renderEmpty("No profile memories yet.");
}

function renderConnectors(
  status,
  telegrams,
  discords,
  slacks,
  signals,
  homeAssistants,
  webhooks,
  inboxes,
  gmails,
  braves
) {
  if (window.dashboardConnectors) {
    window.dashboardConnectors.render({
      status,
      telegrams: telegrams || [],
      discords: discords || [],
      slacks: slacks || [],
      signals: signals || [],
      homeAssistants: homeAssistants || [],
      webhooks: webhooks || [],
      inboxes: inboxes || [],
      gmails: gmails || [],
      braves: braves || [],
    });
    return;
  }
  elements.connectorSummary.textContent = "Connector module unavailable";
  elements.connectorCards.innerHTML = "";
}

function renderDelegation(targets) {
  elements.delegationSummary.textContent = `${targets.length} target(s) available`;
  elements.delegationList.innerHTML = targets.length
    ? targets
        .map(
          (target) => `
            <article class="target-card">
              <h3>${escapeHtml(target.alias)}</h3>
              <p>${escapeHtml(target.provider_display_name)}</p>
              <p class="meta">${escapeHtml(target.provider_id)} · ${escapeHtml(target.model)}</p>
              <div class="badge-row">
                ${target.primary ? badge("primary", "good") : ""}
                ${target.target_names.slice(0, 4).map((name) => badge(name, "info")).join("")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No delegation targets available.");
}

function renderEvents(events) {
  elements.eventsSummary.textContent = `${events.length} recent event(s)`;
  elements.eventsList.innerHTML = events.length
    ? events
        .map(
          (event) => `
            <article class="event-item">
              <div class="event-item__meta">
                <span class="timestamp">${escapeHtml(fmtDate(event.created_at))}</span>
                ${badge(event.level, event.level === "error" ? "danger" : event.level === "warn" ? "warn" : "good")}
                <span class="event-item__scope">${escapeHtml(event.scope)}</span>
              </div>
              <div class="event-item__body">
                <p class="event-item__message">${escapeHtml(event.message)}</p>
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No daemon events yet.");
}

function renderPermissions(permissions) {
  const preset = typeof permissions === "string" ? permissions : permissions?.preset || permissions?.mode || "unknown";
  elements.permissionsSummary.textContent = `Preset: ${preset}`;
  const presets = [
    ["Suggest", "suggest"],
    ["AutoEdit", "auto_edit"],
    ["FullAuto", "full_auto"],
  ];
  elements.permissionPresetActions.innerHTML = presets
    .map(([label, value]) =>
      buttonHtml(label, { permissionPreset: value }, preset.toLowerCase() === value ? "" : "button-ghost")
    )
    .join("");
  const details = typeof permissions === "string"
    ? []
    : Object.entries(permissions || {})
        .filter(([key]) => key !== "preset" && key !== "mode")
        .slice(0, 10);
  elements.permissionsDetails.innerHTML = details.length
    ? details
        .map(([key, value]) => `<dt>${escapeHtml(key)}</dt><dd>${escapeHtml(fmt(typeof value === "boolean" ? fmtBoolean(value) : value))}</dd>`)
        .join("")
    : "";
}

function renderTrust(trust) {
  const flags = ["allow_shell", "allow_network", "allow_full_disk", "allow_self_edit"];
  elements.trustToggles.innerHTML = flags
    .map(
      (flag) => `
        <label class="toggle">
          <input type="checkbox" data-trust-flag="${escapeHtml(flag)}" ${trust[flag] ? "checked" : ""}>
          <span>${escapeHtml(flag.replaceAll("_", " "))}</span>
        </label>
      `
    )
    .join("");
  elements.trustPaths.innerHTML = Array.isArray(trust.trusted_paths) && trust.trusted_paths.length
    ? trust.trusted_paths
        .map(
          (path) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div><h4>${escapeHtml(path)}</h4></div>
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No trusted paths configured.");
}

function renderLogs(logs) {
  elements.logsSummary.textContent = `${logs.length} log entry/entries`;
  elements.logsList.innerHTML = logs.length
    ? logs
        .map(
          (log) => `
            <article class="event-item">
              <div class="event-item__meta">
                <span class="timestamp">${escapeHtml(fmtDate(log.timestamp || log.created_at))}</span>
                ${badge(log.level || "info", (log.level || "info") === "error" ? "danger" : (log.level || "info") === "warn" ? "warn" : "good")}
              </div>
              <div class="event-item__body">
                <p class="event-item__message">${escapeHtml(log.message || log.text || JSON.stringify(log))}</p>
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No log entries.");
}

function renderSessions(sessions) {
  elements.sessionsSummary.textContent = `${sessions.length} session(s)`;
  elements.sessionsBody.innerHTML = sessions.length
    ? sessions
        .map(
          (session) => `
            <tr>
              <td>
                <strong>${escapeHtml(session.title || session.id)}</strong>
                <div class="table-sub mono">${escapeHtml(session.id)}</div>
              </td>
              <td>${escapeHtml(fmt(session.alias))}</td>
              <td>${badge(session.task_mode || "default", session.task_mode === "build" ? "good" : session.task_mode === "daily" ? "info" : "neutral")}</td>
              <td>${escapeHtml(fmt(session.model || session.requested_model))}</td>
              <td>${escapeHtml(fmt(session.message_count))}</td>
              <td>${escapeHtml(fmtDate(session.created_at))}</td>
              <td>${escapeHtml(fmtDate(session.updated_at))}</td>
              <td><div class="inline-actions">${buttonHtml("Chat", { sessionUse: session.id }, "button-muted")}${buttonHtml("View", { sessionView: session.id }, "button-ghost")}${buttonHtml("Fork", { sessionFork: session.id }, "button-ghost")}${buttonHtml("Rename", { sessionRename: session.id }, "button-small--ghost")}</div></td>
            </tr>
          `
        )
        .join("")
    : `<tr><td colspan="8" class="empty-table">No sessions yet.</td></tr>`;
}

function renderMcpServers(servers) {
  elements.mcpSummary.textContent = `${servers.length} MCP server(s)`;
  elements.mcpList.innerHTML = servers.length
    ? servers
        .map(
          (server) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h4>${escapeHtml(server.name || server.id)}</h4>
                  <p class="card-subtitle">${escapeHtml(fmt(server.command))}</p>
                </div>
                ${badge(server.enabled !== false ? "enabled" : "disabled", server.enabled !== false ? "good" : "warn")}
              </div>
              <ul class="micro-list">
                <li>id: ${escapeHtml(fmt(server.id))}</li>
                <li>args: ${escapeHtml(fmtList(server.args))}</li>
              </ul>
              <div class="inline-actions">
                ${buttonHtml("Delete", { mcpDelete: server.id }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No MCP servers configured.");
}

function renderDaemonConfig(status) {
  const persistence = status.persistence_mode || "on_demand";
  const autoStart = status.auto_start || false;
  elements.daemonConfigSummary.textContent = `${persistence} | auto-start: ${fmtBoolean(autoStart)}`;
  elements.daemonPersistenceActions.innerHTML = ["on_demand", "always_on"]
    .map((mode) =>
      buttonHtml(mode.replaceAll("_", " "), { daemonPersistence: mode }, persistence === mode ? "" : "button-ghost")
    )
    .join("");
  elements.daemonAutostartActions.innerHTML = [
    buttonHtml("Enable auto-start", { daemonAutostart: "true" }, autoStart ? "" : "button-ghost"),
    buttonHtml("Disable auto-start", { daemonAutostart: "false" }, autoStart ? "button-ghost" : ""),
  ].join("");
}

function updateDelegationFormInputs(delegationConfig, fallbackDelegation) {
  if (document.activeElement !== elements.delegationMaxDepth) {
    elements.delegationMaxDepth.value = displayLimit(
      delegationConfig.max_depth || fallbackDelegation?.max_depth
    );
  }
  if (document.activeElement !== elements.delegationMaxParallel) {
    elements.delegationMaxParallel.value = displayLimit(
      delegationConfig.max_parallel_subagents || fallbackDelegation?.max_parallel_subagents
    );
  }
  if (document.activeElement !== elements.delegationDisabledProviders) {
    elements.delegationDisabledProviders.value = (delegationConfig.disabled_provider_ids || []).join(", ");
  }
}

function normalizeBootstrapData(payload) {
  return {
    status: payload.status,
    providers: payload.providers || [],
    aliases: payload.aliases || [],
    delegationTargets: payload.delegation_targets || [],
    telegrams: payload.telegram_connectors || [],
    discords: payload.discord_connectors || [],
    slacks: payload.slack_connectors || [],
    signals: payload.signal_connectors || [],
    homeAssistants: payload.home_assistant_connectors || [],
    webhooks: payload.webhook_connectors || [],
    inboxes: payload.inbox_connectors || [],
    gmails: payload.gmail_connectors || [],
    braves: payload.brave_connectors || [],
    plugins: payload.plugins || [],
    sessions: payload.sessions || [],
    events: payload.events || [],
    permissions: payload.permissions || "unknown",
    trust: payload.trust || { trusted_paths: [] },
    delegationConfig: payload.delegation_config || {},
  };
}

function renderBootstrapData(data) {
  renderWhenChanged("status", data.status, () => renderStatus(data.status));
  renderWhenChanged(
    "health",
    { status: data.status?.persistence_mode, health: state.lastData?.health || null },
    () => renderHealth(data.status, state.lastData?.health || null)
  );
  renderWhenChanged(
    "autopilot",
    {
      autonomy: data.status?.autonomy,
      autopilot: data.status?.autopilot,
      evolve: data.status?.evolve,
    },
    () => renderAutopilot(data.status)
  );
  renderWhenChanged(
    "connectors",
    {
      status: data.status,
      telegrams: data.telegrams,
      discords: data.discords,
      slacks: data.slacks,
      signals: data.signals,
      homeAssistants: data.homeAssistants,
      webhooks: data.webhooks,
      inboxes: data.inboxes,
      gmails: data.gmails,
      braves: data.braves,
    },
    () =>
      renderConnectors(
        data.status,
        data.telegrams,
        data.discords,
        data.slacks,
        data.signals,
        data.homeAssistants,
        data.webhooks,
        data.inboxes,
        data.gmails,
        data.braves
      )
  );
  renderWhenChanged("delegation", data.delegationTargets, () => renderDelegation(data.delegationTargets));
  renderWhenChanged("events", data.events, () => renderEvents(data.events));
  renderWhenChanged(
    "providers",
    {
      providers: data.providers,
      aliases: data.aliases,
      mainAlias: data.status?.main_agent_alias,
      mainTarget: data.status?.main_target,
    },
    () => {
      if (window.dashboardProviders) {
        window.dashboardProviders.render(data);
      }
    }
  );
  renderWhenChanged("permissions", data.permissions, () => renderPermissions(data.permissions));
  renderWhenChanged("trust", data.trust, () => renderTrust(data.trust));
  renderWhenChanged("sessions", data.sessions, () => renderSessions(data.sessions));
  if (window.dashboardPlugins) {
    window.dashboardPlugins.render(data.plugins, state.lastData?.health?.plugins || []);
  }
  if (window.dashboardWorkspace) {
    window.dashboardWorkspace.handleBootstrap(data).catch((error) => {
      setStatus(`Workspace scan failed: ${error.message}`, "warn");
    });
  }
  if (window.dashboardSettings) {
    window.dashboardSettings.render(data);
  }
  renderWhenChanged(
    "daemon-config",
    { persistence: data.status?.persistence_mode, autoStart: data.status?.auto_start },
    () => renderDaemonConfig(data.status)
  );
  updateDelegationFormInputs(data.delegationConfig, data.status?.delegation);
}

async function loadMissionsPanel() {
  const missions = await apiGet("/v1/missions?limit=25");
  mergeLastData({ missions });
  renderWhenChanged("missions", missions, () => renderMissions(missions));
  return missions;
}

async function loadApprovalsPanel() {
  const approvals = await apiGet("/v1/connector-approvals?status=pending&limit=25");
  mergeLastData({ approvals });
  renderWhenChanged("approvals", approvals, () => renderApprovals(approvals));
  return approvals;
}

async function loadMemoryReviewPanel() {
  const memoryReview = await apiGet("/v1/memory/review?limit=25");
  mergeLastData({ memoryReview });
  renderWhenChanged("memory", memoryReview, () => {
    elements.memorySummary.textContent = `${memoryReview.length} memory candidate(s)`;
    elements.memoryList.innerHTML = memoryReview.length
      ? memoryReview
          .map(
            (memory) =>
              renderMemoryCard(memory, {
                actions:
                  `${buttonHtml("Approve", { memoryApprove: memory.id })}` +
                  `${buttonHtml("Reject", { memoryReject: memory.id }, "button-small--ghost")}`,
              })
          )
          .join("")
      : renderEmpty("No memory candidates pending review.");
  });
  return memoryReview;
}

async function loadProfilePanel() {
  const profileMemories = await apiGet("/v1/memory/profile");
  mergeLastData({ profileMemories });
  renderWhenChanged("profile", profileMemories, () => {
    elements.profileSummary.textContent = `${profileMemories.length} accepted profile fact(s)`;
    elements.profileList.innerHTML = profileMemories.length
      ? profileMemories.map((memory) => renderMemoryCard(memory, { showReview: false })).join("")
      : renderEmpty("No profile memories yet.");
  });
  return profileMemories;
}

async function loadSkillsPanel() {
  const skillDrafts = await apiGet("/v1/skills/drafts");
  mergeLastData({ skillDrafts });
  renderWhenChanged("skills", skillDrafts, () => renderSkills(skillDrafts));
  return skillDrafts;
}

async function loadLogsPanel() {
  const logs = await apiGet("/v1/logs?limit=100").catch(() => []);
  mergeLastData({ logs });
  renderWhenChanged("logs", logs, () => renderLogs(logs));
  return logs;
}

async function loadSessionsPanel() {
  const sessions = await refreshSessionsSummary();
  renderActiveSessionDetail();
  return sessions;
}

async function loadMcpPanel() {
  const mcpServers = await apiGet("/v1/mcp").catch(() => []);
  mergeLastData({ mcpServers });
  renderWhenChanged("mcp", mcpServers, () => renderMcpServers(mcpServers));
  return mcpServers;
}

const lazyPanelLoaders = {
  missions: loadMissionsPanel,
  approvals: loadApprovalsPanel,
  memory: loadMemoryReviewPanel,
  skills: loadSkillsPanel,
  profile: loadProfilePanel,
  sessions: loadSessionsPanel,
  logs: loadLogsPanel,
  mcp: loadMcpPanel,
};

async function ensurePanelLoaded(panelId, { force = false } = {}) {
  if (panelId === "sessions" && !hasDashboardAuth()) {
    renderActiveSessionDetail();
    return null;
  }

  const loader = lazyPanelLoaders[panelId];
  if (!loader || !hasDashboardAuth()) {
    return null;
  }
  if (!force && state.loadedPanels.has(panelId)) {
    return null;
  }
  if (state.panelInFlight[panelId]) {
    return state.panelInFlight[panelId];
  }

  const request = loader()
    .then((result) => {
      state.loadedPanels.add(panelId);
      return result;
    })
    .finally(() => {
      delete state.panelInFlight[panelId];
    });
  state.panelInFlight[panelId] = request;
  return request;
}

async function refreshLoadedPanels(forcePanels = []) {
  const panels = [...new Set([...Array.from(state.loadedPanels), ...forcePanels])]
    .filter((panelId) => panelId === "sessions" || lazyPanelLoaders[panelId]);
  if (!panels.length) {
    return;
  }
  await Promise.all(panels.map((panelId) => ensurePanelLoaded(panelId, { force: true })));
}

async function refreshHealth({ silent = false } = {}) {
  if (!hasDashboardAuth()) {
    return null;
  }
  if (state.healthInFlight) {
    return state.healthInFlight;
  }
  state.healthInFlight = (async () => {
    const health = await apiGet("/v1/doctor");
    mergeLastData({ health });
    renderWhenChanged(
      "health",
      { status: state.lastData?.status?.persistence_mode, health },
      () => renderHealth(state.lastData?.status, health)
    );
    if (window.dashboardPlugins) {
      window.dashboardPlugins.render(state.lastData?.plugins || [], health.plugins || []);
    }
    return health;
  })();
  try {
    return await state.healthInFlight;
  } catch (error) {
    if (!silent) {
      setStatus(`Health check failed: ${error.message}`, "warn");
    }
    throw error;
  } finally {
    state.healthInFlight = null;
  }
}

function loadVisiblePanels() {
  ["missions", "approvals", "memory", "skills", "profile", "sessions", "logs", "mcp"].forEach((panelId) => {
    if (isPanelNearViewport(panelId)) {
      ensurePanelLoaded(panelId).catch((error) => {
        setStatus(`Panel load failed: ${error.message}`, "warn");
      });
    }
  });
}

function setupLazyPanels() {
  if (state.lazyObserver) {
    state.lazyObserver.disconnect();
  }
  if (!("IntersectionObserver" in window)) {
    return;
  }
  state.lazyObserver = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) {
          return;
        }
        ensurePanelLoaded(entry.target.id).catch((error) => {
          setStatus(`Panel load failed: ${error.message}`, "warn");
        });
      });
    },
    { rootMargin: "240px 0px" }
  );

  ["missions", "approvals", "memory", "skills", "profile", "sessions", "logs", "mcp"].forEach((panelId) => {
    const panel = panelElement(panelId);
    if (panel) {
      state.lazyObserver.observe(panel);
    }
  });
}

function initializeLazyPanelPlaceholders() {
  elements.missionSummary.textContent = "Loads when visible";
  elements.missionsBody.innerHTML =
    `<tr><td colspan="7" class="empty-table">Scroll this panel into view to load recent missions.</td></tr>`;
  elements.approvalsSummary.textContent = "Loads when visible";
  elements.approvalsList.innerHTML = renderEmpty("Open this panel to load pending approvals.");
  elements.memorySummary.textContent = "Loads when visible";
  elements.memoryList.innerHTML = renderEmpty("Open this panel to load memory review candidates.");
  elements.skillsSummary.textContent = "Loads when visible";
  elements.skillsList.innerHTML = renderEmpty("Open this panel to load skill drafts.");
  elements.profileSummary.textContent = "Loads when visible";
  elements.profileList.innerHTML = renderEmpty("Open this panel to load profile memories.");
  elements.logsSummary.textContent = "Loads when visible";
  elements.logsList.innerHTML = renderEmpty("Open this panel to load daemon logs.");
  elements.mcpSummary.textContent = "Loads when visible";
  elements.mcpList.innerHTML = renderEmpty("Open this panel to load MCP servers.");
  elements.sessionDetail.innerHTML = renderEmpty("No session selected.");
}

function resetDashboardSurface() {
  elements.heroChips.innerHTML = "";
  elements.statusCards.innerHTML = "";
  elements.statusDetails.innerHTML = "";
  renderHealth(null, null);
  elements.controlSummary.textContent = "No control data yet";
  elements.autopilotActions.innerHTML = "";
  elements.autopilotDetails.innerHTML = "";
  initializeLazyPanelPlaceholders();
  elements.connectorSummary.textContent = "No connector data yet";
  elements.connectorCards.innerHTML = "";
  if (window.dashboardConnectors) {
    window.dashboardConnectors.reset();
  }
  elements.delegationSummary.textContent = "No delegation data yet";
  elements.delegationList.innerHTML = renderEmpty("No delegation targets available.");
  elements.eventsSummary.textContent = "No event data yet";
  elements.eventsList.innerHTML = renderEmpty("No daemon events yet.");
  state.activeSessionResumePacket = null;
  state.sessionDetailPacket = null;
  state.sessionDetailSessionId = null;
  if (window.dashboardProviders) {
    window.dashboardProviders.reset();
  }
  elements.permissionsSummary.textContent = "No permissions data yet";
  elements.permissionPresetActions.innerHTML = "";
  elements.permissionsDetails.innerHTML = "";
  renderTrust({ trusted_paths: [] });
  renderSessions([]);
  elements.daemonConfigSummary.textContent = "No daemon config yet";
  elements.daemonPersistenceActions.innerHTML = "";
  elements.daemonAutostartActions.innerHTML = "";
  if (window.dashboardPlugins) {
    window.dashboardPlugins.reset();
  }
  if (window.dashboardWorkspace) {
    window.dashboardWorkspace.reset();
  }
  if (window.dashboardSettings) {
    window.dashboardSettings.reset();
  }
  renderChatAttachments();
  renderConsoleEntries();
  syncChatCwdInput();
}

async function refreshSessionsSummary() {
  const sessions = await apiGet("/v1/sessions?limit=25").catch(() => []);
  mergeLastData({ sessions });
  renderWhenChanged("sessions", sessions, () => renderSessions(sessions));
  return sessions;
}

async function refreshDashboard(options = {}) {
  const refreshOptions = {
    includeLoadedPanels: true,
    includeHealth: true,
    silent: false,
    allowUnauthenticatedAttempt: false,
    forcePanels: [],
    ...options,
  };
  if (!refreshOptions.allowUnauthenticatedAttempt && !hasDashboardAuth()) {
    setStatus("Waiting for a daemon token.", "neutral");
    return;
  }
  if (state.refreshInFlight) {
    await state.refreshInFlight;
    if (refreshOptions.includeLoadedPanels || refreshOptions.includeHealth || refreshOptions.forcePanels.length) {
      return refreshDashboard({ ...refreshOptions, silent: true });
    }
    return;
  }
  if (!refreshOptions.silent) {
    setStatus("Refreshing dashboard...", "neutral");
  }
  state.refreshInFlight = (async () => {
    let bootstrapPayload;
    try {
      bootstrapPayload = expectControlResponse(
        await controlRequest({ kind: "dashboard_bootstrap" }),
        "dashboard_bootstrap"
      );
    } catch (_) {
      bootstrapPayload = await apiGet("/v1/dashboard/bootstrap");
      if (state.dashboardSessionAuthenticated) {
        ensureControlSocket().catch(() => {});
      }
    }
    const bootstrap = normalizeBootstrapData(bootstrapPayload);
    state.dashboardSessionAuthenticated = true;
    mergeLastData(bootstrap);
    renderBootstrapData(bootstrap);
    if (refreshOptions.includeLoadedPanels || refreshOptions.forcePanels.length) {
      await refreshLoadedPanels(refreshOptions.forcePanels);
    }
    if (refreshOptions.includeHealth) {
      await refreshHealth({ silent: true }).catch(() => null);
    }
    elements.lastUpdated.textContent = `Updated ${new Date().toLocaleTimeString()}`;
    setStatus("Connected.", "ok");
    loadVisiblePanels();
  })();
  try {
    await state.refreshInFlight;
  } catch (error) {
    if (isUnauthorizedError(error)) {
      clearDashboardConnectionState();
      setStatus("Waiting for a daemon token.", "neutral");
      return;
    }
    setStatus(`Refresh failed: ${error.message}`, "warn");
    throw error;
  } finally {
    state.refreshInFlight = null;
  }
}

function scheduleRefresh() {
  if (state.refreshTimer) {
    clearInterval(state.refreshTimer);
  }
  if (state.healthTimer) {
    clearInterval(state.healthTimer);
  }
  if (state.autoRefresh && hasDashboardAuth()) {
    if (!state.controlSocketReady) {
      state.refreshTimer = setInterval(() => {
        refreshDashboard({
          includeLoadedPanels: false,
          includeHealth: false,
          silent: true,
        }).catch(() => {});
      }, 12000);
    }
    state.healthTimer = setInterval(() => {
      refreshHealth({ silent: true }).catch(() => {});
    }, 60000);
  }
}

function renderActiveSessionDetail() {
  if (!state.loadedPanels.has("sessions")) {
    return;
  }
  const packet = state.sessionDetailPacket || state.activeSessionResumePacket;
  elements.sessionDetail.innerHTML = packet
    ? renderSessionResumePacket(packet)
    : state.activeChatSessionId
      ? renderSessionMessages(state.activeTranscript || [])
      : renderEmpty("No session selected.");
}

function snapshotChatState() {
  return {
    activeChatSessionId: state.activeChatSessionId,
    pendingChatSessionId: state.pendingChatSessionId,
    activeTranscript: [...(state.activeTranscript || [])],
    activeSessionResumePacket: state.activeSessionResumePacket,
    sessionDetailPacket: state.sessionDetailPacket,
    sessionDetailSessionId: state.sessionDetailSessionId,
    chatSessionMeta: elements.chatSessionMeta.textContent,
    chatAttachments: [...state.chatAttachments],
    chatCwd: state.chatCwd,
  };
}

function restoreChatState(snapshot) {
  state.activeChatSessionId = snapshot.activeChatSessionId;
  state.pendingChatSessionId = snapshot.pendingChatSessionId;
  state.activeTranscript = [...(snapshot.activeTranscript || [])];
  state.activeSessionResumePacket = snapshot.activeSessionResumePacket || null;
  state.sessionDetailPacket = snapshot.sessionDetailPacket || null;
  state.sessionDetailSessionId = snapshot.sessionDetailSessionId || null;
  elements.chatSessionMeta.textContent = snapshot.chatSessionMeta;
  state.chatAttachments = [...(snapshot.chatAttachments || [])];
  setChatCwd(snapshot.chatCwd || "");
  if (state.lastData && window.dashboardProviders) {
    window.dashboardProviders.render(state.lastData);
  }
  renderChatAttachments();
  renderChatTranscript(state.activeTranscript);
  renderActiveSessionDetail();
}

async function loadChatSession(sessionId, { focusChat = false } = {}) {
  const transcript = await apiGet(`/v1/sessions/${encodeURIComponent(sessionId)}`);
  const packet = await apiGet(`/v1/sessions/${encodeURIComponent(sessionId)}/resume-packet`).catch(() => null);
  state.activeChatSessionId = transcript.session.id;
  state.pendingChatSessionId = null;
  state.activeTranscript = transcript.messages || [];
  state.activeSessionResumePacket = packet;
  state.sessionDetailSessionId = transcript.session.id;
  state.sessionDetailPacket = packet;
  state.chatAttachments = [];
  renderChatAttachments();
  setChatCwd(transcript.session.cwd || "");
  elements.runTaskAlias.value = transcript.session.alias || "";
  elements.runTaskModel.value = "";
  elements.runTaskMode.value = transcript.session.task_mode || "";
  elements.chatSessionMeta.textContent = [
    transcript.session.title || transcript.session.id,
    transcript.session.alias,
    transcript.session.model,
    transcript.session.task_mode || "default",
  ]
    .filter(Boolean)
    .join(" | ");
  renderChatTranscript(transcript.messages || []);
  renderActiveSessionDetail();
  if (focusChat) {
    focusSection("run-task");
  }
}

function startNewChat() {
  state.activeChatSessionId = null;
  state.pendingChatSessionId = null;
  state.activeTranscript = [];
  state.activeSessionResumePacket = null;
  state.sessionDetailPacket = null;
  state.sessionDetailSessionId = null;
  elements.runTaskPrompt.value = "";
  elements.chatSessionMeta.textContent = "New chat";
  renderChatAttachments();
  syncChatCwdInput();
  renderChatTranscript([]);
  renderActiveSessionDetail();
}

function setChatRunState(inFlight) {
  state.chatRunInFlight = inFlight;
  elements.chatNewSession.disabled = inFlight;
  elements.chatRenameButton.disabled = inFlight;
  elements.chatForkButton.disabled = inFlight;
  elements.chatCompactButton.disabled = inFlight;
  elements.chatAttachmentAdd.disabled = inFlight;
  elements.chatAttachmentsClear.disabled = inFlight;
}

function ensureChatRunIdle(actionLabel) {
  if (state.chatRunInFlight) {
    throw new Error(`Wait for the active chat run to finish before ${actionLabel}.`);
  }
}

function messageTone(message) {
  if (message.role === "user") {
    return {
      label: "You",
      className: "chat-message chat-message--user",
      subtitle: fmt(message.provider_id || "prompt"),
    };
  }
  if (message.role === "assistant" && Array.isArray(message.tool_calls) && message.tool_calls.length) {
    return {
      label: "Thinking",
      className: "chat-message chat-message--thinking",
      subtitle: fmt(message.model || "planning"),
    };
  }
  if (message.role === "assistant") {
    return {
      label: "Assistant",
      className: "chat-message chat-message--assistant",
      subtitle: fmt(message.model || message.provider_id || ""),
    };
  }
  if (message.role === "tool") {
    const failed = String(message.content || "").startsWith("ERROR:");
    return {
      label: failed ? "Tool Error" : "Tool",
      className: `chat-message ${failed ? "chat-message--error" : "chat-message--tool"}`,
      subtitle: fmt(message.tool_name || "tool execution"),
    };
  }
  return {
    label: "System",
    className: "chat-message chat-message--system",
    subtitle: fmt(message.provider_id || "system"),
  };
}

function chatCodeLineClass(line) {
  const trimmed = line.trimStart();
  if (trimmed.startsWith("@@") || trimmed.startsWith("diff --git")) {
    return "chat-code__line chat-code__line--meta";
  }
  if (trimmed.startsWith("+") && !trimmed.startsWith("+++")) {
    return "chat-code__line chat-code__line--add";
  }
  if (trimmed.startsWith("-") && !trimmed.startsWith("---")) {
    return "chat-code__line chat-code__line--remove";
  }
  if (/^(fn|pub|async|class|function)\b/.test(trimmed)) {
    return "chat-code__line chat-code__line--keyword";
  }
  return "chat-code__line";
}

function renderCodeBlock(content, language = "") {
  const lines = String(content || "").split(/\r?\n/);
  return `
    <div class="chat-code-block">
      <div class="chat-code-label">${escapeHtml(language ? `Code ${language}` : "Code")}</div>
      <pre class="chat-code"><code>${lines
        .map(
          (line) =>
            `<span class="${chatCodeLineClass(line)}">${escapeHtml(line || " ")}</span>`
        )
        .join("")}</code></pre>
    </div>
  `;
}

function renderRichContent(content) {
  const lines = String(content || "").split(/\r?\n/);
  const blocks = [];
  let paragraph = [];
  let codeLines = [];
  let codeLanguage = "";
  let inCode = false;

  const flushParagraph = () => {
    if (!paragraph.length) {
      return;
    }
    blocks.push(
      `<p class="card-copy">${escapeHtml(paragraph.join("\n"))}</p>`
    );
    paragraph = [];
  };

  const flushCode = () => {
    if (!codeLines.length) {
      blocks.push(renderCodeBlock("", codeLanguage));
      return;
    }
    blocks.push(renderCodeBlock(codeLines.join("\n"), codeLanguage));
    codeLines = [];
    codeLanguage = "";
  };

  lines.forEach((line) => {
    const trimmed = line.trimStart();
    if (trimmed.startsWith("```")) {
      if (inCode) {
        flushCode();
        inCode = false;
      } else {
        flushParagraph();
        inCode = true;
        codeLanguage = trimmed.slice(3).trim();
      }
      return;
    }
    if (inCode) {
      codeLines.push(line);
    } else {
      paragraph.push(line);
    }
  });

  flushParagraph();
  if (inCode) {
    flushCode();
  }

  return blocks.join("");
}

function parseToolArgumentField(argumentsText, field) {
  try {
    const parsed = JSON.parse(argumentsText || "{}");
    const value = parsed[field];
    if (typeof value === "string") {
      return value;
    }
    if (value === null || typeof value === "undefined") {
      return null;
    }
    return JSON.stringify(value, null, 2);
  } catch {
    return null;
  }
}

function renderToolCallPreview(toolCall) {
  const name = toolCall.name || "tool";
  let summary = name.replaceAll("_", " ");
  let preview = "";
  if (name === "run_shell") {
    const command = parseToolArgumentField(toolCall.arguments, "command");
    if (command) {
      summary = `run shell: ${command}`;
      preview = renderCodeBlock(command, "shell");
    }
  } else if (name === "apply_patch") {
    const patch = parseToolArgumentField(toolCall.arguments, "patch");
    if (patch) {
      summary = "apply patch";
      preview = renderCodeBlock(patch, "diff");
    }
  } else if (name === "write_file" || name === "append_file") {
    const path = parseToolArgumentField(toolCall.arguments, "path");
    const content = parseToolArgumentField(toolCall.arguments, "content");
    summary = path ? `${name.replaceAll("_", " ")} ${path}` : name.replaceAll("_", " ");
    if (content) {
      preview = renderCodeBlock(content);
    }
  } else if (name === "replace_in_file") {
    const path = parseToolArgumentField(toolCall.arguments, "path");
    const oldValue = parseToolArgumentField(toolCall.arguments, "old");
    const newValue = parseToolArgumentField(toolCall.arguments, "new");
    summary = path ? `replace text in ${path}` : "replace text in file";
    preview = renderCodeBlock(`--- old\n${oldValue || ""}\n+++ new\n${newValue || ""}`, "diff");
  }
  return `
    <div class="chat-tool-preview">
      <div class="chat-tool-preview__header">
        ${badge("action")}
        <span>${escapeHtml(summary)}</span>
      </div>
      ${preview}
    </div>
  `;
}

function renderConversationMessage(message) {
  const tone = messageTone(message);
  const toolCalls = Array.isArray(message.tool_calls) ? message.tool_calls : [];
  const attachments = Array.isArray(message.attachments) ? message.attachments : [];
  return `
    <article class="${tone.className}">
      <div class="chat-message__header">
        <div class="chat-message__title">
          ${badge(tone.label)}
          <span>${escapeHtml(tone.subtitle)}</span>
        </div>
        ${badge(fmtDate(message.created_at || message.timestamp))}
      </div>
      ${message.content ? renderRichContent(message.content) : ""}
      ${attachments
        .map(
          (attachment) => `
            <div class="chat-attachment">
              ${badge("image")}
              <span>${escapeHtml(attachment.path || "")}</span>
            </div>
          `
        )
        .join("")}
      ${toolCalls.length
        ? `
          <div class="chat-tool-preview-list">
            ${toolCalls.map(renderToolCallPreview).join("")}
          </div>
        `
        : ""}
    </article>
  `;
}

function renderConversationThread(messages, emptyText) {
  return messages.length
    ? messages.map(renderConversationMessage).join("")
    : renderEmpty(emptyText);
}

function renderSessionMessages(messages) {
  return renderConversationThread(messages, "No messages in this session.");
}

function renderChatTranscript(messages) {
  elements.chatTranscript.innerHTML = renderConversationThread(messages, "No active chat yet.");
  elements.chatTranscript.scrollTop = elements.chatTranscript.scrollHeight;
}

function syncChatCwdInput() {
  if (elements.chatCwd && document.activeElement !== elements.chatCwd) {
    elements.chatCwd.value = state.chatCwd || "";
  }
}

function setChatCwd(nextCwd) {
  state.chatCwd = String(nextCwd || "").trim();
  syncChatCwdInput();
}

function currentWorkspaceReport() {
  return window.dashboardWorkspace?.getReport?.() || null;
}

function renderChatAttachments() {
  if (!elements.chatAttachments) {
    return;
  }
  elements.chatAttachments.innerHTML = state.chatAttachments.length
    ? state.chatAttachments
        .map(
          (attachment, index) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h4>${escapeHtml(attachment.path)}</h4>
                  <p class="card-subtitle">${escapeHtml(attachment.kind || "image")}</p>
                </div>
                ${buttonHtml("Remove", { chatAttachmentRemove: String(index) }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No attachments queued.");
}

function renderConsoleEntries() {
  const entries = state.consoleEntries.slice(0, 8);
  elements.runTaskResult.innerHTML = entries.length
    ? entries
        .map(
          (entry) => `
            <article class="console-entry">
              <div class="console-entry__header">
                <div class="chat-message__title">
                  ${badge(entry.label || "console", entry.tone || "info")}
                  <span>${escapeHtml(entry.title || "")}</span>
                </div>
                <span class="panel__meta">${escapeHtml(fmtDate(entry.timestamp))}</span>
              </div>
              ${
                entry.body
                  ? entry.preformatted
                    ? `<pre>${escapeHtml(entry.body)}</pre>`
                    : renderRichContent(entry.body)
                  : ""
              }
            </article>
          `
        )
        .join("")
    : renderEmpty("Slash commands, shell output, and chat task summaries appear here.");
}

function pushConsoleEntry(title, body, options = {}) {
  state.consoleEntries = [
    {
      title,
      body: String(body || "").trim(),
      tone: options.tone || "info",
      label: options.label || "console",
      preformatted: options.preformatted !== false,
      timestamp: new Date().toISOString(),
    },
    ...state.consoleEntries,
  ].slice(0, 16);
  renderConsoleEntries();
}

function latestAssistantMessage(messages = state.activeTranscript || []) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message?.role === "assistant") {
      return message;
    }
  }
  return null;
}

function persistToken(token) {
  state.token = token.trim();
  if (window.dashboardProviders && state.lastData) {
    window.dashboardProviders.render(state.lastData);
  }
}

function bootstrapToken() {
  const params = new URLSearchParams(window.location.search);
  const tokenFromUrl = params.get("token");
  state.token = (tokenFromUrl || "").trim();
  if (tokenFromUrl) {
    const url = new URL(window.location.href);
    url.searchParams.delete("token");
    const cleanedSearch = url.searchParams.toString();
    const cleanedUrl = `${url.pathname}${cleanedSearch ? `?${cleanedSearch}` : ""}${url.hash}`;
    window.history.replaceState({}, document.title, cleanedUrl);
  }
  elements.tokenInput.value = state.token;
}

function clearDashboardConnectionState({ clearTokenInput = false } = {}) {
  closeControlSocket({ rejectMessage: "Dashboard connection was cleared." });
  state.dashboardSessionAuthenticated = false;
  state.token = "";
  state.pendingChatSessionId = null;
  state.chatAttachments = [];
  state.consoleEntries = [];
  state.chatCwd = "";
  if (clearTokenInput) {
    elements.tokenInput.value = "";
  }
  scheduleRefresh();
  state.lastData = null;
  state.loadedPanels.clear();
  state.renderCache = {};
  state.panelInFlight = {};
  elements.lastUpdated.textContent = "Not connected";
  resetDashboardSurface();
  renderChatTranscript([]);
  elements.chatSessionMeta.textContent = "New chat";
  renderChatAttachments();
  renderConsoleEntries();
  syncChatCwdInput();
}

async function initializeDashboardConnection() {
  if (state.token) {
    await createDashboardSession(state.token);
  }
  try {
    await refreshDashboard({ allowUnauthenticatedAttempt: true });
  } catch (error) {
    if (!isUnauthorizedError(error)) {
      throw error;
    }
  }
}

function buildMissionPayload({ scheduled }) {
  const title = elements.missionTitle.value.trim();
  const details = elements.missionDetails.value.trim();
  if (!title || !details) {
    throw new Error("Mission title and details are required.");
  }
  const now = new Date();
  const delaySeconds = Number.parseInt(elements.missionDelay.value || "0", 10) || 0;
  const repeatSeconds = Number.parseInt(elements.missionRepeat.value || "0", 10) || 0;
  const wakeAt = scheduled && delaySeconds > 0 ? new Date(now.getTime() + delaySeconds * 1000) : null;
  return {
    id: crypto.randomUUID(),
    title,
    details,
    status: wakeAt ? "scheduled" : "queued",
    created_at: now.toISOString(),
    updated_at: now.toISOString(),
    alias: elements.missionAlias.value.trim() || null,
    requested_model: elements.missionModel.value.trim() || null,
    session_id: null,
    workspace_key: null,
    watch_path: null,
    watch_recursive: false,
    watch_fingerprint: null,
    wake_trigger: wakeAt ? "timer" : "manual",
    wake_at: wakeAt ? wakeAt.toISOString() : null,
    repeat_interval_seconds: repeatSeconds > 0 ? repeatSeconds : null,
    last_error: null,
    retries: 0,
    max_retries: 3,
  };
}

async function createMission(scheduled) {
  const payload = buildMissionPayload({ scheduled });
  await apiPost("/v1/missions", payload);
  elements.missionForm.reset();
  elements.missionDelay.value = "0";
  elements.missionRepeat.value = "0";
  await refreshDashboard();
}

async function setAutopilot(mode) {
  const current = state.lastData?.status?.autopilot?.state || "enabled";
  await apiPut("/v1/autopilot/status", { state: mode === "wake" ? current : mode });
  await refreshDashboard();
}

async function setAutonomy(mode) {
  await apiPost("/v1/autonomy/enable", { mode, allow_self_edit: true });
  await refreshDashboard();
}

async function setEvolve(mode) {
  if (mode === "start") {
    await apiPost("/v1/evolve/start", { budget_friendly: false });
  } else {
    await apiPost(`/v1/evolve/${mode}`, {});
  }
  await refreshDashboard();
}

async function handleApprovalAction(id, approved) {
  const note = window.prompt(`Optional note for ${approved ? "approving" : "rejecting"} this pairing:`, "") || "";
  await apiPost(
    `/v1/connector-approvals/${id}/${approved ? "approve" : "reject"}`,
    note.trim() ? { note } : {}
  );
  await refreshDashboard();
}

async function handleMemoryAction(id, approved) {
  const note = window.prompt(`Optional note for ${approved ? "approving" : "rejecting"} this memory:`, "") || "";
  await apiPost(`/v1/memory/${id}/${approved ? "approve" : "reject"}`, note.trim() ? { note } : {});
  await refreshDashboard();
}

async function handleSkillAction(id, publish) {
  await apiPost(`/v1/skills/drafts/${id}/${publish ? "publish" : "reject"}`, {});
  await refreshDashboard();
}

async function handleMissionAction(action, id) {
  if (action === "pause") {
    const note = window.prompt("Pause note (optional):", "") || "";
    await apiPost(`/v1/missions/${id}/pause`, note.trim() ? { note } : {});
  } else if (action === "resume") {
    await apiPost(`/v1/missions/${id}/resume`, {});
  } else if (action === "delay") {
    const secondsText = window.prompt("Wake again in how many seconds?", "300");
    if (!secondsText) {
      return;
    }
    const seconds = Number.parseInt(secondsText, 10);
    if (!Number.isFinite(seconds) || seconds < 0) {
      throw new Error("Delay must be a non-negative number of seconds.");
    }
    const note = window.prompt("Resume note (optional):", "") || "";
    const payload = {
      wake_at: new Date(Date.now() + seconds * 1000).toISOString(),
    };
    if (note.trim()) {
      payload.note = note;
    }
    await apiPost(`/v1/missions/${id}/resume`, payload);
  } else if (action === "cancel") {
    if (!window.confirm("Cancel this mission?")) {
      return;
    }
    await apiPost(`/v1/missions/${id}/cancel`, {});
  }
  await refreshDashboard();
}

function currentChatCwd() {
  const typed = elements.chatCwd?.value?.trim() || "";
  if (typed !== state.chatCwd) {
    state.chatCwd = typed;
  }
  return state.chatCwd;
}

function activeSessionSummary() {
  return (state.lastData?.sessions || []).find((session) => session.id === state.activeChatSessionId) || null;
}

function buildRunTaskPayload(prompt, overrides = {}) {
  const activeSession = activeSessionSummary();
  const payload = {
    prompt,
    session_id: state.activeChatSessionId,
    cwd: currentChatCwd() || null,
    attachments: [...state.chatAttachments],
  };
  const alias = String(overrides.alias ?? elements.runTaskAlias.value).trim();
  const model = String(overrides.model ?? elements.runTaskModel.value).trim();
  const thinking = String(overrides.thinking ?? elements.runTaskThinking.value).trim();
  const taskMode = String(
    overrides.taskMode ?? elements.runTaskMode.value ?? activeSession?.task_mode ?? ""
  ).trim();
  const permissionPreset = String(overrides.permissionPreset ?? elements.runTaskPermission.value).trim();
  if (alias) payload.alias = alias;
  if (model) payload.requested_model = model;
  if (thinking) payload.thinking_level = thinking;
  if (taskMode) payload.task_mode = taskMode;
  if (permissionPreset) payload.permission_preset = permissionPreset;
  return payload;
}

function preferAliasForProvider(providerId) {
  const aliases = state.lastData?.aliases || [];
  const currentAlias = elements.runTaskAlias.value.trim();
  const current = aliases.find((alias) => alias.alias === currentAlias && alias.provider_id === providerId);
  if (current) {
    return current.alias;
  }
  const mainAlias = state.lastData?.status?.main_agent_alias;
  const main = aliases.find((alias) => alias.alias === mainAlias && alias.provider_id === providerId);
  if (main) {
    return main.alias;
  }
  const direct = aliases.find((alias) => alias.alias === providerId && alias.provider_id === providerId);
  if (direct) {
    return direct.alias;
  }
  return aliases
    .filter((alias) => alias.provider_id === providerId)
    .sort((left, right) => left.alias.localeCompare(right.alias))[0]?.alias || null;
}

function resolveProviderAliasSelection(value) {
  const aliases = state.lastData?.aliases || [];
  const providers = state.lastData?.providers || [];
  const exactAlias = aliases.find((alias) => alias.alias.toLowerCase() === value.toLowerCase());
  if (exactAlias) {
    return exactAlias.alias;
  }
  const normalized = value.replace(/[^a-z0-9]/gi, "").toLowerCase();
  const matches = providers
    .filter((provider) => {
      const display = String(provider.display_name || provider.id || "");
      return (
        String(provider.id || "").toLowerCase() === value.toLowerCase() ||
        display.toLowerCase() === value.toLowerCase() ||
        String(provider.id || "").replace(/[^a-z0-9]/gi, "").toLowerCase() === normalized ||
        display.replace(/[^a-z0-9]/gi, "").toLowerCase() === normalized
      );
    })
    .map((provider) => preferAliasForProvider(provider.id))
    .filter(Boolean);
  const unique = [...new Set(matches)];
  if (unique.length === 1) {
    return unique[0];
  }
  if (!unique.length) {
    throw new Error(`unknown logged-in provider '${value}'`);
  }
  throw new Error(`provider selection '${value}' is ambiguous`);
}

function resolveSessionTarget(target) {
  const sessions = state.lastData?.sessions || [];
  if (!target) {
    return null;
  }
  if (target === "last") {
    return sessions[0]?.id || null;
  }
  const exact = sessions.find((session) => session.id === target || session.title === target);
  if (exact) {
    return exact.id;
  }
  const prefix = sessions.find((session) => session.id.startsWith(target));
  return prefix?.id || null;
}

function formatRecordLines(records, mapper) {
  return records.length ? records.map(mapper).join("\n") : "None.";
}

function formatConnectorSummary(connectors) {
  return formatRecordLines(connectors, (connector) => {
    const details = [
      `${connector.id} [${connector.name || connector.id}]`,
      `enabled=${fmtBoolean(!!connector.enabled)}`,
      connector.alias ? `alias=${connector.alias}` : null,
      connector.requested_model ? `model=${connector.requested_model}` : null,
      connector.cwd ? `cwd=${connector.cwd}` : null,
    ].filter(Boolean);
    return details.join(" ");
  });
}

function formatStatusSummary() {
  const status = state.lastData?.status;
  if (!status) {
    return "No daemon status loaded yet.";
  }
  const activeSession = activeSessionSummary();
  return [
    `session=${state.activeChatSessionId || "(new)"}`,
    activeSession?.title ? `title=${activeSession.title}` : null,
    `alias=${elements.runTaskAlias.value || status.main_agent_alias || "-"}`,
    `model=${elements.runTaskModel.value || activeSession?.model || "-"}`,
    `thinking=${elements.runTaskThinking.value || "default"}`,
    `mode=${elements.runTaskMode.value || activeSession?.task_mode || "default"}`,
    `permission_preset=${elements.runTaskPermission.value || state.lastData?.permissions || "default"}`,
    `attachments=${state.chatAttachments.length}`,
    `cwd=${currentChatCwd() || "(daemon cwd)"}`,
    status.main_target
      ? `main=${status.main_target.alias} (${status.main_target.provider_id}/${status.main_target.model})`
      : "main=(not configured)",
    `autonomy=${status.autonomy?.state || "-"}`,
    `autopilot=${status.autopilot?.state || "-"}`,
    `active_missions=${status.active_missions || 0}`,
    `memories=${status.memories || 0}`,
  ]
    .filter(Boolean)
    .join("\n");
}

function buildReviewPrompt(diffText, customPrompt) {
  const instructions =
    customPrompt && customPrompt.trim()
      ? customPrompt.trim()
      : "Review these code changes. Focus on bugs, regressions, security issues, and missing tests. Put findings first, ordered by severity, and be concise.";
  return `${instructions}\n\nReview target:\n\`\`\`\n${diffText}\n\`\`\``;
}

async function copyLatestAssistantOutput() {
  const latest = latestAssistantMessage();
  if (!latest?.content?.trim()) {
    throw new Error("No assistant output available to copy.");
  }
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard access is not available in this browser.");
  }
  await navigator.clipboard.writeText(latest.content);
  pushConsoleEntry("Clipboard", "Copied the latest assistant reply.", {
    tone: "good",
    label: "clipboard",
    preformatted: false,
  });
}

function parseMaybeInteger(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`${label} must be a non-negative integer.`);
  }
  return parsed;
}

function addChatAttachment(pathValue) {
  const path = String(pathValue || "").trim();
  if (!path) {
    throw new Error("Attachment path is required.");
  }
  state.chatAttachments = [...state.chatAttachments, { kind: "image", path }];
  renderChatAttachments();
  pushConsoleEntry("Attachments", `Queued image attachment:\n${path}`, {
    tone: "good",
    label: "attachments",
  });
}

function clearChatAttachments() {
  state.chatAttachments = [];
  renderChatAttachments();
}

async function forkSessionById(sessionId) {
  const response = await apiPost(`/v1/sessions/${encodeURIComponent(sessionId)}/fork`, {});
  await refreshSessionsSummary();
  await loadChatSession(response.session.id, { focusChat: true });
  pushConsoleEntry("Fork", `Forked session into ${response.session.id}.`, {
    tone: "good",
    label: "session",
    preformatted: false,
  });
}

async function compactActiveChat() {
  if (!state.activeChatSessionId) {
    throw new Error("No active chat to compact.");
  }
  const response = await apiPost(`/v1/sessions/${encodeURIComponent(state.activeChatSessionId)}/compact`, {
    alias: elements.runTaskAlias.value.trim() || null,
    requested_model: elements.runTaskModel.value.trim() || null,
    cwd: currentChatCwd() || null,
    thinking_level: elements.runTaskThinking.value || null,
    task_mode: elements.runTaskMode.value || null,
    permission_preset: elements.runTaskPermission.value || null,
  });
  await refreshSessionsSummary();
  await loadChatSession(response.session.id, { focusChat: true });
  pushConsoleEntry("Compact", `Compacted into session ${response.session.id}.`, {
    tone: "good",
    label: "session",
    preformatted: false,
  });
}

async function executeShellCommand(command) {
  const response = await apiPost("/v1/workspace/shell", {
    command,
    cwd: currentChatCwd() || null,
  });
  setChatCwd(response.cwd || "");
  pushConsoleEntry(`Shell ${command}`, response.output || "(no output)", {
    tone: "info",
    label: "shell",
  });
}

async function executeChatPrompt(prompt, overrides = {}) {
  let chatSnapshot = null;
  let completedResponse = null;
  const payload = buildRunTaskPayload(prompt, overrides);
  try {
    if (state.chatRunInFlight) {
      throw new Error("A chat run is already in progress.");
    }
    chatSnapshot = snapshotChatState();
    setChatRunState(true);
    const optimisticMessage = {
      role: "user",
      content: prompt,
      created_at: new Date().toISOString(),
      provider_id: payload.alias || "user",
      model: payload.requested_model || "",
      tool_calls: [],
      attachments: payload.attachments || [],
    };
    state.activeTranscript = [...(state.activeTranscript || []), optimisticMessage];
    renderChatTranscript(state.activeTranscript);
    renderActiveSessionDetail();
    pushConsoleEntry("Chat", "Working...", {
      tone: "info",
      label: "chat",
      preformatted: false,
    });
    let streamError = null;
    state.pendingChatSessionId = null;
    const handleStreamEvent = (streamEvent) => {
      if (streamEvent.type === "session_started") {
        state.pendingChatSessionId = streamEvent.session_id || state.pendingChatSessionId;
        elements.chatSessionMeta.textContent = [
          streamEvent.alias,
          streamEvent.model,
          streamEvent.session_id,
        ]
          .filter(Boolean)
          .join(" | ");
      } else if (streamEvent.type === "message" && streamEvent.message) {
        state.activeTranscript = [...(state.activeTranscript || []), streamEvent.message];
        renderChatTranscript(state.activeTranscript);
        renderActiveSessionDetail();
      } else if (streamEvent.type === "completed") {
        completedResponse = streamEvent.response || null;
      } else if (streamEvent.type === "error") {
        streamError = streamEvent.message || "Task failed.";
      }
    };
    try {
      completedResponse = expectControlResponse(
        await controlRequest(
          {
            kind: "run_task",
            payload: {
              request: payload,
            },
          },
          { onEvent: handleStreamEvent }
        ),
        "run_task"
      );
    } catch (_) {
      await apiStream("/v1/run/stream", payload, handleStreamEvent);
    }
    if (streamError) {
      throw new Error(streamError);
    }
    elements.runTaskPrompt.value = "";
    pushConsoleEntry("Chat", "Response received.", {
      tone: "good",
      label: "chat",
      preformatted: false,
    });
    if (completedResponse && completedResponse.session_id) {
      await loadChatSession(completedResponse.session_id);
    } else if (state.pendingChatSessionId) {
      await loadChatSession(state.pendingChatSessionId);
    } else if (!state.activeChatSessionId) {
      const newestSessionId = state.lastData?.sessions?.[0]?.id || null;
      if (newestSessionId) {
        await loadChatSession(newestSessionId);
      }
    }
    await refreshSessionsSummary();
  } catch (error) {
    if (!completedResponse && chatSnapshot) {
      restoreChatState(chatSnapshot);
    } else {
      state.pendingChatSessionId = null;
    }
    pushConsoleEntry("Chat failed", error.message, {
      tone: "danger",
      label: "chat",
    });
    throw error;
  } finally {
    setChatRunState(false);
  }
}

function bindActions() {
  document.body.addEventListener("click", async (event) => {
    const target = findActionTarget(event.target);
    if (!(target instanceof HTMLElement)) {
      return;
    }
    try {
      if (target.dataset.approvalApprove) {
        await handleApprovalAction(target.dataset.approvalApprove, true);
      } else if (target.dataset.approvalReject) {
        await handleApprovalAction(target.dataset.approvalReject, false);
      } else if (target.dataset.memoryApprove) {
        await handleMemoryAction(target.dataset.memoryApprove, true);
      } else if (target.dataset.memoryReject) {
        await handleMemoryAction(target.dataset.memoryReject, false);
      } else if (target.dataset.skillPublish) {
        await handleSkillAction(target.dataset.skillPublish, true);
      } else if (target.dataset.skillReject) {
        await handleSkillAction(target.dataset.skillReject, false);
      } else if (target.dataset.missionPause) {
        await handleMissionAction("pause", target.dataset.missionPause);
      } else if (target.dataset.missionResume) {
        await handleMissionAction("resume", target.dataset.missionResume);
      } else if (target.dataset.missionDelay) {
        await handleMissionAction("delay", target.dataset.missionDelay);
      } else if (target.dataset.missionCancel) {
        await handleMissionAction("cancel", target.dataset.missionCancel);
      } else if (target.dataset.autopilot) {
        await setAutopilot(target.dataset.autopilot);
      } else if (target.dataset.autonomy) {
        await setAutonomy(target.dataset.autonomy);
      } else if (target.dataset.evolve) {
        await setEvolve(target.dataset.evolve);
      } else if (target.dataset.permissionPreset) {
        await apiPut("/v1/permissions", { permission_preset: target.dataset.permissionPreset });
        await refreshDashboard();
      } else if (target.dataset.trustFlag) {
        const flag = target.dataset.trustFlag;
        const checked = target instanceof HTMLInputElement ? target.checked : false;
        const trust = { ...(state.lastData?.trust || {}) };
        trust[flag] = checked;
        await apiPut("/v1/trust", trust);
        await refreshDashboard();
      } else if (target.dataset.memoryDelete) {
        if (window.confirm("Delete this memory?")) {
          await apiDelete(`/v1/memory/${target.dataset.memoryDelete}`);
          await refreshDashboard();
        }
      } else if (target.dataset.sessionView) {
        try {
          state.loadedPanels.add("sessions");
          const packet = await apiGet(
            `/v1/sessions/${encodeURIComponent(target.dataset.sessionView)}/resume-packet`
          );
          state.sessionDetailSessionId = packet.session?.id || target.dataset.sessionView;
          state.sessionDetailPacket = packet;
          elements.sessionDetail.innerHTML = renderSessionResumePacket(packet);
        } catch (err) {
          elements.sessionDetail.innerHTML = renderEmpty(`Failed to load session: ${err.message}`);
        }
      } else if (target.dataset.sessionUse) {
        ensureChatRunIdle("switching chats");
        state.loadedPanels.add("sessions");
        await loadChatSession(target.dataset.sessionUse, { focusChat: true });
      } else if (target.dataset.sessionFork) {
        ensureChatRunIdle("forking chats");
        await forkSessionById(target.dataset.sessionFork);
      } else if (target.dataset.sessionRename) {
        ensureChatRunIdle("renaming chats");
        const title = window.prompt("New session title:", "") || "";
        if (!title.trim()) {
          return;
        }
        await apiPut(`/v1/sessions/${encodeURIComponent(target.dataset.sessionRename)}/title`, { title });
        await refreshSessionsSummary();
        if (state.activeChatSessionId === target.dataset.sessionRename) {
          await loadChatSession(target.dataset.sessionRename);
        }
      } else if (target.dataset.chatAttachmentRemove) {
        const index = Number.parseInt(target.dataset.chatAttachmentRemove, 10);
        if (Number.isInteger(index) && index >= 0) {
          state.chatAttachments = state.chatAttachments.filter((_, currentIndex) => currentIndex !== index);
          renderChatAttachments();
        }
      } else if (target.dataset.mcpDelete) {
        if (window.confirm(`Delete MCP server ${target.dataset.mcpDelete}?`)) {
          await apiDelete(`/v1/mcp/${target.dataset.mcpDelete}`);
          await refreshDashboard();
        }
      } else if (target.dataset.daemonPersistence) {
        await apiPut("/v1/daemon/config", { persistence_mode: target.dataset.daemonPersistence });
        await refreshDashboard();
      } else if (target.dataset.daemonAutostart) {
        await apiPut("/v1/daemon/config", { auto_start: target.dataset.daemonAutostart === "true" });
        await refreshDashboard();
      }
    } catch (error) {
      setStatus(`Action failed: ${error.message}`, "warn");
    }
  });
}

elements.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  persistToken(elements.tokenInput.value);
  if (state.token) {
    await createDashboardSession(state.token);
  }
  scheduleRefresh();
  await refreshDashboard({ allowUnauthenticatedAttempt: true });
});

elements.refreshButton.addEventListener("click", async () => {
  persistToken(elements.tokenInput.value);
  if (state.token) {
    await createDashboardSession(state.token);
  }
  scheduleRefresh();
  await refreshDashboard({ allowUnauthenticatedAttempt: true });
});

elements.clearButton.addEventListener("click", async () => {
  try {
    await clearDashboardSession();
  } catch (_) {
  }
  clearDashboardConnectionState({ clearTokenInput: true });
  state.activeChatSessionId = null;
  state.activeTranscript = [];
  setStatus("Waiting for a daemon token.", "neutral");
});

elements.autoRefreshInput.addEventListener("change", () => {
  state.autoRefresh = elements.autoRefreshInput.checked;
  scheduleRefresh();
});

elements.missionForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    await createMission(false);
  } catch (error) {
    setStatus(`Mission create failed: ${error.message}`, "warn");
  }
});

elements.missionSchedule.addEventListener("click", async () => {
  try {
    await createMission(true);
  } catch (error) {
    setStatus(`Mission schedule failed: ${error.message}`, "warn");
  }
});

elements.trustPathForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const path = elements.trustPathInput.value.trim();
    if (!path) {
      throw new Error("Trusted path is required.");
    }
    await apiPut("/v1/trust", { trusted_path: path });
    elements.trustPathForm.reset();
    await refreshDashboard();
  } catch (error) {
    setStatus(`Trusted path update failed: ${error.message}`, "warn");
  }
});

elements.delegationForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    await apiPut("/v1/delegation/config", {
      max_depth: parseLimitInput(elements.delegationMaxDepth.value, state.lastData?.delegationConfig?.max_depth || state.lastData?.status?.delegation?.max_depth),
      max_parallel_subagents: parseLimitInput(
        elements.delegationMaxParallel.value,
        state.lastData?.delegationConfig?.max_parallel_subagents || state.lastData?.status?.delegation?.max_parallel_subagents
      ),
      disabled_provider_ids: parseDelimitedList(elements.delegationDisabledProviders.value),
    });
    await refreshDashboard();
  } catch (error) {
    setStatus(`Delegation update failed: ${error.message}`, "warn");
  }
});

elements.memorySearchForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const query = elements.memorySearchQuery.value.trim();
    if (!query) {
      throw new Error("Search query is required.");
    }
    const results = await apiPost("/v1/memory/search", { query });
    const items = Array.isArray(results) ? results : results.memories || results.results || [];
    const transcriptHits = Array.isArray(results.transcript_hits) ? results.transcript_hits : [];
    const memoryCards = items.length
      ? items
          .map((memory) =>
            renderMemoryCard(memory, {
              actions: buttonHtml("Delete", { memoryDelete: memory.id }, "button-small--ghost"),
            })
          )
          .join("")
      : "";
    const transcriptCard = transcriptHits.length
      ? `
        <article class="stack-card stack-card--resume">
          <div class="card-title-row">
            <div>
              <h4>Transcript hits</h4>
              <p class="card-subtitle">${escapeHtml(transcriptHits.length)} related message(s)</p>
            </div>
            ${badge("transcripts", "info")}
          </div>
          <div class="resume-section">
            ${transcriptHits
              .map(
                (hit) => `
                  <article class="stack-card stack-card--compact">
                    <div class="card-title-row">
                      <div>
                        <h4>${escapeHtml(hit.session_id)}</h4>
                        <p class="card-subtitle">${escapeHtml(hit.role || "-")} · ${escapeHtml(fmtDate(hit.created_at))}</p>
                      </div>
                      ${badge("hit", "info")}
                    </div>
                    <p class="card-copy">${escapeHtml(hit.preview)}</p>
                  </article>
                `
              )
              .join("")}
          </div>
        </article>
      `
      : "";
    elements.memorySearchResults.innerHTML =
      memoryCards || transcriptCard
        ? `${memoryCards}${transcriptCard}`
        : renderEmpty("No matching memories found.");
  } catch (error) {
    setStatus(`Memory search failed: ${error.message}`, "warn");
  }
});

elements.memoryCreateForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const subject = elements.memoryCreateSubject.value.trim();
    const content = elements.memoryCreateContent.value.trim();
    const sourceSessionId =
      state.activeChatSessionId ||
      state.sessionDetailSessionId ||
      elements.memoryRebuildSessionId?.value.trim() ||
      null;
    if (!subject || !content) {
      throw new Error("Subject and content are required.");
    }
    await apiPost("/v1/memory", {
      kind: elements.memoryCreateKind.value,
      scope: elements.memoryCreateScope.value,
      subject,
      content,
      confidence: 100,
      source_session_id: sourceSessionId,
      workspace_key: currentChatCwd() || null,
      tags: ["manual"],
      review_status: "accepted",
    });
    elements.memoryCreateForm.reset();
    await refreshDashboard();
  } catch (error) {
    setStatus(`Memory create failed: ${error.message}`, "warn");
  }
});

if (elements.memoryRebuildForm) {
  elements.memoryRebuildForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      const sessionId = elements.memoryRebuildSessionId?.value.trim() || "";
      const recomputeEmbeddings = Boolean(elements.memoryRebuildEmbeddings?.checked);
      const response = await apiPost("/v1/memory/rebuild", {
        session_id: sessionId || undefined,
        recompute_embeddings: recomputeEmbeddings,
      });
      elements.memoryRebuildForm.reset();
      setStatus(
        `Rebuilt memory from ${response.sessions_scanned} session(s) and ${response.observations_scanned} observation(s).`,
        "ok"
      );
      await refreshDashboard({ includeLoadedPanels: true, forcePanels: ["memory", "profile", "sessions"] });
      if (state.sessionDetailSessionId) {
        const packet = await apiGet(
          `/v1/sessions/${encodeURIComponent(state.sessionDetailSessionId)}/resume-packet`
        ).catch(() => null);
        if (packet) {
          state.sessionDetailPacket = packet;
          renderActiveSessionDetail();
        }
      }
    } catch (error) {
      setStatus(`Memory rebuild failed: ${error.message}`, "warn");
    }
  });
}

if (window.dashboardConnectors) {
  window.dashboardConnectors.bind();
}

elements.runTaskForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const input = elements.runTaskPrompt.value.trim();
    if (!input) {
      throw new Error("Prompt is required.");
    }
    if (input.startsWith("!")) {
      await executeShellCommand(input.slice(1).trim());
    } else if (!(await runSlashCommand(input))) {
      await executeChatPrompt(input);
    }
  } catch (error) {
    setStatus(`Chat action failed: ${error.message}`, "warn");
  }
});

elements.chatNewSession.addEventListener("click", () => {
  try {
    ensureChatRunIdle("starting a new chat");
    startNewChat();
  } catch (error) {
    setStatus(`Chat action failed: ${error.message}`, "warn");
  }
});

elements.chatRenameButton.addEventListener("click", async () => {
  try {
    ensureChatRunIdle("renaming chats");
    if (!state.activeChatSessionId) {
      throw new Error("No active chat to rename.");
    }
    const title = window.prompt("New chat title:", "") || "";
    if (!title.trim()) {
      return;
    }
    await apiPut(`/v1/sessions/${encodeURIComponent(state.activeChatSessionId)}/title`, { title });
    await refreshSessionsSummary();
    await loadChatSession(state.activeChatSessionId);
  } catch (error) {
    setStatus(`Chat rename failed: ${error.message}`, "warn");
  }
});

elements.chatForkButton.addEventListener("click", async () => {
  try {
    ensureChatRunIdle("forking chats");
    if (!state.activeChatSessionId) {
      throw new Error("No active chat to fork.");
    }
    await forkSessionById(state.activeChatSessionId);
  } catch (error) {
    setStatus(`Chat fork failed: ${error.message}`, "warn");
  }
});

elements.chatCompactButton.addEventListener("click", async () => {
  try {
    ensureChatRunIdle("compacting chats");
    await compactActiveChat();
  } catch (error) {
    setStatus(`Chat compact failed: ${error.message}`, "warn");
  }
});

elements.chatAttachmentAdd.addEventListener("click", () => {
  try {
    addChatAttachment(elements.chatAttachmentPath.value);
    elements.chatAttachmentPath.value = "";
  } catch (error) {
    setStatus(`Attachment failed: ${error.message}`, "warn");
  }
});

elements.chatAttachmentsClear.addEventListener("click", () => {
  clearChatAttachments();
  pushConsoleEntry("Attachments", "attachments cleared", {
    tone: "good",
    label: "attachments",
    preformatted: false,
  });
});

elements.chatAttachmentPath.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    elements.chatAttachmentAdd.click();
  }
});

elements.chatCwd.addEventListener("change", () => {
  setChatCwd(elements.chatCwd.value);
});

elements.chatUseDaemonCwd.addEventListener("click", () => {
  setChatCwd("");
  pushConsoleEntry("Workspace", "Chat cwd reset to the daemon working directory.", {
    tone: "info",
    label: "workspace",
    preformatted: false,
  });
});

elements.chatUseSessionCwd.addEventListener("click", () => {
  setChatCwd(activeSessionSummary()?.cwd || "");
});

elements.chatUseWorkspaceCwd.addEventListener("click", () => {
  const report = currentWorkspaceReport();
  const workspacePath =
    report?.workspace_root ||
    report?.requested_path ||
    document.getElementById("workspace-path")?.value ||
    "";
  setChatCwd(workspacePath);
});

elements.chatStatusShortcut.addEventListener("click", () => {
  runSlashCommand("/status").catch((error) => setStatus(`Status failed: ${error.message}`, "warn"));
});

elements.chatDiffShortcut.addEventListener("click", () => {
  runSlashCommand("/diff").catch((error) => setStatus(`Diff failed: ${error.message}`, "warn"));
});

elements.chatReviewShortcut.addEventListener("click", () => {
  runSlashCommand("/review").catch((error) => setStatus(`Review failed: ${error.message}`, "warn"));
});

elements.chatCopyShortcut.addEventListener("click", () => {
  copyLatestAssistantOutput().catch((error) => setStatus(`Copy failed: ${error.message}`, "warn"));
});

elements.mcpAddForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const id = elements.mcpAddId.value.trim();
    const name = elements.mcpAddName.value.trim();
    const command = elements.mcpAddCommand.value.trim();
    if (!id || !command) {
      throw new Error("Server ID and command are required.");
    }
    const argsRaw = elements.mcpAddArgs.value.trim();
    const args = argsRaw ? argsRaw.split(",").map((s) => s.trim()).filter(Boolean) : [];
    await apiPost("/v1/mcp", {
      id,
      name: name || id,
      command,
      args,
      enabled: elements.mcpAddEnabled.checked,
    });
    elements.mcpAddForm.reset();
    elements.mcpAddEnabled.checked = true;
    await refreshDashboard();
  } catch (error) {
    setStatus(`MCP add failed: ${error.message}`, "warn");
  }
});

bootstrapToken();
initializeLazyPanelPlaceholders();
const initialPanelId = window.location.hash.replace(/^#/, "");
state.activeDashboardTab = PANEL_TO_TAB[initialPanelId] || loadActiveDashboardTab();
updateDashboardTabButtons();
updateDashboardPanelVisibility();
renderDashboardTabLinks();
renderChatAttachments();
renderConsoleEntries();
syncChatCwdInput();
startNewChat();
if (window.dashboardProviders) {
  window.dashboardProviders.bind();
}
if (window.dashboardWorkspace) {
  window.dashboardWorkspace.bind();
}
if (window.dashboardSettings) {
  window.dashboardSettings.bind();
}
bindActions();
setupLazyPanels();
scheduleRefresh();
elements.dashboardTabButtons.forEach((button) => {
  button.addEventListener("click", () => {
    activateDashboardTab(button.dataset.dashboardTabTrigger, { scrollToTop: true });
  });
});
window.addEventListener("hashchange", () => {
  const panelId = window.location.hash.replace(/^#/, "");
  if (panelId) {
    focusSection(panelId);
    ensurePanelLoaded(panelId).catch((error) => {
      setStatus(`Panel load failed: ${error.message}`, "warn");
    });
  }
});
async function updateMainAlias(alias) {
  const selectedAlias = String(alias || "").trim();
  if (!selectedAlias) {
    throw new Error("Select an alias first.");
  }
  await apiPut("/v1/main-alias", { alias: selectedAlias });
  await refreshDashboard({ includeLoadedPanels: false });
  setStatus(`Default main alias set to ${selectedAlias}.`, "ok");
}

elements.runTaskAlias.addEventListener("change", () => {
  if (state.lastData && window.dashboardProviders) {
    window.dashboardProviders.render(state.lastData);
  }
});

elements.chatMakeMainButton.addEventListener("click", async () => {
  try {
    await updateMainAlias(elements.runTaskAlias.value);
  } catch (error) {
    setStatus(`Main alias update failed: ${error.message}`, "warn");
  }
});

initializeDashboardConnection().catch((error) => {
  setStatus(`Refresh failed: ${error.message}`, "warn");
});
