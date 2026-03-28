use super::*;

impl<'a> TuiApp<'a> {
    pub(super) async fn handle_picker_key(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) async fn activate_picker_action(&mut self, action: PickerAction) -> Result<()> {
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
            PickerAction::SwitchChatAlias(alias) => {
                let config = self.storage.load_config()?;
                config
                    .get_alias(&alias)
                    .ok_or_else(|| anyhow!("unknown alias '{alias}'"))?;
                self.alias = Some(alias.clone());
                self.requested_model = None;
                self.refresh_active_model_metadata().await?;
                self.open_static_overlay("Models", format!("current chat alias set to {alias}"));
            }
            PickerAction::SetMainAlias(alias) => {
                let summary: MainTargetSummary = self
                    .client
                    .put(
                        "/v1/main-alias",
                        &agent_core::MainAliasUpdateRequest {
                            alias: alias.clone(),
                        },
                    )
                    .await?;
                self.refresh_main_target_summary()?;
                self.open_static_overlay(
                    "Settings",
                    format!(
                        "default alias set to {} ({}/{})",
                        summary.alias, summary.provider_id, summary.model
                    ),
                );
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
            PickerAction::OpenAliasSwitcher => {
                self.open_alias_switcher().await?;
            }
            PickerAction::OpenCurrentAliasPicker => {
                self.open_current_alias_picker().await?;
            }
            PickerAction::OpenMainAliasPicker => {
                self.open_main_alias_picker().await?;
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
            PickerAction::OpenProviderSwitchPicker => {
                self.open_provider_switch_picker()?;
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
                            clear_webhook_token: false,
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
}
