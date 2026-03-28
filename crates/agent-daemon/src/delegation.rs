use std::collections::{BTreeSet, HashSet};

use agent_core::{
    AppConfig, BatchTaskRequest, DelegationLimit, DelegationTarget, ModelAlias, ProviderConfig,
    SubAgentResult, SubAgentStrategy, SubAgentTask,
};
use axum::http::StatusCode;

use crate::{
    runtime::provider_has_runnable_access, ApiError, ResolvedSubAgentTask,
    MAX_RESOLVED_SUBAGENT_RUNS, MAX_SUBAGENT_TASKS_PER_REQUEST,
};

pub(crate) fn resolve_delegation_tasks(
    config: &AppConfig,
    payload: &BatchTaskRequest,
) -> Result<Vec<ResolvedSubAgentTask>, ApiError> {
    if payload.tasks.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "at least one subagent task is required",
        ));
    }
    if payload.tasks.len() > MAX_SUBAGENT_TASKS_PER_REQUEST {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "subagent task limit exceeded (max {})",
                MAX_SUBAGENT_TASKS_PER_REQUEST
            ),
        ));
    }

    let mut resolved = Vec::new();
    for task in &payload.tasks {
        let strategy = task
            .strategy
            .or(payload.strategy)
            .unwrap_or(SubAgentStrategy::SingleBest);
        let candidates =
            resolve_subagent_candidates(config, payload.parent_alias.as_deref(), task)?;
        let selected = match strategy {
            SubAgentStrategy::SingleBest => candidates.into_iter().take(1).collect::<Vec<_>>(),
            SubAgentStrategy::ParallelBestEffort | SubAgentStrategy::ParallelAll => candidates,
        };

        for (alias, provider) in selected {
            resolved.push(ResolvedSubAgentTask {
                prompt: task.prompt.clone(),
                alias,
                provider,
                requested_model: task.requested_model.clone(),
                cwd: task.cwd.clone().or_else(|| payload.cwd.clone()),
                thinking_level: task.thinking_level.or(payload.thinking_level),
                task_mode: task.task_mode.or(payload.task_mode),
                output_schema_json: task.output_schema_json.clone(),
            });
            if resolved.len() > MAX_RESOLVED_SUBAGENT_RUNS {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "resolved subagent run limit exceeded (max {})",
                        MAX_RESOLVED_SUBAGENT_RUNS
                    ),
                ));
            }
        }
    }

    Ok(resolved)
}

pub(crate) fn resolve_subagent_candidates(
    config: &AppConfig,
    parent_alias: Option<&str>,
    task: &SubAgentTask,
) -> Result<Vec<(ModelAlias, ProviderConfig)>, ApiError> {
    if let Some(alias_name) = task.alias.as_deref() {
        let (alias, provider) = resolve_alias_and_provider_from_config(config, Some(alias_name))?;
        if task
            .provider_id
            .as_deref()
            .is_some_and(|provider_id| provider_id != provider.id)
        {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "alias '{}' does not belong to provider '{}'",
                    alias_name,
                    task.provider_id.as_deref().unwrap_or_default()
                ),
            ));
        }
        return Ok(vec![(alias, provider)]);
    }

    if let Some(target) = task
        .target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some((alias, provider)) = all_usable_alias_targets(config)
            .into_iter()
            .find(|(alias, _)| normalize_target_key(&alias.alias) == normalize_target_key(target))
        {
            if task
                .provider_id
                .as_deref()
                .is_some_and(|provider_id| provider_id != provider.id)
            {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "target '{}' resolved to alias '{}' on provider '{}', which does not match provider '{}'",
                        target,
                        alias.alias,
                        provider.id,
                        task.provider_id.as_deref().unwrap_or_default()
                    ),
                ));
            }
            return Ok(vec![(alias, provider)]);
        }

        let mut pool = provider_pool_candidates(config, parent_alias)?;
        if let Some(provider_id) = task.provider_id.as_deref() {
            pool.retain(|(_, provider)| provider.id == provider_id);
            if pool.is_empty() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("no usable alias found for provider '{}'", provider_id),
                ));
            }
        }
        let matches = pool
            .into_iter()
            .filter(|(alias, provider)| {
                delegation_target_names(alias, provider)
                    .iter()
                    .any(|name| normalize_target_key(name) == normalize_target_key(target))
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("no logged-in delegation target matched '{target}'"),
            ));
        }
        return Ok(matches);
    }

    let pool = provider_pool_candidates(config, parent_alias)?;
    if let Some(provider_id) = task.provider_id.as_deref() {
        let matches = pool
            .into_iter()
            .filter(|(_, provider)| provider.id == provider_id)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("no usable alias found for provider '{}'", provider_id),
            ));
        }
        return Ok(matches);
    }

    if pool.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "no usable logged-in providers are configured",
        ));
    }
    Ok(pool)
}

