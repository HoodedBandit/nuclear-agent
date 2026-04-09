use super::*;
use crate::attachments::{ensure_no_attachments, load_image_attachment};
use crate::chatgpt_codex_catalog::{
    merge_chatgpt_codex_model_catalog, resolve_chatgpt_codex_model_descriptor,
    ChatGptCodexModelsResponse,
};
use crate::oauth::{force_refresh_oauth_token_for_request, oauth_token_for_request};
use crate::openai_compatible::openai_reasoning_effort;
use crate::tools::{
    extract_chatgpt_codex_item_text, parse_argument_string, responses_tool_backend,
    tool_definitions_to_responses_api, validate_tool_definitions,
};

pub(crate) async fn list_chatgpt_codex_model_descriptors(
    client: &Client,
    provider: &ProviderConfig,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<ModelDescriptor>> {
    let token = codex_session_token(client, provider, oauth_token_override).await?;
    let models = load_chatgpt_codex_model_descriptors(
        client,
        provider,
        &token,
        oauth_token_override.is_none(),
    )
    .await?;
    if models.is_empty() {
        if let Some(model) = &provider.default_model {
            Ok(vec![ModelDescriptor {
                id: model.clone(),
                display_name: None,
                description: None,
                context_window: None,
                effective_context_window_percent: None,
                show_in_picker: true,
                default_reasoning_effort: None,
                supported_reasoning_levels: Vec::new(),
                supports_reasoning_summaries: false,
                default_reasoning_summary: None,
                support_verbosity: false,
                default_verbosity: None,
                supports_parallel_tool_calls: false,
                priority: None,
                capabilities: ModelToolCapabilities::default(),
            }])
        } else {
            Ok(Vec::new())
        }
    } else {
        Ok(models)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_chatgpt_codex(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    messages: &[ConversationMessage],
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
    oauth_token_override: Option<&OAuthToken>,
) -> Result<ProviderReply> {
    validate_tool_definitions(tools, "ChatGPT/Codex")?;
    let token = codex_session_token(client, provider, oauth_token_override).await?;
    let model_descriptor = resolve_chatgpt_codex_model_descriptor(model);
    let payload = chatgpt_codex_payload(
        model,
        messages,
        thinking_level,
        tools,
        model_descriptor.as_ref(),
    )?;
    let (status, body) =
        send_chatgpt_codex_response_request(client, provider, &token, &payload, session_id).await?;
    let (status, body) = if !status.is_success()
        && oauth_token_override.is_none()
        && should_retry_chatgpt_codex_auth(status, &body)
    {
        let auth_error = parse_chatgpt_codex_error(&body);
        let refreshed = force_refresh_oauth_token_for_request(client, provider)
            .await
            .with_context(|| {
                format!(
                    "ChatGPT/Codex session refresh failed after backend auth rejection: {auth_error}"
                )
            })?;
        send_chatgpt_codex_response_request(client, provider, &refreshed, &payload, session_id)
            .await?
    } else {
        (status, body)
    };
    if !status.is_success() {
        bail!(
            "ChatGPT/Codex request failed: {}",
            parse_chatgpt_codex_error(&body)
        );
    }
    let streamed = parse_chatgpt_codex_stream(&body)?;
    if streamed.content.is_empty() && streamed.tool_calls.is_empty() {
        bail!("ChatGPT/Codex response contained neither text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content: streamed.content,
        tool_calls: streamed.tool_calls,
        provider_payload_json: if streamed.output_items.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&streamed.output_items)?)
        },
        output_items: streamed
            .output_items
            .iter()
            .filter_map(parse_chatgpt_codex_output_item)
            .collect(),
        artifacts: Vec::new(),
        remote_content: Vec::new(),
    })
}

pub(crate) fn messages_to_chatgpt_codex_input(
    messages: &[ConversationMessage],
) -> Result<Vec<Value>> {
    let mut input = Vec::new();
    for message in messages {
        match message.role {
            MessageRole::System => {
                input.push(chatgpt_codex_message_item("developer", message)?);
            }
            MessageRole::User => {
                input.push(chatgpt_codex_message_item("user", message)?);
            }
            MessageRole::Assistant => {
                ensure_no_attachments(message, "ChatGPT/Codex assistant")?;
                if let Some(raw_items) = &message.provider_payload_json {
                    let items: Vec<Value> = serde_json::from_str(raw_items)
                        .context("failed to decode stored ChatGPT/Codex assistant items")?;
                    input.extend(items);
                    continue;
                }
                if !message.content.is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": message.content,
                        }],
                    }));
                }
                for tool_call in &message.tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "name": tool_call.name,
                        "arguments": tool_call.arguments,
                        "call_id": tool_call.id,
                    }));
                }
            }
            MessageRole::Tool => {
                ensure_no_attachments(message, "ChatGPT/Codex tool")?;
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": message.tool_call_id.clone().unwrap_or_default(),
                    "output": message.content,
                }));
            }
        }
    }
    Ok(input)
}

