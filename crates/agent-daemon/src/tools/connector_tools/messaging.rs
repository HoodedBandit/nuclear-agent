use agent_providers::load_api_key;
use serde::Deserialize;

use super::super::argument_helpers::{
    optional_bool, optional_i64_array, optional_string, optional_string_array, required_i64,
    required_string, required_string_array, truncate,
};
use super::*;

pub(super) fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        tool(
            "configure_telegram_connector",
            "Create or update a Telegram bot connector from a bot token. Defaults to the current alias when alias is omitted and enables pairing approval for unknown chats by default.",
            json!({
                "type": "object",
                "properties": {
                    "bot_token": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "require_pairing_approval": {"type": "boolean"},
                    "allowed_chat_ids": {"type": "array", "items": {"type": "integer"}},
                    "allowed_user_ids": {"type": "array", "items": {"type": "integer"}},
                    "alias": {"type": "string"},
                    "requested_model": {"type": "string"},
                    "cwd": {"type": "string"}
                },
                "required": ["bot_token"],
                "additionalProperties": false
            }),
        ),
        tool(
            "send_telegram_message",
            "Send an outbound Telegram message through a configured bot connector. Use this to reply to Telegram users, send approval outcomes, or push mission updates.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "chat_id": {"type": "integer"},
                    "text": {"type": "string"},
                    "disable_notification": {"type": "boolean"}
                },
                "required": ["chat_id", "text"],
                "additionalProperties": false
            }),
        ),
        tool(
            "configure_discord_connector",
            "Create or update a Discord bot connector from a bot token. Defaults to the current alias when alias is omitted and enables pairing approval for unknown users by default.",
            json!({
                "type": "object",
                "properties": {
                    "bot_token": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "require_pairing_approval": {"type": "boolean"},
                    "monitored_channel_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_channel_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_user_ids": {"type": "array", "items": {"type": "string"}},
                    "alias": {"type": "string"},
                    "requested_model": {"type": "string"},
                    "cwd": {"type": "string"}
                },
                "required": ["bot_token", "monitored_channel_ids"],
                "additionalProperties": false
            }),
        ),
        tool(
            "send_discord_message",
            "Send an outbound Discord message through a configured bot connector. Use this to reply to Discord users, send approval outcomes, or push mission updates.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "channel_id": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["channel_id", "content"],
                "additionalProperties": false
            }),
        ),
        tool(
            "configure_slack_connector",
            "Create or update a Slack bot connector from a bot token. Defaults to the current alias when alias is omitted and enables pairing approval for unknown users by default.",
            json!({
                "type": "object",
                "properties": {
                    "bot_token": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "require_pairing_approval": {"type": "boolean"},
                    "monitored_channel_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_channel_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_user_ids": {"type": "array", "items": {"type": "string"}},
                    "alias": {"type": "string"},
                    "requested_model": {"type": "string"},
                    "cwd": {"type": "string"}
                },
                "required": ["bot_token", "monitored_channel_ids"],
                "additionalProperties": false
            }),
        ),
        tool(
            "send_slack_message",
            "Send an outbound Slack message through a configured bot connector. Use this to reply to Slack users, send approval outcomes, or push mission updates.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "channel_id": {"type": "string"},
                    "text": {"type": "string"}
                },
                "required": ["channel_id", "text"],
                "additionalProperties": false
            }),
        ),
        tool(
            "configure_signal_connector",
            "Create or update a Signal connector backed by a local signal-cli account. Defaults to the current alias when alias is omitted.",
            json!({
                "type": "object",
                "properties": {
                    "account": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "cli_path": {"type": "string"},
                    "require_pairing_approval": {"type": "boolean"},
                    "monitored_group_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_group_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_user_ids": {"type": "array", "items": {"type": "string"}},
                    "alias": {"type": "string"},
                    "requested_model": {"type": "string"},
                    "cwd": {"type": "string"}
                },
                "required": ["account"],
                "additionalProperties": false
            }),
        ),
        tool(
            "send_signal_message",
            "Send an outbound Signal message through a configured connector using signal-cli.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "recipient": {"type": "string"},
                    "group_id": {"type": "string"},
                    "text": {"type": "string"}
                },
                "required": ["text"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub(super) async fn execute_tool_call(
    context: &ToolContext,
    tool_name: &str,
    args: &Value,
) -> Result<Option<String>> {
    let output = match tool_name {
        "configure_telegram_connector" => configure_telegram_connector(context, args).await?,
        "send_telegram_message" => send_telegram_message_tool(context, args).await?,
        "configure_discord_connector" => configure_discord_connector(context, args).await?,
        "send_discord_message" => send_discord_message_tool(context, args).await?,
        "configure_slack_connector" => configure_slack_connector(context, args).await?,
        "send_slack_message" => send_slack_message_tool(context, args).await?,
        "configure_signal_connector" => configure_signal_connector(context, args).await?,
        "send_signal_message" => send_signal_message_tool(context, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(output))
}

async fn configure_telegram_connector(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_connector_admin_allowed(context)?;
    let bot_token = required_string(args, "bot_token")?.trim();
    if bot_token.is_empty() {
        bail!("bot_token must not be empty");
    }
    let bot_profile = fetch_telegram_bot_profile(&context.http_client, bot_token).await?;
    let requested_id = optional_string(args, "id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_connector_id(bot_profile.suggested_id(), "telegram-bot"));
    let existing = {
        let config = context.state.config.read().await;
        config
            .telegram_connectors
            .iter()
            .find(|entry| entry.id == requested_id)
            .cloned()
    };
    let account = store_api_key(&format!("connector:telegram:{requested_id}"), bot_token)?;
    let connector = TelegramConnectorConfig {
        id: requested_id.clone(),
        name: optional_string(args, "name")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.name.clone()))
            .unwrap_or_else(|| bot_profile.display_name()),
        description: optional_string(args, "description")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.description.clone()))
            .unwrap_or_else(|| bot_profile.default_description()),
        enabled: optional_bool(args, "enabled")
            .or_else(|| existing.as_ref().map(|entry| entry.enabled))
            .unwrap_or(true),
        bot_token_keychain_account: Some(account),
        require_pairing_approval: optional_bool(args, "require_pairing_approval")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.require_pairing_approval)
            })
            .unwrap_or(true),
        allowed_chat_ids: optional_i64_array(args, "allowed_chat_ids").unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|entry| entry.allowed_chat_ids.clone())
                .unwrap_or_default()
        }),
        allowed_user_ids: optional_i64_array(args, "allowed_user_ids").unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|entry| entry.allowed_user_ids.clone())
                .unwrap_or_default()
        }),
        last_update_id: existing.as_ref().and_then(|entry| entry.last_update_id),
        alias: optional_string(args, "alias")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|entry| entry.alias.clone()))
            .or_else(|| context.current_alias.clone()),
        requested_model: optional_string(args, "requested_model")
            .map(ToOwned::to_owned)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|entry| entry.requested_model.clone())
            }),
        cwd: optional_string(args, "cwd")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cwd.clone())),
    };
    {
        let mut config = context.state.config.write().await;
        config.upsert_telegram_connector(connector.clone());
        context.state.storage.save_config(&config)?;
    }
    append_log(
        &context.state,
        "info",
        "telegram",
        format!(
            "telegram connector '{}' configured by agent tool (bot=@{})",
            connector.id, bot_profile.username
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector": connector,
        "bot": {
            "id": bot_profile.id,
            "username": bot_profile.username,
            "display_name": bot_profile.display_name(),
        }
    }))
    .context("failed to serialize telegram connector result")
}

