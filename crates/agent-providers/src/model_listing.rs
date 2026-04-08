use agent_core::{
    ModelDescriptor, ModelToolCapabilities, OAuthToken, ProviderConfig, ProviderHealth,
    ProviderKind, ProviderProfile, ReasoningLevelDescriptor,
};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use super::chatgpt_codex_models as codex_models;
use super::common::{extract_error, trim_slash};
use super::oauth::apply_auth_with_overrides;

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

pub fn describe_model(provider: &ProviderConfig, model: &str) -> ModelDescriptor {
    match provider.kind {
        ProviderKind::ChatGptCodex => codex_models::resolve_chatgpt_codex_model_descriptor(model)
            .unwrap_or_else(|| codex_models::default_model_descriptor(model)),
        _ => codex_models::default_model_descriptor(model),
    }
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
                capabilities: ModelToolCapabilities::default(),
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
                        capabilities: ModelToolCapabilities::default(),
                    })
                    .collect(),
            )
        }
        ProviderKind::ChatGptCodex => {
            list_chatgpt_codex_model_descriptors(client, provider, oauth_token_override).await
        }
        ProviderKind::OpenAiCompatible => {
            list_openai_model_descriptors(client, provider, api_key_override, oauth_token_override)
                .await
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
    let request = super::oauth::apply_auth(client, provider, request).await?;
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

pub(super) fn validate_default_model(provider: &ProviderConfig, models: &[String]) -> Result<()> {
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

#[allow(dead_code)]
async fn list_openai_models(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    Ok(
        list_openai_model_descriptors(client, provider, api_key_override, oauth_token_override)
            .await?
            .into_iter()
            .map(|model| model.id)
            .collect(),
    )
}

async fn list_openai_model_descriptors(
    client: &Client,
    provider: &ProviderConfig,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<ModelDescriptor>> {
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
            return Ok(provider
                .default_model
                .clone()
                .into_iter()
                .map(|id| default_model_descriptor(&id))
                .collect());
        }
        bail!("model listing failed: {}", extract_error(&body));
    }

    let models = body
        .get("data")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| model_descriptor_from_openai_entry(provider, entry))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if models.is_empty() {
        if let Some(model) = &provider.default_model {
            Ok(vec![default_model_descriptor(model)])
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
    _oauth_token_override: Option<&OAuthToken>,
) -> Result<Vec<String>> {
    if !matches!(provider.auth_mode, agent_core::AuthMode::ApiKey) {
        bail!("anthropic providers require API key authentication");
    }
    let url = format!("{}/v1/models", trim_slash(&provider.base_url));
    let api_key = match api_key_override {
        Some(api_key) => api_key.to_string(),
        None => super::keyring_store::api_key_for(provider)?,
    };
    let request = client
        .get(url)
        .header("anthropic-version", "2023-06-01")
        .header("x-api-key", api_key);
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

fn default_model_descriptor(id: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
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
    }
}

fn model_descriptor_from_openai_entry(
    provider: &ProviderConfig,
    entry: &Value,
) -> Option<ModelDescriptor> {
    let id = entry.get("id").and_then(Value::as_str)?.to_string();
    let supported_parameters = entry
        .get("supported_parameters")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let supports_function_calling = entry
        .get("supportsFunctionCalling")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || supported_parameters.iter().any(|value| {
            matches!(
                value.as_str(),
                "tools" | "tool_choice" | "parallel_tool_calls" | "function_calling"
            )
        });
    let context_window = entry
        .get("context_length")
        .and_then(Value::as_i64)
        .or_else(|| entry.get("context_window").and_then(Value::as_i64))
        .or_else(|| {
            entry
                .get("top_provider")
                .and_then(|value| value.get("context_length"))
                .and_then(Value::as_i64)
        });
    let display_name = entry
        .get("name")
        .or_else(|| entry.get("display_name"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let description = entry
        .get("description")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let provider_profile = provider.effective_profile();
    let reasoning_levels = match provider_profile {
        ProviderProfile::OpenRouter | ProviderProfile::OpenAi | ProviderProfile::Venice => vec![
            ReasoningLevelDescriptor {
                effort: "low".to_string(),
                description: None,
            },
            ReasoningLevelDescriptor {
                effort: "medium".to_string(),
                description: None,
            },
            ReasoningLevelDescriptor {
                effort: "high".to_string(),
                description: None,
            },
        ],
        _ => Vec::new(),
    };

    Some(ModelDescriptor {
        id,
        display_name,
        description,
        context_window,
        effective_context_window_percent: None,
        show_in_picker: true,
        default_reasoning_effort: None,
        supported_reasoning_levels: reasoning_levels,
        supports_reasoning_summaries: false,
        default_reasoning_summary: None,
        support_verbosity: false,
        default_verbosity: None,
        supports_parallel_tool_calls: supports_function_calling,
        priority: None,
        capabilities: ModelToolCapabilities::default(),
    })
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
    let token =
        super::chatgpt_codex::codex_session_token(client, provider, oauth_token_override).await?;
    let models = super::chatgpt_codex::load_chatgpt_codex_model_descriptors(
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

pub(super) fn supports_local_model_listing_fallback(
    provider: &ProviderConfig,
    status: reqwest::StatusCode,
) -> bool {
    provider.local
        && provider.default_model.is_some()
        && matches!(
            status,
            reqwest::StatusCode::NOT_FOUND
                | reqwest::StatusCode::METHOD_NOT_ALLOWED
                | reqwest::StatusCode::NOT_IMPLEMENTED
        )
}
