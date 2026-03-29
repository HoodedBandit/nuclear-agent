use agent_core::{
    AttachmentKind, AuthMode, ConversationMessage, InputAttachment, MessageRole, OAuthConfig,
    OAuthToken, ProviderConfig, ProviderHealth, ProviderKind, ProviderReply, ThinkingLevel,
    ToolCall, ToolDefinition, KEYCHAIN_SERVICE,
};
use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::OnceLock};
use tracing::warn;
use url::Url;

const OAUTH_REFRESH_SKEW_SECONDS: i64 = 60;
const OPENAI_BROWSER_AUTH_ISSUER: &str = "https://auth.openai.com";
const CHATGPT_CODEX_ORIGINATOR: &str = "codex_cli_rs";
const CHATGPT_CODEX_BUNDLED_MODELS_JSON: &str =
    include_str!("../../../codex-main/codex-rs/core/models.json");

mod keyring_store;
use keyring_store::{
    api_key_for, is_openai_browser_oauth, oauth_refresh_lock_for, store_oauth_token_for_account,
    uses_openai_api_key_exchange,
};
pub use keyring_store::{
    delete_secret, keychain_account, keyring_available, load_api_key, load_oauth_token,
    store_api_key, store_oauth_token,
};
#[cfg(test)]
use keyring_store::{
    deserialize_oauth_token_secret, deserialize_secret_storage, secret_storage_units,
    serialize_oauth_token_secret, serialize_secret_storage, split_secret_chunks,
    SerializedOAuthTokenSecret, SerializedSecret, KEYCHAIN_SECRET_SAFE_UTF16_UNITS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningLevelDescriptor {
    pub effort: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub context_window: Option<i64>,
    pub effective_context_window_percent: Option<i64>,
    pub show_in_picker: bool,
    pub default_reasoning_effort: Option<String>,
    pub supported_reasoning_levels: Vec<ReasoningLevelDescriptor>,
    pub supports_reasoning_summaries: bool,
    pub default_reasoning_summary: Option<String>,
    pub support_verbosity: bool,
    pub default_verbosity: Option<String>,
    pub supports_parallel_tool_calls: bool,
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatGptCodexModelsResponse {
    #[serde(default)]
    models: Vec<ChatGptCodexModelRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatGptCodexModelRecord {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default_reasoning_level: Option<String>,
    #[serde(default)]
    supported_reasoning_levels: Vec<ChatGptCodexReasoningLevelRecord>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    supports_reasoning_summaries: Option<bool>,
    #[serde(default)]
    default_reasoning_summary: Option<String>,
    #[serde(default)]
    support_verbosity: Option<bool>,
    #[serde(default)]
    default_verbosity: Option<String>,
    #[serde(default)]
    supports_parallel_tool_calls: Option<bool>,
    #[serde(default)]
    context_window: Option<i64>,
    #[serde(default)]
    effective_context_window_percent: Option<i64>,
    #[serde(default)]
    available_in_plans: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ChatGptCodexReasoningLevelRecord {
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

pub fn build_oauth_authorization_url(
    provider: &ProviderConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String> {
    let oauth = oauth_config(provider)?;
    let mut url =
        Url::parse(&oauth.authorization_url).context("failed to parse OAuth authorization URL")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &oauth.client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        if !oauth.scopes.is_empty() {
            query.append_pair("scope", &oauth.scopes.join(" "));
        }
        for extra in &oauth.extra_authorize_params {
            query.append_pair(&extra.key, &extra.value);
        }
    }
    Ok(url.into())
}

pub async fn exchange_oauth_code(
    client: &Client,
    provider: &ProviderConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    let oauth = oauth_config(provider)?;
    let form = base_token_form(oauth)
        .into_iter()
        .chain([
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("code_verifier".to_string(), code_verifier.to_string()),
        ])
        .collect::<Vec<_>>();

    let response = client
        .post(&oauth.token_url)
        .form(&form)
        .send()
        .await
        .context("failed to exchange OAuth authorization code")?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .context("failed to read OAuth token response")?;
    if !status.is_success() {
        bail!(
            "OAuth token exchange failed: {}",
            parse_token_endpoint_error(&raw)
        );
    }
    let body: Value = serde_json::from_str(&raw).context("failed to parse OAuth token response")?;

    Ok(finalize_oauth_token(
        provider,
        parse_oauth_token(oauth, &body)?,
        None,
    ))
}

pub async fn health_check(client: &Client, provider: &ProviderConfig) -> ProviderHealth {
    match list_models(client, provider).await {
        Ok(models) => match validate_default_model(provider, &models) {
            Ok(()) => ProviderHealth {
                id: provider.id.clone(),
                ok: true,
                detail: format!("{} model(s) reachable", models.len()),
            },
            Err(error) => ProviderHealth {
                id: provider.id.clone(),
                ok: false,
                detail: error.to_string(),
            },
        },
        Err(error) => ProviderHealth {
            id: provider.id.clone(),
            ok: false,
            detail: error.to_string(),
        },
    }
}

pub async fn list_models(client: &Client, provider: &ProviderConfig) -> Result<Vec<String>> {
    list_models_with_overrides(client, provider, None, None).await
}

pub async fn list_model_descriptors(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<Vec<ModelDescriptor>> {
    list_model_descriptors_with_overrides(client, provider, None, None).await
}

pub async fn list_model_descriptors_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<ModelDescriptor>> {
    match provider.kind {
        ProviderKind::Ollama => Ok(list_ollama_models(client, provider)
            .await?
            .into_iter()
            .map(|id| ModelDescriptor {
                id,
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
            })
            .collect()),
        ProviderKind::Anthropic => {
            Ok(
                list_anthropic_models(client, provider, api_key_override, oauth_token_override)
                    .await?
                    .into_iter()
                    .map(|id| ModelDescriptor {
                        id,
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
                    })
                    .collect(),
            )
        }
        ProviderKind::ChatGptCodex => {
            list_chatgpt_codex_model_descriptors(client, provider, oauth_token_override).await
        }
        ProviderKind::OpenAiCompatible => {
            Ok(
                list_openai_models(client, provider, api_key_override, oauth_token_override)
                    .await?
                    .into_iter()
                    .map(|id| ModelDescriptor {
                        id,
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
                    })
                    .collect(),
            )
        }
    }
}

pub async fn list_models_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    Ok(list_model_descriptors_with_overrides(
        client,
        provider,
        api_key_override,
        oauth_token_override,
    )
    .await?
    .into_iter()
    .map(|model| model.id)
    .collect())
}

pub async fn run_prompt(
    client: &Client,
    provider: &ProviderConfig,
    messages: &[ConversationMessage],
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[ToolDefinition],
) -> Result<ProviderReply> {
    let model = requested_model
        .map(ToOwned::to_owned)
        .or_else(|| provider.default_model.clone())
        .ok_or_else(|| anyhow!("provider '{}' has no default model configured", provider.id))?;

    match provider.kind {
        ProviderKind::OpenAiCompatible => {
            run_openai_compatible(client, provider, &model, messages, thinking_level, tools).await
        }
        ProviderKind::ChatGptCodex => {
            run_chatgpt_codex(
                client,
                provider,
                &model,
                messages,
                session_id,
                thinking_level,
                tools,
                None,
            )
            .await
        }
        ProviderKind::Anthropic => {
            run_anthropic(client, provider, &model, messages, thinking_level, tools).await
        }
        ProviderKind::Ollama => {
            run_ollama(client, provider, &model, messages, thinking_level, tools).await
        }
    }
}

/// Compute an embedding vector for the given text using an OpenAI-compatible
/// embeddings endpoint (`/v1/embeddings`). Returns the raw `f32` vector.
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
        bail!(
            "embedding request returned {}: {}",
            status,
            extract_error(&response_body)
        );
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

fn validate_default_model(provider: &ProviderConfig, models: &[String]) -> Result<()> {
    let Some(default_model) = provider.default_model.as_deref() else {
        return Ok(());
    };

    if models.is_empty() || models.iter().any(|model| model == default_model) {
        return Ok(());
    }

    let discovered = models
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if models.len() > 5 { ", ..." } else { "" };
    bail!(
        "default model '{}' not available; discovered {} model(s): {}{}",
        default_model,
        models.len(),
        discovered,
        suffix
    )
}

async fn list_openai_models(
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
        bail!("model listing failed: {}", extract_error(&body));
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

async fn list_anthropic_models(
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
        bail!("anthropic model listing failed: {}", extract_error(&body));
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

async fn list_ollama_models(client: &Client, provider: &ProviderConfig) -> Result<Vec<String>> {
    let url = format!("{}/api/tags", trim_slash(&provider.base_url));
    let response = client
        .get(url)
        .send()
        .await
        .context("failed to query Ollama")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse Ollama models response")?;
    if !status.is_success() {
        bail!("ollama listing failed: {}", extract_error(&body));
    }

    Ok(body
        .get("models")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

async fn list_chatgpt_codex_model_descriptors(
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
            }])
        } else {
            Ok(Vec::new())
        }
    } else {
        Ok(models)
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_chatgpt_codex(
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
    })
}

async fn run_openai_compatible(
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
        bail!("completion failed: {}", extract_error(&body));
    }

    let message = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| anyhow!("provider returned no assistant message"))?;
    let content = message.get("content").map(extract_text).unwrap_or_default();
    let tool_calls = parse_openai_tool_calls(message)?;
    if content.is_empty() && tool_calls.is_empty() {
        bail!("provider returned neither assistant text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content,
        tool_calls,
        provider_payload_json: None,
    })
}

async fn run_anthropic(
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
        bail!("anthropic request failed: {}", extract_error(&body));
    }

    let content_blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("anthropic response contained no content"))?;
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
    })
}