async fn send_telegram_message_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }
    let connector =
        resolve_telegram_connector_for_send(context, optional_string(args, "connector_id")).await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "telegram",
        &connector.id,
        "sending messages",
    )?;
    let chat_id = required_i64(args, "chat_id")?;
    let text = required_string(args, "text")?.trim();
    if text.is_empty() {
        bail!("text must not be empty");
    }
    let token_account = connector
        .bot_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "telegram connector '{}' has no bot token configured",
                connector.id
            )
        })?;
    let token = load_api_key(token_account)
        .with_context(|| format!("failed to load telegram bot token for '{}'", connector.id))?;
    let disable_notification = optional_bool(args, "disable_notification").unwrap_or(false);
    let response = send_telegram_message_via_api(
        &context.http_client,
        &token,
        TelegramSendRequest {
            chat_id,
            text: text.to_string(),
            disable_notification,
        },
    )
    .await?;
    append_log(
        &context.state,
        "info",
        "telegram",
        format!(
            "telegram connector '{}' sent outbound tool message to chat {}",
            connector.id, chat_id
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector_id": connector.id,
        "chat_id": chat_id,
        "message_id": response.message_id,
    }))
    .context("failed to serialize telegram send result")
}

async fn configure_discord_connector(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_connector_admin_allowed(context)?;
    let bot_token = required_string(args, "bot_token")?.trim();
    if bot_token.is_empty() {
        bail!("bot_token must not be empty");
    }
    let monitored_channel_ids = required_string_array(args, "monitored_channel_ids")?;
    if monitored_channel_ids.is_empty() {
        bail!("monitored_channel_ids must not be empty");
    }
    let bot_profile = fetch_discord_bot_profile(&context.http_client, bot_token).await?;
    let requested_id = optional_string(args, "id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_connector_id(bot_profile.suggested_id(), "discord-bot"));
    let existing = {
        let config = context.state.config.read().await;
        config
            .discord_connectors
            .iter()
            .find(|entry| entry.id == requested_id)
            .cloned()
    };
    let account = store_api_key(&format!("connector:discord:{requested_id}"), bot_token)?;
    let connector = DiscordConnectorConfig {
        id: requested_id.clone(),
        name: optional_string(args, "name")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.name.clone()))
            .unwrap_or_else(|| bot_profile.display_name()),
        description: optional_string(args, "description")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.description.clone()))
            .unwrap_or_else(|| bot_profile.default_description()),
        enabled: optional_bool(args, "enabled")
            .or_else(|| existing.as_ref().map(|entry| entry.enabled))
            .unwrap_or(true),
        bot_token_keychain_account: Some(account),
        require_pairing_approval: optional_bool(args, "require_pairing_approval")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.require_pairing_approval)
            })
            .unwrap_or(true),
        monitored_channel_ids,
        allowed_channel_ids: optional_string_array(args, "allowed_channel_ids").unwrap_or_else(
            || {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_channel_ids.clone())
                    .unwrap_or_default()
            },
        ),
        allowed_user_ids: optional_string_array(args, "allowed_user_ids").unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|entry| entry.allowed_user_ids.clone())
                .unwrap_or_default()
        }),
        channel_cursors: existing
            .as_ref()
            .map(|entry| entry.channel_cursors.clone())
            .unwrap_or_default(),
        alias: optional_string(args, "alias")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|entry| entry.alias.clone()))
            .or_else(|| context.current_alias.clone()),
        requested_model: optional_string(args, "requested_model")
            .map(ToOwned::to_owned)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|entry| entry.requested_model.clone())
            }),
        cwd: optional_string(args, "cwd")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cwd.clone())),
    };
    {
        let mut config = context.state.config.write().await;
        config.upsert_discord_connector(connector.clone());
        context.state.storage.save_config(&config)?;
    }
    append_log(
        &context.state,
        "info",
        "discord",
        format!(
            "discord connector '{}' configured by agent tool (bot_id={})",
            connector.id, bot_profile.id
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector": connector,
        "bot": {
            "id": bot_profile.id,
            "username": bot_profile.username,
            "display_name": bot_profile.display_name(),
        }
    }))
    .context("failed to serialize discord connector result")
}

