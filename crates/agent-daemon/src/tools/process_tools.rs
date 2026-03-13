use super::admin_helpers::{ensure_git_tools_enabled, git_target, run_git_command};
use super::argument_helpers::{
    is_sensitive_env_var, optional_bool, optional_string, optional_u64, required_string, truncate,
};
use super::path_helpers::resolve_existing_path;
use super::*;

pub(super) async fn run_shell(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_shell_allowed || !allow_shell(&context.trust_policy, &context.autonomy) {
        bail!("shell execution is disabled by trust policy");
    }

    let command_text = required_string(args, "command")?;
    let workdir = match optional_string(args, "workdir") {
        Some(path) => resolve_existing_path(context, Some(path))?,
        None => context.cwd.clone(),
    };
    let timeout_seconds = optional_u64(args, "timeout_seconds")
        .unwrap_or(DEFAULT_SHELL_TIMEOUT_SECS)
        .clamp(1, MAX_SHELL_TIMEOUT_SECS);

    let mut command = shell_command(command_text);
    command.kill_on_drop(true);
    command.current_dir(&workdir);
    let output = timeout(Duration::from_secs(timeout_seconds), command.output())
        .await
        .context("shell command timed out")?
        .with_context(|| format!("failed to execute shell command in {}", workdir.display()))?;

    let stdout = truncate(
        String::from_utf8_lossy(&output.stdout).trim(),
        MAX_COMMAND_OUTPUT_BYTES,
    );
    let stderr = truncate(
        String::from_utf8_lossy(&output.stderr).trim(),
        MAX_COMMAND_OUTPUT_BYTES,
    );
    Ok(format!(
        "exit_code={}\nstdout:\n{}\n\nstderr:\n{}",
        output.status.code().unwrap_or(-1),
        if stdout.is_empty() {
            "(empty)"
        } else {
            &stdout
        },
        if stderr.is_empty() {
            "(empty)"
        } else {
            &stderr
        }
    ))
}

pub(super) fn read_env(args: &Value) -> Result<String> {
    let name = required_string(args, "name")?;
    if is_sensitive_env_var(name) {
        bail!("reading sensitive environment variables is not allowed");
    }

    match std::env::var(name) {
        Ok(value) => Ok(format!(
            "{name}={}",
            truncate(&value, MAX_COMMAND_OUTPUT_BYTES)
        )),
        Err(std::env::VarError::NotPresent) => Ok(format!("{name} is not set")),
        Err(std::env::VarError::NotUnicode(_)) => Ok(format!("{name} is set but not valid UTF-8")),
    }
}

pub(super) async fn git_status(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_git_tools_enabled(context)?;
    let (workdir, _) = git_target(context, args)?;
    run_git_command(&workdir, &["status", "--short", "--branch"]).await
}

pub(super) async fn git_diff(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_git_tools_enabled(context)?;
    let (workdir, filter) = git_target(context, args)?;
    let mut command = vec!["diff"];
    if optional_bool(args, "staged").unwrap_or(false) {
        command.push("--staged");
    }
    if let Some(revision) = optional_string(args, "revision") {
        command.push(revision);
    }
    if let Some(filter) = filter.as_deref() {
        command.push("--");
        command.push(filter);
    }
    run_git_command(&workdir, &command).await
}

pub(super) async fn git_log(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_git_tools_enabled(context)?;
    let (workdir, _) = git_target(context, args)?;
    let limit = optional_u64(args, "limit")
        .unwrap_or(10)
        .clamp(1, MAX_GIT_LOG_ENTRIES as u64);
    let limit_arg = format!("-n{limit}");
    run_git_command(
        &workdir,
        &[
            "log",
            &limit_arg,
            "--date=iso",
            "--pretty=format:%H %ad %an %s",
        ],
    )
    .await
}

pub(super) async fn git_show(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_git_tools_enabled(context)?;
    let (workdir, _) = git_target(context, args)?;
    let revision = required_string(args, "revision")?;
    run_git_command(&workdir, &["show", "--stat", "--patch", revision]).await
}

pub(super) async fn fetch_url(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }

    let url = required_string(args, "url")?;
    let response = context
        .http_client
        .get(url)
        .send()
        .await
        .context("failed to fetch URL")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    let truncated = truncate(&body, MAX_FETCH_BYTES);
    Ok(format!("status={status}\nbody:\n{truncated}"))
}

pub(super) async fn http_request(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }

    let url = required_string(args, "url")?;
    let method = optional_string(args, "method").unwrap_or("GET");
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .with_context(|| format!("unsupported HTTP method '{method}'"))?;
    let max_bytes = optional_u64(args, "max_bytes")
        .unwrap_or(MAX_HTTP_BODY_BYTES as u64)
        .min(MAX_HTTP_BODY_BYTES as u64) as usize;

    let mut request = context.http_client.request(method, url);
    if let Some(headers) = args.get("headers").and_then(Value::as_object) {
        for (name, value) in headers {
            let header_value = value
                .as_str()
                .ok_or_else(|| anyhow!("HTTP header '{name}' must be a string"))?;
            request = request.header(name, header_value);
        }
    }
    if let Some(body) = optional_string(args, "body") {
        request = request.body(body.to_string());
    }

    let response = request
        .send()
        .await
        .context("failed to execute HTTP request")?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    let rendered_headers = headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| format!("{name}: {value}")))
        .collect::<Vec<_>>()
        .join("\n");
    let truncated_body = truncate(&body, max_bytes);

    Ok(format!(
        "status={status}\nheaders:\n{}\n\nbody:\n{}",
        if rendered_headers.is_empty() {
            "(empty)"
        } else {
            &rendered_headers
        },
        truncated_body
    ))
}
