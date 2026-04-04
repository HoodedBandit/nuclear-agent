use agent_core::{
    HostedToolKind, MessageRole, ProviderOutputItem, ToolBackend, ToolCall, ToolDefinition,
};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use super::chatgpt_codex_models as codex_models;
use super::common::parse_argument_string;

pub(super) fn validate_tool_definitions(
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

pub(super) fn tool_definitions_to_openai(tools: &[ToolDefinition]) -> Vec<Value> {
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

pub(super) fn tool_definitions_to_responses_api(tools: &[ToolDefinition]) -> Result<Vec<Value>> {
    tools.iter().map(responses_api_tool_definition).collect()
}

fn responses_api_tool_definition(tool: &ToolDefinition) -> Result<Value> {
    match tool.backend {
        ToolBackend::LocalFunction => {
            let parameters = if tool.strict_schema {
                agent_core::responses_strict_json_schema(&tool.input_schema)?
            } else {
                tool.input_schema.clone()
            };
            Ok(json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": parameters,
                "strict": tool.strict_schema,
            }))
        }
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

pub(super) fn responses_api_hosted_tool_type(hosted_kind: HostedToolKind) -> &'static str {
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

pub(super) fn tool_definitions_to_anthropic(tools: &[ToolDefinition]) -> Vec<Value> {
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

pub(super) fn parse_openai_tool_calls(message: &Value) -> Result<Vec<ToolCall>> {
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

pub(super) fn parse_chatgpt_codex_tool_call(value: &Value) -> Result<Option<ToolCall>> {
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

pub(super) fn parse_chatgpt_codex_output_item(value: &Value) -> Option<ProviderOutputItem> {
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
            let (backend, hosted_kind) = codex_models::responses_tool_backend(other);
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

pub(super) fn openai_output_items(
    content: &str,
    tool_calls: &[ToolCall],
) -> Vec<ProviderOutputItem> {
    let mut items = Vec::new();
    if !content.is_empty() {
        items.push(ProviderOutputItem::Message {
            role: MessageRole::Assistant,
            content: content.to_string(),
        });
    }
    items.extend(
        tool_calls
            .iter()
            .cloned()
            .map(|call| ProviderOutputItem::FunctionCall { call }),
    );
    items
}

pub(super) fn anthropic_output_items(content_blocks: &[Value]) -> Vec<ProviderOutputItem> {
    let mut items = Vec::new();
    for block in content_blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    items.push(ProviderOutputItem::Message {
                        role: MessageRole::Assistant,
                        content: text.to_string(),
                    });
                }
            }
            Some("tool_use") => {
                if let Ok(call) = parse_anthropic_tool_call(block) {
                    items.push(ProviderOutputItem::FunctionCall { call });
                }
            }
            _ => {}
        }
    }
    items
}

pub(super) fn parse_anthropic_tool_call(value: &Value) -> Result<ToolCall> {
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

pub(super) fn parse_ollama_tool_calls(message: &Value) -> Result<Vec<ToolCall>> {
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::tool_definitions_to_responses_api;
    use agent_core::{ToolBackend, ToolDefinition};

    #[test]
    fn responses_api_tool_definitions_normalize_strict_function_schemas() {
        let tools = vec![ToolDefinition {
            name: "list_dir".to_string(),
            description: "List a directory".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_entries": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            backend: ToolBackend::LocalFunction,
            hosted_kind: None,
            strict_schema: true,
        }];

        let payload = tool_definitions_to_responses_api(&tools).unwrap();

        assert_eq!(payload[0]["strict"], true);
        let required = payload[0]["parameters"]["required"]
            .as_array()
            .expect("required should be an array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"path"));
        assert!(required.contains(&"max_entries"));
        assert_eq!(
            payload[0]["parameters"]["properties"]["max_entries"]["type"],
            json!(["integer", "null"])
        );
    }
}