pub(crate) fn provider_pool_candidates(
    config: &AppConfig,
    parent_alias: Option<&str>,
) -> Result<Vec<(ModelAlias, ProviderConfig)>, ApiError> {
    let mut ordered_aliases = Vec::new();
    if let Some(parent_alias) = parent_alias {
        ordered_aliases.push(parent_alias.to_string());
    }
    if let Ok(alias) = config.main_alias() {
        ordered_aliases.push(alias.alias.clone());
    }
    ordered_aliases.extend(config.aliases.iter().map(|alias| alias.alias.clone()));

    let mut seen_providers = HashSet::new();
    let mut resolved = Vec::new();
    for alias_name in ordered_aliases {
        let Ok((alias, provider)) =
            resolve_alias_and_provider_from_config(config, Some(&alias_name))
        else {
            continue;
        };
        if !config.provider_delegation_enabled(&provider.id) {
            continue;
        }
        if seen_providers.insert(provider.id.clone()) {
            resolved.push((alias, provider));
        }
    }
    Ok(resolved)
}

fn all_usable_alias_targets(config: &AppConfig) -> Vec<(ModelAlias, ProviderConfig)> {
    config
        .aliases
        .iter()
        .filter_map(|alias| {
            let provider = config.resolve_provider(&alias.provider_id)?;
            if !config.provider_delegation_enabled(&provider.id) {
                return None;
            }
            if ensure_provider_usable(&provider).is_err() {
                return None;
            }
            Some((alias.clone(), provider))
        })
        .collect()
}

fn delegation_target_names(alias: &ModelAlias, provider: &ProviderConfig) -> Vec<String> {
    let mut names = BTreeSet::new();
    for value in [
        alias.alias.as_str(),
        alias.model.as_str(),
        provider.id.as_str(),
        provider.display_name.as_str(),
    ] {
        add_target_name_variants(&mut names, value);
    }
    add_target_name_variants_from_base_url(&mut names, &provider.base_url);
    match provider.kind {
        agent_core::ProviderKind::ChatGptCodex => {
            for value in ["chatgpt", "chat-gpt", "codex", "openai"] {
                add_target_name_variants(&mut names, value);
            }
        }
        agent_core::ProviderKind::Anthropic => {
            for value in ["claude", "anthropic"] {
                add_target_name_variants(&mut names, value);
            }
        }
        agent_core::ProviderKind::OpenAiCompatible => {
            let base_url = provider.base_url.to_ascii_lowercase();
            if base_url.contains("moonshot") || provider.id.to_ascii_lowercase().contains("kimi") {
                for value in ["kimi", "moonshot"] {
                    add_target_name_variants(&mut names, value);
                }
            } else if base_url.contains("openai.com") {
                add_target_name_variants(&mut names, "openai");
            }
        }
        agent_core::ProviderKind::Ollama => add_target_name_variants(&mut names, "ollama"),
    }
    names.into_iter().collect()
}

fn add_target_name_variants(names: &mut BTreeSet<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    names.insert(trimmed.to_string());

    let lowercase = trimmed.to_ascii_lowercase();
    names.insert(lowercase.clone());
    let collapsed = lowercase.replace(' ', "");
    if !collapsed.is_empty() {
        names.insert(collapsed);
    }

    for token in trimmed
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(str::trim)
        .filter(|token| token.len() >= 2)
    {
        names.insert(token.to_ascii_lowercase());
    }
}

