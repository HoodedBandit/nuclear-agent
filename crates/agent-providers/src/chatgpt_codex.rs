use agent_core::{
    AuthMode, ConversationMessage, MessageRole, ModelDescriptor, OAuthToken, ProviderConfig,
    ProviderReply, ThinkingLevel, ToolCall, ToolDefinition,
};
use anyhow::{bail, Context, Result};
use reqwest::{header, Client, StatusCode};
use serde_json::{json, Value};

use super::attachments::load_image_attachment;
use super::chatgpt_codex_models as codex_models;
use super::common::{ensure_no_attachments, openai_reasoning_effort, trim_slash};
use super::oauth::{force_refresh_oauth_token_for_request, oauth_token_for_request};
use super::tooling::{
    parse_chatgpt_codex_output_item, parse_chatgpt_codex_tool_call,
    tool_definitions_to_responses_api, validate_tool_definitions,
};

const CHATGPT_CODEX_ORIGINATOR: &str = "codex_cli_rs";

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_chatgpt_codex(
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
    let model_descriptor = codex_models::resolve_chatgpt_codex_model_descriptor(model);
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

pub(super) fn messages_to_chatgpt_codex_input(
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

pub(super) fn chatgpt_codex_message_item(
    role: &str,
    message: &ConversationMessage,
) -> Result<Value> {
    Ok(json!({
        "type": "message",
        "role": role,
        "content": chatgpt_codex_message_content(message)?,
    }))
}

pub(super) fn chatgpt_codex_message_content(message: &ConversationMessage) -> Result<Vec<Value>> {
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

pub(super) fn chatgpt_codex_payload(
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

pub(super) fn chatgpt_codex_reasoning_payload(
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
        .and_then(codex_models::normalize_chatgpt_codex_reasoning_summary_str)
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

pub(super) fn chatgpt_codex_text_payload(
    model_descriptor: Option<&ModelDescriptor>,
) -> Option<Value> {
    let descriptor = model_descriptor?;
    if !descriptor.support_verbosity {
        return None;
    }
    let verbosity = descriptor
        .default_verbosity
        .as_deref()
        .and_then(codex_models::normalize_chatgpt_codex_verbosity_str)?;
    Some(json!({
        "verbosity": verbosity,
    }))
}

pub(super) async fn load_chatgpt_codex_model_descriptors(
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

    let body: codex_models::ChatGptCodexModelsResponse =
        serde_json::from_str(&raw_body).context("failed to parse ChatGPT/Codex models response")?;
    Ok(codex_models::merge_chatgpt_codex_model_catalog(
        body.models,
        subscription_type.as_deref(),
    ))
}

pub(super) struct ChatGptCodexStreamResponse {
    pub(super) content: String,
    pub(super) tool_calls: Vec<ToolCall>,
    pub(super) output_items: Vec<Value>,
}

pub(super) async fn codex_session_token(
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

fn apply_chatgpt_codex_auth(
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

async fn send_chatgpt_codex_models_request(
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

async fn send_chatgpt_codex_response_request(
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

pub(super) fn chatgpt_codex_user_agent() -> String {
    format!("{CHATGPT_CODEX_ORIGINATOR}/{}", env!("CARGO_PKG_VERSION"))
}

pub(super) fn should_retry_chatgpt_codex_auth(status: StatusCode, body: &str) -> bool {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return true;
    }
    let detail = parse_chatgpt_codex_error(body).to_ascii_lowercase();
    detail.contains("authentication token")
        || detail.contains("sign in again")
        || detail.contains("unauthorized")
}

pub(super) fn parse_chatgpt_codex_stream(body: &str) -> Result<ChatGptCodexStreamResponse> {
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

pub(super) fn parse_sse_events(body: &str) -> Vec<(String, String)> {
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

pub(super) fn parse_sse_payload(kind: &str, data: &str) -> Result<Value> {
    if data.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(data)
        .with_context(|| format!("failed to parse ChatGPT/Codex SSE payload for {kind}"))
}

pub(super) fn parse_chatgpt_codex_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(body) => extract_chatgpt_codex_stream_error(&body),
        Err(_) => trimmed.to_string(),
    }
}

pub(super) fn extract_chatgpt_codex_stream_error(body: &Value) -> String {
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

fn extract_chatgpt_codex_item_text(value: &Value) -> String {
    if value.get("type").and_then(Value::as_str) != Some("message")
        || value.get("role").and_then(Value::as_str) != Some("assistant")
    {
        return String::new();
    }

    value
        .get("content")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| match entry.get("type").and_then(Value::as_str) {
                    Some("output_text") | Some("input_text") | Some("text") => {
                        entry.get("text").and_then(Value::as_str)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}
