use super::argument_helpers::optional_string;
use super::*;

pub(super) async fn spawn_subagents(context: &ToolContext, args: &Value) -> Result<String> {
    let tasks = args
        .get("tasks")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("spawn_subagents requires a tasks array"))?
        .iter()
        .map(parse_subagent_task)
        .collect::<Result<Vec<_>>>()?;

    let request = BatchTaskRequest {
        tasks,
        cwd: Some(
            optional_string(args, "cwd")
                .map(PathBuf::from)
                .unwrap_or_else(|| context.cwd.clone()),
        ),
        thinking_level: optional_thinking_level(args, "thinking_level")?
            .or(context.default_thinking_level),
        strategy: optional_subagent_strategy(args, "strategy")?,
        parent_alias: context.current_alias.clone(),
    };
    let response = execute_batch_request(
        &context.state,
        request,
        DelegationExecutionOptions {
            background: context.background,
            permission_preset: Some(context.permission_preset),
            delegation_depth: context.delegation_depth,
        },
    )
    .await
    .map_err(|error| anyhow!(error.message))?;
    serde_json::to_string_pretty(&response).context("failed to serialize subagent results")
}

pub(super) fn spawn_subagents_description(context: &ToolContext) -> String {
    let mut description = String::from(
        "Delegate one or more prompts to subagents. Use 'target' for a provider name, model family, host family, or explicit alias like claude, codex, chatgpt, kimi, sonnet, moonshot, or a custom alias. If target/alias/provider is omitted, use the current alias first and then other logged-in providers from the configured pool.",
    );
    if !context.delegation_targets.is_empty() {
        let targets = context
            .delegation_targets
            .iter()
            .take(6)
            .map(|target| {
                let names = target
                    .target_names
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} [{}]", target.alias, names)
            })
            .collect::<Vec<_>>()
            .join("; ");
        description.push_str(&format!(" Available targets: {targets}."));
        if context.delegation_targets.len() > 6 {
            description.push_str(&format!(
                " {} more target(s) are also available.",
                context.delegation_targets.len() - 6
            ));
        }
    }
    description.push_str(&format!(
        " Current delegation depth is {}. Max depth is {}. Max parallel subagents is {}.",
        context.delegation_depth,
        context.delegation.max_depth,
        context.delegation.max_parallel_subagents
    ));
    description
}

pub(super) fn parse_subagent_task(value: &Value) -> Result<agent_core::SubAgentTask> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("each subagent task must be an object"))?;
    let prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("each subagent task requires a non-empty prompt"))?;

    Ok(agent_core::SubAgentTask {
        prompt: prompt.to_string(),
        target: object
            .get("target")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        alias: object
            .get("alias")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        provider_id: object
            .get("provider_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        requested_model: object
            .get("requested_model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        cwd: object
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
        thinking_level: value_thinking_level(object.get("thinking_level"), "thinking_level")?,
        output_schema_json: object
            .get("output_schema_json")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        strategy: value_subagent_strategy(object.get("strategy"), "strategy")?,
    })
}

fn optional_thinking_level(args: &Value, key: &str) -> Result<Option<ThinkingLevel>> {
    value_thinking_level(args.get(key), key)
}

fn value_thinking_level(value: Option<&Value>, key: &str) -> Result<Option<ThinkingLevel>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .ok_or_else(|| anyhow!("'{key}' must be a string"))?;
    let parsed = match raw.trim().to_ascii_lowercase().as_str() {
        "none" => ThinkingLevel::None,
        "minimal" => ThinkingLevel::Minimal,
        "low" => ThinkingLevel::Low,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        "xhigh" => ThinkingLevel::XHigh,
        other => bail!("unsupported {key} '{other}'"),
    };
    Ok(Some(parsed))
}

fn optional_subagent_strategy(args: &Value, key: &str) -> Result<Option<SubAgentStrategy>> {
    value_subagent_strategy(args.get(key), key)
}

fn value_subagent_strategy(value: Option<&Value>, key: &str) -> Result<Option<SubAgentStrategy>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .ok_or_else(|| anyhow!("'{key}' must be a string"))?;
    let parsed = match raw.trim().to_ascii_lowercase().as_str() {
        "single_best" => SubAgentStrategy::SingleBest,
        "parallel_best_effort" => SubAgentStrategy::ParallelBestEffort,
        "parallel_all" => SubAgentStrategy::ParallelAll,
        other => bail!("unsupported {key} '{other}'"),
    };
    Ok(Some(parsed))
}
