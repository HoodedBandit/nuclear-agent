use super::*;

impl<'a> TuiApp<'a> {
    pub(super) async fn open_persistence_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_autonomy_picker(&mut self) -> Result<()> {
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

    pub(super) fn open_input_overlay(
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

    pub(super) async fn toggle_trust_setting(&mut self, toggle: TrustToggle) -> Result<()> {
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

    pub(super) async fn apply_autonomy_action(&mut self, action: AutonomyMenuAction) -> Result<()> {
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
                            allow_self_edit: None,
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

    pub(super) async fn open_evolve_picker(&mut self) -> Result<()> {
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

    pub(super) async fn apply_evolve_action(&mut self, action: EvolveMenuAction) -> Result<()> {
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

    pub(super) async fn active_model_descriptor(&self) -> Result<Option<ModelDescriptor>> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .resolve_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string();
        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), &provider),
        )
        .await;
        let Ok(Ok(models)) = listed else {
            return Ok(None);
        };
        Ok(models.into_iter().find(|model| model.id == selected_model))
    }

    pub(super) fn show_provider_details(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .resolve_provider(provider_id)
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
            "name={}\nid={}\nkind={}\nauth={}\nbase_url={}\ndefault_model={}\nlocal={}\nsaved_access={}\ndelegation={}\naliases={}",
            provider.display_name,
            provider.id,
            provider_kind_label(&provider),
            provider_auth_label(&provider),
            provider.base_url,
            provider.default_model.as_deref().unwrap_or("(unset)"),
            provider.local,
            boolean_status(provider_has_saved_access(&provider)),
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

    pub(super) fn resume_session(&mut self, session: SessionSummary) -> Result<()> {
        self.alias = Some(session.alias.clone());
        self.session_id = Some(session.id.clone());
        self.requested_model =
            resolve_requested_model_override(self.storage, self.alias.as_deref(), &session.model)?;
        self.task_mode = session.task_mode;
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

    pub(super) fn fork_session(&mut self, session: SessionSummary) -> Result<()> {
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
        self.task_mode = forked.session.task_mode;
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

    pub(super) async fn status_text(&self) -> Result<String> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .resolve_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref());
        let daemon_status: agent_core::DaemonStatus = self.client.get("/v1/status").await?;
        Ok(format!(
            "session={}\nalias={}\nprovider={}\nmodel={}\nthinking={}\nmode={}\npermission_preset={}\nattachments={}\ncwd={}\ndaemon={} auto_start={} autonomy={}",
            self.session_id.as_deref().unwrap_or("(new)"),
            active_alias.alias,
            provider.id,
            selected_model,
            thinking_level_label(self.thinking_level),
            task_mode_label(self.task_mode),
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

    pub(super) async fn refresh_active_model_metadata(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .resolve_provider(&active_alias.provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
        let selected_model =
            resolved_requested_model(active_alias, self.requested_model.as_deref());

        self.active_model = Some(selected_model.to_string());
        self.active_provider_name = Some(provider.display_name.clone());
        self.context_window_tokens = None;
        self.context_window_percent = None;

        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), &provider),
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
}
