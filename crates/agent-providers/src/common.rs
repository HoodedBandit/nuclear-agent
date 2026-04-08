use agent_core::{ConversationMessage, MessageRole, ThinkingLevel};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

pub(super) fn trim_slash(url: &str) -> &str {
    url.trim_end_matches('/')
}

pub(super) fn extract_error(body: &Value) -> String {
    for candidate in [
        body.pointer("/error/message"),
        body.pointer("/error/metadata/raw/error/message"),
        body.pointer("/error/metadata/raw/message"),
        body.pointer("/detail"),
        body.pointer("/message"),
        body.pointer("/error_description"),
    ] {
        if let Some(text) = candidate.and_then(Value::as_str).map(str::trim) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }

    if let Some(text) = body
        .pointer("/error/metadata/raw")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return text.to_string();
    }

    if let Some(error) = body.get("error").and_then(Value::as_str).map(str::trim) {
        if !error.is_empty() {
            return error.to_string();
        }
    }

    let provider_name = body
        .pointer("/error/metadata/provider_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let provider_error = body
        .pointer("/error/metadata/raw")
        .map(extract_error)
        .or_else(|| {
            body.pointer("/error/metadata/raw/error").map(|nested| {
                extract_error(&Value::Object(
                    [("error".to_string(), nested.clone())]
                        .into_iter()
                        .collect(),
                ))
            })
        });
    if let Some(provider_error) = provider_error.filter(|text| !text.trim().is_empty()) {
        if let Some(provider_name) = provider_name {
            return format!("{provider_name}: {provider_error}");
        }
        return provider_error;
    }

    if let Some(code) = body
        .pointer("/error/code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return code.to_string();
    }

    body.to_string()
}

pub(super) fn parse_token_endpoint_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }

    let parsed = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return trimmed.to_string(),
    };

    if let Some(text) = parsed
        .get("error_description")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return text.to_string();
    }

    trimmed.to_string()
}

pub(super) fn merge_json_object(target: &mut Value, updates: Value) -> Result<()> {
    let target_object = target
        .as_object_mut()
        .ok_or_else(|| anyhow!("target JSON payload is not an object"))?;
    let updates = updates
        .as_object()
        .ok_or_else(|| anyhow!("update JSON payload is not an object"))?;
    for (key, value) in updates {
        target_object.insert(key.clone(), value.clone());
    }
    Ok(())
}

pub(super) fn parse_argument_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "{}".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn parse_arguments_to_value(arguments: &str) -> Result<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(trimmed)
        .with_context(|| format!("failed to parse tool arguments as JSON: {trimmed}"))
}

pub(super) fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

pub(super) fn string_or_null(value: &str) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value.to_string())
    }
}

pub(super) fn ensure_no_attachments(message: &ConversationMessage, context: &str) -> Result<()> {
    if message.attachments.is_empty() {
        Ok(())
    } else {
        bail!("{context} messages do not support image attachments")
    }
}

pub(super) fn extract_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }

    if let Some(parts) = value.as_array() {
        return parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }

    tracing::warn!("unrecognized model response content: {}", value);
    String::new()
}

pub(super) fn openai_reasoning_effort(thinking_level: ThinkingLevel) -> Option<&'static str> {
    match thinking_level {
        ThinkingLevel::None => None,
        ThinkingLevel::Minimal | ThinkingLevel::Low => Some("low"),
        ThinkingLevel::Medium => Some("medium"),
        ThinkingLevel::High | ThinkingLevel::XHigh => Some("high"),
    }
}
