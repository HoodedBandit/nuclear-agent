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

    if let Some(plugin_tool) = context
        .plugin_tools
        .iter()
        .find(|plugin_tool| plugin_tool.tool_name == tool_name)
    {
        return run_hosted_plugin_tool(context, plugin_tool, args).await;
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

async fn run_hosted_plugin_tool(
    context: &ToolContext,
    plugin_tool: &crate::HostedPluginTool,
    payload: &Value,
) -> Result<String> {
    ensure_plugin_permissions(context, plugin_tool)?;

    let request = agent_core::PluginToolCallRequest {
        host_version: agent_core::PLUGIN_HOST_VERSION,
        plugin_id: plugin_tool.plugin_id.clone(),
        plugin_name: plugin_tool.plugin_name.clone(),
        plugin_version: plugin_tool.plugin_version.clone(),
        tool_name: plugin_tool.tool_name.clone(),
        workspace_cwd: context.cwd.clone(),
        arguments: payload.clone(),
        shell_allowed: context.background_shell_allowed
            && allow_shell(&context.trust_policy, &context.autonomy),
        network_allowed: context.background_network_allowed
            && allow_network(&context.trust_policy, &context.autonomy),
        full_disk_allowed: context.trust_policy.allow_full_disk,
    };

    let mut process = Command::new(&plugin_tool.command);
    process.kill_on_drop(true);
    process.args(&plugin_tool.args);
    process.current_dir(
        plugin_tool
            .cwd
            .as_deref()
            .unwrap_or(plugin_tool.install_dir.as_path()),
    );
    process.stdin(std::process::Stdio::piped());
    process.stdout(std::process::Stdio::piped());
    process.stderr(std::process::Stdio::piped());
    process.env("AGENT_PLUGIN_ID", &plugin_tool.plugin_id);
    process.env("AGENT_PLUGIN_NAME", &plugin_tool.plugin_name);
    process.env("AGENT_PLUGIN_VERSION", &plugin_tool.plugin_version);
    process.env(
        "AGENT_PLUGIN_HOST_VERSION",
        agent_core::PLUGIN_HOST_VERSION.to_string(),
    );
    process.env("AGENT_PLUGIN_TOOL_NAME", &plugin_tool.tool_name);

    let mut child = process.spawn().with_context(|| {
        format!(
            "failed to start hosted plugin tool '{}' from plugin '{}'",
            plugin_tool.tool_name, plugin_tool.plugin_id
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt as _;
        let body = serde_json::to_vec(&request)?;
        stdin.write_all(&body).await.with_context(|| {
            format!(
                "failed to write plugin request to '{}'",
                plugin_tool.command
            )
        })?;
    }

    let timeout_seconds = plugin_tool
        .timeout_seconds
        .unwrap_or(DEFAULT_SHELL_TIMEOUT_SECS)
        .clamp(1, MAX_SHELL_TIMEOUT_SECS);
    let output = timeout(
        Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .context("hosted plugin tool timed out")?
    .with_context(|| {
        format!(
            "failed while waiting for hosted plugin tool '{}'",
            plugin_tool.tool_name
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Ok(response) = serde_json::from_str::<agent_core::PluginToolCallResponse>(stdout.trim())
    {
        if response.ok && output.status.success() {
            return Ok(super::truncate(&response.content, MAX_COMMAND_OUTPUT_BYTES));
        }
        bail!(
            "plugin '{}' tool '{}' failed: {}",
            plugin_tool.plugin_id,
            plugin_tool.tool_name,
            response.content
        );
    }

    let mut text = String::new();
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

fn ensure_plugin_permissions(
    context: &ToolContext,
    plugin_tool: &crate::HostedPluginTool,
) -> Result<()> {
    if plugin_tool.permissions.shell
        && !(context.background_shell_allowed
            && allow_shell(&context.trust_policy, &context.autonomy))
    {
        bail!(
            "plugin '{}' tool '{}' requires shell permission",
            plugin_tool.plugin_id,
            plugin_tool.tool_name
        );
    }
    if plugin_tool.permissions.network
        && !(context.background_network_allowed
            && allow_network(&context.trust_policy, &context.autonomy))
    {
        bail!(
            "plugin '{}' tool '{}' requires network permission",
            plugin_tool.plugin_id,
            plugin_tool.tool_name
        );
    }
    if plugin_tool.permissions.full_disk && !context.trust_policy.allow_full_disk {
        bail!(
            "plugin '{}' tool '{}' requires full-disk permission",
            plugin_tool.plugin_id,
            plugin_tool.tool_name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc};

    use agent_core::{
        AppConfig, AutonomyProfile, DelegationConfig, PermissionPreset, PluginPermissions,
        TrustPolicy,
    };
    use reqwest::Client;
    use serde_json::json;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, AppState, HostedPluginTool, ProviderRateLimiter,
    };

    use super::*;

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("agent-dynamic-plugin-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn test_state() -> AppState {
        let storage = agent_storage::Storage::open_at(
            std::env::temp_dir().join(format!("agent-dynamic-tools-test-{}", Uuid::new_v4())),
        )
        .unwrap();
        let (shutdown_tx, _) = mpsc::unbounded_channel();
        AppState {
            storage,
            config: Arc::new(RwLock::new(AppConfig::default())),
            http_client: Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: chrono::Utc::now(),
            shutdown: shutdown_tx,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    fn test_context(root: &std::path::Path) -> ToolContext {
        ToolContext {
            state: test_state(),
            cwd: root.to_path_buf(),
            trust_policy: TrustPolicy {
                trusted_paths: vec![root.to_path_buf()],
                allow_shell: true,
                allow_network: true,
                allow_full_disk: false,
                allow_self_edit: false,
            },
            autonomy: AutonomyProfile::default(),
            permission_preset: PermissionPreset::FullAuto,
            http_client: Client::new(),
            mcp_servers: Vec::new(),
            app_connectors: Vec::new(),
            plugin_tools: Vec::new(),
            brave_connectors: Vec::new(),
            current_alias: Some("main".to_string()),
            default_thinking_level: None,
            task_mode: None,
            delegation: DelegationConfig::default(),
            delegation_targets: Vec::new(),
            delegation_depth: 0,
            background: false,
            background_shell_allowed: true,
            background_network_allowed: true,
            background_self_edit_allowed: true,
        }
    }

    fn hosted_plugin_tool(
        root: &std::path::Path,
        command: String,
        args: Vec<String>,
        permissions: PluginPermissions,
    ) -> HostedPluginTool {
        HostedPluginTool {
            plugin_id: "echo-toolkit".to_string(),
            plugin_name: "Echo Toolkit".to_string(),
            plugin_version: "0.8.0".to_string(),
            install_dir: root.to_path_buf(),
            command,
            args,
            tool_name: "echo_tool".to_string(),
            description: "Echo tool".to_string(),
            input_schema_json: "{\"type\":\"object\"}".to_string(),
            cwd: Some(root.to_path_buf()),
            permissions,
            timeout_seconds: Some(5),
        }
    }

    fn protocol_script(root: &std::path::Path) -> (String, Vec<String>) {
        if cfg!(windows) {
            let script = root.join("plugin.ps1");
            fs::write(
                &script,
                "$null = [Console]::In.ReadToEnd()\n[Console]::Out.Write('{\"ok\":true,\"content\":\"plugin-ok\"}')\n",
            )
            .unwrap();
            (
                "powershell".to_string(),
                vec![
                    "-NoLogo".to_string(),
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    script.display().to_string(),
                ],
            )
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                let script = root.join("plugin.sh");
                fs::write(
                    &script,
                    "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{\"ok\":true,\"content\":\"plugin-ok\"}'\n",
                )
                .unwrap();
                let mut permissions = fs::metadata(&script).unwrap().permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&script, permissions).unwrap();
                ("sh".to_string(), vec![script.display().to_string()])
            }
            #[cfg(not(unix))]
            unreachable!()
        }
    }

    #[tokio::test]
    async fn hosted_plugin_tool_respects_permission_gates() {
        let root = temp_root();
        let mut context = test_context(&root);
        context.background_shell_allowed = false;
        let plugin_tool = hosted_plugin_tool(
            &root,
            "missing-command".to_string(),
            Vec::new(),
            PluginPermissions {
                shell: true,
                network: false,
                full_disk: false,
            },
        );

        let error = run_hosted_plugin_tool(&context, &plugin_tool, &json!({}))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("requires shell permission"));
    }

    #[tokio::test]
    async fn hosted_plugin_tool_parses_protocol_response() {
        let root = temp_root();
        let context = test_context(&root);
        let (command, args) = protocol_script(&root);
        let plugin_tool = hosted_plugin_tool(&root, command, args, PluginPermissions::default());

        let output = run_hosted_plugin_tool(&context, &plugin_tool, &json!({ "text": "hello" }))
            .await
            .unwrap();

        assert_eq!(output, "plugin-ok");
    }
}
