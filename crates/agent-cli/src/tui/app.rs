use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use dialoguer::{theme::ColorfulTheme, Confirm};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use agent_core::{
    AliasUpsertRequest, AppConfig, AuthMode, AutonomyEnableRequest, AutonomyMode, AutonomyProfile,
    AutonomyState, AutopilotConfig, AutopilotState, AutopilotUpdateRequest,
    ConnectorApprovalRecord, ConnectorApprovalUpdateRequest, DaemonConfigUpdateRequest,
    DelegationConfig, DelegationConfigUpdateRequest, DelegationLimit, DelegationTarget,
    DiscordConnectorConfig, DiscordConnectorUpsertRequest, DiscordPollResponse, EvolveConfig,
    EvolveStartRequest, HomeAssistantConnectorConfig, HomeAssistantConnectorUpsertRequest,
    HomeAssistantPollResponse, InboxConnectorConfig, InboxConnectorUpsertRequest,
    InboxPollResponse, InputAttachment, LogEntry, MainTargetSummary, MemoryKind, MemoryRecord,
    MemoryReviewStatus, MemoryReviewUpdateRequest, MemoryScope, MemorySearchQuery,
    MemorySearchResponse, MemoryUpsertRequest, MessageRole, Mission, MissionStatus,
    PermissionPreset, PersistenceMode, ProviderConfig, ProviderKind, ProviderUpsertRequest,
    RunTaskResponse, SessionMessage, SessionSummary, SessionTranscript, SignalConnectorConfig,
    SignalConnectorUpsertRequest, SignalPollResponse, SkillDraft, SkillDraftStatus,
    SlackConnectorConfig, SlackConnectorUpsertRequest, SlackPollResponse, TaskMode,
    TelegramConnectorConfig, TelegramConnectorUpsertRequest, TelegramPollResponse, ThinkingLevel,
    ToolCall, TrustUpdateRequest, WakeTrigger, WebhookConnectorConfig,
    WebhookConnectorUpsertRequest, DEFAULT_CHATGPT_CODEX_URL, DEFAULT_MOONSHOT_URL,
    DEFAULT_OPENAI_URL, DEFAULT_OPENROUTER_URL, DEFAULT_VENICE_URL,
};
use agent_providers::{list_model_descriptors, ModelDescriptor};
use agent_storage::Storage;
use reqwest::Client;
use tokio::time::timeout;
use uuid::Uuid;

mod actions;
mod connector_pickers;
mod pickers;
mod runtime_pickers;
mod settings_pickers;
mod slash_commands;
mod support;

use crate::{
    autonomy_summary, browser_hosted_kind_to_provider_kind, build_compact_prompt,
    build_uncommitted_review_prompt, collect_image_attachments, compact_session,
    complete_browser_login, complete_oauth_login, copy_to_clipboard, current_request_cwd,
    dashboard_launch_url, dashboard_ui_url, default_browser_hosted_url, default_hosted_url,
    execute_prompt, fork_session, hash_webhook_token_local, hosted_kind_to_provider_kind,
    init_agents_file, interactive_provider_setup, load_transcript_for_interactive_fork,
    load_transcript_for_interactive_resume, openai_browser_oauth_config, parse_interactive_command,
    permission_summary, persist_thinking_level, provider_has_saved_access,
    rank_sessions_for_picker, resolve_active_alias, resolve_interactive_model_selection,
    resolve_interactive_provider_selection, resolve_requested_model_override,
    resolve_session_model_override, resolved_requested_model, run_bang_command,
    run_onboarding_reset, task_mode_label, thinking_level_label, BrowserLoginResult, DaemonClient,
    HostedKindArg, InteractiveCommand, InteractiveModelSelection,
};
use support::{
    boolean_status, cursor_line_and_column, default_webhook_prompt_template, input_line_count,
    line_column_to_offset, line_end_offset, line_start_offset, next_char_boundary,
    previous_char_boundary, prompt_csv_i64, prompt_csv_strings, prompt_optional,
    prompt_optional_path, prompt_required, prompt_required_path, prompt_secret,
    settings_section_title, slugify_identifier,
};