async fn run_ollama(
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
    let content = message.get("content").map(extract_text).unwrap_or_default();
    let tool_calls = parse_ollama_tool_calls(message)?;
    if content.is_empty() && tool_calls.is_empty() {
        bail!("Ollama response contained neither text nor tool calls");
    }

    Ok(ProviderReply {
        provider_id: provider.id.clone(),
        model: model.to_string(),
        content,
        tool_calls,
        provider_payload_json: None,
    })
}

fn messages_to_openai(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
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

fn messages_to_anthropic(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
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

fn messages_to_chatgpt_codex_input(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
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

fn chatgpt_codex_message_item(role: &str, message: &ConversationMessage) -> Result<Value> {
    Ok(json!({
        "type": "message",
        "role": role,
        "content": chatgpt_codex_message_content(message)?,
    }))
}

fn chatgpt_codex_message_content(message: &ConversationMessage) -> Result<Vec<Value>> {
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

fn chatgpt_codex_payload(
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
        "tools": tool_definitions_to_responses_api(tools),
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

fn chatgpt_codex_reasoning_payload(
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

fn chatgpt_codex_text_payload(model_descriptor: Option<&ModelDescriptor>) -> Option<Value> {
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

fn normalize_chatgpt_codex_reasoning_summary(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_reasoning_summary_str)
        .map(ToOwned::to_owned)
}

fn normalize_chatgpt_codex_reasoning_summary_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some("auto"),
        "concise" => Some("concise"),
        "detailed" => Some("detailed"),
        "none" | "" => None,
        _ => None,
    }
}

fn normalize_chatgpt_codex_verbosity(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_verbosity_str)
        .map(ToOwned::to_owned)
}

fn normalize_chatgpt_codex_verbosity_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        _ => None,
    }
}

async fn load_chatgpt_codex_model_descriptors(
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

fn bundled_chatgpt_codex_model_catalog() -> &'static [ChatGptCodexModelRecord] {
    static CATALOG: OnceLock<Vec<ChatGptCodexModelRecord>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        match serde_json::from_str::<ChatGptCodexModelsResponse>(CHATGPT_CODEX_BUNDLED_MODELS_JSON)
        {
            Ok(response) => response
                .models
                .into_iter()
                .filter(|model| !model.slug.trim().is_empty())
                .collect(),
            Err(error) => {
                warn!("failed to parse bundled ChatGPT/Codex models catalog: {error}");
                Vec::new()
            }
        }
    })
}

