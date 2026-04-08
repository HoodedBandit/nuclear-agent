use super::*;

impl<'a> TuiApp<'a> {
    pub(crate) async fn run_external_action(&mut self, action: ExternalAction) -> Result<()> {
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
            ExternalAction::OnboardReset => self.run_onboarding_reset_flow().await,
            ExternalAction::OpenDashboard => self.open_dashboard().await,
        }
    }

    pub(super) async fn run_onboarding_reset_flow(&mut self) -> Result<()> {
        run_onboarding_reset(self.storage, true).await?;
        self.client = crate::ensure_daemon(self.storage).await?;
        let config = self.storage.load_config()?;
        self.alias = config.main_agent_alias.clone();
        self.session_id = None;
        self.transcript.clear();
        self.transcript_scroll_back = 0;
        self.requested_model = None;
        self.pending_tool_calls.clear();
        self.attachments.clear();
        self.main_target = config.main_target_summary();
        self.thinking_level = config.thinking_level;
        self.task_mode = None;
        self.permission_preset = Some(config.permission_preset);
        self.cwd = current_request_cwd()?;
        self.recent_events.clear();
        self.last_event_cursor = None;
        self.restart_event_poller = true;
        self.refresh_active_model_metadata().await?;
        self.open_static_overlay(
            "Onboarding",
            format!(
                "Reset complete. Fresh setup finished with main alias {}.",
                config
                    .main_agent_alias
                    .as_deref()
                    .unwrap_or("(not configured)")
            ),
        );
        Ok(())
    }

    pub(super) async fn open_dashboard(&mut self) -> Result<()> {
        let ui_url = dashboard_ui_url(self.storage)?;
        let launch_url = dashboard_launch_url(self.storage).await?;
        match opener::open_browser(&launch_url) {
            Ok(_) => self.open_static_overlay(
                "Dashboard",
                format!(
                    "opened an immediate one-time connect link.\nreusable dashboard URL:\n{ui_url}"
                ),
            ),
            Err(error) => self.open_static_overlay(
                "Dashboard",
                format!(
                    "failed to open browser automatically.\nreusable dashboard URL:\n{ui_url}\n\nimmediate one-time connect URL (expires soon):\n{launch_url}\n\nerror: {error}"
                ),
            ),
        }
        Ok(())
    }

    pub(super) async fn run_add_provider_flow(&mut self) -> Result<()> {
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

    pub(super) async fn run_add_webhook_connector_flow(&mut self) -> Result<()> {
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
                    clear_webhook_token: false,
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

    pub(super) async fn run_add_inbox_connector_flow(&mut self) -> Result<()> {
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

    pub(super) async fn run_add_telegram_connector_flow(&mut self) -> Result<()> {
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
        let connector = TelegramConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account: None,
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

    pub(super) async fn run_add_discord_connector_flow(&mut self) -> Result<()> {
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
            "Monitored Discord channel ids (comma-separated, at least one required)",
        )?;
        crate::connector_cli::ensure_discord_monitored_channel_ids(&monitored_channel_ids)?;
        let allowed_channel_ids = prompt_csv_strings(
            &theme,
            "Allowed Discord channel ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_strings(
            &theme,
            "Allowed Discord user ids (comma-separated, blank for none)",
        )?;
        let connector = DiscordConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account: None,
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

    pub(super) async fn run_add_slack_connector_flow(&mut self) -> Result<()> {
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
            "Monitored Slack channel ids (comma-separated, at least one required)",
        )?;
        crate::connector_cli::ensure_slack_monitored_channel_ids(&monitored_channel_ids)?;
        let allowed_channel_ids = prompt_csv_strings(
            &theme,
            "Allowed Slack channel ids (comma-separated, blank for none)",
        )?;
        let allowed_user_ids = prompt_csv_strings(
            &theme,
            "Allowed Slack user ids (comma-separated, blank for none)",
        )?;
        let connector = SlackConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            bot_token_keychain_account: None,
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

    pub(super) async fn run_add_signal_connector_flow(&mut self) -> Result<()> {
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

    pub(super) async fn run_add_home_assistant_connector_flow(&mut self) -> Result<()> {
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
            "Monitored entity ids (comma-separated, at least one required)",
        )?;
        crate::connector_cli::ensure_home_assistant_monitored_entity_ids(&monitored_entity_ids)?;
        let allowed_service_domains = prompt_csv_strings(
            &theme,
            "Allowed service domains (comma-separated, blank for none)",
        )?;
        let allowed_service_entity_ids = prompt_csv_strings(
            &theme,
            "Allowed service entity ids (comma-separated, blank for none)",
        )?;
        let connector = HomeAssistantConnectorConfig {
            id,
            name: name.clone(),
            description: description.unwrap_or_default(),
            enabled: true,
            base_url,
            access_token_keychain_account: None,
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

    pub(super) async fn run_provider_browser_login(&mut self, provider_id: &str) -> Result<()> {
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

    pub(super) async fn run_provider_oauth_login(&mut self, provider_id: &str) -> Result<()> {
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
}
