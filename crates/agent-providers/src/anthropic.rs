use super::*;
use crate::attachments::{ensure_no_attachments, load_image_attachment};
use crate::oauth::{apply_auth, apply_auth_with_overrides};
use crate::tools::{parse_argument_string, parse_arguments_to_value, validate_tool_definitions};

pub(crate) async fn list_anthropic_models(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    let url = format!("{}/v1/models", trim_slash(&provider.base_url));
    let request = match provider.auth_mode {
        AuthMode::ApiKey => {
            let api_key = match api_key_override {
                Some(api_key) => api_key.to_string(),
                None => api_key_for(provider)?,
            };
            client
                .get(url)
                .header("anthropic-version", "2023-06-01")
                .header("x-api-key", api_key)
        }
        AuthMode::OAuth => {
            let request = client.get(url).header("anthropic-version", "2023-06-01");
            apply_auth_with_overrides(
                client,
                provider,
                request,
                api_key_override,
                oauth_token_override,
            )
            .await?
        }
        AuthMode::None => bail!("anthropic providers require API key or OAuth authentication"),
    };
    let response = request
        .send()
        .await
        .context("failed to query anthropic models")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse anthropic models response")?;
    if !status.is_success() {
        drop(body);
        bail!(
            "anthropic model listing failed with {}: {}",
            status,
            provider_error_for_status(status)
        );
    }

    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

pub(crate) async fn run_anthropic(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply> {
    validate_tool_definitions(tools, "Anthropic")?;
    let url = format!("{}/v1/messages", trim_slash(&provider.base_url));
    let request = match provider.auth_mode {
        AuthMode::ApiKey => {
            let api_key = api_key_for(provider)?;
            client
                .post(url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
        }
        AuthMode::OAuth => {
            let request = client.post(url).header("anthropic-version", "2023-06-01");
            apply_auth(client, provider, request).await?
        }
        AuthMode::None => bail!("anthropic providers require API key or OAuth authentication"),
    };
    let request = if tools.is_empty() {
        request
    } else {
        request.header("anthropic-beta", "interleaved-thinking-2025-05-14")
    };
    let mut payload = json!({
        "model": model,
        "max_tokens": 2048,
        "messages": messages_to_anthropic(messages)?,
    });
    if let Some(system) = anthropic_system_message(messages)? {
        payload["system"] = Value::String(system);
    }
    if !tools.is_empty() {
        payload["tools"] = Value::Array(tool_definitions_to_anthropic(tools));
    }
    if let Some(thinking) = anthropic_thinking_payload(thinking_level) {
        payload["thinking"] = thinking;
        payload["max_tokens"] = Value::from(4096);
    }
    let response = request
        .json(&payload)
        .send()
        .await
        .context("failed to send anthropic request")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse anthropic response")?;
    if !status.is_success() {
        drop(body);
        bail!(
            "anthropic request failed with {}: {}",
            status,
            provider_error_for_status(status)
        );
    }

    let content_blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("anthropic response contained no content"))?;
    let output_items = anthropic_output_items(content_blocks);
    let content = content_blocks
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let tool_calls = content_blocks
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("tool_use"))
        .map(parse_anthropic_tool_call)
        .collect::<Result<Vec<_>>>()?;
    if content.is_empty() && tool_calls.is_empty() {
        bail!("anthropic response contained neither text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content,
        tool_calls,
        provider_payload_json: Some(serde_json::to_string(content_blocks)?),
        output_items,
        artifacts: Vec::new(),
        remote_content: Vec::new(),
    })
}

pub(crate) fn messages_to_anthropic(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
    messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .map(|message| match message.role {
            MessageRole::User => Ok(json!({
                "role": "user",
                "content": anthropic_user_content(message)?,
            })),
            MessageRole::Assistant => {
                ensure_no_attachments(message, "Anthropic assistant")?;
                if let Some(raw_blocks) = &message.provider_payload_json {
                    let content: Vec<Value> = serde_json::from_str(raw_blocks)
                        .context("failed to decode stored Anthropic assistant content")?;
                    return Ok(json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }
                let mut blocks = Vec::new();
                if !message.content.is_empty() {
                    blocks.push(json!({
                        "type": "text",
                        "text": message.content,
                    }));
                }
                for tool_call in &message.tool_calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": tool_call.id,
                        "name": tool_call.name,
                        "input": parse_arguments_to_value(&tool_call.arguments)?,
                    }));
                }
                Ok(json!({
                    "role": "assistant",
                    "content": blocks,
                }))
            }
            MessageRole::Tool => {
                ensure_no_attachments(message, "Anthropic tool")?;
                Ok(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                        "content": message.content,
                    }],
                }))
            }
            MessageRole::System => unreachable!(),
        })
        .collect()
}

pub(crate) fn anthropic_user_content(message: &ConversationMessage) -> Result<Value> {
    let mut content = Vec::new();
    if !message.content.is_empty() || message.attachments.is_empty() {
        content.push(json!({
            "type": "text",
            "text": message.content,
        }));
    }
    for attachment in &message.attachments {
        let image = load_image_attachment(attachment)?;
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": image.mime_type,
                "data": image.data_base64,
            }
        }));
    }
    Ok(Value::Array(content))
}

pub(crate) fn anthropic_thinking_payload(thinking_level: Option<ThinkingLevel>) -> Option<Value> {
    let thinking_level = thinking_level?;
    if matches!(thinking_level, ThinkingLevel::None) {
        return None;
    }

    Some(json!({
        "type": "enabled",
        "budget_tokens": anthropic_budget_tokens(thinking_level),
    }))
}

pub(crate) fn anthropic_budget_tokens(thinking_level: ThinkingLevel) -> u64 {
    match thinking_level {
        ThinkingLevel::None => 0,
        ThinkingLevel::Minimal => 256,
        ThinkingLevel::Low => 512,
        ThinkingLevel::Medium => 1024,
        ThinkingLevel::High => 2048,
        ThinkingLevel::XHigh => 3072,
    }
}

pub(crate) fn tool_definitions_to_anthropic(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            })
        })
        .collect()
}

pub(crate) fn anthropic_system_message(messages: &[ConversationMessage]) -> Result<Option<String>> {
    let mut collected = Vec::new();
    for message in messages
        .iter()
        .filter(|message| message.role == MessageRole::System)
    {
        ensure_no_attachments(message, "Anthropic system")?;
        if !message.content.is_empty() {
            collected.push(message.content.clone());
        }
    }

    if collected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(collected.join("\n\n")))
    }
}

pub(crate) fn anthropic_output_items(content_blocks: &[Value]) -> Vec<ProviderOutputItem> {
    let mut items = Vec::new();
    for block in content_blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    items.push(ProviderOutputItem::Message {
                        role: MessageRole::Assistant,
                        content: text.to_string(),
                    });
                }
            }
            Some("tool_use") => {
                if let Ok(call) = parse_anthropic_tool_call(block) {
                    items.push(ProviderOutputItem::FunctionCall { call });
                }
            }
            _ => {}
        }
    }
    items
}

pub(crate) fn parse_anthropic_tool_call(value: &Value) -> Result<ToolCall> {
    Ok(ToolCall {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("anthropic tool_use block missing id"))?
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("anthropic tool_use block missing name"))?
            .to_string(),
        arguments: parse_argument_string(value.get("input").unwrap_or(&Value::Null)),
    })
}