fn merge_chatgpt_codex_model_catalog(
    remote_models: Vec<ChatGptCodexModelRecord>,
    subscription_type: Option<&str>,
) -> Vec<ModelDescriptor> {
    let mut merged = bundled_chatgpt_codex_model_catalog().to_vec();
    for remote in remote_models
        .into_iter()
        .filter(|model| !model.slug.trim().is_empty())
    {
        if let Some(index) = merged
            .iter()
            .position(|existing| existing.slug == remote.slug)
        {
            let existing = merged[index].clone();
            merged[index] = merge_chatgpt_codex_model_record(existing, remote);
        } else {
            merged.push(remote);
        }
    }

    let normalized_plan = subscription_type.map(normalize_chatgpt_plan);
    let mut descriptors = merged
        .into_iter()
        .filter(|model| chatgpt_codex_model_available_for_plan(model, normalized_plan.as_deref()))
        .map(model_descriptor_from_chatgpt_codex_record)
        .collect::<Vec<_>>();
    descriptors.sort_by(|left, right| {
        left.show_in_picker
            .cmp(&right.show_in_picker)
            .reverse()
            .then_with(|| {
                left.priority
                    .unwrap_or(i64::MAX)
                    .cmp(&right.priority.unwrap_or(i64::MAX))
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    descriptors
}

fn merge_chatgpt_codex_model_record(
    mut existing: ChatGptCodexModelRecord,
    update: ChatGptCodexModelRecord,
) -> ChatGptCodexModelRecord {
    if update.display_name.is_some() {
        existing.display_name = update.display_name;
    }
    if update.description.is_some() {
        existing.description = update.description;
    }
    if update.default_reasoning_level.is_some() {
        existing.default_reasoning_level = update.default_reasoning_level;
    }
    if update.visibility.is_some() {
        existing.visibility = update.visibility;
    }
    if update.priority.is_some() {
        existing.priority = update.priority;
    }
    if update.supports_reasoning_summaries.is_some() {
        existing.supports_reasoning_summaries = update.supports_reasoning_summaries;
    }
    if update.default_reasoning_summary.is_some() {
        existing.default_reasoning_summary = update.default_reasoning_summary;
    }
    if update.support_verbosity.is_some() {
        existing.support_verbosity = update.support_verbosity;
    }
    if update.default_verbosity.is_some() {
        existing.default_verbosity = update.default_verbosity;
    }
    if update.supports_parallel_tool_calls.is_some() {
        existing.supports_parallel_tool_calls = update.supports_parallel_tool_calls;
    }
    if update.context_window.is_some() {
        existing.context_window = update.context_window;
    }
    if update.effective_context_window_percent.is_some() {
        existing.effective_context_window_percent = update.effective_context_window_percent;
    }
    if !update.available_in_plans.is_empty() {
        existing.available_in_plans = update.available_in_plans;
    }
    existing
}

fn model_descriptor_from_chatgpt_codex_record(record: ChatGptCodexModelRecord) -> ModelDescriptor {
    ModelDescriptor {
        id: record.slug,
        display_name: non_empty_option(record.display_name),
        description: non_empty_option(record.description),
        context_window: record.context_window,
        effective_context_window_percent: record.effective_context_window_percent,
        show_in_picker: !matches!(record.visibility.as_deref(), Some("hide" | "none")),
        default_reasoning_effort: non_empty_option(record.default_reasoning_level),
        supported_reasoning_levels: record
            .supported_reasoning_levels
            .into_iter()
            .filter_map(|level| {
                Some(ReasoningLevelDescriptor {
                    effort: non_empty_option(level.effort)?,
                    description: non_empty_option(level.description),
                })
            })
            .collect(),
        supports_reasoning_summaries: record.supports_reasoning_summaries.unwrap_or(false),
        default_reasoning_summary: normalize_chatgpt_codex_reasoning_summary(non_empty_option(
            record.default_reasoning_summary,
        )),
        support_verbosity: record.support_verbosity.unwrap_or(false),
        default_verbosity: normalize_chatgpt_codex_verbosity(non_empty_option(
            record.default_verbosity,
        )),
        supports_parallel_tool_calls: record.supports_parallel_tool_calls.unwrap_or(false),
        priority: record.priority,
    }
}

fn resolve_chatgpt_codex_model_descriptor(model: &str) -> Option<ModelDescriptor> {
    let bundled = merge_chatgpt_codex_model_catalog(Vec::new(), None)
        .into_iter()
        .filter(|descriptor| descriptor.show_in_picker)
        .collect::<Vec<_>>();
    find_chatgpt_codex_model_descriptor(model, &bundled)
}

fn find_chatgpt_codex_model_descriptor(
    model: &str,
    descriptors: &[ModelDescriptor],
) -> Option<ModelDescriptor> {
    find_chatgpt_codex_model_by_longest_prefix(model, descriptors)
        .or_else(|| find_chatgpt_codex_model_by_namespaced_suffix(model, descriptors))
}

fn find_chatgpt_codex_model_by_longest_prefix(
    model: &str,
    descriptors: &[ModelDescriptor],
) -> Option<ModelDescriptor> {
    let mut best: Option<ModelDescriptor> = None;
    for descriptor in descriptors {
        if !model.starts_with(&descriptor.id) {
            continue;
        }
        let is_better = best
            .as_ref()
            .map(|current| descriptor.id.len() > current.id.len())
            .unwrap_or(true);
        if is_better {
            best = Some(descriptor.clone());
        }
    }
    best
}

fn find_chatgpt_codex_model_by_namespaced_suffix(
    model: &str,
    descriptors: &[ModelDescriptor],
) -> Option<ModelDescriptor> {
    let (namespace, suffix) = model.split_once('/')?;
    if suffix.contains('/') {
        return None;
    }
    if !namespace
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    find_chatgpt_codex_model_by_longest_prefix(suffix, descriptors)
}

fn normalize_chatgpt_plan(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn chatgpt_codex_model_available_for_plan(
    model: &ChatGptCodexModelRecord,
    subscription_type: Option<&str>,
) -> bool {
    let Some(subscription_type) = subscription_type else {
        return true;
    };
    if model.available_in_plans.is_empty() {
        return true;
    }
    model.available_in_plans.iter().any(|plan| {
        let normalized = normalize_chatgpt_plan(plan);
        normalized == subscription_type
            || (normalized == "edu" && subscription_type == "education")
            || (normalized == "education" && subscription_type == "edu")
    })
}

fn non_empty_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

struct ChatGptCodexStreamResponse {
    content: String,
    tool_calls: Vec<ToolCall>,
    output_items: Vec<Value>,
}

async fn codex_session_token(
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

fn chatgpt_codex_user_agent() -> String {
    format!("{CHATGPT_CODEX_ORIGINATOR}/{}", env!("CARGO_PKG_VERSION"))
}

fn should_retry_chatgpt_codex_auth(status: StatusCode, body: &str) -> bool {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return true;
    }
    let detail = parse_chatgpt_codex_error(body).to_ascii_lowercase();
    detail.contains("authentication token")
        || detail.contains("sign in again")
        || detail.contains("unauthorized")
}

fn parse_chatgpt_codex_stream(body: &str) -> Result<ChatGptCodexStreamResponse> {
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

fn parse_sse_events(body: &str) -> Vec<(String, String)> {
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

fn parse_sse_payload(kind: &str, data: &str) -> Result<Value> {
    if data.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(data)
        .with_context(|| format!("failed to parse ChatGPT/Codex SSE payload for {kind}"))
}

fn parse_chatgpt_codex_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(body) => extract_chatgpt_codex_stream_error(&body),
        Err(_) => trimmed.to_string(),
    }
}

fn extract_chatgpt_codex_stream_error(body: &Value) -> String {
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

fn anthropic_user_content(message: &ConversationMessage) -> Result<Value> {
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

fn openai_reasoning_payload(
    provider: &ProviderConfig,
    thinking_level: Option<ThinkingLevel>,
) -> Option<Value> {
    let thinking_level = thinking_level?;
    if is_openrouter_provider(provider) {
        return openrouter_reasoning_payload(thinking_level);
    }

    openai_reasoning_effort(thinking_level).map(|effort| json!({ "reasoning_effort": effort }))
}

fn is_openrouter_provider(provider: &ProviderConfig) -> bool {
    provider.id.eq_ignore_ascii_case("openrouter") || provider.base_url.contains("openrouter.ai")
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

fn anthropic_thinking_payload(thinking_level: Option<ThinkingLevel>) -> Option<Value> {
    let thinking_level = thinking_level?;
    if matches!(thinking_level, ThinkingLevel::None) {
        return None;
    }

    Some(json!({
        "type": "enabled",
        "budget_tokens": anthropic_budget_tokens(thinking_level),
    }))
}

fn anthropic_budget_tokens(thinking_level: ThinkingLevel) -> u64 {
    match thinking_level {
        ThinkingLevel::None => 0,
        ThinkingLevel::Minimal => 256,
        ThinkingLevel::Low => 512,
        ThinkingLevel::Medium => 1024,
        ThinkingLevel::High => 2048,
        ThinkingLevel::XHigh => 3072,
    }
}

fn merge_json_object(target: &mut Value, updates: Value) -> Result<()> {
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

fn messages_to_ollama(messages: &[ConversationMessage]) -> Result<Vec<Value>> {
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

fn tool_definitions_to_openai(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
}

fn validate_tool_definitions(tools: &[ToolDefinition], provider_label: &str) -> Result<()> {
    for tool in tools {
        if tool.name.trim().is_empty() {
            bail!("{provider_label} tool definition is missing a name");
        }
        if !tool.input_schema.is_object() {
            bail!(
                "{provider_label} tool '{}' must use an object JSON schema for parameters",
                tool.name
            );
        }
    }
    Ok(())
}

fn tool_definitions_to_responses_api(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
                "strict": false,
            })
        })
        .collect()
}

fn tool_definitions_to_anthropic(tools: &[ToolDefinition]) -> Vec<Value> {
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

fn anthropic_system_message(messages: &[ConversationMessage]) -> Result<Option<String>> {
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

fn parse_openai_tool_calls(message: &Value) -> Result<Vec<ToolCall>> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|entries| entries.iter().map(parse_openai_tool_call).collect())
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn parse_openai_tool_call(value: &Value) -> Result<ToolCall> {
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

fn parse_chatgpt_codex_tool_call(value: &Value) -> Result<Option<ToolCall>> {
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

fn parse_anthropic_tool_call(value: &Value) -> Result<ToolCall> {
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

fn parse_ollama_tool_calls(message: &Value) -> Result<Vec<ToolCall>> {
    message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    Ok(ToolCall {
                        id: value
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| format!("ollama-tool-{}", index + 1)),
                        name: value
                            .get("function")
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                            .ok_or_else(|| anyhow!("Ollama tool call missing function name"))?
                            .to_string(),
                        arguments: parse_argument_string(
                            value
                                .get("function")
                                .and_then(|function| function.get("arguments"))
                                .unwrap_or(&Value::Null),
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}

fn parse_argument_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "{}".to_string(),
        other => other.to_string(),
    }
}

fn parse_arguments_to_value(arguments: &str) -> Result<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed)
        .with_context(|| format!("failed to parse tool arguments as JSON: {trimmed}"))
}

fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn string_or_null(value: &str) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value.to_string())
    }
}

struct LoadedImageAttachment {
    mime_type: &'static str,
    data_base64: String,
}

fn load_image_attachment(attachment: &InputAttachment) -> Result<LoadedImageAttachment> {
    match attachment.kind {
        AttachmentKind::Image => load_image_attachment_from_path(&attachment.path),
    }
}

fn load_image_attachment_from_path(path: &Path) -> Result<LoadedImageAttachment> {
    let mime_type = infer_image_mime_type(path)?;
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read image attachment from {}", path.display()))?;
    Ok(LoadedImageAttachment {
        mime_type,
        data_base64: encode_base64(&bytes),
    })
}

fn infer_image_mime_type(path: &Path) -> Result<&'static str> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or_else(|| {
            anyhow!(
                "image attachment '{}' is missing a file extension",
                path.display()
            )
        })?;

    match extension.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "gif" => Ok("image/gif"),
        "webp" => Ok("image/webp"),
        _ => bail!(
            "image attachment '{}' uses unsupported extension '.{}'",
            path.display(),
            extension
        ),
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let block = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | (bytes[index + 2] as u32);
        encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
        encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
        encoded.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
        encoded.push(TABLE[(block & 0x3f) as usize] as char);
        index += 3;
    }

    match bytes.len() - index {
        1 => {
            let block = (bytes[index] as u32) << 16;
            encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
            encoded.push('=');
            encoded.push('=');
        }
        2 => {
            let block = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
            encoded.push(TABLE[((block >> 18) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 12) & 0x3f) as usize] as char);
            encoded.push(TABLE[((block >> 6) & 0x3f) as usize] as char);
            encoded.push('=');
        }
        _ => {}
    }

    encoded
}