pub(crate) fn chatgpt_codex_message_item(
    role: &str,
    message: &ConversationMessage,
) -> Result<Value> {
    Ok(json!({
        "type": "message",
        "role": role,
        "content": chatgpt_codex_message_content(message)?,
    }))
}

pub(crate) fn chatgpt_codex_message_content(message: &ConversationMessage) -> Result<Vec<Value>> {
    let mut content = Vec::new();
    if !message.content.is_empty() || message.attachments.is_empty() {
        content.push(json!({
            "type": "input_text",
            "text": message.content,
        }));
    }
    for attachment in &message.attachments {
        let image = load_image_attachment(attachment)?;
        content.push(json!({
            "type": "input_image",
            "image_url": format!("data:{};base64,{}", image.mime_type, image.data_base64),
        }));
    }
    Ok(content)
}

pub(crate) fn chatgpt_codex_payload(
    model: &str,
    messages: &[ConversationMessage],
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
    model_descriptor: Option<&ModelDescriptor>,
) -> Result<Value> {
    let allow_parallel_tool_calls = model_descriptor
        .map(|descriptor| descriptor.supports_parallel_tool_calls)
        .unwrap_or(true);
    let mut payload = json!({
        "model": model,
        "instructions": "",
        "input": messages_to_chatgpt_codex_input(messages)?,
        "tools": tool_definitions_to_responses_api(tools)?,
        "tool_choice": "auto",
        "parallel_tool_calls": !tools.is_empty() && allow_parallel_tool_calls,
        "store": false,
        "stream": true,
        "include": [],
    });
    if let Some(reasoning) = chatgpt_codex_reasoning_payload(thinking_level, model_descriptor) {
        payload["reasoning"] = reasoning;
        payload["include"] = json!(["reasoning.encrypted_content"]);
    }
    if let Some(text) = chatgpt_codex_text_payload(model_descriptor) {
        payload["text"] = text;
    }
    Ok(payload)
}

pub(crate) fn chatgpt_codex_reasoning_payload(
    thinking_level: Option<ThinkingLevel>,
    model_descriptor: Option<&ModelDescriptor>,
) -> Option<Value> {
    let effort = thinking_level
        .and_then(openai_reasoning_effort)
        .map(ToOwned::to_owned)
        .or_else(|| {
            model_descriptor.and_then(|descriptor| descriptor.default_reasoning_effort.clone())
        });
    let summary = model_descriptor
        .and_then(|descriptor| {
            descriptor
                .supports_reasoning_summaries
                .then_some(descriptor.default_reasoning_summary.as_deref())
        })
        .flatten()
        .and_then(normalize_chatgpt_codex_reasoning_summary_str)
        .map(ToOwned::to_owned);

    if effort.is_none() && summary.is_none() {
        return None;
    }

    let mut reasoning = serde_json::Map::new();
    if let Some(effort) = effort {
        reasoning.insert("effort".to_string(), Value::String(effort));
    }
    if let Some(summary) = summary {
        reasoning.insert("summary".to_string(), Value::String(summary));
    }
    Some(Value::Object(reasoning))
}

pub(crate) fn chatgpt_codex_text_payload(
    model_descriptor: Option<&ModelDescriptor>,
) -> Option<Value> {
    let descriptor = model_descriptor?;
    if !descriptor.support_verbosity {
        return None;
    }
    let verbosity = descriptor
        .default_verbosity
        .as_deref()
        .and_then(normalize_chatgpt_codex_verbosity_str)?;
    Some(json!({
        "verbosity": verbosity,
    }))
}

pub(crate) fn normalize_chatgpt_codex_reasoning_summary(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_reasoning_summary_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn normalize_chatgpt_codex_reasoning_summary_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some("auto"),
        "concise" => Some("concise"),
        "detailed" => Some("detailed"),
        "none" | "" => None,
        _ => None,
    }
}

pub(crate) fn normalize_chatgpt_codex_verbosity(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_verbosity_str)
        .map(ToOwned::to_owned)
}

pub(crate) fn normalize_chatgpt_codex_verbosity_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        _ => None,
    }
}