async fn send_discord_message_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }
    let connector =
        resolve_discord_connector_for_send(context, optional_string(args, "connector_id")).await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "discord",
        &connector.id,
        "sending messages",
    )?;
    let channel_id = required_string(args, "channel_id")?.trim();
    if channel_id.is_empty() {
        bail!("channel_id must not be empty");
    }
    let content = required_string(args, "content")?.trim();
    if content.is_empty() {
        bail!("content must not be empty");
    }
    let token_account = connector
        .bot_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "discord connector '{}' has no bot token configured",
                connector.id
            )
        })?;
    let token = load_api_key(token_account)
        .with_context(|| format!("failed to load discord bot token for '{}'", connector.id))?;
    let response = send_discord_message_via_api(
        &context.http_client,
        &token,
        DiscordSendRequest {
            channel_id: channel_id.to_string(),
            content: content.to_string(),
        },
    )
    .await?;
    append_log(
        &context.state,
        "info",
        "discord",
        format!(
            "discord connector '{}' sent outbound tool message to channel {}",
            connector.id, channel_id
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector_id": connector.id,
        "channel_id": channel_id,
        "message_id": response.id,
    }))
    .context("failed to serialize discord send result")
}

async fn configure_slack_connector(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_connector_admin_allowed(context)?;
    let bot_token = required_string(args, "bot_token")?.trim();
    if bot_token.is_empty() {
        bail!("bot_token must not be empty");
    }
    let monitored_channel_ids = required_string_array(args, "monitored_channel_ids")?;
    if monitored_channel_ids.is_empty() {
        bail!("monitored_channel_ids must not be empty");
    }
    let bot_profile = fetch_slack_bot_profile(&context.http_client, bot_token).await?;
    let requested_id = optional_string(args, "id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_connector_id(bot_profile.suggested_id(), "slack-bot"));
    let existing = {
        let config = context.state.config.read().await;
        config
            .slack_connectors
            .iter()
            .find(|entry| entry.id == requested_id)
            .cloned()
    };
    let account = store_api_key(&format!("connector:slack:{requested_id}"), bot_token)?;
    let connector = SlackConnectorConfig {
        id: requested_id.clone(),
        name: optional_string(args, "name")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.name.clone()))
            .unwrap_or_else(|| bot_profile.display_name()),
        description: optional_string(args, "description")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.description.clone()))
            .unwrap_or_else(|| bot_profile.default_description()),
        enabled: optional_bool(args, "enabled")
            .or_else(|| existing.as_ref().map(|entry| entry.enabled))
            .unwrap_or(true),
        bot_token_keychain_account: Some(account),
        require_pairing_approval: optional_bool(args, "require_pairing_approval")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.require_pairing_approval)
            })
            .unwrap_or(true),
        monitored_channel_ids,
        allowed_channel_ids: optional_string_array(args, "allowed_channel_ids").unwrap_or_else(
            || {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_channel_ids.clone())
                    .unwrap_or_default()
            },
        ),
        allowed_user_ids: optional_string_array(args, "allowed_user_ids").unwrap_or_else(|| {
            existing
                .as_ref()
                .map(|entry| entry.allowed_user_ids.clone())
                .unwrap_or_default()
        }),
        channel_cursors: existing
            .as_ref()
            .map(|entry| entry.channel_cursors.clone())
            .unwrap_or_default(),
        alias: optional_string(args, "alias")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|entry| entry.alias.clone()))
            .or_else(|| context.current_alias.clone()),
        requested_model: optional_string(args, "requested_model")
            .map(ToOwned::to_owned)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|entry| entry.requested_model.clone())
            }),
        cwd: optional_string(args, "cwd")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cwd.clone())),
    };
    {
        let mut config = context.state.config.write().await;
        config.upsert_slack_connector(connector.clone());
        context.state.storage.save_config(&config)?;
    }
    append_log(
        &context.state,
        "info",
        "slack",
        format!(
            "slack connector '{}' configured by agent tool (team={})",
            connector.id, bot_profile.team
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector": connector,
        "bot": {
            "team": bot_profile.team,
            "display_name": bot_profile.display_name(),
            "user_id": bot_profile.user_id,
            "bot_id": bot_profile.bot_id,
        }
    }))
    .context("failed to serialize slack connector result")
}