use super::events::{spawn_prompt_task, AppEvent, AppEventSender, PromptTask};

pub(super) const SESSION_PICKER_LIMIT: usize = crate::SESSION_PICKER_LIMIT;
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
    SwitchChatAlias(String),
    SetMainAlias(String),
    SetThinking(Option<ThinkingLevel>),
    SetPermission(PermissionPreset),
    OpenConfig,
    OpenSettingsSection(SettingsSection),
    OpenAliasSwitcher,
    OpenCurrentAliasPicker,
    OpenMainAliasPicker,
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
    OpenProviderSwitchPicker,
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
                    "{} {} {} {} {} {} {}",
                    session.id,
                    title,
                    session.alias,
                    session.provider_id,
                    session.model,
                    task_mode_label(session.task_mode),
                    cwd
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
    OnboardReset,
    OpenDashboard,
}

#[derive(Clone)]
struct PromptSnapshot {
    session_id: Option<String>,
    alias: Option<String>,
    requested_model: Option<String>,
    transcript: Vec<SessionMessage>,
    transcript_scroll_back: usize,
}

pub(crate) struct TuiApp<'a> {
    pub(crate) storage: &'a Storage,
    pub(crate) client: DaemonClient,
    pub(crate) alias: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) task_mode: Option<TaskMode>,
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
    pub(crate) pending_tool_calls: Vec<ToolCall>,
    pub(crate) main_target: Option<MainTargetSummary>,
    pub(crate) restart_event_poller: bool,
    pending_prompt_snapshot: Option<PromptSnapshot>,
}

