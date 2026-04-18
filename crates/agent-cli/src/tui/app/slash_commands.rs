use super::*;

impl<'a> TuiApp<'a> {
    pub(super) async fn handle_slash_command(
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
            InteractiveCommand::UpdateStatus => {
                let status = crate::update_cli::request_update_status(&self.client).await?;
                self.open_static_overlay(
                    "Update",
                    crate::update_cli::render_update_status(&status),
                );
            }
            InteractiveCommand::UpdateRun => {
                let status =
                    crate::update_cli::run_update_request(&self.client, Some(std::process::id()))
                        .await?;
                let body = if crate::update_cli::should_exit_for_update(&status) {
                    self.exit_requested = true;
                    format!(
                        "{}\n\nClosing this CLI session so the packaged updater can continue.",
                        crate::update_cli::render_update_status(&status)
                    )
                } else {
                    crate::update_cli::render_update_status(&status)
                };
                self.open_static_overlay("Update", body);
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
                let body = crate::format_memory_records(&memories);
                self.open_static_overlay("Memory Review", body);
            }
            InteractiveCommand::MemoryRebuild { session_id } => {
                let response: agent_core::MemoryRebuildResponse = self
                    .client
                    .post(
                        "/v1/memory/rebuild",
                        &agent_core::MemoryRebuildRequest {
                            session_id,
                            recompute_embeddings: false,
                        },
                    )
                    .await?;
                self.open_static_overlay(
                    "Memory Rebuild",
                    format!(
                        "generated_at={}\nsessions_scanned={}\nobservations_scanned={}\nmemories_upserted={}\nembeddings_refreshed={}",
                        response.generated_at,
                        response.sessions_scanned,
                        response.observations_scanned,
                        response.memories_upserted,
                        response.embeddings_refreshed
                    ),
                );
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
                    if !result.memories.is_empty() {
                        lines.push(crate::format_memory_records(&result.memories));
                    }
                    if !result.transcript_hits.is_empty() {
                        lines.push(crate::format_session_search_hits(&result.transcript_hits));
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
                    self.open_static_overlay("Memory", crate::format_memory_records(&memories));
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
                            evidence_refs: Vec::new(),
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
                self.pending_tool_calls.clear();
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
                    self.task_mode,
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
            InteractiveCommand::Onboard => {
                self.pending_external_action = Some(ExternalAction::OnboardReset);
            }
            InteractiveCommand::ModelShow => {
                self.open_alias_switcher().await?;
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
            InteractiveCommand::ProviderShow => {
                self.open_provider_switch_picker()?;
            }
            InteractiveCommand::ProviderSet(selection) => {
                let new_alias = resolve_interactive_provider_selection(
                    self.storage,
                    self.alias.as_deref(),
                    &selection,
                )?;
                let config = self.storage.load_config()?;
                let summary = config
                    .alias_target_summary(&new_alias)
                    .ok_or_else(|| anyhow!("unknown alias '{new_alias}'"))?;
                self.alias = Some(new_alias.clone());
                self.requested_model = None;
                self.refresh_active_model_metadata().await?;
                self.open_static_overlay(
                    "Providers",
                    format!(
                        "provider set to {} via alias {} ({})",
                        summary.provider_display_name, summary.alias, summary.model
                    ),
                );
            }
            InteractiveCommand::ThinkingShow => {
                self.open_thinking_picker().await?;
            }
            InteractiveCommand::ModeShow => {
                self.open_static_overlay(
                    "Mode",
                    format!("mode={}", task_mode_label(self.task_mode)),
                );
            }
            InteractiveCommand::ThinkingSet(new_level) => {
                self.activate_picker_action(PickerAction::SetThinking(new_level))
                    .await?;
            }
            InteractiveCommand::ModeSet(new_mode) => {
                self.task_mode = new_mode;
                self.open_static_overlay("Mode", format!("mode={}", task_mode_label(new_mode)));
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

    pub(super) async fn complete_prompt(&mut self, response: RunTaskResponse) -> Result<()> {
        self.pending_prompt_snapshot = None;
        self.session_id = Some(response.session_id.clone());
        self.alias = Some(response.alias.clone());
        self.requested_model =
            resolve_requested_model_override(self.storage, self.alias.as_deref(), &response.model)?;
        self.attachments.clear();
        self.transcript = self.storage.list_session_messages(&response.session_id)?;
        self.transcript_scroll_back = 0;
        self.pending_tool_calls.clear();
        self.overlay = None;
        self.refresh_active_model_metadata().await?;
        Ok(())
    }

    pub(super) async fn restore_prompt_snapshot(&mut self) -> Result<()> {
        let Some(snapshot) = self.pending_prompt_snapshot.take() else {
            return Ok(());
        };
        self.session_id = snapshot.session_id;
        self.alias = snapshot.alias;
        self.requested_model = snapshot.requested_model;
        self.transcript = snapshot.transcript;
        self.transcript_scroll_back = snapshot.transcript_scroll_back;
        self.refresh_active_model_metadata().await?;
        Ok(())
    }

    pub(super) async fn open_picker(&mut self, mode: PickerMode) -> Result<()> {
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

    pub(super) fn open_generic_picker(
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
}
