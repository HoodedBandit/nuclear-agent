use super::*;

mod brave_tools;
mod home_assistant_tools;
mod messaging;

pub(super) fn tool_definitions(context: &ToolContext) -> Vec<ToolDefinition> {
    let mut tools = messaging::tool_definitions();
    tools.extend(home_assistant_tools::tool_definitions());
    tools.extend(brave_tools::tool_definitions(context));
    tools
}

pub(super) async fn execute_tool_call(
    context: &ToolContext,
    tool_name: &str,
    args: &Value,
) -> Result<Option<String>> {
    if let Some(output) = messaging::execute_tool_call(context, tool_name, args).await? {
        return Ok(Some(output));
    }
    if let Some(output) = brave_tools::execute_tool_call(context, tool_name, args).await? {
        return Ok(Some(output));
    }
    home_assistant_tools::execute_tool_call(context, tool_name, args).await
}

fn sanitize_connector_id(value: String, fallback: &str) -> String {
    let mut cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while cleaned.contains("--") {
        cleaned = cleaned.replace("--", "-");
    }
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

fn ensure_connector_enabled_tool(
    enabled: bool,
    kind: &str,
    connector_id: &str,
    action: &str,
) -> Result<()> {
    if enabled {
        Ok(())
    } else {
        bail!("{kind} connector '{connector_id}' is disabled for {action}");
    }
}