impl<'a> TuiApp<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn new(
        storage: &'a Storage,
        client: DaemonClient,
        alias: Option<String>,
        session_id: Option<String>,
        thinking_level: Option<ThinkingLevel>,
        task_mode: Option<TaskMode>,
        attachments: Vec<InputAttachment>,
        permission_preset: Option<PermissionPreset>,
    ) -> Result<Self> {
        let existing_session = session_id
            .as_deref()
            .and_then(|session_id| storage.get_session(session_id).ok().flatten());
        let cwd = existing_session
            .as_ref()
            .and_then(|session| session.cwd.clone())
            .unwrap_or(current_request_cwd()?);
        let config = storage.load_config()?;
        let transcript = if let Some(session_id) = session_id.as_deref() {
            storage.list_session_messages(session_id)?
        } else {
            Vec::new()
        };
        let alias = alias.or_else(|| config.main_agent_alias.clone());
        let thinking_level = thinking_level.or(config.thinking_level);
        let task_mode = task_mode.or(existing_session
            .as_ref()
            .and_then(|session| session.task_mode));
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
            task_mode,
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
            pending_tool_calls: Vec::new(),
            main_target: config.main_target_summary(),
            restart_event_poller: false,
            pending_prompt_snapshot: None,
        };
        app.requested_model = resolve_session_model_override(
            storage,
            app.session_id.as_deref(),
            app.alias.as_deref(),
        )?;
        app.refresh_main_target_summary()?;
        app.refresh_active_model_metadata().await?;
        Ok(app)
    }

    pub(super) fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub(super) fn take_external_action(&mut self) -> Option<ExternalAction> {
        self.pending_external_action.take()
    }

    pub(super) fn take_restart_event_poller(&mut self) -> bool {
        let restart = self.restart_event_poller;
        self.restart_event_poller = false;
        restart
    }

    fn refresh_main_target_summary(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        self.main_target = config.main_target_summary();
        Ok(())
    }

    pub(super) fn record_error(&mut self, error: impl Into<String>) {
        self.busy = false;
        self.busy_since = None;
        self.open_static_overlay("Error", format!("error: {}", error.into()));
    }

    pub(super) async fn handle_event(&mut self, event: AppEvent) -> Result<()> {
        match event {
            AppEvent::PromptProgress(event) => {
                self.apply_prompt_progress(event);
            }
            AppEvent::PromptFinished(result) => {
                self.busy = false;
                self.busy_since = None;
                match result {
                    Ok(response) => self.complete_prompt(response).await?,
                    Err(error) => {
                        self.restore_prompt_snapshot().await?;
                        self.pending_tool_calls.clear();
                        self.open_static_overlay("Error", format!("error: {error}"));
                    }
                }
            }
            AppEvent::DaemonEvents(events) => {
                self.apply_daemon_events(events);
            }
        }
        Ok(())
    }

    fn apply_prompt_progress(&mut self, event: agent_core::RunTaskStreamEvent) {
        match event {
            agent_core::RunTaskStreamEvent::SessionStarted { alias, .. } => {
                self.alias = Some(alias);
            }
            agent_core::RunTaskStreamEvent::Message { message } => {
                if message.role == MessageRole::Assistant {
                    if message.tool_calls.is_empty() {
                        self.pending_tool_calls.clear();
                    } else {
                        self.pending_tool_calls = message.tool_calls.clone();
                    }
                } else if message.role == MessageRole::Tool {
                    if let Some(call_id) = message.tool_call_id.as_deref() {
                        self.pending_tool_calls.retain(|call| call.id != call_id);
                    }
                }
                self.transcript.push(message);
                self.transcript_scroll_back = 0;
            }
            agent_core::RunTaskStreamEvent::Completed { .. } => {}
            agent_core::RunTaskStreamEvent::Error { .. } => {
                self.pending_tool_calls.clear();
            }
        }
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
                code: KeyCode::Char('p'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_alias_switcher().await?;
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
            .resolve_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let session_id = self
            .session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();
        self.pending_prompt_snapshot = Some(PromptSnapshot {
            session_id: self.session_id.clone(),
            alias: self.alias.clone(),
            requested_model: self.requested_model.clone(),
            transcript: self.transcript.clone(),
            transcript_scroll_back: self.transcript_scroll_back,
        });
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
        self.pending_tool_calls.clear();
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
            task_mode: self.task_mode,
            attachments: self.attachments.clone(),
            permission_preset: self.permission_preset,
            output_schema_json: None,
            ephemeral: false,
        };
        spawn_prompt_task(self.client.clone(), task, event_tx.clone());
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
        PickerState, TuiApp,
    };
    use crate::HostedKindArg;
    use agent_core::{
        plugin_provider_id, AppConfig, AuthMode, InstalledPluginConfig, MainTargetSummary,
        ModelAlias, PermissionPreset, PluginCompatibility, PluginManifest, PluginPermissions,
        PluginProviderAdapterManifest, PluginSourceKind, ProviderConfig, ProviderKind,
        SessionSummary, TaskMode, ThinkingLevel, DEFAULT_OPENROUTER_URL, PLUGIN_SCHEMA_VERSION,
    };
    use agent_providers::{ModelDescriptor, ReasoningLevelDescriptor};
    use agent_storage::Storage;
    use uuid::Uuid;

    fn temp_storage() -> Storage {
        Storage::open_at(std::env::temp_dir().join(format!("nuclear-tui-test-{}", Uuid::new_v4())))
            .unwrap()
    }

    fn sample_provider(id: &str, display_name: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: display_name.to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "http://127.0.0.1:11434".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some(model.to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        }
    }

    fn sample_alias(alias: &str, provider_id: &str, model: &str) -> ModelAlias {
        ModelAlias {
            alias: alias.to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            description: None,
        }
    }

    fn build_test_app<'a>(storage: &'a Storage) -> TuiApp<'a> {
        let config = AppConfig {
            onboarding_complete: true,
            providers: vec![
                sample_provider("codex", "Codex", "gpt-5-codex"),
                sample_provider("anthropic", "Anthropic", "claude-sonnet"),
            ],
            aliases: vec![
                sample_alias("main", "codex", "gpt-5-codex"),
                sample_alias("claude", "anthropic", "claude-sonnet"),
            ],
            main_agent_alias: Some("main".to_string()),
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        TuiApp {
            storage,
            client: crate::DaemonClient {
                base_url: "http://127.0.0.1:9".to_string(),
                token: "test-token".to_string(),
                http: reqwest::Client::new(),
            },
            alias: Some("main".to_string()),
            session_id: None,
            thinking_level: None,
            task_mode: None,
            permission_preset: Some(PermissionPreset::AutoEdit),
            attachments: Vec::new(),
            cwd: std::env::current_dir().unwrap(),
            transcript: Vec::new(),
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
            active_model: Some("gpt-5-codex".to_string()),
            active_provider_name: Some("Codex".to_string()),
            context_window_tokens: None,
            context_window_percent: None,
            recent_events: Vec::new(),
            last_event_cursor: None,
            pending_tool_calls: Vec::new(),
            main_target: Some(MainTargetSummary {
                alias: "main".to_string(),
                provider_id: "codex".to_string(),
                provider_display_name: "Codex".to_string(),
                model: "gpt-5-codex".to_string(),
            }),
            restart_event_poller: false,
            pending_prompt_snapshot: None,
        }
    }

    fn build_projected_plugin_app<'a>(storage: &'a Storage) -> TuiApp<'a> {
        let plugin = InstalledPluginConfig {
            id: "echo-toolkit".to_string(),
            manifest: PluginManifest {
                schema_version: PLUGIN_SCHEMA_VERSION,
                id: "echo-toolkit".to_string(),
                name: "Echo Toolkit".to_string(),
                version: "0.1.0".to_string(),
                description: "test plugin".to_string(),
                homepage: None,
                compatibility: PluginCompatibility::default(),
                permissions: PluginPermissions::default(),
                tools: Vec::new(),
                connectors: Vec::new(),
                provider_adapters: vec![PluginProviderAdapterManifest {
                    id: "echo-provider".to_string(),
                    provider_kind: ProviderKind::OpenAiCompatible,
                    description: "projected provider".to_string(),
                    command: "plugin-host".to_string(),
                    args: Vec::new(),
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    default_model: Some("echo-1".to_string()),
                    timeout_seconds: None,
                }],
            },
            source_kind: PluginSourceKind::LocalPath,
            install_dir: std::env::temp_dir().join(format!("plugin-install-{}", Uuid::new_v4())),
            source_reference: String::new(),
            source_path: std::env::temp_dir().join(format!("plugin-source-{}", Uuid::new_v4())),
            integrity_sha256: "reviewed".to_string(),
            enabled: true,
            trusted: true,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: "reviewed".to_string(),
            reviewed_at: Some(chrono::Utc::now()),
            pinned: false,
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let provider_id = plugin_provider_id(&plugin.id, "echo-provider");
        let config = AppConfig {
            onboarding_complete: true,
            plugins: vec![plugin],
            aliases: vec![sample_alias("main", &provider_id, "echo-1")],
            main_agent_alias: Some("main".to_string()),
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        TuiApp {
            storage,
            client: crate::DaemonClient {
                base_url: "http://127.0.0.1:9".to_string(),
                token: "test-token".to_string(),
                http: reqwest::Client::new(),
            },
            alias: Some("main".to_string()),
            session_id: None,
            thinking_level: None,
            task_mode: None,
            permission_preset: Some(PermissionPreset::AutoEdit),
            attachments: Vec::new(),
            cwd: std::env::current_dir().unwrap(),
            transcript: Vec::new(),
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
            active_model: Some("echo-1".to_string()),
            active_provider_name: Some("Echo Toolkit / echo-provider".to_string()),
            context_window_tokens: None,
            context_window_percent: None,
            recent_events: Vec::new(),
            last_event_cursor: None,
            pending_tool_calls: Vec::new(),
            main_target: Some(MainTargetSummary {
                alias: "main".to_string(),
                provider_id,
                provider_display_name: "Echo Toolkit / echo-provider".to_string(),
                model: "echo-1".to_string(),
            }),
            restart_event_poller: false,
            pending_prompt_snapshot: None,
        }
    }

    #[test]
    fn cursor_boundaries_follow_utf8_chars() {
        let input = "aÃ©";
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
    fn picker_state_filters_sessions_by_task_mode() {
        let now = chrono::Utc::now();
        let picker = PickerState {
            mode: PickerMode::Resume,
            title: "Sessions".to_string(),
            hint: String::new(),
            empty_message: String::new(),
            query: "daily".to_string(),
            selected: 0,
            sessions: vec![
                SessionSummary {
                    id: "session-build".to_string(),
                    title: Some("Build work".to_string()),
                    alias: "main".to_string(),
                    provider_id: "local".to_string(),
                    model: "qwen".to_string(),
                    task_mode: Some(TaskMode::Build),
                    message_count: 4,
                    cwd: None,
                    created_at: now,
                    updated_at: now,
                },
                SessionSummary {
                    id: "session-daily".to_string(),
                    title: Some("Daily tasks".to_string()),
                    alias: "main".to_string(),
                    provider_id: "local".to_string(),
                    model: "qwen".to_string(),
                    task_mode: Some(TaskMode::Daily),
                    message_count: 2,
                    cwd: None,
                    created_at: now,
                    updated_at: now + chrono::Duration::seconds(1),
                },
            ],
            models: Vec::new(),
            items: Vec::new(),
        };

        let filtered = picker.filtered_sessions();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "session-daily");
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

    #[tokio::test]
    async fn dashboard_slash_command_sets_external_action() {
        let storage = temp_storage();
        let mut app = build_test_app(&storage);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        app.handle_slash_command(crate::InteractiveCommand::DashboardOpen, &tx)
            .await
            .unwrap();

        assert!(matches!(
            app.pending_external_action,
            Some(super::ExternalAction::OpenDashboard)
        ));
    }

    #[tokio::test]
    async fn onboard_slash_command_sets_external_action() {
        let storage = temp_storage();
        let mut app = build_test_app(&storage);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        app.handle_slash_command(crate::InteractiveCommand::Onboard, &tx)
            .await
            .unwrap();

        assert!(matches!(
            app.pending_external_action,
            Some(super::ExternalAction::OnboardReset)
        ));
    }

    #[tokio::test]
    async fn mode_slash_command_updates_task_mode() {
        let storage = temp_storage();
        let mut app = build_test_app(&storage);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        app.handle_slash_command(
            crate::InteractiveCommand::ModeSet(Some(TaskMode::Daily)),
            &tx,
        )
        .await
        .unwrap();

        assert_eq!(app.task_mode, Some(TaskMode::Daily));
    }

    #[tokio::test]
    async fn provider_show_opens_switch_picker_with_logged_in_providers() {
        let storage = temp_storage();
        let mut app = build_test_app(&storage);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        app.handle_slash_command(crate::InteractiveCommand::ProviderShow, &tx)
            .await
            .unwrap();

        let picker = app.picker.expect("provider picker should open");
        assert!(matches!(picker.mode, PickerMode::Provider));
        let labels = picker
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["Anthropic", "Codex"]);
        assert!(picker.items.iter().any(|item| item
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("alias main"))));
    }

    #[tokio::test]
    async fn queue_prompt_accepts_projected_plugin_provider_alias() {
        let storage = temp_storage();
        let mut app = build_projected_plugin_app(&storage);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        app.queue_prompt("Plan the week".to_string(), &tx).unwrap();

        assert!(app.busy);
        assert_eq!(app.transcript.len(), 1);
        assert_eq!(
            app.transcript[0].provider_id.as_deref(),
            app.main_target
                .as_ref()
                .map(|target| target.provider_id.as_str())
        );
    }
}
