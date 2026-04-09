use super::*;

pub(crate) fn tool_definitions_to_openai(tools: &[ToolDefinition]) -> Vec<Value> {
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

pub(crate) fn validate_tool_definitions(
    tools: &[ToolDefinition],
    provider_label: &str,
) -> Result<()> {
    for tool in tools {
        if tool.name.trim().is_empty() {
            bail!("{provider_label} tool definition is missing a name");
        }
        if !matches!(tool.backend, ToolBackend::LocalFunction) && tool.hosted_kind.is_none() {
            bail!(
                "{provider_label} tool '{}' is missing hosted tool metadata",
                tool.name
            );
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

pub(crate) fn tool_definitions_to_responses_api(tools: &[ToolDefinition]) -> Result<Vec<Value>> {
    tools.iter().map(responses_api_tool_definition).collect()
}

pub(crate) fn responses_api_tool_definition(tool: &ToolDefinition) -> Result<Value> {
    match tool.backend {
        ToolBackend::LocalFunction => Ok(json!({
            "type": "function",
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
            "strict": tool.strict_schema,
        })),
        ToolBackend::ProviderBuiltin | ToolBackend::ProviderProtocol => {
            let hosted_kind = tool.hosted_kind.ok_or_else(|| {
                anyhow!(
                    "Responses tool '{}' is missing a hosted tool kind for backend {:?}",
                    tool.name,
                    tool.backend
                )
            })?;
            Ok(json!({
                "type": responses_api_hosted_tool_type(hosted_kind),
            }))
        }
    }
}

pub(crate) fn responses_api_hosted_tool_type(hosted_kind: HostedToolKind) -> &'static str {
    match hosted_kind {
        HostedToolKind::WebSearch => "web_search",
        HostedToolKind::FileSearch => "file_search",
        HostedToolKind::ImageGeneration => "image_generation",
        HostedToolKind::CodeInterpreter => "code_interpreter",
        HostedToolKind::ComputerUse => "computer_use",
        HostedToolKind::RemoteMcp => "remote_mcp",
        HostedToolKind::ToolSearch => "tool_search",
        HostedToolKind::Shell => "shell",
        HostedToolKind::ApplyPatch => "apply_patch",
        HostedToolKind::LocalShell => "local_shell",
        HostedToolKind::Skills => "skills",
    }
}

pub(crate) fn responses_tool_backend(item_type: &str) -> (ToolBackend, Option<HostedToolKind>) {
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

pub(crate) fn extract_chatgpt_codex_item_text(value: &Value) -> String {
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

pub(crate) fn parse_argument_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "{}".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn parse_arguments_to_value(arguments: &str) -> Result<Value> {
    let trimmed = arguments.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed)
        .with_context(|| format!("failed to parse tool arguments as JSON: {trimmed}"))
}
