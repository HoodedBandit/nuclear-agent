use super::*;

pub(super) fn dynamic_tool_definition(
    name: &str,
    description: &str,
    input_schema_json: &str,
) -> Option<ToolDefinition> {
    let Ok(input_schema) = serde_json::from_str::<Value>(input_schema_json) else {
        return None;
    };
    Some(super::tool(name, description, input_schema))
}

pub(super) async fn execute_dynamic_tool(
    context: &ToolContext,
    tool_name: &str,
    args: &Value,
) -> Result<String> {
    if let Some(server) = context
        .mcp_servers
        .iter()
        .find(|server| server.enabled && server.tool_name == tool_name)
    {
        return run_external_tool(
            &server.command,
            &server.args,
            server.cwd.as_deref(),
            args,
            &context.cwd,
        )
        .await;
    }

    if let Some(connector) = context
        .app_connectors
        .iter()
        .find(|connector| connector.enabled && connector.tool_name == tool_name)
    {
        return run_external_tool(
            &connector.command,
            &connector.args,
            connector.cwd.as_deref(),
            args,
            &context.cwd,
        )
        .await;
    }

    bail!("unknown tool '{tool_name}'")
}

async fn run_external_tool(
    command: &str,
    args: &[String],
    declared_cwd: Option<&Path>,
    payload: &Value,
    default_cwd: &Path,
) -> Result<String> {
    let mut process = Command::new(command);
    process.kill_on_drop(true);
    process.args(args);
    process.current_dir(declared_cwd.unwrap_or(default_cwd));
    process.stdin(std::process::Stdio::piped());
    process.stdout(std::process::Stdio::piped());
    process.stderr(std::process::Stdio::piped());

    let mut child = process
        .spawn()
        .with_context(|| format!("failed to start external tool '{}'", command))?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt as _;
        let body = serde_json::to_vec(payload)?;
        stdin
            .write_all(&body)
            .await
            .with_context(|| format!("failed to write payload to '{}'", command))?;
    }

    let output = timeout(
        Duration::from_secs(DEFAULT_SHELL_TIMEOUT_SECS),
        child.wait_with_output(),
    )
    .await
    .context("external tool timed out")?
    .with_context(|| format!("failed while waiting for '{}'", command))?;
    let mut text = String::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        text.push_str(stdout.trim_end());
    }
    if !stderr.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(stderr.trim_end());
    }
    if text.is_empty() {
        text = format!("exit={}", output.status);
    } else if !output.status.success() {
        text.push_str(&format!("\nexit={}", output.status));
    }
    Ok(super::truncate(&text, MAX_COMMAND_OUTPUT_BYTES))
}
