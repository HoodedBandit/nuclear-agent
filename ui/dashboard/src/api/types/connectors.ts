import type { ConnectorApprovalStatus, ConnectorKind } from "./primitives";

export interface ConnectorBase {
  id: string;
  name: string;
  description: string;
  enabled?: boolean;
  alias?: string | null;
  requested_model?: string | null;
  cwd?: string | null;
}

export interface AppConnectorConfig extends ConnectorBase {
  command: string;
  args?: string[];
  tool_name: string;
  input_schema_json: string;
}

export type McpServerConfig = AppConnectorConfig;

export interface WebhookConnectorConfig extends ConnectorBase {
  prompt_template: string;
  token_sha256?: string | null;
}

export interface InboxConnectorConfig extends ConnectorBase {
  path: string;
  delete_after_read?: boolean;
}

export interface TelegramConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  allowed_chat_ids?: number[];
  allowed_user_ids?: number[];
  last_update_id?: number | null;
}

export interface DiscordConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids?: string[];
  allowed_channel_ids?: string[];
  allowed_user_ids?: string[];
}

export interface SlackConnectorConfig extends ConnectorBase {
  bot_token_keychain_account?: string | null;
  require_pairing_approval?: boolean;
  monitored_channel_ids?: string[];
  allowed_channel_ids?: string[];
  allowed_user_ids?: string[];
}

export interface SignalConnectorConfig extends ConnectorBase {
  account: string;
  cli_path?: string | null;
  require_pairing_approval?: boolean;
  monitored_group_ids?: string[];
  allowed_group_ids?: string[];
  allowed_user_ids?: string[];
}

export interface HomeAssistantConnectorConfig extends ConnectorBase {
  base_url: string;
  access_token_keychain_account?: string | null;
  monitored_entity_ids?: string[];
  allowed_service_domains?: string[];
  allowed_service_entity_ids?: string[];
}

export interface GmailConnectorConfig extends ConnectorBase {
  oauth_keychain_account?: string | null;
  allowed_senders?: string[];
  require_pairing_approval?: boolean;
}

export interface BraveConnectorConfig extends ConnectorBase {
  api_key_keychain_account?: string | null;
}

export interface ConnectorApprovalRecord {
  id: string;
  connector_kind: ConnectorKind;
  connector_id: string;
  connector_name: string;
  status: ConnectorApprovalStatus;
  title: string;
  details: string;
  source_key: string;
  message_preview?: string | null;
  queued_mission_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ToolInvocation {
  call_id: string;
  name: string;
  arguments: string;
  outcome: string;
  output: string;
}
