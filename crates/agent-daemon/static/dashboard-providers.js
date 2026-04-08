(function () {
  const providerState = {
    bound: false,
    currentData: null,
    selectedPreset: "codex",
    editingProviderId: null,
    authSessionId: null,
    authKind: null,
    authWindow: null,
    authPollTimer: null,
    authStatusMessage: "",
    authStatusTone: "neutral",
    autoDefaults: null,
  };

  const PROVIDER_PRESETS = {
    codex: {
      id: "codex",
      label: "ChatGPT Codex",
      name: "ChatGPT Codex",
      summary: "Use the built-in browser sign-in flow for ChatGPT Codex with the daemon's native Codex provider.",
      kind: "chat_gpt_codex",
      baseUrl: "https://chatgpt.com/backend-api/codex",
      authMode: "oauth",
      defaultModel: "gpt-5-codex",
      local: false,
      authStrategy: "browser",
      browserAuthKind: "codex",
      browserAuthLabel: "Codex",
      portalLabel: "Open ChatGPT",
      portalUrl: "https://chatgpt.com/",
      docsLabel: "OpenAI quickstart",
      docsUrl: "https://platform.openai.com/docs/quickstart",
      apiKeyPlaceholder: "",
      steps: [
        "Use browser sign-in so the daemon captures your Codex session without pasting secrets into the dashboard.",
        "Pick the default model and alias name you want the coding agent to use.",
        "Save the provider after sign-in finishes. The alias will become available to chat, missions, and autonomy.",
      ],
    },
    openai: {
      id: "openai",
      label: "OpenAI",
      name: "OpenAI",
      summary: "Standard OpenAI-compatible API setup with an API key and a recommended GPT model.",
      kind: "open_ai_compatible",
      baseUrl: "https://api.openai.com/v1",
      authMode: "api_key",
      defaultModel: "gpt-5",
      local: false,
      authStrategy: "api_key",
      portalLabel: "Open API keys",
      portalUrl: "https://platform.openai.com/api-keys",
      docsLabel: "OpenAI quickstart",
      docsUrl: "https://platform.openai.com/docs/quickstart",
      apiKeyPlaceholder: "sk-...",
      steps: [
        "Create an API key in the OpenAI dashboard.",
        "Paste the key here, keep the default base URL, and choose the model and alias you want.",
        "Save the provider, then use the alias everywhere else in the dashboard.",
      ],
    },
    anthropic: {
      id: "anthropic",
      label: "Claude / Anthropic",
      name: "Claude",
      summary: "Connect Anthropic with a manual API key. Browser sign-in is not supported for third-party use.",
      kind: "anthropic",
      baseUrl: "https://api.anthropic.com",
      authMode: "api_key",
      defaultModel: "claude-sonnet-4-20250514",
      local: false,
      authStrategy: "api_key",
      portalLabel: "Open Anthropic console",
      portalUrl: "https://console.anthropic.com/settings/keys",
      docsLabel: "Anthropic getting started",
      docsUrl: "https://docs.anthropic.com/en/docs/get-started",
      apiKeyPlaceholder: "sk-ant-...",
      steps: [
        "Create an Anthropic API key in the console.",
        "Paste the key here and choose the default model and alias you want.",
        "Choose the default model and alias to expose Claude across chat, missions, and delegation.",
        "Save the provider and optionally make the alias the main target if Claude should be your default.",
      ],
    },
    openrouter: {
      id: "openrouter",
      label: "OpenRouter",
      name: "OpenRouter",
      summary: "Use one API key to reach multiple upstream models through OpenRouter's OpenAI-compatible endpoint.",
      kind: "open_ai_compatible",
      baseUrl: "https://openrouter.ai/api/v1",
      authMode: "api_key",
      defaultModel: "openai/gpt-4.1",
      local: false,
      authStrategy: "api_key",
      portalLabel: "Open OpenRouter keys",
      portalUrl: "https://openrouter.ai/keys",
      docsLabel: "OpenRouter quickstart",
      docsUrl: "https://openrouter.ai/docs/",
      apiKeyPlaceholder: "sk-or-...",
      steps: [
        "Create an API key in OpenRouter.",
        "Paste the key, keep the OpenRouter base URL, and choose the default routed model you want.",
        "Save the provider and alias so the rest of the dashboard can target it directly.",
      ],
    },
    moonshot: {
      id: "moonshot",
      label: "Moonshot",
      name: "Moonshot",
      summary: "OpenAI-compatible Moonshot setup for Kimi models.",
      kind: "open_ai_compatible",
      baseUrl: "https://api.moonshot.ai/v1",
      authMode: "api_key",
      defaultModel: "kimi-k2",
      local: false,
      authStrategy: "api_key",
      portalLabel: "Open Moonshot platform",
      portalUrl: "https://platform.moonshot.ai/",
      docsLabel: "Moonshot platform",
      docsUrl: "https://platform.moonshot.ai/",
      apiKeyPlaceholder: "moonshot key",
      steps: [
        "Create an API key from the Moonshot platform.",
        "Paste it here, keep the Moonshot API base URL, and confirm the model and alias names you want.",
        "Save the provider to make the Kimi target available across the runtime.",
      ],
    },
    venice: {
      id: "venice",
      label: "Venice AI",
      name: "Venice AI",
      summary: "Connect the Venice OpenAI-compatible endpoint with a Venice API key.",
      kind: "open_ai_compatible",
      baseUrl: "https://api.venice.ai/api/v1",
      authMode: "api_key",
      defaultModel: "venice-large",
      local: false,
      authStrategy: "api_key",
      portalLabel: "Open Venice settings",
      portalUrl: "https://venice.ai/settings/api",
      docsLabel: "Venice quickstart",
      docsUrl: "https://docs.venice.ai/overview/getting-started",
      apiKeyPlaceholder: "VENICE_API_KEY",
      steps: [
        "Generate a Venice API key.",
        "Paste the key, keep the Venice API base URL, and set the default model and alias you want exposed.",
        "Save the provider and use the alias from chat, missions, or autonomy.",
      ],
    },
    ollama: {
      id: "ollama-local",
      label: "Ollama",
      name: "Ollama",
      summary: "Local provider for models served by Ollama on your machine or LAN.",
      kind: "ollama",
      baseUrl: "http://127.0.0.1:11434",
      authMode: "none",
      defaultModel: "",
      local: true,
      authStrategy: "none",
      portalLabel: "Download Ollama",
      portalUrl: "https://ollama.com/download",
      docsLabel: "Ollama API intro",
      docsUrl: "https://docs.ollama.com/api/introduction",
      apiKeyPlaceholder: "",
      steps: [
        "Install Ollama and make sure the daemon can reach the base URL.",
        "Choose the default model you want to expose and name the alias for chat and mission routing.",
        "Save the provider, then discover models if you want to validate the endpoint.",
      ],
    },
    custom: {
      id: "",
      label: "Custom",
      name: "",
      summary: "Manual provider setup for non-standard OpenAI-compatible, Anthropic, or local endpoints.",
      kind: "open_ai_compatible",
      baseUrl: "",
      authMode: "api_key",
      defaultModel: "",
      local: false,
      authStrategy: "manual",
      portalLabel: "No single portal",
      portalUrl: null,
      docsLabel: "No single doc set",
      docsUrl: null,
      apiKeyPlaceholder: "optional",
      steps: [
        "Choose the provider kind, auth mode, and base URL in the advanced drawer.",
        "Paste any required secret or OAuth material, then set the default model and alias.",
        "Save the provider and validate model discovery before using it in missions or autonomy.",
      ],
    },
  };

  function app() {
    return window.dashboardApp || {};
  }

  function elements() {
    return {
      summary: document.getElementById("providers-summary"),
      overviewCards: document.getElementById("provider-overview-cards"),
      presetGrid: document.getElementById("provider-preset-grid"),
      workbenchTitle: document.getElementById("provider-workbench-title"),
      workbenchStatus: document.getElementById("provider-workbench-status"),
      workbenchIntro: document.getElementById("provider-workbench-intro"),
      workbenchLinks: document.getElementById("provider-workbench-links"),
      workbenchSteps: document.getElementById("provider-workbench-steps"),
      formMode: document.getElementById("provider-form-mode"),
      form: document.getElementById("provider-form"),
      preset: document.getElementById("provider-preset"),
      id: document.getElementById("provider-id"),
      name: document.getElementById("provider-name"),
      kind: document.getElementById("provider-kind"),
      baseUrl: document.getElementById("provider-base-url"),
      authMode: document.getElementById("provider-auth-mode"),
      defaultModel: document.getElementById("provider-default-model"),
      local: document.getElementById("provider-local"),
      apiKeyRow: document.getElementById("provider-api-key-row"),
      apiKey: document.getElementById("provider-api-key"),
      browserAuthToolbar: document.getElementById("provider-browser-auth-toolbar"),
      browserAuth: document.getElementById("provider-browser-auth"),
      browserAuthStatus: document.getElementById("provider-browser-auth-status"),
      aliasName: document.getElementById("provider-alias-name"),
      aliasModel: document.getElementById("provider-alias-model"),
      aliasDescription: document.getElementById("provider-alias-description"),
      setMainRow: document.getElementById("provider-set-main-row"),
      setMain: document.getElementById("provider-set-main"),
      advancedDetails: document.getElementById("provider-advanced-details"),
      kindRow: document.getElementById("provider-kind-row"),
      authModeRow: document.getElementById("provider-auth-mode-row"),
      localRow: document.getElementById("provider-local-row"),
      oauthConfigRow: document.getElementById("provider-oauth-config-row"),
      oauthConfig: document.getElementById("provider-oauth-config"),
      oauthTokenRow: document.getElementById("provider-oauth-token-row"),
      oauthToken: document.getElementById("provider-oauth-token"),
      openPortal: document.getElementById("provider-open-portal"),
      openDocs: document.getElementById("provider-open-docs"),
      discoverModels: document.getElementById("provider-discover-models"),
      reset: document.getElementById("provider-reset"),
      modelResults: document.getElementById("provider-model-results"),
      providersList: document.getElementById("providers-list"),
      aliasesList: document.getElementById("aliases-list"),
      aliasForm: document.getElementById("alias-form"),
      aliasQuickName: document.getElementById("alias-name"),
      aliasQuickProvider: document.getElementById("alias-provider"),
      aliasQuickModel: document.getElementById("alias-model"),
      aliasQuickDescription: document.getElementById("alias-description"),
      aliasQuickMain: document.getElementById("alias-main"),
      runTaskAlias: document.getElementById("run-task-alias"),
      chatMainTarget: document.getElementById("chat-main-target"),
    };
  }

  function normalizeData(data) {
    return {
      providers: data?.providers || [],
      aliases: data?.aliases || [],
      status: data?.status || {},
    };
  }

  function currentData() {
    return normalizeData(providerState.currentData);
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
    return `<div class="empty-state">${escapeHtml(message)}</div>`;
  }

  function badge(label, tone = "info") {
    return `<span class="badge" data-tone="${escapeHtml(tone)}">${escapeHtml(label)}</span>`;
  }

  function buttonHtml(label, action, payload, extraClass = "") {
    const className = ["button-small", extraClass].filter(Boolean).join(" ");
    const dataAttrs = Object.entries(payload)
      .map(([key, value]) => `data-${escapeHtml(key)}="${escapeHtml(value)}"`)
      .join(" ");
    return `<button type="button" class="${className}" data-provider-action="${escapeHtml(action)}" ${dataAttrs}>${escapeHtml(label)}</button>`;
  }

  function aliasButtonHtml(label, action, payload, extraClass = "") {
    const className = ["button-small", extraClass].filter(Boolean).join(" ");
    const dataAttrs = Object.entries(payload)
      .map(([key, value]) => `data-${escapeHtml(key)}="${escapeHtml(value)}"`)
      .join(" ");
    return `<button type="button" class="${className}" data-alias-action="${escapeHtml(action)}" ${dataAttrs}>${escapeHtml(label)}</button>`;
  }

  function fmt(value) {
    if (value === null || value === undefined || value === "") {
      return "-";
    }
    return String(value);
  }

  function fmtBoolean(value) {
    return value ? "yes" : "no";
  }

  function trimToNull(value) {
    const trimmed = String(value ?? "").trim();
    return trimmed ? trimmed : null;
  }

  function openExternal(url) {
    if (url) {
      window.open(url, "_blank", "noopener");
    }
  }

  function selectedPresetMeta() {
    const presetKey = elements().preset?.value || providerState.selectedPreset;
    return PROVIDER_PRESETS[presetKey] || PROVIDER_PRESETS.custom;
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
    const ui = elements();
    return trimToNull(ui.defaultModel.value) || suggestedProviderDefaultModel(ui.kind.value, ui.baseUrl.value);
  }

  function applySuggestedProviderModelDefaults() {
    const ui = elements();
    const suggestedModel = resolveProviderDefaultModel();
    if (suggestedModel && !trimToNull(ui.defaultModel.value)) {
      ui.defaultModel.value = suggestedModel;
    }
    if (suggestedModel && trimToNull(ui.aliasName.value) && !trimToNull(ui.aliasModel.value)) {
      ui.aliasModel.value = suggestedModel;
    }
    return suggestedModel;
  }

  function isCreateMode() {
    return !providerState.editingProviderId;
  }

  function providerSuggestionRequest() {
    const ui = elements();
    const preset = selectedPresetMeta();
    return {
      preferred_provider_id: trimToNull(ui.id.value) || preset.id || "provider",
      preferred_alias_name: trimToNull(ui.aliasName.value),
      default_model: resolveProviderDefaultModel(),
      editing_provider_id: providerState.editingProviderId || null,
      editing_alias_name: providerState.editingProviderId ? trimToNull(ui.aliasName.value) : null,
    };
  }

  function applyProviderSuggestions(suggestions) {
    const ui = elements();
    const previous = providerState.autoDefaults;
    providerState.autoDefaults = suggestions;
    if (!isCreateMode()) {
      return;
    }

    const currentProviderId = trimToNull(ui.id.value);
    if (!currentProviderId || (previous && currentProviderId === previous.provider_id)) {
      ui.id.value = suggestions.provider_id || "";
    }

    const currentAliasName = trimToNull(ui.aliasName.value);
    const previousAliasName = previous?.alias_name || null;
    if (!currentAliasName || currentAliasName === previousAliasName) {
      ui.aliasName.value = suggestions.alias_name || "";
    }

    const currentAliasModel = trimToNull(ui.aliasModel.value);
    const previousAliasModel = previous?.alias_model || null;
    if (!currentAliasModel || currentAliasModel === previousAliasModel) {
      ui.aliasModel.value = suggestions.alias_model || "";
    }
  }

  async function refreshProviderCreateSuggestions() {
    if (!isCreateMode()) {
      providerState.autoDefaults = null;
      syncProviderModeUi();
      return null;
    }
    if (!app().hasDashboardAuth?.()) {
      providerState.autoDefaults = null;
      syncProviderModeUi();
      return null;
    }
    const suggestions = await app().apiPost("/v1/providers/suggest", providerSuggestionRequest());
    applyProviderSuggestions(suggestions);
    syncProviderModeUi();
    return suggestions;
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

  function refreshAliasProviderOptions() {
    const ui = elements();
    const providers = currentData().providers;
    const selectedValue = ui.aliasQuickProvider.value;
    ui.aliasQuickProvider.innerHTML = providers.length
      ? providers
          .map((provider) => `<option value="${escapeHtml(provider.id)}">${escapeHtml(provider.display_name || provider.id)}</option>`)
          .join("")
      : '<option value="">No providers saved yet</option>';
    if (selectedValue && providers.some((provider) => provider.id === selectedValue)) {
      ui.aliasQuickProvider.value = selectedValue;
    }
  }

  function renderChatAliasOptions(providers, aliases) {
    const ui = elements();
    const providerNames = new Map(
      (providers || []).map((provider) => [provider.id, provider.display_name || provider.id])
    );
    const grouped = new Map();
    (aliases || []).forEach((alias) => {
      const providerId = alias.provider_id || "unknown";
      if (!grouped.has(providerId)) {
        grouped.set(providerId, []);
      }
      grouped.get(providerId).push(alias);
    });
    const currentValue = ui.runTaskAlias.value;
    const options = [];
    Array.from(grouped.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .forEach(([providerId, providerAliases]) => {
        const providerLabel = providerNames.get(providerId) || providerId;
        options.push(`<optgroup label="${escapeHtml(providerLabel)}">`);
        providerAliases
          .sort((left, right) => String(left.alias || "").localeCompare(String(right.alias || "")))
          .forEach((alias) => {
            options.push(
              `<option value="${escapeHtml(alias.alias)}">${escapeHtml(
                `${alias.alias} · ${alias.model || providerId}`
              )}</option>`
            );
          });
        options.push("</optgroup>");
      });
    ui.runTaskAlias.innerHTML = options.join("");
    if (currentValue && (aliases || []).some((alias) => alias.alias === currentValue)) {
      ui.runTaskAlias.value = currentValue;
    } else if (currentData().status?.main_agent_alias) {
      ui.runTaskAlias.value = currentData().status.main_agent_alias;
    }
  }

  function renderMainTargetSummary(mainTarget, mainAlias) {
    const ui = elements();
    if (mainTarget) {
      ui.chatMainTarget.textContent = `Default main alias: ${mainTarget.alias} -> ${mainTarget.provider_display_name} / ${mainTarget.model}`;
    } else if (mainAlias) {
      ui.chatMainTarget.textContent = `Default main alias: ${mainAlias}`;
    } else {
      ui.chatMainTarget.textContent = "Default main alias: not configured.";
    }
  }

  function syncProviderModeUi() {
    const ui = elements();
    const createMode = isCreateMode();
    ui.formMode.textContent = createMode
      ? "Recommended path: pick a provider preset, sign in or paste one secret, then save one alias."
      : `Editing provider ${providerState.editingProviderId}`;
    ui.reset.textContent = createMode ? "Reset" : "Create new";
    ui.id.readOnly = !createMode;
    if (createMode && !trimToNull(ui.aliasName.value)) {
      ui.setMain.checked = !currentData().status?.main_agent_alias;
    }
  }

  function currentProviderBrowserAuthDescriptor() {
    const ui = elements();
    const preset = selectedPresetMeta();
    if (!providerState.editingProviderId && preset.browserAuthKind) {
      return {
        kind: preset.browserAuthKind,
        label: preset.browserAuthLabel || providerBrowserAuthLabel(preset.browserAuthKind),
      };
    }
    if (ui.kind.value === "chat_gpt_codex") {
      return { kind: "codex", label: "Codex" };
    }
    return null;
  }

  function syncProviderFieldVisibility() {
    const ui = elements();
    const preset = selectedPresetMeta();
    const descriptor = currentProviderBrowserAuthDescriptor();
    const authMode = ui.authMode.value;
    const showApiKey =
      preset.authStrategy === "api_key" ||
      (preset.authStrategy === "browser_or_api_key" && authMode === "api_key") ||
      (preset.authStrategy === "manual" && authMode === "api_key");
    const showOauthFields = authMode === "oauth" && !descriptor;
    ui.apiKeyRow.hidden = !showApiKey;
    ui.browserAuthToolbar.hidden = !descriptor;
    ui.oauthConfigRow.hidden = !showOauthFields;
    ui.oauthTokenRow.hidden = !showOauthFields;
    const apiKeyLabel = ui.apiKeyRow.querySelector("span");
    if (apiKeyLabel) {
      apiKeyLabel.textContent =
        preset.authStrategy === "browser_or_api_key" ? "API key (manual fallback)" : "API key / secret";
    }
    ui.apiKey.placeholder = preset.apiKeyPlaceholder || "Optional for api_key mode";
    ui.openPortal.disabled = !preset.portalUrl;
    ui.openPortal.textContent = preset.portalLabel || "Open setup portal";
    ui.openDocs.disabled = !preset.docsUrl;
    ui.openDocs.textContent = preset.docsLabel || "Open docs";
    const canDiscover = currentData().providers.some((provider) => provider.id === trimToNull(ui.id.value));
    ui.discoverModels.disabled = !canDiscover;
  }

  function renderProviderBrowserAuthState() {
    const ui = elements();
    const descriptor = currentProviderBrowserAuthDescriptor();
    const activeKind = providerState.authKind || descriptor?.kind || null;
    const activeLabel = activeKind ? providerBrowserAuthLabel(activeKind) : "browser";
    const isPending = !!providerState.authSessionId;
    ui.browserAuth.textContent = isPending
      ? `Waiting for ${activeLabel}...`
      : descriptor
        ? `Sign in with ${descriptor.label}`
        : "Sign in with browser";
    ui.browserAuth.disabled = !app().hasDashboardAuth?.() || isPending || !descriptor;
    const fallbackMessage = descriptor
      ? `${descriptor.label} browser sign-in will save credentials directly into this provider.`
      : "Browser sign-in is available for ChatGPT Codex only.";
    ui.browserAuthStatus.textContent = providerState.authStatusMessage || fallbackMessage;
    ui.browserAuthStatus.dataset.tone = providerState.authStatusTone || "neutral";
    ui.browserAuthToolbar.hidden = !descriptor;
  }

  function setProviderBrowserAuthStatus(message, tone = "neutral") {
    providerState.authStatusMessage = message;
    providerState.authStatusTone = tone;
    renderProviderBrowserAuthState();
  }

  function clearProviderBrowserAuthPolling() {
    if (providerState.authPollTimer) {
      clearInterval(providerState.authPollTimer);
      providerState.authPollTimer = null;
    }
  }

  function clearProviderSecretInputs() {
    const ui = elements();
    ui.apiKey.value = "";
    ui.oauthToken.value = "";
  }

  async function pollProviderBrowserAuthSession(sessionId, { refresh = true } = {}) {
    const session = await app().apiGet(`/v1/provider-auth/${encodeURIComponent(sessionId)}`);
    if (providerState.authSessionId && providerState.authSessionId !== sessionId) {
      return session;
    }
    if (session.status === "pending") {
      const label = providerBrowserAuthLabel(session.kind);
      setProviderBrowserAuthStatus(`Continue the ${label} sign-in flow in the popup window.`, "neutral");
      return session;
    }

    clearProviderBrowserAuthPolling();
    providerState.authSessionId = null;
    providerState.authKind = null;
    providerState.authWindow = null;
    if (session.status === "completed") {
      clearProviderSecretInputs();
      providerState.editingProviderId = session.provider_id;
      setProviderBrowserAuthStatus(
        `${providerBrowserAuthLabel(session.kind)} credentials saved for ${session.provider_id}.`,
        "ok"
      );
      if (refresh) {
        await app().refreshDashboard({ includeLoadedPanels: false, silent: true });
        render(app().getLastData());
        populateProviderForm(session.provider_id);
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
    providerState.authPollTimer = window.setInterval(() => {
      pollProviderBrowserAuthSession(sessionId, { refresh: true }).catch((error) => {
        setProviderBrowserAuthStatus(`Browser sign-in check failed: ${error.message}`, "warn");
        clearProviderBrowserAuthPolling();
        providerState.authSessionId = null;
        providerState.authKind = null;
        providerState.authWindow = null;
        renderProviderBrowserAuthState();
      });
    }, 1500);
  }

  async function resolveProviderFormSubmission() {
    const ui = elements();
    const createMode = isCreateMode();
    const requestedProviderId = trimToNull(ui.id.value);
    const requestedAliasName = trimToNull(ui.aliasName.value);
    let defaultModel = applySuggestedProviderModelDefaults();
    let providerId = requestedProviderId;
    let aliasName = requestedAliasName;
    let aliasModel = trimToNull(ui.aliasModel.value) || (aliasName ? defaultModel : null);
    const displayName = trimToNull(ui.name.value);

    if (createMode) {
      const suggestions = await app().apiPost("/v1/providers/suggest", providerSuggestionRequest());
      applyProviderSuggestions(suggestions);
      providerId = suggestions.provider_id;
      aliasName = suggestions.alias_name;
      aliasModel = suggestions.alias_model;
      defaultModel = suggestions.alias_model || defaultModel;

      const providerAdjusted = requestedProviderId !== providerId;
      const aliasAdjusted = (requestedAliasName || null) !== aliasName;
      if (providerAdjusted || aliasAdjusted) {
        app().setStatus(`Using safe multi-provider defaults for ${displayName || providerId}.`, "neutral");
      }
    }

    if (!providerId || !displayName) {
      throw new Error("Provider ID and display name are required.");
    }
    if (aliasName && !aliasModel) {
      throw new Error("Set a default model or alias model before creating an alias.");
    }

    return {
      providerId,
      displayName,
      defaultModel,
      aliasName,
      aliasModel,
      setAsMain: aliasName ? !!ui.setMain.checked : false,
    };
  }

  async function startProviderBrowserAuth() {
    const descriptor = currentProviderBrowserAuthDescriptor();
    const ui = elements();
    if (!descriptor) {
      throw new Error("Browser sign-in is only available for ChatGPT Codex.");
    }
    const submission = await resolveProviderFormSubmission();
    const { providerId, displayName, defaultModel, aliasName, aliasModel, setAsMain } = submission;

    const popup = window.open("", `provider-auth-${Date.now()}`, "popup=yes,width=720,height=840");
    if (!popup) {
      throw new Error("The browser blocked the sign-in popup.");
    }
    popup.document.title = `Starting ${descriptor.label} sign-in`;
    popup.document.body.innerHTML =
      "<main><h1>Starting sign-in...</h1><p>The daemon is preparing the provider authorization flow.</p></main>";

    setProviderBrowserAuthStatus(`Starting ${descriptor.label} browser sign-in...`, "neutral");
    try {
      const response = await app().apiPost("/v1/provider-auth/start", {
        kind: descriptor.kind,
        provider_id: providerId,
        display_name: displayName,
        default_model: defaultModel,
        alias_name: aliasName,
        alias_model: aliasModel,
        alias_description: trimToNull(ui.aliasDescription.value),
        set_as_main: setAsMain,
      });
      if (response.status === "completed") {
        popup.close();
        providerState.authSessionId = null;
        providerState.authKind = null;
        providerState.authWindow = null;
        setProviderBrowserAuthStatus(`${descriptor.label} credentials saved for ${providerId}.`, "ok");
        await app().refreshDashboard({ includeLoadedPanels: false, silent: true });
        render(app().getLastData());
        populateProviderForm(providerId);
        return;
      }
      if (!response.authorization_url) {
        popup.close();
        throw new Error("The daemon did not return an authorization URL.");
      }
      providerState.authSessionId = response.session_id;
      providerState.authKind = descriptor.kind;
      providerState.authWindow = popup;
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
      providerState.authSessionId = null;
      providerState.authKind = null;
      providerState.authWindow = null;
      setProviderBrowserAuthStatus(`Browser sign-in failed: ${error.message}`, "warn");
      throw error;
    }
  }

  function renderWorkbench() {
    const ui = elements();
    const preset = selectedPresetMeta();
    const editing = providerState.editingProviderId
      ? currentData().providers.find((provider) => provider.id === providerState.editingProviderId) || null
      : null;
    const setupMode =
      preset.authStrategy === "browser"
        ? "Browser sign-in"
        : preset.authStrategy === "browser_or_api_key"
          ? "Browser or API key"
          : preset.authStrategy === "api_key"
            ? "API key"
            : preset.authStrategy === "none"
              ? "No secret"
              : "Manual";
    ui.workbenchTitle.textContent = editing ? `Edit ${preset.label} provider` : `New ${preset.label} provider`;
    ui.workbenchStatus.textContent = editing
      ? `Editing ${editing.display_name || editing.id}. Existing credentials stay in place unless you replace them.`
      : `Recommended setup: ${setupMode}. Start with the portal or docs if you still need credentials.`;
    ui.workbenchIntro.textContent = preset.summary;
    ui.workbenchSteps.innerHTML = preset.steps.map((step) => `<li>${escapeHtml(step)}</li>`).join("");
    ui.workbenchLinks.innerHTML = [
      preset.portalLabel ? `<span class="table-sub">${escapeHtml(preset.portalLabel)}</span>` : "",
      preset.docsLabel ? `<span class="table-sub">${escapeHtml(preset.docsLabel)}</span>` : "",
    ]
      .filter(Boolean)
      .join("");
    syncProviderFieldVisibility();
    renderProviderBrowserAuthState();
  }

  function renderPresetCatalog() {
    const presetKey = elements().preset.value || providerState.selectedPreset;
    elements().presetGrid.innerHTML = Object.entries(PROVIDER_PRESETS)
      .map(([key, preset]) => {
        const activeClass = presetKey === key ? " provider-preset-card--active" : "";
        const badges = [
          badge(
            preset.authStrategy === "browser"
              ? "browser sign-in"
              : preset.authStrategy === "browser_or_api_key"
                ? "browser or api key"
                : preset.authStrategy === "api_key"
                  ? "api key"
                  : preset.authStrategy === "none"
                    ? "no secret"
                    : "manual",
            "info"
          ),
          preset.local ? badge("local", "good") : badge("remote", "info"),
        ].join("");
        return `
          <article class="provider-preset-card${activeClass}" data-provider-select="${escapeHtml(key)}" tabindex="0" role="button">
            <div>
              <h3>${escapeHtml(preset.label)}</h3>
              <p>${escapeHtml(preset.summary)}</p>
            </div>
            <div class="badge-row">${badges}</div>
            <div class="provider-preset-card__footer">
              <span class="table-sub">${escapeHtml(preset.portalLabel || "manual setup")}</span>
            </div>
          </article>
        `;
      })
      .join("");
  }

  function renderOverviewCards() {
    const data = currentData();
    const providers = data.providers;
    const aliases = data.aliases;
    const localProviders = providers.filter((provider) => provider.local).length;
    const remoteProviders = providers.length - localProviders;
    const browserCapable = providers.filter((provider) => {
      const presetKey = providerPresetKeyFor(provider);
      const preset = PROVIDER_PRESETS[presetKey] || PROVIDER_PRESETS.custom;
      return !!preset.browserAuthKind;
    }).length;
    const mainTarget = data.status?.main_target;
    elements().overviewCards.innerHTML = [
      ["Providers", providers.length, "saved runtimes and endpoints"],
      ["Aliases", aliases.length, "named routing targets"],
      ["Local", localProviders, "Ollama and other local runtimes"],
      ["Remote", remoteProviders, "cloud provider connections"],
      ["Browser auth", browserCapable, "Codex-capable providers"],
      ["Main target", mainTarget ? mainTarget.alias : "none", mainTarget ? mainTarget.provider_display_name : "not configured"],
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

  function renderSummary() {
    const data = currentData();
    const providers = data.providers;
    const aliases = data.aliases;
    const mainTarget = data.status?.main_target || null;
    elements().summary.textContent = mainTarget
      ? `${providers.length} provider(s), ${aliases.length} alias(es) | main ${mainTarget.alias} -> ${mainTarget.provider_display_name} / ${mainTarget.model}`
      : `${providers.length} provider(s), ${aliases.length} alias(es)`;
  }

  function providerActionCard(provider, aliases, mainAlias) {
    const presetKey = providerPresetKeyFor(provider);
    const preset = PROVIDER_PRESETS[presetKey] || PROVIDER_PRESETS.custom;
    const providerAliases = aliases.filter((alias) => alias.provider_id === provider.id);
    const actions = [
      buttonHtml("Edit", "edit", { providerId: provider.id }, "button-small--ghost"),
      buttonHtml("Models", "models", { providerId: provider.id }, "button-muted"),
      buttonHtml("Clear creds", "clear-credentials", { providerId: provider.id }, "button-small--ghost"),
      buttonHtml("Delete", "delete", { providerId: provider.id }, "button-small--ghost"),
    ].join("");
    return `
      <article class="stack-card">
        <div class="card-title-row">
          <div>
            <h4>${escapeHtml(provider.display_name || provider.id)}</h4>
            <p class="card-subtitle">${escapeHtml(provider.id)} | ${escapeHtml(preset.label)}</p>
          </div>
          <div class="badge-row">
            ${badge(provider.auth_mode || "unknown", "info")}
            ${provider.local ? badge("local", "good") : badge("remote", "info")}
            ${providerAliases.some((alias) => alias.alias === mainAlias) ? badge("main target", "good") : ""}
          </div>
        </div>
        <ul class="micro-list">
          <li>base_url: ${escapeHtml(fmt(provider.base_url))}</li>
          <li>default_model: ${escapeHtml(fmt(provider.default_model))}</li>
          <li>aliases: ${escapeHtml(providerAliases.map((alias) => alias.alias).join(", ") || "-")}</li>
          <li>local: ${escapeHtml(fmtBoolean(provider.local))}</li>
        </ul>
        <div class="inline-actions">${actions}</div>
      </article>
    `;
  }

  function renderProvidersList() {
    const data = currentData();
    const mainAlias = data.status?.main_agent_alias || null;
    elements().providersList.innerHTML = data.providers.length
      ? data.providers.map((provider) => providerActionCard(provider, data.aliases, mainAlias)).join("")
      : renderEmpty("No providers configured.");
  }

  function renderAliasesList() {
    const data = currentData();
    const mainAlias = data.status?.main_agent_alias || null;
    const currentChatAlias = elements().runTaskAlias.value;
    elements().aliasesList.innerHTML = data.aliases.length
      ? data.aliases
          .map(
            (alias) => `
              <article class="stack-card">
                <div class="card-title-row">
                  <div>
                    <h4>${escapeHtml(alias.alias || alias.name || alias.id)}</h4>
                    <p class="card-subtitle">${escapeHtml(alias.provider_id || "-")} / ${escapeHtml(alias.model || "-")}</p>
                  </div>
                  <div class="badge-row">
                    ${mainAlias === alias.alias ? badge("main", "good") : ""}
                    ${currentChatAlias === alias.alias ? badge("current chat", "info") : ""}
                  </div>
                </div>
                ${alias.description ? `<p class="card-copy">${escapeHtml(alias.description)}</p>` : ""}
                <div class="inline-actions">
                  ${aliasButtonHtml("Edit", "edit", { aliasName: alias.alias }, "button-small--ghost")}
                  ${mainAlias === alias.alias ? "" : aliasButtonHtml("Set main", "make-main", { aliasName: alias.alias }, "button-muted")}
                  ${aliasButtonHtml("Delete", "delete", { aliasName: alias.alias }, "button-small--ghost")}
                </div>
              </article>
            `
          )
          .join("")
      : renderEmpty("No aliases configured.");
  }

  function renderModelResults(models) {
    const ui = elements();
    ui.modelResults.innerHTML = Array.isArray(models) && models.length
      ? models
          .map(
            (model) => `
              <article class="stack-card">
                <div class="card-title-row">
                  <div><h4>${escapeHtml(model)}</h4></div>
                  <button type="button" class="button-small button-ghost" data-provider-use-model="${escapeHtml(model)}">Use</button>
                </div>
              </article>
            `
          )
          .join("")
      : renderEmpty("No models reported by the provider.");
  }

  function render(data) {
    providerState.currentData = normalizeData(data || providerState.currentData);
    const normalized = currentData();
    renderSummary();
    renderOverviewCards();
    renderPresetCatalog();
    renderWorkbench();
    renderProvidersList();
    renderAliasesList();
    renderChatAliasOptions(normalized.providers, normalized.aliases);
    renderMainTargetSummary(normalized.status?.main_target || null, normalized.status?.main_agent_alias || null);
    refreshAliasProviderOptions();
  }

  function resetProviderForm(applyPreset = true) {
    const ui = elements();
    providerState.editingProviderId = null;
    providerState.autoDefaults = null;
    ui.form.reset();
    clearProviderSecretInputs();
    if (!providerState.authSessionId) {
      providerState.authStatusMessage = "";
      providerState.authStatusTone = "neutral";
    }
    if (applyPreset) {
      ui.preset.value = providerState.selectedPreset || ui.preset.value || "codex";
      applyProviderPreset(ui.preset.value);
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    }
    ui.modelResults.innerHTML = "";
    renderPresetCatalog();
    renderWorkbench();
  }

  function applyProviderPreset(presetKey) {
    const ui = elements();
    const preset = PROVIDER_PRESETS[presetKey] || PROVIDER_PRESETS.custom;
    const previous = providerState.autoDefaults;
    providerState.selectedPreset = presetKey;
    ui.preset.value = presetKey;
    const currentProviderId = trimToNull(ui.id.value);
    if (!currentProviderId || (previous && currentProviderId === previous.provider_id)) {
      ui.id.value = preset.id;
    }
    ui.name.value = preset.name;
    ui.kind.value = preset.kind;
    ui.baseUrl.value = preset.baseUrl;
    ui.authMode.value = preset.authMode;
    ui.local.checked = preset.local;
    if (!providerState.editingProviderId) {
      ui.defaultModel.value = preset.defaultModel || "";
      const currentAliasName = trimToNull(ui.aliasName.value);
      const previousAliasName = previous?.alias_name || null;
      if (!currentAliasName || currentAliasName === previousAliasName) {
        ui.aliasName.value = "";
      }
      const currentAliasModel = trimToNull(ui.aliasModel.value);
      const previousAliasModel = previous?.alias_model || null;
      if (!currentAliasModel || currentAliasModel === previousAliasModel) {
        ui.aliasModel.value = preset.defaultModel || "";
      }
      ui.aliasDescription.value = "";
      ui.setMain.checked = !currentData().status?.main_agent_alias;
    }
    if (!providerState.authSessionId) {
      providerState.authStatusMessage = "";
      providerState.authStatusTone = "neutral";
    }
    applySuggestedProviderModelDefaults();
    syncProviderModeUi();
    renderPresetCatalog();
    renderWorkbench();
  }

  function populateProviderForm(providerId) {
    const ui = elements();
    const provider = currentData().providers.find((entry) => entry.id === providerId);
    if (!provider) {
      throw new Error(`Unknown provider '${providerId}'.`);
    }
    providerState.editingProviderId = providerId;
    providerState.autoDefaults = null;
    providerState.selectedPreset = providerPresetKeyFor(provider);
    ui.preset.value = providerState.selectedPreset;
    ui.id.value = provider.id;
    ui.name.value = provider.display_name || provider.id;
    ui.kind.value = provider.kind;
    ui.baseUrl.value = provider.base_url || "";
    ui.authMode.value = provider.auth_mode || "api_key";
    ui.defaultModel.value = provider.default_model || "";
    ui.local.checked = !!provider.local;
    clearProviderSecretInputs();
    ui.oauthConfig.value = provider.oauth ? JSON.stringify(provider.oauth, null, 2) : "";
    ui.oauthToken.value = "";
    const aliases = currentData().aliases.filter((entry) => entry.provider_id === provider.id);
    const primaryAlias = aliases[0];
    ui.aliasName.value = primaryAlias?.alias || "";
    ui.aliasModel.value = primaryAlias?.model || provider.default_model || "";
    ui.aliasDescription.value = primaryAlias?.description || "";
    ui.setMain.checked = currentData().status?.main_agent_alias === primaryAlias?.alias;
    if (ui.advancedDetails) {
      ui.advancedDetails.open = true;
    }
    if (!providerState.authSessionId) {
      providerState.authStatusMessage = "";
      providerState.authStatusTone = "neutral";
    }
    syncProviderModeUi();
    renderPresetCatalog();
    renderWorkbench();
  }

  async function discoverProviderModels(providerId) {
    const models = await app().apiGet(`/v1/providers/${encodeURIComponent(providerId)}/models`);
    renderModelResults(models);
  }

  async function saveProviderForm(event) {
    event.preventDefault();
    const ui = elements();
    const submission = await resolveProviderFormSubmission();
    const { providerId, displayName, defaultModel, aliasName, aliasModel, setAsMain } = submission;
    const provider = {
      id: providerId,
      display_name: displayName,
      kind: ui.kind.value,
      base_url: ui.baseUrl.value.trim(),
      auth_mode: ui.authMode.value,
      default_model: defaultModel,
      keychain_account: null,
      oauth: null,
      local: ui.local.checked,
    };
    const oauthConfigText = ui.oauthConfig.value.trim();
    if (oauthConfigText) {
      provider.oauth = JSON.parse(oauthConfigText);
    }
    const payload = {
      provider,
      api_key: ui.apiKey.value.trim() || null,
      oauth_token: null,
    };
    const oauthTokenText = ui.oauthToken.value.trim();
    if (oauthTokenText) {
      payload.oauth_token = JSON.parse(oauthTokenText);
    }
    await app().apiPost("/v1/providers", payload);
    clearProviderSecretInputs();
    if (aliasName) {
      await app().apiPost("/v1/aliases", {
        alias: {
          alias: aliasName,
          provider_id: providerId,
          model: aliasModel,
          description: ui.aliasDescription.value.trim() || null,
        },
        set_as_main: setAsMain,
      });
    }
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
    populateProviderForm(providerId);
    app().setStatus(`Provider ${providerId} saved.`, "ok");
  }

  async function saveAliasForm(event) {
    event.preventDefault();
    const ui = elements();
    const name = ui.aliasQuickName.value.trim();
    const providerId = ui.aliasQuickProvider.value.trim();
    const model = ui.aliasQuickModel.value.trim();
    if (!name || !providerId || !model) {
      throw new Error("Alias name, provider, and model are required.");
    }
    await app().apiPost("/v1/aliases", {
      alias: {
        alias: name,
        provider_id: providerId,
        model,
        description: ui.aliasQuickDescription.value.trim() || null,
      },
      set_as_main: ui.aliasQuickMain.checked,
    });
    ui.aliasForm.reset();
    refreshAliasProviderOptions();
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
    app().setStatus(`Alias ${name} saved.`, "ok");
  }

  async function clearProviderCredentials(providerId) {
    await app().apiDelete(`/v1/providers/${encodeURIComponent(providerId)}/credentials`);
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
    if (providerState.editingProviderId === providerId) {
      populateProviderForm(providerId);
    }
  }

  async function deleteProvider(providerId) {
    if (!window.confirm(`Delete provider ${providerId}? Aliases pointing at it will also be removed.`)) {
      return;
    }
    await app().apiDelete(`/v1/providers/${encodeURIComponent(providerId)}`);
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
    if (providerState.editingProviderId === providerId) {
      resetProviderForm();
    }
  }

  function populateAliasForm(aliasName) {
    const ui = elements();
    const alias = currentData().aliases.find((entry) => entry.alias === aliasName);
    if (!alias) {
      throw new Error("Unknown alias.");
    }
    ui.aliasQuickName.value = alias.alias;
    ui.aliasQuickProvider.value = alias.provider_id;
    ui.aliasQuickModel.value = alias.model;
    ui.aliasQuickDescription.value = alias.description || "";
    ui.aliasQuickMain.checked = currentData().status?.main_agent_alias === alias.alias;
  }

  async function deleteAlias(aliasName) {
    if (!window.confirm(`Delete alias ${aliasName}?`)) {
      return;
    }
    await app().apiDelete(`/v1/aliases/${encodeURIComponent(aliasName)}`);
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
  }

  function selectPreset(presetKey, options = {}) {
    if (!PROVIDER_PRESETS[presetKey]) {
      return;
    }
    providerState.selectedPreset = presetKey;
    providerState.editingProviderId = null;
    resetProviderForm(false);
    elements().preset.value = presetKey;
    applyProviderPreset(presetKey);
    refreshProviderCreateSuggestions().catch((error) => {
      app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
    });
    if (options.focus && typeof app().focusSection === "function") {
      app().focusSection("providers");
    }
  }

  async function quickStart(presetKey) {
    selectPreset(presetKey, { focus: true });
    const preset = PROVIDER_PRESETS[presetKey];
    if (preset?.browserAuthKind) {
      await startProviderBrowserAuth();
    }
  }

  function reset() {
    clearProviderBrowserAuthPolling();
    providerState.currentData = null;
    providerState.editingProviderId = null;
    providerState.authSessionId = null;
    providerState.authKind = null;
    providerState.authWindow = null;
    providerState.authStatusMessage = "";
    providerState.authStatusTone = "neutral";
    providerState.autoDefaults = null;
    const ui = elements();
    if (ui.summary) {
      ui.summary.textContent = "No provider data yet";
    }
    if (ui.overviewCards) {
      ui.overviewCards.innerHTML = "";
    }
    if (ui.presetGrid) {
      ui.presetGrid.innerHTML = "";
    }
    if (ui.workbenchTitle) {
      ui.workbenchTitle.textContent = "Quick provider setup";
    }
    if (ui.workbenchStatus) {
      ui.workbenchStatus.textContent = "Choose a provider preset to begin.";
    }
    if (ui.workbenchIntro) {
      ui.workbenchIntro.textContent = "Guided provider setup will appear here.";
    }
    if (ui.workbenchLinks) {
      ui.workbenchLinks.innerHTML = "";
    }
    if (ui.workbenchSteps) {
      ui.workbenchSteps.innerHTML = "";
    }
    if (ui.providersList) {
      ui.providersList.innerHTML = renderEmpty("No providers configured.");
    }
    if (ui.aliasesList) {
      ui.aliasesList.innerHTML = renderEmpty("No aliases configured.");
    }
    if (ui.modelResults) {
      ui.modelResults.innerHTML = "";
    }
    if (ui.aliasQuickProvider) {
      ui.aliasQuickProvider.innerHTML = '<option value="">No providers saved yet</option>';
    }
    if (ui.chatMainTarget) {
      ui.chatMainTarget.textContent = "Default main alias: not configured.";
    }
    if (ui.runTaskAlias) {
      ui.runTaskAlias.innerHTML = "";
    }
    resetProviderForm();
  }

  function handleAuthMessage(event) {
    if (event.origin !== window.location.origin) {
      return;
    }
    const data = event.data;
    if (!data || data.type !== "provider-auth" || !data.sessionId) {
      return;
    }
    if (providerState.authSessionId && providerState.authSessionId !== data.sessionId) {
      return;
    }
    pollProviderBrowserAuthSession(data.sessionId, { refresh: true }).catch((error) => {
      setProviderBrowserAuthStatus(`Browser sign-in failed: ${error.message}`, "warn");
      app().setStatus(`Browser sign-in failed: ${error.message}`, "warn");
    });
  }

  function bind() {
    if (providerState.bound) {
      return;
    }
    providerState.bound = true;
    const ui = elements();
    resetProviderForm();
    renderPresetCatalog();
    renderWorkbench();
    ui.preset.addEventListener("change", () => {
      if (!providerState.editingProviderId) {
        providerState.selectedPreset = ui.preset.value;
        applyProviderPreset(ui.preset.value);
        refreshProviderCreateSuggestions().catch((error) => {
          app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
        });
      }
    });
    ui.reset.addEventListener("click", () => {
      resetProviderForm();
    });
    ui.browserAuth.addEventListener("click", () => {
      startProviderBrowserAuth().catch((error) => {
        app().setStatus(`Browser sign-in failed: ${error.message}`, "warn");
      });
    });
    ui.kind.addEventListener("change", () => {
      if (!providerState.authSessionId) {
        providerState.authStatusMessage = "";
        providerState.authStatusTone = "neutral";
      }
      applySuggestedProviderModelDefaults();
      renderWorkbench();
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    });
    ui.baseUrl.addEventListener("change", () => {
      applySuggestedProviderModelDefaults();
      renderWorkbench();
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    });
    ui.authMode.addEventListener("change", () => {
      renderWorkbench();
    });
    ui.defaultModel.addEventListener("change", () => {
      if (trimToNull(ui.aliasName.value) && !trimToNull(ui.aliasModel.value)) {
        ui.aliasModel.value = trimToNull(ui.defaultModel.value) || "";
      }
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    });
    ui.id.addEventListener("change", () => {
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    });
    ui.aliasName.addEventListener("change", () => {
      applySuggestedProviderModelDefaults();
      renderWorkbench();
      refreshProviderCreateSuggestions().catch((error) => {
        app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
      });
    });
    ui.openPortal.addEventListener("click", () => {
      openExternal(selectedPresetMeta().portalUrl);
    });
    ui.openDocs.addEventListener("click", () => {
      openExternal(selectedPresetMeta().docsUrl);
    });
    ui.discoverModels.addEventListener("click", () => {
      const providerId = trimToNull(ui.id.value);
      if (!providerId) {
        app().setStatus("Save or enter a provider ID first.", "warn");
        return;
      }
      discoverProviderModels(providerId).catch((error) => {
        app().setStatus(`Model discovery failed: ${error.message}`, "warn");
      });
    });
    ui.form.addEventListener("submit", (event) => {
      saveProviderForm(event).catch((error) => {
        app().setStatus(`Provider save failed: ${error.message}`, "warn");
      });
    });
    ui.aliasForm.addEventListener("submit", (event) => {
      saveAliasForm(event).catch((error) => {
        app().setStatus(`Alias save failed: ${error.message}`, "warn");
      });
    });
    document.body.addEventListener("click", (event) => {
      const target = event.target?.closest?.("[data-provider-select],[data-provider-action],[data-alias-action],[data-provider-use-model]");
      if (!(target instanceof HTMLElement)) {
        return;
      }
      if (target.dataset.providerSelect) {
        selectPreset(target.dataset.providerSelect, { focus: true });
        return;
      }
      if (target.dataset.providerUseModel) {
        ui.defaultModel.value = target.dataset.providerUseModel;
        if (!trimToNull(ui.aliasModel.value)) {
          ui.aliasModel.value = target.dataset.providerUseModel;
        }
        refreshProviderCreateSuggestions().catch((error) => {
          app().setStatus(`Provider suggestion failed: ${error.message}`, "warn");
        });
        return;
      }
      const work = async () => {
        if (target.dataset.providerAction === "edit") {
          populateProviderForm(target.dataset.providerId);
          app().focusSection?.("providers");
        } else if (target.dataset.providerAction === "models") {
          await discoverProviderModels(target.dataset.providerId);
        } else if (target.dataset.providerAction === "clear-credentials") {
          await clearProviderCredentials(target.dataset.providerId);
        } else if (target.dataset.providerAction === "delete") {
          await deleteProvider(target.dataset.providerId);
        } else if (target.dataset.aliasAction === "edit") {
          populateAliasForm(target.dataset.aliasName);
          app().focusSection?.("providers");
        } else if (target.dataset.aliasAction === "make-main") {
          await app().updateMainAlias(target.dataset.aliasName);
          render(app().getLastData());
        } else if (target.dataset.aliasAction === "delete") {
          await deleteAlias(target.dataset.aliasName);
        }
      };
      work().catch((error) => app().setStatus(`Provider action failed: ${error.message}`, "warn"));
    });
    document.body.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") {
        return;
      }
      const target = event.target?.closest?.("[data-provider-select]");
      if (!(target instanceof HTMLElement)) {
        return;
      }
      event.preventDefault();
      selectPreset(target.dataset.providerSelect, { focus: true });
    });
    window.addEventListener("message", handleAuthMessage);
  }

  window.dashboardProviders = {
    bind,
    render,
    reset,
    quickStart,
    handleAuthMessage,
  };
})();
