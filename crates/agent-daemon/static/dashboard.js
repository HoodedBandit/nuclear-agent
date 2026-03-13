const state = {
  token: "",
  autoRefresh: true,
  refreshTimer: null,
  lastData: null,
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
};

function bearerHeaders() {
  return state.token ? { Authorization: `Bearer ${state.token}` } : {};
}

async function apiRequest(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      ...(options.headers || {}),
      ...bearerHeaders(),
    },
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}${text ? `: ${text}` : ""}`);
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

function buttonHtml(label, datasetEntries, klass = "") {
  const attrs = Object.entries(datasetEntries)
    .map(([key, value]) => `data-${escapeHtml(key)}="${escapeHtml(value)}"`)
    .join(" ");
  return `<button class="button-small ${klass}" ${attrs}>${escapeHtml(label)}</button>`;
}

function renderEmpty(message) {
  return `<div class="empty-state">${escapeHtml(message)}</div>`;
}

function renderStatus(status, health) {
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
        status.inbox_connectors,
      "telegram, discord, slack, signal, home assistant, webhook, inbox",
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

  elements.healthSummary.innerHTML = [
    heroChip(`Daemon ${health.daemon_running ? "running" : "down"}`),
    heroChip(`Keyring ${health.keyring_ok ? "ok" : "issue"}`),
    heroChip(`Config ${status.persistence_mode}`),
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
    buttonHtml(
      connector.enabled ? "Disable" : "Enable",
      { connectorToggle: `${kind}:${connector.id}` },
      connector.enabled ? "button-small--ghost" : ""
    ),
    pollable ? buttonHtml("Poll", { connectorPoll: `${kind}:${connector.id}` }, "button-muted") : "",
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
  inboxes
) {
  const cards = [
    ["Telegram", status.telegram_connectors],
    ["Discord", status.discord_connectors],
    ["Slack", status.slack_connectors],
    ["Signal", status.signal_connectors],
    ["Home Assistant", status.home_assistant_connectors],
    ["Webhooks", status.webhook_connectors],
    ["Inboxes", status.inbox_connectors],
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

async function refreshDashboard() {
  if (!state.token) {
    setStatus("Waiting for a daemon token.", "neutral");
    return;
  }
  setStatus("Refreshing dashboard...", "neutral");
  try {
    const [
      status,
      health,
      missions,
      approvals,
      memoryReview,
      profileMemories,
      skillDrafts,
      delegationTargets,
      events,
      telegrams,
      discords,
      slacks,
      signals,
      homeAssistants,
      webhooks,
      inboxes,
    ] = await Promise.all([
      apiGet("/v1/status"),
      apiGet("/v1/doctor"),
      apiGet("/v1/missions"),
      apiGet("/v1/connector-approvals?status=pending&limit=25"),
      apiGet("/v1/memory/review?limit=25"),
      apiGet("/v1/memory/profile"),
      apiGet("/v1/skills/drafts"),
      apiGet("/v1/delegation/targets"),
      apiGet("/v1/events?limit=40"),
      apiGet("/v1/telegram"),
      apiGet("/v1/discord"),
      apiGet("/v1/slack"),
      apiGet("/v1/signal"),
      apiGet("/v1/home-assistant"),
      apiGet("/v1/webhooks"),
      apiGet("/v1/inboxes"),
    ]);

    state.lastData = {
      status,
      health,
      missions,
      approvals,
      memoryReview,
      profileMemories,
      skillDrafts,
      delegationTargets,
      events,
      telegrams,
      discords,
      slacks,
      signals,
      homeAssistants,
      webhooks,
      inboxes,
    };

    renderStatus(status, health);
    renderAutopilot(status);
    renderMissions(missions);
    renderApprovals(approvals);
    renderMemory(memoryReview);
    renderSkills(skillDrafts);
    renderProfile(profileMemories);
    renderConnectors(
      status,
      telegrams,
      discords,
      slacks,
      signals,
      homeAssistants,
      webhooks,
      inboxes
    );
    renderDelegation(delegationTargets);
    renderEvents(events);
    elements.lastUpdated.textContent = `Updated ${new Date().toLocaleTimeString()}`;
    setStatus("Connected.", "ok");
  } catch (error) {
    setStatus(`Refresh failed: ${error.message}`, "warn");
  }
}

function scheduleRefresh() {
  if (state.refreshTimer) {
    clearInterval(state.refreshTimer);
  }
  if (state.autoRefresh) {
    state.refreshTimer = setInterval(() => {
      refreshDashboard().catch(() => {});
    }, 12000);
  }
}

function persistToken(token) {
  state.token = token.trim();
  if (state.token) {
    window.localStorage.setItem("agent-daemon-token", state.token);
  } else {
    window.localStorage.removeItem("agent-daemon-token");
  }
}

function bootstrapToken() {
  const params = new URLSearchParams(window.location.search);
  const tokenFromUrl = params.get("token");
  const savedToken = window.localStorage.getItem("agent-daemon-token");
  state.token = tokenFromUrl || savedToken || "";
  elements.tokenInput.value = state.token;
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
  if (!basePath || kind === "webhook") {
    throw new Error("This connector does not support polling.");
  }
  await apiPost(`${basePath}/${id}/poll`, {});
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
    const target = event.target;
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
      } else if (target.dataset.connectorPoll) {
        const [kind, id] = target.dataset.connectorPoll.split(":");
        await pollConnector(kind, id);
      }
    } catch (error) {
      setStatus(`Action failed: ${error.message}`, "warn");
    }
  });
}

elements.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  persistToken(elements.tokenInput.value);
  await refreshDashboard();
});

elements.refreshButton.addEventListener("click", async () => {
  persistToken(elements.tokenInput.value);
  await refreshDashboard();
});

elements.clearButton.addEventListener("click", () => {
  persistToken("");
  elements.tokenInput.value = "";
  elements.lastUpdated.textContent = "Not connected";
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

bootstrapToken();
bindActions();
scheduleRefresh();
if (state.token) {
  refreshDashboard().catch((error) => setStatus(`Refresh failed: ${error.message}`, "warn"));
} else {
  setStatus("Waiting for a daemon token.", "neutral");
}