pub(crate) async fn load_chatgpt_codex_model_descriptors(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
    allow_refresh: bool,
) -> Result<Vec<ModelDescriptor>> {
    let (status, raw_body) = send_chatgpt_codex_models_request(client, provider, token).await?;
    let (status, raw_body, subscription_type) = if !status.is_success()
        && allow_refresh
        && should_retry_chatgpt_codex_auth(status, &raw_body)
    {
        let auth_error = parse_chatgpt_codex_error(&raw_body);
        let refreshed = force_refresh_oauth_token_for_request(client, provider)
            .await
            .with_context(|| {
                format!(
                    "ChatGPT/Codex session refresh failed after model auth rejection: {auth_error}"
                )
            })?;
        let (status, raw_body) =
            send_chatgpt_codex_models_request(client, provider, &refreshed).await?;
        (status, raw_body, refreshed.subscription_type.clone())
    } else {
        (status, raw_body, token.subscription_type.clone())
    };
    if !status.is_success() {
        bail!(
            "ChatGPT/Codex model listing failed: {}",
            parse_chatgpt_codex_error(&raw_body)
        );
    }

    let body: ChatGptCodexModelsResponse =
        serde_json::from_str(&raw_body).context("failed to parse ChatGPT/Codex models response")?;
    Ok(merge_chatgpt_codex_model_catalog(
        body.models,
        subscription_type.as_deref(),
    ))
}

pub(crate) struct ChatGptCodexStreamResponse {
    content: String,
    tool_calls: Vec<ToolCall>,
    output_items: Vec<Value>,
}

pub(crate) async fn codex_session_token(
    client: &Client,
    provider: &ProviderConfig,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<OAuthToken> {
    if provider.auth_mode != AuthMode::OAuth {
        bail!(
            "provider '{}' requires browser-managed OAuth credentials",
            provider.id
        );
    }

    Ok(match oauth_token_override {
        Some(token) => token.clone(),
        None => oauth_token_for_request(client, provider).await?,
    })
}

pub(crate) fn apply_chatgpt_codex_auth(
    request: reqwest::RequestBuilder,
    token: &OAuthToken,
    session_id: Option<&str>,
) -> reqwest::RequestBuilder {
    let request = request
        .header(header::USER_AGENT, chatgpt_codex_user_agent())
        .header("originator", CHATGPT_CODEX_ORIGINATOR)
        .header("version", env!("CARGO_PKG_VERSION"))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", token.access_token),
        );
    let request = if let Some(session_id) = session_id {
        request.header("session_id", session_id)
    } else {
        request
    };
    if let Some(account_id) = token.account_id.as_deref() {
        request.header("ChatGPT-Account-ID", account_id)
    } else {
        request
    }
}

pub(crate) async fn send_chatgpt_codex_models_request(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<(StatusCode, String)> {
    let response = apply_chatgpt_codex_auth(
        client.get(format!(
            "{}/models?client_version={}",
            trim_slash(&provider.base_url),
            env!("CARGO_PKG_VERSION")
        )),
        token,
        None,
    )
    .send()
    .await
    .context("failed to query ChatGPT/Codex models")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read ChatGPT/Codex models response")?;
    Ok((status, body))
}

pub(crate) async fn send_chatgpt_codex_response_request(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
    payload: &Value,
    session_id: Option<&str>,
) -> Result<(StatusCode, String)> {
    let response = apply_chatgpt_codex_auth(
        client
            .post(format!("{}/responses", trim_slash(&provider.base_url)))
            .header(header::ACCEPT, "text/event-stream")
            .json(payload),
        token,
        session_id,
    )
    .send()
    .await
    .context("failed to send ChatGPT/Codex request")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read ChatGPT/Codex response stream")?;
    Ok((status, body))
}

pub(crate) fn chatgpt_codex_user_agent() -> String {
    format!("{CHATGPT_CODEX_ORIGINATOR}/{}", env!("CARGO_PKG_VERSION"))
}

pub(crate) fn should_retry_chatgpt_codex_auth(status: StatusCode, body: &str) -> bool {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return true;
    }
    let detail = parse_chatgpt_codex_error(body).to_ascii_lowercase();
    detail.contains("authentication token")
        || detail.contains("sign in again")
        || detail.contains("unauthorized")
}