fn ensure_no_attachments(message: &ConversationMessage, context: &str) -> Result<()> {
    if message.attachments.is_empty() {
        Ok(())
    } else {
        bail!("{context} messages do not support image attachments")
    }
}

async fn apply_auth(
    client: &Client,
    provider: &ProviderConfig,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder> {
    apply_auth_with_overrides(client, provider, request, None, None).await
}

async fn apply_auth_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    request: reqwest::RequestBuilder,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<reqwest::RequestBuilder> {
    match provider.auth_mode {
        AuthMode::None => Ok(request),
        AuthMode::ApiKey => {
            let api_key = match api_key_override {
                Some(api_key) => api_key.to_string(),
                None => api_key_for(provider)?,
            };
            Ok(request.header(header::AUTHORIZATION, format!("Bearer {api_key}")))
        }
        AuthMode::OAuth => {
            let token = match oauth_token_override {
                Some(token) => token.clone(),
                None => oauth_token_for_request(client, provider).await?,
            };
            if uses_openai_api_key_exchange(provider) {
                let api_key = exchange_openai_api_key(client, provider, &token).await?;
                return Ok(request.header(header::AUTHORIZATION, format!("Bearer {api_key}")));
            }
            let token_type = token.token_type.as_deref().unwrap_or("Bearer");
            Ok(request.header(
                header::AUTHORIZATION,
                format!("{token_type} {}", token.access_token),
            ))
        }
    }
}

async fn oauth_token_for_request(client: &Client, provider: &ProviderConfig) -> Result<OAuthToken> {
    let account = provider
        .keychain_account
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' is missing keychain metadata", provider.id))?;
    let lock = oauth_refresh_lock_for(account);
    let _guard = lock.lock().await;
    let token = load_oauth_token(account)?;
    let token = if token_needs_refresh(&token) {
        let refreshed = refresh_oauth_token(client, provider, &token).await?;
        store_oauth_token_for_account(account, &refreshed)?;
        refreshed
    } else {
        token
    };
    Ok(token)
}

async fn force_refresh_oauth_token_for_request(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<OAuthToken> {
    let account = provider
        .keychain_account
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' is missing keychain metadata", provider.id))?;
    let lock = oauth_refresh_lock_for(account);
    let _guard = lock.lock().await;
    let token = load_oauth_token(account)?;
    let refreshed = refresh_oauth_token(client, provider, &token).await?;
    store_oauth_token_for_account(account, &refreshed)?;
    Ok(refreshed)
}

async fn refresh_oauth_token(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<OAuthToken> {
    if is_openai_browser_oauth(provider) {
        return refresh_openai_oauth_token(client, provider, token).await;
    }

    let oauth = oauth_config(provider)?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' has no refresh token", provider.id))?;
    let form = base_token_form(oauth)
        .into_iter()
        .chain([
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("refresh_token".to_string(), refresh_token.to_string()),
        ])
        .collect::<Vec<_>>();

    let response = client
        .post(&oauth.token_url)
        .form(&form)
        .send()
        .await
        .context("failed to refresh OAuth token")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OAuth refresh response")?;
    if !status.is_success() {
        bail!("OAuth token refresh failed: {}", extract_error(&body));
    }

    let mut refreshed = parse_oauth_token(oauth, &body)?;
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = token.refresh_token.clone();
    }
    if refreshed.id_token.is_none() {
        refreshed.id_token = token.id_token.clone();
    }
    Ok(finalize_oauth_token(provider, refreshed, Some(token)))
}

fn token_needs_refresh(token: &OAuthToken) -> bool {
    token
        .expires_at
        .map(|expires_at| expires_at <= Utc::now() + Duration::seconds(OAUTH_REFRESH_SKEW_SECONDS))
        .unwrap_or(false)
}

fn parse_oauth_token(oauth: &OAuthConfig, body: &Value) -> Result<OAuthToken> {
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("OAuth response missing access_token"))?
        .to_string();
    let id_token = body
        .get("id_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let refresh_token = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let token_type = body
        .get("token_type")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let scopes = body
        .get("scope")
        .and_then(Value::as_str)
        .map(|scope| scope.split_whitespace().map(ToOwned::to_owned).collect())
        .unwrap_or_else(|| oauth.scopes.clone());
    let expires_at = parse_expires_in(body)
        .map(|seconds| Utc::now() + Duration::seconds(seconds))
        .filter(|expiry| *expiry > Utc::now());

    Ok(OAuthToken {
        access_token,
        refresh_token,
        expires_at,
        token_type,
        scopes,
        id_token,
        account_id: None,
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    })
}

fn parse_expires_in(body: &Value) -> Option<i64> {
    body.get("expires_in").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().map(|value| value as i64))
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    })
}

fn base_token_form(oauth: &OAuthConfig) -> Vec<(String, String)> {
    let mut form = vec![("client_id".to_string(), oauth.client_id.clone())];
    form.extend(
        oauth
            .extra_token_params
            .iter()
            .map(|param| (param.key.clone(), param.value.clone())),
    );
    form
}

fn oauth_config(provider: &ProviderConfig) -> Result<&OAuthConfig> {
    provider
        .oauth
        .as_ref()
        .ok_or_else(|| anyhow!("provider '{}' is missing OAuth configuration", provider.id))
}

