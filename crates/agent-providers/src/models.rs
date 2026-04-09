use super::*;
use crate::anthropic::list_anthropic_models;
use crate::chatgpt_codex::list_chatgpt_codex_model_descriptors;
use crate::chatgpt_codex_catalog::{
    default_model_descriptor, resolve_chatgpt_codex_model_descriptor,
};
use crate::ollama::list_ollama_models;
use crate::openai_compatible::list_openai_models;

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
        ProviderKind::ChatGptCodex => resolve_chatgpt_codex_model_descriptor(model)
            .unwrap_or_else(|| default_model_descriptor(model)),
        _ => default_model_descriptor(model),
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
                        capabilities: ModelToolCapabilities::default(),
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

pub(crate) fn validate_default_model(provider: &ProviderConfig, models: &[String]) -> Result<()> {
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