fn add_target_name_variants_from_base_url(names: &mut BTreeSet<String>, base_url: &str) {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return;
    };
    let Some(host) = url.host_str() else {
        return;
    };
    add_target_name_variants(names, host);
    for segment in host.split('.') {
        let segment = segment.trim().to_ascii_lowercase();
        if matches!(segment.as_str(), "api" | "www" | "platform" | "com" | "ai") {
            continue;
        }
        add_target_name_variants(names, &segment);
    }
}

fn normalize_target_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(crate) fn delegation_targets_from_config(
    config: &AppConfig,
    parent_alias: Option<&str>,
) -> Vec<DelegationTarget> {
    let primary_aliases = provider_pool_candidates(config, parent_alias)
        .unwrap_or_default()
        .into_iter()
        .map(|(alias, _)| alias.alias)
        .collect::<HashSet<_>>();
    let mut targets = all_usable_alias_targets(config)
        .into_iter()
        .map(|(alias, provider)| DelegationTarget {
            alias: alias.alias.clone(),
            provider_id: provider.id.clone(),
            provider_display_name: provider.display_name.clone(),
            model: alias.model.clone(),
            target_names: delegation_target_names(&alias, &provider),
            primary: primary_aliases.contains(&alias.alias),
        })
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| {
        right
            .primary
            .cmp(&left.primary)
            .then_with(|| left.alias.cmp(&right.alias))
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });
    targets
}

pub(crate) fn summarize_batch_results(results: &[SubAgentResult]) -> String {
    let successes = results.iter().filter(|result| result.success).count();
    let failures = results.len().saturating_sub(successes);
    let success_labels = results
        .iter()
        .filter(|result| result.success)
        .map(|result| format!("{}@{}", result.alias, result.provider_id))
        .collect::<Vec<_>>();
    let failure_labels = results
        .iter()
        .filter(|result| !result.success)
        .map(|result| format!("{}@{}", result.alias, result.provider_id))
        .collect::<Vec<_>>();
    match (successes, failures) {
        (_, 0) => format!(
            "{} subagent run(s) succeeded: {}",
            successes,
            success_labels.join(", ")
        ),
        (0, _) => format!(
            "{} subagent run(s) failed: {}",
            failures,
            failure_labels.join(", ")
        ),
        _ => format!(
            "{} succeeded ({}) and {} failed ({})",
            successes,
            success_labels.join(", "),
            failures,
            failure_labels.join(", ")
        ),
    }
}

pub(crate) fn normalize_delegation_limit(
    limit: DelegationLimit,
    minimum: u8,
) -> Result<DelegationLimit, ApiError> {
    match limit {
        DelegationLimit::Limited { value } if value >= minimum => {
            Ok(DelegationLimit::Limited { value })
        }
        DelegationLimit::Limited { .. } => Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("delegation limits must be at least {minimum}"),
        )),
        DelegationLimit::Unlimited => Ok(DelegationLimit::Unlimited),
    }
}

pub(crate) fn resolve_alias_and_provider_from_config(
    config: &AppConfig,
    requested_alias: Option<&str>,
) -> Result<(ModelAlias, ProviderConfig), ApiError> {
    let alias = match requested_alias {
        Some(alias) => config
            .get_alias(alias)
            .cloned()
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "unknown alias"))?,
        None => config
            .main_alias()
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?
            .clone(),
    };
    let provider = config.resolve_provider(&alias.provider_id).ok_or_else(|| {
        ApiError::new(StatusCode::BAD_REQUEST, "alias references unknown provider")
    })?;
    ensure_provider_usable(&provider)?;
    Ok((alias, provider))
}

fn ensure_provider_usable(provider: &ProviderConfig) -> Result<(), ApiError> {
    if provider_has_runnable_access(provider) {
        return Ok(());
    }
    Err(ApiError::new(
        StatusCode::BAD_REQUEST,
        format!(
            "provider '{}' is configured but does not have usable saved credentials",
            provider.id
        ),
    ))
}