async fn send_slack_message_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }
    let connector =
        resolve_slack_connector_for_send(context, optional_string(args, "connector_id")).await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "slack",
        &connector.id,
        "sending messages",
    )?;
    let channel_id = required_string(args, "channel_id")?.trim();
    if channel_id.is_empty() {
        bail!("channel_id must not be empty");
    }
    let text = required_string(args, "text")?.trim();
    if text.is_empty() {
        bail!("text must not be empty");
    }
    let token_account = connector
        .bot_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "slack connector '{}' has no bot token configured",
                connector.id
            )
        })?;
    let token = load_api_key(token_account)
        .with_context(|| format!("failed to load slack bot token for '{}'", connector.id))?;
    let response = send_slack_message_via_api(
        &context.http_client,
        &token,
        SlackSendRequest {
            channel_id: channel_id.to_string(),
            text: text.to_string(),
        },
    )
    .await?;
    append_log(
        &context.state,
        "info",
        "slack",
        format!(
            "slack connector '{}' sent outbound tool message to channel {}",
            connector.id, channel_id
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector_id": connector.id,
        "channel_id": response.channel.unwrap_or_else(|| channel_id.to_string()),
        "message_ts": response.ts,
    }))
    .context("failed to serialize slack send result")
}

async fn configure_signal_connector(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_connector_admin_allowed(context)?;
    if !context.background_shell_allowed || !allow_shell(&context.trust_policy, &context.autonomy) {
        bail!("shell execution is disabled by trust policy");
    }
    let account = required_string(args, "account")?.trim();
    if account.is_empty() {
        bail!("account must not be empty");
    }
    let requested_id = optional_string(args, "id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_connector_id(account.to_string(), "signal"));
    let existing = {
        let config = context.state.config.read().await;
        config
            .signal_connectors
            .iter()
            .find(|entry| entry.id == requested_id)
            .cloned()
    };
    let connector = SignalConnectorConfig {
        id: requested_id.clone(),
        name: optional_string(args, "name")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.name.clone()))
            .unwrap_or_else(|| format!("Signal {}", account)),
        description: optional_string(args, "description")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.description.clone()))
            .unwrap_or_else(|| format!("Signal connector for {account}")),
        enabled: optional_bool(args, "enabled")
            .or_else(|| existing.as_ref().map(|entry| entry.enabled))
            .unwrap_or(true),
        account: account.to_string(),
        cli_path: optional_string(args, "cli_path")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cli_path.clone())),
        require_pairing_approval: optional_bool(args, "require_pairing_approval")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.require_pairing_approval)
            })
            .unwrap_or(true),
        monitored_group_ids: optional_string_array(args, "monitored_group_ids")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.monitored_group_ids.clone())
            })
            .unwrap_or_default(),
        allowed_group_ids: optional_string_array(args, "allowed_group_ids")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_group_ids.clone())
            })
            .unwrap_or_default(),
        allowed_user_ids: optional_string_array(args, "allowed_user_ids")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_user_ids.clone())
            })
            .unwrap_or_default(),
        alias: optional_string(args, "alias")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|entry| entry.alias.clone()))
            .or_else(|| context.current_alias.clone()),
        requested_model: optional_string(args, "requested_model")
            .map(ToOwned::to_owned)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|entry| entry.requested_model.clone())
            }),
        cwd: optional_string(args, "cwd")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cwd.clone())),
    };
    let version = run_signal_cli(&connector, &["--version".to_string()]).await?;
    {
        let mut config = context.state.config.write().await;
        config.upsert_signal_connector(connector.clone());
        context.state.storage.save_config(&config)?;
    }
    append_log(
        &context.state,
        "info",
        "signal",
        format!(
            "signal connector '{}' configured by agent tool (account={})",
            connector.id, connector.account
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector": connector,
        "signal_cli": truncate(version.trim(), 120),
    }))
    .context("failed to serialize signal connector result")
}

