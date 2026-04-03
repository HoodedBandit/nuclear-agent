import * as foundation from "./dashboard-core-foundation.js";

const {
  ACTION_DATASET_KEYS,
  PANEL_TO_TAB,
  activateDashboardTab,
  apiDelete,
  apiGet,
  apiPost,
  apiPut,
  apiRequest,
  apiStream,
  badge,
  bearerHeaders,
  buttonHtml,
  clearDashboardSession,
  createDashboardSession,
  dataAttributeName,
  displayLimit,
  elements,
  escapeHtml,
  findActionTarget,
  focusSection,
  forcePanelsForTab,
  fmt,
  fmtBoolean,
  fmtDate,
  formatMemoryEvidence,
  formatMemoryProvenance,
  hasActionDataset,
  hasDashboardAuth,
  heroChip,
  initializeLazyPanelPlaceholders,
  isPanelNearViewport,
  isUnauthorizedError,
  lazyPanelLoaders,
  loadActiveDashboardTab,
  loadApprovalsPanel,
  loadLogsPanel,
  loadMcpPanel,
  loadMemoryReviewPanel,
  loadMissionsPanel,
  loadProfilePanel,
  loadSessionsPanel,
  loadSkillsPanel,
  loadVisiblePanels,
  mergeLastData,
  normalizeBootstrapData,
  panelElement,
  parseDelimitedList,
  parseLimitInput,
  quickStartProvider,
  renderApprovals,
  renderBootstrapData,
  renderConnectors,
  renderDaemonConfig,
  renderDelegation,
  renderEmpty,
  renderEvents,
  renderHealth,
  renderLogs,
  renderMcpServers,
  renderMemory,
  renderMemoryCard,
  renderMissions,
  renderPermissions,
  renderProfile,
  renderSessionResumePacket,
  renderSessions,
  renderSkills,
  renderStatus,
  renderTrust,
  renderWhenChanged,
  resetDashboardSurface,
  saveActiveDashboardTab,
  setStatus,
  setupLazyPanels,
  stableKey,
  state,
  tabDefinition,
  updateDashboardPanelVisibility,
  updateDashboardTabButtons,
  updateDelegationFormInputs,
} = foundation;

function requireDashboardControl(name) {
  const fn = globalThis[name];
  if (typeof fn !== "function") {
    throw new Error(`Dashboard control function '${name}' is not available.`);
  }
  return fn;
}

function controlRequest(...args) {
  return requireDashboardControl("controlRequest")(...args);
}

function expectControlResponse(...args) {
  return requireDashboardControl("expectControlResponse")(...args);
}

function ensureControlSocket(...args) {
  return requireDashboardControl("ensureControlSocket")(...args);
}

