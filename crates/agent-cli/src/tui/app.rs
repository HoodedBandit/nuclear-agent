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
mod state;
mod support;
#[cfg(test)]
mod tests;

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
pub(crate) use state::*;

pub(super) const SESSION_PICKER_LIMIT: usize = crate::SESSION_PICKER_LIMIT;
const UI_HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn build_http_client() -> Client {
    Client::builder()
        .timeout(UI_HTTP_TIMEOUT)
        .build()
        .unwrap_or_else(|_| Client::new())
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
            agent_core::RunTaskStreamEvent::RemoteContent { .. } => {}
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
