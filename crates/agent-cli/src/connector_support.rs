use agent_core::{DiscordChannelCursor, HomeAssistantEntityCursor, SlackChannelCursor};
use sha2::{Digest, Sha256};

pub(crate) fn hash_webhook_token_local(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub(crate) fn format_i64_list(values: &[i64]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub(crate) fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(",")
    }
}

pub(crate) fn format_discord_channel_cursors(values: &[DiscordChannelCursor]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| match cursor.last_message_id.as_deref() {
                Some(last_message_id) => format!("{}:{last_message_id}", cursor.channel_id),
                None => format!("{}:-", cursor.channel_id),
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub(crate) fn format_slack_channel_cursors(values: &[SlackChannelCursor]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| match cursor.last_message_ts.as_deref() {
                Some(last_message_ts) => format!("{}:{last_message_ts}", cursor.channel_id),
                None => format!("{}:-", cursor.channel_id),
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub(crate) fn format_home_assistant_entity_cursors(values: &[HomeAssistantEntityCursor]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values
            .iter()
            .map(|cursor| {
                format!(
                    "{}:{}@{}",
                    cursor.entity_id,
                    cursor.last_state.as_deref().unwrap_or("-"),
                    cursor.last_changed.as_deref().unwrap_or("-")
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}
