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
  editingProviderId: null,
  editingConnectorKey: null,
  providerAuthSessionId: null,
  providerAuthKind: null,
  providerAuthWindow: null,
  providerAuthPollTimer: null,
  providerAuthStatusMessage: "",
  providerAuthStatusTone: "neutral",
  providerAutoDefaults: null,
  lazyObserver: null,
  loadedPanels: new Set(),
  renderCache: {},
  refreshInFlight: null,
  healthInFlight: null,
  panelInFlight: {},
  chatRunInFlight: false,
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
  connectorsBody: document.getElementById("connectors-body"),
  delegationSummary: document.getElementById("delegation-summary"),
  delegationList: document.getElementById("delegation-list"),
  eventsSummary: document.getElementById("events-summary"),
  eventsList: document.getElementById("events-list"),
  providersSummary: document.getElementById("providers-summary"),
  providersList: document.getElementById("providers-list"),
  providerFormMode: document.getElementById("provider-form-mode"),
  providerForm: document.getElementById("provider-form"),
  providerPreset: document.getElementById("provider-preset"),
  providerId: document.getElementById("provider-id"),
  providerName: document.getElementById("provider-name"),
  providerKind: document.getElementById("provider-kind"),
  providerBaseUrl: document.getElementById("provider-base-url"),
  providerAuthMode: document.getElementById("provider-auth-mode"),
  providerDefaultModel: document.getElementById("provider-default-model"),
  providerLocal: document.getElementById("provider-local"),
  providerApiKey: document.getElementById("provider-api-key"),
  providerOauthConfig: document.getElementById("provider-oauth-config"),
  providerOauthToken: document.getElementById("provider-oauth-token"),
  providerAliasName: document.getElementById("provider-alias-name"),
  providerAliasModel: document.getElementById("provider-alias-model"),
  providerAliasDescription: document.getElementById("provider-alias-description"),
  providerSetMainRow: document.getElementById("provider-set-main-row"),
  providerSetMain: document.getElementById("provider-set-main"),
  providerBrowserAuth: document.getElementById("provider-browser-auth"),
  providerBrowserAuthStatus: document.getElementById("provider-browser-auth-status"),
  providerDiscoverModels: document.getElementById("provider-discover-models"),
  providerReset: document.getElementById("provider-reset"),
  providerModelResults: document.getElementById("provider-model-results"),
  aliasesList: document.getElementById("aliases-list"),
  aliasForm: document.getElementById("alias-form"),
  aliasName: document.getElementById("alias-name"),
  aliasProvider: document.getElementById("alias-provider"),
  aliasModel: document.getElementById("alias-model"),
  aliasDescription: document.getElementById("alias-description"),
  aliasMain: document.getElementById("alias-main"),
  memorySearchForm: document.getElementById("memory-search-form"),
  memorySearchQuery: document.getElementById("memory-search-query"),
  memorySearchResults: document.getElementById("memory-search-results"),
  memoryCreateForm: document.getElementById("memory-create-form"),
  memoryCreateKind: document.getElementById("memory-create-kind"),
  memoryCreateScope: document.getElementById("memory-create-scope"),
  memoryCreateSubject: document.getElementById("memory-create-subject"),
  memoryCreateContent: document.getElementById("memory-create-content"),
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
  connectorAddForm: document.getElementById("connector-add-form"),
  connectorAddType: document.getElementById("connector-add-type"),
  connectorAddName: document.getElementById("connector-add-name"),
  connectorAddId: document.getElementById("connector-add-id"),
  connectorAddDescription: document.getElementById("connector-add-description"),
  connectorAddAlias: document.getElementById("connector-add-alias"),
  connectorAddModel: document.getElementById("connector-add-model"),
  connectorAddCwd: document.getElementById("connector-add-cwd"),
  connectorAddEnabled: document.getElementById("connector-add-enabled"),
  connectorAddFields: document.getElementById("connector-add-fields"),
  connectorReset: document.getElementById("connector-reset"),
  runTaskForm: document.getElementById("run-task-form"),
  runTaskPrompt: document.getElementById("run-task-prompt"),
  runTaskAlias: document.getElementById("run-task-alias"),
  runTaskModel: document.getElementById("run-task-model"),
  runTaskThinking: document.getElementById("run-task-thinking"),
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
}

async function clearDashboardSession() {
  await fetch("/auth/dashboard/session", {
    method: "DELETE",
    credentials: "same-origin",
  });
  state.dashboardSessionAuthenticated = false;
}

const providerPresets = {
  codex: {
    id: "codex",
    name: "ChatGPT Codex",
    kind: "chat_gpt_codex",
    baseUrl: "https://chatgpt.com/backend-api/codex",
    authMode: "oauth",
    defaultModel: "gpt-5-codex",
    local: false,
    browserAuthKind: "codex",
    browserAuthLabel: "Codex",
  },
  openai: {
    id: "openai",
    name: "OpenAI",
    kind: "open_ai_compatible",
    baseUrl: "https://api.openai.com/v1",
    authMode: "api_key",
    defaultModel: "gpt-5",
    local: false,
  },
  anthropic: {
    id: "anthropic",
    name: "Claude",
    kind: "anthropic",
    baseUrl: "https://api.anthropic.com",
    authMode: "api_key",
    defaultModel: "claude-sonnet-4-20250514",
    local: false,
    browserAuthKind: "claude",
    browserAuthLabel: "Claude",
  },
  openrouter: {
    id: "openrouter",
    name: "OpenRouter",
    kind: "open_ai_compatible",
    baseUrl: "https://openrouter.ai/api/v1",
    authMode: "api_key",
    defaultModel: "openai/gpt-4.1",
    local: false,
  },
  moonshot: {
    id: "moonshot",
    name: "Moonshot",
    kind: "open_ai_compatible",
    baseUrl: "https://api.moonshot.ai/v1",
    authMode: "api_key",
    defaultModel: "kimi-k2",
    local: false,
  },
  venice: {
    id: "venice",
    name: "Venice AI",
    kind: "open_ai_compatible",
    baseUrl: "https://api.venice.ai/api/v1",
    authMode: "api_key",
    defaultModel: "venice-large",
    local: false,
  },
  ollama: {
    id: "ollama-local",
    name: "Ollama",
    kind: "ollama",
    baseUrl: "http://127.0.0.1:11434",
    authMode: "none",
    defaultModel: null,
    local: true,
  },
  custom: {
    id: "",
    name: "",
    kind: "open_ai_compatible",
    baseUrl: "",
    authMode: "api_key",
    defaultModel: null,
    local: false,
  },
};

function trimToNull(value) {
  const trimmed = String(value ?? "").trim();
  return trimmed ? trimmed : null;
}

function providerBrowserAuthLabel(kind) {
  return kind === "claude" ? "Claude" : "Codex";
}

function suggestedProviderDefaultModel(kind, baseUrl) {
  const normalizedBaseUrl = String(baseUrl ?? "").trim();
  if (kind === "chat_gpt_codex") {
    return "gpt-5-codex";
  }
  if (kind === "anthropic") {
    return "claude-sonnet-4-20250514";
  }
  if (kind === "open_ai_compatible") {
    if (normalizedBaseUrl === "https://api.openai.com/v1") {
      return "gpt-5";
    }
    if (normalizedBaseUrl === "https://openrouter.ai/api/v1") {
      return "openai/gpt-4.1";
    }
    if (normalizedBaseUrl === "https://api.moonshot.ai/v1") {
      return "kimi-k2";
    }
    if (normalizedBaseUrl === "https://api.venice.ai/api/v1") {
      return "venice-large";
    }
  }
  return null;
}

function resolveProviderDefaultModel() {
  return (
    trimToNull(elements.providerDefaultModel.value) ||
    suggestedProviderDefaultModel(elements.providerKind.value, elements.providerBaseUrl.value)
  );
}

