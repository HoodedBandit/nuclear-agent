use agent_core::{
    ConversationMessage, MessageRole, ProviderConfig, ProviderReply, ThinkingLevel, ToolDefinition,
};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use super::attachments::load_image_attachment;
use super::common::{
    ensure_no_attachments, extract_error, parse_arguments_to_value, role_name, trim_slash,
};
use super::tooling::{
    openai_output_items, parse_ollama_tool_calls, tool_definitions_to_openai,
    validate_tool_definitions,
};

pub(super) async fn run_ollama(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    _thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply> {
    validate_tool_definitions(tools, "Ollama")?;
    let url = format!("{}/api/chat", trim_slash(&provider.base_url));
    let mut payload = json!({
        "model": model,
        "messages": messages_to_ollama(messages)?,
        "stream": false
    });
    if !tools.is_empty() {
        payload["tools"] = Value::Array(tool_definitions_to_openai(tools));
    }
    let response = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .context("failed to send Ollama request")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse Ollama response")?;
    if !status.is_success() {
        bail!("ollama request failed: {}", extract_error(&body));
    }

    let message = body
        .get("message")
        .ok_or_else(|| anyhow!("Ollama response contained no message"))?;
    let content = message
        .get("content")
        .map(extract_error_text)
        .unwrap_or_default();
    let tool_calls = parse_ollama_tool_calls(message)?;
    let output_items = openai_output_items(&content, &tool_calls);
    if content.is_empty() && tool_calls.is_empty() {
        bail!("Ollama response contained neither text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content,
        tool_calls,
        provider_payload_json: None,
        output_items,
        artifacts: Vec::new(),
        remote_content: Vec::new(),
    })
}

pub(super) fn messages_to_ollama(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
    messages
        .iter()
        .map(|message| match message.role {
            MessageRole::System | MessageRole::User => {
                let mut value = json!({
                    "role": role_name(&message.role),
                    "content": message.content,
                });
                if !message.attachments.is_empty() {
                    value["images"] = Value::Array(ollama_images(message)?);
                }
                Ok(value)
            }
            MessageRole::Assistant => {
                ensure_no_attachments(message, "Ollama assistant")?;
                let mut value = json!({
                    "role": "assistant",
                    "content": message.content,
                });
                if !message.tool_calls.is_empty() {
                    value["tool_calls"] = Value::Array(
                        message
                            .tool_calls
                            .iter()
                            .map(|tool_call| {
                                json!({
                                    "function": {
                                        "name": tool_call.name,
                                        "arguments": parse_arguments_to_value(&tool_call.arguments).unwrap_or_else(|_| json!({})),
                                    }
                                })
                            })
                            .collect(),
                    );
                }
                Ok(value)
            }
            MessageRole::Tool => {
                ensure_no_attachments(message, "Ollama tool")?;
                Ok(json!({
                    "role": "tool",
                    "content": message.content,
                }))
            }
        })
        .collect()
}

fn ollama_images(message: &ConversationMessage) -> Result<Vec<Value>> {
    message
        .attachments
        .iter()
        .map(|attachment| {
            Ok(Value::String(
                load_image_attachment(attachment)?.data_base64,
            ))
        })
        .collect()
}

fn extract_error_text(value: &Value) -> String {
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

    String::new()
}