pub(crate) fn parse_chatgpt_codex_stream(body: &str) -> Result<ChatGptCodexStreamResponse> {
    let mut content = String::new();
    let mut saw_text_delta = false;
    let mut tool_calls = Vec::new();
    let mut output_items = Vec::new();

    for (kind, data) in parse_sse_events(body) {
        match kind.as_str() {
            "response.output_text.delta" => {
                let payload = parse_sse_payload(&kind, &data)?;
                if let Some(delta) = payload.get("delta").and_then(Value::as_str) {
                    content.push_str(delta);
                    saw_text_delta = true;
                }
            }
            "response.output_item.done" => {
                let payload = parse_sse_payload(&kind, &data)?;
                let Some(item) = payload.get("item").cloned() else {
                    continue;
                };
                if let Some(tool_call) = parse_chatgpt_codex_tool_call(&item)? {
                    tool_calls.push(tool_call);
                } else if !saw_text_delta {
                    content.push_str(&extract_chatgpt_codex_item_text(&item));
                }
                output_items.push(item);
            }
            "response.failed" => {
                let payload = parse_sse_payload(&kind, &data)?;
                bail!(
                    "ChatGPT/Codex request failed: {}",
                    extract_chatgpt_codex_stream_error(&payload)
                );
            }
            _ => {}
        }
    }

    if content.is_empty() && !saw_text_delta {
        content = output_items
            .iter()
            .map(extract_chatgpt_codex_item_text)
            .collect::<Vec<_>>()
            .join("");
    }

    Ok(ChatGptCodexStreamResponse {
        content,
        tool_calls,
        output_items,
    })
}

pub(crate) fn parse_sse_events(body: &str) -> Vec<(String, String)> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|block| {
            let mut kind = None;
            let mut data_lines = Vec::new();
            for line in block.lines() {
                if let Some(rest) = line.strip_prefix("event:") {
                    kind = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start().to_string());
                }
            }
            kind.map(|kind| (kind, data_lines.join("\n")))
        })
        .collect()
}

pub(crate) fn parse_sse_payload(kind: &str, data: &str) -> Result<Value> {
    if data.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(data)
        .with_context(|| format!("failed to parse ChatGPT/Codex SSE payload for {kind}"))
}

pub(crate) fn parse_chatgpt_codex_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(body) => extract_chatgpt_codex_stream_error(&body),
        Err(_) => trimmed.to_string(),
    }
}

pub(crate) fn extract_chatgpt_codex_stream_error(body: &Value) -> String {
    if let Some(text) = body.get("detail").and_then(Value::as_str) {
        return text.to_string();
    }
    body.get("response")
        .and_then(|response| response.get("error"))
        .and_then(|error| error.get("message").or_else(|| error.get("code")))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            body.get("error")
                .and_then(|error| error.get("message").or_else(|| error.get("code")))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body.to_string())
}

pub(crate) fn parse_chatgpt_codex_tool_call(value: &Value) -> Result<Option<ToolCall>> {
    if value.get("type").and_then(Value::as_str) != Some("function_call") {
        return Ok(None);
    }

    Ok(Some(ToolCall {
        id: value
            .get("call_id")
            .or_else(|| value.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("ChatGPT/Codex tool call missing call_id"))?
            .to_string(),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("ChatGPT/Codex tool call missing name"))?
            .to_string(),
        arguments: parse_argument_string(value.get("arguments").unwrap_or(&Value::Null)),
    }))
}

pub(crate) fn parse_chatgpt_codex_output_item(value: &Value) -> Option<ProviderOutputItem> {
    let item_type = value.get("type").and_then(Value::as_str)?;
    match item_type {
        "message" => {
            let role = match value.get("role").and_then(Value::as_str) {
                Some("assistant") => MessageRole::Assistant,
                Some("user") => MessageRole::User,
                Some("developer") | Some("system") => MessageRole::System,
                Some("tool") => MessageRole::Tool,
                _ => return None,
            };
            Some(ProviderOutputItem::Message {
                role,
                content: extract_chatgpt_codex_item_text(value),
            })
        }
        "function_call" => parse_chatgpt_codex_tool_call(value)
            .ok()
            .flatten()
            .map(|call| ProviderOutputItem::FunctionCall { call }),
        "function_call_output" => Some(ProviderOutputItem::ToolResult {
            call_id: value
                .get("call_id")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("function")
                .to_string(),
            backend: ToolBackend::LocalFunction,
            hosted_kind: None,
            status: value
                .get("status")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            content: value
                .get("output")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        }),
        other => {
            let (backend, hosted_kind) = responses_tool_backend(other);
            let call_id = value
                .get("call_id")
                .or_else(|| value.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if call_id.is_empty() {
                return None;
            }
            Some(ProviderOutputItem::ToolCall {
                call_id,
                name: other.to_string(),
                backend,
                hosted_kind,
                status: value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                arguments_json: Some(value.to_string()),
            })
        }
    }
}
