use super::*;

impl<'a> TuiApp<'a> {
    pub(super) async fn open_model_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider_id = resolve_active_alias(&config, self.alias.as_deref())?
            .provider_id
            .clone();
        self.open_provider_model_picker(&provider_id, false).await
    }

    pub(super) async fn open_provider_model_picker(
        &mut self,
        provider_id: &str,
        set_as_main: bool,
    ) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let provider = config
            .resolve_provider(provider_id)
            .ok_or_else(|| anyhow!("unknown provider '{provider_id}'"))?;
        let provider_name = if provider.display_name.trim().is_empty() {
            provider.id.clone()
        } else {
            provider.display_name.clone()
        };
        let selected_model = if set_as_main {
            config
                .main_target_summary()
                .filter(|summary| summary.provider_id == provider_id)
                .map(|summary| summary.model)
                .unwrap_or_else(|| provider.default_model.clone().unwrap_or_default())
        } else if active_alias.provider_id == provider_id {
            resolved_requested_model(active_alias, self.requested_model.as_deref()).to_string()
        } else if let Some(alias) = self.preferred_provider_alias(&config, provider_id) {
            alias.model.clone()
        } else {
            provider.default_model.clone().unwrap_or_default()
        };

        let listed = timeout(
            Duration::from_secs(3),
            list_model_descriptors(&build_http_client(), &provider),
        )
        .await;

        let mut models = match listed {
            Ok(Ok(models)) => models
                .into_iter()
                .filter(|model| model.show_in_picker)
                .map(|model| {
                    let model_id = model.id.clone();
                    ModelPickerEntry {
                        display_name: model
                            .display_name
                            .clone()
                            .unwrap_or_else(|| model_id.clone()),
                        id: model_id.clone(),
                        description: model.description,
                        context_window: model.context_window,
                        effective_context_window_percent: model.effective_context_window_percent,
                        action: PickerAction::SetProviderModel {
                            provider_id: provider.id.clone(),
                            model_id,
                            set_as_main,
                        },
                    }
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
            let fallback_model = if selected_model.trim().is_empty() {
                provider
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "(no model available)".to_string())
            } else {
                selected_model.clone()
            };
            models.push(ModelPickerEntry {
                id: fallback_model.clone(),
                display_name: fallback_model.clone(),
                description: Some("Current model".to_string()),
                context_window: self.context_window_tokens,
                effective_context_window_percent: self.context_window_percent,
                action: PickerAction::SetProviderModel {
                    provider_id: provider.id.clone(),
                    model_id: fallback_model,
                    set_as_main,
                },
            });
        }

        let selected = models
            .iter()
            .position(|entry| entry.id == selected_model)
            .unwrap_or(0);
        let title = if set_as_main {
            format!("Set default main model for {provider_name}")
        } else if active_alias.provider_id == provider_id {
            format!("Select a model for {provider_name}")
        } else {
            format!("Select a model for {provider_name} in this chat")
        };

        self.picker = Some(PickerState {
            mode: PickerMode::Model,
            title,
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

    pub(super) fn open_provider_model_switch_picker(&mut self, set_as_main: bool) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_provider = resolve_active_alias(&config, self.alias.as_deref())
            .ok()
            .map(|alias| alias.provider_id.clone());
        let mut items = config
            .all_providers()
            .into_iter()
            .filter(provider_has_saved_access)
            .map(|provider| {
                let alias = self.preferred_provider_alias(&config, &provider.id);
                let provider_name = if provider.display_name.trim().is_empty() {
                    provider.id.clone()
                } else {
                    provider.display_name.clone()
                };
                let detail = if let Some(alias) = alias {
                    format!(
                        "{} | alias {} | model {}",
                        provider.id, alias.alias, alias.model
                    )
                } else {
                    format!(
                        "{} | no alias yet | default {}",
                        provider.id,
                        provider
                            .default_model
                            .as_deref()
                            .unwrap_or("(choose a model)")
                    )
                };
                let search_text = if let Some(alias) = alias {
                    format!(
                        "{} {} {} {}",
                        provider.id, provider_name, alias.alias, alias.model
                    )
                } else {
                    format!(
                        "{} {} {}",
                        provider.id,
                        provider_name,
                        provider.default_model.as_deref().unwrap_or_default()
                    )
                };
                GenericPickerEntry {
                    label: provider_name.clone(),
                    detail: Some(detail),
                    search_text,
                    current: (!set_as_main
                        && active_provider.as_deref() == Some(provider.id.as_str()))
                        || (set_as_main
                            && config
                                .main_target_summary()
                                .as_ref()
                                .is_some_and(|summary| summary.provider_id == provider.id)),
                    action: PickerAction::OpenProviderModelPicker {
                        provider_id: provider.id.clone(),
                        set_as_main,
                    },
                }
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.label.cmp(&right.label));
        if items.is_empty() {
            self.open_static_overlay(
                "Models",
                "No logged-in providers are ready for model switching.".to_string(),
            );
            return Ok(());
        }
        self.open_generic_picker(
            PickerMode::Provider,
            if set_as_main {
                "Choose a provider for the default main model"
            } else {
                "Choose a provider for this chat"
            },
            "Enter select | Type to filter | Esc cancel",
            "No matching provider.",
            items,
        );
        Ok(())
    }

    pub(super) async fn open_alias_switcher(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let current_alias = self.alias.clone().unwrap_or_else(|| "main".to_string());
        let main_target = config.main_target_summary();
        let current_detail = config
            .alias_target_summary(&current_alias)
            .as_ref()
            .map(|summary| format!("{} / {}", summary.provider_display_name, summary.model))
            .unwrap_or_else(|| "current chat alias".to_string());
        let main_detail = main_target
            .as_ref()
            .map(|summary| {
                format!(
                    "{} -> {} / {}",
                    summary.alias, summary.provider_display_name, summary.model
                )
            })
            .unwrap_or_else(|| "no default alias configured".to_string());
        let selected_model = resolved_requested_model(
            resolve_active_alias(&config, self.alias.as_deref())?,
            self.requested_model.as_deref(),
        )
        .to_string();
        let active_alias = resolve_active_alias(&config, self.alias.as_deref())?;
        let main_model = main_target
            .as_ref()
            .map(|summary| summary.model.clone())
            .unwrap_or_else(|| "(not configured)".to_string());
        let items = vec![
            GenericPickerEntry {
                label: "Change current chat provider and model".to_string(),
                detail: Some(current_detail.clone()),
                search_text: "change current chat provider model logged in services".to_string(),
                current: false,
                action: PickerAction::OpenProviderModelSwitchPicker { set_as_main: false },
            },
            GenericPickerEntry {
                label: "Switch current chat alias".to_string(),
                detail: Some(format!("{} -> {}", current_alias, current_detail)),
                search_text: format!("switch current chat alias {}", current_alias),
                current: false,
                action: PickerAction::OpenCurrentAliasPicker,
            },
            GenericPickerEntry {
                label: "Change current provider model".to_string(),
                detail: Some(selected_model.clone()),
                search_text: format!(
                    "change current provider model {} {}",
                    active_alias.provider_id, selected_model
                ),
                current: false,
                action: PickerAction::OpenProviderModelPicker {
                    provider_id: active_alias.provider_id.clone(),
                    set_as_main: false,
                },
            },
            GenericPickerEntry {
                label: "Change default main provider and model".to_string(),
                detail: Some(main_detail.clone()),
                search_text: "change default main provider model".to_string(),
                current: false,
                action: PickerAction::OpenProviderModelSwitchPicker { set_as_main: true },
            },
            GenericPickerEntry {
                label: "Change default main model".to_string(),
                detail: Some(main_model),
                search_text: "change default main model".to_string(),
                current: false,
                action: PickerAction::OpenProviderModelPicker {
                    provider_id: main_target
                        .as_ref()
                        .map(|summary| summary.provider_id.clone())
                        .unwrap_or_else(|| active_alias.provider_id.clone()),
                    set_as_main: true,
                },
            },
            GenericPickerEntry {
                label: "Set default main alias".to_string(),
                detail: Some(main_detail),
                search_text: "set default main alias provider".to_string(),
                current: false,
                action: PickerAction::OpenMainAliasPicker,
            },
        ];
        self.open_generic_picker(
            PickerMode::Alias,
            "Provider, model, and alias switcher",
            "Enter select | Type to filter | Esc cancel",
            "No matching switch action.",
            items,
        );
        Ok(())
    }

    pub(super) async fn open_current_alias_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let current_alias = self.alias.clone();
        let mut items = config
            .aliases
            .iter()
            .filter_map(|alias| {
                let provider = config.resolve_provider(&alias.provider_id)?;
                let provider_name = if provider.display_name.trim().is_empty() {
                    provider.id.clone()
                } else {
                    provider.display_name.clone()
                };
                Some(GenericPickerEntry {
                    label: alias.alias.clone(),
                    detail: Some(format!("{provider_name} / {}", alias.model)),
                    search_text: format!(
                        "{} {} {} {} {}",
                        alias.alias,
                        alias.provider_id,
                        provider_name,
                        alias.model,
                        alias.description.as_deref().unwrap_or_default()
                    ),
                    current: current_alias.as_deref() == Some(alias.alias.as_str()),
                    action: PickerAction::SwitchChatAlias(alias.alias.clone()),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.detail
                .as_deref()
                .unwrap_or_default()
                .cmp(right.detail.as_deref().unwrap_or_default())
                .then_with(|| left.label.cmp(&right.label))
        });
        self.open_generic_picker(
            PickerMode::Alias,
            "Switch current chat alias",
            "Enter select | Type to filter | Esc cancel",
            "No matching alias.",
            items,
        );
        Ok(())
    }

    pub(super) async fn open_main_alias_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let current_main_alias = config.main_agent_alias.clone();
        let mut items = config
            .aliases
            .iter()
            .filter_map(|alias| {
                let provider = config.resolve_provider(&alias.provider_id)?;
                let provider_name = if provider.display_name.trim().is_empty() {
                    provider.id.clone()
                } else {
                    provider.display_name.clone()
                };
                Some(GenericPickerEntry {
                    label: alias.alias.clone(),
                    detail: Some(format!("{provider_name} / {}", alias.model)),
                    search_text: format!(
                        "{} {} {} {} {}",
                        alias.alias,
                        alias.provider_id,
                        provider_name,
                        alias.model,
                        alias.description.as_deref().unwrap_or_default()
                    ),
                    current: current_main_alias.as_deref() == Some(alias.alias.as_str()),
                    action: PickerAction::SetMainAlias(alias.alias.clone()),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.detail
                .as_deref()
                .unwrap_or_default()
                .cmp(right.detail.as_deref().unwrap_or_default())
                .then_with(|| left.label.cmp(&right.label))
        });
        self.open_generic_picker(
            PickerMode::Alias,
            "Set default main alias",
            "Enter select | Type to filter | Esc cancel",
            "No matching alias.",
            items,
        );
        Ok(())
    }

    pub(super) async fn open_thinking_picker(&mut self) -> Result<()> {
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

    pub(super) fn open_permission_picker(&mut self) {
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

    pub(super) async fn open_delegation_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_config_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_settings_section_picker(
        &mut self,
        section: SettingsSection,
    ) -> Result<()> {
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
                    label: "Current chat and default main".to_string(),
                    detail: Some(format!(
                        "chat {} -> {} / {}",
                        active_alias.alias, active_alias.provider_id, active_alias.model
                    )),
                    search_text: format!(
                        "current chat default alias provider {} {} {}",
                        active_alias.alias, active_alias.provider_id, active_alias.model
                    ),
                    current: false,
                    action: PickerAction::OpenAliasSwitcher,
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

    pub(super) async fn open_skill_draft_picker(
        &mut self,
        status: Option<SkillDraftStatus>,
    ) -> Result<()> {
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

    pub(super) async fn open_skill_draft_action_picker(&mut self, draft_id: &str) -> Result<()> {
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

    pub(super) async fn show_skill_draft_details(&mut self, draft_id: &str) -> Result<()> {
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
}
