use super::argument_helpers::{optional_string, truncate};
use super::path_helpers::resolve_existing_path;
use super::*;

pub(super) fn ensure_connector_admin_allowed(context: &ToolContext) -> Result<()> {
    if context.background && !context.background_self_edit_allowed {
        bail!("connector configuration is disabled for background runs");
    }
    if matches!(context.permission_preset, PermissionPreset::Suggest)
        && !allow_self_edit(&context.trust_policy, &context.autonomy)
    {
        bail!("connector configuration requires self-edit approval");
    }
    Ok(())
}

pub(super) fn ensure_git_tools_enabled(context: &ToolContext) -> Result<()> {
    if !context.background_shell_allowed || !allow_shell(&context.trust_policy, &context.autonomy) {
        bail!("git tools are disabled because shell execution is disabled");
    }
    Ok(())
}

pub(super) fn git_target(context: &ToolContext, args: &Value) -> Result<(PathBuf, Option<String>)> {
    let Some(path) = optional_string(args, "path") else {
        return Ok((context.cwd.clone(), None));
    };
    let resolved = resolve_existing_path(context, Some(path))?;
    if resolved.is_file() {
        let workdir = resolved
            .parent()
            .ok_or_else(|| anyhow!("file '{}' has no parent directory", resolved.display()))?
            .to_path_buf();
        let filter = resolved
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("file '{}' has no file name", resolved.display()))?;
        Ok((workdir, Some(filter)))
    } else {
        Ok((resolved, None))
    }
}

pub(super) async fn run_git_command(workdir: &Path, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    command.kill_on_drop(true);
    let output = timeout(
        Duration::from_secs(DEFAULT_GIT_TIMEOUT_SECS),
        command.args(args).current_dir(workdir).output(),
    )
    .await
    .with_context(|| {
        format!(
            "git command timed out in {} after {}s",
            workdir.display(),
            DEFAULT_GIT_TIMEOUT_SECS
        )
    })?
    .with_context(|| format!("failed to execute git in {}", workdir.display()))?;
    let stdout = truncate(
        String::from_utf8_lossy(&output.stdout).trim(),
        MAX_COMMAND_OUTPUT_BYTES,
    );
    let stderr = truncate(
        String::from_utf8_lossy(&output.stderr).trim(),
        MAX_COMMAND_OUTPUT_BYTES,
    );

    if !output.status.success() {
        bail!(
            "git exited with code {}.\nstdout:\n{}\n\nstderr:\n{}",
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
        );
    }

    Ok(if stdout.is_empty() {
        "(empty)".to_string()
    } else {
        stdout
    })
}
