use super::*;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub tool_name: String,
    pub input_schema_json: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub tool_name: String,
    pub input_schema_json: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token_sha256: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub delete_after_read: bool,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub allowed_chat_ids: Vec<i64>,
    #[serde(default)]
    pub allowed_user_ids: Vec<i64>,
    #[serde(default)]
    pub last_update_id: Option<i64>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordChannelCursor {
    pub channel_id: String,
    #[serde(default)]
    pub last_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub channel_cursors: Vec<DiscordChannelCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackChannelCursor {
    pub channel_id: String,
    #[serde(default)]
    pub last_message_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_channel_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub channel_cursors: Vec<SlackChannelCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantEntityCursor {
    pub entity_id: String,
    #[serde(default)]
    pub last_state: Option<String>,
    #[serde(default)]
    pub last_changed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub base_url: String,
    #[serde(default)]
    pub access_token_keychain_account: Option<String>,
    #[serde(default)]
    pub monitored_entity_ids: Vec<String>,
    #[serde(default)]
    pub allowed_service_domains: Vec<String>,
    #[serde(default)]
    pub allowed_service_entity_ids: Vec<String>,
    #[serde(default)]
    pub entity_cursors: Vec<HomeAssistantEntityCursor>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub account: String,
    #[serde(default)]
    pub cli_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub monitored_group_ids: Vec<String>,
    #[serde(default)]
    pub allowed_group_ids: Vec<String>,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BraveConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key_keychain_account: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorApprovalRecord {
    pub id: String,
    pub connector_kind: ConnectorKind,
    pub connector_id: String,
    pub connector_name: String,
    pub status: ConnectorApprovalStatus,
    pub title: String,
    pub details: String,
    pub source_key: String,
    #[serde(default)]
    pub source_event_id: Option<String>,
    #[serde(default)]
    pub external_chat_id: Option<String>,
    #[serde(default)]
    pub external_chat_display: Option<String>,
    #[serde(default)]
    pub external_user_id: Option<String>,
    #[serde(default)]
    pub external_user_display: Option<String>,
    #[serde(default)]
    pub message_preview: Option<String>,
    #[serde(default)]
    pub queued_mission_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub reviewed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub review_note: Option<String>,
}

impl ConnectorApprovalRecord {
    pub fn new(
        connector_kind: ConnectorKind,
        connector_id: String,
        connector_name: String,
        title: String,
        details: String,
        source_key: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            connector_kind,
            connector_id,
            connector_name,
            status: ConnectorApprovalStatus::Pending,
            title,
            details,
            source_key,
            source_event_id: None,
            external_chat_id: None,
            external_chat_display: None,
            external_user_id: None,
            external_user_display: None,
            message_preview: None,
            queued_mission_id: None,
            created_at: now,
            updated_at: now,
            reviewed_at: None,
            review_note: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]

pub struct McpServerUpsertRequest {
    pub server: McpServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConnectorUpsertRequest {
    pub connector: AppConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConnectorUpsertRequest {
    pub connector: WebhookConnectorConfig,
    #[serde(default)]
    pub webhook_token: Option<String>,
    #[serde(default)]
    pub clear_webhook_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxConnectorUpsertRequest {
    pub connector: InboxConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConnectorUpsertRequest {
    pub connector: TelegramConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConnectorUpsertRequest {
    pub connector: DiscordConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackConnectorUpsertRequest {
    pub connector: SlackConnectorConfig,
    #[serde(default)]
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantConnectorUpsertRequest {
    pub connector: HomeAssistantConnectorConfig,
    #[serde(default)]
    pub access_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalConnectorUpsertRequest {
    pub connector: SignalConnectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookEventRequest {
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookEventResponse {
    pub connector_id: String,
    pub mission_id: String,
    pub title: String,
    pub status: MissionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxPollResponse {
    pub connector_id: String,
    pub processed_files: usize,
    pub queued_missions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramPollResponse {
    pub connector_id: String,
    pub processed_updates: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub last_update_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub updated_channels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
    #[serde(default)]
    pub updated_channels: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantPollResponse {
    pub connector_id: String,
    pub processed_entities: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub updated_entities: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectorApprovalUpdateRequest {
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramSendRequest {
    pub chat_id: i64,
    pub text: String,
    #[serde(default)]
    pub disable_notification: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramSendResponse {
    pub connector_id: String,
    pub chat_id: i64,
    #[serde(default)]
    pub message_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordSendRequest {
    pub channel_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordSendResponse {
    pub connector_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackSendRequest {
    pub channel_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlackSendResponse {
    pub connector_id: String,
    pub channel_id: String,
    #[serde(default)]
    pub message_ts: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeAssistantEntityState {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub last_changed: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HomeAssistantServiceCallRequest {
    pub domain: String,
    pub service: String,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub service_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HomeAssistantServiceCallResponse {
    pub connector_id: String,
    pub domain: String,
    pub service: String,
    pub changed_entities: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalSendRequest {
    #[serde(default)]
    pub recipient: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalSendResponse {
    pub connector_id: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailConnectorConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub oauth_keychain_account: Option<String>,
    #[serde(default = "default_true")]
    pub require_pairing_approval: bool,
    #[serde(default)]
    pub allowed_sender_addresses: Vec<String>,
    #[serde(default)]
    pub label_filter: Option<String>,
    #[serde(default)]
    pub last_history_id: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub requested_model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailConnectorUpsertRequest {
    pub connector: GmailConnectorConfig,
    #[serde(default)]
    pub oauth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BraveConnectorUpsertRequest {
    pub connector: BraveConnectorConfig,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailPollResponse {
    pub connector_id: String,
    pub processed_messages: usize,
    pub queued_missions: usize,
    #[serde(default)]
    pub pending_approvals: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailSendRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GmailSendResponse {
    pub connector_id: String,
    #[serde(default)]
    pub message_id: Option<String>,
}