async fn send_signal_message_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_shell_allowed || !allow_shell(&context.trust_policy, &context.autonomy) {
        bail!("shell execution is disabled by trust policy");
    }
    let connector =
        resolve_signal_connector_for_use(context, optional_string(args, "connector_id")).await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "signal",
        &connector.id,
        "sending messages",
    )?;
    let text = required_string(args, "text")?.trim();
    if text.is_empty() {
        bail!("text must not be empty");
    }
    let target = send_signal_message_via_cli(
        &connector,
        SignalSendRequest {
            recipient: optional_string(args, "recipient").map(ToOwned::to_owned),
            group_id: optional_string(args, "group_id").map(ToOwned::to_owned),
            text: text.to_string(),
        },
    )
    .await?;
    append_log(
        &context.state,
        "info",
        "signal",
        format!(
            "signal connector '{}' sent outbound tool message to {}",
            connector.id, target
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector_id": connector.id,
        "target": target,
    }))
    .context("failed to serialize signal send result")
}

#[derive(Deserialize)]
struct TelegramGetMeResponse {
    ok: bool,
    result: Option<TelegramBotProfile>,
    description: Option<String>,
}

#[derive(Clone, Deserialize)]
struct TelegramBotProfile {
    id: i64,
    username: String,
    first_name: String,
}