async fn refresh_openai_oauth_token(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<OAuthToken> {
    let oauth = oauth_config(provider)?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' has no refresh token", provider.id))?;
    let response = client
        .post(&oauth.token_url)
        .json(&json!({
            "client_id": oauth.client_id,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .context("failed to refresh OpenAI browser token")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OpenAI refresh response")?;
    if !status.is_success() {
        bail!(
            "OpenAI browser token refresh failed: {}",
            extract_error(&body)
        );
    }

    let mut refreshed = parse_oauth_token(oauth, &body)?;
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = token.refresh_token.clone();
    }
    if refreshed.id_token.is_none() {
        refreshed.id_token = token.id_token.clone();
    }
    Ok(finalize_oauth_token(provider, refreshed, Some(token)))
}

async fn exchange_openai_api_key(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<String> {
    let oauth = oauth_config(provider)?;
    let id_token = token.id_token.as_deref().ok_or_else(|| {
        anyhow!(
            "provider '{}' is missing OpenAI id_token state",
            provider.id
        )
    })?;
    let token_url = Url::parse(&oauth.token_url).context("failed to parse OpenAI token URL")?;
    let issuer = format!(
        "{}://{}",
        token_url.scheme(),
        token_url
            .host_str()
            .ok_or_else(|| anyhow!("OpenAI token URL is missing a host"))?
    );
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "grant_type",
            "urn:ietf:params:oauth:grant-type:token-exchange",
        )
        .append_pair("client_id", &oauth.client_id)
        .append_pair("requested_token", "openai-api-key")
        .append_pair("subject_token", id_token)
        .append_pair(
            "subject_token_type",
            "urn:ietf:params:oauth:token-type:id_token",
        )
        .finish();
    let response = client
        .post(format!("{issuer}/oauth/token"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .context("failed to exchange OpenAI browser token for API key")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OpenAI API key exchange response")?;
    if !status.is_success() {
        let error = extract_error(&body);
        if error.contains("missing organization_id") {
            bail!(
                "OpenAI browser sign-in succeeded, but this account is missing the organization access required to mint a platform API key. Finish setup at https://platform.openai.com/ or use API-key auth instead."
            );
        }
        bail!("OpenAI API key exchange failed: {error}");
    }

    body.get("access_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("OpenAI API key exchange response missing access_token"))
}

fn finalize_oauth_token(
    provider: &ProviderConfig,
    mut token: OAuthToken,
    previous: Option<&OAuthToken>,
) -> OAuthToken {
    if is_openai_browser_oauth(provider) {
        hydrate_openai_browser_token_metadata(&mut token);
    }
    if let Some(previous) = previous {
        preserve_oauth_token_metadata(&mut token, previous);
    }
    token
}

fn preserve_oauth_token_metadata(token: &mut OAuthToken, previous: &OAuthToken) {
    if token.account_id.is_none() {
        token.account_id = previous.account_id.clone();
    }
    if token.user_id.is_none() {
        token.user_id = previous.user_id.clone();
    }
    if token.org_id.is_none() {
        token.org_id = previous.org_id.clone();
    }
    if token.project_id.is_none() {
        token.project_id = previous.project_id.clone();
    }
    if token.display_email.is_none() {
        token.display_email = previous.display_email.clone();
    }
    if token.subscription_type.is_none() {
        token.subscription_type = previous.subscription_type.clone();
    }
}

fn hydrate_openai_browser_token_metadata(token: &mut OAuthToken) {
    let Some(id_token) = token.id_token.as_deref() else {
        return;
    };
    let Some(claims) = parse_openai_browser_claims(id_token) else {
        return;
    };

    token.account_id = claims.account_id.or(token.account_id.take());
    token.user_id = claims.user_id.or(token.user_id.take());
    token.org_id = claims.org_id.or(token.org_id.take());
    token.project_id = claims.project_id.or(token.project_id.take());
    token.display_email = claims.email.or(token.display_email.take());
    token.subscription_type = claims.subscription_type.or(token.subscription_type.take());
}

#[derive(Debug)]
struct OpenAiBrowserClaims {
    account_id: Option<String>,
    user_id: Option<String>,
    org_id: Option<String>,
    project_id: Option<String>,
    email: Option<String>,
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiIdClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/profile", default)]
    profile: Option<OpenAiProfileClaims>,
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<OpenAiAuthClaims>,
}

#[derive(Debug, Deserialize)]
struct OpenAiProfileClaims {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiAuthClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
    #[serde(default)]
    org_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

fn parse_openai_browser_claims(jwt: &str) -> Option<OpenAiBrowserClaims> {
    let payload = jwt.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: OpenAiIdClaims = serde_json::from_slice(&decoded).ok()?;
    let auth = claims.auth;
    let profile_email = claims.profile.and_then(|profile| profile.email);

    Some(OpenAiBrowserClaims {
        account_id: auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_account_id.clone()),
        user_id: auth.as_ref().and_then(|auth| {
            auth.chatgpt_user_id
                .clone()
                .or_else(|| auth.user_id.clone())
        }),
        org_id: auth
            .as_ref()
            .and_then(|auth| auth.organization_id.clone().or_else(|| auth.org_id.clone())),
        project_id: auth.as_ref().and_then(|auth| auth.project_id.clone()),
        email: claims.email.or(profile_email),
        subscription_type: auth.and_then(|auth| auth.chatgpt_plan_type),
    })
}

fn trim_slash(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn supports_local_model_listing_fallback(provider: &ProviderConfig, status: StatusCode) -> bool {
    provider.local
        && provider.default_model.is_some()
        && matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED
        )
}

fn extract_error(body: &Value) -> String {
    if let Some(text) = body
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return text.to_string();
    }

    if let Some(text) = body.get("error_description").and_then(Value::as_str) {
        return text.to_string();
    }

    body.to_string()
}

fn parse_token_endpoint_error(body: &str) -> String {
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

fn extract_text(value: &Value) -> String {
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

    warn!("unrecognized model response content: {}", value);
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        AttachmentKind, ConversationMessage, InputAttachment, KeyValuePair, MessageRole,
        OAuthConfig, ToolDefinition,
    };
    use std::{
        collections::HashMap,
        env, fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        sync::mpsc::{self, Receiver},
        thread,
        time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn builds_oauth_authorization_url() {
        let provider = ProviderConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://example.com/v1".to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: Some("model".to_string()),
            keychain_account: None,
            oauth: Some(OAuthConfig {
                client_id: "client".to_string(),
                authorization_url: "https://auth.example.com/authorize".to_string(),
                token_url: "https://auth.example.com/token".to_string(),
                scopes: vec!["profile".to_string(), "offline_access".to_string()],
                extra_authorize_params: vec![KeyValuePair {
                    key: "audience".to_string(),
                    value: "agent-builder".to_string(),
                }],
                extra_token_params: Vec::new(),
            }),
            local: false,
        };

        let url = build_oauth_authorization_url(
            &provider,
            "http://127.0.0.1:1234/callback",
            "state",
            "challenge",
        )
        .unwrap();

        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("audience=agent-builder"));
    }

    #[test]
    fn parses_expires_in_from_string() {
        let value = json!({
            "access_token": "abc",
            "expires_in": "90"
        });
        let oauth = OAuthConfig {
            client_id: "client".to_string(),
            authorization_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: Vec::new(),
            extra_authorize_params: Vec::new(),
            extra_token_params: Vec::new(),
        };

        let token = parse_oauth_token(&oauth, &value).unwrap();
        assert_eq!(token.access_token, "abc");
        assert!(token.expires_at.is_some());
    }

    #[test]
    fn local_openai_provider_can_fallback_when_models_endpoint_is_missing() {
        let provider = ProviderConfig {
            id: "local".to_string(),
            display_name: "Local".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "http://127.0.0.1:5001/v1".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("kobold".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        };

        assert!(supports_local_model_listing_fallback(
            &provider,
            StatusCode::NOT_FOUND
        ));
        assert!(!supports_local_model_listing_fallback(
            &provider,
            StatusCode::UNAUTHORIZED
        ));
    }

    #[test]
    fn validate_default_model_accepts_available_model() {
        let provider = ProviderConfig {
            id: "local".to_string(),
            display_name: "Local".to_string(),
            kind: ProviderKind::Ollama,
            base_url: "http://127.0.0.1:11434".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("qwen".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        };

        assert!(
            validate_default_model(&provider, &["qwen".to_string(), "llama".to_string()]).is_ok()
        );
    }

    #[test]
    fn validate_default_model_rejects_missing_model() {
        let provider = ProviderConfig {
            id: "local".to_string(),
            display_name: "Local".to_string(),
            kind: ProviderKind::Ollama,
            base_url: "http://127.0.0.1:11434".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("llama3.2".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        };

        let error = validate_default_model(
            &provider,
            &["qwen3.5:9b".to_string(), "qwen3.5:4b".to_string()],
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("default model 'llama3.2' not available"));
    }

    #[test]
    fn openai_tool_request_and_response_are_supported() {
        let (base_url, request_rx) = spawn_json_server(json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"Cargo.toml\"}"
                        }
                    }]
                }
            }]
        }));

        let provider = ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url,
            auth_mode: AuthMode::None,
            default_model: Some("gpt-test".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        };
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let reply = runtime
            .block_on(run_prompt(
                &Client::new(),
                &provider,
                &[ConversationMessage {
                    role: MessageRole::User,
                    content: "Inspect the file".to_string(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: Vec::new(),
                    provider_payload_json: None,
                    attachments: Vec::new(),
                }],
                Some("gpt-test"),
                None,
                None,
                &[ToolDefinition {
                    name: "read_file".to_string(),
                    description: "Read a file".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        },
                        "required": ["path"]
                    }),
                }],
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.contains("\"tools\""));
        assert!(request.contains("\"read_file\""));
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].name, "read_file");
    }

    #[test]
    fn openai_compatible_requests_include_reasoning_effort() {
        let (base_url, request_rx) = spawn_json_server(json!({
            "choices": [{
                "message": {
                    "content": "done"
                }
            }]
        }));

        let provider = ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url,
            auth_mode: AuthMode::None,
            default_model: Some("gpt-test".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_prompt(
                &Client::new(),
                &provider,
                &[ConversationMessage {
                    role: MessageRole::User,
                    content: "Think carefully".to_string(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: Vec::new(),
                    provider_payload_json: None,
                    attachments: Vec::new(),
                }],
                Some("gpt-test"),
                None,
                Some(ThinkingLevel::High),
                &[],
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.contains("\"reasoning_effort\":\"high\""));
    }

    #[test]
    fn openrouter_requests_use_reasoning_object() {
        let (base_url, request_rx) = spawn_json_server(json!({
            "choices": [{
                "message": {
                    "content": "done"
                }
            }]
        }));

        let provider = ProviderConfig {
            id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: base_url.replace("/token", "/api/v1"),
            auth_mode: AuthMode::None,
            default_model: Some("openai/gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        };

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime
            .block_on(run_prompt(
                &Client::new(),
                &provider,
                &[ConversationMessage {
                    role: MessageRole::User,
                    content: "Think carefully".to_string(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: Vec::new(),
                    provider_payload_json: None,
                    attachments: Vec::new(),
                }],
                Some("openai/gpt-4.1"),
                None,
                Some(ThinkingLevel::Medium),
                &[],
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.contains("\"reasoning\":{\"effort\":\"medium\"}"));
    }

    #[test]
    fn chatgpt_codex_lists_models_with_browser_session_headers() {
        let (base_url, request_rx) = spawn_response_server_at(
            "/backend-api/codex",
            "200 OK",
            "application/json",
            &json!({
                "models": [{
                    "slug": "gpt-5",
                    "display_name": "GPT-5",
                    "description": "desc",
                    "default_reasoning_level": "medium",
                    "supported_reasoning_levels": [
                        {"effort": "low", "description": "low"},
                        {"effort": "medium", "description": "medium"},
                        {"effort": "high", "description": "high"}
                    ],
                    "shell_type": "shell_command",
                    "visibility": "list",
                    "supported_in_api": true,
                    "priority": 1,
                    "availability_nux": null,
                    "upgrade": null,
                    "base_instructions": "base instructions",
                    "model_messages": null,
                    "supports_reasoning_summaries": false,
                    "default_reasoning_summary": "auto",
                    "support_verbosity": false,
                    "default_verbosity": null,
                    "apply_patch_tool_type": null,
                    "web_search_tool_type": "web_search_preview",
                    "truncation_policy": {"mode": "bytes", "limit": 10000},
                    "supports_parallel_tool_calls": true,
                    "supports_image_detail_original": false,
                    "context_window": 272000,
                    "auto_compact_token_limit": null,
                    "effective_context_window_percent": 90,
                    "experimental_supported_tools": [],
                    "input_modalities": ["text"],
                    "prefer_websockets": false
                }]
            })
            .to_string(),
        );

        let provider = ProviderConfig {
            id: "openai-browser".to_string(),
            display_name: "OpenAI Browser Session".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url,
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(openai_browser_test_oauth_config()),
            local: false,
        };
        let token = OAuthToken {
            access_token: "session-token".to_string(),
            refresh_token: None,
            expires_at: None,
            token_type: Some("Bearer".to_string()),
            scopes: Vec::new(),
            id_token: None,
            account_id: Some("acct-123".to_string()),
            user_id: None,
            org_id: None,
            project_id: None,
            display_email: None,
            subscription_type: None,
        };

        let models = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(list_models_with_overrides(
                &Client::new(),
                &provider,
                None,
                Some(&token),
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("GET /backend-api/codex/models?client_version="));
        assert!(request_lower.contains("authorization: bearer session-token"));
        assert!(request_lower.contains("chatgpt-account-id: acct-123"));
        assert!(models.iter().any(|model| model == "gpt-5"));
    }

    #[test]
    fn chatgpt_codex_model_descriptors_include_context_window_metadata() {
        let (base_url, _request_rx) = spawn_response_server_at(
            "/backend-api/codex",
            "200 OK",
            "application/json",
            &json!({
                "models": [{
                    "slug": "gpt-5",
                    "display_name": "GPT-5",
                    "context_window": 272000,
                    "effective_context_window_percent": 90
                }]
            })
            .to_string(),
        );

        let provider = ProviderConfig {
            id: "openai-browser".to_string(),
            display_name: "OpenAI Browser Session".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url,
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(openai_browser_test_oauth_config()),
            local: false,
        };
        let token = OAuthToken {
            access_token: "session-token".to_string(),
            refresh_token: None,
            expires_at: None,
            token_type: Some("Bearer".to_string()),
            scopes: Vec::new(),
            id_token: None,
            account_id: Some("acct-123".to_string()),
            user_id: None,
            org_id: None,
            project_id: None,
            display_email: None,
            subscription_type: None,
        };

        let models = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(list_model_descriptors_with_overrides(
                &Client::new(),
                &provider,
                None,
                Some(&token),
            ))
            .unwrap();

        let model = models
            .iter()
            .find(|model| model.id == "gpt-5")
            .expect("merged model list should include gpt-5");
        assert_eq!(model.display_name.as_deref(), Some("GPT-5"));
        assert_eq!(model.context_window, Some(272000));
        assert_eq!(model.effective_context_window_percent, Some(90));
    }

    #[test]
    fn chatgpt_codex_run_prompt_supports_tool_calls() {
        let body = build_sse_body(&[
            json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "function_call",
                    "name": "read_file",
                    "arguments": "{\"path\":\"Cargo.toml\"}",
                    "call_id": "call_1"
                }
            }),
            json!({
                "type": "response.completed",
                "response": { "id": "resp_1" }
            }),
        ]);
        let (base_url, request_rx) =
            spawn_response_server_at("/backend-api/codex", "200 OK", "text/event-stream", &body);

        let provider = ProviderConfig {
            id: "openai-browser".to_string(),
            display_name: "OpenAI Browser Session".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url,
            auth_mode: AuthMode::OAuth,
            default_model: Some("gpt-5".to_string()),
            keychain_account: None,
            oauth: Some(openai_browser_test_oauth_config()),
            local: false,
        };
        let token = OAuthToken {
            access_token: "session-token".to_string(),
            refresh_token: None,
            expires_at: None,
            token_type: Some("Bearer".to_string()),
            scopes: Vec::new(),
            id_token: None,
            account_id: Some("acct-123".to_string()),
            user_id: None,
            org_id: None,
            project_id: None,
            display_email: None,
            subscription_type: None,
        };

        let reply = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run_chatgpt_codex(
                &Client::new(),
                &provider,
                "gpt-5",
                &[ConversationMessage {
                    role: MessageRole::User,
                    content: "Inspect Cargo.toml".to_string(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: Vec::new(),
                    provider_payload_json: None,
                    attachments: Vec::new(),
                }],
                Some("session-123"),
                None,
                &[ToolDefinition {
                    name: "read_file".to_string(),
                    description: "Read a file".to_string(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        },
                        "required": ["path"]
                    }),
                }],
                Some(&token),
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.starts_with("POST /backend-api/codex/responses "));
        assert!(request
            .to_ascii_lowercase()
            .contains("session_id: session-123"));
        assert!(request
            .to_ascii_lowercase()
            .contains("user-agent: codex_cli_rs/"));
        assert!(request
            .to_ascii_lowercase()
            .contains("chatgpt-account-id: acct-123"));
        assert!(request.contains("\"type\":\"message\""));
        assert!(request.contains("\"role\":\"user\""));
        assert!(request.contains("\"type\":\"input_text\""));
        assert!(request.contains("\"tool_choice\":\"auto\""));
        assert!(request.contains("\"parallel_tool_calls\":true"));
        assert!(request.contains("\"read_file\""));
        assert_eq!(reply.content, "");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].id, "call_1");
        assert_eq!(reply.tool_calls[0].name, "read_file");
        let payload: Value =
            serde_json::from_str(reply.provider_payload_json.as_deref().unwrap()).unwrap();
        assert_eq!(payload[0]["type"], "function_call");
        assert_eq!(payload[0]["call_id"], "call_1");
    }

    #[test]
    fn chatgpt_codex_payload_includes_responses_defaults_without_tools() {
        let payload = chatgpt_codex_payload(
            "gpt-5",
            &[ConversationMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            }],
            None,
            &[],
            None,
        )
        .unwrap();

        assert_eq!(payload["tools"], Value::Array(Vec::new()));
        assert_eq!(payload["tool_choice"], Value::String("auto".to_string()));
        assert_eq!(payload["parallel_tool_calls"], Value::Bool(false));
        assert_eq!(payload["include"], Value::Array(Vec::new()));
    }

    #[test]
    fn chatgpt_codex_payload_uses_responses_api_tool_shape() {
        let payload = chatgpt_codex_payload(
            "gpt-5",
            &[ConversationMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            }],
            None,
            &[ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            None,
        )
        .unwrap();

        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(payload["tools"][0]["name"], "read_file");
        assert_eq!(payload["tools"][0]["description"], "Read a file");
        assert_eq!(payload["tools"][0]["strict"], false);
        assert_eq!(payload["tools"][0]["parameters"]["type"], "object");
        assert!(payload["tools"][0].get("function").is_none());
    }

    #[test]
    fn chatgpt_codex_payload_uses_bundled_model_metadata_for_newer_models() {
        let descriptor = resolve_chatgpt_codex_model_descriptor("gpt-5.4")
            .expect("bundled model catalog should include gpt-5.4");
        let payload = chatgpt_codex_payload(
            "gpt-5.4",
            &[ConversationMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            }],
            None,
            &[],
            Some(&descriptor),
        )
        .unwrap();

        assert_eq!(
            payload["reasoning"]["effort"],
            descriptor.default_reasoning_effort.as_deref().unwrap()
        );
        assert!(payload["reasoning"].get("summary").is_none());
        assert_eq!(
            payload["include"],
            Value::Array(vec![Value::String(
                "reasoning.encrypted_content".to_string()
            )])
        );
        assert_eq!(
            payload["text"]["verbosity"],
            descriptor.default_verbosity.as_deref().unwrap()
        );
    }

    #[test]
    fn chatgpt_codex_model_descriptor_normalizes_summary_and_verbosity_defaults() {
        let descriptor = model_descriptor_from_chatgpt_codex_record(ChatGptCodexModelRecord {
            slug: "gpt-test".to_string(),
            supports_reasoning_summaries: Some(true),
            default_reasoning_summary: Some("none".to_string()),
            support_verbosity: Some(true),
            default_verbosity: Some("loud".to_string()),
            ..Default::default()
        });

        assert_eq!(descriptor.default_reasoning_summary, None);
        assert_eq!(descriptor.default_verbosity, None);
    }

    #[test]
    fn chatgpt_codex_payload_omits_invalid_reasoning_and_text_defaults() {
        let descriptor = ModelDescriptor {
            id: "gpt-test".to_string(),
            display_name: None,
            description: None,
            context_window: None,
            effective_context_window_percent: None,
            show_in_picker: true,
            default_reasoning_effort: None,
            supported_reasoning_levels: Vec::new(),
            supports_reasoning_summaries: true,
            default_reasoning_summary: Some("none".to_string()),
            support_verbosity: true,
            default_verbosity: Some("unsupported".to_string()),
            supports_parallel_tool_calls: true,
            priority: None,
        };
        let payload = chatgpt_codex_payload(
            "gpt-test",
            &[ConversationMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            }],
            None,
            &[],
            Some(&descriptor),
        )
        .unwrap();

        assert!(payload.get("reasoning").is_none());
        assert!(payload.get("text").is_none());
    }

    #[test]
    fn validate_tool_definitions_rejects_missing_name() {
        let error = validate_tool_definitions(
            &[ToolDefinition {
                name: "   ".to_string(),
                description: "broken".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
            }],
            "ChatGPT/Codex",
        )
        .unwrap_err();

        assert!(error.to_string().contains("missing a name"));
    }

    #[test]
    fn validate_tool_definitions_rejects_non_object_schema() {
        let error = validate_tool_definitions(
            &[ToolDefinition {
                name: "read_file".to_string(),
                description: "broken".to_string(),
                input_schema: json!(["not", "an", "object"]),
            }],
            "ChatGPT/Codex",
        )
        .unwrap_err();

        assert!(error.to_string().contains("object JSON schema"));
    }

    #[test]
    fn anthropic_message_encoding_supports_tool_use_and_results() {
        let messages = messages_to_anthropic(&[
            ConversationMessage {
                role: MessageRole::User,
                content: "Find the file".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            },
            ConversationMessage {
                role: MessageRole::Assistant,
                content: String::new(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: vec![ToolCall {
                    id: "toolu_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"src/main.rs\"}".to_string(),
                }],
                provider_payload_json: None,
                attachments: Vec::new(),
            },
            ConversationMessage {
                role: MessageRole::Tool,
                content: "1: fn main() {}".to_string(),
                tool_call_id: Some("toolu_1".to_string()),
                tool_name: Some("read_file".to_string()),
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            },
        ])
        .unwrap();

        assert_eq!(messages[1]["content"][0]["type"], "tool_use");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_1");
    }

    #[test]
    fn openai_message_encoding_supports_image_attachments() {
        let image = TestImageFile::new("png", &[1, 2, 3]);
        let messages = messages_to_openai(&[ConversationMessage {
            role: MessageRole::User,
            content: "Describe this".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: vec![image.attachment()],
        }])
        .unwrap();

        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][1]["type"], "image_url");
        assert_eq!(
            messages[0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AQID"
        );
    }

    #[test]
    fn anthropic_message_encoding_supports_image_attachments() {
        let image = TestImageFile::new("jpg", &[1, 2, 3]);
        let messages = messages_to_anthropic(&[ConversationMessage {
            role: MessageRole::User,
            content: "Describe this".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: vec![image.attachment()],
        }])
        .unwrap();

        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][1]["type"], "image");
        assert_eq!(
            messages[0]["content"][1]["source"]["media_type"],
            "image/jpeg"
        );
        assert_eq!(messages[0]["content"][1]["source"]["data"], "AQID");
    }

    #[test]
    fn ollama_message_encoding_supports_image_attachments() {
        let image = TestImageFile::new("webp", &[1, 2, 3]);
        let messages = messages_to_ollama(&[ConversationMessage {
            role: MessageRole::User,
            content: "Describe this".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: vec![image.attachment()],
        }])
        .unwrap();

        assert_eq!(messages[0]["images"][0], "AQID");
    }

    #[test]
    fn ollama_tool_calls_get_generated_ids_when_missing() {
        let tool_calls = parse_ollama_tool_calls(&json!({
            "tool_calls": [{
                "function": {
                    "name": "search_files",
                    "arguments": {"query": "main"}
                }
            }]
        }))
        .unwrap();

        assert_eq!(tool_calls[0].id, "ollama-tool-1");
        assert_eq!(tool_calls[0].name, "search_files");
    }

    #[test]
    fn exchanges_oauth_code_against_token_endpoint() {
        let (token_url, request_rx) = spawn_json_server(json!({
            "access_token": "access-123",
            "refresh_token": "refresh-123",
            "expires_in": 120,
            "token_type": "Bearer",
            "scope": "profile offline_access"
        }));
        let provider = oauth_provider(token_url);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let token = runtime
            .block_on(exchange_oauth_code(
                &Client::new(),
                &provider,
                "code-123",
                "verifier-123",
                "http://127.0.0.1:8080/callback",
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.contains("grant_type=authorization_code"));
        assert!(request.contains("code=code-123"));
        assert!(request.contains("code_verifier=verifier-123"));
        assert!(request.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8080%2Fcallback"));
        assert_eq!(token.access_token, "access-123");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh-123"));
        assert_eq!(token.token_type.as_deref(), Some("Bearer"));
        assert_eq!(token.scopes, vec!["profile", "offline_access"]);
    }

    #[test]
    fn oauth_token_exchange_surfaces_error_description() {
        let (token_url, _request_rx) = spawn_response_server(
            "400 Bad Request",
            "application/json",
            r#"{"error":"access_denied","error_description":"unknown authentication error"}"#,
        );
        let provider = oauth_provider(token_url);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let error = runtime
            .block_on(exchange_oauth_code(
                &Client::new(),
                &provider,
                "code-123",
                "verifier-123",
                "http://127.0.0.1:8080/callback",
            ))
            .unwrap_err();

        assert!(error.to_string().contains("unknown authentication error"));
    }

    #[test]
    fn oauth_token_exchange_surfaces_plain_text_errors() {
        let (token_url, _request_rx) =
            spawn_response_server("502 Bad Gateway", "text/plain", "service unavailable");
        let provider = oauth_provider(token_url);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let error = runtime
            .block_on(exchange_oauth_code(
                &Client::new(),
                &provider,
                "code-123",
                "verifier-123",
                "http://127.0.0.1:8080/callback",
            ))
            .unwrap_err();

        assert!(error.to_string().contains("service unavailable"));
    }

    #[test]
    fn refresh_keeps_existing_refresh_token_when_provider_omits_it() {
        let (token_url, request_rx) = spawn_json_server(json!({
            "access_token": "access-456",
            "expires_in": 45,
            "token_type": "Bearer"
        }));
        let provider = oauth_provider(token_url);
        let runtime = tokio::runtime::Runtime::new().unwrap();

        let refreshed = runtime
            .block_on(refresh_oauth_token(
                &Client::new(),
                &provider,
                &OAuthToken {
                    access_token: "stale".to_string(),
                    refresh_token: Some("refresh-keep".to_string()),
                    expires_at: None,
                    token_type: Some("Bearer".to_string()),
                    scopes: vec!["profile".to_string()],
                    id_token: None,
                    account_id: None,
                    user_id: None,
                    org_id: None,
                    project_id: None,
                    display_email: None,
                    subscription_type: None,
                },
            ))
            .unwrap();

        let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
        assert!(request.contains("grant_type=refresh_token"));
        assert!(request.contains("refresh_token=refresh-keep"));
        assert_eq!(refreshed.access_token, "access-456");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-keep"));
    }

    #[test]
    fn oversized_oauth_tokens_use_segmented_keychain_storage() {
        let token = OAuthToken {
            access_token: "a".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 350),
            refresh_token: Some("r".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 275)),
            expires_at: Some(Utc::now()),
            token_type: Some("Bearer".to_string()),
            scopes: vec!["profile".to_string(), "offline_access".to_string()],
            id_token: Some("i".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 125)),
            account_id: Some("account-123".to_string()),
            user_id: Some("user-123".to_string()),
            org_id: Some("org-123".to_string()),
            project_id: Some("project-123".to_string()),
            display_email: Some("user@example.com".to_string()),
            subscription_type: Some("pro".to_string()),
        };

        let serialized = serialize_oauth_token_secret("provider:test", &token).unwrap();
        let secret = match serialized {
            SerializedOAuthTokenSecret::Inline(_) => {
                panic!("expected oversized token to use segmented storage")
            }
            SerializedOAuthTokenSecret::Segmented(secret) => secret,
        };

        assert!(secret_storage_units(&secret.metadata_raw) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
        assert!(secret
            .segments
            .iter()
            .all(|(_, value)| secret_storage_units(value) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS));

        let stored_segments = secret
            .segments
            .iter()
            .cloned()
            .collect::<HashMap<String, String>>();
        let restored = deserialize_oauth_token_secret(
            "provider:test",
            &secret.metadata_raw,
            |segment_account| {
                stored_segments
                    .get(segment_account)
                    .cloned()
                    .ok_or_else(|| anyhow!("missing segment {segment_account}"))
            },
        )
        .unwrap();

        assert_eq!(restored, token);
    }

    #[test]
    fn oversized_plain_secrets_use_segmented_keychain_storage() {
        let secret_value = "k".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 512);

        let serialized = serialize_secret_storage("provider:test", &secret_value).unwrap();
        let secret = match serialized {
            SerializedSecret::Inline(_) => {
                panic!("expected oversized secret to use segmented storage")
            }
            SerializedSecret::Segmented(secret) => secret,
        };

        assert!(secret_storage_units(&secret.metadata_raw) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
        assert!(secret
            .segments
            .iter()
            .all(|(_, value)| secret_storage_units(value) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS));

        let stored_segments = secret
            .segments
            .iter()
            .cloned()
            .collect::<HashMap<String, String>>();
        let restored =
            deserialize_secret_storage("provider:test", &secret.metadata_raw, |segment_account| {
                stored_segments
                    .get(segment_account)
                    .cloned()
                    .ok_or_else(|| anyhow!("missing segment {segment_account}"))
            })
            .unwrap();

        assert_eq!(restored, secret_value);
    }

    #[test]
    fn split_secret_chunks_respects_utf16_boundaries() {
        let secret = format!("A{}\u{1F600}BC{}\u{1F680}", "D".repeat(16), "E".repeat(16));

        let chunks = split_secret_chunks(&secret, 8);

        assert!(chunks.iter().all(|chunk| secret_storage_units(chunk) <= 8));
        assert_eq!(chunks.concat(), secret);
    }

    fn oauth_provider(token_url: String) -> ProviderConfig {
        ProviderConfig {
            id: "oauth".to_string(),
            display_name: "OAuth".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://example.com/v1".to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: Some("model".to_string()),
            keychain_account: None,
            oauth: Some(OAuthConfig {
                client_id: "client".to_string(),
                authorization_url: "https://auth.example.com/authorize".to_string(),
                token_url,
                scopes: vec!["profile".to_string(), "offline_access".to_string()],
                extra_authorize_params: Vec::new(),
                extra_token_params: vec![KeyValuePair {
                    key: "audience".to_string(),
                    value: "agent-builder".to_string(),
                }],
            }),
            local: false,
        }
    }

    fn openai_browser_test_oauth_config() -> OAuthConfig {
        OAuthConfig {
            client_id: "browser-client".to_string(),
            authorization_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/authorize"),
            token_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/token"),
            scopes: vec!["openid".to_string(), "offline_access".to_string()],
            extra_authorize_params: Vec::new(),
            extra_token_params: Vec::new(),
        }
    }

    fn build_sse_body(events: &[Value]) -> String {
        let mut body = String::new();
        for event in events {
            let kind = event
                .get("type")
                .and_then(Value::as_str)
                .expect("SSE fixture event missing type");
            if event.as_object().is_some_and(|event| event.len() == 1) {
                body.push_str(&format!("event: {kind}\n\n"));
            } else {
                body.push_str(&format!("event: {kind}\ndata: {event}\n\n"));
            }
        }
        body
    }

    fn spawn_json_server(body: Value) -> (String, Receiver<String>) {
        spawn_response_server("200 OK", "application/json", &body.to_string())
    }

    fn spawn_response_server_at(
        base_path: &str,
        status: &str,
        content_type: &str,
        body: &str,
    ) -> (String, Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let status = status.to_string();
        let content_type = content_type.to_string();
        let body = body.to_string();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 65536];
            let bytes_read = stream.read(&mut buffer).unwrap();
            request_tx
                .send(String::from_utf8_lossy(&buffer[..bytes_read]).to_string())
                .unwrap();

            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        (format!("http://{address}{base_path}"), request_rx)
    }

    fn spawn_response_server(
        status: &str,
        content_type: &str,
        body: &str,
    ) -> (String, Receiver<String>) {
        spawn_response_server_at("/token", status, content_type, body)
    }

    struct TestImageFile {
        path: PathBuf,
    }

    impl TestImageFile {
        fn new(extension: &str, bytes: &[u8]) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "agent-providers-test-{}-{unique}.{extension}",
                std::process::id()
            ));
            fs::write(&path, bytes).unwrap();
            Self { path }
        }

        fn attachment(&self) -> InputAttachment {
            InputAttachment {
                kind: AttachmentKind::Image,
                path: self.path.clone(),
            }
        }
    }

    impl Drop for TestImageFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
