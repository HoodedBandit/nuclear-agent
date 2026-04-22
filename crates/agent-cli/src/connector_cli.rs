use super::connector_management_cli::{
    cleanup_replaced_connector_secret, load_discord_connectors, load_home_assistant_connectors,
    load_signal_connectors, load_slack_connectors, load_telegram_connectors, set_discord_enabled,
    set_home_assistant_enabled, set_signal_enabled, set_slack_enabled, set_telegram_enabled,
};
pub(crate) use super::connector_management_cli::{
    ensure_discord_monitored_channel_ids, ensure_home_assistant_monitored_entity_ids,
    ensure_slack_monitored_channel_ids,
};
use super::*;

pub(crate) async fn telegram_command(storage: &Storage, command: TelegramCommands) -> Result<()> {
    match command {
        TelegramCommands::List { json } => {
            let connectors = load_telegram_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} chats={} users={} alias={} model={} last_update_id={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
                        connector.require_pairing_approval,
                        format_i64_list(&connector.allowed_chat_ids),
                        format_i64_list(&connector.allowed_user_ids),
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
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        TelegramCommands::Get { id, json } => {
            let connector = load_telegram_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!("chat_ids={}", format_i64_list(&connector.allowed_chat_ids));
                println!("user_ids={}", format_i64_list(&connector.allowed_user_ids));
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "last_update_id={}",
                    connector
                        .last_update_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        TelegramCommands::Add(args) => {
            let existing = load_telegram_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            let daemon = try_daemon(storage).await?;
            let previous_account = existing
                .as_ref()
                .and_then(|connector| connector.bot_token_keychain_account.clone());
            let daemon_managed_secret = daemon.is_some() && args.bot_token.is_some();
            let bot_token_keychain_account = match (daemon.as_ref(), args.bot_token.as_deref()) {
                (Some(_), _) => previous_account.clone(),
                (None, Some(bot_token)) => Some(store_api_key(
                    &format!("connector:telegram:{}", args.id),
                    bot_token,
                )?),
                (None, None) => previous_account.clone(),
            };
            if bot_token_keychain_account.is_none() && !daemon_managed_secret {
                bail!("--bot-token is required for new telegram connectors");
            }
            let connector = TelegramConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                allowed_chat_ids: args.chat_ids,
                allowed_user_ids: args.user_ids,
                last_update_id: existing.and_then(|connector| connector.last_update_id),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = daemon {
                let _: TelegramConnectorConfig = client
                    .post(
                        "/v1/telegram",
                        &TelegramConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: args.bot_token,
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_telegram_connector(connector.clone());
                if let Err(error) = storage.save_config(&config) {
                    cleanup_replaced_connector_secret(
                        connector.bot_token_keychain_account.as_deref(),
                        previous_account.as_deref(),
                    );
                    return Err(error);
                }
            }
            println!(
                "telegram='{}' configured require_pairing_approval={} chats={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_i64_list(&connector.allowed_chat_ids),
                format_i64_list(&connector.allowed_user_ids)
            );
        }
        TelegramCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/telegram/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .telegram_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
                config.remove_telegram_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("telegram='{}' removed", id);
        }
        TelegramCommands::Enable { id } => {
            set_telegram_enabled(storage, &id, true).await?;
            println!("telegram='{}' enabled", id);
        }
        TelegramCommands::Disable { id } => {
            set_telegram_enabled(storage, &id, false).await?;
            println!("telegram='{}' disabled", id);
        }
        TelegramCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: TelegramPollResponse = client
                .post(&format!("/v1/telegram/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "telegram='{}' processed_updates={} queued_missions={} pending_approvals={} last_update_id={}",
                response.connector_id,
                response.processed_updates,
                response.queued_missions,
                response.pending_approvals,
                response
                    .last_update_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        TelegramCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: TelegramSendResponse = client
                .post(
                    &format!("/v1/telegram/{}/send", args.id),
                    &TelegramSendRequest {
                        chat_id: args.chat_id,
                        text: args.text,
                        disable_notification: args.disable_notification,
                    },
                )
                .await?;
            println!(
                "telegram='{}' sent message to chat={} message_id={}",
                response.connector_id,
                response.chat_id,
                response
                    .message_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        TelegramCommands::Approvals { command } => match command {
            TelegramApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Telegram, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            TelegramApprovalCommands::Approve { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Approved,
                    note,
                )
                .await?;
                println!(
                    "approved telegram pairing={} connector={} chat={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
            TelegramApprovalCommands::Reject { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Rejected,
                    note,
                )
                .await?;
                println!(
                    "rejected telegram pairing={} connector={} chat={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
        },
    }
    Ok(())
}

pub(crate) async fn discord_command(storage: &Storage, command: DiscordCommands) -> Result<()> {
    match command {
        DiscordCommands::List { json } => {
            let connectors = load_discord_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
                        connector.require_pairing_approval,
                        format_string_list(&connector.monitored_channel_ids),
                        format_string_list(&connector.allowed_channel_ids),
                        format_string_list(&connector.allowed_user_ids),
                        connector.channel_cursors.len(),
                        connector.alias.as_deref().unwrap_or("-"),
                        connector.requested_model.as_deref().unwrap_or("-"),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        DiscordCommands::Get { id, json } => {
            let connector = load_discord_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_channel_ids={}",
                    format_string_list(&connector.monitored_channel_ids)
                );
                println!(
                    "allowed_channel_ids={}",
                    format_string_list(&connector.allowed_channel_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!(
                    "channel_cursors={}",
                    format_discord_channel_cursors(&connector.channel_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        DiscordCommands::Add(args) => {
            let existing = load_discord_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            ensure_discord_monitored_channel_ids(&args.monitored_channel_ids)?;
            let daemon = try_daemon(storage).await?;
            let previous_account = existing
                .as_ref()
                .and_then(|connector| connector.bot_token_keychain_account.clone());
            let daemon_managed_secret = daemon.is_some() && args.bot_token.is_some();
            let bot_token_keychain_account = match (daemon.as_ref(), args.bot_token.as_deref()) {
                (Some(_), _) => previous_account.clone(),
                (None, Some(bot_token)) => Some(store_api_key(
                    &format!("connector:discord:{}", args.id),
                    bot_token,
                )?),
                (None, None) => previous_account.clone(),
            };
            if bot_token_keychain_account.is_none() && !daemon_managed_secret {
                bail!("--bot-token is required for new discord connectors");
            }
            let connector = DiscordConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                monitored_channel_ids: args.monitored_channel_ids,
                allowed_channel_ids: args.allowed_channel_ids,
                allowed_user_ids: args.user_ids,
                channel_cursors: existing
                    .map(|connector| connector.channel_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = daemon {
                let _: DiscordConnectorConfig = client
                    .post(
                        "/v1/discord",
                        &DiscordConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: args.bot_token,
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_discord_connector(connector.clone());
                if let Err(error) = storage.save_config(&config) {
                    cleanup_replaced_connector_secret(
                        connector.bot_token_keychain_account.as_deref(),
                        previous_account.as_deref(),
                    );
                    return Err(error);
                }
            }
            println!(
                "discord='{}' configured require_pairing_approval={} monitored_channels={} allowed_channels={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_string_list(&connector.monitored_channel_ids),
                format_string_list(&connector.allowed_channel_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        DiscordCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/discord/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .discord_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
                config.remove_discord_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("discord='{}' removed", id);
        }
        DiscordCommands::Enable { id } => {
            set_discord_enabled(storage, &id, true).await?;
            println!("discord='{}' enabled", id);
        }
        DiscordCommands::Disable { id } => {
            set_discord_enabled(storage, &id, false).await?;
            println!("discord='{}' disabled", id);
        }
        DiscordCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: DiscordPollResponse = client
                .post(&format!("/v1/discord/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "discord='{}' processed_messages={} queued_missions={} pending_approvals={} updated_channels={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals,
                response.updated_channels
            );
        }
        DiscordCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: DiscordSendResponse = client
                .post(
                    &format!("/v1/discord/{}/send", args.id),
                    &DiscordSendRequest {
                        channel_id: args.channel_id.clone(),
                        content: args.content.clone(),
                    },
                )
                .await?;
            println!(
                "discord='{}' sent message to channel={} message_id={}",
                response.connector_id,
                response.channel_id,
                response.message_id.as_deref().unwrap_or("-")
            );
        }
        DiscordCommands::Approvals { command } => match command {
            DiscordApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Discord, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            DiscordApprovalCommands::Approve { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Approved,
                    note,
                )
                .await?;
                println!(
                    "approved discord pairing={} connector={} channel={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
            DiscordApprovalCommands::Reject { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Rejected,
                    note,
                )
                .await?;
                println!(
                    "rejected discord pairing={} connector={} channel={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
        },
    }
    Ok(())
}

pub(crate) async fn slack_command(storage: &Storage, command: SlackCommands) -> Result<()> {
    match command {
        SlackCommands::List { json } => {
            let connectors = load_slack_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.bot_token_keychain_account.is_some(),
                        connector.require_pairing_approval,
                        format_string_list(&connector.monitored_channel_ids),
                        format_string_list(&connector.allowed_channel_ids),
                        format_string_list(&connector.allowed_user_ids),
                        connector.channel_cursors.len(),
                        connector.alias.as_deref().unwrap_or("-"),
                        connector.requested_model.as_deref().unwrap_or("-"),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        SlackCommands::Get { id, json } => {
            let connector = load_slack_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "bot_token_configured={}",
                    connector.bot_token_keychain_account.is_some()
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_channel_ids={}",
                    format_string_list(&connector.monitored_channel_ids)
                );
                println!(
                    "allowed_channel_ids={}",
                    format_string_list(&connector.allowed_channel_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!(
                    "channel_cursors={}",
                    format_slack_channel_cursors(&connector.channel_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        SlackCommands::Add(args) => {
            let existing = load_slack_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            ensure_slack_monitored_channel_ids(&args.monitored_channel_ids)?;
            let daemon = try_daemon(storage).await?;
            let previous_account = existing
                .as_ref()
                .and_then(|connector| connector.bot_token_keychain_account.clone());
            let daemon_managed_secret = daemon.is_some() && args.bot_token.is_some();
            let bot_token_keychain_account = match (daemon.as_ref(), args.bot_token.as_deref()) {
                (Some(_), _) => previous_account.clone(),
                (None, Some(bot_token)) => Some(store_api_key(
                    &format!("connector:slack:{}", args.id),
                    bot_token,
                )?),
                (None, None) => previous_account.clone(),
            };
            if bot_token_keychain_account.is_none() && !daemon_managed_secret {
                bail!("--bot-token is required for new slack connectors");
            }
            let connector = SlackConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                bot_token_keychain_account,
                require_pairing_approval: args.require_pairing_approval,
                monitored_channel_ids: args.monitored_channel_ids,
                allowed_channel_ids: args.allowed_channel_ids,
                allowed_user_ids: args.user_ids,
                channel_cursors: existing
                    .map(|connector| connector.channel_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = daemon {
                let _: SlackConnectorConfig = client
                    .post(
                        "/v1/slack",
                        &SlackConnectorUpsertRequest {
                            connector: connector.clone(),
                            bot_token: args.bot_token,
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_slack_connector(connector.clone());
                if let Err(error) = storage.save_config(&config) {
                    cleanup_replaced_connector_secret(
                        connector.bot_token_keychain_account.as_deref(),
                        previous_account.as_deref(),
                    );
                    return Err(error);
                }
            }
            println!(
                "slack='{}' configured require_pairing_approval={} monitored_channels={} allowed_channels={} users={} auto_poll=daemon",
                args.id,
                connector.require_pairing_approval,
                format_string_list(&connector.monitored_channel_ids),
                format_string_list(&connector.allowed_channel_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        SlackCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/slack/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .slack_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
                config.remove_slack_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.bot_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("slack='{}' removed", id);
        }
        SlackCommands::Enable { id } => {
            set_slack_enabled(storage, &id, true).await?;
            println!("slack='{}' enabled", id);
        }
        SlackCommands::Disable { id } => {
            set_slack_enabled(storage, &id, false).await?;
            println!("slack='{}' disabled", id);
        }
        SlackCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: SlackPollResponse = client
                .post(&format!("/v1/slack/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "slack='{}' processed_messages={} queued_missions={} pending_approvals={} updated_channels={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals,
                response.updated_channels
            );
        }
        SlackCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: SlackSendResponse = client
                .post(
                    &format!("/v1/slack/{}/send", args.id),
                    &SlackSendRequest {
                        channel_id: args.channel_id.clone(),
                        text: args.text.clone(),
                    },
                )
                .await?;
            println!(
                "slack='{}' sent message to channel={} ts={}",
                response.connector_id,
                response.channel_id,
                response.message_ts.as_deref().unwrap_or("-")
            );
        }
        SlackCommands::Approvals { command } => match command {
            SlackApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Slack, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            SlackApprovalCommands::Approve { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Approved,
                    note,
                )
                .await?;
                println!(
                    "approved slack pairing={} connector={} channel={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
            SlackApprovalCommands::Reject { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Rejected,
                    note,
                )
                .await?;
                println!(
                    "rejected slack pairing={} connector={} channel={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
        },
    }
    Ok(())
}

pub(crate) async fn signal_command(storage: &Storage, command: SignalCommands) -> Result<()> {
    match command {
        SignalCommands::List { json } => {
            let connectors = load_signal_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} account={} cli_path={} require_pairing_approval={} monitored_groups={} allowed_groups={} users={} alias={} model={} cwd={}",
                        agent_core::display_safe_id(&connector.id),
                        agent_core::display_safe_label(&connector.name),
                        connector.enabled,
                        agent_core::display_safe_id(&connector.account),
                        connector
                            .cli_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "signal-cli".to_string()),
                        connector.require_pairing_approval,
                        format_string_list(&connector.monitored_group_ids),
                        format_string_list(&connector.allowed_group_ids),
                        format_string_list(&connector.allowed_user_ids),
                        agent_core::display_safe_label(connector.alias.as_deref().unwrap_or("-")),
                        agent_core::display_safe_model(
                            connector.requested_model.as_deref().unwrap_or("-")
                        ),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        SignalCommands::Get { id, json } => {
            let connector = load_signal_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown signal connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", agent_core::display_safe_id(&connector.id));
                println!("name={}", agent_core::display_safe_label(&connector.name));
                println!("enabled={}", connector.enabled);
                println!(
                    "account={}",
                    agent_core::display_safe_id(&connector.account)
                );
                println!(
                    "cli_path={}",
                    connector
                        .cli_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "signal-cli".to_string())
                );
                println!(
                    "require_pairing_approval={}",
                    connector.require_pairing_approval
                );
                println!(
                    "monitored_group_ids={}",
                    format_string_list(&connector.monitored_group_ids)
                );
                println!(
                    "allowed_group_ids={}",
                    format_string_list(&connector.allowed_group_ids)
                );
                println!(
                    "allowed_user_ids={}",
                    format_string_list(&connector.allowed_user_ids)
                );
                println!(
                    "alias={}",
                    agent_core::display_safe_label(connector.alias.as_deref().unwrap_or("-"))
                );
                println!(
                    "model={}",
                    agent_core::display_safe_model(
                        connector.requested_model.as_deref().unwrap_or("-")
                    )
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        SignalCommands::Add(args) => {
            let connector = SignalConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                account: args.account,
                cli_path: args.cli_path,
                require_pairing_approval: args.require_pairing_approval,
                monitored_group_ids: args.monitored_group_ids,
                allowed_group_ids: args.allowed_group_ids,
                allowed_user_ids: args.user_ids,
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: SignalConnectorConfig = client
                    .post(
                        "/v1/signal",
                        &SignalConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_signal_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "signal='{}' configured account={} monitored_groups={} allowed_groups={} users={} auto_poll=daemon",
                agent_core::display_safe_id(&connector.id),
                agent_core::display_safe_id(&connector.account),
                format_string_list(&connector.monitored_group_ids),
                format_string_list(&connector.allowed_group_ids),
                format_string_list(&connector.allowed_user_ids)
            );
        }
        SignalCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/signal/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let exists = config
                    .signal_connectors
                    .iter()
                    .any(|connector| connector.id == id);
                if !exists {
                    bail!("unknown signal connector '{id}'");
                }
                config.remove_signal_connector(&id);
                storage.save_config(&config)?;
            }
            println!("signal='{}' removed", id);
        }
        SignalCommands::Enable { id } => {
            set_signal_enabled(storage, &id, true).await?;
            println!("signal='{}' enabled", id);
        }
        SignalCommands::Disable { id } => {
            set_signal_enabled(storage, &id, false).await?;
            println!("signal='{}' disabled", id);
        }
        SignalCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: SignalPollResponse = client
                .post(&format!("/v1/signal/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "signal='{}' processed_messages={} queued_missions={} pending_approvals={}",
                response.connector_id,
                response.processed_messages,
                response.queued_missions,
                response.pending_approvals
            );
        }
        SignalCommands::Send(args) => {
            let client = ensure_daemon(storage).await?;
            let response: SignalSendResponse = client
                .post(
                    &format!("/v1/signal/{}/send", args.id),
                    &SignalSendRequest {
                        recipient: args.recipient.clone(),
                        group_id: args.group_id.clone(),
                        text: args.text.clone(),
                    },
                )
                .await?;
            println!(
                "signal='{}' sent message to {}",
                response.connector_id, response.target
            );
        }
        SignalCommands::Approvals { command } => match command {
            SignalApprovalCommands::List { limit, json } => {
                let approvals =
                    load_connector_approvals(storage, ConnectorKind::Signal, limit).await?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&approvals)?);
                } else {
                    println!("{}", format_connector_approvals(&approvals));
                }
            }
            SignalApprovalCommands::Approve { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Approved,
                    note,
                )
                .await?;
                println!(
                    "approved signal pairing={} connector={} conversation={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
            SignalApprovalCommands::Reject { id, note } => {
                let approval = update_connector_approval_status(
                    storage,
                    &id,
                    ConnectorApprovalStatus::Rejected,
                    note,
                )
                .await?;
                println!(
                    "rejected signal pairing={} connector={} conversation={} user={}",
                    approval.id,
                    approval.connector_id,
                    approval.external_chat_display.as_deref().unwrap_or("-"),
                    approval.external_user_display.as_deref().unwrap_or("-")
                );
            }
        },
    }
    Ok(())
}

pub(crate) async fn home_assistant_command(
    storage: &Storage,
    command: HomeAssistantCommands,
) -> Result<()> {
    match command {
        HomeAssistantCommands::List { json } => {
            let connectors = load_home_assistant_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} token={} base_url={} monitored_entities={} service_domains={} service_entities={} tracked_entities={} alias={} model={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.access_token_keychain_account.is_some(),
                        connector.base_url,
                        format_string_list(&connector.monitored_entity_ids),
                        format_string_list(&connector.allowed_service_domains),
                        format_string_list(&connector.allowed_service_entity_ids),
                        connector.entity_cursors.len(),
                        connector.alias.as_deref().unwrap_or("-"),
                        connector.requested_model.as_deref().unwrap_or("-"),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        HomeAssistantCommands::Get { id, json } => {
            let connector = load_home_assistant_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!(
                    "access_token_configured={}",
                    connector.access_token_keychain_account.is_some()
                );
                println!("base_url={}", connector.base_url);
                println!(
                    "monitored_entity_ids={}",
                    format_string_list(&connector.monitored_entity_ids)
                );
                println!(
                    "allowed_service_domains={}",
                    format_string_list(&connector.allowed_service_domains)
                );
                println!(
                    "allowed_service_entity_ids={}",
                    format_string_list(&connector.allowed_service_entity_ids)
                );
                println!(
                    "tracked_entities={}",
                    format_home_assistant_entity_cursors(&connector.entity_cursors)
                );
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        HomeAssistantCommands::Add(args) => {
            let existing = load_home_assistant_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == args.id);
            ensure_home_assistant_monitored_entity_ids(&args.monitored_entity_ids)?;
            let daemon = try_daemon(storage).await?;
            let previous_account = existing
                .as_ref()
                .and_then(|connector| connector.access_token_keychain_account.clone());
            let daemon_managed_secret = daemon.is_some() && args.access_token.is_some();
            let access_token_keychain_account =
                match (daemon.as_ref(), args.access_token.as_deref()) {
                    (Some(_), _) => previous_account.clone(),
                    (None, Some(access_token)) => Some(store_api_key(
                        &format!("connector:home-assistant:{}", args.id),
                        access_token,
                    )?),
                    (None, None) => previous_account.clone(),
                };
            if access_token_keychain_account.is_none() && !daemon_managed_secret {
                bail!("--access-token is required for new home assistant connectors");
            }
            let connector = HomeAssistantConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                enabled: args.enabled,
                base_url: args.base_url.trim_end_matches('/').to_string(),
                access_token_keychain_account,
                monitored_entity_ids: args.monitored_entity_ids,
                allowed_service_domains: args.allowed_service_domains,
                allowed_service_entity_ids: args.allowed_service_entity_ids,
                entity_cursors: existing
                    .map(|connector| connector.entity_cursors)
                    .unwrap_or_default(),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = daemon {
                let _: HomeAssistantConnectorConfig = client
                    .post(
                        "/v1/home-assistant",
                        &HomeAssistantConnectorUpsertRequest {
                            connector: connector.clone(),
                            access_token: args.access_token,
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_home_assistant_connector(connector.clone());
                if let Err(error) = storage.save_config(&config) {
                    cleanup_replaced_connector_secret(
                        connector.access_token_keychain_account.as_deref(),
                        previous_account.as_deref(),
                    );
                    return Err(error);
                }
            }
            println!(
                "home_assistant='{}' configured base_url={} monitored_entities={} service_domains={} service_entities={} auto_poll=daemon",
                args.id,
                connector.base_url,
                format_string_list(&connector.monitored_entity_ids),
                format_string_list(&connector.allowed_service_domains),
                format_string_list(&connector.allowed_service_entity_ids)
            );
        }
        HomeAssistantCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value =
                    client.delete(&format!("/v1/home-assistant/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                let connector = config
                    .home_assistant_connectors
                    .iter()
                    .find(|connector| connector.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
                config.remove_home_assistant_connector(&id);
                storage.save_config(&config)?;
                if let Some(account) = connector.access_token_keychain_account.as_deref() {
                    let _ = delete_secret(account);
                }
            }
            println!("home_assistant='{}' removed", id);
        }
        HomeAssistantCommands::Enable { id } => {
            set_home_assistant_enabled(storage, &id, true).await?;
            println!("home_assistant='{}' enabled", id);
        }
        HomeAssistantCommands::Disable { id } => {
            set_home_assistant_enabled(storage, &id, false).await?;
            println!("home_assistant='{}' disabled", id);
        }
        HomeAssistantCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: HomeAssistantPollResponse = client
                .post(
                    &format!("/v1/home-assistant/{id}/poll"),
                    &serde_json::json!({}),
                )
                .await?;
            println!(
                "home_assistant='{}' processed_entities={} queued_missions={} updated_entities={}",
                response.connector_id,
                response.processed_entities,
                response.queued_missions,
                response.updated_entities
            );
        }
        HomeAssistantCommands::State { id, entity_id } => {
            let client = ensure_daemon(storage).await?;
            let state: HomeAssistantEntityState = client
                .get(&format!("/v1/home-assistant/{id}/entities/{entity_id}"))
                .await?;
            println!("{}", serde_json::to_string_pretty(&state)?);
        }
        HomeAssistantCommands::CallService(args) => {
            let client = ensure_daemon(storage).await?;
            let service_data = args
                .service_data_json
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()
                .context("--service-data-json must be valid JSON")?;
            let response: HomeAssistantServiceCallResponse = client
                .post(
                    &format!("/v1/home-assistant/{}/services", args.id),
                    &HomeAssistantServiceCallRequest {
                        domain: args.domain.clone(),
                        service: args.service.clone(),
                        entity_id: args.entity_id.clone(),
                        service_data,
                    },
                )
                .await?;
            println!(
                "home_assistant='{}' called {}.{} changed_entities={}",
                response.connector_id, response.domain, response.service, response.changed_entities
            );
        }
    }
    Ok(())
}
