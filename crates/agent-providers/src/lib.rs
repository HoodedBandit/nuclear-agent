use agent_core::{
    redact_sensitive_json_value, redact_sensitive_text, AttachmentKind, AuthMode,
    ConversationMessage, HostedToolKind, InputAttachment, MessageRole, ModelToolCapabilities,
    OAuthConfig, OAuthToken, ProviderConfig, ProviderHealth, ProviderKind, ProviderOutputItem,
    ProviderReply, ThinkingLevel, ToolBackend, ToolCall, ToolDefinition, KEYCHAIN_SERVICE,
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

pub use agent_core::{ModelDescriptor, ReasoningLevelDescriptor};

const OAUTH_REFRESH_SKEW_SECONDS: i64 = 60;
const OPENAI_BROWSER_AUTH_ISSUER: &str = "https://auth.openai.com";
const CHATGPT_CODEX_ORIGINATOR: &str = "codex_cli_rs";
const CHATGPT_CODEX_BUNDLED_MODELS_JSON: &str =
    include_str!("../../../codex-main/codex-rs/core/models.json");

mod anthropic;
mod attachments;
mod chatgpt_codex;
mod chatgpt_codex_catalog;
mod keyring_store;
mod models;
mod oauth;
mod ollama;
mod openai_compatible;
#[cfg(test)]
mod tests;
mod tools;

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

pub fn build_oauth_authorization_url(
    provider: &ProviderConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String> {
    oauth::build_oauth_authorization_url(provider, redirect_uri, state, code_challenge)
}

pub async fn exchange_oauth_code(
    client: &Client,
    provider: &ProviderConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    oauth::exchange_oauth_code(client, provider, code, code_verifier, redirect_uri).await
}

pub async fn health_check(client: &Client, provider: &ProviderConfig) -> ProviderHealth {
    models::health_check(client, provider).await
}

pub async fn list_models(client: &Client, provider: &ProviderConfig) -> Result<Vec<String>> {
    models::list_models(client, provider).await
}

pub fn describe_model(provider: &ProviderConfig, model: &str) -> ModelDescriptor {
    models::describe_model(provider, model)
}

pub async fn list_model_descriptors(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<Vec<ModelDescriptor>> {
    models::list_model_descriptors(client, provider).await
}

pub async fn list_model_descriptors_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<ModelDescriptor>> {
    models::list_model_descriptors_with_overrides(
        client,
        provider,
        api_key_override,
        oauth_token_override,
    )
    .await
}

pub async fn list_models_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    models::list_models_with_overrides(client, provider, api_key_override, oauth_token_override)
        .await
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
            openai_compatible::run_openai_compatible(
                client,
                provider,
                &model,
                messages,
                thinking_level,
                tools,
            )
            .await
        }
        ProviderKind::ChatGptCodex => {
            chatgpt_codex::run_chatgpt_codex(
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
            anthropic::run_anthropic(client, provider, &model, messages, thinking_level, tools)
                .await
        }
        ProviderKind::Ollama => {
            ollama::run_ollama(client, provider, &model, messages, thinking_level, tools).await
        }
    }
}

pub async fn compute_embedding(
    client: &Client,
    provider: &ProviderConfig,
    model: &str,
    text: &str,
    dimensions: Option<u32>,
) -> Result<Vec<f32>> {
    openai_compatible::compute_embedding(client, provider, model, text, dimensions).await
}

fn trim_slash(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn provider_endpoint_url(provider: &ProviderConfig, suffix: &str, label: &str) -> Result<Url> {
    let base_url = format!("{}/", trim_slash(&provider.base_url));
    let base = Url::parse(&base_url)
        .with_context(|| format!("provider '{}' has an invalid base URL", provider.id))?;
    let endpoint = base
        .join(suffix.trim_start_matches('/'))
        .with_context(|| format!("failed to build {label} URL"))?;
    validate_provider_transport(provider, &endpoint, label)?;
    Ok(endpoint)
}

fn validate_provider_transport(provider: &ProviderConfig, url: &Url, label: &str) -> Result<()> {
    if matches!(provider.auth_mode, AuthMode::None) {
        return Ok(());
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("provider '{}' {label} URL is missing a host", provider.id))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(host) => Ok(()),
        "http" => bail!(
            "provider '{}' {label} URL must use https unless it targets localhost or a loopback address",
            provider.id
        ),
        scheme => bail!(
            "provider '{}' {label} URL must use https or loopback-local http; found '{}'",
            provider.id,
            scheme
        ),
    }
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let candidate = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    candidate
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
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
        return redact_sensitive_text(text);
    }

    if let Some(text) = body.get("error_description").and_then(Value::as_str) {
        return redact_sensitive_text(text);
    }

    serde_json::to_string(&redact_sensitive_json_value(body))
        .map(|text| redact_sensitive_text(&text))
        .unwrap_or_else(|_| "[REDACTED]".to_string())
}

fn provider_error_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "authentication error",
        StatusCode::TOO_MANY_REQUESTS => "rate limit exceeded",
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => "invalid request",
        StatusCode::NOT_FOUND => "resource not found",
        status if status.is_server_error() => "provider server error",
        _ => "provider error",
    }
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
