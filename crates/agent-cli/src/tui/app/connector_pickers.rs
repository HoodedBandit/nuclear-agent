use super::*;

impl<'a> TuiApp<'a> {
    pub(super) fn open_provider_switch_picker(&mut self) -> Result<()> {
        let config = self.storage.load_config()?;
        let active_provider = resolve_active_alias(&config, self.alias.as_deref())
            .ok()
            .map(|alias| alias.provider_id.clone());
        let mut items = config
            .all_providers()
            .into_iter()
            .filter(|provider| provider_has_saved_access(provider))
            .filter_map(|provider| {
                let alias = self.preferred_provider_alias(&config, &provider.id)?;
                let provider_name = if provider.display_name.trim().is_empty() {
                    provider.id.clone()
                } else {
                    provider.display_name.clone()
                };
                Some(GenericPickerEntry {
                    label: provider_name.clone(),
                    detail: Some(format!(
                        "{} | alias {} | model {}",
                        provider.id, alias.alias, alias.model
                    )),
                    search_text: format!(
                        "{} {} {} {}",
                        provider.id, provider_name, alias.alias, alias.model
                    ),
                    current: active_provider.as_deref() == Some(provider.id.as_str()),
                    action: PickerAction::SwitchChatAlias(alias.alias.clone()),
                })
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.label.cmp(&right.label));
        if items.is_empty() {
            self.open_static_overlay(
                "Providers",
                "No usable providers with aliases are configured.".to_string(),
            );
            return Ok(());
        }
        self.open_generic_picker(
            PickerMode::Provider,
            "Switch current provider",
            "Enter select | Type to filter | Esc cancel",
            "No matching provider.",
            items,
        );
        Ok(())
    }

    pub(super) fn open_provider_picker(&mut self) -> Result<()> {
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
        let mut providers = config.all_providers();
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

    pub(super) fn preferred_provider_alias<'b>(
        &self,
        config: &'b AppConfig,
        provider_id: &str,
    ) -> Option<&'b agent_core::ModelAlias> {
        if let Some(alias) = self
            .alias
            .as_deref()
            .and_then(|name| config.get_alias(name))
            .filter(|alias| alias.provider_id == provider_id)
        {
            return Some(alias);
        }

        if let Some(alias) = config
            .main_agent_alias
            .as_deref()
            .and_then(|name| config.get_alias(name))
            .filter(|alias| alias.provider_id == provider_id)
        {
            return Some(alias);
        }

        if let Some(alias) = config
            .get_alias(provider_id)
            .filter(|alias| alias.provider_id == provider_id)
        {
            return Some(alias);
        }

        config
            .aliases
            .iter()
            .filter(|alias| alias.provider_id == provider_id)
            .min_by(|left, right| left.alias.cmp(&right.alias))
    }

    pub(super) fn open_provider_action_picker(&mut self, provider_id: &str) -> Result<()> {
        let config = self.storage.load_config()?;
        let provider = config
            .resolve_provider(provider_id)
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

    pub(super) async fn open_webhook_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_webhook_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_inbox_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_inbox_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_telegram_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_telegram_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_discord_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_discord_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_slack_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_slack_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_signal_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_signal_action_picker(&mut self, connector_id: &str) -> Result<()> {
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

    pub(super) async fn open_home_assistant_picker(&mut self) -> Result<()> {
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

    pub(super) async fn open_home_assistant_action_picker(
        &mut self,
        connector_id: &str,
    ) -> Result<()> {
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
}