function applySuggestedProviderModelDefaults() {
  const suggestedModel = resolveProviderDefaultModel();
  if (suggestedModel && !trimToNull(elements.providerDefaultModel.value)) {
    elements.providerDefaultModel.value = suggestedModel;
  }
  if (
    suggestedModel &&
    trimToNull(elements.providerAliasName.value) &&
    !trimToNull(elements.providerAliasModel.value)
  ) {
    elements.providerAliasModel.value = suggestedModel;
  }
  return suggestedModel;
}

function isProviderCreateMode() {
  return !state.editingProviderId;
}

function syncProviderModeUi() {
  const createMode = isProviderCreateMode();
  elements.providerFormMode.textContent = createMode
    ? "Create a new provider without replacing existing logged-in providers or aliases."
    : `Editing provider ${state.editingProviderId}`;
  elements.providerReset.textContent = createMode ? "Reset" : "Create new";
  elements.providerId.readOnly = !createMode;
  if (elements.providerSetMainRow) {
    elements.providerSetMainRow.hidden = createMode;
  }
  if (createMode) {
    elements.providerSetMain.checked = false;
  }
}

function providerSuggestionRequest() {
  const preset = providerPresets[elements.providerPreset.value] || providerPresets.custom;
  return {
    preferred_provider_id:
      trimToNull(elements.providerId.value) || preset.id || "provider",
    preferred_alias_name: trimToNull(elements.providerAliasName.value),
    default_model: resolveProviderDefaultModel(),
    editing_provider_id: state.editingProviderId || null,
    editing_alias_name: state.editingProviderId ? trimToNull(elements.providerAliasName.value) : null,
  };
}

function applyProviderSuggestions(suggestions) {
  const previous = state.providerAutoDefaults;
  state.providerAutoDefaults = suggestions;
  if (!isProviderCreateMode()) {
    return;
  }

  const currentProviderId = trimToNull(elements.providerId.value);
  if (!currentProviderId || (previous && currentProviderId === previous.provider_id)) {
    elements.providerId.value = suggestions.provider_id || "";
  }

  const currentAliasName = trimToNull(elements.providerAliasName.value);
  const previousAliasName = previous?.alias_name || null;
  if (!currentAliasName || currentAliasName === previousAliasName) {
    elements.providerAliasName.value = suggestions.alias_name || "";
  }

  const currentAliasModel = trimToNull(elements.providerAliasModel.value);
  const previousAliasModel = previous?.alias_model || null;
  if (!currentAliasModel || currentAliasModel === previousAliasModel) {
    elements.providerAliasModel.value = suggestions.alias_model || "";
  }
}

async function refreshProviderCreateSuggestions() {
  if (!isProviderCreateMode()) {
    state.providerAutoDefaults = null;
    syncProviderModeUi();
    return null;
  }
  if (!hasDashboardAuth()) {
    state.providerAutoDefaults = null;
    syncProviderModeUi();
    return null;
  }
  const suggestions = await apiPost("/v1/providers/suggest", providerSuggestionRequest());
  applyProviderSuggestions(suggestions);
  syncProviderModeUi();
  return suggestions;
}

async function resolveProviderFormSubmission() {
  const createMode = isProviderCreateMode();
  const requestedProviderId = trimToNull(elements.providerId.value);
  const requestedAliasName = trimToNull(elements.providerAliasName.value);
  let defaultModel = applySuggestedProviderModelDefaults();
  let providerId = requestedProviderId;
  let aliasName = requestedAliasName;
  let aliasModel = trimToNull(elements.providerAliasModel.value) || (aliasName ? defaultModel : null);
  const displayName = trimToNull(elements.providerName.value);

  if (createMode) {
    const suggestions = await apiPost("/v1/providers/suggest", providerSuggestionRequest());
    applyProviderSuggestions(suggestions);
    providerId = suggestions.provider_id;
    aliasName = suggestions.alias_name;
    aliasModel = suggestions.alias_model;
    defaultModel = suggestions.alias_model || defaultModel;

    const providerAdjusted =
      requestedProviderId !== providerId;
    const aliasAdjusted =
      (requestedAliasName || null) !== aliasName;
    if (providerAdjusted || aliasAdjusted) {
      setStatus(
        `Using safe multi-provider defaults for ${displayName || providerId}.`,
        "neutral"
      );
    }
  }

  if (!providerId || !displayName) {
    throw new Error("Provider ID and display name are required.");
  }
  if (aliasName && !aliasModel) {
    throw new Error("Set a default model or alias model before creating an alias.");
  }

  let setAsMain = false;
  if (aliasName) {
    if (createMode) {
      setAsMain = window.confirm(`Make '${aliasName}' the default alias?`);
    } else {
      setAsMain = elements.providerSetMain.checked;
    }
  }

  return {
    providerId,
    displayName,
    defaultModel,
    aliasName,
    aliasModel,
    setAsMain,
  };
}

function currentProviderBrowserAuthDescriptor() {
  const preset = providerPresets[elements.providerPreset.value] || providerPresets.custom;
  if (!state.editingProviderId && preset.browserAuthKind) {
    return {
      kind: preset.browserAuthKind,
      label: preset.browserAuthLabel || providerBrowserAuthLabel(preset.browserAuthKind),
    };
  }
  if (elements.providerKind.value === "chat_gpt_codex") {
    return { kind: "codex", label: "Codex" };
  }
  if (elements.providerKind.value === "anthropic") {
    return { kind: "claude", label: "Claude" };
  }
  return null;
}

function renderProviderBrowserAuthState() {
  const descriptor = currentProviderBrowserAuthDescriptor();
  const activeKind = state.providerAuthKind || descriptor?.kind || null;
  const activeLabel = activeKind ? providerBrowserAuthLabel(activeKind) : "browser";
  const isPending = !!state.providerAuthSessionId;
  elements.providerBrowserAuth.textContent = isPending
    ? `Waiting for ${activeLabel}...`
    : descriptor
      ? `Sign in with ${descriptor.label}`
      : "Sign in with browser";
  elements.providerBrowserAuth.disabled = !hasDashboardAuth() || isPending || !descriptor;
  const fallbackMessage = descriptor
    ? `${descriptor.label} browser sign-in will save credentials directly into this provider.`
    : "Browser sign-in is available for ChatGPT Codex and Claude providers.";
  elements.providerBrowserAuthStatus.textContent =
    state.providerAuthStatusMessage || fallbackMessage;
  elements.providerBrowserAuthStatus.dataset.tone =
    state.providerAuthStatusTone || "neutral";
}

function setProviderBrowserAuthStatus(message, tone = "neutral") {
  state.providerAuthStatusMessage = message;
  state.providerAuthStatusTone = tone;
  renderProviderBrowserAuthState();
}

function clearProviderBrowserAuthPolling() {
  if (state.providerAuthPollTimer) {
    clearInterval(state.providerAuthPollTimer);
    state.providerAuthPollTimer = null;
  }
}

function clearProviderSecretInputs() {
  elements.providerApiKey.value = "";
  elements.providerOauthToken.value = "";
}

async function pollProviderBrowserAuthSession(sessionId, { refresh = true } = {}) {
  const session = await apiGet(`/v1/provider-auth/${encodeURIComponent(sessionId)}`);
  if (state.providerAuthSessionId && state.providerAuthSessionId !== sessionId) {
    return session;
  }
  if (session.status === "pending") {
    const label = providerBrowserAuthLabel(session.kind);
    setProviderBrowserAuthStatus(
      `Continue the ${label} sign-in flow in the popup window.`,
      "neutral"
    );
    return session;
  }

  clearProviderBrowserAuthPolling();
  state.providerAuthSessionId = null;
  state.providerAuthKind = null;
  state.providerAuthWindow = null;
  if (session.status === "completed") {
    clearProviderSecretInputs();
    state.editingProviderId = session.provider_id;
    setProviderBrowserAuthStatus(
      `${providerBrowserAuthLabel(session.kind)} credentials saved for ${session.provider_id}.`,
      "ok"
    );
    if (refresh) {
      await refreshDashboard({ includeLoadedPanels: false });
      try {
        populateProviderForm(session.provider_id);
      } catch (_) {
      }
    }
  } else {
    setProviderBrowserAuthStatus(
      session.error || `${providerBrowserAuthLabel(session.kind)} sign-in failed.`,
      "warn"
    );
  }
  renderProviderBrowserAuthState();
  return session;
}

