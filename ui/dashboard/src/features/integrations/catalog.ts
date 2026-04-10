import type { AuthMode, ProviderKind } from "../../api/types";

export interface ProviderPreset {
  id: string;
  label: string;
  displayName: string;
  kind: ProviderKind;
  baseUrl: string;
  authMode: AuthMode;
  defaultModel: string;
  local: boolean;
  browserAuthKind?: "codex" | "claude";
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "codex",
    label: "ChatGPT Codex",
    displayName: "ChatGPT Codex",
    kind: "chat_gpt_codex",
    baseUrl: "https://chatgpt.com/backend-api/codex",
    authMode: "oauth",
    defaultModel: "gpt-5-codex",
    local: false,
    browserAuthKind: "codex"
  },
  {
    id: "openai",
    label: "OpenAI",
    displayName: "OpenAI",
    kind: "open_ai_compatible",
    baseUrl: "https://api.openai.com/v1",
    authMode: "api_key",
    defaultModel: "gpt-5",
    local: false
  },
  {
    id: "anthropic",
    label: "Claude / Anthropic",
    displayName: "Claude",
    kind: "anthropic",
    baseUrl: "https://api.anthropic.com",
    authMode: "api_key",
    defaultModel: "claude-sonnet-4-20250514",
    local: false,
    browserAuthKind: "claude"
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    displayName: "OpenRouter",
    kind: "open_ai_compatible",
    baseUrl: "https://openrouter.ai/api/v1",
    authMode: "api_key",
    defaultModel: "openai/gpt-4.1",
    local: false
  },
  {
    id: "moonshot",
    label: "Moonshot",
    displayName: "Moonshot",
    kind: "open_ai_compatible",
    baseUrl: "https://api.moonshot.ai/v1",
    authMode: "api_key",
    defaultModel: "kimi-k2",
    local: false
  },
  {
    id: "venice",
    label: "Venice",
    displayName: "Venice AI",
    kind: "open_ai_compatible",
    baseUrl: "https://api.venice.ai/api/v1",
    authMode: "api_key",
    defaultModel: "venice-large",
    local: false
  },
  {
    id: "ollama",
    label: "Ollama",
    displayName: "Ollama",
    kind: "ollama",
    baseUrl: "http://127.0.0.1:11434",
    authMode: "none",
    defaultModel: "",
    local: true
  },
  {
    id: "custom",
    label: "Custom",
    displayName: "",
    kind: "open_ai_compatible",
    baseUrl: "",
    authMode: "api_key",
    defaultModel: "",
    local: false
  }
];

export interface ConnectorField {
  id: string;
  label: string;
  type: "text" | "textarea" | "checkbox" | "string_list" | "int_list" | "secret";
  placeholder?: string;
  defaultValue?: string | boolean;
}

export interface ConnectorDefinition {
  id:
    | "webhook"
    | "inbox"
    | "telegram"
    | "discord"
    | "slack"
    | "signal"
    | "home-assistant"
    | "gmail"
    | "brave";
  label: string;
  endpoint: string;
  secretField?: string;
  fields: ConnectorField[];
}

const COMMON_FIELDS: ConnectorField[] = [
  { id: "id", label: "Connector ID", type: "text", placeholder: "auto if blank" },
  { id: "name", label: "Display name", type: "text", placeholder: "Operator bridge" },
  { id: "description", label: "Description", type: "textarea", placeholder: "Optional note" },
  { id: "enabled", label: "Enabled", type: "checkbox", defaultValue: true },
  { id: "alias", label: "Alias", type: "text", placeholder: "main" },
  { id: "requested_model", label: "Model override", type: "text", placeholder: "optional" },
  { id: "cwd", label: "Working directory", type: "text", placeholder: "optional" }
];

