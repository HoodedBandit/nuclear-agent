use super::*;
use crate::attachments::{ensure_no_attachments, load_image_attachment};
use crate::oauth::{apply_auth, apply_auth_with_overrides};
use crate::tools::{parse_argument_string, tool_definitions_to_openai, validate_tool_definitions};

pub async fn compute_embedding(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    text: &str,
    dimensions: Option<u32>,
) -> Result<Vec<f32>> {
    let url = format!("{}/embeddings", trim_slash(&provider.base_url));
    let mut body = json!({
        "input": text,
        "model": model,
    });
    if let Some(dims) = dimensions {
        if dims > 0 {
            body["dimensions"] = json!(dims);
        }
    }
    let request = client.post(&url).json(&body);
    let request = apply_auth(client, provider, request).await?;
    let response = request.send().await.context("embedding request failed")?;
    let status = response.status();
    let response_body: Value = response
        .json()
        .await
        .context("failed to parse embedding response")?;
    if !status.is_success() {
        let error = redact_sensitive_text(&extract_error(&response_body));
        bail!("embedding request returned {}: {}", status, error);
    }
    let embedding = response_body
        .pointer("/data/0/embedding")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("embedding response missing data[0].embedding"))?;
    embedding
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| anyhow!("embedding vector contains non-numeric value"))
        })
        .collect()
}

pub(crate) async fn list_openai_models(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    let url = format!("{}/models", trim_slash(&provider.base_url));
    let request = apply_auth_with_overrides(
        client,
        provider,
        client.get(url),
        api_key_override,
        oauth_token_override,
    )
    .await?;
    let response = request.send().await.context("failed to query models")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse models response")?;
    if !status.is_success() {
        if supports_local_model_listing_fallback(provider, status) {
            return Ok(provider.default_model.clone().into_iter().collect());
        }
        let error = redact_sensitive_text(&extract_error(&body));
        bail!("model listing failed: {}", error);
    }

    let models = body
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if models.is_empty() {
        if let Some(model) = &provider.default_model {
            Ok(vec![model.clone()])
        } else {
            Ok(Vec::new())
        }
    } else {
        Ok(models)
    }
}

pub(crate) async fn run_openai_compatible(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply> {
    validate_tool_definitions(tools, "OpenAI-compatible")?;
    let url = format!("{}/chat/completions", trim_slash(&provider.base_url));
    let mut payload = json!({
        "model": model,
        "messages": messages_to_openai(messages)?,
        "temperature": 0.2
    });
    if let Some(reasoning_payload) = openai_reasoning_payload(provider, thinking_level) {
        merge_json_object(&mut payload, reasoning_payload)?;
    }
    if !tools.is_empty() {
        payload["tools"] = Value::Array(tool_definitions_to_openai(tools));
        payload["tool_choice"] = Value::String("auto".to_string());
    }
    let request = client.post(url).json(&payload);
    let request = apply_auth(client, provider, request).await?;
    let response = request.send().await.context("failed to send request")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse completion response")?;
    if !status.is_success() {
        let error = redact_sensitive_text(&extract_error(&body));
        bail!("completion failed: {}", error);
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
        provider_payload_json: None,
        output_items,
        artifacts: Vec::new(),
        remote_content: Vec::new(),
    })
}

pub(crate) fn messages_to_openai(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
    messages
        .iter()
        .map(|message| match message.role {
            MessageRole::System | MessageRole::User => Ok(json!({
                "role": role_name(&message.role),
                "content": openai_message_content(message)?,
            })),
            MessageRole::Assistant => {
                ensure_no_attachments(message, "OpenAI-compatible assistant")?;
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

pub(crate) fn openai_message_content(message: &ConversationMessage) -> Result<Value> {
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

pub(crate) fn openai_reasoning_payload(
    provider: &ProviderConfig,
    thinking_level: Option<ThinkingLevel>,
) -> Option<Value> {
    let thinking_level = thinking_level?;
    if is_openrouter_provider(provider) {
        return openrouter_reasoning_payload(thinking_level);
    }

    openai_reasoning_effort(thinking_level).map(|effort| json!({ "reasoning_effort": effort }))
}

pub(crate) fn is_openrouter_provider(provider: &ProviderConfig) -> bool {
    provider.id.eq_ignore_ascii_case("openrouter") || provider.base_url.contains("openrouter.ai")
}

pub(crate) fn openrouter_reasoning_payload(thinking_level: ThinkingLevel) -> Option<Value> {
    openai_reasoning_effort(thinking_level).map(|effort| {
        json!({
            "reasoning": {
                "effort": effort
            }
        })
    })
}

pub(crate) fn openai_reasoning_effort(thinking_level: ThinkingLevel) -> Option<&'static str> {
    match thinking_level {
        ThinkingLevel::None => None,
        ThinkingLevel::Minimal | ThinkingLevel::Low => Some("low"),
        ThinkingLevel::Medium => Some("medium"),
        ThinkingLevel::High | ThinkingLevel::XHigh => Some("high"),
    }
}

pub(crate) fn merge_json_object(target: &mut Value, updates: Value) -> Result<()> {
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

pub(crate) fn parse_openai_tool_calls(message: &Value) -> Result<Vec<ToolCall>> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().map(parse_openai_tool_call).collect())
        .unwrap_or_else(|| Ok(Vec::new()))
}

pub(crate) fn parse_openai_tool_call(value: &Value) -> Result<ToolCall> {
    Ok(ToolCall {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("tool call missing id"))?
            .to_string(),
        name: value
            .get("function")
            .and_then(|function| function.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("tool call missing function name"))?
            .to_string(),
        arguments: parse_argument_string(
            value
                .get("function")
                .and_then(|function| function.get("arguments"))
                .unwrap_or(&Value::Null),
        ),
    })
}

pub(crate) fn openai_output_items(
    content: &str,
    tool_calls: &[ToolCall],
) -> Vec<ProviderOutputItem> {
    let mut items = Vec::new();
    if !content.is_empty() {
        items.push(ProviderOutputItem::Message {
            role: MessageRole::Assistant,
            content: content.to_string(),
        });
    }
    items.extend(
        tool_calls
            .iter()
            .cloned()
            .map(|call| ProviderOutputItem::FunctionCall { call }),
    );
    items
}