impl TelegramBotProfile {
    fn suggested_id(&self) -> String {
        self.username.clone()
    }
    fn display_name(&self) -> String {
        let name = self.first_name.trim();
        if name.is_empty() {
            format!("@{}", self.username)
        } else {
            name.to_string()
        }
    }
    fn default_description(&self) -> String {
        format!("Telegram bot @{}", self.username)
    }
}

#[derive(Deserialize)]
struct DiscordGetMeResponse {
    id: String,
    username: String,
    #[serde(default)]
    global_name: Option<String>,
}

#[derive(Clone)]
struct DiscordBotProfile {
    id: String,
    username: String,
    global_name: Option<String>,
}

impl DiscordBotProfile {
    fn suggested_id(&self) -> String {
        self.username.clone()
    }
    fn display_name(&self) -> String {
        self.global_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.username.clone())
    }
    fn default_description(&self) -> String {
        format!("Discord bot {}", self.display_name())
    }
}

#[derive(Deserialize)]
struct SlackAuthTestResponse {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    bot_id: Option<String>,
    #[serde(default)]
    team: Option<String>,
}

#[derive(Clone)]
struct SlackBotProfile {
    team: String,
    user: Option<String>,
    user_id: Option<String>,
    bot_id: Option<String>,
}

impl SlackBotProfile {
    fn suggested_id(&self) -> String {
        self.user
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| self.bot_id.clone())
            .unwrap_or_else(|| self.team.clone())
    }
    fn display_name(&self) -> String {
        self.user
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| self.bot_id.clone())
            .unwrap_or_else(|| self.team.clone())
    }
    fn default_description(&self) -> String {
        format!("Slack bot {}", self.display_name())
    }
}

async fn fetch_telegram_bot_profile(
    client: &Client,
    bot_token: &str,
) -> Result<TelegramBotProfile> {
    let response = client
        .get(format!("https://api.telegram.org/bot{bot_token}/getMe"))
        .send()
        .await
        .context("telegram getMe request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read telegram getMe response")?;
    let parsed: TelegramGetMeResponse =
        serde_json::from_str(&body).context("failed to parse telegram getMe response")?;
    if !status.is_success() || !parsed.ok {
        bail!(
            "telegram getMe failed: {}",
            parsed.description.unwrap_or_else(|| status.to_string())
        );
    }
    parsed
        .result
        .ok_or_else(|| anyhow!("telegram getMe did not return bot profile"))
}

