use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use agent_core::{
    AliasUpsertRequest, AuthMode, AutonomyEnableRequest, AutonomyMode, AutonomyProfile,
    AutonomyState, AutopilotConfig, AutopilotState, AutopilotUpdateRequest,
    ConnectorApprovalRecord, ConnectorApprovalUpdateRequest, DaemonConfigUpdateRequest,
    DelegationConfig, DelegationConfigUpdateRequest, DelegationLimit, DelegationTarget,
    DiscordConnectorConfig, DiscordConnectorUpsertRequest, DiscordPollResponse, EvolveConfig,
    EvolveStartRequest, HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest,
    HomeAssistantPollResponse, InboxConnectorConfig, InboxConnectorUpsertRequest,
    InboxPollResponse, InputAttachment, LogEntry, MemoryKind, MemoryRecord, MemoryReviewStatus,
    MemoryReviewUpdateRequest, MemoryScope, MemorySearchQuery, MemorySearchResponse,
    MemoryUpsertRequest, MessageRole, Mission, MissionStatus, PermissionPreset, PersistenceMode,
    ProviderConfig, ProviderKind, ProviderUpsertRequest, RunTaskResponse, SessionMessage,
    SessionSummary, SessionTranscript, SignalConnectorConfig, SignalConnectorUpsertRequest,
    SignalPollResponse, SkillDraft, SkillDraftStatus, SlackConnectorConfig,
    SlackConnectorUpsertRequest, SlackPollResponse, TelegramConnectorConfig,
    TelegramConnectorUpsertRequest, TelegramPollResponse, ThinkingLevel, TrustUpdateRequest,
    WakeTrigger, WebhookConnectorConfig, WebhookConnectorUpsertRequest, DEFAULT_CHATGPT_CODEX_URL,
    DEFAULT_MOONSHOT_URL, DEFAULT_OPENAI_URL, DEFAULT_OPENROUTER_URL, DEFAULT_VENICE_URL,
};
use agent_providers::{list_model_descriptors, store_api_key, ModelDescriptor};
use agent_storage::Storage;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    autonomy_summary, browser_hosted_kind_to_provider_kind, build_compact_prompt,
    build_uncommitted_review_prompt, collect_image_attachments, compact_session,
    complete_browser_login, complete_oauth_login, copy_to_clipboard, current_request_cwd,
    default_browser_hosted_url, default_hosted_url, execute_prompt, fork_session,
    hosted_kind_to_provider_kind, init_agents_file, interactive_provider_setup,
    load_transcript_for_interactive_fork, load_transcript_for_interactive_resume,
    openai_browser_oauth_config, parse_interactive_command, permission_summary,
    persist_thinking_level, rank_sessions_for_picker, resolve_active_alias,
    resolve_interactive_model_selection, resolve_requested_model_override,
    resolve_session_model_override, resolved_requested_model, run_bang_command,
    thinking_level_label, BrowserLoginResult, DaemonClient, HostedKindArg, InteractiveCommand,
    InteractiveModelSelection,
};

use super::events::{spawn_prompt_task, AppEvent, AppEventSender, PromptTask};

pub(super) const SESSION_PICKER_LIMIT: usize = 5_000;
const UI_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn build_http_client() -> Client {
    Client::builder()
        .timeout(UI_HTTP_TIMEOUT)
        .build()
        .unwrap_or_else(|_| Client::new())
}

#[derive(Clone, Copy)]
pub(crate) enum PickerMode {
    Resume,
    Fork,
    Model,
    Alias,
    Thinking,
    Permissions,
    Config,
    Delegation,
    Autonomy,
    Provider,
    ProviderAction,
    Webhook,
    WebhookAction,
    Inbox,
    InboxAction,
    Telegram,
    TelegramAction,
    Discord,
    DiscordAction,
    Slack,
    SlackAction,
    Signal,
    SignalAction,
    HomeAssistant,
    HomeAssistantAction,
    Persistence,
    SkillDraft,
    SkillDraftAction,
}

