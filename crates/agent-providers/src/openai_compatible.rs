use agent_core::{
    ConversationMessage, MessageRole, ProviderConfig, ProviderProfile, ProviderReply,
    ThinkingLevel, ToolDefinition,
};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use super::attachments::load_image_attachment;
use super::common::{
    ensure_no_attachments, extract_error, extract_text, merge_json_object, role_name,
    string_or_null, trim_slash,
};
use super::oauth::apply_auth_with_overrides;
use super::tooling::{
    openai_output_items, parse_openai_tool_calls, tool_definitions_to_openai,
    validate_tool_definitions,
};
use super::PromptAuthOverrides;

#[allow(dead_code)]
pub(super) async fn run_openai_compatible(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply> {
    run_openai_compatible_with_overrides(
        client,
        provider,
        model,
        messages,
        thinking_level,
        tools,
        PromptAuthOverrides::default(),
    )
    .await
}

pub(super) async fn run_openai_compatible_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
    auth_overrides: PromptAuthOverrides<'_>,
) -> Result<ProviderReply> {
    validate_tool_definitions(tools, "OpenAI-compatible")?;
    let url = format!("{}/chat/completions", trim_slash(&provider.base_url));
    let mut payload = json!({
        "model": model,
        "messages": messages_to_openai(messages)?,
    });
    if let Some(temperature) = openai_compatible_temperature(provider, thinking_level) {
        payload["temperature"] = json!(temperature);
    }
    if let Some(reasoning_payload) = openai_reasoning_payload(provider, thinking_level) {
        merge_json_object(&mut payload, reasoning_payload)?;
    }
    if !tools.is_empty() {
        payload["tools"] = Value::Array(tool_definitions_to_openai(tools));
        payload["tool_choice"] = Value::String("auto".to_string());
    }
    let request = client.post(url).json(&payload);
    let request = apply_auth_with_overrides(
        client,
        provider,
        request,
        auth_overrides.api_key,
        auth_overrides.oauth_token,
    )
    .await?;
    let response = request.send().await.context("failed to send request")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse completion response")?;
    if !status.is_success() {
        bail!(
            "completion failed for '{}' ({status}): {}",
            provider.id,
            extract_error(&body)
        );
    }

    let message = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| anyhow!("provider returned no assistant message"))?;
    let content = message.get("content").map(extract_text).unwrap_or_default();
    let tool_calls = parse_openai_tool_calls(message)?;
    let output_items = openai_output_items(&content, &tool_calls);
    if content.is_empty() && tool_calls.is_empty() {
        bail!("provider returned neither assistant text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content,
        tool_calls,
        provider_payload_json: Some(serde_json::to_string(message)?),
        output_items,
        artifacts: Vec::new(),
        remote_content: Vec::new(),
    })
}

pub(super) fn messages_to_openai(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
    messages
        .iter()
        .map(|message| match message.role {
            MessageRole::System | MessageRole::User => Ok(json!({
                "role": role_name(&message.role),
                "content": openai_message_content(message)?,
            })),
            MessageRole::Assistant => {
                ensure_no_attachments(message, "OpenAI-compatible assistant")?;
                if let Some(raw_message) = &message.provider_payload_json {
                    let stored: Value = serde_json::from_str(raw_message)
                        .context("failed to decode stored OpenAI-compatible assistant payload")?;
                    if stored.get("role").and_then(Value::as_str) == Some("assistant") {
                        return Ok(stored);
                    }
                }
                let mut value = json!({
                    "role": "assistant",
                    "content": string_or_null(&message.content),
                });
                if !message.tool_calls.is_empty() {
                    value["tool_calls"] = Value::Array(
                        message
                            .tool_calls
                            .iter()
                            .map(|tool_call| {
                                json!({
                                    "id": tool_call.id,
                                    "type": "function",
                                    "function": {
                                        "name": tool_call.name,
                                        "arguments": tool_call.arguments,
                                    }
                                })
                            })
                            .collect(),
                    );
                }
                Ok(value)
            }
            MessageRole::Tool => {
                ensure_no_attachments(message, "OpenAI-compatible tool")?;
                Ok(json!({
                    "role": "tool",
                    "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
                    "content": string_or_null(&message.content),
                }))
            }
        })
        .collect()
}

fn openai_message_content(message: &ConversationMessage) -> Result<Value> {
    if message.attachments.is_empty() {
        return Ok(string_or_null(&message.content));
    }

    let mut content = Vec::new();
    if !message.content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": message.content,
        }));
    }
    for attachment in &message.attachments {
        let image = load_image_attachment(attachment)?;
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", image.mime_type, image.data_base64),
            }
        }));
    }
    Ok(Value::Array(content))
}

fn openai_reasoning_payload(
    provider: &ProviderConfig,
    thinking_level: Option<ThinkingLevel>,
) -> Option<Value> {
    let thinking_level = thinking_level?;
    match provider.effective_profile() {
        ProviderProfile::OpenRouter => return openrouter_reasoning_payload(thinking_level),
        ProviderProfile::Moonshot => return None,
        _ => {}
    }

    openai_reasoning_effort(thinking_level).map(|effort| json!({ "reasoning_effort": effort }))
}

fn openai_compatible_temperature(
    provider: &ProviderConfig,
    _thinking_level: Option<ThinkingLevel>,
) -> Option<f64> {
    match provider.effective_profile() {
        ProviderProfile::Moonshot => None,
        _ => Some(0.2),
    }
}

fn openrouter_reasoning_payload(thinking_level: ThinkingLevel) -> Option<Value> {
    openai_reasoning_effort(thinking_level).map(|effort| {
        json!({
            "reasoning": {
                "effort": effort
            }
        })
    })
}

fn openai_reasoning_effort(thinking_level: ThinkingLevel) -> Option<&'static str> {
    match thinking_level {
        ThinkingLevel::None => None,
        ThinkingLevel::Minimal | ThinkingLevel::Low => Some("low"),
        ThinkingLevel::Medium => Some("medium"),
        ThinkingLevel::High | ThinkingLevel::XHigh => Some("high"),
    }
}