async fn fetch_discord_bot_profile(client: &Client, bot_token: &str) -> Result<DiscordBotProfile> {
    let response = client
        .get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bot {bot_token}"))
        .send()
        .await
        .context("discord get current user request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read discord current user response")?;
    if !status.is_success() {
        let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
        let detail = parsed
            .as_ref()
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_else(|| status.as_str());
        bail!("discord get current user failed: {detail}");
    }
    let parsed: DiscordGetMeResponse =
        serde_json::from_str(&body).context("failed to parse discord current user response")?;
    Ok(DiscordBotProfile {
        id: parsed.id,
        username: parsed.username,
        global_name: parsed.global_name,
    })
}

async fn fetch_slack_bot_profile(client: &Client, bot_token: &str) -> Result<SlackBotProfile> {
    let response = client
        .post("https://slack.com/api/auth.test")
        .bearer_auth(bot_token)
        .send()
        .await
        .context("slack auth.test request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read slack auth.test response")?;
    let parsed: SlackAuthTestResponse =
        serde_json::from_str(&body).context("failed to parse slack auth.test response")?;
    if !status.is_success() || !parsed.ok {
        bail!(
            "slack auth.test failed: {}",
            parsed.error.unwrap_or_else(|| status.to_string())
        );
    }
    Ok(SlackBotProfile {
        team: parsed.team.unwrap_or_else(|| "slack".to_string()),
        user: parsed.user,
        user_id: parsed.user_id,
        bot_id: parsed.bot_id,
    })
}

async fn resolve_telegram_connector_for_send(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<TelegramConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .telegram_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown telegram connector '{connector_id}'"));
    }
    match config.telegram_connectors.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no telegram connectors are configured"),
        _ => bail!("multiple telegram connectors are configured; specify connector_id"),
    }
}

async fn resolve_discord_connector_for_send(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<DiscordConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .discord_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown discord connector '{connector_id}'"));
    }
    match config.discord_connectors.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no discord connectors are configured"),
        _ => bail!("multiple discord connectors are configured; specify connector_id"),
    }
}

async fn resolve_slack_connector_for_send(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<SlackConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .slack_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown slack connector '{connector_id}'"));
    }
    match config.slack_connectors.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no slack connectors are configured"),
        _ => bail!("multiple slack connectors are configured; specify connector_id"),
    }
}

async fn resolve_signal_connector_for_use(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<SignalConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .signal_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown signal connector '{connector_id}'"));
    }
    match config.signal_connectors.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no signal connectors are configured"),
        _ => bail!("multiple signal connectors are configured; specify connector_id"),
    }
}

