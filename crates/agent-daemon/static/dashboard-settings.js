(function () {
  const settingsState = {
    bound: false,
    bootstrap: null,
    cachedConfig: null,
    cachedText: "",
  };

  function app() {
    return window.dashboardApp || {};
  }

  function elements() {
    return {
      setupSummary: document.getElementById("setup-summary"),
      setupChecklist: document.getElementById("setup-checklist"),
      overviewCards: document.getElementById("settings-overview-cards"),
      advancedSummary: document.getElementById("advanced-config-summary"),
      editor: document.getElementById("advanced-config-editor"),
      load: document.getElementById("advanced-config-load"),
      format: document.getElementById("advanced-config-format"),
      save: document.getElementById("advanced-config-save"),
      reset: document.getElementById("advanced-config-reset"),
      coverage: document.getElementById("advanced-settings-coverage"),
    };
  }

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  function renderEmpty(message) {
    if (typeof app().renderEmpty === "function") {
      return app().renderEmpty(message);
    }
    return `<p class="panel__meta">${escapeHtml(message)}</p>`;
  }

  function badge(label, tone = "info") {
    return `<span class="badge" data-tone="${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
  }

  function actionButton(label, attrs, extraClass = "") {
    const className = ["button-small", extraClass].filter(Boolean).join(" ");
    const dataAttrs = Object.entries(attrs)
      .map(([key, value]) => {
        const attrName = String(key).replace(/[A-Z]/g, (char) => `-${char.toLowerCase()}`);
        return `data-${escapeHtml(attrName)}="${escapeHtml(value)}"`;
      })
      .join(" ");
    return `<button type="button" class="${className}" ${dataAttrs}>${escapeHtml(label)}</button>`;
  }

  function connectorTotal(data) {
    return [
      data.status?.telegram_connectors || 0,
      data.status?.discord_connectors || 0,
      data.status?.slack_connectors || 0,
      data.status?.signal_connectors || 0,
      data.status?.home_assistant_connectors || 0,
      data.status?.webhook_connectors || 0,
      data.status?.inbox_connectors || 0,
      data.status?.gmail_connectors || 0,
      data.status?.brave_connectors || 0,
    ].reduce((sum, value) => sum + value, 0);
  }

  function formatJson(value) {
    return JSON.stringify(value, null, 2);
  }

  function renderSetup(data) {
    const ui = elements();
    const totalConnectors = connectorTotal(data);
    const mainReady = !!data.status?.main_target;
    const providerReady = !!data.providers?.length;
    ui.setupSummary.textContent = mainReady
      ? `Main target: ${data.status.main_target.alias} -> ${data.status.main_target.provider_display_name}`
      : "No runnable main target configured yet";

    ui.setupChecklist.innerHTML = [
      {
        title: "Provider access",
        summary: providerReady
          ? `${data.providers.length} provider(s) configured`
          : "No providers configured yet. Start with Codex, OpenAI, Anthropic API key, or Ollama.",
        badges: [
          providerReady ? badge("configured", "good") : badge("needs setup", "warn"),
          badge(`${data.aliases.length} alias(es)`, "info"),
        ].join(""),
        actions: [
          actionButton("Connect Codex", { setupProvider: "codex" }, "button-muted"),
          actionButton("Connect Anthropic", { setupProvider: "anthropic" }, "button-muted"),
          actionButton("Open providers", { setupFocus: "providers" }, "button-small--ghost"),
        ].join(""),
      },
      {
        title: "Connectors",
        summary: totalConnectors
          ? `${totalConnectors} connector(s) live`
          : "No connectors configured yet. Use the guided workbench instead of the old blank-field form.",
        badges: [
          totalConnectors ? badge("live", "good") : badge("empty", "warn"),
          badge(`${data.status?.pending_connector_approvals || 0} pending approvals`, data.status?.pending_connector_approvals ? "warn" : "info"),
        ].join(""),
        actions: [
          actionButton("Telegram", { setupConnector: "telegram" }, "button-muted"),
          actionButton("Discord", { setupConnector: "discord" }, "button-muted"),
          actionButton("Open connectors", { setupFocus: "connectors" }, "button-small--ghost"),
        ].join(""),
      },
      {
        title: "Safety and control",
        summary: `Permission preset: ${data.permissions || "unknown"} | Autopilot: ${data.status?.autopilot?.state || "disabled"}`,
        badges: [
          badge(data.status?.autonomy?.state || "disabled", data.status?.autonomy?.state === "enabled" ? "warn" : "info"),
          badge(data.status?.autopilot?.state || "disabled", data.status?.autopilot?.state === "enabled" ? "good" : "info"),
        ].join(""),
        actions: [
          actionButton("Permissions", { setupFocus: "permissions" }, "button-muted"),
          actionButton("Autonomy", { setupFocus: "controls" }, "button-small--ghost"),
        ].join(""),
      },
      {
        title: "Everything else",
        summary: "Use the advanced editor for daemon token, embedding config, app connectors, onboarding flags, and other low-frequency settings.",
        badges: [
          badge("full config editor", "info"),
          badge(`${data.status?.plugins || 0} plugin(s)`, "info"),
        ].join(""),
        actions: [
          actionButton("Load full config", { setupConfigLoad: "1" }, "button-muted"),
          actionButton("Open advanced", { setupFocus: "advanced" }, "button-small--ghost"),
        ].join(""),
      },
    ]
      .map(
        (item) => `
          <article class="setup-check-card">
            <div class="card-title-row">
              <div>
                <h3>${escapeHtml(item.title)}</h3>
                <p>${escapeHtml(item.summary)}</p>
              </div>
              <div class="badge-row">${item.badges}</div>
            </div>
            <div class="setup-check-card__footer">${item.actions}</div>
          </article>
        `
      )
      .join("");

    ui.overviewCards.innerHTML = [
      ["Providers", data.providers?.length || 0, "saved endpoints and browser sign-ins"],
      ["Aliases", data.aliases?.length || 0, "named model targets"],
      ["Connectors", totalConnectors, "guided external integrations"],
      ["Plugins", data.status?.plugins || 0, "managed extensions"],
      ["Missions", data.status?.missions || 0, "queued and active jobs"],
      ["Memory", data.status?.memories || 0, "stored project context"],
    ]
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
  }

  function renderCoverage() {
    const ui = elements();
    const config = settingsState.cachedConfig;
    if (!config) {
      ui.coverage.innerHTML = renderEmpty("Load the full config to inspect advanced-only settings.");
      return;
    }
    ui.coverage.innerHTML = [
      {
        title: "Guided dashboard coverage",
        points: [
          `providers: ${config.providers.length}`,
          `aliases: ${config.aliases.length}`,
          `connectors: ${config.telegram_connectors.length + config.discord_connectors.length + config.slack_connectors.length + config.signal_connectors.length + config.home_assistant_connectors.length + config.webhook_connectors.length + config.inbox_connectors.length + config.gmail_connectors.length + config.brave_connectors.length}`,
          `plugins: ${config.plugins.length}`,
          `permissions, trust, delegation, autopilot, missions, chat, memory, logs`,
        ],
      },
      {
        title: "Advanced editor only",
        points: [
          `daemon host/port/token: ${config.daemon.host}:${config.daemon.port}`,
          `global thinking_level: ${config.thinking_level || "default"}`,
          `embedding enabled: ${config.embedding?.enabled ? "yes" : "no"}`,
          `app connectors: ${config.app_connectors.length}`,
          `MCP servers: ${config.mcp_servers.length}`,
          `onboarding_complete: ${config.onboarding_complete ? "true" : "false"}`,
        ],
      },
    ]
      .map(
        (card) => `
          <article class="stack-card coverage-card">
            <h3>${escapeHtml(card.title)}</h3>
            <ul>
              ${card.points.map((point) => `<li>${escapeHtml(point)}</li>`).join("")}
            </ul>
          </article>
        `
      )
      .join("");
  }

  async function loadConfig() {
    const config = await app().apiGet("/v1/config");
    settingsState.cachedConfig = config;
    settingsState.cachedText = formatJson(config);
    elements().editor.value = settingsState.cachedText;
    elements().advancedSummary.textContent = `Loaded full config | version ${config.version} | ${config.providers.length} provider(s) | ${config.plugins.length} plugin(s)`;
    renderCoverage();
    return config;
  }

  async function saveConfig() {
    const raw = elements().editor.value.trim();
    if (!raw) {
      throw new Error("Config editor is empty.");
    }
    const parsed = JSON.parse(raw);
    const saved = await app().apiPut("/v1/config", parsed);
    settingsState.cachedConfig = saved;
    settingsState.cachedText = formatJson(saved);
    elements().editor.value = settingsState.cachedText;
    elements().advancedSummary.textContent = `Saved full config at ${new Date().toLocaleTimeString()}`;
    renderCoverage();
    await app().refreshDashboard({ silent: true });
    app().setStatus("Full dashboard config saved.", "ok");
  }

  function render(data) {
    settingsState.bootstrap = data;
    renderSetup(data);
    if (!settingsState.cachedConfig) {
      loadConfig().catch(() => {
        renderCoverage();
      });
    } else {
      renderCoverage();
    }
  }

  function reset() {
    settingsState.bootstrap = null;
    settingsState.cachedConfig = null;
    settingsState.cachedText = "";
    const ui = elements();
    if (ui.setupSummary) {
      ui.setupSummary.textContent = "No setup data yet";
    }
    if (ui.setupChecklist) {
      ui.setupChecklist.innerHTML = "";
    }
    if (ui.overviewCards) {
      ui.overviewCards.innerHTML = "";
    }
    if (ui.advancedSummary) {
      ui.advancedSummary.textContent = "No advanced config loaded yet";
    }
    if (ui.editor) {
      ui.editor.value = "";
    }
    if (ui.coverage) {
      ui.coverage.innerHTML = renderEmpty("Load the full config to inspect advanced-only settings.");
    }
  }

  function bind() {
    if (settingsState.bound) {
      return;
    }
    settingsState.bound = true;
    const ui = elements();
    ui.load?.addEventListener("click", () => {
      loadConfig().catch((error) => app().setStatus(`Config load failed: ${error.message}`, "warn"));
    });
    ui.format?.addEventListener("click", () => {
      try {
        ui.editor.value = formatJson(JSON.parse(ui.editor.value || "{}"));
      } catch (error) {
        app().setStatus(`JSON format failed: ${error.message}`, "warn");
      }
    });
    ui.save?.addEventListener("click", () => {
      saveConfig().catch((error) => app().setStatus(`Config save failed: ${error.message}`, "warn"));
    });
    ui.reset?.addEventListener("click", () => {
      ui.editor.value = settingsState.cachedText || "";
    });
    document.body.addEventListener("click", (event) => {
      const target = event.target?.closest?.("[data-setup-provider],[data-setup-connector],[data-setup-focus],[data-setup-config-load]");
      if (!(target instanceof HTMLElement)) {
        return;
      }
      if (target.dataset.setupProvider && typeof app().quickStartProvider === "function") {
        app().quickStartProvider(target.dataset.setupProvider).catch((error) =>
          app().setStatus(`Provider quick start failed: ${error.message}`, "warn")
        );
        return;
      }
      if (target.dataset.setupConnector && window.dashboardConnectors) {
        window.dashboardConnectors.selectKind(target.dataset.setupConnector, { focus: true });
        return;
      }
      if (target.dataset.setupFocus && typeof app().focusSection === "function") {
        app().focusSection(target.dataset.setupFocus);
        return;
      }
      if (target.dataset.setupConfigLoad) {
        loadConfig()
          .then(() => app().focusSection?.("advanced"))
          .catch((error) => app().setStatus(`Config load failed: ${error.message}`, "warn"));
      }
    });
  }

  window.dashboardSettings = {
    bind,
    render,
    reset,
  };
})();
