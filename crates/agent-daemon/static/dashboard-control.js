import {
  apiPost,
  apiPut,
  buttonHtml,
  elements,
  escapeHtml,
  fmt,
  fmtBoolean,
  hasDashboardAuth,
  mergeLastData,
  refreshDashboard,
  refreshLoadedPanels,
  renderBootstrapData,
  renderEvents,
  renderLogs,
  renderWhenChanged,
  scheduleRefresh,
  setStatus,
  state,
} from "./dashboard-core.js";

function controlSocketUrl() {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/v1/ws`;
}

function controlConnectPayload() {
  return {
    type: "connect",
    request: {
      protocol_version: 2,
      client_name: "dashboard",
      subscriptions: [
        { topic: "status", limit: 1 },
        { topic: "logs", limit: 50 },
      ],
    },
  };
}

function scheduleControlDrivenRefresh(forcePanels = []) {
  if (state.controlRefreshTimer) {
    clearTimeout(state.controlRefreshTimer);
  }
  state.controlRefreshTimer = window.setTimeout(() => {
    state.controlRefreshTimer = null;
    if (!hasDashboardAuth()) {
      return;
    }
    refreshLoadedPanels(forcePanels).catch(() => {});
  }, 500);
}

function rejectPendingControlRequests(message) {
  for (const pending of state.controlSocketRequests.values()) {
    const error = new Error(message);
    error.code = "control_socket_disconnected_after_send";
    pending.reject(error);
  }
  state.controlSocketRequests.clear();
}

function closeControlSocket({ rejectMessage = null } = {}) {
  if (state.controlRefreshTimer) {
    clearTimeout(state.controlRefreshTimer);
    state.controlRefreshTimer = null;
  }
  if (rejectMessage) {
    rejectPendingControlRequests(rejectMessage);
  }
  if (state.controlSocket) {
    state.controlSocketClosing = true;
    try {
      state.controlSocket.close();
    } catch (_) {}
  }
  state.controlSocket = null;
  state.controlSocketReady = false;
  state.controlSocketPromise = null;
}

function expectControlResponse(response, expectedKind) {
  if (!response || response.kind !== expectedKind) {
    throw new Error(
      `Unexpected control response kind: ${response?.kind || "unknown"}`
    );
  }
  return response.payload;
}

function mergeLiveLogBatch(batch) {
  if (!state.lastData || !Array.isArray(batch?.entries) || !batch.entries.length) {
    return;
  }
  const existingEvents = Array.isArray(state.lastData.events) ? state.lastData.events : [];
  const seenEvents = new Set(existingEvents.map((entry) => entry.id));
  const mergedEvents = [...existingEvents];
  for (const entry of batch.entries) {
    if (!seenEvents.has(entry.id)) {
      mergedEvents.push(entry);
      seenEvents.add(entry.id);
    }
  }
  mergedEvents.sort(
    (left, right) => new Date(left.created_at).getTime() - new Date(right.created_at).getTime()
  );
  mergeLastData({ events: mergedEvents });
  renderWhenChanged("events", mergedEvents, () => renderEvents(mergedEvents));

  if (Array.isArray(state.lastData.logs)) {
    const existingLogs = state.lastData.logs;
    const incomingLogs = [...batch.entries].sort(
      (left, right) => new Date(right.created_at).getTime() - new Date(left.created_at).getTime()
    );
    const seenLogs = new Set(existingLogs.map((entry) => entry.id));
    const mergedLogs = [...incomingLogs.filter((entry) => !seenLogs.has(entry.id)), ...existingLogs]
      .slice(0, 100);
    mergeLastData({ logs: mergedLogs });
    renderWhenChanged("logs", mergedLogs, () => renderLogs(mergedLogs));
  }

  scheduleControlDrivenRefresh();
}

function applyLiveStatus(status) {
  if (!state.lastData) {
    return;
  }
  mergeLastData({ status });
  renderBootstrapData({ ...state.lastData, status });
  scheduleControlDrivenRefresh();
}

function handleControlSocketMessage(message) {
  if (message.type === "connected") {
    state.controlSocketReady = true;
    scheduleRefresh();
    return;
  }

  if (message.type === "response") {
    const pending = state.controlSocketRequests.get(message.request_id);
    if (!pending) {
      return;
    }
    state.controlSocketRequests.delete(message.request_id);
    pending.resolve(message.response);
    return;
  }

  if (message.type === "error") {
    const pending = message.request_id
      ? state.controlSocketRequests.get(message.request_id)
      : null;
    if (pending && message.request_id) {
      state.controlSocketRequests.delete(message.request_id);
      pending.reject(
        new Error(message.error?.message || "Control request failed.")
      );
      return;
    }
    if (message.error?.message) {
      setStatus(`Control socket error: ${message.error.message}`, "warn");
    }
    return;
  }

  if (message.type !== "event" || !message.event) {
    return;
  }

  if (message.event.kind === "status") {
    applyLiveStatus(message.event.payload);
    return;
  }

  if (message.event.kind === "logs") {
    mergeLiveLogBatch(message.event.payload);
    return;
  }

  if (message.event.kind === "task_stream") {
    const stream = message.event.payload;
    const pending = stream?.request_id
      ? state.controlSocketRequests.get(stream.request_id)
      : null;
    if (pending?.onEvent) {
      pending.onEvent(stream.event);
    }
  }
}

async function ensureControlSocket() {
  if (!state.dashboardSessionAuthenticated) {
    throw new Error("Live control requires a dashboard session.");
  }
  if (state.controlSocketReady && state.controlSocket) {
    return state.controlSocket;
  }
  if (state.controlSocketPromise) {
    return state.controlSocketPromise;
  }

  state.controlSocketPromise = new Promise((resolve, reject) => {
    let settled = false;
    const socket = new WebSocket(controlSocketUrl());

    socket.addEventListener("open", () => {
      state.controlSocketClosing = false;
      state.controlSocket = socket;
      socket.send(JSON.stringify(controlConnectPayload()));
    });

    socket.addEventListener("message", (event) => {
      let parsed;
      try {
        parsed = JSON.parse(event.data);
      } catch (error) {
        setStatus(`Control socket parse failed: ${error.message}`, "warn");
        return;
      }
      if (parsed.type === "connected" && !settled) {
        settled = true;
        resolve(socket);
      }
      handleControlSocketMessage(parsed);
    });

    socket.addEventListener("error", () => {
      if (!settled) {
        settled = true;
        reject(new Error("Control socket connection failed."));
      }
    });

  socket.addEventListener("close", () => {
    const wasReady = state.controlSocketReady;
    const intentional = state.controlSocketClosing;
    const hadPendingRequests = state.controlSocketRequests.size > 0;
    state.controlSocketClosing = false;
    state.controlSocket = null;
    state.controlSocketReady = false;
    state.controlSocketPromise = null;
    rejectPendingControlRequests("Control socket disconnected.");
    scheduleRefresh();
    if (wasReady && hasDashboardAuth() && !intentional) {
      setStatus(
        hadPendingRequests
          ? "Live control connection lost during a live request. Retry manually to avoid duplicate work."
          : "Live control connection lost. Refreshing from HTTP.",
        "warn"
      );
    }
    if (!settled) {
      settled = true;
      reject(new Error("Control socket closed before it was ready."));
      }
    });
  });

  try {
    return await state.controlSocketPromise;
  } catch (error) {
    state.controlSocketPromise = null;
    throw error;
  }
}

async function controlRequest(request, { onEvent = null } = {}) {
  const socket = await ensureControlSocket();
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    throw new Error("Control socket is not connected.");
  }
  const requestId = `control-${Date.now()}-${++state.controlRequestCounter}`;
  return new Promise((resolve, reject) => {
    state.controlSocketRequests.set(requestId, { resolve, reject, onEvent });
    try {
      socket.send(
        JSON.stringify({
          type: "request",
          request_id: requestId,
          request,
        })
      );
    } catch (error) {
      state.controlSocketRequests.delete(requestId);
      error.code = "control_socket_send_failed";
      reject(error);
    }
  });
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
    actions.push(buttonHtml("Free thinking", { autonomy: "free_thinking" }, "button-ghost"));
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

async function setAutopilot(mode) {
  const current = state.lastData?.status?.autopilot?.state || "enabled";
  await apiPut("/v1/autopilot/status", { state: mode === "wake" ? current : mode });
  await refreshDashboard();
}

async function setAutonomy(mode) {
  await apiPost("/v1/autonomy/enable", { mode });
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

window.dashboardControl = {
  controlRequest,
  renderAutopilot,
  setAutopilot,
  setAutonomy,
  setEvolve,
};

export {
  closeControlSocket,
  controlRequest,
  ensureControlSocket,
  expectControlResponse,
  renderAutopilot,
  setAutonomy,
  setAutopilot,
  setEvolve,
};