async fn run_signal_cli(connector: &SignalConnectorConfig, args: &[String]) -> Result<String> {
    let executable = connector
        .cli_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("signal-cli"));
    let mut command = Command::new(&executable);
    command.kill_on_drop(true);
    command.args(args);
    let output = timeout(
        Duration::from_secs(SIGNAL_CLI_TIMEOUT_SECS),
        command.output(),
    )
    .await
    .map_err(|_| {
        anyhow!(
            "signal-cli timed out for connector '{}' after {}s",
            connector.id,
            SIGNAL_CLI_TIMEOUT_SECS
        )
    })?
    .with_context(|| {
        format!(
            "failed to execute signal-cli '{}' for connector '{}'",
            executable.display(),
            connector.id
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "signal-cli failed for connector '{}': {}",
            connector.id,
            if stderr.is_empty() {
                output.status.to_string()
            } else {
                stderr
            }
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn send_signal_message_via_cli(
    connector: &SignalConnectorConfig,
    request: SignalSendRequest,
) -> Result<String> {
    let recipient = request
        .recipient
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let group_id = request
        .group_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if recipient.is_none() && group_id.is_none() {
        bail!("signal send requires recipient or group_id");
    }
    if let Some(group_id) = group_id.as_deref() {
        if !connector.allowed_group_ids.is_empty()
            && !connector
                .allowed_group_ids
                .iter()
                .any(|value| value.trim() == group_id)
        {
            bail!(
                "signal group '{}' is not allowed for connector '{}'",
                group_id,
                connector.id
            );
        }
    }
    if let Some(recipient) = recipient.as_deref() {
        if !connector.allowed_user_ids.is_empty()
            && !connector
                .allowed_user_ids
                .iter()
                .any(|value| value.trim() == recipient)
        {
            bail!(
                "signal recipient '{}' is not allowed for connector '{}'",
                recipient,
                connector.id
            );
        }
    }
    let mut args = vec![
        "-a".to_string(),
        connector.account.trim().to_string(),
        "send".to_string(),
        "-m".to_string(),
        request.text,
    ];
    let target = if let Some(group_id) = group_id {
        args.push("-g".to_string());
        args.push(group_id.clone());
        format!("group {group_id}")
    } else {
        let recipient = recipient.ok_or_else(|| {
            anyhow!(
                "signal connector '{}' requires either recipient or group_id",
                connector.id
            )
        })?;
        args.push(recipient.clone());
        recipient
    };
    let _ = run_signal_cli(connector, &args).await?;
    Ok(target)
}

#[derive(Deserialize)]
struct TelegramSendToolResponse {
    ok: bool,
    result: Option<TelegramSendToolMessage>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct TelegramSendToolMessage {
    message_id: i64,
}

async fn send_telegram_message_via_api(
    client: &Client,
    token: &str,
    request: TelegramSendRequest,
) -> Result<TelegramSendToolMessage> {
    let response = client
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&request)
        .send()
        .await
        .context("telegram sendMessage request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read telegram sendMessage response")?;
    let parsed: TelegramSendToolResponse =
        serde_json::from_str(&body).context("failed to parse telegram sendMessage response")?;
    if !status.is_success() || !parsed.ok {
        bail!(
            "telegram sendMessage failed: {}",
            parsed.description.unwrap_or_else(|| status.to_string())
        );
    }
    parsed
        .result
        .ok_or_else(|| anyhow!("telegram sendMessage did not return a message"))
}

#[derive(Deserialize)]
struct DiscordSendToolResponse {
    id: String,
}

#[derive(Deserialize)]
struct SlackSendToolResponse {
    ok: bool,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

async fn send_discord_message_via_api(
    client: &Client,
    token: &str,
    request: DiscordSendRequest,
) -> Result<DiscordSendToolResponse> {
    let response = client
        .post(format!(
            "https://discord.com/api/v10/channels/{}/messages",
            request.channel_id
        ))
        .header("Authorization", format!("Bot {token}"))
        .json(&serde_json::json!({ "content": request.content }))
        .send()
        .await
        .context("discord create message request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read discord create message response")?;
    if !status.is_success() {
        let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
        let detail = parsed
            .as_ref()
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or_else(|| status.as_str());
        bail!("discord create message failed: {detail}");
    }
    serde_json::from_str::<DiscordSendToolResponse>(&body)
        .context("failed to parse discord create message response")
}

async fn send_slack_message_via_api(
    client: &Client,
    token: &str,
    request: SlackSendRequest,
) -> Result<SlackSendToolResponse> {
    let response = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await
        .context("slack chat.postMessage request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read slack chat.postMessage response")?;
    let parsed: SlackSendToolResponse =
        serde_json::from_str(&body).context("failed to parse slack chat.postMessage response")?;
    if !status.is_success() || !parsed.ok {
        bail!(
            "slack chat.postMessage failed: {}",
            parsed.error.unwrap_or_else(|| status.to_string())
        );
    }
    Ok(parsed)
}