export const CONNECTOR_DEFINITIONS: ConnectorDefinition[] = [
  {
    id: "webhook",
    label: "Webhook",
    endpoint: "/v1/webhooks",
    secretField: "webhook_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "prompt_template", label: "Prompt template", type: "textarea" },
      { id: "webhook_token", label: "Webhook token", type: "secret" }
    ]
  },
  {
    id: "inbox",
    label: "Inbox",
    endpoint: "/v1/inboxes",
    fields: [
      ...COMMON_FIELDS,
      { id: "path", label: "Inbox path", type: "text", placeholder: "C:\\mailbox" },
      { id: "delete_after_read", label: "Delete after read", type: "checkbox" }
    ]
  },
  {
    id: "telegram",
    label: "Telegram",
    endpoint: "/v1/telegram",
    secretField: "bot_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "bot_token", label: "Bot token", type: "secret" },
      { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
      { id: "allowed_chat_ids", label: "Allowed chat IDs", type: "int_list", placeholder: "12345,67890" },
      { id: "allowed_user_ids", label: "Allowed user IDs", type: "int_list", placeholder: "12345,67890" }
    ]
  },
  {
    id: "discord",
    label: "Discord",
    endpoint: "/v1/discord",
    secretField: "bot_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "bot_token", label: "Bot token", type: "secret" },
      { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
      { id: "monitored_channel_ids", label: "Monitored channel IDs", type: "string_list", placeholder: "123,456" },
      { id: "allowed_channel_ids", label: "Allowed channel IDs", type: "string_list", placeholder: "123,456" },
      { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "123,456" }
    ]
  },
  {
    id: "slack",
    label: "Slack",
    endpoint: "/v1/slack",
    secretField: "bot_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "bot_token", label: "Bot token", type: "secret" },
      { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
      { id: "monitored_channel_ids", label: "Monitored channel IDs", type: "string_list", placeholder: "C01,C02" },
      { id: "allowed_channel_ids", label: "Allowed channel IDs", type: "string_list", placeholder: "C01,C02" },
      { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "U01,U02" }
    ]
  },
  {
    id: "signal",
    label: "Signal",
    endpoint: "/v1/signal",
    fields: [
      ...COMMON_FIELDS,
      { id: "account", label: "Account", type: "text", placeholder: "+15551234567" },
      { id: "cli_path", label: "CLI path", type: "text", placeholder: "optional" },
      { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
      { id: "monitored_group_ids", label: "Monitored group IDs", type: "string_list", placeholder: "group-a,group-b" },
      { id: "allowed_group_ids", label: "Allowed group IDs", type: "string_list", placeholder: "group-a" },
      { id: "allowed_user_ids", label: "Allowed user IDs", type: "string_list", placeholder: "+1555,+1666" }
    ]
  },
  {
    id: "home-assistant",
    label: "Home Assistant",
    endpoint: "/v1/home-assistant",
    secretField: "access_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "base_url", label: "Base URL", type: "text", placeholder: "http://homeassistant.local:8123" },
      { id: "access_token", label: "Access token", type: "secret" },
      { id: "monitored_entity_ids", label: "Monitored entity IDs", type: "string_list", placeholder: "light.kitchen" },
      { id: "allowed_service_domains", label: "Allowed service domains", type: "string_list", placeholder: "light,scene" },
      { id: "allowed_service_entity_ids", label: "Allowed service entities", type: "string_list", placeholder: "light.kitchen" }
    ]
  },
  {
    id: "gmail",
    label: "Gmail",
    endpoint: "/v1/gmail",
    secretField: "oauth_token",
    fields: [
      ...COMMON_FIELDS,
      { id: "oauth_token", label: "OAuth bearer token", type: "secret" },
      { id: "require_pairing_approval", label: "Require pairing approval", type: "checkbox", defaultValue: true },
      { id: "allowed_sender_addresses", label: "Allowed sender addresses", type: "string_list", placeholder: "ops@example.com" },
      { id: "label_filter", label: "Label filter", type: "text", placeholder: "optional" }
    ]
  },
  {
    id: "brave",
    label: "Brave Search",
    endpoint: "/v1/brave",
    secretField: "api_key",
    fields: [
      ...COMMON_FIELDS,
      { id: "api_key", label: "API key", type: "secret" }
    ]
  }
];