#[derive(Clone)]
pub(crate) struct ModelPickerEntry {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    pub(crate) context_window: Option<i64>,
    pub(crate) effective_context_window_percent: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct GenericPickerEntry {
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
    pub(crate) search_text: String,
    pub(crate) current: bool,
    pub(crate) action: PickerAction,
}

#[derive(Clone)]
pub(crate) enum PickerAction {
    Resume(SessionSummary),
    Fork(SessionSummary),
    SetModel(String),
    SetAlias(String),
    SetThinking(Option<ThinkingLevel>),
    SetPermission(PermissionPreset),
    OpenConfig,
    OpenSettingsSection(SettingsSection),
    OpenAliasPicker,
    OpenModelPicker,
    OpenThinkingPicker,
    OpenPermissionPicker,
    OpenDelegationPicker,
    ToggleTrust(TrustToggle),
    OpenAutonomyPicker,
    OpenEvolvePicker,
    OpenAutopilotPicker,
    SetAutonomy(AutonomyMenuAction),
    SetEvolve(EvolveMenuAction),
    ShowMissionQueue,
    ShowMemoryBrowser,
    ShowResidentProfile,
    OpenSkillDraftPicker(Option<SkillDraftStatus>),
    OpenSkillDraftActions(String),
    ShowSkillDraftDetails(String),
    PublishSkillDraft(String),
    RejectSkillDraft(String),
    ShowDelegationTargets,
    EditApiKey(String),
    OpenProviderPicker,
    OpenProviderActions(String),
    ShowProviderDetails(String),
    QueueExternal(ExternalAction),
    ClearProviderCredentials(String),
    OpenWebhookPicker,
    OpenWebhookActions(String),
    ShowWebhookDetails(String),
    ToggleWebhookEnabled(String, bool),
    OpenInboxPicker,
    OpenInboxActions(String),
    ShowInboxDetails(String),
    ToggleInboxEnabled(String, bool),
    PollInbox(String),
    OpenTelegramPicker,
    OpenTelegramActions(String),
    ShowTelegramDetails(String),
    ToggleTelegramEnabled(String, bool),
    PollTelegram(String),
    OpenDiscordPicker,
    OpenDiscordActions(String),
    ShowDiscordDetails(String),
    ToggleDiscordEnabled(String, bool),
    PollDiscord(String),
    OpenSlackPicker,
    OpenSlackActions(String),
    ShowSlackDetails(String),
    ToggleSlackEnabled(String, bool),
    PollSlack(String),
    OpenSignalPicker,
    OpenSignalActions(String),
    ShowSignalDetails(String),
    ToggleSignalEnabled(String, bool),
    PollSignal(String),
    OpenHomeAssistantPicker,
    OpenHomeAssistantActions(String),
    ShowHomeAssistantDetails(String),
    ToggleHomeAssistantEnabled(String, bool),
    PollHomeAssistant(String),
    ShowTelegramApprovals,
    SetDelegationDepth(DelegationLimit),
    SetDelegationParallel(DelegationLimit),
    ToggleProviderDelegation(String, bool),
    OpenPersistencePicker,
    SetPersistenceMode(PersistenceMode),
    ToggleAutoStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustToggle {
    Shell,
    Network,
    FullDisk,
    SelfEdit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Providers,
    ModelThinking,
    Permissions,
    Connectors,
    Autonomy,
    MemorySkills,
    Delegation,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutonomyMenuAction {
    EnableFreeThinking,
    EnableEvolve,
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvolveMenuAction {
    Start,
    StartBudgetFriendly,
    Pause,
    Resume,
    Stop,
}

pub(crate) struct PickerState {
    pub(crate) mode: PickerMode,
    pub(crate) title: String,
    pub(crate) hint: String,
    pub(crate) empty_message: String,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) sessions: Vec<SessionSummary>,
    pub(crate) models: Vec<ModelPickerEntry>,
    pub(crate) items: Vec<GenericPickerEntry>,
}

impl PickerState {
    pub(crate) fn filtered_sessions(&self) -> Vec<SessionSummary> {
        let query = self.query.trim().to_ascii_lowercase();
        let mut sessions = self
            .sessions
            .iter()
            .filter(|session| {
                if query.is_empty() {
                    return true;
                }
                let title = session.title.as_deref().unwrap_or("(untitled)");
                let cwd = session
                    .cwd
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                format!(
                    "{} {} {} {} {} {}",
                    session.id, title, session.alias, session.provider_id, session.model, cwd
                )
                .to_ascii_lowercase()
                .contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        sessions
    }

    pub(crate) fn filtered_models(&self) -> Vec<ModelPickerEntry> {
        let query = self.query.trim().to_ascii_lowercase();
        self.models
            .iter()
            .filter(|model| {
                if query.is_empty() {
                    return true;
                }
                format!(
                    "{} {} {}",
                    model.id,
                    model.display_name,
                    model.description.as_deref().unwrap_or_default()
                )
                .to_ascii_lowercase()
                .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn filtered_items(&self) -> Vec<GenericPickerEntry> {
        let query = self.query.trim().to_ascii_lowercase();
        self.items
            .iter()
            .filter(|item| {
                query.is_empty() || item.search_text.to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn filtered_len(&self) -> usize {
        match self.mode {
            PickerMode::Resume | PickerMode::Fork => self.filtered_sessions().len(),
            PickerMode::Model => self.filtered_models().len(),
            PickerMode::Alias
            | PickerMode::Thinking
            | PickerMode::Permissions
            | PickerMode::Config
            | PickerMode::Delegation
            | PickerMode::Autonomy
            | PickerMode::Provider
            | PickerMode::ProviderAction
            | PickerMode::Webhook
            | PickerMode::WebhookAction
            | PickerMode::Inbox
            | PickerMode::InboxAction
            | PickerMode::Telegram
            | PickerMode::TelegramAction
            | PickerMode::Discord
            | PickerMode::DiscordAction
            | PickerMode::Slack
            | PickerMode::SlackAction
            | PickerMode::Signal
            | PickerMode::SignalAction
            | PickerMode::HomeAssistant
            | PickerMode::HomeAssistantAction
            | PickerMode::Persistence
            | PickerMode::SkillDraft
            | PickerMode::SkillDraftAction => self.filtered_items().len(),
        }
    }

    pub(crate) fn clamp_selected(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

pub(crate) enum OverlayState {
    Transcript {
        scroll_back: usize,
    },
    Static {
        title: String,
        body: String,
        scroll: usize,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        cursor: usize,
        secret: bool,
        action: InputPromptAction,
    },
}

#[derive(Clone)]
pub(crate) enum InputPromptAction {
    UpdateApiKey { provider_id: String },
}

#[derive(Clone)]
pub(crate) enum ExternalAction {
    AddProvider,
    AddWebhookConnector,
    AddInboxConnector,
    AddTelegramConnector,
    AddDiscordConnector,
    AddSlackConnector,
    AddSignalConnector,
    AddHomeAssistantConnector,
    ProviderBrowserLogin { provider_id: String },
    ProviderOAuthLogin { provider_id: String },
    OpenDashboard,
}

pub(crate) struct TuiApp<'a> {
    pub(crate) storage: &'a Storage,
    pub(crate) client: DaemonClient,
    pub(crate) alias: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) permission_preset: Option<PermissionPreset>,
    pub(crate) attachments: Vec<InputAttachment>,
    pub(crate) cwd: PathBuf,
    pub(crate) transcript: Vec<SessionMessage>,
    pub(crate) input: String,
    pub(crate) input_cursor: usize,
    pub(crate) overlay: Option<OverlayState>,
    pub(crate) picker: Option<PickerState>,
    pub(crate) pending_external_action: Option<ExternalAction>,
    pub(crate) exit_requested: bool,
    pub(crate) busy: bool,
    pub(crate) busy_since: Option<Instant>,
    pub(crate) transcript_scroll_back: usize,
    pub(crate) requested_model: Option<String>,
    pub(crate) active_model: Option<String>,
    pub(crate) active_provider_name: Option<String>,
    pub(crate) context_window_tokens: Option<i64>,
    pub(crate) context_window_percent: Option<i64>,
    pub(crate) recent_events: Vec<LogEntry>,
    pub(crate) last_event_cursor: Option<DateTime<Utc>>,
}

impl<'a> TuiApp<'a> {
    pub(super) async fn new(
        storage: &'a Storage,
        client: DaemonClient,
        alias: Option<String>,
        session_id: Option<String>,
        thinking_level: Option<ThinkingLevel>,
        attachments: Vec<InputAttachment>,
        permission_preset: Option<PermissionPreset>,
    ) -> Result<Self> {
        let cwd = session_id
            .as_deref()
            .and_then(|session_id| storage.get_session(session_id).ok().flatten())
            .and_then(|session| session.cwd)
            .unwrap_or(current_request_cwd()?);
        let config = storage.load_config()?;
        let transcript = if let Some(session_id) = session_id.as_deref() {
            storage.list_session_messages(session_id)?
        } else {
            Vec::new()
        };
        let alias = alias.or_else(|| config.main_agent_alias.clone());
        let thinking_level = thinking_level.or(config.thinking_level);
        let permission_preset = permission_preset.or(Some(config.permission_preset));
        let mut recent_events = storage.list_logs(12)?;
        recent_events.reverse();
        let last_event_cursor = recent_events.last().map(|entry| entry.created_at);

        let mut app = Self {
            storage,
            client,
            alias,
            session_id,
            thinking_level,
            permission_preset,
            attachments,
            cwd,
            transcript,
            input: String::new(),
            input_cursor: 0,
            overlay: None,
            picker: None,
            pending_external_action: None,
            exit_requested: false,
            busy: false,
            busy_since: None,
            transcript_scroll_back: 0,
            requested_model: None,
            active_model: None,
            active_provider_name: None,
            context_window_tokens: None,
            context_window_percent: None,
            recent_events,
            last_event_cursor,
        };
        app.requested_model = resolve_session_model_override(
            storage,
            app.session_id.as_deref(),
            app.alias.as_deref(),
        )?;
        app.refresh_active_model_metadata().await?;
        Ok(app)
    }

    pub(super) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(super) fn take_external_action(&mut self) -> Option<ExternalAction> {
        self.pending_external_action.take()
    }

    pub(super) fn record_error(&mut self, error: impl Into<String>) {
        self.busy = false;
        self.busy_since = None;
        self.open_static_overlay("Error", format!("error: {}", error.into()));
    }

    pub(super) async fn handle_event(&mut self, event: AppEvent) -> Result<()> {
        match event {
            AppEvent::PromptFinished(result) => {
                self.busy = false;
                self.busy_since = None;
                match result {
                    Ok(response) => self.complete_prompt(response).await?,
                    Err(error) => self.open_static_overlay("Error", format!("error: {error}")),
                }
            }
            AppEvent::DaemonEvents(events) => {
                self.apply_daemon_events(events);
            }
        }
        Ok(())
    }

    pub(super) fn event_cursor(&self) -> Option<DateTime<Utc>> {
        self.last_event_cursor
    }

    pub(crate) fn latest_event_summary(&self) -> Option<String> {
        self.recent_events.last().map(|entry| {
            let mut text = format!("{}: {}", entry.scope, entry.message);
            if text.chars().count() > 56 {
                text = text.chars().take(56).collect::<String>();
                text.push_str("...");
            }
            text
        })
    }

    pub(crate) fn rendered_event_body(&self) -> String {
        if self.recent_events.is_empty() {
            return "No daemon events yet.".to_string();
        }
        self.recent_events
            .iter()
            .map(|entry| {
                format!(
                    "{} [{}] {} {}",
                    entry.created_at, entry.level, entry.scope, entry.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn apply_daemon_events(&mut self, events: Vec<LogEntry>) {
        if events.is_empty() {
            return;
        }

        for event in events {
            self.last_event_cursor = Some(event.created_at);
            if self
                .recent_events
                .iter()
                .any(|existing| existing.id == event.id)
            {
                continue;
            }
            self.recent_events.push(event);
        }
        self.recent_events
            .sort_by(|left, right| left.created_at.cmp(&right.created_at));
        if self.recent_events.len() > 60 {
            let drop_count = self.recent_events.len() - 60;
            self.recent_events.drain(0..drop_count);
        }
        let events_body = self.rendered_event_body();
        if let Some(OverlayState::Static { title, body, .. }) = &mut self.overlay {
            if title == "Events" {
                *body = events_body;
            }
        }
    }

    pub(super) async fn handle_key(
        &mut self,
        key: KeyEvent,
        event_tx: &AppEventSender,
    ) -> Result<()> {
        if self.picker.is_some() {
            return self.handle_picker_key(key).await;
        }

        if self.overlay.is_some() {
            return self.handle_overlay_key(key).await;
        }

        match key {
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.exit_requested = true;
            }
            KeyEvent {
                code: KeyCode::Char('t'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_transcript_overlay();
            }
            KeyEvent {
                code: KeyCode::F(1),
                ..
            } => {
                self.open_help_overlay();
            }
            KeyEvent {
                code: KeyCode::Char('?'),
                modifiers,
                ..
            } if self.input.is_empty() && !modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_help_overlay();
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.clear_input();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                self.backspace_input();
            }
            KeyEvent {
                code: KeyCode::Delete,
                ..
            } => {
                self.delete_input();
            }
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => {
                self.move_cursor_left();
            }
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => {
                self.move_cursor_right();
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                if self.input.is_empty() {
                    self.transcript_scroll_back = self.transcript_scroll_back.saturating_add(1);
                } else {
                    self.move_cursor_vertical(-1);
                }
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                if self.input.is_empty() {
                    self.transcript_scroll_back = self.transcript_scroll_back.saturating_sub(1);
                } else {
                    self.move_cursor_vertical(1);
                }
            }
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => {
                self.transcript_scroll_back = self.transcript_scroll_back.saturating_add(10);
            }
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => {
                self.transcript_scroll_back = self.transcript_scroll_back.saturating_sub(10);
            }
            KeyEvent {
                code: KeyCode::Home,
                modifiers,
                ..
            } => {
                if modifiers.contains(KeyModifiers::CONTROL) && self.input.is_empty() {
                    self.transcript_scroll_back = usize::MAX;
                } else {
                    self.move_cursor_line_start();
                }
            }
            KeyEvent {
                code: KeyCode::End,
                modifiers,
                ..
            } => {
                if modifiers.contains(KeyModifiers::CONTROL) && self.input.is_empty() {
                    self.transcript_scroll_back = 0;
                } else {
                    self.move_cursor_line_end();
                }
            }
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_cursor_line_start();
            }
            KeyEvent {
                code: KeyCode::Char('e'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_cursor_line_end();
            }
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert_input_char('\n');
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers,
                ..
            } => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.insert_input_char('\n');
                    return Ok(());
                }
                let line = self.input.trim().to_string();
                if line.is_empty() {
                    self.clear_input();
                    return Ok(());
                }
                if self.busy {
                    self.open_static_overlay(
                        "Busy",
                        "A request is already running. Wait for it to finish before submitting another command."
                            .to_string(),
                    );
                    return Ok(());
                }

                self.clear_input();
                if let Some(shell_command) = line.strip_prefix('!') {
                    match run_bang_command(self.storage, shell_command.trim(), &mut self.cwd).await
                    {
                        Ok(output) => self.open_static_overlay("Shell Output", output),
                        Err(error) => {
                            self.open_static_overlay("Error", format!("error: {error:#}"))
                        }
                    }
                } else if line.starts_with('/') {
                    if let Some(command) = parse_interactive_command(&line)? {
                        self.handle_slash_command(command, event_tx).await?;
                    }
                } else {
                    self.queue_prompt(line, event_tx)?;
                }
            }
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers,
                ..
            } if !modifiers.contains(KeyModifiers::CONTROL) => {
                self.insert_input_char(ch);
            }
            _ => {}
        }

        Ok(())
    }

    pub(super) async fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_active_view_up(3),
            MouseEventKind::ScrollDown => self.scroll_active_view_down(3),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn queue_prompt(&mut self, prompt: String, event_tx: &AppEventSender) -> Result<()> {
        if self.busy {
            self.open_static_overlay(
                "Busy",
                "A request is already running. Wait for it to finish before submitting another command."
                    .to_string(),
            );
            return Ok(());
        }

        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .get_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let session_id = self
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();
        self.transcript.push(
            SessionMessage::new(
                session_id.clone(),
                MessageRole::User,
                prompt.clone(),
                Some(provider.id.clone()),
                Some(selected_model),
            )
            .with_attachments(self.attachments.clone()),
        );
        self.transcript_scroll_back = 0;
        self.overlay = None;
        self.busy = true;
        self.busy_since = Some(Instant::now());
        let task = PromptTask {
            prompt,
            alias: self.alias.clone(),
            requested_model: self.requested_model.clone(),
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            thinking_level: self.thinking_level,
            attachments: self.attachments.clone(),
            permission_preset: self.permission_preset,
            output_schema_json: None,
            ephemeral: false,
        };
        spawn_prompt_task(self.client.clone(), task, event_tx.clone());
        Ok(())
    }

    async fn handle_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(mut picker) = self.picker.take() else {
            return Ok(());
        };

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.exit_requested = true;
                self.picker = Some(picker);
            }
            KeyCode::Esc => {
                self.picker = None;
            }
            KeyCode::Backspace => {
                picker.query.pop();
                picker.clamp_selected();
                self.picker = Some(picker);
            }
            KeyCode::Up => {
                if picker.selected > 0 {
                    picker.selected -= 1;
                }
                self.picker = Some(picker);
            }
            KeyCode::Down => {
                let len = picker.filtered_len();
                if picker.selected + 1 < len {
                    picker.selected += 1;
                }
                self.picker = Some(picker);
            }
            KeyCode::PageUp => {
                picker.selected = picker.selected.saturating_sub(10);
                self.picker = Some(picker);
            }
            KeyCode::PageDown => {
                let len = picker.filtered_len();
                if len > 0 {
                    picker.selected = (picker.selected + 10).min(len - 1);
                }
                self.picker = Some(picker);
            }
            KeyCode::Home => {
                picker.selected = 0;
                self.picker = Some(picker);
            }
            KeyCode::End => {
                let len = picker.filtered_len();
                if len > 0 {
                    picker.selected = len - 1;
                }
                self.picker = Some(picker);
            }
            KeyCode::Enter => match picker.mode {
                PickerMode::Resume => {
                    let sessions = picker.filtered_sessions();
                    let Some(session) = sessions.get(picker.selected).cloned() else {
                        self.open_static_overlay("Notice", picker.empty_message.clone());
                        self.picker = None;
                        return Ok(());
                    };
                    self.picker = None;
                    self.activate_picker_action(PickerAction::Resume(session))
                        .await?;
                }
                PickerMode::Fork => {
                    let sessions = picker.filtered_sessions();
                    let Some(session) = sessions.get(picker.selected).cloned() else {
                        self.open_static_overlay("Notice", picker.empty_message.clone());
                        self.picker = None;
                        return Ok(());
                    };
                    self.picker = None;
                    self.activate_picker_action(PickerAction::Fork(session))
                        .await?;
                }
                PickerMode::Model => {
                    let models = picker.filtered_models();
                    let Some(model) = models.get(picker.selected).cloned() else {
                        self.open_static_overlay("Notice", picker.empty_message.clone());
                        self.picker = None;
                        return Ok(());
                    };
                    self.picker = None;
                    self.activate_picker_action(PickerAction::SetModel(model.id))
                        .await?;
                }
                PickerMode::Alias
                | PickerMode::Thinking
                | PickerMode::Permissions
                | PickerMode::Config
                | PickerMode::Delegation
                | PickerMode::Autonomy
                | PickerMode::Provider
                | PickerMode::ProviderAction
                | PickerMode::Webhook
                | PickerMode::WebhookAction
                | PickerMode::Inbox
                | PickerMode::InboxAction
                | PickerMode::Telegram
                | PickerMode::TelegramAction
                | PickerMode::Discord
                | PickerMode::DiscordAction
                | PickerMode::Slack
                | PickerMode::SlackAction
                | PickerMode::Signal
                | PickerMode::SignalAction
                | PickerMode::HomeAssistant
                | PickerMode::HomeAssistantAction
                | PickerMode::Persistence
                | PickerMode::SkillDraft
                | PickerMode::SkillDraftAction => {
                    let items = picker.filtered_items();
                    let Some(item) = items.get(picker.selected).cloned() else {
                        self.open_static_overlay("Notice", picker.empty_message.clone());
                        self.picker = None;
                        return Ok(());
                    };
                    self.picker = None;
                    self.activate_picker_action(item.action).await?;
                }
            },
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    picker.query.push(ch);
                    picker.clamp_selected();
                }
                self.picker = Some(picker);
            }
            _ => {
                self.picker = Some(picker);
            }
        }

        Ok(())
    }

    async fn activate_picker_action(&mut self, action: PickerAction) -> Result<()> {
        match action {
            PickerAction::Resume(session) => {
                self.resume_session(session)?;
                self.refresh_active_model_metadata().await?;
            }
            PickerAction::Fork(session) => {
                self.fork_session(session)?;
                self.refresh_active_model_metadata().await?;
            }
            PickerAction::SetModel(model_id) => {
                self.requested_model = resolve_requested_model_override(
                    self.storage,
                    self.alias.as_deref(),
                    &model_id,
                )?;
                self.refresh_active_model_metadata().await?;
                self.open_static_overlay("Models", format!("model override set to {model_id}"));
            }
            PickerAction::SetAlias(alias) => {
                let config = self.storage.load_config()?;
                let alias_entry = config
                    .get_alias(&alias)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown alias '{alias}'"))?;
                let _: agent_core::ModelAlias = self
                    .client
                    .post(
                        "/v1/aliases",
                        &AliasUpsertRequest {
                            alias: alias_entry,
                            set_as_main: true,
                        },
                    )
                    .await?;
                self.alias = Some(alias.clone());
                self.requested_model = None;
                self.refresh_active_model_metadata().await?;
                self.open_static_overlay("Settings", format!("default alias set to {alias}"));
            }
            PickerAction::SetThinking(level) => {
                self.thinking_level = level;
                persist_thinking_level(self.storage, self.thinking_level)?;
                self.open_static_overlay(
                    "Thinking",
                    format!("thinking={}", thinking_level_label(self.thinking_level)),
                );
            }
            PickerAction::SetPermission(preset) => {
                let updated: PermissionPreset = self
                    .client
                    .put(
                        "/v1/permissions",
                        &agent_core::PermissionUpdateRequest {
                            permission_preset: preset,
                        },
                    )
                    .await?;
                self.permission_preset = Some(updated);
                let mut config = self.storage.load_config()?;
                config.permission_preset = updated;
                self.storage.save_config(&config)?;
                self.open_static_overlay(
                    "Permissions",
                    format!("permission_preset={}", permission_summary(updated)),
                );
            }
            PickerAction::OpenConfig => {
                self.open_config_picker().await?;
            }
            PickerAction::OpenSettingsSection(section) => {
                self.open_settings_section_picker(section).await?;
            }
            PickerAction::OpenAliasPicker => {
                self.open_alias_picker().await?;
            }
            PickerAction::OpenModelPicker => {
                self.open_model_picker().await?;
            }
            PickerAction::OpenThinkingPicker => {
                self.open_thinking_picker().await?;
            }
            PickerAction::OpenPermissionPicker => {
                self.open_permission_picker();
            }
            PickerAction::OpenDelegationPicker => {
                self.open_delegation_picker().await?;
            }
            PickerAction::ToggleTrust(toggle) => {
                self.toggle_trust_setting(toggle).await?;
            }
            PickerAction::OpenAutonomyPicker => {
                self.open_autonomy_picker().await?;
            }
            PickerAction::OpenEvolvePicker => {
                self.open_evolve_picker().await?;
            }
            PickerAction::OpenAutopilotPicker => {
                let status: AutopilotConfig = self.client.get("/v1/autopilot/status").await?;
                self.open_static_overlay("Autopilot", crate::autopilot_summary(&status));
            }
            PickerAction::SetAutonomy(action) => {
                self.apply_autonomy_action(action).await?;
            }
            PickerAction::SetEvolve(action) => {
                self.apply_evolve_action(action).await?;
            }
            PickerAction::ShowMissionQueue => {
                let missions: Vec<Mission> = self.client.get("/v1/missions").await?;
                let body = if missions.is_empty() {
                    "No missions queued.".to_string()
                } else {
                    missions
                        .into_iter()
                        .map(|mission| {
                            format!(
                                "{} [{:?}] {} wake_at={} watch={} retries={}/{}",
                                mission.id,
                                mission.status,
                                mission.title,
                                mission
                                    .wake_at
                                    .map(|value| value.to_rfc3339())
                                    .unwrap_or_else(|| "-".to_string()),
                                mission
                                    .watch_path
                                    .as_deref()
                                    .map(|value| value.display().to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                                mission.retries,
                                mission.max_retries
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Missions", body);
            }
            PickerAction::ShowMemoryBrowser => {
                let memories: Vec<MemoryRecord> = self.client.get("/v1/memory?limit=20").await?;
                self.open_static_overlay("Memory", crate::format_memory_records(&memories));
            }
            PickerAction::ShowResidentProfile => {
                let memories: Vec<MemoryRecord> =
                    self.client.get("/v1/memory/profile?limit=20").await?;
                self.open_static_overlay(
                    "Resident Profile",
                    crate::format_memory_records(&memories),
                );
            }
            PickerAction::OpenSkillDraftPicker(status) => {
                self.open_skill_draft_picker(status).await?;
            }
            PickerAction::OpenSkillDraftActions(draft_id) => {
                self.open_skill_draft_action_picker(&draft_id).await?;
            }
            PickerAction::ShowSkillDraftDetails(draft_id) => {
                self.show_skill_draft_details(&draft_id).await?;
            }
            PickerAction::PublishSkillDraft(draft_id) => {
                let draft: SkillDraft = self
                    .client
                    .post(
                        &format!("/v1/skills/drafts/{draft_id}/publish"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Learned Skills",
                    format!("published skill draft={} title={}", draft.id, draft.title),
                );
            }
            PickerAction::RejectSkillDraft(draft_id) => {
                let draft: SkillDraft = self
                    .client
                    .post(
                        &format!("/v1/skills/drafts/{draft_id}/reject"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Learned Skills",
                    format!("rejected skill draft={} title={}", draft.id, draft.title),
                );
            }
            PickerAction::ShowDelegationTargets => {
                let targets: Vec<DelegationTarget> =
                    self.client.get("/v1/delegation/targets").await?;
                let body = if targets.is_empty() {
                    "No delegation targets are currently available.".to_string()
                } else {
                    targets
                        .into_iter()
                        .map(|target| {
                            format!(
                                "{} [{}] {} / {}{}\n  names: {}",
                                target.alias,
                                target.provider_id,
                                target.provider_display_name,
                                target.model,
                                if target.primary { " (primary)" } else { "" },
                                target.target_names.join(", ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Delegation targets", body);
            }
            PickerAction::EditApiKey(provider_id) => {
                self.open_input_overlay(
                    "API Key",
                    format!("Paste the new API key for {provider_id}"),
                    true,
                    InputPromptAction::UpdateApiKey { provider_id },
                );
            }
            PickerAction::OpenProviderPicker => {
                self.open_provider_picker()?;
            }
            PickerAction::OpenProviderActions(provider_id) => {
                self.open_provider_action_picker(&provider_id)?;
            }
            PickerAction::ShowProviderDetails(provider_id) => {
                self.show_provider_details(&provider_id)?;
            }
            PickerAction::OpenWebhookPicker => {
                self.open_webhook_picker().await?;
            }
            PickerAction::OpenWebhookActions(connector_id) => {
                self.open_webhook_action_picker(&connector_id).await?;
            }
            PickerAction::ShowWebhookDetails(connector_id) => {
                let connector: WebhookConnectorConfig = self
                    .client
                    .get(&format!("/v1/webhooks/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\nalias={}\nmodel={}\ncwd={}\ntoken_configured={}\nprompt_template=\n{}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    connector.token_sha256.is_some(),
                    connector.prompt_template
                );
                self.open_static_overlay("Webhook Connector", body);
            }
            PickerAction::OpenInboxPicker => {
                self.open_inbox_picker().await?;
            }
            PickerAction::OpenInboxActions(connector_id) => {
                self.open_inbox_action_picker(&connector_id).await?;
            }
            PickerAction::ShowInboxDetails(connector_id) => {
                let connector: InboxConnectorConfig = self
                    .client
                    .get(&format!("/v1/inboxes/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\ndelete_after_read={}\nalias={}\nmodel={}\npath={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.delete_after_read,
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector.path.display(),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Inbox Connector", body);
            }
            PickerAction::QueueExternal(action) => {
                self.pending_external_action = Some(action);
            }
            PickerAction::ClearProviderCredentials(provider_id) => {
                let updated: agent_core::ProviderConfig = self
                    .client
                    .delete(&format!("/v1/providers/{provider_id}/credentials"))
                    .await?;
                self.open_static_overlay(
                    "Providers",
                    format!("cleared stored credentials for {}", updated.display_name),
                );
            }
            PickerAction::ToggleWebhookEnabled(connector_id, enabled) => {
                let mut connector: WebhookConnectorConfig = self
                    .client
                    .get(&format!("/v1/webhooks/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: WebhookConnectorConfig = self
                    .client
                    .post(
                        "/v1/webhooks",
                        &WebhookConnectorUpsertRequest {
                            connector: connector.clone(),
                            webhook_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Webhook Connectors",
                    format!(
                        "{} webhook connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::ToggleInboxEnabled(connector_id, enabled) => {
                let mut connector: InboxConnectorConfig = self
                    .client
                    .get(&format!("/v1/inboxes/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: InboxConnectorConfig = self
                    .client
                    .post(
                        "/v1/inboxes",
                        &InboxConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Inbox Connectors",
                    format!(
                        "{} inbox connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollInbox(connector_id) => {
                let response: InboxPollResponse = self
                    .client
                    .post(
                        &format!("/v1/inboxes/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Inbox Connectors",
                    format!(
                        "polled {}: processed {} file(s), queued {} mission(s)",
                        response.connector_id, response.processed_files, response.queued_missions
                    ),
                );
            }
            PickerAction::OpenTelegramPicker => {
                self.open_telegram_picker().await?;
            }
            PickerAction::OpenTelegramActions(connector_id) => {
                self.open_telegram_action_picker(&connector_id).await?;
            }
            PickerAction::ShowTelegramDetails(connector_id) => {
                let connector: TelegramConnectorConfig = self
                    .client
                    .get(&format!("/v1/telegram/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\nbot_token_configured={}\nchat_ids={}\nuser_ids={}\nalias={}\nmodel={}\nlast_update_id={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.bot_token_keychain_account.is_some(),
                    crate::format_i64_list(&connector.allowed_chat_ids),
                    crate::format_i64_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .last_update_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Telegram Connector", body);
            }
            PickerAction::ToggleTelegramEnabled(connector_id, enabled) => {
                let mut connector: TelegramConnectorConfig = self
                    .client
                    .get(&format!("/v1/telegram/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: TelegramConnectorConfig = self
                    .client
                    .post(
                        "/v1/telegram",
                        &TelegramConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Telegram Connectors",
                    format!(
                        "{} telegram connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollTelegram(connector_id) => {
                let response: TelegramPollResponse = self
                    .client
                    .post(
                        &format!("/v1/telegram/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Telegram Connectors",
                    format!(
                        "polled {}: processed {} update(s), queued {} mission(s), last_update_id={}",
                        response.connector_id,
                        response.processed_updates,
                        response.queued_missions,
                        response
                            .last_update_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "-".to_string())
                    ),
                );
            }
            PickerAction::OpenDiscordPicker => {
                self.open_discord_picker().await?;
            }
            PickerAction::OpenDiscordActions(connector_id) => {
                self.open_discord_action_picker(&connector_id).await?;
            }
            PickerAction::ShowDiscordDetails(connector_id) => {
                let connector: DiscordConnectorConfig = self
                    .client
                    .get(&format!("/v1/discord/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\nbot_token_configured={}\nrequire_pairing_approval={}\nmonitored_channel_ids={}\nallowed_channel_ids={}\nallowed_user_ids={}\nchannel_cursors={}\nalias={}\nmodel={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.bot_token_keychain_account.is_some(),
                    connector.require_pairing_approval,
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    crate::format_discord_channel_cursors(&connector.channel_cursors),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Discord Connector", body);
            }
            PickerAction::ToggleDiscordEnabled(connector_id, enabled) => {
                let mut connector: DiscordConnectorConfig = self
                    .client
                    .get(&format!("/v1/discord/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: DiscordConnectorConfig = self
                    .client
                    .post(
                        "/v1/discord",
                        &DiscordConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Discord Connectors",
                    format!(
                        "{} discord connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollDiscord(connector_id) => {
                let response: DiscordPollResponse = self
                    .client
                    .post(
                        &format!("/v1/discord/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Discord Connectors",
                    format!(
                        "polled {}: processed {} message(s), queued {} mission(s), pending_approvals={}, updated_channels={}",
                        response.connector_id,
                        response.processed_messages,
                        response.queued_missions,
                        response.pending_approvals,
                        response.updated_channels
                    ),
                );
            }
            PickerAction::OpenSlackPicker => {
                self.open_slack_picker().await?;
            }
            PickerAction::OpenSlackActions(connector_id) => {
                self.open_slack_action_picker(&connector_id).await?;
            }
            PickerAction::ShowSlackDetails(connector_id) => {
                let connector: SlackConnectorConfig = self
                    .client
                    .get(&format!("/v1/slack/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\nbot_token_configured={}\nrequire_pairing_approval={}\nmonitored_channel_ids={}\nallowed_channel_ids={}\nallowed_user_ids={}\nchannel_cursors={}\nalias={}\nmodel={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.bot_token_keychain_account.is_some(),
                    connector.require_pairing_approval,
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    crate::format_slack_channel_cursors(&connector.channel_cursors),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Slack Connector", body);
            }
            PickerAction::ToggleSlackEnabled(connector_id, enabled) => {
                let mut connector: SlackConnectorConfig = self
                    .client
                    .get(&format!("/v1/slack/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: SlackConnectorConfig = self
                    .client
                    .post(
                        "/v1/slack",
                        &SlackConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Slack Connectors",
                    format!(
                        "{} slack connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollSlack(connector_id) => {
                let response: SlackPollResponse = self
                    .client
                    .post(
                        &format!("/v1/slack/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Slack Connectors",
                    format!(
                        "polled {}: processed {} message(s), queued {} mission(s), pending_approvals={}, updated_channels={}",
                        response.connector_id,
                        response.processed_messages,
                        response.queued_missions,
                        response.pending_approvals,
                        response.updated_channels
                    ),
                );
            }
            PickerAction::OpenSignalPicker => {
                self.open_signal_picker().await?;
            }
            PickerAction::OpenSignalActions(connector_id) => {
                self.open_signal_action_picker(&connector_id).await?;
            }
            PickerAction::ShowSignalDetails(connector_id) => {
                let connector: SignalConnectorConfig = self
                    .client
                    .get(&format!("/v1/signal/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\naccount={}\ncli_path={}\nrequire_pairing_approval={}\nmonitored_group_ids={}\nallowed_group_ids={}\nallowed_user_ids={}\nalias={}\nmodel={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.account,
                    connector
                        .cli_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "signal-cli".to_string()),
                    connector.require_pairing_approval,
                    crate::format_string_list(&connector.monitored_group_ids),
                    crate::format_string_list(&connector.allowed_group_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Signal Connector", body);
            }
            PickerAction::ToggleSignalEnabled(connector_id, enabled) => {
                let mut connector: SignalConnectorConfig = self
                    .client
                    .get(&format!("/v1/signal/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: SignalConnectorConfig = self
                    .client
                    .post(
                        "/v1/signal",
                        &SignalConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Signal Connectors",
                    format!(
                        "{} signal connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollSignal(connector_id) => {
                let response: SignalPollResponse = self
                    .client
                    .post(
                        &format!("/v1/signal/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Signal Connectors",
                    format!(
                        "polled {}: processed {} message(s), queued {} mission(s), pending_approvals={}",
                        response.connector_id,
                        response.processed_messages,
                        response.queued_missions,
                        response.pending_approvals
                    ),
                );
            }
            PickerAction::OpenHomeAssistantPicker => {
                self.open_home_assistant_picker().await?;
            }
            PickerAction::OpenHomeAssistantActions(connector_id) => {
                self.open_home_assistant_action_picker(&connector_id)
                    .await?;
            }
            PickerAction::ShowHomeAssistantDetails(connector_id) => {
                let connector: HomeAssistantConnectorConfig = self
                    .client
                    .get(&format!("/v1/home-assistant/{connector_id}"))
                    .await?;
                let body = format!(
                    "id={}\nname={}\nenabled={}\naccess_token_configured={}\nbase_url={}\nmonitored_entity_ids={}\nallowed_service_domains={}\nallowed_service_entity_ids={}\ntracked_entities={}\nalias={}\nmodel={}\ncwd={}",
                    connector.id,
                    connector.name,
                    connector.enabled,
                    connector.access_token_keychain_account.is_some(),
                    connector.base_url,
                    crate::format_string_list(&connector.monitored_entity_ids),
                    crate::format_string_list(&connector.allowed_service_domains),
                    crate::format_string_list(&connector.allowed_service_entity_ids),
                    crate::format_home_assistant_entity_cursors(&connector.entity_cursors),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                );
                self.open_static_overlay("Home Assistant Connector", body);
            }
            PickerAction::ToggleHomeAssistantEnabled(connector_id, enabled) => {
                let mut connector: HomeAssistantConnectorConfig = self
                    .client
                    .get(&format!("/v1/home-assistant/{connector_id}"))
                    .await?;
                connector.enabled = enabled;
                let updated: HomeAssistantConnectorConfig = self
                    .client
                    .post(
                        "/v1/home-assistant",
                        &HomeAssistantConnectorUpsertRequest {
                            connector: connector.clone(),
                            access_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Home Assistant Connectors",
                    format!(
                        "{} Home Assistant connector {}",
                        if updated.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        updated.name
                    ),
                );
            }
            PickerAction::PollHomeAssistant(connector_id) => {
                let response: HomeAssistantPollResponse = self
                    .client
                    .post(
                        &format!("/v1/home-assistant/{connector_id}/poll"),
                        &serde_json::json!({}),
                    )
                    .await?;
                self.open_static_overlay(
                    "Home Assistant Connectors",
                    format!(
                        "polled {}: processed {} entity update(s), queued {} mission(s), updated_entities={}",
                        response.connector_id,
                        response.processed_entities,
                        response.queued_missions,
                        response.updated_entities
                    ),
                );
            }
            PickerAction::ShowTelegramApprovals => {
                let approvals: Vec<ConnectorApprovalRecord> = self
                    .client
                    .get("/v1/connector-approvals?status=pending&limit=25")
                    .await?;
                self.open_static_overlay(
                    "Connector Approvals",
                    crate::format_connector_approvals(&approvals),
                );
            }
            PickerAction::SetDelegationDepth(limit) => {
                let updated: DelegationConfig = self
                    .client
                    .put(
                        "/v1/delegation/config",
                        &DelegationConfigUpdateRequest {
                            max_depth: Some(limit),
                            max_parallel_subagents: None,
                            disabled_provider_ids: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Delegation",
                    format!(
                        "max_depth={} max_parallel_subagents={}",
                        updated.max_depth, updated.max_parallel_subagents
                    ),
                );
            }
            PickerAction::SetDelegationParallel(limit) => {
                let updated: DelegationConfig = self
                    .client
                    .put(
                        "/v1/delegation/config",
                        &DelegationConfigUpdateRequest {
                            max_depth: None,
                            max_parallel_subagents: Some(limit),
                            disabled_provider_ids: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Delegation",
                    format!(
                        "max_depth={} max_parallel_subagents={}",
                        updated.max_depth, updated.max_parallel_subagents
                    ),
                );
            }
            PickerAction::ToggleProviderDelegation(provider_id, enabled) => {
                let mut delegation: DelegationConfig =
                    self.client.get("/v1/delegation/config").await?;
                if enabled {
                    delegation
                        .disabled_provider_ids
                        .retain(|id| id != &provider_id);
                } else if !delegation
                    .disabled_provider_ids
                    .iter()
                    .any(|id| id == &provider_id)
                {
                    delegation.disabled_provider_ids.push(provider_id.clone());
                }
                let updated: DelegationConfig = self
                    .client
                    .put(
                        "/v1/delegation/config",
                        &DelegationConfigUpdateRequest {
                            max_depth: None,
                            max_parallel_subagents: None,
                            disabled_provider_ids: Some(delegation.disabled_provider_ids),
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Delegation",
                    format!(
                        "{} delegation for {} (disabled providers: {})",
                        if enabled { "enabled" } else { "disabled" },
                        provider_id,
                        if updated.disabled_provider_ids.is_empty() {
                            "(none)".to_string()
                        } else {
                            updated.disabled_provider_ids.join(", ")
                        }
                    ),
                );
            }
            PickerAction::OpenPersistencePicker => {
                self.open_persistence_picker().await?;
            }
            PickerAction::SetPersistenceMode(mode) => {
                let updated: agent_core::DaemonConfig = self
                    .client
                    .put(
                        "/v1/daemon/config",
                        &DaemonConfigUpdateRequest {
                            persistence_mode: Some(mode),
                            auto_start: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Daemon",
                    format!(
                        "persistence={} auto_start={}",
                        updated.persistence_mode, updated.auto_start
                    ),
                );
            }
            PickerAction::ToggleAutoStart => {
                let status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
                let updated: agent_core::DaemonConfig = self
                    .client
                    .put(
                        "/v1/daemon/config",
                        &DaemonConfigUpdateRequest {
                            persistence_mode: None,
                            auto_start: Some(!status.auto_start),
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Daemon",
                    format!(
                        "persistence={} auto_start={}",
                        updated.persistence_mode, updated.auto_start
                    ),
                );
            }
        }
        Ok(())
    }

    async fn handle_slash_command(
        &mut self,
        command: InteractiveCommand,
        event_tx: &AppEventSender,
    ) -> Result<()> {
        match command {
            InteractiveCommand::Exit => {
                self.exit_requested = true;
            }
            InteractiveCommand::Help => {
                self.open_help_overlay();
            }
            InteractiveCommand::Status => {
                self.open_static_overlay("Status", self.status_text().await?);
            }
            InteractiveCommand::ConfigShow => {
                self.open_config_picker().await?;
            }
            InteractiveCommand::DashboardOpen => {
                self.pending_external_action = Some(ExternalAction::OpenDashboard);
            }
            InteractiveCommand::WebhooksShow => {
                self.open_webhook_picker().await?;
            }
            InteractiveCommand::InboxesShow => {
                self.open_inbox_picker().await?;
            }
            InteractiveCommand::DiscordsShow => {
                self.open_discord_picker().await?;
            }
            InteractiveCommand::SlacksShow => {
                self.open_slack_picker().await?;
            }
            InteractiveCommand::SignalsShow => {
                self.open_signal_picker().await?;
            }
            InteractiveCommand::HomeAssistantsShow => {
                self.open_home_assistant_picker().await?;
            }
            InteractiveCommand::AutopilotShow => {
                let status: AutopilotConfig = self.client.get("/v1/autopilot/status").await?;
                self.open_static_overlay("Autopilot", crate::autopilot_summary(&status));
            }
            InteractiveCommand::AutopilotEnable => {
                let status: AutopilotConfig = self
                    .client
                    .put(
                        "/v1/autopilot/status",
                        &AutopilotUpdateRequest {
                            state: Some(AutopilotState::Enabled),
                            max_concurrent_missions: None,
                            wake_interval_seconds: None,
                            allow_background_shell: None,
                            allow_background_network: None,
                            allow_background_self_edit: None,
                        },
                    )
                    .await?;
                self.open_static_overlay("Autopilot", crate::autopilot_summary(&status));
            }
            InteractiveCommand::AutopilotPause => {
                let status: AutopilotConfig = self
                    .client
                    .put(
                        "/v1/autopilot/status",
                        &AutopilotUpdateRequest {
                            state: Some(AutopilotState::Paused),
                            max_concurrent_missions: None,
                            wake_interval_seconds: None,
                            allow_background_shell: None,
                            allow_background_network: None,
                            allow_background_self_edit: None,
                        },
                    )
                    .await?;
                self.open_static_overlay("Autopilot", crate::autopilot_summary(&status));
            }
            InteractiveCommand::AutopilotResume => {
                let status: AutopilotConfig = self
                    .client
                    .put(
                        "/v1/autopilot/status",
                        &AutopilotUpdateRequest {
                            state: Some(AutopilotState::Enabled),
                            max_concurrent_missions: None,
                            wake_interval_seconds: None,
                            allow_background_shell: None,
                            allow_background_network: None,
                            allow_background_self_edit: None,
                        },
                    )
                    .await?;
                self.open_static_overlay("Autopilot", crate::autopilot_summary(&status));
            }
            InteractiveCommand::MissionsShow => {
                let missions: Vec<Mission> = self.client.get("/v1/missions").await?;
                let body = if missions.is_empty() {
                    "No missions queued.".to_string()
                } else {
                    missions
                        .into_iter()
                        .map(|mission| {
                            format!(
                                "{} [{:?}] {} wake_at={} repeat={} watch={} retries={}/{}",
                                mission.id,
                                mission.status,
                                mission.title,
                                mission
                                    .wake_at
                                    .map(|value| value.to_rfc3339())
                                    .unwrap_or_else(|| "-".to_string()),
                                mission
                                    .repeat_interval_seconds
                                    .map(|value| format!("{value}s"))
                                    .unwrap_or_else(|| "-".to_string()),
                                mission
                                    .watch_path
                                    .as_deref()
                                    .map(|value| value.display().to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                                mission.retries,
                                mission.max_retries
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Missions", body);
            }
            InteractiveCommand::EventsShow(limit) => {
                let events: Vec<LogEntry> = self
                    .client
                    .get(&format!("/v1/events?limit={limit}"))
                    .await?;
                self.apply_daemon_events(events);
                self.open_static_overlay("Events", self.rendered_event_body());
            }
            InteractiveCommand::Schedule {
                after_seconds,
                title,
            } => {
                let mut mission = Mission::new(title, String::new());
                mission.status = MissionStatus::Scheduled;
                mission.wake_at =
                    Some(chrono::Utc::now() + chrono::Duration::seconds(after_seconds as i64));
                mission.wake_trigger = Some(WakeTrigger::Timer);
                mission.workspace_key = Some(self.cwd.display().to_string());
                let mission: Mission = self.client.post("/v1/missions", &mission).await?;
                self.open_static_overlay(
                    "Mission Scheduled",
                    format!(
                        "{} [{:?}] wake_at={}",
                        mission.title,
                        mission.status,
                        mission
                            .wake_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_else(|| "-".to_string())
                    ),
                );
            }
            InteractiveCommand::Repeat {
                every_seconds,
                title,
            } => {
                let mut mission = Mission::new(title, String::new());
                mission.status = MissionStatus::Scheduled;
                mission.wake_at =
                    Some(chrono::Utc::now() + chrono::Duration::seconds(every_seconds as i64));
                mission.repeat_interval_seconds = Some(every_seconds);
                mission.wake_trigger = Some(WakeTrigger::Timer);
                mission.workspace_key = Some(self.cwd.display().to_string());
                let mission: Mission = self.client.post("/v1/missions", &mission).await?;
                self.open_static_overlay(
                    "Mission Scheduled",
                    format!(
                        "{} [{:?}] wake_at={} repeat={}",
                        mission.title,
                        mission.status,
                        mission
                            .wake_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_else(|| "-".to_string()),
                        mission
                            .repeat_interval_seconds
                            .map(|value| format!("{value}s"))
                            .unwrap_or_else(|| "-".to_string())
                    ),
                );
            }
            InteractiveCommand::Watch { path, title } => {
                let watch_path = if path.is_absolute() {
                    path
                } else {
                    self.cwd.join(path)
                };
                let mut mission = Mission::new(title, String::new());
                mission.status = MissionStatus::Waiting;
                mission.wake_trigger = Some(WakeTrigger::FileChange);
                mission.workspace_key = Some(self.cwd.display().to_string());
                mission.watch_path = Some(watch_path);
                mission.watch_recursive = true;
                let mission: Mission = self.client.post("/v1/missions", &mission).await?;
                self.open_static_overlay(
                    "Mission Watch",
                    format!(
                        "{} [{:?}] watch={}",
                        mission.title,
                        mission.status,
                        mission
                            .watch_path
                            .as_deref()
                            .map(|value| value.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    ),
                );
            }
            InteractiveCommand::ProfileShow => {
                self.activate_picker_action(PickerAction::ShowResidentProfile)
                    .await?;
            }
            InteractiveCommand::TelegramsShow => {
                let connectors = crate::load_telegram_connectors(self.storage).await?;
                let body = if connectors.is_empty() {
                    "No telegram connectors configured.".to_string()
                } else {
                    connectors
                        .into_iter()
                        .map(|connector| {
                            format!(
                                "{} [{}] enabled={} require_pairing_approval={} chats={} users={} alias={} model={}",
                                connector.id,
                                connector.name,
                                connector.enabled,
                                connector.require_pairing_approval,
                                crate::format_i64_list(&connector.allowed_chat_ids),
                                crate::format_i64_list(&connector.allowed_user_ids),
                                connector.alias.as_deref().unwrap_or("-"),
                                connector.requested_model.as_deref().unwrap_or("-")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Telegram Connectors", body);
            }
            InteractiveCommand::TelegramApprovalsShow => {
                let approvals: Vec<ConnectorApprovalRecord> = self
                    .client
                    .get("/v1/connector-approvals?kind=telegram&status=pending&limit=25")
                    .await?;
                self.open_static_overlay(
                    "Telegram Approvals",
                    crate::format_connector_approvals(&approvals),
                );
            }
            InteractiveCommand::TelegramApprove { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/approve"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Telegram Approvals",
                    format!(
                        "approved telegram pairing={} connector={} chat={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::DiscordApprovalsShow => {
                let approvals: Vec<ConnectorApprovalRecord> = self
                    .client
                    .get("/v1/connector-approvals?kind=discord&status=pending&limit=25")
                    .await?;
                self.open_static_overlay(
                    "Discord Approvals",
                    crate::format_connector_approvals(&approvals),
                );
            }
            InteractiveCommand::DiscordApprove { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/approve"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Discord Approvals",
                    format!(
                        "approved discord pairing={} connector={} channel={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::DiscordReject { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/reject"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Discord Approvals",
                    format!(
                        "rejected discord pairing={} connector={} channel={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::SlackApprovalsShow => {
                let approvals: Vec<ConnectorApprovalRecord> = self
                    .client
                    .get("/v1/connector-approvals?kind=slack&status=pending&limit=25")
                    .await?;
                self.open_static_overlay(
                    "Slack Approvals",
                    crate::format_connector_approvals(&approvals),
                );
            }
            InteractiveCommand::SlackApprove { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/approve"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Slack Approvals",
                    format!(
                        "approved slack pairing={} connector={} channel={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::SlackReject { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/reject"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Slack Approvals",
                    format!(
                        "rejected slack pairing={} connector={} channel={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::TelegramReject { id, note } => {
                let approval: ConnectorApprovalRecord = self
                    .client
                    .post(
                        &format!("/v1/connector-approvals/{id}/reject"),
                        &ConnectorApprovalUpdateRequest { note },
                    )
                    .await?;
                self.open_static_overlay(
                    "Telegram Approvals",
                    format!(
                        "rejected telegram pairing={} connector={} chat={} user={}",
                        approval.id,
                        approval.connector_id,
                        approval.external_chat_display.as_deref().unwrap_or("-"),
                        approval.external_user_display.as_deref().unwrap_or("-")
                    ),
                );
            }
            InteractiveCommand::MemoryReviewShow => {
                let memories: Vec<MemoryRecord> =
                    self.client.get("/v1/memory/review?limit=25").await?;
                let body = if memories.is_empty() {
                    "No candidate memory awaiting review.".to_string()
                } else {
                    memories
                        .into_iter()
                        .map(|memory| {
                            let note = memory
                                .review_note
                                .as_deref()
                                .map(|value| format!("\n  note: {value}"))
                                .unwrap_or_default();
                            format!(
                                "{} [{:?}/{:?} review={:?}] {}\n  {}{}",
                                memory.id,
                                memory.kind,
                                memory.scope,
                                memory.review_status,
                                memory.subject,
                                memory.content,
                                note
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Memory Review", body);
            }
            InteractiveCommand::MemoryShow(query) => {
                if let Some(query) = query {
                    let result: MemorySearchResponse = self
                        .client
                        .post(
                            "/v1/memory/search",
                            &MemorySearchQuery {
                                query,
                                limit: Some(10),
                                workspace_key: Some(self.cwd.display().to_string()),
                                provider_id: None,
                                review_statuses: Vec::new(),
                                include_superseded: false,
                            },
                        )
                        .await?;
                    let mut lines = Vec::new();
                    for memory in result.memories {
                        lines.push(format!(
                            "{} [{:?}/{:?}] {}\n  {}",
                            memory.id, memory.kind, memory.scope, memory.subject, memory.content
                        ));
                    }
                    for hit in result.transcript_hits {
                        lines.push(format!(
                            "session={} [{:?}] {}",
                            hit.session_id, hit.role, hit.preview
                        ));
                    }
                    self.open_static_overlay(
                        "Memory Search",
                        if lines.is_empty() {
                            "No matching memory.".to_string()
                        } else {
                            lines.join("\n")
                        },
                    );
                } else {
                    let memories: Vec<MemoryRecord> =
                        self.client.get("/v1/memory?limit=10").await?;
                    let body = if memories.is_empty() {
                        "No stored memory.".to_string()
                    } else {
                        memories
                            .into_iter()
                            .map(|memory| {
                                format!(
                                    "{} [{:?}/{:?}] {}\n  {}",
                                    memory.id,
                                    memory.kind,
                                    memory.scope,
                                    memory.subject,
                                    memory.content
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    self.open_static_overlay("Memory", body);
                }
            }
            InteractiveCommand::MemoryApprove { id, note } => {
                let memory: MemoryRecord = self
                    .client
                    .post(
                        &format!("/v1/memory/{id}/approve"),
                        &MemoryReviewUpdateRequest {
                            status: MemoryReviewStatus::Accepted,
                            note,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Memory Review",
                    format!("approved memory={} subject={}", memory.id, memory.subject),
                );
            }
            InteractiveCommand::MemoryReject { id, note } => {
                let memory: MemoryRecord = self
                    .client
                    .post(
                        &format!("/v1/memory/{id}/reject"),
                        &MemoryReviewUpdateRequest {
                            status: MemoryReviewStatus::Rejected,
                            note,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Memory Review",
                    format!("rejected memory={} subject={}", memory.id, memory.subject),
                );
            }
            InteractiveCommand::Skills(command) => match command {
                crate::InteractiveSkillCommand::Show(status) => {
                    self.open_skill_draft_picker(status).await?;
                }
                crate::InteractiveSkillCommand::Publish(id) => {
                    self.activate_picker_action(PickerAction::PublishSkillDraft(id))
                        .await?;
                }
                crate::InteractiveSkillCommand::Reject(id) => {
                    self.activate_picker_action(PickerAction::RejectSkillDraft(id))
                        .await?;
                }
            },
            InteractiveCommand::Remember(content) => {
                let subject = crate::manual_memory_subject(&content);
                let memory: MemoryRecord = self
                    .client
                    .post(
                        "/v1/memory",
                        &MemoryUpsertRequest {
                            kind: MemoryKind::Note,
                            scope: MemoryScope::Global,
                            subject,
                            content,
                            confidence: Some(100),
                            source_session_id: self.session_id.clone(),
                            source_message_id: None,
                            provider_id: None,
                            workspace_key: Some(self.cwd.display().to_string()),
                            tags: vec!["manual".to_string()],
                            identity_key: None,
                            observation_source: None,
                            review_status: Some(MemoryReviewStatus::Accepted),
                            review_note: None,
                            reviewed_at: None,
                            supersedes: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Memory",
                    format!("stored memory={} subject={}", memory.id, memory.subject),
                );
            }
            InteractiveCommand::Forget(id) => {
                let _: serde_json::Value = self.client.delete(&format!("/v1/memory/{id}")).await?;
                self.open_static_overlay("Memory", format!("forgot memory={id}"));
            }
            InteractiveCommand::PermissionsShow => {
                self.open_permission_picker();
            }
            InteractiveCommand::PermissionsSet(preset) => {
                let preset = preset.unwrap_or(self.storage.load_config()?.permission_preset);
                self.activate_picker_action(PickerAction::SetPermission(preset))
                    .await?;
            }
            InteractiveCommand::Attach(path) => {
                let mut new = collect_image_attachments(&self.cwd, &[path])?;
                self.attachments.append(&mut new);
                self.open_static_overlay(
                    "Attachments",
                    format!("attachments={}", self.attachments.len()),
                );
            }
            InteractiveCommand::AttachmentsShow => {
                let body = if self.attachments.is_empty() {
                    "attachments=(none)".to_string()
                } else {
                    self.attachments
                        .iter()
                        .map(|attachment| attachment.path.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                self.open_static_overlay("Attachments", body);
            }
            InteractiveCommand::AttachmentsClear => {
                self.attachments.clear();
                self.open_static_overlay("Attachments", "attachments cleared".to_string());
            }
            InteractiveCommand::New | InteractiveCommand::Clear => {
                self.session_id = None;
                self.transcript.clear();
                self.transcript_scroll_back = 0;
                self.requested_model = None;
                self.open_static_overlay("Session", "Started a new chat session.".to_string());
            }
            InteractiveCommand::Diff => {
                self.open_static_overlay("Git Diff", crate::build_uncommitted_diff()?);
            }
            InteractiveCommand::Copy => {
                let text = self
                    .transcript
                    .iter()
                    .rev()
                    .find(|message| message.role == agent_core::MessageRole::Assistant)
                    .map(|message| message.content.clone())
                    .ok_or_else(|| anyhow!("no assistant output available to copy"))?;
                copy_to_clipboard(&text)?;
                self.open_static_overlay(
                    "Clipboard",
                    "Copied the latest assistant output.".to_string(),
                );
            }
            InteractiveCommand::Compact => {
                let current_session = self
                    .session_id
                    .clone()
                    .ok_or_else(|| anyhow!("no active session to compact"))?;
                let transcript = SessionTranscript {
                    session: self
                        .storage
                        .list_sessions(SESSION_PICKER_LIMIT)?
                        .into_iter()
                        .find(|entry| entry.id == current_session)
                        .ok_or_else(|| anyhow!("unknown session"))?,
                    messages: self.storage.list_session_messages(&current_session)?,
                };
                let prompt = build_compact_prompt(&transcript)?;
                let response = execute_prompt(
                    &self.client,
                    prompt,
                    self.alias.clone(),
                    self.requested_model.clone(),
                    None,
                    self.cwd.clone(),
                    self.thinking_level,
                    Vec::new(),
                    self.permission_preset,
                    None,
                    true,
                )
                .await?;
                let new_session_id =
                    compact_session(self.storage, &transcript, &response.response)?;
                self.session_id = Some(new_session_id.clone());
                self.transcript = self.storage.list_session_messages(&new_session_id)?;
                self.transcript_scroll_back = 0;
                self.open_static_overlay(
                    "Compact",
                    format!("Compacted into session {new_session_id}"),
                );
            }
            InteractiveCommand::Init => {
                let path = self.cwd.join("AGENTS.md");
                if init_agents_file(&path)? {
                    self.open_static_overlay("Init", format!("Initialized {}", path.display()));
                } else {
                    self.open_static_overlay("Init", format!("{} already exists.", path.display()));
                }
            }
            InteractiveCommand::ModelShow => {
                self.open_model_picker().await?;
            }
            InteractiveCommand::ModelSet(selection) => {
                match resolve_interactive_model_selection(
                    self.storage,
                    self.alias.as_deref(),
                    &selection,
                )
                .await?
                {
                    InteractiveModelSelection::Alias(new_alias) => {
                        self.alias = Some(new_alias.clone());
                        self.requested_model = None;
                        self.refresh_active_model_metadata().await?;
                        self.open_static_overlay(
                            "Models",
                            format!("model alias set to {new_alias}"),
                        );
                    }
                    InteractiveModelSelection::Explicit(model_id) => {
                        self.requested_model = resolve_requested_model_override(
                            self.storage,
                            self.alias.as_deref(),
                            &model_id,
                        )?;
                        self.refresh_active_model_metadata().await?;
                        self.open_static_overlay(
                            "Models",
                            format!("model override set to {model_id}"),
                        );
                    }
                }
            }
            InteractiveCommand::ThinkingShow => {
                self.open_thinking_picker().await?;
            }
            InteractiveCommand::ThinkingSet(new_level) => {
                self.activate_picker_action(PickerAction::SetThinking(new_level))
                    .await?;
            }
            InteractiveCommand::Fast => {
                self.activate_picker_action(PickerAction::SetThinking(Some(
                    ThinkingLevel::Minimal,
                )))
                .await?;
            }
            InteractiveCommand::Rename(title) => {
                let session_id = self
                    .session_id
                    .as_deref()
                    .ok_or_else(|| anyhow!("no active session to rename"))?;
                let title = title.ok_or_else(|| anyhow!("usage: /rename <title>"))?;
                self.storage.rename_session(session_id, &title)?;
                self.open_static_overlay(
                    "Session",
                    format!("renamed session={} title={}", session_id, title),
                );
            }
            InteractiveCommand::Review(custom_prompt) => {
                let prompt = build_uncommitted_review_prompt(custom_prompt)?;
                self.queue_prompt(prompt, event_tx)?;
            }
            InteractiveCommand::Resume(target) => {
                if let Some(target) = target {
                    let transcript = load_transcript_for_interactive_resume(
                        self.storage,
                        Some(target.as_str()),
                    )?;
                    self.resume_session(transcript.session)?;
                    self.refresh_active_model_metadata().await?;
                } else {
                    self.open_picker(PickerMode::Resume).await?;
                }
            }
            InteractiveCommand::Fork(target) => {
                if let Some(target) = target {
                    let transcript = load_transcript_for_interactive_fork(
                        self.storage,
                        self.session_id.as_deref(),
                        Some(target.as_str()),
                    )?;
                    let new_session_id = fork_session(self.storage, &transcript)?;
                    let session = self
                        .storage
                        .get_session(&new_session_id)?
                        .ok_or_else(|| anyhow!("forked session not found"))?;
                    self.resume_session(session)?;
                    self.refresh_active_model_metadata().await?;
                    self.open_static_overlay("Fork", format!("Forked session {new_session_id}"));
                } else {
                    self.open_picker(PickerMode::Fork).await?;
                }
            }
        }

        Ok(())
    }

    async fn complete_prompt(&mut self, response: RunTaskResponse) -> Result<()> {
        self.session_id = Some(response.session_id.clone());
        self.alias = Some(response.alias.clone());
        self.requested_model =
            resolve_requested_model_override(self.storage, self.alias.as_deref(), &response.model)?;
        self.attachments.clear();
        self.transcript = self.storage.list_session_messages(&response.session_id)?;
        self.transcript_scroll_back = 0;
        self.overlay = None;
        self.refresh_active_model_metadata().await?;
        Ok(())
    }

    async fn open_picker(&mut self, mode: PickerMode) -> Result<()> {
        let sessions =
            rank_sessions_for_picker(self.storage.list_sessions(SESSION_PICKER_LIMIT)?, false)?;
        self.picker = Some(PickerState {
            mode,
            title: match mode {
                PickerMode::Resume => "Resume a previous session".to_string(),
                PickerMode::Fork => "Fork a previous session".to_string(),
                PickerMode::Model
                | PickerMode::Alias
                | PickerMode::Thinking
                | PickerMode::Permissions
                | PickerMode::Config
                | PickerMode::Delegation
                | PickerMode::Autonomy
                | PickerMode::Provider
                | PickerMode::ProviderAction
                | PickerMode::Webhook
                | PickerMode::WebhookAction
                | PickerMode::Inbox
                | PickerMode::InboxAction
                | PickerMode::Telegram
                | PickerMode::TelegramAction
                | PickerMode::Discord
                | PickerMode::DiscordAction
                | PickerMode::Slack
                | PickerMode::SlackAction
                | PickerMode::Signal
                | PickerMode::SignalAction
                | PickerMode::HomeAssistant
                | PickerMode::HomeAssistantAction
                | PickerMode::Persistence
                | PickerMode::SkillDraft
                | PickerMode::SkillDraftAction => "Picker".to_string(),
            },
            hint: "Enter select | Esc cancel | PageUp/PageDown jump | Mouse wheel scroll"
                .to_string(),
            empty_message: "No matching session.".to_string(),
            query: String::new(),
            selected: 0,
            sessions,
            models: Vec::new(),
            items: Vec::new(),
        });
        Ok(())
    }

    fn open_generic_picker(
        &mut self,
        mode: PickerMode,
        title: impl Into<String>,
        hint: impl Into<String>,
        empty_message: impl Into<String>,
        items: Vec<GenericPickerEntry>,
    ) {
        self.picker = Some(PickerState {
            mode,
            title: title.into(),
            hint: hint.into(),
            empty_message: empty_message.into(),
            query: String::new(),
            selected: 0,
            sessions: Vec::new(),
            models: Vec::new(),
            items,
        });
    }

    async fn open_model_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .get_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();

        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), provider),
        )
        .await;

        let mut models = match listed {
            Ok(Ok(models)) => models
                .into_iter()
                .filter(|model| model.show_in_picker)
                .map(|model| ModelPickerEntry {
                    display_name: model
                        .display_name
                        .clone()
                        .unwrap_or_else(|| model.id.clone()),
                    id: model.id,
                    description: model.description,
                    context_window: model.context_window,
                    effective_context_window_percent: model.effective_context_window_percent,
                })
                .collect::<Vec<_>>(),
            Ok(Err(error)) => {
                self.open_static_overlay(
                    "Models",
                    format!("provider models unavailable: {error:#}"),
                );
                return Ok(());
            }
            Err(_) => {
                self.open_static_overlay(
                    "Models",
                    "provider models unavailable: request timed out".to_string(),
                );
                return Ok(());
            }
        };

        if models.is_empty() {
            models.push(ModelPickerEntry {
                id: selected_model.clone(),
                display_name: selected_model.clone(),
                description: Some("Current model".to_string()),
                context_window: self.context_window_tokens,
                effective_context_window_percent: self.context_window_percent,
            });
        }

        let selected = models
            .iter()
            .position(|entry| entry.id == selected_model)
            .unwrap_or(0);

        self.picker = Some(PickerState {
            mode: PickerMode::Model,
            title: "Select a model".to_string(),
            hint: "Enter select | Type to filter | Esc cancel | PageUp/PageDown jump | Mouse wheel scroll".to_string(),
            empty_message: "No matching model.".to_string(),
            query: String::new(),
            selected,
            sessions: Vec::new(),
            models,
            items: Vec::new(),
        });
        Ok(())
    }

    async fn open_alias_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let current_main_alias = config.main_agent_alias.clone();
        let current_alias = self.alias.clone();
        let mut items = config
            .aliases
            .iter()
            .map(|alias| GenericPickerEntry {
                label: alias.alias.clone(),
                detail: Some(format!("{} / {}", alias.provider_id, alias.model)),
                search_text: format!(
                    "{} {} {} {}",
                    alias.alias,
                    alias.provider_id,
                    alias.model,
                    alias.description.as_deref().unwrap_or_default()
                ),
                current: current_main_alias.as_deref() == Some(alias.alias.as_str())
                    || current_alias.as_deref() == Some(alias.alias.as_str()),
                action: PickerAction::SetAlias(alias.alias.clone()),
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.label.cmp(&right.label));
        self.open_generic_picker(
            PickerMode::Alias,
            "Select a default alias",
            "Enter select | Type to filter | Esc cancel",
            "No matching alias.",
            items,
        );
        Ok(())
    }

    async fn open_thinking_picker(&mut self) -> Result<()> {
        let descriptor = self.active_model_descriptor().await?;
        let options = build_thinking_picker_entries(descriptor.as_ref(), self.thinking_level);
        self.open_generic_picker(
            PickerMode::Thinking,
            "Select thinking level",
            "Enter select | Type to filter | Esc cancel",
            "No matching thinking level.",
            options,
        );
        Ok(())
    }

    fn open_permission_picker(&mut self) {
        let current = self.permission_preset.unwrap_or(PermissionPreset::AutoEdit);
        let items = [
            (
                PermissionPreset::Suggest,
                "suggest",
                "Ask before edits and riskier actions.",
            ),
            (
                PermissionPreset::AutoEdit,
                "auto-edit",
                "Allow routine edits without stopping.",
            ),
            (
                PermissionPreset::FullAuto,
                "full-auto",
                "Take actions aggressively with fewer stops.",
            ),
        ]
        .into_iter()
        .map(|(preset, label, detail)| GenericPickerEntry {
            label: label.to_string(),
            detail: Some(detail.to_string()),
            search_text: format!("{label} {detail}"),
            current: current == preset,
            action: PickerAction::SetPermission(preset),
        })
        .collect();
        self.open_generic_picker(
            PickerMode::Permissions,
            "Select permission preset",
            "Enter select | Esc cancel",
            "No matching permission preset.",
            items,
        );
    }

    async fn open_delegation_picker(&mut self) -> Result<()> {
        let status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Show delegation targets".to_string(),
                detail: Some(format!("{} target(s) available", status.delegation_targets)),
                search_text: "delegation targets aliases providers".to_string(),
                current: false,
                action: PickerAction::ShowDelegationTargets,
            },
            GenericPickerEntry {
                label: "Max delegation depth: 1".to_string(),
                detail: Some("Default: parent can spawn subagents, but those children cannot fan out further.".to_string()),
                search_text: "delegation depth 1 default".to_string(),
                current: matches!(status.delegation.max_depth, DelegationLimit::Limited { value: 1 }),
                action: PickerAction::SetDelegationDepth(DelegationLimit::Limited { value: 1 }),
            },
            GenericPickerEntry {
                label: "Max delegation depth: 2".to_string(),
                detail: Some("Allow subagents to spawn one extra layer.".to_string()),
                search_text: "delegation depth 2 recursion".to_string(),
                current: matches!(status.delegation.max_depth, DelegationLimit::Limited { value: 2 }),
                action: PickerAction::SetDelegationDepth(DelegationLimit::Limited { value: 2 }),
            },
            GenericPickerEntry {
                label: "Max delegation depth: 3".to_string(),
                detail: Some("Allow deeper recursive delegation with limits.".to_string()),
                search_text: "delegation depth 3 recursion".to_string(),
                current: matches!(status.delegation.max_depth, DelegationLimit::Limited { value: 3 }),
                action: PickerAction::SetDelegationDepth(DelegationLimit::Limited { value: 3 }),
            },
            GenericPickerEntry {
                label: "Max delegation depth: unlimited".to_string(),
                detail: Some("No fixed recursion depth; still bounded by other runtime caps.".to_string()),
                search_text: "delegation depth unlimited recursion".to_string(),
                current: matches!(status.delegation.max_depth, DelegationLimit::Unlimited),
                action: PickerAction::SetDelegationDepth(DelegationLimit::Unlimited),
            },
            GenericPickerEntry {
                label: "Max parallel subagents: 4".to_string(),
                detail: Some("Keep fanout small and predictable.".to_string()),
                search_text: "parallel subagents 4".to_string(),
                current: matches!(status.delegation.max_parallel_subagents, DelegationLimit::Limited { value: 4 }),
                action: PickerAction::SetDelegationParallel(DelegationLimit::Limited { value: 4 }),
            },
            GenericPickerEntry {
                label: "Max parallel subagents: 8".to_string(),
                detail: Some("Balanced default for cross-provider fanout.".to_string()),
                search_text: "parallel subagents 8 default".to_string(),
                current: matches!(status.delegation.max_parallel_subagents, DelegationLimit::Limited { value: 8 }),
                action: PickerAction::SetDelegationParallel(DelegationLimit::Limited { value: 8 }),
            },
            GenericPickerEntry {
                label: "Max parallel subagents: 16".to_string(),
                detail: Some("Allow larger multi-provider batches.".to_string()),
                search_text: "parallel subagents 16".to_string(),
                current: matches!(status.delegation.max_parallel_subagents, DelegationLimit::Limited { value: 16 }),
                action: PickerAction::SetDelegationParallel(DelegationLimit::Limited { value: 16 }),
            },
            GenericPickerEntry {
                label: "Max parallel subagents: unlimited".to_string(),
                detail: Some("No fixed parallel cap; other daemon limits still apply.".to_string()),
                search_text: "parallel subagents unlimited".to_string(),
                current: matches!(status.delegation.max_parallel_subagents, DelegationLimit::Unlimited),
                action: PickerAction::SetDelegationParallel(DelegationLimit::Unlimited),
            },
        ];
        self.open_generic_picker(
            PickerMode::Delegation,
            "Delegation",
            "Enter select | Type to filter | Esc cancel",
            "No matching delegation setting.",
            items,
        );
        Ok(())
    }

    async fn open_config_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let daemon_status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        let items = vec![
            GenericPickerEntry {
                label: "Providers & Login".to_string(),
                detail: Some(format!("{} provider(s), {} alias(es)", config.providers.len(), config.aliases.len())),
                search_text: "providers login browser oauth api key aliases main alias".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::Providers),
            },
            GenericPickerEntry {
                label: "Model & Thinking".to_string(),
                detail: Some("Active model, overrides, and reasoning level".to_string()),
                search_text: "model thinking reasoning effort fast".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::ModelThinking),
            },
            GenericPickerEntry {
                label: "Permissions".to_string(),
                detail: Some("Approval preset and trust toggles".to_string()),
                search_text: "permissions approvals shell network disk self edit".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::Permissions),
            },
            GenericPickerEntry {
                label: "Connectors".to_string(),
                detail: Some(format!(
                    "{} total connector(s)",
                    daemon_status.telegram_connectors
                        + daemon_status.discord_connectors
                        + daemon_status.slack_connectors
                        + daemon_status.signal_connectors
                        + daemon_status.home_assistant_connectors
                        + daemon_status.webhook_connectors
                        + daemon_status.inbox_connectors
                        + daemon_status.gmail_connectors
                        + daemon_status.brave_connectors
                )),
                search_text: "connectors telegram discord slack signal home assistant webhook inbox gmail brave approvals".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::Connectors),
            },
            GenericPickerEntry {
                label: "Autonomy".to_string(),
                detail: Some(format!("{} mission(s), {} active", daemon_status.missions, daemon_status.active_missions)),
                search_text: "autonomy autopilot missions scheduling free thinking".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::Autonomy),
            },
            GenericPickerEntry {
                label: "Memory & Skills".to_string(),
                detail: Some(format!(
                    "{} memories, {} draft skill(s)",
                    daemon_status.memories, daemon_status.skill_drafts
                )),
                search_text: "memory resident profile skills learning review".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::MemorySkills),
            },
            GenericPickerEntry {
                label: "Delegation".to_string(),
                detail: Some(format!("{} target(s) available", daemon_status.delegation_targets)),
                search_text: "delegation subagents other providers depth parallel".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::Delegation),
            },
            GenericPickerEntry {
                label: "System".to_string(),
                detail: Some("Dashboard, persistence, and daemon startup".to_string()),
                search_text: "system dashboard persistence autostart daemon".to_string(),
                current: false,
                action: PickerAction::OpenSettingsSection(SettingsSection::System),
            },
        ];
        self.open_generic_picker(
            PickerMode::Config,
            "Settings",
            "Enter open | Type filter | Esc cancel",
            "No matching setting.",
            items,
        );
        Ok(())
    }

    async fn open_settings_section_picker(&mut self, section: SettingsSection) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let daemon_status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        let autonomy: AutonomyProfile = self.client.get("/v1/autonomy/status").await?;
        let autopilot: AutopilotConfig = self.client.get("/v1/autopilot/status").await?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();

        let mut items = vec![GenericPickerEntry {
            label: "Back to settings".to_string(),
            detail: Some("Return to the main settings menu.".to_string()),
            search_text: "back settings menu".to_string(),
            current: false,
            action: PickerAction::OpenConfig,
        }];

        match section {
            SettingsSection::Providers => items.extend([
                GenericPickerEntry {
                    label: "Manage providers & login".to_string(),
                    detail: Some(format!("{} provider(s) configured", config.providers.len())),
                    search_text: "providers browser oauth api key login".to_string(),
                    current: false,
                    action: PickerAction::OpenProviderPicker,
                },
                GenericPickerEntry {
                    label: "Default alias / provider".to_string(),
                    detail: Some(format!(
                        "{} -> {} / {}",
                        active_alias.alias, active_alias.provider_id, active_alias.model
                    )),
                    search_text: format!(
                        "default alias provider {} {} {}",
                        active_alias.alias, active_alias.provider_id, active_alias.model
                    ),
                    current: false,
                    action: PickerAction::OpenAliasPicker,
                },
            ]),
            SettingsSection::ModelThinking => items.extend([
                GenericPickerEntry {
                    label: "Active model".to_string(),
                    detail: Some(selected_model),
                    search_text: "model active override".to_string(),
                    current: false,
                    action: PickerAction::OpenModelPicker,
                },
                GenericPickerEntry {
                    label: "Thinking".to_string(),
                    detail: Some(thinking_level_label(self.thinking_level).to_string()),
                    search_text: "thinking reasoning effort fast".to_string(),
                    current: false,
                    action: PickerAction::OpenThinkingPicker,
                },
            ]),
            SettingsSection::Permissions => items.extend([
                GenericPickerEntry {
                    label: "Permission preset".to_string(),
                    detail: Some(
                        permission_summary(
                            self.permission_preset.unwrap_or(config.permission_preset),
                        )
                        .to_string(),
                    ),
                    search_text: "permissions approvals preset".to_string(),
                    current: false,
                    action: PickerAction::OpenPermissionPicker,
                },
                GenericPickerEntry {
                    label: "Shell access".to_string(),
                    detail: Some(boolean_status(config.trust_policy.allow_shell)),
                    search_text: "shell trust allow shell".to_string(),
                    current: false,
                    action: PickerAction::ToggleTrust(TrustToggle::Shell),
                },
                GenericPickerEntry {
                    label: "Network access".to_string(),
                    detail: Some(boolean_status(config.trust_policy.allow_network)),
                    search_text: "network trust allow network".to_string(),
                    current: false,
                    action: PickerAction::ToggleTrust(TrustToggle::Network),
                },
                GenericPickerEntry {
                    label: "Full disk access".to_string(),
                    detail: Some(boolean_status(config.trust_policy.allow_full_disk)),
                    search_text: "full disk trust allow full disk".to_string(),
                    current: false,
                    action: PickerAction::ToggleTrust(TrustToggle::FullDisk),
                },
                GenericPickerEntry {
                    label: "Self edit".to_string(),
                    detail: Some(boolean_status(config.trust_policy.allow_self_edit)),
                    search_text: "self edit trust".to_string(),
                    current: false,
                    action: PickerAction::ToggleTrust(TrustToggle::SelfEdit),
                },
            ]),
            SettingsSection::Connectors => items.extend([
                GenericPickerEntry {
                    label: "Set up Telegram connector".to_string(),
                    detail: Some("Guided setup for a Telegram bot connector.".to_string()),
                    search_text: "setup telegram connector bot add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddTelegramConnector),
                },
                GenericPickerEntry {
                    label: "Set up Discord connector".to_string(),
                    detail: Some("Guided setup for a Discord bot connector.".to_string()),
                    search_text: "setup discord connector bot add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddDiscordConnector),
                },
                GenericPickerEntry {
                    label: "Set up Slack connector".to_string(),
                    detail: Some("Guided setup for a Slack bot connector.".to_string()),
                    search_text: "setup slack connector bot add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddSlackConnector),
                },
                GenericPickerEntry {
                    label: "Set up Signal connector".to_string(),
                    detail: Some("Guided setup for a Signal connector.".to_string()),
                    search_text: "setup signal connector add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddSignalConnector),
                },
                GenericPickerEntry {
                    label: "Set up Home Assistant connector".to_string(),
                    detail: Some("Guided setup for a Home Assistant connector.".to_string()),
                    search_text: "setup home assistant connector add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddHomeAssistantConnector),
                },
                GenericPickerEntry {
                    label: "Set up Webhook connector".to_string(),
                    detail: Some("Guided setup for an inbound webhook connector.".to_string()),
                    search_text: "setup webhook connector add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddWebhookConnector),
                },
                GenericPickerEntry {
                    label: "Set up Inbox connector".to_string(),
                    detail: Some("Guided setup for a local inbox connector.".to_string()),
                    search_text: "setup inbox connector add".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::AddInboxConnector),
                },
                GenericPickerEntry {
                    label: "Connector approvals".to_string(),
                    detail: Some(format!(
                        "{} pending pairing request(s)",
                        daemon_status.pending_connector_approvals
                    )),
                    search_text: "connector approvals pairing pending".to_string(),
                    current: false,
                    action: PickerAction::ShowTelegramApprovals,
                },
                GenericPickerEntry {
                    label: "Telegram connectors".to_string(),
                    detail: Some(format!(
                        "{} connector(s)",
                        daemon_status.telegram_connectors
                    )),
                    search_text: "telegram bot connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenTelegramPicker,
                },
                GenericPickerEntry {
                    label: "Discord connectors".to_string(),
                    detail: Some(format!("{} connector(s)", daemon_status.discord_connectors)),
                    search_text: "discord bot connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenDiscordPicker,
                },
                GenericPickerEntry {
                    label: "Slack connectors".to_string(),
                    detail: Some(format!("{} connector(s)", daemon_status.slack_connectors)),
                    search_text: "slack bot connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenSlackPicker,
                },
                GenericPickerEntry {
                    label: "Signal connectors".to_string(),
                    detail: Some(format!("{} connector(s)", daemon_status.signal_connectors)),
                    search_text: "signal connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenSignalPicker,
                },
                GenericPickerEntry {
                    label: "Home Assistant connectors".to_string(),
                    detail: Some(format!(
                        "{} connector(s)",
                        daemon_status.home_assistant_connectors
                    )),
                    search_text: "home assistant connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenHomeAssistantPicker,
                },
                GenericPickerEntry {
                    label: "Webhook connectors".to_string(),
                    detail: Some(format!("{} connector(s)", daemon_status.webhook_connectors)),
                    search_text: "webhook connectors".to_string(),
                    current: false,
                    action: PickerAction::OpenWebhookPicker,
                },
                GenericPickerEntry {
                    label: "Inbox connectors".to_string(),
                    detail: Some(format!("{} connector(s)", daemon_status.inbox_connectors)),
                    search_text: "inbox connectors folders".to_string(),
                    current: false,
                    action: PickerAction::OpenInboxPicker,
                },
            ]),
            SettingsSection::Autonomy => items.extend([
                GenericPickerEntry {
                    label: "Free thinking mode".to_string(),
                    detail: Some(autonomy_summary(autonomy.state).to_string()),
                    search_text: "autonomy free thinking".to_string(),
                    current: false,
                    action: PickerAction::OpenAutonomyPicker,
                },
                GenericPickerEntry {
                    label: "Autopilot runner".to_string(),
                    detail: Some(crate::autopilot_summary(&autopilot)),
                    search_text: "autopilot missions background runner".to_string(),
                    current: false,
                    action: PickerAction::OpenAutopilotPicker,
                },
                GenericPickerEntry {
                    label: "Mission queue".to_string(),
                    detail: Some(format!(
                        "{} active / {} total",
                        daemon_status.active_missions, daemon_status.missions
                    )),
                    search_text: "missions queue background tasks".to_string(),
                    current: false,
                    action: PickerAction::ShowMissionQueue,
                },
            ]),
            SettingsSection::MemorySkills => items.extend([
                GenericPickerEntry {
                    label: "Memory".to_string(),
                    detail: Some(format!("{} stored memories", daemon_status.memories)),
                    search_text: "memory persistent learning".to_string(),
                    current: false,
                    action: PickerAction::ShowMemoryBrowser,
                },
                GenericPickerEntry {
                    label: "Resident profile".to_string(),
                    detail: Some("User, system, and workspace facts".to_string()),
                    search_text: "resident profile user system workspace".to_string(),
                    current: false,
                    action: PickerAction::ShowResidentProfile,
                },
                GenericPickerEntry {
                    label: "Learned skills".to_string(),
                    detail: Some(format!("{} draft(s)", daemon_status.skill_drafts)),
                    search_text: "learned skills workflows drafts".to_string(),
                    current: false,
                    action: PickerAction::OpenSkillDraftPicker(None),
                },
            ]),
            SettingsSection::Delegation => items.extend([
                GenericPickerEntry {
                    label: "Delegation settings".to_string(),
                    detail: Some(format!(
                        "depth={} parallel={}",
                        daemon_status.delegation.max_depth,
                        daemon_status.delegation.max_parallel_subagents
                    )),
                    search_text: "delegation subagents providers recursion parallel".to_string(),
                    current: false,
                    action: PickerAction::OpenDelegationPicker,
                },
                GenericPickerEntry {
                    label: "Delegation targets".to_string(),
                    detail: Some(format!(
                        "{} available target(s)",
                        daemon_status.delegation_targets
                    )),
                    search_text: "delegation targets providers aliases".to_string(),
                    current: false,
                    action: PickerAction::ShowDelegationTargets,
                },
            ]),
            SettingsSection::System => items.extend([
                GenericPickerEntry {
                    label: "Open web dashboard".to_string(),
                    detail: Some("Launch the localhost control room in your browser.".to_string()),
                    search_text: "dashboard browser ui localhost web gui".to_string(),
                    current: false,
                    action: PickerAction::QueueExternal(ExternalAction::OpenDashboard),
                },
                GenericPickerEntry {
                    label: "Daemon persistence".to_string(),
                    detail: Some(daemon_status.persistence_mode.to_string()),
                    search_text: "daemon persistence always on on demand".to_string(),
                    current: false,
                    action: PickerAction::OpenPersistencePicker,
                },
                GenericPickerEntry {
                    label: "Launch daemon at login".to_string(),
                    detail: Some(boolean_status(daemon_status.auto_start)),
                    search_text: "daemon auto start autostart startup".to_string(),
                    current: false,
                    action: PickerAction::ToggleAutoStart,
                },
            ]),
        }

        self.open_generic_picker(
            PickerMode::Config,
            settings_section_title(section),
            "Enter open | Type filter | Esc cancel",
            "No matching setting.",
            items,
        );
        Ok(())
    }

    async fn open_skill_draft_picker(&mut self, status: Option<SkillDraftStatus>) -> Result<()> {
        let mut path = "/v1/skills/drafts?limit=50".to_string();
        if let Some(ref status) = status {
            path.push_str("&status=");
            path.push_str(match status {
                SkillDraftStatus::Draft => "draft",
                SkillDraftStatus::Published => "published",
                SkillDraftStatus::Rejected => "rejected",
            });
        }
        let drafts: Vec<SkillDraft> = self.client.get(&path).await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Review queue".to_string(),
                detail: Some("Draft and unpublished learned workflows.".to_string()),
                search_text: "skills draft review queue workflows".to_string(),
                current: status.is_none() || status == Some(SkillDraftStatus::Draft),
                action: PickerAction::OpenSkillDraftPicker(None),
            },
            GenericPickerEntry {
                label: "Published skills".to_string(),
                detail: Some("Approved procedural memory.".to_string()),
                search_text: "skills published approved".to_string(),
                current: status == Some(SkillDraftStatus::Published),
                action: PickerAction::OpenSkillDraftPicker(Some(SkillDraftStatus::Published)),
            },
            GenericPickerEntry {
                label: "Rejected skills".to_string(),
                detail: Some("Discarded learned workflows.".to_string()),
                search_text: "skills rejected discarded".to_string(),
                current: status == Some(SkillDraftStatus::Rejected),
                action: PickerAction::OpenSkillDraftPicker(Some(SkillDraftStatus::Rejected)),
            },
        ];

        items.extend(drafts.into_iter().map(|draft| {
            let status_label = match draft.status {
                SkillDraftStatus::Draft => "draft",
                SkillDraftStatus::Published => "published",
                SkillDraftStatus::Rejected => "rejected",
            };
            let mut detail_parts = vec![
                status_label.to_string(),
                format!("usage={}", draft.usage_count),
            ];
            if let Some(provider_id) = &draft.provider_id {
                detail_parts.push(provider_id.clone());
            }
            if let Some(trigger_hint) = &draft.trigger_hint {
                detail_parts.push(format!("trigger={trigger_hint}"));
            }
            GenericPickerEntry {
                label: draft.title.clone(),
                detail: Some(detail_parts.join(" | ")),
                search_text: format!(
                    "{} {} {} {}",
                    draft.id, draft.title, draft.summary, draft.instructions
                ),
                current: false,
                action: PickerAction::OpenSkillDraftActions(draft.id),
            }
        }));

        self.open_generic_picker(
            PickerMode::SkillDraft,
            "Learned skills",
            "Enter select | Type to filter | Esc cancel",
            "No matching learned skill.",
            items,
        );
        Ok(())
    }

    async fn open_skill_draft_action_picker(&mut self, draft_id: &str) -> Result<()> {
        let draft: SkillDraft = self
            .client
            .get(&format!("/v1/skills/drafts/{draft_id}"))
            .await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to learned skills".to_string(),
                detail: Some("Return to the learned skills list.".to_string()),
                search_text: "back learned skills".to_string(),
                current: false,
                action: PickerAction::OpenSkillDraftPicker(None),
            },
            GenericPickerEntry {
                label: "View details".to_string(),
                detail: Some("Show the full summary and generated instructions.".to_string()),
                search_text: "details instructions summary".to_string(),
                current: false,
                action: PickerAction::ShowSkillDraftDetails(draft.id.clone()),
            },
        ];

        if draft.status != SkillDraftStatus::Published {
            items.push(GenericPickerEntry {
                label: "Publish".to_string(),
                detail: Some("Approve this learned workflow for future reuse.".to_string()),
                search_text: "publish approve learned skill".to_string(),
                current: false,
                action: PickerAction::PublishSkillDraft(draft.id.clone()),
            });
        }
        if draft.status != SkillDraftStatus::Rejected {
            items.push(GenericPickerEntry {
                label: "Reject".to_string(),
                detail: Some("Discard this learned workflow.".to_string()),
                search_text: "reject discard learned skill".to_string(),
                current: false,
                action: PickerAction::RejectSkillDraft(draft.id.clone()),
            });
        }

        self.open_generic_picker(
            PickerMode::SkillDraftAction,
            format!("Skill: {}", draft.title),
            "Enter select | Type to filter | Esc cancel",
            "No matching action.",
            items,
        );
        Ok(())
    }

    async fn show_skill_draft_details(&mut self, draft_id: &str) -> Result<()> {
        let draft: SkillDraft = self
            .client
            .get(&format!("/v1/skills/drafts/{draft_id}"))
            .await?;
        let body = format!(
            "id={}\nstatus={:?}\nusage_count={}\nprovider={}\nworkspace={}\nsource_session={}\ntrigger={}\n\nsummary:\n{}\n\ninstructions:\n{}",
            draft.id,
            draft.status,
            draft.usage_count,
            draft.provider_id.as_deref().unwrap_or("-"),
            draft.workspace_key.as_deref().unwrap_or("-"),
            draft.source_session_id.as_deref().unwrap_or("-"),
            draft.trigger_hint.as_deref().unwrap_or("-"),
            draft.summary,
            draft.instructions
        );
        self.open_static_overlay("Skill Draft", body);
        Ok(())
    }

    fn open_provider_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_provider = resolve_active_alias(&config, self.alias.as_deref())
            .ok()
            .map(|alias| alias.provider_id.clone());
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Add provider".to_string(),
                detail: Some("Run the full setup flow for a new provider.".to_string()),
                search_text: "add provider auth login setup".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddProvider),
            },
        ];
        let mut providers = config.providers.clone();
        providers.sort_by(|left, right| left.display_name.cmp(&right.display_name));
        items.extend(providers.into_iter().map(|provider| {
            let aliases = config
                .aliases
                .iter()
                .filter(|alias| alias.provider_id == provider.id)
                .map(|alias| alias.alias.clone())
                .collect::<Vec<_>>();
            let alias_summary = if aliases.is_empty() {
                "no aliases".to_string()
            } else {
                format!("aliases: {}", aliases.join(", "))
            };
            GenericPickerEntry {
                label: provider.display_name.clone(),
                detail: Some(format!(
                    "{} | {} | {} | delegation={} | {}",
                    provider.id,
                    provider_kind_label(&provider),
                    provider_auth_label(&provider),
                    boolean_status(config.provider_delegation_enabled(&provider.id)),
                    alias_summary
                )),
                search_text: format!(
                    "{} {} {} {} {} {}",
                    provider.id,
                    provider.display_name,
                    provider.base_url,
                    provider_kind_label(&provider),
                    if config.provider_delegation_enabled(&provider.id) {
                        "delegation enabled"
                    } else {
                        "delegation disabled"
                    },
                    aliases.join(" ")
                ),
                current: active_provider.as_deref() == Some(provider.id.as_str()),
                action: PickerAction::OpenProviderActions(provider.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Provider,
            "Providers & authentication",
            "Enter select | Type to filter | Esc cancel",
            "No matching provider.",
            items,
        );
        Ok(())
    }

    fn open_provider_action_picker(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .get_provider(provider_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to providers".to_string(),
                detail: Some("Return to the provider list.".to_string()),
                search_text: "back providers auth".to_string(),
                current: false,
                action: PickerAction::OpenProviderPicker,
            },
            GenericPickerEntry {
                label: "View provider details".to_string(),
                detail: Some("Show auth mode, aliases, model, and base URL.".to_string()),
                search_text: "details info aliases model base url".to_string(),
                current: false,
                action: PickerAction::ShowProviderDetails(provider.id.clone()),
            },
        ];

        if hosted_kind_for_provider(&provider).is_some() {
            items.push(GenericPickerEntry {
                label: browser_action_label(&provider).to_string(),
                detail: Some("Run the provider-specific browser sign-in flow again.".to_string()),
                search_text: "browser sign in portal reauth login oauth api key".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::ProviderBrowserLogin {
                    provider_id: provider.id.clone(),
                }),
            });
        }

        if provider.auth_mode == AuthMode::OAuth && provider.oauth.is_some() {
            items.push(GenericPickerEntry {
                label: "Run OAuth login again".to_string(),
                detail: Some("Refresh this provider by re-running its OAuth flow.".to_string()),
                search_text: "oauth auth sign in reauth refresh".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::ProviderOAuthLogin {
                    provider_id: provider.id.clone(),
                }),
            });
        }

        if provider.auth_mode == AuthMode::ApiKey {
            items.push(GenericPickerEntry {
                label: "Update API key".to_string(),
                detail: Some("Paste a new API key for this provider.".to_string()),
                search_text: "api key update rotate credential".to_string(),
                current: false,
                action: PickerAction::EditApiKey(provider.id.clone()),
            });
        }

        if provider.keychain_account.is_some() {
            items.push(GenericPickerEntry {
                label: "Clear stored credentials".to_string(),
                detail: Some(
                    "Remove the saved browser/API credentials for this provider.".to_string(),
                ),
                search_text: "logout clear credentials keychain token".to_string(),
                current: false,
                action: PickerAction::ClearProviderCredentials(provider.id.clone()),
            });
        }

        items.push(GenericPickerEntry {
            label: if config.provider_delegation_enabled(&provider.id) {
                "Disable delegation".to_string()
            } else {
                "Enable delegation".to_string()
            },
            detail: Some(
                "Control whether this provider is available to cross-provider subagents."
                    .to_string(),
            ),
            search_text: "delegation providers subagents enable disable".to_string(),
            current: false,
            action: PickerAction::ToggleProviderDelegation(
                provider.id.clone(),
                !config.provider_delegation_enabled(&provider.id),
            ),
        });

        self.open_generic_picker(
            PickerMode::ProviderAction,
            format!("Provider actions: {}", provider.display_name),
            "Enter select | Type to filter | Esc cancel",
            "No matching provider action.",
            items,
        );
        Ok(())
    }

    async fn open_webhook_picker(&mut self) -> Result<()> {
        let connectors: Vec<WebhookConnectorConfig> = self.client.get("/v1/webhooks").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new webhook connector".to_string(),
                detail: Some("Create a new inbound webhook connector.".to_string()),
                search_text: "setup add new webhook connector".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddWebhookConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | alias={} | model={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    connector.alias.as_deref().unwrap_or_default(),
                    connector.requested_model.as_deref().unwrap_or_default(),
                    cwd,
                    if connector.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                current: connector.enabled,
                action: PickerAction::OpenWebhookActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Webhook,
            "Webhook connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching webhook connector.",
            items,
        );
        Ok(())
    }

    async fn open_webhook_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: WebhookConnectorConfig = self
            .client
            .get(&format!("/v1/webhooks/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to webhook connectors".to_string(),
                detail: Some("Return to the webhook connector list.".to_string()),
                search_text: "back webhook connectors".to_string(),
                current: false,
                action: PickerAction::OpenWebhookPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some("Show alias, model, cwd, and prompt template.".to_string()),
                search_text: "details alias model cwd prompt template".to_string(),
                current: false,
                action: PickerAction::ShowWebhookDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether inbound events can queue missions.".to_string()),
                search_text: "enable disable connector inbound events".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleWebhookEnabled(
                    connector.id.clone(),
                    !connector.enabled,
                ),
            },
        ];
        self.open_generic_picker(
            PickerMode::WebhookAction,
            format!("Webhook actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching webhook action.",
            items,
        );
        Ok(())
    }

    async fn open_inbox_picker(&mut self) -> Result<()> {
        let connectors: Vec<InboxConnectorConfig> = self.client.get("/v1/inboxes").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new inbox connector".to_string(),
                detail: Some("Create a new local inbox connector.".to_string()),
                search_text: "setup add new inbox connector".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddInboxConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | delete_after_read={} | alias={} | model={} | path={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    boolean_status(connector.delete_after_read),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector.path.display(),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    connector.alias.as_deref().unwrap_or_default(),
                    connector.requested_model.as_deref().unwrap_or_default(),
                    connector.path.display(),
                    cwd,
                    if connector.enabled { "enabled" } else { "disabled" },
                    if connector.delete_after_read { "delete" } else { "archive" }
                ),
                current: connector.enabled,
                action: PickerAction::OpenInboxActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Inbox,
            "Inbox connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching inbox connector.",
            items,
        );
        Ok(())
    }

    async fn open_inbox_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: InboxConnectorConfig = self
            .client
            .get(&format!("/v1/inboxes/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to inbox connectors".to_string(),
                detail: Some("Return to the inbox connector list.".to_string()),
                search_text: "back inbox connectors".to_string(),
                current: false,
                action: PickerAction::OpenInboxPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some("Show alias, model, path, cwd, and retention mode.".to_string()),
                search_text: "details alias model path cwd delete archive".to_string(),
                current: false,
                action: PickerAction::ShowInboxDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether inbox files can queue missions.".to_string()),
                search_text: "enable disable connector inbox missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleInboxEnabled(connector.id.clone(), !connector.enabled),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some(
                    "Scan the inbox directory immediately and queue any new files.".to_string(),
                ),
                search_text: "poll inbox scan now queue files".to_string(),
                current: false,
                action: PickerAction::PollInbox(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::InboxAction,
            format!("Inbox actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching inbox action.",
            items,
        );
        Ok(())
    }

    async fn open_telegram_picker(&mut self) -> Result<()> {
        let connectors: Vec<TelegramConnectorConfig> = self.client.get("/v1/telegram").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new Telegram connector".to_string(),
                detail: Some("Create a new Telegram bot connector.".to_string()),
                search_text: "setup add new telegram connector bot".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddTelegramConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | chats={} | users={} | alias={} | model={} | last_update_id={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    crate::format_i64_list(&connector.allowed_chat_ids),
                    crate::format_i64_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or("-"),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    connector
                        .last_update_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    crate::format_i64_list(&connector.allowed_chat_ids),
                    crate::format_i64_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or_default(),
                    connector.requested_model.as_deref().unwrap_or_default(),
                    cwd,
                    if connector.enabled { "enabled" } else { "disabled" }
                ),
                current: connector.enabled,
                action: PickerAction::OpenTelegramActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Telegram,
            "Telegram connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching telegram connector.",
            items,
        );
        Ok(())
    }

    async fn open_telegram_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: TelegramConnectorConfig = self
            .client
            .get(&format!("/v1/telegram/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to Telegram connectors".to_string(),
                detail: Some("Return to the Telegram connector list.".to_string()),
                search_text: "back telegram connectors".to_string(),
                current: false,
                action: PickerAction::OpenTelegramPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some("Show alias, model, filters, cursor, and cwd.".to_string()),
                search_text: "details alias model chats users cursor cwd".to_string(),
                current: false,
                action: PickerAction::ShowTelegramDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether Telegram updates can queue missions.".to_string()),
                search_text: "enable disable connector telegram missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleTelegramEnabled(
                    connector.id.clone(),
                    !connector.enabled,
                ),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some("Fetch Telegram updates immediately and queue new work.".to_string()),
                search_text: "poll telegram updates now".to_string(),
                current: false,
                action: PickerAction::PollTelegram(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::TelegramAction,
            format!("Telegram actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching telegram action.",
            items,
        );
        Ok(())
    }

    async fn open_discord_picker(&mut self) -> Result<()> {
        let connectors: Vec<DiscordConnectorConfig> = self.client.get("/v1/discord").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new Discord connector".to_string(),
                detail: Some("Create a new Discord bot connector.".to_string()),
                search_text: "setup add new discord connector bot".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddDiscordConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | monitored={} | allowed={} | users={} | model={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or_default(),
                    cwd,
                    if connector.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                current: connector.enabled,
                action: PickerAction::OpenDiscordActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Discord,
            "Discord connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching discord connector.",
            items,
        );
        Ok(())
    }

    async fn open_discord_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: DiscordConnectorConfig = self
            .client
            .get(&format!("/v1/discord/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to Discord connectors".to_string(),
                detail: Some("Return to the Discord connector list.".to_string()),
                search_text: "back discord connectors".to_string(),
                current: false,
                action: PickerAction::OpenDiscordPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some(
                    "Show alias, model, pairing mode, channel filters, cursors, and cwd."
                        .to_string(),
                ),
                search_text: "details alias model channels users cursors cwd".to_string(),
                current: false,
                action: PickerAction::ShowDiscordDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether Discord messages can queue missions.".to_string()),
                search_text: "enable disable connector discord missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleDiscordEnabled(
                    connector.id.clone(),
                    !connector.enabled,
                ),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some("Fetch Discord messages immediately and queue new work.".to_string()),
                search_text: "poll discord messages now".to_string(),
                current: false,
                action: PickerAction::PollDiscord(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::DiscordAction,
            format!("Discord actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching discord action.",
            items,
        );
        Ok(())
    }

    async fn open_slack_picker(&mut self) -> Result<()> {
        let connectors: Vec<SlackConnectorConfig> = self.client.get("/v1/slack").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new Slack connector".to_string(),
                detail: Some("Create a new Slack bot connector.".to_string()),
                search_text: "setup add new slack connector bot".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddSlackConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | monitored={} | allowed={} | users={} | model={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    crate::format_string_list(&connector.monitored_channel_ids),
                    crate::format_string_list(&connector.allowed_channel_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or_default(),
                    cwd,
                    if connector.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                current: connector.enabled,
                action: PickerAction::OpenSlackActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Slack,
            "Slack connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching slack connector.",
            items,
        );
        Ok(())
    }

    async fn open_slack_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: SlackConnectorConfig = self
            .client
            .get(&format!("/v1/slack/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to Slack connectors".to_string(),
                detail: Some("Return to the Slack connector list.".to_string()),
                search_text: "back slack connectors".to_string(),
                current: false,
                action: PickerAction::OpenSlackPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some(
                    "Show alias, model, pairing mode, channel filters, cursors, and cwd."
                        .to_string(),
                ),
                search_text: "details alias model channels users cursors cwd".to_string(),
                current: false,
                action: PickerAction::ShowSlackDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether Slack messages can queue missions.".to_string()),
                search_text: "enable disable connector slack missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleSlackEnabled(connector.id.clone(), !connector.enabled),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some("Fetch Slack messages immediately and queue new work.".to_string()),
                search_text: "poll slack messages now".to_string(),
                current: false,
                action: PickerAction::PollSlack(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::SlackAction,
            format!("Slack actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching slack action.",
            items,
        );
        Ok(())
    }

    async fn open_signal_picker(&mut self) -> Result<()> {
        let connectors: Vec<SignalConnectorConfig> = self.client.get("/v1/signal").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new Signal connector".to_string(),
                detail: Some("Create a new Signal connector.".to_string()),
                search_text: "setup add new signal connector".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddSignalConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            let cli_path = connector
                .cli_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "signal-cli".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | account={} | monitored_groups={} | allowed_groups={} | users={} | model={} | cli={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    connector.account,
                    crate::format_string_list(&connector.monitored_group_ids),
                    crate::format_string_list(&connector.allowed_group_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    cli_path,
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    connector.account,
                    crate::format_string_list(&connector.monitored_group_ids),
                    crate::format_string_list(&connector.allowed_group_ids),
                    crate::format_string_list(&connector.allowed_user_ids),
                    connector.alias.as_deref().unwrap_or_default(),
                    cli_path,
                    cwd,
                    if connector.enabled { "enabled" } else { "disabled" }
                ),
                current: connector.enabled,
                action: PickerAction::OpenSignalActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::Signal,
            "Signal connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching signal connector.",
            items,
        );
        Ok(())
    }

    async fn open_signal_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: SignalConnectorConfig = self
            .client
            .get(&format!("/v1/signal/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to Signal connectors".to_string(),
                detail: Some("Return to the Signal connector list.".to_string()),
                search_text: "back signal connectors".to_string(),
                current: false,
                action: PickerAction::OpenSignalPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some(
                    "Show alias, model, pairing mode, group filters, cli path, and cwd."
                        .to_string(),
                ),
                search_text: "details alias model groups users cli cwd".to_string(),
                current: false,
                action: PickerAction::ShowSignalDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some("Control whether Signal messages can queue missions.".to_string()),
                search_text: "enable disable connector signal missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleSignalEnabled(connector.id.clone(), !connector.enabled),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some("Fetch Signal messages immediately and queue new work.".to_string()),
                search_text: "poll signal messages now".to_string(),
                current: false,
                action: PickerAction::PollSignal(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::SignalAction,
            format!("Signal actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching signal action.",
            items,
        );
        Ok(())
    }

    async fn open_home_assistant_picker(&mut self) -> Result<()> {
        let connectors: Vec<HomeAssistantConnectorConfig> =
            self.client.get("/v1/home-assistant").await?;
        let mut items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Set up new Home Assistant connector".to_string(),
                detail: Some("Create a new Home Assistant connector.".to_string()),
                search_text: "setup add new home assistant connector".to_string(),
                current: false,
                action: PickerAction::QueueExternal(ExternalAction::AddHomeAssistantConnector),
            },
        ];
        let mut connectors = connectors;
        connectors.sort_by(|left, right| left.name.cmp(&right.name));
        items.extend(connectors.into_iter().map(|connector| {
            let cwd = connector
                .cwd
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            GenericPickerEntry {
                label: connector.name.clone(),
                detail: Some(format!(
                    "{} | enabled={} | base_url={} | monitored={} | service_domains={} | service_entities={} | model={} | cwd={}",
                    connector.id,
                    boolean_status(connector.enabled),
                    connector.base_url,
                    crate::format_string_list(&connector.monitored_entity_ids),
                    crate::format_string_list(&connector.allowed_service_domains),
                    crate::format_string_list(&connector.allowed_service_entity_ids),
                    connector.requested_model.as_deref().unwrap_or("-"),
                    cwd
                )),
                search_text: format!(
                    "{} {} {} {} {} {} {} {} {}",
                    connector.id,
                    connector.name,
                    connector.base_url,
                    crate::format_string_list(&connector.monitored_entity_ids),
                    crate::format_string_list(&connector.allowed_service_domains),
                    crate::format_string_list(&connector.allowed_service_entity_ids),
                    connector.alias.as_deref().unwrap_or_default(),
                    cwd,
                    if connector.enabled { "enabled" } else { "disabled" }
                ),
                current: connector.enabled,
                action: PickerAction::OpenHomeAssistantActions(connector.id),
            }
        }));
        self.open_generic_picker(
            PickerMode::HomeAssistant,
            "Home Assistant connectors",
            "Enter select | Type to filter | Esc cancel",
            "No matching Home Assistant connector.",
            items,
        );
        Ok(())
    }

    async fn open_home_assistant_action_picker(&mut self, connector_id: &str) -> Result<()> {
        let connector: HomeAssistantConnectorConfig = self
            .client
            .get(&format!("/v1/home-assistant/{connector_id}"))
            .await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to Home Assistant connectors".to_string(),
                detail: Some("Return to the Home Assistant connector list.".to_string()),
                search_text: "back home assistant connectors".to_string(),
                current: false,
                action: PickerAction::OpenHomeAssistantPicker,
            },
            GenericPickerEntry {
                label: "View connector details".to_string(),
                detail: Some(
                    "Show base URL, entity filters, service allowlists, cursors, alias, model, and cwd."
                        .to_string(),
                ),
                search_text:
                    "details base url entities services cursors alias model cwd".to_string(),
                current: false,
                action: PickerAction::ShowHomeAssistantDetails(connector.id.clone()),
            },
            GenericPickerEntry {
                label: if connector.enabled {
                    "Disable connector".to_string()
                } else {
                    "Enable connector".to_string()
                },
                detail: Some(
                    "Control whether Home Assistant entity changes can queue missions."
                        .to_string(),
                ),
                search_text: "enable disable connector home assistant missions".to_string(),
                current: connector.enabled,
                action: PickerAction::ToggleHomeAssistantEnabled(
                    connector.id.clone(),
                    !connector.enabled,
                ),
            },
            GenericPickerEntry {
                label: "Poll connector now".to_string(),
                detail: Some(
                    "Fetch Home Assistant entity state immediately and queue any changes."
                        .to_string(),
                ),
                search_text: "poll home assistant entity state now".to_string(),
                current: false,
                action: PickerAction::PollHomeAssistant(connector.id.clone()),
            },
        ];
        self.open_generic_picker(
            PickerMode::HomeAssistantAction,
            format!("Home Assistant actions: {}", connector.name),
            "Enter select | Type to filter | Esc cancel",
            "No matching Home Assistant action.",
            items,
        );
        Ok(())
    }

    async fn open_persistence_picker(&mut self) -> Result<()> {
        let status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        let items = vec![
            GenericPickerEntry {
                label: "Back to configuration".to_string(),
                detail: Some("Return to the main settings menu.".to_string()),
                search_text: "back configuration settings".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "on-demand".to_string(),
                detail: Some("Start the daemon when needed and let it exit when idle.".to_string()),
                search_text: "daemon on demand".to_string(),
                current: status.persistence_mode == PersistenceMode::OnDemand,
                action: PickerAction::SetPersistenceMode(PersistenceMode::OnDemand),
            },
            GenericPickerEntry {
                label: "always-on".to_string(),
                detail: Some("Keep the daemon running in the background.".to_string()),
                search_text: "daemon always on".to_string(),
                current: status.persistence_mode == PersistenceMode::AlwaysOn,
                action: PickerAction::SetPersistenceMode(PersistenceMode::AlwaysOn),
            },
        ];
        self.open_generic_picker(
            PickerMode::Persistence,
            "Daemon persistence",
            "Enter select | Esc cancel",
            "No matching persistence mode.",
            items,
        );
        Ok(())
    }

    async fn open_autonomy_picker(&mut self) -> Result<()> {
        let autonomy: AutonomyProfile = self.client.get("/v1/autonomy/status").await?;
        let evolve: EvolveConfig = self.client.get("/v1/evolve/status").await?;
        let mut items = vec![GenericPickerEntry {
            label: "Back to configuration".to_string(),
            detail: Some("Return to the main settings menu.".to_string()),
            search_text: "back configuration settings".to_string(),
            current: false,
            action: PickerAction::OpenConfig,
        }];
        items.push(GenericPickerEntry {
            label: "Manage evolve mode".to_string(),
            detail: Some(format!(
                "Inspect or control the self-improvement loop. Current evolve state: {:?}.",
                evolve.state
            )),
            search_text: "evolve self improvement".to_string(),
            current: false,
            action: PickerAction::OpenEvolvePicker,
        });
        match autonomy.state {
            AutonomyState::Disabled => {
                items.push(GenericPickerEntry {
                    label: "Enable free thinking".to_string(),
                    detail: Some("Turns on the all-guardrails-off free thinking mode.".to_string()),
                    search_text: "enable autonomy free thinking".to_string(),
                    current: true,
                    action: PickerAction::SetAutonomy(AutonomyMenuAction::EnableFreeThinking),
                });
                items.push(GenericPickerEntry {
                    label: "Enable evolve mode".to_string(),
                    detail: Some("Starts the methodical self-improvement loop.".to_string()),
                    search_text: "enable evolve self improve".to_string(),
                    current: false,
                    action: PickerAction::SetAutonomy(AutonomyMenuAction::EnableEvolve),
                });
            }
            AutonomyState::Enabled => {
                items.push(GenericPickerEntry {
                    label: format!(
                        "Pause {}",
                        match autonomy.mode {
                            AutonomyMode::Evolve => "evolve mode",
                            _ => "free thinking",
                        }
                    ),
                    detail: Some("Keeps consent but pauses autonomous execution.".to_string()),
                    search_text: "pause autonomy".to_string(),
                    current: true,
                    action: PickerAction::SetAutonomy(AutonomyMenuAction::Pause),
                });
            }
            AutonomyState::Paused => {
                items.push(GenericPickerEntry {
                    label: "Resume free thinking".to_string(),
                    detail: Some(
                        "Resumes autonomous execution with the existing consent.".to_string(),
                    ),
                    search_text: "resume autonomy".to_string(),
                    current: true,
                    action: PickerAction::SetAutonomy(AutonomyMenuAction::Resume),
                });
            }
        }
        self.open_generic_picker(
            PickerMode::Autonomy,
            "Free thinking mode",
            "Enter select | Esc cancel",
            "No matching autonomy action.",
            items,
        );
        Ok(())
    }

    fn open_input_overlay(
        &mut self,
        title: impl Into<String>,
        prompt: impl Into<String>,
        secret: bool,
        action: InputPromptAction,
    ) {
        self.overlay = Some(OverlayState::Input {
            title: title.into(),
            prompt: prompt.into(),
            value: String::new(),
            cursor: 0,
            secret,
            action,
        });
    }

    async fn toggle_trust_setting(&mut self, toggle: TrustToggle) -> Result<()> {
        let config = self.storage.load_config()?;
        let mut update = TrustUpdateRequest {
            trusted_path: None,
            allow_shell: None,
            allow_network: None,
            allow_full_disk: None,
            allow_self_edit: None,
        };
        let title = match toggle {
            TrustToggle::Shell => {
                update.allow_shell = Some(!config.trust_policy.allow_shell);
                "Shell access"
            }
            TrustToggle::Network => {
                update.allow_network = Some(!config.trust_policy.allow_network);
                "Network access"
            }
            TrustToggle::FullDisk => {
                update.allow_full_disk = Some(!config.trust_policy.allow_full_disk);
                "Full disk access"
            }
            TrustToggle::SelfEdit => {
                update.allow_self_edit = Some(!config.trust_policy.allow_self_edit);
                "Self edit"
            }
        };
        let updated: agent_core::TrustPolicy = self.client.put("/v1/trust", &update).await?;
        self.open_static_overlay(title, crate::trust_summary(&updated).to_string());
        Ok(())
    }

    async fn apply_autonomy_action(&mut self, action: AutonomyMenuAction) -> Result<()> {
        let current_mode = self
            .client
            .get::<AutonomyProfile>("/v1/autonomy/status")
            .await?
            .mode;
        let status: AutonomyProfile = match action {
            AutonomyMenuAction::EnableFreeThinking => {
                self.client
                    .post(
                        "/v1/autonomy/enable",
                        &AutonomyEnableRequest {
                            mode: Some(AutonomyMode::FreeThinking),
                            allow_self_edit: true,
                        },
                    )
                    .await?
            }
            AutonomyMenuAction::EnableEvolve => {
                let _: EvolveConfig = self
                    .client
                    .post(
                        "/v1/evolve/start",
                        &EvolveStartRequest {
                            alias: None,
                            requested_model: None,
                            budget_friendly: Some(false),
                        },
                    )
                    .await?;
                self.client.get("/v1/autonomy/status").await?
            }
            AutonomyMenuAction::Pause => {
                if matches!(current_mode, AutonomyMode::Evolve) {
                    let _: EvolveConfig = self
                        .client
                        .post("/v1/evolve/pause", &serde_json::json!({}))
                        .await?;
                    self.client.get("/v1/autonomy/status").await?
                } else {
                    self.client
                        .post("/v1/autonomy/pause", &serde_json::json!({}))
                        .await?
                }
            }
            AutonomyMenuAction::Resume => {
                if matches!(current_mode, AutonomyMode::Evolve) {
                    let _: EvolveConfig = self
                        .client
                        .post("/v1/evolve/resume", &serde_json::json!({}))
                        .await?;
                    self.client.get("/v1/autonomy/status").await?
                } else {
                    self.client
                        .post("/v1/autonomy/resume", &serde_json::json!({}))
                        .await?
                }
            }
        };
        self.open_static_overlay(
            "Free thinking mode",
            format!(
                "autonomy={} mode={:?} unlimited_usage={} full_network={} self_edit={}",
                autonomy_summary(status.state),
                status.mode,
                status.unlimited_usage,
                status.full_network,
                status.allow_self_edit
            ),
        );
        Ok(())
    }

    async fn open_evolve_picker(&mut self) -> Result<()> {
        let evolve: EvolveConfig = self.client.get("/v1/evolve/status").await?;
        let mut items = vec![GenericPickerEntry {
            label: "Back to autonomy settings".to_string(),
            detail: Some("Return to the autonomy menu.".to_string()),
            search_text: "back autonomy settings".to_string(),
            current: false,
            action: PickerAction::OpenAutonomyPicker,
        }];
        match evolve.state {
            agent_core::EvolveState::Disabled
            | agent_core::EvolveState::Completed
            | agent_core::EvolveState::Failed => {
                items.push(GenericPickerEntry {
                    label: "Start evolve mode".to_string(),
                    detail: Some(
                        "Unlimited recursion, test-gated, agent-decides stop.".to_string(),
                    ),
                    search_text: "start evolve".to_string(),
                    current: true,
                    action: PickerAction::SetEvolve(EvolveMenuAction::Start),
                });
                items.push(GenericPickerEntry {
                    label: "Start evolve mode (budget-friendly)".to_string(),
                    detail: Some(
                        "Uses the lighter stop policy for a cheaper improvement loop.".to_string(),
                    ),
                    search_text: "start evolve budget friendly".to_string(),
                    current: false,
                    action: PickerAction::SetEvolve(EvolveMenuAction::StartBudgetFriendly),
                });
            }
            agent_core::EvolveState::Running => {
                items.push(GenericPickerEntry {
                    label: "Pause evolve mode".to_string(),
                    detail: Some("Pause the self-improvement loop.".to_string()),
                    search_text: "pause evolve".to_string(),
                    current: true,
                    action: PickerAction::SetEvolve(EvolveMenuAction::Pause),
                });
                items.push(GenericPickerEntry {
                    label: "Stop evolve mode".to_string(),
                    detail: Some("Stop the loop and clear active evolve control.".to_string()),
                    search_text: "stop evolve".to_string(),
                    current: false,
                    action: PickerAction::SetEvolve(EvolveMenuAction::Stop),
                });
            }
            agent_core::EvolveState::Paused => {
                items.push(GenericPickerEntry {
                    label: "Resume evolve mode".to_string(),
                    detail: Some("Resume the self-improvement loop.".to_string()),
                    search_text: "resume evolve".to_string(),
                    current: true,
                    action: PickerAction::SetEvolve(EvolveMenuAction::Resume),
                });
                items.push(GenericPickerEntry {
                    label: "Stop evolve mode".to_string(),
                    detail: Some("Stop the loop and clear active evolve control.".to_string()),
                    search_text: "stop evolve".to_string(),
                    current: false,
                    action: PickerAction::SetEvolve(EvolveMenuAction::Stop),
                });
            }
        }
        self.open_generic_picker(
            PickerMode::Autonomy,
            "Evolve mode",
            "Enter select | Esc cancel",
            "No matching evolve action.",
            items,
        );
        Ok(())
    }

    async fn apply_evolve_action(&mut self, action: EvolveMenuAction) -> Result<()> {
        let status: EvolveConfig = match action {
            EvolveMenuAction::Start => {
                self.client
                    .post(
                        "/v1/evolve/start",
                        &EvolveStartRequest {
                            alias: None,
                            requested_model: None,
                            budget_friendly: Some(false),
                        },
                    )
                    .await?
            }
            EvolveMenuAction::StartBudgetFriendly => {
                self.client
                    .post(
                        "/v1/evolve/start",
                        &EvolveStartRequest {
                            alias: None,
                            requested_model: None,
                            budget_friendly: Some(true),
                        },
                    )
                    .await?
            }
            EvolveMenuAction::Pause => {
                self.client
                    .post("/v1/evolve/pause", &serde_json::json!({}))
                    .await?
            }
            EvolveMenuAction::Resume => {
                self.client
                    .post("/v1/evolve/resume", &serde_json::json!({}))
                    .await?
            }
            EvolveMenuAction::Stop => {
                self.client
                    .post("/v1/evolve/stop", &serde_json::json!({}))
                    .await?
            }
        };
        self.open_static_overlay(
            "Evolve mode",
            format!(
                "state={:?} mission={} iteration={} pending_restart={} last_goal={} last_summary={}",
                status.state,
                status.current_mission_id.as_deref().unwrap_or("-"),
                status.iteration,
                status.pending_restart,
                status.last_goal.as_deref().unwrap_or("-"),
                status.last_summary.as_deref().unwrap_or("-"),
            ),
        );
        Ok(())
    }

    async fn active_model_descriptor(&self) -> Result<Option<ModelDescriptor>> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .get_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();
        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), provider),
        )
        .await;
        let Ok(Ok(models)) = listed else {
            return Ok(None);
        };
        Ok(models.into_iter().find(|model| model.id == selected_model))
    }

    fn show_provider_details(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .get_provider(provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        let aliases = config
            .aliases
            .iter()
            .filter(|alias| alias.provider_id == provider.id)
            .map(|alias| {
                if config.main_agent_alias.as_deref() == Some(alias.alias.as_str()) {
                    format!("{} (default)", alias.alias)
                } else {
                    alias.alias.clone()
                }
            })
            .collect::<Vec<_>>();
        let body = format!(
            "name={}\nid={}\nkind={}\nauth={}\nbase_url={}\ndefault_model={}\nlocal={}\nkeychain={}\ndelegation={}\naliases={}",
            provider.display_name,
            provider.id,
            provider_kind_label(provider),
            provider_auth_label(provider),
            provider.base_url,
            provider.default_model.as_deref().unwrap_or("(unset)"),
            provider.local,
            provider.keychain_account.as_deref().unwrap_or("(none)"),
            boolean_status(config.provider_delegation_enabled(&provider.id)),
            if aliases.is_empty() {
                "(none)".to_string()
            } else {
                aliases.join(", ")
            }
        );
        self.open_static_overlay("Provider details", body);
        Ok(())
    }

    fn resume_session(&mut self, session: SessionSummary) -> Result<()> {
        self.alias = Some(session.alias.clone());
        self.session_id = Some(session.id.clone());
        self.requested_model =
            resolve_requested_model_override(self.storage, self.alias.as_deref(), &session.model)?;
        if let Some(cwd) = &session.cwd {
            self.cwd = cwd.clone();
        }
        self.transcript = self.storage.list_session_messages(&session.id)?;
        self.transcript_scroll_back = 0;
        self.open_static_overlay(
            "Resume",
            format!(
                "Resumed {} ({})",
                session.id,
                session.title.as_deref().unwrap_or("(untitled)")
            ),
        );
        Ok(())
    }

    fn fork_session(&mut self, session: SessionSummary) -> Result<()> {
        let transcript = SessionTranscript {
            messages: self.storage.list_session_messages(&session.id)?,
            session,
        };
        let new_session_id = crate::fork_session(self.storage, &transcript)?;
        let forked = SessionTranscript {
            session: self
                .storage
                .list_sessions(SESSION_PICKER_LIMIT)?
                .into_iter()
                .find(|entry| entry.id == new_session_id)
                .ok_or_else(|| anyhow!("forked session not found"))?,
            messages: self.storage.list_session_messages(&new_session_id)?,
        };
        self.alias = Some(forked.session.alias.clone());
        self.session_id = Some(forked.session.id.clone());
        self.requested_model = resolve_requested_model_override(
            self.storage,
            self.alias.as_deref(),
            &forked.session.model,
        )?;
        if let Some(cwd) = &forked.session.cwd {
            self.cwd = cwd.clone();
        }
        self.transcript = forked.messages;
        self.transcript_scroll_back = 0;
        self.open_static_overlay(
            "Fork",
            format!(
                "Forked session {}",
                self.session_id.as_deref().unwrap_or_default()
            ),
        );
        Ok(())
    }

    async fn status_text(&self) -> Result<String> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .get_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref());
        let daemon_status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        Ok(format!(
            "session={}\nalias={}\nprovider={}\nmodel={}\nthinking={}\npermission_preset={}\nattachments={}\ncwd={}\ndaemon={} auto_start={} autonomy={}",
            self.session_id.as_deref().unwrap_or("(new)"),
            active_alias.alias,
            provider.id,
            selected_model,
            thinking_level_label(self.thinking_level),
            permission_summary(self.permission_preset.unwrap_or(config.permission_preset)),
            self.attachments.len(),
            self.cwd.display(),
            match daemon_status.persistence_mode {
                PersistenceMode::OnDemand => "on-demand",
                PersistenceMode::AlwaysOn => "always-on",
            },
            daemon_status.auto_start,
            autonomy_summary(daemon_status.autonomy.state),
        ))
    }

    async fn refresh_active_model_metadata(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .get_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref());

        self.active_model = Some(selected_model.to_string());
        self.active_provider_name = Some(provider.display_name.clone());
        self.context_window_tokens = None;
        self.context_window_percent = None;

        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), provider),
        )
        .await;

        let Ok(Ok(models)) = listed else {
            return Ok(());
        };

        if let Some(model) = models.iter().find(|entry| entry.id == selected_model) {
            self.active_model = Some(
                model
                    .display_name
                    .clone()
                    .unwrap_or_else(|| model.id.clone()),
            );
            self.context_window_tokens = model.context_window;
            self.context_window_percent = model.effective_context_window_percent;
        }

        Ok(())
    }

    pub(super) async fn run_external_action(&mut self, action: ExternalAction) -> Result<()> {
        match action {
            ExternalAction::AddProvider => self.run_add_provider_flow().await,
            ExternalAction::AddWebhookConnector => self.run_add_webhook_connector_flow().await,
            ExternalAction::AddInboxConnector => self.run_add_inbox_connector_flow().await,
            ExternalAction::AddTelegramConnector => self.run_add_telegram_connector_flow().await,
            ExternalAction::AddDiscordConnector => self.run_add_discord_connector_flow().await,
            ExternalAction::AddSlackConnector => self.run_add_slack_connector_flow().await,
            ExternalAction::AddSignalConnector => self.run_add_signal_connector_flow().await,
            ExternalAction::AddHomeAssistantConnector => {
                self.run_add_home_assistant_connector_flow().await
            }
            ExternalAction::ProviderBrowserLogin { provider_id } => {
                self.run_provider_browser_login(&provider_id).await
            }
            ExternalAction::ProviderOAuthLogin { provider_id } => {
                self.run_provider_oauth_login(&provider_id).await
            }
            ExternalAction::OpenDashboard => self.open_dashboard().await,
        }
    }

    async fn open_dashboard(&mut self) -> Result<()> {
        let _ = crate::ensure_daemon(self.storage).await?;
        let config = self.storage.load_config()?;
        let url = format!(
            "http://{}:{}/ui?token={}",
            config.daemon.host, config.daemon.port, config.daemon.token
        );
        match webbrowser::open(&url) {
            Ok(_) => self.open_static_overlay("Dashboard", format!("opened {url}")),
            Err(error) => self.open_static_overlay(
                "Dashboard",
                format!("failed to open browser automatically.\nopen this URL manually:\n{url}\n\nerror: {error}"),
            ),
        }
        Ok(())
    }

    async fn run_add_provider_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let config = self.storage.load_config()?;
        let (request, alias) = interactive_provider_setup(&theme, &config).await?;
        let set_as_main = Confirm::with_theme(&theme)
            .with_prompt(format!("Set '{}' as the default alias?", alias.alias))
            .default(config.main_agent_alias.is_none())
            .interact()?;
        let _: agent_core::ProviderConfig = self.client.post("/v1/providers", &request).await?;
        let _: agent_core::ModelAlias = self
            .client
            .post(
                "/v1/aliases",
                &AliasUpsertRequest {
                    alias: alias.clone(),
                    set_as_main,
                },
            )
            .await?;
        if set_as_main {
            self.alias = Some(alias.alias.clone());
            self.requested_model = None;
            self.refresh_active_model_metadata().await?;
        }
        self.open_static_overlay(
            "Providers",
            format!(
                "configured {} as {}{}",
                request.provider.display_name,
                alias.alias,
                if set_as_main { " (default)" } else { "" }
            ),
        );
        Ok(())
    }

    async fn run_add_webhook_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Webhook connector name", Some("Webhook"))?;
        let id = prompt_required(
            &theme,
            "Webhook connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(
            &theme,
            "Description",
            Some("Receive inbound webhook events"),
        )?;
        let alias = prompt_optional(
            &theme,
            "Alias for webhook-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let token = Uuid::new_v4().to_string();
        let connector = WebhookConnectorConfig {
            id: id.clone(),
            name,
            description: description.unwrap_or_default(),
            prompt_template: default_webhook_prompt_template(),
            enabled: true,
            token_sha256: Some(hash_webhook_token_local(&token)),
            alias,
            requested_model: model,
            cwd,
        };
        let _: WebhookConnectorConfig = self
            .client
            .post(
                "/v1/webhooks",
                &WebhookConnectorUpsertRequest {
                    connector: connector.clone(),
                    webhook_token: Some(token.clone()),
                },
            )
            .await?;
        let config = self.storage.load_config()?;
        self.open_static_overlay(
            "Webhook Connectors",
            format!(
                "configured webhook {}\n\nurl=http://{}:{}/v1/hooks/{}\ntoken={}",
                connector.name, config.daemon.host, config.daemon.port, connector.id, token
            ),
        );
        Ok(())
    }

    async fn run_add_inbox_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Inbox connector name", Some("Inbox"))?;
        let id = prompt_required(
            &theme,
            "Inbox connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description =
            prompt_optional(&theme, "Description", Some("Watch a local inbox folder"))?;
        let path = prompt_required_path(&theme, "Inbox folder path", None)?;
        let delete_after_read = Confirm::with_theme(&theme)
            .with_prompt("Delete inbox files after processing?")
            .default(false)
            .interact()?;
        let alias = prompt_optional(
            &theme,
            "Alias for inbox-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let connector = InboxConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            path,
            enabled: true,
            delete_after_read,
            alias,
            requested_model: model,
            cwd,
        };
        let _: InboxConnectorConfig = self
            .client
            .post(
                "/v1/inboxes",
                &InboxConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
        self.open_static_overlay(
            "Inbox Connectors",
            format!("configured inbox connector {}", name),
        );
        Ok(())
    }

    async fn run_add_telegram_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Telegram connector name", Some("Telegram"))?;
        let id = prompt_required(
            &theme,
            "Telegram connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(&theme, "Description", Some("Telegram bot connector"))?;
        let bot_token = prompt_secret(&theme, "Telegram bot token")?;
        let require_pairing_approval = Confirm::with_theme(&theme)
            .with_prompt("Require approval for new Telegram chats/users?")
            .default(true)
            .interact()?;
        let alias = prompt_optional(
            &theme,
            "Alias for Telegram-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let allowed_chat_ids = prompt_csv_i64(
            &theme,
            "Allowed Telegram chat ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_i64(
            &theme,
            "Allowed Telegram user ids (comma-separated, blank for none)",
        )?;
        let bot_token_keychain_account = Some(store_api_key(
            &format!("connector:telegram:{id}"),
            &bot_token,
        )?);
        let connector = TelegramConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account,
            require_pairing_approval,
            allowed_chat_ids,
            allowed_user_ids,
            last_update_id: None,
            alias,
            requested_model: model,
            cwd,
        };
        let _: TelegramConnectorConfig = self
            .client
            .post(
                "/v1/telegram",
                &TelegramConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: Some(bot_token),
                },
            )
            .await?;
        self.open_static_overlay(
            "Telegram Connectors",
            format!("configured Telegram connector {}", name),
        );
        Ok(())
    }

    async fn run_add_discord_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Discord connector name", Some("Discord"))?;
        let id = prompt_required(
            &theme,
            "Discord connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(&theme, "Description", Some("Discord bot connector"))?;
        let bot_token = prompt_secret(&theme, "Discord bot token")?;
        let require_pairing_approval = Confirm::with_theme(&theme)
            .with_prompt("Require approval for new Discord channels/users?")
            .default(true)
            .interact()?;
        let alias = prompt_optional(
            &theme,
            "Alias for Discord-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let monitored_channel_ids = prompt_csv_strings(
            &theme,
            "Monitored Discord channel ids (comma-separated, blank for none)",
        )?;
        let allowed_channel_ids = prompt_csv_strings(
            &theme,
            "Allowed Discord channel ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_strings(
            &theme,
            "Allowed Discord user ids (comma-separated, blank for none)",
        )?;
        let bot_token_keychain_account = Some(store_api_key(
            &format!("connector:discord:{id}"),
            &bot_token,
        )?);
        let connector = DiscordConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account,
            require_pairing_approval,
            monitored_channel_ids,
            allowed_channel_ids,
            allowed_user_ids,
            channel_cursors: Vec::new(),
            alias,
            requested_model: model,
            cwd,
        };
        let _: DiscordConnectorConfig = self
            .client
            .post(
                "/v1/discord",
                &DiscordConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: Some(bot_token),
                },
            )
            .await?;
        self.open_static_overlay(
            "Discord Connectors",
            format!("configured Discord connector {}", name),
        );
        Ok(())
    }

    async fn run_add_slack_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Slack connector name", Some("Slack"))?;
        let id = prompt_required(
            &theme,
            "Slack connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(&theme, "Description", Some("Slack bot connector"))?;
        let bot_token = prompt_secret(&theme, "Slack bot token")?;
        let require_pairing_approval = Confirm::with_theme(&theme)
            .with_prompt("Require approval for new Slack channels/users?")
            .default(true)
            .interact()?;
        let alias = prompt_optional(
            &theme,
            "Alias for Slack-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let monitored_channel_ids = prompt_csv_strings(
            &theme,
            "Monitored Slack channel ids (comma-separated, blank for none)",
        )?;
        let allowed_channel_ids = prompt_csv_strings(
            &theme,
            "Allowed Slack channel ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_strings(
            &theme,
            "Allowed Slack user ids (comma-separated, blank for none)",
        )?;
        let bot_token_keychain_account =
            Some(store_api_key(&format!("connector:slack:{id}"), &bot_token)?);
        let connector = SlackConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account,
            require_pairing_approval,
            monitored_channel_ids,
            allowed_channel_ids,
            allowed_user_ids,
            channel_cursors: Vec::new(),
            alias,
            requested_model: model,
            cwd,
        };
        let _: SlackConnectorConfig = self
            .client
            .post(
                "/v1/slack",
                &SlackConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: Some(bot_token),
                },
            )
            .await?;
        self.open_static_overlay(
            "Slack Connectors",
            format!("configured Slack connector {}", name),
        );
        Ok(())
    }

    async fn run_add_signal_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(&theme, "Signal connector name", Some("Signal"))?;
        let id = prompt_required(
            &theme,
            "Signal connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(&theme, "Description", Some("Signal connector"))?;
        let account = prompt_required(&theme, "Signal account", None)?;
        let cli_path = prompt_optional_path(&theme, "signal-cli path (optional)", None)?;
        let require_pairing_approval = Confirm::with_theme(&theme)
            .with_prompt("Require approval for new Signal groups/users?")
            .default(true)
            .interact()?;
        let alias = prompt_optional(
            &theme,
            "Alias for Signal-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let monitored_group_ids = prompt_csv_strings(
            &theme,
            "Monitored Signal group ids (comma-separated, blank for none)",
        )?;
        let allowed_group_ids = prompt_csv_strings(
            &theme,
            "Allowed Signal group ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_strings(
            &theme,
            "Allowed Signal user ids (comma-separated, blank for none)",
        )?;
        let connector = SignalConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            account,
            cli_path,
            require_pairing_approval,
            monitored_group_ids,
            allowed_group_ids,
            allowed_user_ids,
            alias,
            requested_model: model,
            cwd,
        };
        let _: SignalConnectorConfig = self
            .client
            .post(
                "/v1/signal",
                &SignalConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
        self.open_static_overlay(
            "Signal Connectors",
            format!("configured Signal connector {}", name),
        );
        Ok(())
    }

    async fn run_add_home_assistant_connector_flow(&mut self) -> Result<()> {
        let theme = ColorfulTheme::default();
        let name = prompt_required(
            &theme,
            "Home Assistant connector name",
            Some("Home Assistant"),
        )?;
        let id = prompt_required(
            &theme,
            "Home Assistant connector id",
            Some(&slugify_identifier(&name)),
        )?;
        let description = prompt_optional(&theme, "Description", Some("Home Assistant connector"))?;
        let base_url = prompt_required(&theme, "Home Assistant base URL", None)?;
        let access_token = prompt_secret(&theme, "Home Assistant access token")?;
        let alias = prompt_optional(
            &theme,
            "Alias for Home Assistant-triggered work",
            self.alias.as_deref(),
        )?;
        let model = prompt_optional(&theme, "Model override (optional)", None)?;
        let cwd = prompt_optional_path(&theme, "Working directory override (optional)", None)?;
        let monitored_entity_ids = prompt_csv_strings(
            &theme,
            "Monitored entity ids (comma-separated, blank for none)",
        )?;
        let allowed_service_domains = prompt_csv_strings(
            &theme,
            "Allowed service domains (comma-separated, blank for none)",
        )?;
        let allowed_service_entity_ids = prompt_csv_strings(
            &theme,
            "Allowed service entity ids (comma-separated, blank for none)",
        )?;
        let access_token_keychain_account = Some(store_api_key(
            &format!("connector:home-assistant:{id}"),
            &access_token,
        )?);
        let connector = HomeAssistantConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            base_url,
            access_token_keychain_account,
            monitored_entity_ids,
            allowed_service_domains,
            allowed_service_entity_ids,
            entity_cursors: Vec::new(),
            alias,
            requested_model: model,
            cwd,
        };
        let _: HomeAssistantConnectorConfig = self
            .client
            .post(
                "/v1/home-assistant",
                &HomeAssistantConnectorUpsertRequest {
                    connector: connector.clone(),
                    access_token: Some(access_token),
                },
            )
            .await?;
        self.open_static_overlay(
            "Home Assistant Connectors",
            format!("configured Home Assistant connector {}", name),
        );
        Ok(())
    }

    async fn run_provider_browser_login(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .get_provider(provider_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        let kind = hosted_kind_for_provider(&provider).ok_or_else(|| {
            anyhow!(
                "provider '{}' does not support browser sign-in",
                provider.id
            )
        })?;
        let login = complete_browser_login(kind, &provider.display_name).await?;
        let mut updated_provider = provider.clone();
        updated_provider.keychain_account = None;
        match login {
            BrowserLoginResult::ApiKey(api_key) => {
                updated_provider.kind = hosted_kind_to_provider_kind(kind);
                updated_provider.base_url = default_hosted_url(kind).to_string();
                updated_provider.auth_mode = AuthMode::ApiKey;
                updated_provider.oauth = None;
                let _: agent_core::ProviderConfig = self
                    .client
                    .post(
                        "/v1/providers",
                        &ProviderUpsertRequest {
                            provider: updated_provider,
                            api_key: Some(api_key),
                            oauth_token: None,
                        },
                    )
                    .await?;
            }
            BrowserLoginResult::OAuthToken(token) => {
                updated_provider.kind = browser_hosted_kind_to_provider_kind(kind);
                updated_provider.base_url = default_browser_hosted_url(kind).to_string();
                updated_provider.auth_mode = AuthMode::OAuth;
                updated_provider.oauth = Some(openai_browser_oauth_config());
                let _: agent_core::ProviderConfig = self
                    .client
                    .post(
                        "/v1/providers",
                        &ProviderUpsertRequest {
                            provider: updated_provider,
                            api_key: None,
                            oauth_token: Some(token),
                        },
                    )
                    .await?;
            }
        }
        self.refresh_active_model_metadata().await?;
        self.open_static_overlay(
            "Providers",
            format!("updated browser credentials for {}", provider.display_name),
        );
        Ok(())
    }

    async fn run_provider_oauth_login(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .get_provider(provider_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        if provider.auth_mode != AuthMode::OAuth || provider.oauth.is_none() {
            return Err(anyhow!(
                "provider '{}' is not configured for OAuth sign-in",
                provider.id
            ));
        }
        let token = complete_oauth_login(&provider).await?;
        let mut updated_provider = provider.clone();
        updated_provider.keychain_account = None;
        let _: agent_core::ProviderConfig = self
            .client
            .post(
                "/v1/providers",
                &ProviderUpsertRequest {
                    provider: updated_provider,
                    api_key: None,
                    oauth_token: Some(token),
                },
            )
            .await?;
        self.refresh_active_model_metadata().await?;
        self.open_static_overlay(
            "Providers",
            format!("updated OAuth credentials for {}", provider.display_name),
        );
        Ok(())
    }

    fn open_help_overlay(&mut self) {
        self.open_static_overlay("Shortcuts", crate::tui::render::help_text().to_string());
    }

    fn open_transcript_overlay(&mut self) {
        self.overlay = Some(OverlayState::Transcript { scroll_back: 0 });
    }

    fn open_static_overlay(&mut self, title: impl Into<String>, body: impl Into<String>) {
        self.overlay = Some(OverlayState::Static {
            title: title.into(),
            body: body.into(),
            scroll: 0,
        });
    }

    async fn handle_overlay_key(&mut self, key: KeyEvent) -> Result<()> {
        let mut close_overlay = false;
        let mut submit_input = None::<(InputPromptAction, String)>;
        let mut input_error = None::<String>;

        match self.overlay.as_mut() {
            Some(OverlayState::Transcript { scroll_back }) => match key {
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.exit_requested = true;
                }
                KeyEvent {
                    code: KeyCode::Char('t'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    close_overlay = true;
                }
                KeyEvent {
                    code: KeyCode::Esc | KeyCode::Enter,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                } => {
                    close_overlay = true;
                }
                KeyEvent {
                    code: KeyCode::Up | KeyCode::Char('k'),
                    ..
                } => {
                    *scroll_back = scroll_back.saturating_add(1);
                }
                KeyEvent {
                    code: KeyCode::Down | KeyCode::Char('j'),
                    ..
                } => {
                    *scroll_back = scroll_back.saturating_sub(1);
                }
                KeyEvent {
                    code: KeyCode::PageUp,
                    ..
                } => {
                    *scroll_back = scroll_back.saturating_add(10);
                }
                KeyEvent {
                    code: KeyCode::PageDown,
                    ..
                } => {
                    *scroll_back = scroll_back.saturating_sub(10);
                }
                KeyEvent {
                    code: KeyCode::Char(' '),
                    modifiers,
                    ..
                } if !modifiers.contains(KeyModifiers::SHIFT) => {
                    *scroll_back = scroll_back.saturating_sub(10);
                }
                KeyEvent {
                    code: KeyCode::Home,
                    ..
                } => {
                    *scroll_back = usize::MAX;
                }
                KeyEvent {
                    code: KeyCode::End, ..
                } => {
                    *scroll_back = 0;
                }
                KeyEvent {
                    code: KeyCode::Char(' '),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::SHIFT) => {
                    *scroll_back = 0;
                }
                _ => {}
            },
            Some(OverlayState::Static { scroll, .. }) => match key {
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.exit_requested = true;
                }
                KeyEvent {
                    code: KeyCode::Esc | KeyCode::Enter,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                }
                | KeyEvent {
                    code: KeyCode::F(1),
                    ..
                } => {
                    close_overlay = true;
                }
                KeyEvent {
                    code: KeyCode::Up | KeyCode::Char('k'),
                    ..
                } => {
                    *scroll = scroll.saturating_sub(1);
                }
                KeyEvent {
                    code: KeyCode::Down | KeyCode::Char('j'),
                    ..
                } => {
                    *scroll = scroll.saturating_add(1);
                }
                KeyEvent {
                    code: KeyCode::PageUp,
                    ..
                } => {
                    *scroll = scroll.saturating_sub(10);
                }
                KeyEvent {
                    code: KeyCode::PageDown,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char(' '),
                    ..
                } => {
                    *scroll = scroll.saturating_add(10);
                }
                KeyEvent {
                    code: KeyCode::Home,
                    ..
                } => {
                    *scroll = 0;
                }
                KeyEvent {
                    code: KeyCode::End, ..
                } => {
                    *scroll = usize::MAX;
                }
                _ => {}
            },
            Some(OverlayState::Input {
                value,
                cursor,
                action,
                ..
            }) => match key {
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.exit_requested = true;
                }
                KeyEvent {
                    code: KeyCode::Esc, ..
                } => {
                    close_overlay = true;
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    let action = action.clone();
                    let submitted = value.trim().to_string();
                    if submitted.is_empty() {
                        input_error = Some("input cannot be empty".to_string());
                    } else {
                        submit_input = Some((action, submitted));
                    }
                }
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    if *cursor > 0 {
                        let previous = previous_char_boundary(value, *cursor);
                        value.drain(previous..*cursor);
                        *cursor = previous;
                    }
                }
                KeyEvent {
                    code: KeyCode::Delete,
                    ..
                } => {
                    if *cursor < value.len() {
                        let next = next_char_boundary(value, *cursor);
                        value.drain(*cursor..next);
                    }
                }
                KeyEvent {
                    code: KeyCode::Left,
                    ..
                } => {
                    *cursor = previous_char_boundary(value, *cursor);
                }
                KeyEvent {
                    code: KeyCode::Right,
                    ..
                } => {
                    *cursor = next_char_boundary(value, *cursor);
                }
                KeyEvent {
                    code: KeyCode::Home,
                    ..
                } => {
                    *cursor = 0;
                }
                KeyEvent {
                    code: KeyCode::Char('a'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    *cursor = 0;
                }
                KeyEvent {
                    code: KeyCode::End, ..
                } => {
                    *cursor = value.len();
                }
                KeyEvent {
                    code: KeyCode::Char('e'),
                    modifiers,
                    ..
                } if modifiers.contains(KeyModifiers::CONTROL) => {
                    *cursor = value.len();
                }
                KeyEvent {
                    code: KeyCode::Char(ch),
                    modifiers,
                    ..
                } if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
                {
                    value.insert(*cursor, ch);
                    *cursor += ch.len_utf8();
                }
                _ => {}
            },
            None => {}
        }

        if close_overlay {
            self.overlay = None;
        }
        if let Some(error) = input_error {
            self.open_static_overlay("Error", error);
        }
        if let Some((action, submitted)) = submit_input {
            self.submit_input_overlay(action, submitted).await?;
        }
        Ok(())
    }

    async fn submit_input_overlay(
        &mut self,
        action: InputPromptAction,
        value: String,
    ) -> Result<()> {
        self.overlay = None;
        match action {
            InputPromptAction::UpdateApiKey { provider_id } => {
                let config = self.storage.load_config()?;
                let provider = config
                    .get_provider(&provider_id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
                let updated: agent_core::ProviderConfig = self
                    .client
                    .post(
                        "/v1/providers",
                        &ProviderUpsertRequest {
                            provider,
                            api_key: Some(value),
                            oauth_token: None,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "API Key",
                    format!("updated API key for {}", updated.display_name),
                );
            }
        }
        Ok(())
    }

    fn scroll_active_view_up(&mut self, amount: usize) {
        match &mut self.overlay {
            Some(OverlayState::Transcript { scroll_back }) => {
                *scroll_back = scroll_back.saturating_add(amount);
            }
            Some(OverlayState::Static { scroll, .. }) => {
                *scroll = scroll.saturating_sub(amount);
            }
            Some(OverlayState::Input { .. }) => {}
            None => {
                if let Some(picker) = &mut self.picker {
                    picker.selected = picker.selected.saturating_sub(amount);
                } else {
                    self.transcript_scroll_back =
                        self.transcript_scroll_back.saturating_add(amount);
                }
            }
        }
    }

    fn scroll_active_view_down(&mut self, amount: usize) {
        match &mut self.overlay {
            Some(OverlayState::Transcript { scroll_back }) => {
                *scroll_back = scroll_back.saturating_sub(amount);
            }
            Some(OverlayState::Static { scroll, .. }) => {
                *scroll = scroll.saturating_add(amount);
            }
            Some(OverlayState::Input { .. }) => {}
            None => {
                if let Some(picker) = &mut self.picker {
                    let len = picker.filtered_len();
                    if len > 0 {
                        picker.selected = (picker.selected + amount).min(len - 1);
                    }
                } else {
                    self.transcript_scroll_back =
                        self.transcript_scroll_back.saturating_sub(amount);
                }
            }
        }
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
    }

    fn insert_input_char(&mut self, ch: char) {
        self.input.insert(self.input_cursor, ch);
        self.input_cursor += ch.len_utf8();
    }

    fn backspace_input(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let previous = previous_char_boundary(&self.input, self.input_cursor);
        self.input.drain(previous..self.input_cursor);
        self.input_cursor = previous;
    }

    fn delete_input(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let next = next_char_boundary(&self.input, self.input_cursor);
        self.input.drain(self.input_cursor..next);
    }

    fn move_cursor_left(&mut self) {
        self.input_cursor = previous_char_boundary(&self.input, self.input_cursor);
    }

    fn move_cursor_right(&mut self) {
        self.input_cursor = next_char_boundary(&self.input, self.input_cursor);
    }

    fn move_cursor_line_start(&mut self) {
        self.input_cursor = line_start_offset(&self.input, self.input_cursor);
    }

    fn move_cursor_line_end(&mut self) {
        self.input_cursor = line_end_offset(&self.input, self.input_cursor);
    }

    fn move_cursor_vertical(&mut self, delta: isize) {
        let (line, column) = cursor_line_and_column(&self.input, self.input_cursor);
        let line_count = input_line_count(&self.input);
        let target_line = if delta.is_negative() {
            line.saturating_sub(delta.unsigned_abs())
        } else {
            (line + delta as usize).min(line_count.saturating_sub(1))
        };
        self.input_cursor = line_column_to_offset(&self.input, target_line, column);
    }
}

fn previous_char_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .char_indices()
        .last()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(input: &str, cursor: usize) -> usize {
    if cursor >= input.len() {
        input.len()
    } else {
        input[cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| cursor + index)
            .unwrap_or(input.len())
    }
}

fn line_start_offset(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end_offset(input: &str, cursor: usize) -> usize {
    input[cursor..]
        .find('\n')
        .map(|index| cursor + index)
        .unwrap_or(input.len())
}

fn cursor_line_and_column(input: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut column = 0usize;

    for ch in input[..cursor].chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    (line, column)
}

fn line_column_to_offset(input: &str, line: usize, column: usize) -> usize {
    let mut current_line = 0usize;
    let mut line_start = 0usize;

    for (index, ch) in input.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = index + 1;
        }
    }

    if current_line < line {
        return input.len();
    }

    let line_end = input[line_start..]
        .find('\n')
        .map(|index| line_start + index)
        .unwrap_or(input.len());

    let mut offset = line_start;
    let mut remaining = column;
    while offset < line_end && remaining > 0 {
        offset = next_char_boundary(input, offset);
        remaining -= 1;
    }
    offset
}

fn input_line_count(input: &str) -> usize {
    input.chars().filter(|ch| *ch == '\n').count() + 1
}

fn boolean_status(enabled: bool) -> String {
    if enabled {
        "enabled".to_string()
    } else {
        "disabled".to_string()
    }
}

fn slugify_identifier(input: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, ' ' | '_' | '-' | '.' | '/' | '\\') {
            Some('-')
        } else {
            None
        };
        if let Some(value) = normalized {
            if value == '-' {
                if !slug.is_empty() && !last_dash {
                    slug.push(value);
                    last_dash = true;
                }
            } else {
                slug.push(value);
                last_dash = false;
            }
        }
    }
    slug.trim_matches('-').to_string()
}

fn prompt_required(theme: &ColorfulTheme, prompt: &str, initial: Option<&str>) -> Result<String> {
    let mut input = Input::<String>::with_theme(theme);
    input = input.with_prompt(prompt);
    if let Some(initial) = initial {
        input = input.with_initial_text(initial.to_string());
    }
    let value = input.interact_text()?.trim().to_string();
    if value.is_empty() {
        Err(anyhow!("{prompt} cannot be empty"))
    } else {
        Ok(value)
    }
}

fn prompt_optional(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<Option<String>> {
    let mut input = Input::<String>::with_theme(theme);
    input = input.with_prompt(prompt).allow_empty(true);
    if let Some(initial) = initial {
        input = input.with_initial_text(initial.to_string());
    }
    let value = input.interact_text()?.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn prompt_secret(theme: &ColorfulTheme, prompt: &str) -> Result<String> {
    let value = Password::with_theme(theme)
        .with_prompt(prompt)
        .with_confirmation("Confirm", "Values did not match")
        .interact()?;
    if value.trim().is_empty() {
        Err(anyhow!("{prompt} cannot be empty"))
    } else {
        Ok(value)
    }
}

fn prompt_required_path(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<PathBuf> {
    Ok(PathBuf::from(prompt_required(theme, prompt, initial)?))
}

fn prompt_optional_path(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<Option<PathBuf>> {
    Ok(prompt_optional(theme, prompt, initial)?.map(PathBuf::from))
}

fn prompt_csv_strings(theme: &ColorfulTheme, prompt: &str) -> Result<Vec<String>> {
    let value = prompt_optional(theme, prompt, None)?.unwrap_or_default();
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn prompt_csv_i64(theme: &ColorfulTheme, prompt: &str) -> Result<Vec<i64>> {
    let value = prompt_optional(theme, prompt, None)?.unwrap_or_default();
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("failed to parse integer value '{value}'"))
        })
        .collect()
}

fn default_webhook_prompt_template() -> String {
    "Connector: {connector_name}\nSummary: {summary}\nPrompt: {prompt}\nDetails: {details}\nPayload:\n{payload_json}".to_string()
}

fn hash_webhook_token_local(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn settings_section_title(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Providers => "Settings: Providers & Login",
        SettingsSection::ModelThinking => "Settings: Model & Thinking",
        SettingsSection::Permissions => "Settings: Permissions",
        SettingsSection::Connectors => "Settings: Connectors",
        SettingsSection::Autonomy => "Settings: Autonomy",
        SettingsSection::MemorySkills => "Settings: Memory & Skills",
        SettingsSection::Delegation => "Settings: Delegation",
        SettingsSection::System => "Settings: System",
    }
}

fn build_thinking_picker_entries(
    descriptor: Option<&ModelDescriptor>,
    current: Option<ThinkingLevel>,
) -> Vec<GenericPickerEntry> {
    let mut items = vec![
        thinking_picker_entry(
            "default",
            Some("Use the model's default reasoning level.".to_string()),
            current.is_none(),
            PickerAction::SetThinking(None),
        ),
        thinking_picker_entry(
            "none",
            Some("Disable additional reasoning effort.".to_string()),
            current == Some(ThinkingLevel::None),
            PickerAction::SetThinking(Some(ThinkingLevel::None)),
        ),
    ];

    let mut supported_levels = Vec::new();
    let level_description = |level: ThinkingLevel| {
        descriptor
            .and_then(|descriptor| {
                descriptor
                    .supported_reasoning_levels
                    .iter()
                    .find(|entry| thinking_level_from_effort(&entry.effort) == Some(level))
                    .and_then(|entry| entry.description.clone())
            })
            .unwrap_or_else(|| default_thinking_description(level).to_string())
    };

    if let Some(descriptor) = descriptor {
        for level in descriptor
            .supported_reasoning_levels
            .iter()
            .filter_map(|entry| thinking_level_from_effort(&entry.effort))
        {
            if !supported_levels.contains(&level) {
                supported_levels.push(level);
            }
        }
    }

    if supported_levels.is_empty() {
        supported_levels.extend([
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::XHigh,
        ]);
    } else if supported_levels.contains(&ThinkingLevel::Low)
        && !supported_levels.contains(&ThinkingLevel::Minimal)
    {
        supported_levels.insert(0, ThinkingLevel::Minimal);
    }

    for level in [
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
    ] {
        if !supported_levels.contains(&level) {
            continue;
        }
        let detail = if level == ThinkingLevel::Minimal {
            "Fastest option; maps to low effort when supported.".to_string()
        } else {
            level_description(level)
        };
        items.push(thinking_picker_entry(
            level.as_str(),
            Some(detail),
            current == Some(level),
            PickerAction::SetThinking(Some(level)),
        ));
    }

    items
}

fn thinking_picker_entry(
    label: &str,
    detail: Option<String>,
    current: bool,
    action: PickerAction,
) -> GenericPickerEntry {
    GenericPickerEntry {
        label: label.to_string(),
        search_text: format!("thinking {label} {}", detail.as_deref().unwrap_or_default()),
        detail,
        current,
        action,
    }
}

fn thinking_level_from_effort(effort: &str) -> Option<ThinkingLevel> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "none" => Some(ThinkingLevel::None),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" | "x-high" | "extra-high" | "extra_high" => Some(ThinkingLevel::XHigh),
        _ => None,
    }
}

fn default_thinking_description(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::None => "Disable additional reasoning effort.",
        ThinkingLevel::Minimal => "Fastest option; maps to low effort when supported.",
        ThinkingLevel::Low => "Fast responses with lighter reasoning.",
        ThinkingLevel::Medium => "Balanced speed and reasoning depth.",
        ThinkingLevel::High => "More deliberate reasoning for harder tasks.",
        ThinkingLevel::XHigh => "Maximum reasoning depth for complex work.",
    }
}

fn hosted_kind_for_provider(provider: &ProviderConfig) -> Option<HostedKindArg> {
    match provider.kind {
        ProviderKind::ChatGptCodex => Some(HostedKindArg::OpenaiCompatible),
        ProviderKind::Anthropic if !provider.local => Some(HostedKindArg::Anthropic),
        ProviderKind::OpenAiCompatible if !provider.local => {
            let normalized = provider.base_url.trim_end_matches('/');
            if normalized == DEFAULT_OPENAI_URL.trim_end_matches('/')
                || normalized == DEFAULT_CHATGPT_CODEX_URL.trim_end_matches('/')
            {
                Some(HostedKindArg::OpenaiCompatible)
            } else if normalized == DEFAULT_OPENROUTER_URL.trim_end_matches('/') {
                Some(HostedKindArg::Openrouter)
            } else if normalized == DEFAULT_MOONSHOT_URL.trim_end_matches('/') {
                Some(HostedKindArg::Moonshot)
            } else if normalized == DEFAULT_VENICE_URL.trim_end_matches('/') {
                Some(HostedKindArg::Venice)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn provider_kind_label(provider: &ProviderConfig) -> &'static str {
    match provider.kind {
        ProviderKind::ChatGptCodex => "chatgpt/codex",
        ProviderKind::OpenAiCompatible => {
            if provider.local {
                "openai-compatible (local)"
            } else {
                "openai-compatible"
            }
        }
        ProviderKind::Anthropic => {
            if provider.local {
                "anthropic (local)"
            } else {
                "anthropic"
            }
        }
        ProviderKind::Ollama => "ollama",
    }
}

fn provider_auth_label(provider: &ProviderConfig) -> &'static str {
    match provider.auth_mode {
        AuthMode::None => "none",
        AuthMode::ApiKey => "api-key",
        AuthMode::OAuth => "oauth",
    }
}

fn browser_action_label(provider: &ProviderConfig) -> &'static str {
    match hosted_kind_for_provider(provider) {
        Some(HostedKindArg::Moonshot | HostedKindArg::Venice) => "Browser portal",
        Some(_) => "Browser sign-in",
        None => "Browser auth",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        browser_action_label, build_thinking_picker_entries, cursor_line_and_column,
        hosted_kind_for_provider, line_column_to_offset, next_char_boundary,
        previous_char_boundary, GenericPickerEntry, ModelPickerEntry, PickerAction, PickerMode,
        PickerState,
    };
    use crate::HostedKindArg;
    use agent_core::{
        AuthMode, ProviderConfig, ProviderKind, ThinkingLevel, DEFAULT_OPENROUTER_URL,
    };
    use agent_providers::{ModelDescriptor, ReasoningLevelDescriptor};

    #[test]
    fn cursor_boundaries_follow_utf8_chars() {
        let input = "aé";
        assert_eq!(next_char_boundary(input, 0), 1);
        assert_eq!(next_char_boundary(input, 1), 3);
        assert_eq!(previous_char_boundary(input, 3), 1);
    }

    #[test]
    fn line_column_round_trips_for_multiline_input() {
        let input = "alpha\nbeta\ngamma";
        let offset = line_column_to_offset(input, 1, 2);
        assert_eq!(cursor_line_and_column(input, offset), (1, 2));
    }

    #[test]
    fn picker_state_filters_models_by_query() {
        let picker = PickerState {
            mode: PickerMode::Model,
            title: "Models".to_string(),
            hint: String::new(),
            empty_message: String::new(),
            query: "frontier".to_string(),
            selected: 0,
            sessions: Vec::new(),
            models: vec![
                ModelPickerEntry {
                    id: "gpt-5.4".to_string(),
                    display_name: "gpt-5.4".to_string(),
                    description: Some("Latest frontier agentic coding model.".to_string()),
                    context_window: Some(272_000),
                    effective_context_window_percent: Some(90),
                },
                ModelPickerEntry {
                    id: "gpt-oss-20b".to_string(),
                    display_name: "gpt-oss-20b".to_string(),
                    description: Some("Open weights".to_string()),
                    context_window: None,
                    effective_context_window_percent: None,
                },
            ],
            items: Vec::new(),
        };

        let filtered = picker.filtered_models();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "gpt-5.4");
        assert_eq!(picker.filtered_len(), 1);
    }

    #[test]
    fn picker_state_filters_generic_items_by_query() {
        let picker = PickerState {
            mode: PickerMode::Config,
            title: "Config".to_string(),
            hint: String::new(),
            empty_message: String::new(),
            query: "network".to_string(),
            selected: 0,
            sessions: Vec::new(),
            models: Vec::new(),
            items: vec![
                GenericPickerEntry {
                    label: "Shell access".to_string(),
                    detail: Some("enabled".to_string()),
                    search_text: "shell".to_string(),
                    current: false,
                    action: PickerAction::OpenConfig,
                },
                GenericPickerEntry {
                    label: "Network access".to_string(),
                    detail: Some("disabled".to_string()),
                    search_text: "network".to_string(),
                    current: false,
                    action: PickerAction::OpenConfig,
                },
            ],
        };

        let filtered = picker.filtered_items();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].label, "Network access");
        assert_eq!(picker.filtered_len(), 1);
    }

    #[test]
    fn thinking_picker_uses_model_advertised_levels() {
        let descriptor = ModelDescriptor {
            id: "gpt-5.4".to_string(),
            display_name: None,
            description: None,
            context_window: None,
            effective_context_window_percent: None,
            show_in_picker: true,
            default_reasoning_effort: Some("medium".to_string()),
            supported_reasoning_levels: vec![
                ReasoningLevelDescriptor {
                    effort: "low".to_string(),
                    description: Some("low desc".to_string()),
                },
                ReasoningLevelDescriptor {
                    effort: "high".to_string(),
                    description: Some("high desc".to_string()),
                },
            ],
            supports_reasoning_summaries: false,
            default_reasoning_summary: None,
            support_verbosity: false,
            default_verbosity: None,
            supports_parallel_tool_calls: false,
            priority: None,
        };

        let entries = build_thinking_picker_entries(Some(&descriptor), Some(ThinkingLevel::High));
        let labels = entries
            .iter()
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["default", "none", "minimal", "low", "high"]);
        assert!(entries
            .iter()
            .any(|entry| entry.label == "high" && entry.current));
    }

    #[test]
    fn hosted_kind_for_provider_maps_known_remote_urls() {
        let provider = ProviderConfig {
            id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENROUTER_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: None,
            keychain_account: None,
            oauth: None,
            local: false,
        };
        assert!(matches!(
            hosted_kind_for_provider(&provider),
            Some(HostedKindArg::Openrouter)
        ));
        assert_eq!(browser_action_label(&provider), "Browser sign-in");
    }
}