function closeControlSocket(...args) {
  return requireDashboardControl("closeControlSocket")(...args);
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

function renderRemoteContentStreamEvent(artifact) {
  const source = artifact?.source || {};
  const assessment = artifact?.assessment || {};
  const label = source.label || source.host || source.url || "remote content";
  const risk = String(assessment.risk || "unknown");
  const blocked = Boolean(assessment.blocked);
  const warnings = [...(assessment.warnings || []), ...(assessment.reasons || [])].filter(Boolean);
  return {
    label,
    blocked,
    tone: blocked ? "warn" : "info",
    body: [
      `${blocked ? "Blocked" : "Observed"} remote content from ${label}.`,
      `risk: ${risk}`,
      warnings.length ? `warnings: ${warnings.join("; ")}` : null,
      artifact?.excerpt ? `excerpt: ${artifact.excerpt}` : null,
    ]
      .filter(Boolean)
      .join("\n"),
  };
}

function handleChatStreamEvent(streamEvent) {
  if (streamEvent.type === "session_started") {
    state.pendingChatSessionId = streamEvent.session_id || state.pendingChatSessionId;
    elements.chatSessionMeta.textContent = [streamEvent.alias, streamEvent.model, streamEvent.session_id]
      .filter(Boolean)
      .join(" | ");
    return {};
  }
  if (streamEvent.type === "message" && streamEvent.message) {
    state.activeTranscript = [...(state.activeTranscript || []), streamEvent.message];
    renderChatTranscript(state.activeTranscript);
    renderActiveSessionDetail();
    return {};
  }
  if (streamEvent.type === "remote_content" && streamEvent.artifact) {
    const signal = renderRemoteContentStreamEvent(streamEvent.artifact);
    state.activeTranscript = [
      ...(state.activeTranscript || []),
      {
        role: "tool",
        tool_name: "remote_content",
        content: signal.body,
        created_at: new Date().toISOString(),
        provider_id: "remote-content",
        model: "",
      },
    ];
    renderChatTranscript(state.activeTranscript);
    renderActiveSessionDetail();
    pushConsoleEntry("Remote content", signal.body, {
      tone: signal.tone,
      label: "remote-content",
      preformatted: false,
    });
    if (signal.blocked) {
      setStatus(`Remote content blocked: ${signal.label}`, "warn");
    }
    return {};
  }
  if (streamEvent.type === "completed") {
    return { completedResponse: streamEvent.response || null };
  }
  if (streamEvent.type === "error") {
    return { streamError: streamEvent.message || "Task failed." };
  }
  return {};
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
      const result = handleChatStreamEvent(streamEvent);
      if (result.completedResponse) {
        completedResponse = result.completedResponse;
      }
      if (result.streamError) {
        streamError = result.streamError;
      }
      if (streamEvent.type === "session_started" && streamEvent.session_id) {
        state.pendingChatSessionId = streamEvent.session_id;
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
    } catch (error) {
      if (error?.code === "control_socket_disconnected_after_send") {
        throw new Error(
          "Live control connection was lost after the task was already dispatched. Retry manually to avoid duplicate work."
        );
      }
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
        await window.dashboardControl.setAutopilot(target.dataset.autopilot);
      } else if (target.dataset.autonomy) {
        await window.dashboardControl.setAutonomy(target.dataset.autonomy);
      } else if (target.dataset.evolve) {
        await window.dashboardControl.setEvolve(target.dataset.evolve);
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

if (window.__dashboardTestMode) {
  window.dashboardApp.__debug = {
    emitChatStreamEvent: handleChatStreamEvent,
    dropControlSocket: () => closeControlSocket({ rejectMessage: "Control socket disconnected." }),
    getPendingControlRequestCount: () => state.controlSocketRequests.size,
  };
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

function bootstrapDashboard() {
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
  initializeDashboardConnection().catch((error) => {
    setStatus(`Refresh failed: ${error.message}`, "warn");
  });
}

const dashboardApp = window.dashboardApp;


export { refreshSessionsSummary, refreshDashboard, scheduleRefresh, renderActiveSessionDetail, snapshotChatState, restoreChatState, loadChatSession, startNewChat, setChatRunState, ensureChatRunIdle, messageTone, chatCodeLineClass, renderCodeBlock, renderRichContent, parseToolArgumentField, renderToolCallPreview, renderConversationMessage, renderConversationThread, renderSessionMessages, renderChatTranscript, syncChatCwdInput, setChatCwd, currentWorkspaceReport, renderChatAttachments, renderConsoleEntries, pushConsoleEntry, latestAssistantMessage, persistToken, bootstrapToken, clearDashboardConnectionState, initializeDashboardConnection, buildMissionPayload, createMission, handleApprovalAction, handleMemoryAction, handleSkillAction, handleMissionAction, currentChatCwd, activeSessionSummary, buildRunTaskPayload, preferAliasForProvider, resolveProviderAliasSelection, resolveSessionTarget, formatRecordLines, formatConnectorSummary, formatStatusSummary, buildReviewPrompt, copyLatestAssistantOutput, parseMaybeInteger, addChatAttachment, clearChatAttachments, forkSessionById, compactActiveChat, executeShellCommand, renderRemoteContentStreamEvent, handleChatStreamEvent, executeChatPrompt, bindActions, updateMainAlias, bootstrapDashboard, dashboardApp };
