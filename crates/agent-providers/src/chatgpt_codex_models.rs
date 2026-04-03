use agent_core::{
    HostedToolKind, ModelDescriptor, ModelToolCapabilities, ReasoningLevelDescriptor, ToolBackend,
};
use serde::Deserialize;
use std::sync::OnceLock;
use tracing::warn;

use super::CHATGPT_CODEX_BUNDLED_MODELS_JSON;

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ChatGptCodexModelsResponse {
    #[serde(default)]
    pub(super) models: Vec<ChatGptCodexModelRecord>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ChatGptCodexModelRecord {
    #[serde(default)]
    pub(super) slug: String,
    #[serde(default)]
    pub(super) display_name: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) default_reasoning_level: Option<String>,
    #[serde(default)]
    pub(super) supported_reasoning_levels: Vec<ChatGptCodexReasoningLevelRecord>,
    #[serde(default)]
    pub(super) visibility: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<i64>,
    #[serde(default)]
    pub(super) supports_reasoning_summaries: Option<bool>,
    #[serde(default)]
    pub(super) default_reasoning_summary: Option<String>,
    #[serde(default)]
    pub(super) support_verbosity: Option<bool>,
    #[serde(default)]
    pub(super) default_verbosity: Option<String>,
    #[serde(default)]
    pub(super) supports_parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub(super) web_search_tool_type: Option<String>,
    #[serde(default)]
    pub(super) apply_patch_tool_type: Option<String>,
    #[serde(default)]
    pub(super) shell_type: Option<String>,
    #[serde(default)]
    pub(super) context_window: Option<i64>,
    #[serde(default)]
    pub(super) effective_context_window_percent: Option<i64>,
    #[serde(default)]
    pub(super) available_in_plans: Vec<String>,
    #[serde(default)]
    pub(super) experimental_supported_tools: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct ChatGptCodexReasoningLevelRecord {
    #[serde(default)]
    pub(super) effort: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
}

pub(super) fn bundled_chatgpt_codex_model_catalog() -> &'static [ChatGptCodexModelRecord] {
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

pub(super) fn merge_chatgpt_codex_model_catalog(
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

pub(super) fn model_descriptor_from_chatgpt_codex_record(
    record: ChatGptCodexModelRecord,
) -> ModelDescriptor {
    let capabilities = chatgpt_codex_model_capabilities(&record);
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
        capabilities,
    }
}

pub(super) fn default_model_descriptor(model: &str) -> ModelDescriptor {
    ModelDescriptor {
        id: model.to_string(),
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

pub(super) fn resolve_chatgpt_codex_model_descriptor(model: &str) -> Option<ModelDescriptor> {
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

pub(super) fn normalize_chatgpt_codex_reasoning_summary(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_reasoning_summary_str)
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_chatgpt_codex_reasoning_summary_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some("auto"),
        "concise" => Some("concise"),
        "detailed" => Some("detailed"),
        "none" | "" => None,
        _ => None,
    }
}

pub(super) fn normalize_chatgpt_codex_verbosity(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .and_then(normalize_chatgpt_codex_verbosity_str)
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_chatgpt_codex_verbosity_str(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "low" => Some("low"),
        "medium" => Some("medium"),
        "high" => Some("high"),
        "none" | "" => None,
        _ => None,
    }
}

fn chatgpt_codex_model_capabilities(record: &ChatGptCodexModelRecord) -> ModelToolCapabilities {
    let capabilities = ModelToolCapabilities {
        web_search: record.web_search_tool_type.is_some()
            || record
                .experimental_supported_tools
                .iter()
                .any(|tool| tool.eq_ignore_ascii_case("web_search")),
        apply_patch: record.apply_patch_tool_type.is_some()
            || record
                .experimental_supported_tools
                .iter()
                .any(|tool| tool.eq_ignore_ascii_case("apply_patch")),
        shell: record.shell_type.is_some()
            || record
                .experimental_supported_tools
                .iter()
                .any(|tool| tool.eq_ignore_ascii_case("shell")),
        local_shell: record
            .experimental_supported_tools
            .iter()
            .any(|tool| tool.eq_ignore_ascii_case("local_shell")),
        ..Default::default()
    };
    capabilities
}

pub(super) fn responses_tool_backend(item_type: &str) -> (ToolBackend, Option<HostedToolKind>) {
    let normalized = item_type.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "web_search_call" | "web_search" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::WebSearch),
        ),
        "file_search_call" | "file_search" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::FileSearch),
        ),
        "image_generation_call" | "image_generation" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::ImageGeneration),
        ),
        "code_interpreter_call" | "code_interpreter" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::CodeInterpreter),
        ),
        "computer_call" | "computer_use" | "computer_use_call" => (
            ToolBackend::ProviderProtocol,
            Some(HostedToolKind::ComputerUse),
        ),
        "remote_mcp_call" | "remote_mcp" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::RemoteMcp),
        ),
        "tool_search_call" | "tool_search" => (
            ToolBackend::ProviderBuiltin,
            Some(HostedToolKind::ToolSearch),
        ),
        "shell_call" | "shell" => (ToolBackend::ProviderProtocol, Some(HostedToolKind::Shell)),
        "apply_patch_call" | "apply_patch" => (
            ToolBackend::ProviderProtocol,
            Some(HostedToolKind::ApplyPatch),
        ),
        "local_shell_call" | "local_shell" => (
            ToolBackend::ProviderProtocol,
            Some(HostedToolKind::LocalShell),
        ),
        _ => (ToolBackend::ProviderBuiltin, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_stay_conservative_for_unhandled_hosted_tools() {
        let record = ChatGptCodexModelRecord {
            web_search_tool_type: Some("web_search".to_string()),
            apply_patch_tool_type: Some("apply_patch".to_string()),
            shell_type: Some("shell".to_string()),
            experimental_supported_tools: vec![
                "file_search".to_string(),
                "image_generation".to_string(),
                "code_interpreter".to_string(),
                "computer_use".to_string(),
                "remote_mcp".to_string(),
                "tool_search".to_string(),
                "skills".to_string(),
            ],
            ..Default::default()
        };

        let capabilities = chatgpt_codex_model_capabilities(&record);
        assert!(capabilities.web_search);
        assert!(capabilities.apply_patch);
        assert!(capabilities.shell);
        assert!(!capabilities.file_search);
        assert!(!capabilities.image_generation);
        assert!(!capabilities.code_interpreter);
        assert!(!capabilities.computer_use);
        assert!(!capabilities.remote_mcp);
        assert!(!capabilities.tool_search);
        assert!(!capabilities.skills);
    }
}
