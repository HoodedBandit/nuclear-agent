
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

function requireDashboardChat(name) {
  const fn = globalThis[name];
  if (typeof fn !== "function") {
    throw new Error(`Dashboard chat function '${name}' is not available.`);
  }
  return fn;
}

function requireDashboardControl(name) {
  const fn = globalThis[name];
  if (typeof fn !== "function") {
    throw new Error(`Dashboard control function '${name}' is not available.`);
  }
  return fn;
}

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
  refreshDashboard: (...args) => requireDashboardChat("refreshDashboard")(...args),
  renderEmpty,
  setStatus,
  updateMainAlias: (...args) => requireDashboardChat("updateMainAlias")(...args),
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
  requireDashboardControl("closeControlSocket")({
    rejectMessage: "Dashboard session was replaced.",
  });
}

async function clearDashboardSession() {
  requireDashboardControl("closeControlSocket")({
    rejectMessage: "Dashboard session ended.",
  });
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
                ${badge(
                  event.scope === "remote_content" ? "remote content" : event.level,
                  event.scope === "remote_content"
                    ? "warn"
                    : event.level === "error"
                      ? "danger"
                      : event.level === "warn"
                        ? "warn"
                        : "good"
                )}
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
    () => window.dashboardControl.renderAutopilot(data.status)
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
  requireDashboardChat("renderChatAttachments")();
  requireDashboardChat("renderConsoleEntries")();
  requireDashboardChat("syncChatCwdInput")();
}


export { state, DASHBOARD_TAB_STORAGE_KEY, DASHBOARD_TABS, PANEL_TO_TAB, PANEL_LABELS, elements, bearerHeaders, hasDashboardAuth, isUnauthorizedError, apiRequest, apiGet, apiPost, apiPut, apiDelete, saveActiveDashboardTab, loadActiveDashboardTab, tabDefinition, panelElement, updateDashboardTabButtons, renderDashboardTabLinks, updateDashboardPanelVisibility, forcePanelsForTab, activateDashboardTab, activateTabForPanel, focusSection, quickStartProvider, apiStream, createDashboardSession, clearDashboardSession, escapeHtml, fmt, fmtList, fmtDate, fmtBoolean, parseDelimitedList, parseLimitInput, displayLimit, setStatus, badge, heroChip, ACTION_DATASET_KEYS, dataAttributeName, buttonHtml, hasActionDataset, findActionTarget, renderEmpty, formatMemoryEvidence, renderMemoryProvenance, renderMemoryCard, renderSessionResumePacket, stableKey, renderWhenChanged, mergeLastData, isPanelNearViewport, renderStatus, renderHealth, renderMissions, renderApprovals, renderMemory, renderSkills, renderProfile, renderConnectors, renderDelegation, renderEvents, renderPermissions, renderTrust, renderLogs, renderSessions, renderMcpServers, renderDaemonConfig, updateDelegationFormInputs, normalizeBootstrapData, renderBootstrapData, loadMissionsPanel, loadApprovalsPanel, loadMemoryReviewPanel, loadProfilePanel, loadSkillsPanel, loadLogsPanel, loadSessionsPanel, loadMcpPanel, lazyPanelLoaders, ensurePanelLoaded, refreshLoadedPanels, refreshHealth, loadVisiblePanels, setupLazyPanels, initializeLazyPanelPlaceholders, resetDashboardSurface };
