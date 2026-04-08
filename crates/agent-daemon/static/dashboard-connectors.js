(function () {
  const connectorState = {
    bound: false,
    currentData: null,
    selectedKind: "telegram",
    editingKey: null,
  };

  const GROUPS = {
    telegram: "telegrams",
    discord: "discords",
    slack: "slacks",
    signal: "signals",
    "home-assistant": "homeAssistants",
    webhook: "webhooks",
    inbox: "inboxes",
    gmail: "gmails",
    brave: "braves",
  };

  const COMMON_ADVANCED_FIELDS = [
    { id: "description", label: "Description", type: "text", placeholder: "Optional operator note" },
    {
      id: "alias",
      label: "Alias",
      type: "text",
      placeholder: "main",
      hint: "Mission target alias. Leave blank to use the runtime default.",
    },
    { id: "requested_model", label: "Model override", type: "text", placeholder: "optional" },
    { id: "cwd", label: "Working directory", type: "text", placeholder: "optional" },
    {
      id: "id",
      label: "Connector ID",
      type: "text",
      placeholder: "auto-generated if blank",
      hint: "Stable machine id used by the daemon.",
    },
    { id: "enabled", label: "Enabled", type: "checkbox", defaultValue: true },
  ];

  function app() {
    return window.dashboardApp || {};
  }

  function elements() {
    return {
      summary: document.getElementById("connector-summary"),
      cards: document.getElementById("connector-cards"),
      guideGrid: document.getElementById("connector-guide-grid"),
      roster: document.getElementById("connector-roster"),
      workbenchTitle: document.getElementById("connector-workbench-title"),
      workbenchStatus: document.getElementById("connector-workbench-status"),
      workbenchIntro: document.getElementById("connector-workbench-intro"),
      workbenchLinks: document.getElementById("connector-workbench-links"),
      workbenchSteps: document.getElementById("connector-workbench-steps"),
      form: document.getElementById("connector-quick-form"),
      basicFields: document.getElementById("connector-basic-fields"),
      advancedFields: document.getElementById("connector-advanced-fields"),
      openPortal: document.getElementById("connector-open-portal"),
      openDocs: document.getElementById("connector-open-docs"),
      reset: document.getElementById("connector-reset"),
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

  function buttonHtml(label, action, payload, extraClass = "") {
    const className = ["button-small", extraClass].filter(Boolean).join(" ");
    const dataAttrs = Object.entries(payload)
      .map(([key, value]) => `data-${escapeHtml(key)}="${escapeHtml(value)}"`)
      .join(" ");
    return `<button type="button" class="${className}" data-connector-action="${escapeHtml(action)}" ${dataAttrs}>${escapeHtml(label)}</button>`;
  }

  function normalizeOptionalString(value) {
    const trimmed = String(value ?? "").trim();
    return trimmed ? trimmed : null;
  }

  function slugify(value) {
    return String(value ?? "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "");
  }

  function clone(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function parseDelimitedList(raw, mapper) {
    return String(raw ?? "")
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean)
      .map((item) => (typeof mapper === "function" ? mapper(item) : item))
      .filter((item) => item !== null && item !== undefined && item !== "");
  }

  function basePath(kind) {
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

  const CONNECTOR_TYPES = {
    telegram: {
      label: "Telegram",
      summary: "Watch Telegram chats through a bot token and optional pairing approvals.",
      portalLabel: "Open BotFather",
      docsLabel: "Telegram Bot API docs",
      docsUrl: "https://core.telegram.org/bots/api",
      portalUrl() {
        return "https://t.me/BotFather";
      },
      pollable: true,
      defaultName: "Telegram bridge",
      steps: [
        "Create or manage the bot in BotFather, then copy the bot token.",
        "Send that bot a message from each chat you want the daemon to see.",
        "Save the connector. Pairing approval stays on by default so unknown chats do not auto-trigger work.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Telegram bridge" },
        {
          id: "bot_token",
          label: "Bot token",
          type: "secret",
          required: true,
          placeholder: "Telegram bot token",
          hint: "Existing saved token stays in place if you leave this blank while editing.",
        },
      ],
      advancedFields: [
        { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
        { id: "allowed_chat_ids", label: "Allowed chat IDs", type: "int_list", placeholder: "12345,67890" },
        { id: "allowed_user_ids", label: "Allowed user IDs", type: "int_list", placeholder: "12345,67890" },
      ],
      target(connector) {
        return connector.allowed_chat_ids?.length ? `${connector.allowed_chat_ids.length} allowed chat(s)` : "pairing approval";
      },
      detail(connector) {
        return connector.alias || connector.requested_model || "default routing";
      },
      hasSecret(connector) {
        return !!connector.bot_token_keychain_account;
      },
    },
    discord: {
      label: "Discord",
      summary: "Watch specific Discord channels through a bot token from the Developer Portal.",
      portalLabel: "Open Discord portal",
      docsLabel: "Discord getting started",
      docsUrl: "https://docs.discord.com/developers/quick-start/getting-started",
      portalUrl() {
        return "https://discord.com/developers/applications";
      },
      pollable: true,
      defaultName: "Discord bridge",
      steps: [
        "Create an app in the Discord Developer Portal and copy the bot token from the Bot page.",
        "Enable Developer Mode in Discord, then copy the channel IDs you want to monitor.",
        "Invite the bot to the server, save the connector, and poll once to confirm access.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Discord bridge" },
        {
          id: "bot_token",
          label: "Bot token",
          type: "secret",
          required: true,
          placeholder: "Discord bot token",
          hint: "Existing saved token stays in place if you leave this blank while editing.",
        },
        {
          id: "monitored_channel_ids",
          label: "Monitored channel IDs",
          type: "string_list",
          required: true,
          placeholder: "123456789012345678,234567890123456789",
        },
      ],
      advancedFields: [
        { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
        { id: "allowed_channel_ids", label: "Allowed channel IDs", type: "string_list", placeholder: "123456789012345678" },
        { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "123456789012345678" },
      ],
      target(connector) {
        return connector.monitored_channel_ids?.length ? `${connector.monitored_channel_ids.length} channel(s)` : "no channels selected";
      },
      detail(connector) {
        return connector.alias || connector.requested_model || "default routing";
      },
      hasSecret(connector) {
        return !!connector.bot_token_keychain_account;
      },
    },
    slack: {
      label: "Slack",
      summary: "Monitor Slack channels with a bot token from your workspace app install.",
      portalLabel: "Open Slack apps",
      docsLabel: "Slack app setup docs",
      docsUrl: "https://docs.slack.dev/tools/node-slack-sdk/getting-started",
      portalUrl() {
        return "https://api.slack.com/apps";
      },
      pollable: true,
      defaultName: "Slack bridge",
      steps: [
        "Create a Slack app, add the bot scopes you need, and install it to the workspace.",
        "Copy the xoxb bot token and the channel IDs you want the daemon to watch.",
        "Invite the bot to those channels, save the connector, and poll to verify access.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Slack bridge" },
        {
          id: "bot_token",
          label: "Bot token",
          type: "secret",
          required: true,
          placeholder: "xoxb-...",
          hint: "Existing saved token stays in place if you leave this blank while editing.",
        },
        {
          id: "monitored_channel_ids",
          label: "Monitored channel IDs",
          type: "string_list",
          required: true,
          placeholder: "C01ABCDE2FG,C01H1JKL3MN",
        },
      ],
      advancedFields: [
        { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
        { id: "allowed_channel_ids", label: "Allowed channel IDs", type: "string_list", placeholder: "C01ABCDE2FG" },
        { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "U01ABCDE2FG" },
      ],
      target(connector) {
        return connector.monitored_channel_ids?.length ? `${connector.monitored_channel_ids.length} channel(s)` : "no channels selected";
      },
      detail(connector) {
        return connector.alias || connector.requested_model || "default routing";
      },
      hasSecret(connector) {
        return !!connector.bot_token_keychain_account;
      },
    },
    signal: {
      label: "Signal",
      summary: "Use a local signal-cli account to read or send messages from Signal.",
      portalLabel: "Open signal-cli docs",
      docsLabel: "signal-cli project",
      docsUrl: "https://github.com/AsamK/signal-cli",
      portalUrl() {
        return "https://github.com/AsamK/signal-cli";
      },
      pollable: true,
      defaultName: "Signal bridge",
      steps: [
        "Install signal-cli locally and register or link the Signal account you want to use.",
        "Enter the account identifier and any group IDs you want the daemon to monitor.",
        "Save the connector, then poll once to confirm the local signal-cli setup works.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Signal bridge" },
        { id: "account", label: "Account", type: "text", required: true, placeholder: "+15551234567" },
      ],
      advancedFields: [
        { id: "cli_path", label: "signal-cli path", type: "text", placeholder: "C:\\tools\\signal-cli\\bin\\signal-cli.bat" },
        { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
        { id: "monitored_group_ids", label: "Monitored group IDs", type: "string_list", placeholder: "group-id-1,group-id-2" },
        { id: "allowed_group_ids", label: "Allowed group IDs", type: "string_list", placeholder: "group-id-1" },
        { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "+15551234567,+15557654321" },
      ],
      target(connector) {
        return connector.monitored_group_ids?.length ? `${connector.monitored_group_ids.length} group(s)` : connector.account || "local account";
      },
      detail(connector) {
        return connector.cli_path || connector.alias || connector.requested_model || "default routing";
      },
      hasSecret() {
        return true;
      },
    },
    "home-assistant": {
      label: "Home Assistant",
      summary: "Connect to a Home Assistant instance with a long-lived access token.",
      portalLabel: "Open Home Assistant profile",
      docsLabel: "Auth API docs",
      docsUrl: "https://developers.home-assistant.io/docs/auth_api/#long-lived-access-token",
      portalUrl(values) {
        const baseUrl = normalizeOptionalString(values.base_url);
        return baseUrl ? `${baseUrl.replace(/\/+$/, "")}/profile` : null;
      },
      pollable: true,
      defaultName: "Home Assistant",
      steps: [
        "Open your Home Assistant profile page and create a long-lived access token.",
        "Enter the base URL, paste the token, and list the entities you want watched.",
        "Optionally restrict service domains or entity targets before you save and poll.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Home Assistant" },
        { id: "base_url", label: "Base URL", type: "text", required: true, placeholder: "http://homeassistant.local:8123" },
        {
          id: "access_token",
          label: "Access token",
          type: "secret",
          required: true,
          placeholder: "Home Assistant long-lived token",
          hint: "Existing saved token stays in place if you leave this blank while editing.",
        },
        {
          id: "monitored_entity_ids",
          label: "Monitored entity IDs",
          type: "string_list",
          required: true,
          placeholder: "sensor.office_temperature,light.desk",
        },
      ],
      advancedFields: [
        { id: "allowed_service_domains", label: "Allowed service domains", type: "string_list", placeholder: "light,switch,climate" },
        { id: "allowed_service_entity_ids", label: "Allowed service entity IDs", type: "string_list", placeholder: "light.office,switch.fan" },
      ],
      target(connector) {
        return connector.monitored_entity_ids?.length
          ? `${connector.monitored_entity_ids.length} entit${connector.monitored_entity_ids.length === 1 ? "y" : "ies"}`
          : connector.base_url || "instance";
      },
      detail(connector) {
        return connector.base_url || connector.alias || connector.requested_model || "default routing";
      },
      hasSecret(connector) {
        return !!connector.access_token_keychain_account;
      },
    },
    webhook: {
      label: "Webhook",
      summary: "Accept inbound HTTP events and turn them into queued missions.",
      portalLabel: "Webhook setup is local",
      docsLabel: "Built-in guide",
      docsUrl: null,
      portalUrl() {
        return null;
      },
      pollable: false,
      defaultName: "Webhook intake",
      steps: [
        "Write the prompt template that should run whenever this hook receives an event.",
        "Optional: add a shared secret if you want callers to authenticate.",
        "POST to /v1/hooks/{connector_id} once the connector is saved.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Webhook intake" },
        { id: "prompt_template", label: "Prompt template", type: "textarea", required: true, placeholder: "Handle this webhook payload:\\n{{body}}" },
      ],
      advancedFields: [
        {
          id: "webhook_token",
          label: "Webhook shared secret",
          type: "secret",
          placeholder: "Optional shared secret",
          hint: "Leave blank to keep the current secret or run the hook without one.",
        },
      ],
      target(connector) {
        return connector.alias || "default routing";
      },
      detail(connector) {
        return connector.token_sha256 ? "shared secret configured" : "no secret";
      },
      hasSecret(connector) {
        return !!connector.token_sha256;
      },
    },
    inbox: {
      label: "Inbox",
      summary: "Watch a local folder and queue missions from files dropped into it.",
      portalLabel: "Local filesystem only",
      docsLabel: "Built-in guide",
      docsUrl: null,
      portalUrl() {
        return null;
      },
      pollable: true,
      defaultName: "Local inbox",
      steps: [
        "Pick the folder the daemon should watch for inbound files.",
        "Enable delete-after-read only if another process will not need the files later.",
        "Save the connector and poll it to process anything already waiting in the inbox.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Local inbox" },
        { id: "path", label: "Inbox path", type: "text", required: true, placeholder: "J:\\Inbox\\agent" },
      ],
      advancedFields: [{ id: "delete_after_read", label: "Delete files after read", type: "checkbox", defaultValue: false }],
      target(connector) {
        return connector.path || "folder";
      },
      detail(connector) {
        return connector.delete_after_read ? "deletes processed files" : "keeps processed files";
      },
      hasSecret() {
        return true;
      },
    },
    gmail: {
      label: "Gmail",
      summary: "Read Gmail with a stored OAuth bearer token and optional sender allowlist.",
      portalLabel: "Open Google Cloud",
      docsLabel: "Gmail API auth docs",
      docsUrl: "https://developers.google.com/workspace/gmail/api/auth/scopes",
      portalUrl() {
        return "https://console.cloud.google.com/apis/library/gmail.googleapis.com";
      },
      pollable: true,
      defaultName: "Gmail inbox",
      steps: [
        "Enable the Gmail API for your Google project and obtain an OAuth token with Gmail scopes.",
        "Paste the OAuth token, then optionally restrict allowed senders or a specific label.",
        "Save the connector and poll it to read unread messages that match the configured rules.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Gmail inbox" },
        {
          id: "oauth_token",
          label: "OAuth token",
          type: "secret",
          required: true,
          placeholder: "Google OAuth bearer token",
          hint: "Existing saved token stays in place if you leave this blank while editing.",
        },
      ],
      advancedFields: [
        { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
        { id: "allowed_sender_addresses", label: "Allowed sender addresses", type: "string_list", placeholder: "alerts@example.com,ci@example.com" },
        { id: "label_filter", label: "Label filter", type: "text", placeholder: "INBOX" },
      ],
      target(connector) {
        return connector.label_filter || "INBOX";
      },
      detail(connector) {
        return connector.allowed_sender_addresses?.length
          ? `${connector.allowed_sender_addresses.length} allowed sender(s)`
          : connector.alias || connector.requested_model || "default routing";
      },
      hasSecret(connector) {
        return !!connector.oauth_keychain_account;
      },
    },
    brave: {
      label: "Brave Search",
      summary: "Project Brave Search into the runtime for web, news, image, and place search.",
      portalLabel: "Open Brave dashboard",
      docsLabel: "Brave Search API docs",
      docsUrl: "https://api-dashboard.search.brave.com/documentation",
      portalUrl() {
        return "https://api-dashboard.search.brave.com/";
      },
      pollable: false,
      defaultName: "Brave Search",
      steps: [
        "Create or open your Brave Search API account.",
        "Generate an API key and paste it here.",
        "Save the connector. No polling is needed; the search tools become available immediately.",
      ],
      basicFields: [
        { id: "name", label: "Connector name", type: "text", required: true, placeholder: "Brave Search" },
        {
          id: "api_key",
          label: "API key",
          type: "secret",
          required: true,
          placeholder: "BSA...",
          hint: "Existing saved key stays in place if you leave this blank while editing.",
        },
      ],
      advancedFields: [],
      target() {
        return "web, news, images, local";
      },
      detail(connector) {
        return connector.alias || connector.requested_model || "search tools";
      },
      hasSecret(connector) {
        return !!connector.api_key_keychain_account;
      },
    },
  };

  function connectorsForKind(kind) {
    const groupKey = GROUPS[kind];
    return connectorState.currentData?.[groupKey] || [];
  }

  function allConnectors() {
    return Object.keys(CONNECTOR_TYPES).flatMap((kind) =>
      connectorsForKind(kind).map((connector) => ({ kind, connector }))
    );
  }

  function lookupConnector(kind, id) {
    return connectorsForKind(kind).find((connector) => connector.id === id) || null;
  }

  function currentMeta() {
    return CONNECTOR_TYPES[connectorState.selectedKind] || CONNECTOR_TYPES.telegram;
  }

  function currentEditingConnector() {
    if (!connectorState.editingKey) {
      return null;
    }
    const [kind, id] = connectorState.editingKey.split(":");
    return lookupConnector(kind, id);
  }

  function currentMainAlias() {
    return connectorState.currentData?.status?.main_agent_alias || "main";
  }

  function nextAvailableConnectorId(kind, preferred, existingId) {
    const base = slugify(preferred) || `${kind}-connector`;
    const existing = new Set(
      connectorsForKind(kind)
        .map((connector) => connector.id)
        .filter((id) => id && id !== existingId)
    );
    if (!existing.has(base)) {
      return base;
    }
    let index = 2;
    while (existing.has(`${base}-${index}`)) {
      index += 1;
    }
    return `${base}-${index}`;
  }

  function defaultValues(kind, connector) {
    const meta = CONNECTOR_TYPES[kind];
    const values = {
      name: connector?.name || meta.defaultName,
      description: connector?.description || "",
      alias: connector?.alias || currentMainAlias() || "",
      requested_model: connector?.requested_model || "",
      cwd: connector?.cwd || "",
      id: connector?.id || nextAvailableConnectorId(kind, meta.defaultName, null),
      enabled: connector ? connector.enabled !== false : true,
    };
    [...meta.basicFields, ...meta.advancedFields].forEach((field) => {
      const raw = connector?.[field.id];
      if (field.type === "checkbox") {
        values[field.id] = raw !== undefined ? !!raw : !!field.defaultValue;
      } else if (field.type === "int_list" || field.type === "string_list") {
        values[field.id] = Array.isArray(raw) ? raw.join(", ") : "";
      } else {
        values[field.id] = raw ?? "";
      }
    });
    return values;
  }

  function fieldMarkup(field, value) {
    const hint = field.hint ? `<p class="field-hint">${escapeHtml(field.hint)}</p>` : "";
    if (field.type === "checkbox") {
      return `
        <label class="toggle">
          <input type="checkbox" data-connector-field="${escapeHtml(field.id)}"${value ? " checked" : ""}>
          <span>${escapeHtml(field.label)}</span>
        </label>
      `;
    }
    if (field.type === "textarea") {
      return `
        <label class="field">
          <span>${escapeHtml(field.label)}</span>
          <textarea data-connector-field="${escapeHtml(field.id)}" rows="5" placeholder="${escapeHtml(
            field.placeholder || ""
          )}">${escapeHtml(value || "")}</textarea>
          ${hint}
        </label>
      `;
    }
    return `
      <label class="field">
        <span>${escapeHtml(field.label)}</span>
        <input type="${field.type === "secret" ? "password" : "text"}" data-connector-field="${escapeHtml(
          field.id
        )}" autocomplete="off" placeholder="${escapeHtml(field.placeholder || "")}" value="${escapeHtml(value || "")}">
        ${hint}
      </label>
    `;
  }

  function renderForm(kind, connector) {
    const ui = elements();
    const meta = CONNECTOR_TYPES[kind];
    const values = defaultValues(kind, connector);
    ui.basicFields.innerHTML = meta.basicFields.map((field) => fieldMarkup(field, values[field.id])).join("");
    ui.advancedFields.innerHTML = [...meta.advancedFields, ...COMMON_ADVANCED_FIELDS]
      .map((field) => fieldMarkup(field, values[field.id]))
      .join("");
  }

  function readFieldValues() {
    const values = {};
    const form = elements().form;
    const inputs = form ? form.querySelectorAll("[data-connector-field]") : [];
    inputs.forEach((input) => {
      const key = input.dataset.connectorField;
      if (!key) {
        return;
      }
      values[key] =
        input instanceof HTMLInputElement && input.type === "checkbox" ? input.checked : input.value;
    });
    return values;
  }

  function docsUrlForKind(kind) {
    return CONNECTOR_TYPES[kind]?.docsUrl || null;
  }

  function portalUrlForKind(kind) {
    const meta = CONNECTOR_TYPES[kind];
    if (!meta || typeof meta.portalUrl !== "function") {
      return null;
    }
    return meta.portalUrl(readFieldValues());
  }

  function openExternal(url) {
    if (url) {
      window.open(url, "_blank", "noopener");
    }
  }

  function validateField(field, rawValue, editing) {
    if (field.type === "checkbox") {
      return;
    }
    const value = String(rawValue ?? "").trim();
    if (field.type === "secret") {
      if (field.required && !value && !editing) {
        throw new Error(`${field.label} is required.`);
      }
      return;
    }
    if (field.required && !value) {
      throw new Error(`${field.label} is required.`);
    }
  }

  function applyFieldToConnector(connector, payload, field, rawValue) {
    if (field.type === "checkbox") {
      connector[field.id] = !!rawValue;
      return;
    }
    const value = String(rawValue ?? "").trim();
    if (field.type === "secret") {
      if (value) {
        payload[field.id] = value;
      }
      return;
    }
    if (field.type === "string_list") {
      connector[field.id] = parseDelimitedList(value);
      return;
    }
    if (field.type === "int_list") {
      connector[field.id] = parseDelimitedList(value, (item) => {
        const parsed = Number.parseInt(item, 10);
        return Number.isFinite(parsed) ? parsed : null;
      });
      return;
    }
    if (field.id === "description") {
      connector.description = value;
      return;
    }
    if (field.id === "alias" || field.id === "requested_model" || field.id === "cwd") {
      connector[field.id] = normalizeOptionalString(value);
      return;
    }
    connector[field.id] = value;
  }

  function buildPayload(kind) {
    const meta = CONNECTOR_TYPES[kind];
    const editing = currentEditingConnector();
    const values = readFieldValues();
    const allFields = [...meta.basicFields, ...meta.advancedFields, ...COMMON_ADVANCED_FIELDS];
    allFields.forEach((field) => validateField(field, values[field.id], !!editing));

    const connector = editing
      ? clone(editing)
      : {
          id: "",
          name: "",
          description: "",
          enabled: true,
        };
    const payload = { connector };
    allFields.forEach((field) => applyFieldToConnector(connector, payload, field, values[field.id]));

    connector.name = normalizeOptionalString(connector.name) || meta.defaultName;
    connector.id =
      normalizeOptionalString(connector.id) ||
      nextAvailableConnectorId(kind, connector.name, editing?.id || null);
    connector.description = connector.description || "";
    connector.enabled = connector.enabled !== false;
    return payload;
  }

  function renderSummary() {
    const total = allConnectors().length;
    const ready = allConnectors().filter(({ kind, connector }) => CONNECTOR_TYPES[kind].hasSecret(connector)).length;
    elements().summary.textContent = total
      ? `${total} connector(s) configured | ${ready} with saved credentials`
      : "No connectors configured yet";
  }

  function renderCountCards() {
    const ui = elements();
    ui.cards.innerHTML = Object.entries(CONNECTOR_TYPES)
      .map(([kind, meta]) => `
        <article class="stat-card">
          <p class="stat-card__label">${escapeHtml(meta.label)}</p>
          <p class="stat-card__value">${escapeHtml(connectorsForKind(kind).length)}</p>
          <p class="stat-card__hint">configured connector(s)</p>
        </article>
      `)
      .join("");
  }

  function renderGuideCatalog() {
    elements().guideGrid.innerHTML = Object.entries(CONNECTOR_TYPES)
      .map(([kind, meta]) => {
        const activeClass = connectorState.selectedKind === kind ? " connector-guide-card--active" : "";
        return `
          <article class="connector-guide-card${activeClass}" data-connector-select="${escapeHtml(kind)}" tabindex="0" role="button">
            <div>
              <h3>${escapeHtml(meta.label)}</h3>
              <p>${escapeHtml(meta.summary)}</p>
            </div>
            <div class="badge-row">
              ${connectorsForKind(kind).length ? badge(`${connectorsForKind(kind).length} configured`, "good") : badge("not configured")}
              ${meta.pollable ? badge("pollable", "info") : badge("instant", "info")}
            </div>
            <div class="connector-guide-card__footer">
              <span class="table-sub">${escapeHtml(meta.portalLabel)}</span>
            </div>
          </article>
        `;
      })
      .join("");
  }

  function connectorStatusBadges(kind, connector) {
    const meta = CONNECTOR_TYPES[kind];
    const badges = [
      badge(connector.enabled ? "enabled" : "disabled", connector.enabled ? "good" : "warn"),
      badge(meta.hasSecret(connector) ? "credentials ready" : "credentials missing", meta.hasSecret(connector) ? "good" : "warn"),
    ];
    if (connector.require_pairing_approval !== undefined) {
      badges.push(badge(connector.require_pairing_approval ? "pairing approval" : "open pairing", connector.require_pairing_approval ? "info" : "warn"));
    }
    return badges.join("");
  }

  function rosterCard(kind, connector) {
    const meta = CONNECTOR_TYPES[kind];
    const actions = [
      buttonHtml("Edit", "edit", { kind, id: connector.id }, "button-small--ghost"),
      buttonHtml(connector.enabled ? "Disable" : "Enable", "toggle", { kind, id: connector.id }, connector.enabled ? "button-small--ghost" : ""),
      meta.pollable ? buttonHtml("Poll", "poll", { kind, id: connector.id }, "button-muted") : "",
      buttonHtml("Docs", "docs", { kind }, "button-small--ghost"),
      buttonHtml("Delete", "delete", { kind, id: connector.id }, "button-small--ghost"),
    ]
      .filter(Boolean)
      .join("");
    return `
      <article class="stack-card">
        <div class="card-title-row">
          <div>
            <h4>${escapeHtml(connector.name || connector.id)}</h4>
            <p class="card-subtitle">${escapeHtml(meta.label)} | ${escapeHtml(connector.id)}</p>
          </div>
          <div class="badge-row">${connectorStatusBadges(kind, connector)}</div>
        </div>
        <ul class="micro-list">
          <li>target: ${escapeHtml(meta.target(connector))}</li>
          <li>detail: ${escapeHtml(meta.detail(connector))}</li>
          <li>alias: ${escapeHtml(connector.alias || "default")}</li>
          <li>model: ${escapeHtml(connector.requested_model || "default")}</li>
          <li>cwd: ${escapeHtml(connector.cwd || "-")}</li>
        </ul>
        <div class="inline-actions">${actions}</div>
      </article>
    `;
  }

  function renderRoster() {
    const roster = allConnectors();
    elements().roster.innerHTML = roster.length
      ? roster.map(({ kind, connector }) => rosterCard(kind, connector)).join("")
      : renderEmpty("No connectors configured yet. Pick a type above to create your first one.");
  }

  function renderWorkbench() {
    const ui = elements();
    const meta = currentMeta();
    const editing = currentEditingConnector();
    const portalUrl = portalUrlForKind(connectorState.selectedKind);
    const docsUrl = docsUrlForKind(connectorState.selectedKind);
    ui.workbenchTitle.textContent = editing ? `Edit ${meta.label} connector` : `New ${meta.label} connector`;
    ui.workbenchStatus.textContent = editing
      ? `Editing ${editing.name || editing.id}. Saved secrets and runtime cursors are preserved unless you replace them.`
      : `${meta.portalLabel} if you still need credentials, then save the connector here.`;
    ui.workbenchIntro.textContent = meta.summary;
    ui.workbenchSteps.innerHTML = meta.steps.map((step) => `<li>${escapeHtml(step)}</li>`).join("");
    ui.workbenchLinks.innerHTML = [
      meta.portalLabel && portalUrl
        ? `<span class="table-sub">${escapeHtml(meta.portalLabel)}</span>`
        : "",
      meta.docsLabel ? `<span class="table-sub">${escapeHtml(docsUrl ? meta.docsLabel : `${meta.docsLabel} below`)}</span>` : "",
    ]
      .filter(Boolean)
      .join("");
    ui.openPortal.disabled = !portalUrl;
    ui.openPortal.textContent = meta.portalLabel || "Open setup portal";
    ui.openDocs.disabled = !docsUrl;
    ui.openDocs.textContent = meta.docsLabel || "Open docs";
    renderForm(connectorState.selectedKind, editing);
  }

  function render(data) {
    connectorState.currentData = data || connectorState.currentData;
    if (!connectorState.currentData) {
      return;
    }
    renderSummary();
    renderCountCards();
    renderGuideCatalog();
    renderWorkbench();
    renderRoster();
  }

  function reset() {
    connectorState.currentData = null;
    connectorState.selectedKind = "telegram";
    connectorState.editingKey = null;
    const ui = elements();
    if (ui.summary) {
      ui.summary.textContent = "No connector data yet";
    }
    if (ui.cards) {
      ui.cards.innerHTML = "";
    }
    if (ui.guideGrid) {
      ui.guideGrid.innerHTML = "";
    }
    if (ui.roster) {
      ui.roster.innerHTML = renderEmpty("No connectors configured.");
    }
    if (ui.workbenchTitle) {
      ui.workbenchTitle.textContent = "Quick setup";
    }
    if (ui.workbenchStatus) {
      ui.workbenchStatus.textContent = "Choose a connector type to begin.";
    }
    if (ui.workbenchIntro) {
      ui.workbenchIntro.textContent = "Guided setup will appear here.";
    }
    if (ui.workbenchSteps) {
      ui.workbenchSteps.innerHTML = "";
    }
    if (ui.basicFields) {
      ui.basicFields.innerHTML = "";
    }
    if (ui.advancedFields) {
      ui.advancedFields.innerHTML = "";
    }
  }

  function selectKind(kind, options = {}) {
    if (!CONNECTOR_TYPES[kind]) {
      return;
    }
    connectorState.selectedKind = kind;
    connectorState.editingKey = options.editingKey || null;
    render(connectorState.currentData);
    if (options.focus && typeof app().focusSection === "function") {
      app().focusSection("connectors");
    }
  }

  async function saveConnector(event) {
    event.preventDefault();
    const kind = connectorState.selectedKind;
    const path = basePath(kind);
    if (!path) {
      throw new Error("Unknown connector type.");
    }
    await app().apiPost(path, buildPayload(kind));
    connectorState.editingKey = null;
    await app().refreshDashboard({ silent: true });
    render(app().getLastData());
    app().setStatus(`${CONNECTOR_TYPES[kind].label} connector saved.`, "ok");
  }

  async function toggleConnector(kind, id) {
    const connector = lookupConnector(kind, id);
    if (!connector) {
      throw new Error("Unknown connector.");
    }
    await app().apiPost(basePath(kind), { connector: { ...clone(connector), enabled: !connector.enabled } });
    await app().refreshDashboard({ silent: true });
  }

  async function pollConnector(kind, id) {
    if (!CONNECTOR_TYPES[kind]?.pollable) {
      throw new Error("This connector type does not support polling.");
    }
    await app().apiPost(`${basePath(kind)}/${encodeURIComponent(id)}/poll`, {});
    await app().refreshDashboard({ silent: true });
    app().setStatus(`Polled ${CONNECTOR_TYPES[kind].label} connector ${id}.`, "ok");
  }

  async function deleteConnector(kind, id) {
    if (!window.confirm(`Delete ${CONNECTOR_TYPES[kind].label} connector ${id}?`)) {
      return;
    }
    await app().apiDelete(`${basePath(kind)}/${encodeURIComponent(id)}`);
    if (connectorState.editingKey === `${kind}:${id}`) {
      connectorState.editingKey = null;
    }
    await app().refreshDashboard({ silent: true });
  }

  function bind() {
    if (connectorState.bound) {
      return;
    }
    connectorState.bound = true;
    const ui = elements();
    ui.form?.addEventListener("submit", (event) => {
      saveConnector(event).catch((error) => app().setStatus(`Connector save failed: ${error.message}`, "warn"));
    });
    ui.reset?.addEventListener("click", () => {
      connectorState.editingKey = null;
      render(connectorState.currentData);
    });
    ui.openPortal?.addEventListener("click", () => {
      const url = portalUrlForKind(connectorState.selectedKind);
      if (!url) {
        app().setStatus("This connector does not use a browser setup portal.", "neutral");
        return;
      }
      openExternal(url);
    });
    ui.openDocs?.addEventListener("click", () => {
      openExternal(docsUrlForKind(connectorState.selectedKind));
    });
    document.body.addEventListener("click", (event) => {
      const target = event.target?.closest?.("[data-connector-select],[data-connector-action]");
      if (!(target instanceof HTMLElement)) {
        return;
      }
      if (target.dataset.connectorSelect) {
        selectKind(target.dataset.connectorSelect);
        return;
      }
      if (target.dataset.connectorAction === "edit") {
        selectKind(target.dataset.kind, { editingKey: `${target.dataset.kind}:${target.dataset.id}`, focus: true });
        return;
      }
      if (target.dataset.connectorAction === "docs") {
        openExternal(docsUrlForKind(target.dataset.kind));
        return;
      }
      const work = async () => {
        if (target.dataset.connectorAction === "toggle") {
          await toggleConnector(target.dataset.kind, target.dataset.id);
        } else if (target.dataset.connectorAction === "poll") {
          await pollConnector(target.dataset.kind, target.dataset.id);
        } else if (target.dataset.connectorAction === "delete") {
          await deleteConnector(target.dataset.kind, target.dataset.id);
        }
      };
      work().catch((error) => app().setStatus(`Connector action failed: ${error.message}`, "warn"));
    });
    document.body.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") {
        return;
      }
      const target = event.target?.closest?.("[data-connector-select]");
      if (!(target instanceof HTMLElement)) {
        return;
      }
      event.preventDefault();
      selectKind(target.dataset.connectorSelect);
    });
  }

  window.dashboardConnectors = {
    bind,
    render,
    reset,
    selectKind,
  };
})();