function startProviderBrowserAuthPolling(sessionId) {
  clearProviderBrowserAuthPolling();
  state.providerAuthPollTimer = window.setInterval(() => {
    pollProviderBrowserAuthSession(sessionId, { refresh: true }).catch((error) => {
      setProviderBrowserAuthStatus(`Browser sign-in check failed: ${error.message}`, "warn");
      clearProviderBrowserAuthPolling();
      state.providerAuthSessionId = null;
      state.providerAuthKind = null;
      state.providerAuthWindow = null;
      renderProviderBrowserAuthState();
    });
  }, 1500);
}

async function startProviderBrowserAuth() {
  const descriptor = currentProviderBrowserAuthDescriptor();
  if (!descriptor) {
    throw new Error("Browser sign-in is only available for Claude and Codex providers.");
  }
  const submission = await resolveProviderFormSubmission();
  const { providerId, displayName, defaultModel, aliasName, aliasModel, setAsMain } = submission;

  const popup = window.open(
    "",
    `provider-auth-${Date.now()}`,
    "popup=yes,width=720,height=840"
  );
  if (!popup) {
    throw new Error("The browser blocked the sign-in popup.");
  }
  popup.document.title = `Starting ${descriptor.label} sign-in`;
  popup.document.body.innerHTML =
    "<main><h1>Starting sign-in...</h1><p>The daemon is preparing the provider authorization flow.</p></main>";

  setProviderBrowserAuthStatus(`Starting ${descriptor.label} browser sign-in...`, "neutral");
  try {
    const response = await apiPost("/v1/provider-auth/start", {
      kind: descriptor.kind,
      provider_id: providerId,
      display_name: displayName,
      default_model: defaultModel,
      alias_name: aliasName,
      alias_model: aliasModel,
      alias_description: trimToNull(elements.providerAliasDescription.value),
      set_as_main: setAsMain,
    });
    if (response.status === "completed") {
      popup.close();
      state.providerAuthSessionId = null;
      state.providerAuthKind = null;
      state.providerAuthWindow = null;
      setProviderBrowserAuthStatus(
        `${descriptor.label} credentials saved for ${providerId}.`,
        "ok"
      );
      await refreshDashboard({ includeLoadedPanels: false });
      populateProviderForm(providerId);
      return;
    }
    if (!response.authorization_url) {
      popup.close();
      throw new Error("The daemon did not return an authorization URL.");
    }
    state.providerAuthSessionId = response.session_id;
    state.providerAuthKind = descriptor.kind;
    state.providerAuthWindow = popup;
    popup.location.replace(response.authorization_url);
    setProviderBrowserAuthStatus(
      `Continue the ${descriptor.label} sign-in flow in the popup window.`,
      "neutral"
    );
    renderProviderBrowserAuthState();
    startProviderBrowserAuthPolling(response.session_id);
  } catch (error) {
    popup.close();
    clearProviderBrowserAuthPolling();
    state.providerAuthSessionId = null;
    state.providerAuthKind = null;
    state.providerAuthWindow = null;
    setProviderBrowserAuthStatus(`Browser sign-in failed: ${error.message}`, "warn");
    throw error;
  }
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
  "connectorToggle",
  "connectorEdit",
  "connectorPoll",
  "providerEdit",
  "providerModels",
  "providerClearCreds",
  "providerDelete",
  "aliasEdit",
  "aliasMakeMain",
  "aliasDelete",
  "connectorDelete",
  "permissionPreset",
  "trustFlag",
  "memoryDelete",
  "useModel",
  "sessionView",
  "sessionUse",
  "sessionRename",
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

function connectorRow(kind, connector, target, detail, pollable, path) {
  const actions = [
    buttonHtml("Edit", { connectorEdit: `${kind}:${connector.id}` }, "button-ghost"),
    buttonHtml(
      connector.enabled ? "Disable" : "Enable",
      { connectorToggle: `${kind}:${connector.id}` },
      connector.enabled ? "button-small--ghost" : ""
    ),
    pollable ? buttonHtml("Poll", { connectorPoll: `${kind}:${connector.id}` }, "button-muted") : "",
    buttonHtml("Delete", { connectorDelete: `${kind}:${connector.id}` }, "button-small--ghost"),
  ]
    .filter(Boolean)
    .join("");
  return `
    <tr>
      <td>${escapeHtml(kind)}</td>
      <td>
        <strong>${escapeHtml(connector.name)}</strong>
        <div class="table-sub mono">${escapeHtml(connector.id)}</div>
      </td>
      <td>${badge(connector.enabled ? "enabled" : "disabled", connector.enabled ? "good" : "warn")}</td>
      <td>${escapeHtml(fmt(target))}</td>
      <td>
        <div>${escapeHtml(fmt(detail))}</div>
        <div class="table-sub">${escapeHtml(fmt(path))}</div>
      </td>
      <td><div class="inline-actions">${actions}</div></td>
    </tr>
  `;
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
  gmails = gmails || [];
  braves = braves || [];
  const cards = [
    ["Telegram", status.telegram_connectors],
    ["Discord", status.discord_connectors],
    ["Slack", status.slack_connectors],
    ["Signal", status.signal_connectors],
    ["Home Assistant", status.home_assistant_connectors],
    ["Webhooks", status.webhook_connectors],
    ["Inboxes", status.inbox_connectors],
    ["Gmail", gmails.length],
    ["Brave", braves.length],
  ];
  elements.connectorCards.innerHTML = cards
    .map(
      ([label, value]) => `
        <article class="stat-card">
          <p class="stat-card__label">${escapeHtml(label)}</p>
          <p class="stat-card__value">${escapeHtml(value)}</p>
          <p class="stat-card__hint">connector entries</p>
        </article>
      `
    )
    .join("");

  const rows = [
    ...telegrams.map((connector) =>
      connectorRow(
        "telegram",
        connector,
        fmtList(connector.allowed_chat_ids),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd
      )
    ),
    ...discords.map((connector) =>
      connectorRow(
        "discord",
        connector,
        fmtList(connector.monitored_channel_ids),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd
      )
    ),
    ...slacks.map((connector) =>
      connectorRow(
        "slack",
        connector,
        fmtList(connector.monitored_channel_ids),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd
      )
    ),
    ...signals.map((connector) =>
      connectorRow(
        "signal",
        connector,
        fmtList(connector.monitored_group_ids),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd || connector.cli_path || connector.account
      )
    ),
    ...homeAssistants.map((connector) =>
      connectorRow(
        "home-assistant",
        connector,
        fmtList(connector.monitored_entity_ids),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd || connector.base_url
      )
    ),
    ...webhooks.map((connector) =>
      connectorRow(
        "webhook",
        connector,
        connector.alias || "-",
        connector.requested_model || connector.cwd || "-",
        false,
        connector.prompt_template ? "template configured" : "-"
      )
    ),
    ...inboxes.map((connector) =>
      connectorRow(
        "inbox",
        connector,
        connector.path,
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd
      )
    ),
    ...gmails.map((connector) =>
      connectorRow(
        "gmail",
        connector,
        fmtList(connector.allowed_emails || connector.monitored_labels),
        connector.requested_model || connector.alias || "-",
        true,
        connector.cwd || connector.credentials_path || "-"
      )
    ),
    ...braves.map((connector) =>
      connectorRow(
        "brave",
        connector,
        "web/news/images/local",
        connector.requested_model || connector.alias || "online search",
        false,
        connector.cwd || "Brave Search API"
      )
    ),
  ];
  elements.connectorSummary.textContent = `${rows.length} connector entry/entries loaded`;
  elements.connectorsBody.innerHTML = rows.length
    ? rows.join("")
    : `<tr><td colspan="6" class="empty-table">No connectors configured.</td></tr>`;
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

function renderChatAliasOptions(providers, aliases) {
  const selectedValue = elements.runTaskAlias.value;
  const providerNames = new Map(
    (providers || []).map((provider) => [
      provider.id,
      provider.display_name || provider.id,
    ])
  );
  const grouped = new Map();
  (aliases || []).forEach((alias) => {
    const providerId = alias.provider_id || "unknown";
    if (!grouped.has(providerId)) {
      grouped.set(providerId, []);
    }
    grouped.get(providerId).push(alias);
  });
  const options = ['<option value="">Select an alias</option>'];
  Array.from(grouped.entries())
    .sort(([left], [right]) => left.localeCompare(right))
    .forEach(([providerId, providerAliases]) => {
      const providerLabel = providerNames.get(providerId) || providerId;
      options.push(`<optgroup label="${escapeHtml(providerLabel)}">`);
      providerAliases
        .sort((left, right) => (left.alias || "").localeCompare(right.alias || ""))
        .forEach((alias) => {
          options.push(
            `<option value="${escapeHtml(alias.alias)}">${escapeHtml(
              `${alias.alias} · ${alias.model || providerId}`
            )}</option>`
          );
        });
      options.push("</optgroup>");
    });
  elements.runTaskAlias.innerHTML = options.join("");
  if (selectedValue && (aliases || []).some((alias) => alias.alias === selectedValue)) {
    elements.runTaskAlias.value = selectedValue;
  } else if (state.lastData?.status?.main_agent_alias) {
    elements.runTaskAlias.value = state.lastData.status.main_agent_alias;
  }
}

function renderMainTargetSummary(mainTarget, mainAlias) {
  if (mainTarget) {
    elements.chatMainTarget.textContent = `Default main alias: ${mainTarget.alias} -> ${mainTarget.provider_display_name} / ${mainTarget.model}`;
  } else if (mainAlias) {
    elements.chatMainTarget.textContent = `Default main alias: ${mainAlias}`;
  } else {
    elements.chatMainTarget.textContent = "Default main alias: not configured.";
  }
}

function renderProviders(providers, aliases) {
  const mainAlias = state.lastData?.status?.main_agent_alias || null;
  const mainTarget = state.lastData?.status?.main_target || null;
  elements.providersSummary.textContent = mainTarget
    ? `${providers.length} provider(s), ${aliases.length} alias(es) · main ${mainTarget.alias} -> ${mainTarget.provider_display_name} / ${mainTarget.model}`
    : `${providers.length} provider(s), ${aliases.length} alias(es)`;
  renderChatAliasOptions(providers, aliases);
  renderMainTargetSummary(mainTarget, mainAlias);
  elements.providersList.innerHTML = providers.length
    ? providers
        .map(
          (provider) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h4>${escapeHtml(provider.id)}</h4>
                  <p class="card-subtitle">${escapeHtml(provider.display_name || provider.id)} · ${escapeHtml(provider.kind || "-")}</p>
                </div>
                ${badge(provider.auth_mode || "unknown", "info")}
              </div>
              <ul class="micro-list">
                <li>base_url: ${escapeHtml(fmt(provider.base_url))}</li>
                <li>default_model: ${escapeHtml(fmt(provider.default_model))}</li>
                <li>local: ${escapeHtml(fmtBoolean(provider.local))}</li>
              </ul>
              <div class="inline-actions">
                ${buttonHtml("Edit", { providerEdit: provider.id }, "button-ghost")}
                ${buttonHtml("Models", { providerModels: provider.id }, "button-muted")}
                ${buttonHtml("Clear creds", { providerClearCreds: provider.id }, "button-small--ghost")}
                ${buttonHtml("Delete", { providerDelete: provider.id }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No providers configured.");
  elements.aliasesList.innerHTML = aliases.length
    ? aliases
        .map(
          (alias) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div>
                  <h4>${escapeHtml(alias.alias || alias.name || alias.id)}</h4>
                  <p class="card-subtitle">${escapeHtml(alias.provider_id || "-")} / ${escapeHtml(alias.model || "-")}</p>
                </div>
                ${mainAlias === alias.alias ? badge("main", "good") : ""}
                ${elements.runTaskAlias.value === alias.alias ? badge("current chat", "info") : ""}
              </div>
              ${alias.description ? `<p class="card-copy">${escapeHtml(alias.description)}</p>` : ""}
              <div class="inline-actions">
                ${buttonHtml("Edit", { aliasEdit: alias.alias }, "button-ghost")}
                ${mainAlias === alias.alias ? "" : buttonHtml("Set main", { aliasMakeMain: alias.alias }, "button-muted")}
                ${buttonHtml("Delete", { aliasDelete: alias.alias }, "button-small--ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No aliases configured.");
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
              <td>${escapeHtml(fmt(session.model || session.requested_model))}</td>
              <td>${escapeHtml(fmt(session.message_count))}</td>
              <td>${escapeHtml(fmtDate(session.created_at))}</td>
              <td>${escapeHtml(fmtDate(session.updated_at))}</td>
              <td><div class="inline-actions">${buttonHtml("Chat", { sessionUse: session.id }, "button-muted")}${buttonHtml("View", { sessionView: session.id }, "button-ghost")}${buttonHtml("Rename", { sessionRename: session.id }, "button-small--ghost")}</div></td>
            </tr>
          `
        )
        .join("")
    : `<tr><td colspan="7" class="empty-table">No sessions yet.</td></tr>`;
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
    { providers: data.providers, aliases: data.aliases, mainAlias: data.status?.main_agent_alias },
    () => renderProviders(data.providers, data.aliases)
  );
  renderWhenChanged("permissions", data.permissions, () => renderPermissions(data.permissions));
  renderWhenChanged("trust", data.trust, () => renderTrust(data.trust));
  renderWhenChanged("sessions", data.sessions, () => renderSessions(data.sessions));
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
  renderWhenChanged("memory", memoryReview, () => renderMemory(memoryReview));
  return memoryReview;
}

async function loadProfilePanel() {
  const profileMemories = await apiGet("/v1/memory/profile");
  mergeLastData({ profileMemories });
  renderWhenChanged("profile", profileMemories, () => renderProfile(profileMemories));
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
  elements.connectorsBody.innerHTML =
    `<tr><td colspan="6" class="empty-table">No connectors configured.</td></tr>`;
  elements.delegationSummary.textContent = "No delegation data yet";
  elements.delegationList.innerHTML = renderEmpty("No delegation targets available.");
  elements.eventsSummary.textContent = "No event data yet";
  elements.eventsList.innerHTML = renderEmpty("No daemon events yet.");
  elements.providersSummary.textContent = "No provider data yet";
  elements.providersList.innerHTML = renderEmpty("No providers configured.");
  elements.aliasesList.innerHTML = renderEmpty("No aliases configured.");
  elements.permissionsSummary.textContent = "No permissions data yet";
  elements.permissionPresetActions.innerHTML = "";
  elements.permissionsDetails.innerHTML = "";
  renderTrust({ trusted_paths: [] });
  renderSessions([]);
  elements.daemonConfigSummary.textContent = "No daemon config yet";
  elements.daemonPersistenceActions.innerHTML = "";
  elements.daemonAutostartActions.innerHTML = "";
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
    const bootstrap = normalizeBootstrapData(await apiGet("/v1/dashboard/bootstrap"));
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
    if (isProviderCreateMode()) {
      try {
        await refreshProviderCreateSuggestions();
      } catch (error) {
        setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      }
    } else {
      syncProviderModeUi();
    }
    renderProviderBrowserAuthState();
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
    renderProviderBrowserAuthState();
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
    state.refreshTimer = setInterval(() => {
      refreshDashboard({
        includeLoadedPanels: false,
        includeHealth: false,
        silent: true,
      }).catch(() => {});
    }, 12000);
    state.healthTimer = setInterval(() => {
      refreshHealth({ silent: true }).catch(() => {});
    }, 60000);
  }
}

function providerPresetKeyFor(provider) {
  if (!provider) {
    return "custom";
  }
  if (provider.kind === "chat_gpt_codex") {
    return "codex";
  }
  if (provider.kind === "anthropic" && provider.base_url === "https://api.anthropic.com") {
    return "anthropic";
  }
  if (provider.kind === "open_ai_compatible" && provider.base_url === "https://api.openai.com/v1") {
    return "openai";
  }
  if (provider.kind === "open_ai_compatible" && provider.base_url === "https://openrouter.ai/api/v1") {
    return "openrouter";
  }
  if (provider.kind === "open_ai_compatible" && provider.base_url === "https://api.moonshot.ai/v1") {
    return "moonshot";
  }
  if (provider.kind === "open_ai_compatible" && provider.base_url === "https://api.venice.ai/api/v1") {
    return "venice";
  }
  if (provider.kind === "ollama") {
    return "ollama";
  }
  return "custom";
}

function resetProviderForm(applyPreset = true) {
  state.editingProviderId = null;
  state.providerAutoDefaults = null;
  elements.providerForm.reset();
  clearProviderSecretInputs();
  if (!state.providerAuthSessionId) {
    state.providerAuthStatusMessage = "";
    state.providerAuthStatusTone = "neutral";
  }
  syncProviderModeUi();
  if (applyPreset) {
    applyProviderPreset(elements.providerPreset.value || "codex");
    refreshProviderCreateSuggestions().catch((error) => {
      setStatus(`Provider suggestion failed: ${error.message}`, "warn");
    });
  }
  elements.providerModelResults.innerHTML = "";
  renderProviderBrowserAuthState();
}

function applyProviderPreset(presetKey) {
  const preset = providerPresets[presetKey] || providerPresets.custom;
  const previous = state.providerAutoDefaults;
  const currentProviderId = trimToNull(elements.providerId.value);
  if (!currentProviderId || (previous && currentProviderId === previous.provider_id)) {
    elements.providerId.value = preset.id;
  }
  elements.providerName.value = preset.name;
  elements.providerKind.value = preset.kind;
  elements.providerBaseUrl.value = preset.baseUrl;
  elements.providerAuthMode.value = preset.authMode;
  elements.providerLocal.checked = preset.local;
  if (!state.editingProviderId) {
    elements.providerDefaultModel.value = preset.defaultModel || "";
    const currentAliasName = trimToNull(elements.providerAliasName.value);
    const previousAliasName = previous?.alias_name || null;
    if (!currentAliasName || currentAliasName === previousAliasName) {
      elements.providerAliasName.value = "";
    }
    const currentAliasModel = trimToNull(elements.providerAliasModel.value);
    const previousAliasModel = previous?.alias_model || null;
    if (!currentAliasModel || currentAliasModel === previousAliasModel) {
      elements.providerAliasModel.value = preset.defaultModel || "";
    }
    elements.providerAliasDescription.value = "";
  }
  if (!state.providerAuthSessionId) {
    state.providerAuthStatusMessage = "";
    state.providerAuthStatusTone = "neutral";
  }
  applySuggestedProviderModelDefaults();
  syncProviderModeUi();
  renderProviderBrowserAuthState();
}

function populateProviderForm(providerId) {
  const provider = (state.lastData?.providers || []).find((entry) => entry.id === providerId);
  if (!provider) {
    throw new Error(`Unknown provider '${providerId}'.`);
  }
  state.editingProviderId = providerId;
  state.providerAutoDefaults = null;
  elements.providerPreset.value = providerPresetKeyFor(provider);
  elements.providerId.value = provider.id;
  elements.providerName.value = provider.display_name || provider.id;
  elements.providerKind.value = provider.kind;
  elements.providerBaseUrl.value = provider.base_url || "";
  elements.providerAuthMode.value = provider.auth_mode || "api_key";
  elements.providerDefaultModel.value = provider.default_model || "";
  elements.providerLocal.checked = !!provider.local;
  clearProviderSecretInputs();
  elements.providerOauthConfig.value = provider.oauth ? JSON.stringify(provider.oauth, null, 2) : "";
  const aliases = (state.lastData?.aliases || []).filter((entry) => entry.provider_id === provider.id);
  const primaryAlias = aliases[0];
  elements.providerAliasName.value = primaryAlias?.alias || "";
  elements.providerAliasModel.value = primaryAlias?.model || provider.default_model || "";
  elements.providerAliasDescription.value = primaryAlias?.description || "";
  elements.providerSetMain.checked = state.lastData?.status?.main_agent_alias === primaryAlias?.alias;
  applySuggestedProviderModelDefaults();
  syncProviderModeUi();
  if (!state.providerAuthSessionId) {
    state.providerAuthStatusMessage = "";
    state.providerAuthStatusTone = "neutral";
  }
  renderProviderBrowserAuthState();
}

async function discoverProviderModels(providerId) {
  const models = await apiGet(`/v1/providers/${encodeURIComponent(providerId)}/models`);
  elements.providerModelResults.innerHTML = Array.isArray(models) && models.length
    ? models
        .map(
          (model) => `
            <article class="stack-card">
              <div class="card-title-row">
                <div><h4>${escapeHtml(model)}</h4></div>
                ${buttonHtml("Use", { useModel: model }, "button-ghost")}
              </div>
            </article>
          `
        )
        .join("")
    : renderEmpty("No models reported by the provider.");
}

function renderActiveSessionDetail() {
  if (!state.loadedPanels.has("sessions")) {
    return;
  }
  elements.sessionDetail.innerHTML = state.activeChatSessionId
    ? renderSessionMessages(state.activeTranscript || [])
    : renderEmpty("No session selected.");
}

function snapshotChatState() {
  return {
    activeChatSessionId: state.activeChatSessionId,
    pendingChatSessionId: state.pendingChatSessionId,
    activeTranscript: [...(state.activeTranscript || [])],
    chatSessionMeta: elements.chatSessionMeta.textContent,
  };
}

function restoreChatState(snapshot) {
  state.activeChatSessionId = snapshot.activeChatSessionId;
  state.pendingChatSessionId = snapshot.pendingChatSessionId;
  state.activeTranscript = [...(snapshot.activeTranscript || [])];
  elements.chatSessionMeta.textContent = snapshot.chatSessionMeta;
  if (state.lastData) {
    renderProviders(state.lastData.providers || [], state.lastData.aliases || []);
  }
  renderChatTranscript(state.activeTranscript);
  renderActiveSessionDetail();
}

async function loadChatSession(sessionId, { focusChat = false } = {}) {
  const transcript = await apiGet(`/v1/sessions/${encodeURIComponent(sessionId)}`);
  state.activeChatSessionId = transcript.session.id;
  state.pendingChatSessionId = null;
  state.activeTranscript = transcript.messages || [];
  elements.runTaskAlias.value = transcript.session.alias || "";
  elements.runTaskModel.value = "";
  elements.chatSessionMeta.textContent = [
    transcript.session.title || transcript.session.id,
    transcript.session.alias,
    transcript.session.model,
  ]
    .filter(Boolean)
    .join(" | ");
  renderChatTranscript(transcript.messages || []);
  renderActiveSessionDetail();
  if (focusChat) {
    document.getElementById("run-task").scrollIntoView({ behavior: "smooth", block: "start" });
  }
}

function startNewChat() {
  state.activeChatSessionId = null;
  state.pendingChatSessionId = null;
  state.activeTranscript = [];
  elements.runTaskPrompt.value = "";
  elements.chatSessionMeta.textContent = "New chat";
  renderChatTranscript([]);
  renderActiveSessionDetail();
}

function setChatRunState(inFlight) {
  state.chatRunInFlight = inFlight;
  elements.chatNewSession.disabled = inFlight;
  elements.chatRenameButton.disabled = inFlight;
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

function resetConnectorForm() {
  state.editingConnectorKey = null;
  elements.connectorAddForm.reset();
  elements.connectorAddEnabled.checked = true;
  updateConnectorAddFields();
}

function populateConnectorForm(kind, id) {
  const connector = lookupConnector(kind, id);
  if (!connector) {
    throw new Error("Unknown connector.");
  }
  state.editingConnectorKey = `${kind}:${id}`;
  elements.connectorAddType.value = kind;
  updateConnectorAddFields();
  elements.connectorAddName.value = connector.name || "";
  elements.connectorAddId.value = connector.id || "";
  elements.connectorAddDescription.value = connector.description || "";
  elements.connectorAddAlias.value = connector.alias || "";
  elements.connectorAddModel.value = connector.requested_model || "";
  elements.connectorAddCwd.value = connector.cwd || "";
  elements.connectorAddEnabled.checked = connector.enabled !== false;
  const fieldValues = {
    path: connector.path,
    prompt_template: connector.prompt_template,
    delete_after_read: !!connector.delete_after_read,
    require_pairing_approval: connector.require_pairing_approval !== false,
    allowed_chat_ids: (connector.allowed_chat_ids || []).join(", "),
    allowed_user_ids: (connector.allowed_user_ids || []).join(", "),
    monitored_channel_ids: (connector.monitored_channel_ids || []).join(", "),
    allowed_channel_ids: (connector.allowed_channel_ids || []).join(", "),
    monitored_group_ids: (connector.monitored_group_ids || []).join(", "),
    allowed_group_ids: (connector.allowed_group_ids || []).join(", "),
    monitored_entity_ids: (connector.monitored_entity_ids || []).join(", "),
    allowed_service_domains: (connector.allowed_service_domains || []).join(", "),
    allowed_service_entity_ids: (connector.allowed_service_entity_ids || []).join(", "),
    allowed_sender_addresses: (connector.allowed_sender_addresses || []).join(", "),
    label_filter: connector.label_filter,
    base_url: connector.base_url,
    api_key: "",
    account: connector.account,
    cli_path: connector.cli_path,
  };
  elements.connectorAddFields.querySelectorAll("[data-connector-field]").forEach((input) => {
    const key = input.dataset.connectorField;
    if (input instanceof HTMLInputElement && input.type === "checkbox") {
      input.checked = Boolean(fieldValues[key]);
    } else {
      input.value = fieldValues[key] || "";
    }
  });
}

function persistToken(token) {
  state.token = token.trim();
  renderProviderBrowserAuthState();
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
  clearProviderBrowserAuthPolling();
  state.dashboardSessionAuthenticated = false;
  state.token = "";
  state.pendingChatSessionId = null;
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
  renderProviderBrowserAuthState();
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

function lookupConnector(kind, id) {
  if (!state.lastData) {
    return null;
  }
  const groups = {
    telegram: state.lastData.telegrams,
    discord: state.lastData.discords,
    slack: state.lastData.slacks,
    signal: state.lastData.signals,
    "home-assistant": state.lastData.homeAssistants,
    webhook: state.lastData.webhooks,
    inbox: state.lastData.inboxes,
    gmail: state.lastData.gmails,
    brave: state.lastData.braves,
  };
  return (groups[kind] || []).find((entry) => entry.id === id) || null;
}

function connectorBasePath(kind) {
  return {
    telegram: "/v1/telegram",
    discord: "/v1/discord",
    slack: "/v1/slack",
    signal: "/v1/signal",
    "home-assistant": "/v1/home-assistant",
    webhook: "/v1/webhooks",
    inbox: "/v1/inboxes",
    gmail: "/v1/gmail",
    brave: "/v1/brave",
  }[kind];
}

async function toggleConnector(kind, id) {
  const connector = lookupConnector(kind, id);
  const basePath = connectorBasePath(kind);
  if (!connector || !basePath) {
    throw new Error("Unknown connector.");
  }
  await apiPost(basePath, {
    connector: {
      ...connector,
      enabled: !connector.enabled,
    },
  });
  await refreshDashboard();
}

async function pollConnector(kind, id) {
  const basePath = connectorBasePath(kind);
  if (!basePath || kind === "webhook" || kind === "brave") {
    throw new Error("This connector does not support polling.");
  }
  await apiPost(`${basePath}/${encodeURIComponent(id)}/poll`, {});
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
      } else if (target.dataset.connectorToggle) {
        const [kind, id] = target.dataset.connectorToggle.split(":");
        await toggleConnector(kind, id);
      } else if (target.dataset.connectorEdit) {
        const [kind, id] = target.dataset.connectorEdit.split(":");
        populateConnectorForm(kind, id);
        document.getElementById("connector-add").scrollIntoView({ behavior: "smooth", block: "start" });
      } else if (target.dataset.connectorPoll) {
        const [kind, id] = target.dataset.connectorPoll.split(":");
        await pollConnector(kind, id);
      } else if (target.dataset.providerEdit) {
        populateProviderForm(target.dataset.providerEdit);
        document.getElementById("providers").scrollIntoView({ behavior: "smooth", block: "start" });
      } else if (target.dataset.providerModels) {
        await discoverProviderModels(target.dataset.providerModels);
      } else if (target.dataset.providerClearCreds) {
        await apiDelete(`/v1/providers/${encodeURIComponent(target.dataset.providerClearCreds)}/credentials`);
        await refreshDashboard();
      } else if (target.dataset.providerDelete) {
        if (window.confirm(`Delete provider ${target.dataset.providerDelete}? Aliases pointing at it will also be removed.`)) {
          await apiDelete(`/v1/providers/${encodeURIComponent(target.dataset.providerDelete)}`);
          await refreshDashboard();
          resetProviderForm();
        }
      } else if (target.dataset.aliasEdit) {
        const alias = (state.lastData?.aliases || []).find((entry) => entry.alias === target.dataset.aliasEdit);
        if (!alias) {
          throw new Error("Unknown alias.");
        }
        elements.aliasName.value = alias.alias;
        elements.aliasProvider.value = alias.provider_id;
        elements.aliasModel.value = alias.model;
        elements.aliasDescription.value = alias.description || "";
        elements.aliasMain.checked = state.lastData?.status?.main_agent_alias === alias.alias;
      } else if (target.dataset.aliasMakeMain) {
        await updateMainAlias(target.dataset.aliasMakeMain);
      } else if (target.dataset.aliasDelete) {
        if (window.confirm(`Delete alias ${target.dataset.aliasDelete}?`)) {
          await apiDelete(`/v1/aliases/${encodeURIComponent(target.dataset.aliasDelete)}`);
          await refreshDashboard();
        }
      } else if (target.dataset.connectorDelete) {
        const [kind, id] = target.dataset.connectorDelete.split(":");
        if (window.confirm(`Delete ${kind} connector ${id}?`)) {
          const basePath = connectorBasePath(kind);
          if (basePath) {
            await apiDelete(`${basePath}/${encodeURIComponent(id)}`);
            await refreshDashboard();
          }
        }
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
      } else if (target.dataset.useModel) {
        elements.providerDefaultModel.value = target.dataset.useModel;
        if (!elements.providerAliasModel.value.trim()) {
          elements.providerAliasModel.value = target.dataset.useModel;
        }
        await refreshProviderCreateSuggestions();
      } else if (target.dataset.sessionView) {
        try {
          state.loadedPanels.add("sessions");
          const session = await apiGet(`/v1/sessions/${encodeURIComponent(target.dataset.sessionView)}`);
          const messages = session.messages || session.history || [];
          elements.sessionDetail.innerHTML = renderSessionMessages(messages);
        } catch (err) {
          elements.sessionDetail.innerHTML = renderEmpty(`Failed to load session: ${err.message}`);
        }
      } else if (target.dataset.sessionUse) {
        ensureChatRunIdle("switching chats");
        state.loadedPanels.add("sessions");
        await loadChatSession(target.dataset.sessionUse, { focusChat: true });
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
  state.providerAuthSessionId = null;
  state.providerAuthKind = null;
  state.providerAuthWindow = null;
  state.providerAuthStatusMessage = "";
  state.providerAuthStatusTone = "neutral";
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

elements.providerPreset.addEventListener("change", () => {
  if (!state.editingProviderId) {
    applyProviderPreset(elements.providerPreset.value);
    refreshProviderCreateSuggestions().catch((error) => {
      setStatus(`Provider suggestion failed: ${error.message}`, "warn");
    });
  }
});

elements.providerReset.addEventListener("click", () => {
  resetProviderForm();
});

elements.providerBrowserAuth.addEventListener("click", async () => {
  try {
    await startProviderBrowserAuth();
  } catch (error) {
    setStatus(`Browser sign-in failed: ${error.message}`, "warn");
  }
});

elements.providerKind.addEventListener("change", () => {
  if (!state.providerAuthSessionId) {
    state.providerAuthStatusMessage = "";
    state.providerAuthStatusTone = "neutral";
  }
  applySuggestedProviderModelDefaults();
  renderProviderBrowserAuthState();
  refreshProviderCreateSuggestions().catch((error) => {
    setStatus(`Provider suggestion failed: ${error.message}`, "warn");
  });
});

elements.providerBaseUrl.addEventListener("change", () => {
  applySuggestedProviderModelDefaults();
  renderProviderBrowserAuthState();
  refreshProviderCreateSuggestions().catch((error) => {
    setStatus(`Provider suggestion failed: ${error.message}`, "warn");
  });
});

elements.providerDefaultModel.addEventListener("change", () => {
  if (trimToNull(elements.providerAliasName.value) && !trimToNull(elements.providerAliasModel.value)) {
    elements.providerAliasModel.value = trimToNull(elements.providerDefaultModel.value) || "";
  }
  refreshProviderCreateSuggestions().catch((error) => {
    setStatus(`Provider suggestion failed: ${error.message}`, "warn");
  });
});

elements.providerId.addEventListener("change", () => {
  refreshProviderCreateSuggestions().catch((error) => {
    setStatus(`Provider suggestion failed: ${error.message}`, "warn");
  });
});

elements.providerAliasName.addEventListener("change", () => {
  applySuggestedProviderModelDefaults();
  renderProviderBrowserAuthState();
  refreshProviderCreateSuggestions().catch((error) => {
    setStatus(`Provider suggestion failed: ${error.message}`, "warn");
  });
});

elements.providerDiscoverModels.addEventListener("click", async () => {
  try {
    const providerId = elements.providerId.value.trim();
    if (!providerId) {
      throw new Error("Save or enter a provider ID first.");
    }
    await discoverProviderModels(providerId);
  } catch (error) {
    setStatus(`Model discovery failed: ${error.message}`, "warn");
  }
});

elements.providerForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const submission = await resolveProviderFormSubmission();
    const { providerId, displayName, defaultModel, aliasName, aliasModel, setAsMain } = submission;
    const provider = {
      id: providerId,
      display_name: displayName,
      kind: elements.providerKind.value,
      base_url: elements.providerBaseUrl.value.trim(),
      auth_mode: elements.providerAuthMode.value,
      default_model: defaultModel,
      keychain_account: null,
      oauth: null,
      local: elements.providerLocal.checked,
    };
    const oauthConfigText = elements.providerOauthConfig.value.trim();
    if (oauthConfigText) {
      provider.oauth = JSON.parse(oauthConfigText);
    }
    const payload = {
      provider,
      api_key: elements.providerApiKey.value.trim() || null,
      oauth_token: null,
    };
    const oauthTokenText = elements.providerOauthToken.value.trim();
    if (oauthTokenText) {
      payload.oauth_token = JSON.parse(oauthTokenText);
    }
    await apiPost("/v1/providers", payload);
    clearProviderSecretInputs();

    if (aliasName) {
      if (!trimToNull(elements.providerAliasModel.value) && aliasModel) {
        elements.providerAliasModel.value = aliasModel;
      }
      await apiPost("/v1/aliases", {
        alias: {
          alias: aliasName,
          provider_id: providerId,
          model: aliasModel,
          description: elements.providerAliasDescription.value.trim() || null,
        },
        set_as_main: setAsMain,
      });
    }
    await refreshDashboard();
    populateProviderForm(providerId);
    setStatus(`Provider ${providerId} saved.`, "ok");
  } catch (error) {
    setStatus(`Provider save failed: ${error.message}`, "warn");
  }
});

elements.aliasForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const name = elements.aliasName.value.trim();
    const providerId = elements.aliasProvider.value.trim();
    const model = elements.aliasModel.value.trim();
    if (!name || !providerId || !model) {
      throw new Error("Alias name, provider ID, and model are required.");
    }
    await apiPost("/v1/aliases", {
      alias: {
        alias: name,
        provider_id: providerId,
        model,
        description: elements.aliasDescription.value.trim() || null,
      },
      set_as_main: elements.aliasMain.checked,
    });
    elements.aliasForm.reset();
    await refreshDashboard();
  } catch (error) {
    setStatus(`Alias save failed: ${error.message}`, "warn");
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
    const items = Array.isArray(results) ? results : results.results || [];
    elements.memorySearchResults.innerHTML = items.length
      ? items
          .map(
            (memory) => `
              <article class="stack-card">
                <div class="card-title-row">
                  <div>
                    <h4>${escapeHtml(memory.subject)}</h4>
                    <p class="card-subtitle">${escapeHtml(memory.kind || "-")} · ${escapeHtml(memory.scope || "-")}</p>
                  </div>
                  ${buttonHtml("Delete", { memoryDelete: memory.id }, "button-small--ghost")}
                </div>
                <p class="card-copy">${escapeHtml(memory.content)}</p>
              </article>
            `
          )
          .join("")
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
    if (!subject || !content) {
      throw new Error("Subject and content are required.");
    }
    await apiPost("/v1/memory", {
      kind: elements.memoryCreateKind.value,
      scope: elements.memoryCreateScope.value,
      subject,
      content,
    });
    elements.memoryCreateForm.reset();
    await refreshDashboard();
  } catch (error) {
    setStatus(`Memory create failed: ${error.message}`, "warn");
  }
});

function connectorTypeFields(type) {
  const fieldMap = {
    telegram: [
      { label: "Require pairing approval", id: "require_pairing_approval", type: "checkbox" },
      { label: "Bot token", id: "bot_token", placeholder: "123456:ABC-DEF..." },
      { label: "Allowed chat IDs (comma-separated)", id: "allowed_chat_ids", placeholder: "12345,67890" },
      { label: "Allowed user IDs (comma-separated)", id: "allowed_user_ids", placeholder: "12345,67890" },
    ],
    discord: [
      { label: "Require pairing approval", id: "require_pairing_approval", type: "checkbox" },
      { label: "Bot token", id: "bot_token", placeholder: "Discord bot token" },
      { label: "Monitored channel IDs (comma-separated)", id: "monitored_channel_ids", placeholder: "123,456" },
      { label: "Allowed channel IDs (comma-separated)", id: "allowed_channel_ids", placeholder: "123,456" },
      { label: "Allowed user IDs (comma-separated)", id: "allowed_user_ids", placeholder: "123,456" },
    ],
    slack: [
      { label: "Require pairing approval", id: "require_pairing_approval", type: "checkbox" },
      { label: "Bot token", id: "bot_token", placeholder: "xoxb-..." },
      { label: "Monitored channel IDs (comma-separated)", id: "monitored_channel_ids", placeholder: "C01..." },
      { label: "Allowed channel IDs (comma-separated)", id: "allowed_channel_ids", placeholder: "C01..." },
      { label: "Allowed user IDs (comma-separated)", id: "allowed_user_ids", placeholder: "U01..." },
    ],
    signal: [
      { label: "Account", id: "account", placeholder: "+1234567890" },
      { label: "CLI path", id: "cli_path", placeholder: "/usr/bin/signal-cli" },
      { label: "Require pairing approval", id: "require_pairing_approval", type: "checkbox" },
      { label: "Monitored group IDs (comma-separated)", id: "monitored_group_ids", placeholder: "group-id" },
      { label: "Allowed group IDs (comma-separated)", id: "allowed_group_ids", placeholder: "group-id" },
      { label: "Allowed user IDs (comma-separated)", id: "allowed_user_ids", placeholder: "+1234567890" },
    ],
    "home-assistant": [
      { label: "Base URL", id: "base_url", placeholder: "http://homeassistant.local:8123" },
      { label: "Access token", id: "access_token", placeholder: "HA long-lived access token" },
      { label: "Monitored entity IDs (comma-separated)", id: "monitored_entity_ids", placeholder: "sensor.temp" },
      { label: "Allowed service domains (comma-separated)", id: "allowed_service_domains", placeholder: "light,switch" },
      { label: "Allowed service entity IDs (comma-separated)", id: "allowed_service_entity_ids", placeholder: "light.office" },
    ],
    webhook: [
      { label: "Prompt template", id: "prompt_template", placeholder: "Handle: {{body}}" },
      { label: "Webhook token", id: "webhook_token", placeholder: "Generated or pasted shared secret" },
    ],
    inbox: [
      { label: "Path", id: "path", placeholder: "/path/to/inbox" },
      { label: "Delete after read", id: "delete_after_read", type: "checkbox" },
    ],
    gmail: [
      { label: "OAuth token", id: "oauth_token", placeholder: "Gmail OAuth access token" },
      { label: "Require pairing approval", id: "require_pairing_approval", type: "checkbox" },
      { label: "Allowed senders (comma-separated)", id: "allowed_sender_addresses", placeholder: "user@example.com" },
      { label: "Label filter", id: "label_filter", placeholder: "INBOX" },
    ],
    brave: [
      { label: "API key", id: "api_key", placeholder: "BSA..." },
    ],
  };
  return fieldMap[type] || [];
}

function updateConnectorAddFields() {
  const type = elements.connectorAddType.value;
  const fields = connectorTypeFields(type);
  elements.connectorAddFields.innerHTML = fields
    .map(
      (field) =>
        field.type === "checkbox"
          ? `
        <label class="checkbox-field">
          <input type="checkbox" data-connector-field="${escapeHtml(field.id)}">
          <span>${escapeHtml(field.label)}</span>
        </label>
      `
          : `
        <label class="field">
          <span>${escapeHtml(field.label)}</span>
          <input type="text" data-connector-field="${escapeHtml(field.id)}" autocomplete="off" placeholder="${escapeHtml(field.placeholder || "")}">
        </label>
      `
    )
    .join("");
}

elements.connectorAddType.addEventListener("change", updateConnectorAddFields);
updateConnectorAddFields();
elements.connectorReset.addEventListener("click", () => {
  resetConnectorForm();
});

elements.connectorAddForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    const type = elements.connectorAddType.value;
    const name = elements.connectorAddName.value.trim();
    if (!name) {
      throw new Error("Connector name is required.");
    }
    const connector = {
      id: elements.connectorAddId.value.trim() || crypto.randomUUID(),
      name,
      description: elements.connectorAddDescription.value.trim(),
      enabled: elements.connectorAddEnabled.checked,
    };
    if (elements.connectorAddAlias.value.trim()) {
      connector.alias = elements.connectorAddAlias.value.trim();
    }
    if (elements.connectorAddModel.value.trim()) {
      connector.requested_model = elements.connectorAddModel.value.trim();
    }
    if (elements.connectorAddCwd.value.trim()) {
      connector.cwd = elements.connectorAddCwd.value.trim();
    }
    const fieldInputs = elements.connectorAddFields.querySelectorAll("[data-connector-field]");
    const payload = { connector };
    for (const input of fieldInputs) {
      const key = input.dataset.connectorField;
      if (input instanceof HTMLInputElement && input.type === "checkbox") {
        connector[key] = input.checked;
        continue;
      }
      const value = input.value.trim();
      if (!value) {
        continue;
      }
      if (["bot_token", "access_token", "oauth_token", "webhook_token", "api_key"].includes(key)) {
        payload[key] = value;
      } else if (key === "allowed_chat_ids" || key === "allowed_user_ids") {
        connector[key] = parseDelimitedList(value, (item) => Number.parseInt(item, 10)).filter((item) => Number.isFinite(item));
      } else if (key.endsWith("_ids") || key.endsWith("_addresses") || key.endsWith("_domains")) {
        connector[key] = parseDelimitedList(value);
      } else {
        connector[key] = value;
      }
    }
    const basePath = connectorBasePath(type);
    if (!basePath) {
      throw new Error(`Unknown connector type: ${type}`);
    }
    await apiPost(basePath, payload);
    resetConnectorForm();
    await refreshDashboard();
  } catch (error) {
    setStatus(`Connector save failed: ${error.message}`, "warn");
  }
});

elements.runTaskForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  try {
    if (state.chatRunInFlight) {
      throw new Error("A chat run is already in progress.");
    }
    const prompt = elements.runTaskPrompt.value.trim();
    if (!prompt) {
      throw new Error("Prompt is required.");
    }
    const payload = {
      prompt,
      session_id: state.activeChatSessionId,
    };
    const alias = elements.runTaskAlias.value.trim();
    const model = elements.runTaskModel.value.trim();
    const thinking = elements.runTaskThinking.value;
    const permissionPreset = elements.runTaskPermission.value;
    if (alias) payload.alias = alias;
    if (model) payload.requested_model = model;
    if (thinking) payload.thinking_level = thinking;
    if (permissionPreset) payload.permission_preset = permissionPreset;
    const chatSnapshot = snapshotChatState();
    setChatRunState(true);
    const optimisticMessage = {
      role: "user",
      content: prompt,
      created_at: new Date().toISOString(),
      provider_id: alias || "user",
      model: model || "",
      tool_calls: [],
      attachments: [],
    };
    state.activeTranscript = [...(state.activeTranscript || []), optimisticMessage];
    renderChatTranscript(state.activeTranscript);
    renderActiveSessionDetail();
    elements.runTaskResult.innerHTML = renderEmpty("Working...");
    let completedResponse = null;
    let streamError = null;
    state.pendingChatSessionId = null;
    await apiStream("/v1/run/stream", payload, (streamEvent) => {
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
    });
    if (streamError) {
      throw new Error(streamError);
    }
    elements.runTaskPrompt.value = "";
    elements.runTaskResult.innerHTML = renderEmpty("Response received.");
    if (completedResponse && completedResponse.session_id) {
      await loadChatSession(completedResponse.session_id);
    }
    await refreshSessionsSummary();
  } catch (error) {
    if (!completedResponse) {
      restoreChatState(chatSnapshot);
    } else {
      state.pendingChatSessionId = null;
    }
    elements.runTaskResult.innerHTML = renderEmpty(`Task failed: ${error.message}`);
  } finally {
    setChatRunState(false);
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
resetProviderForm();
startNewChat();
bindActions();
setupLazyPanels();
scheduleRefresh();
window.addEventListener("hashchange", () => {
  const panelId = window.location.hash.replace(/^#/, "");
  if (panelId) {
    ensurePanelLoaded(panelId).catch((error) => {
      setStatus(`Panel load failed: ${error.message}`, "warn");
    });
  }
});
window.addEventListener("message", (event) => {
  if (event.origin !== window.location.origin) {
    return;
  }
  const data = event.data;
  if (!data || data.type !== "provider-auth" || !data.sessionId) {
    return;
  }
  if (state.providerAuthSessionId && state.providerAuthSessionId !== data.sessionId) {
    return;
  }
  pollProviderBrowserAuthSession(data.sessionId, { refresh: true }).catch((error) => {
    setProviderBrowserAuthStatus(`Browser sign-in failed: ${error.message}`, "warn");
    setStatus(`Browser sign-in failed: ${error.message}`, "warn");
  });
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
  if (state.lastData) {
    renderProviders(state.lastData.providers || [], state.lastData.aliases || []);
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
