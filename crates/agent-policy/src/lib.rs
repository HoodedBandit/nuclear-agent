use std::path::{Path, PathBuf};

use agent_core::{AutonomyMode, AutonomyProfile, AutonomyState, PermissionPreset, TrustPolicy};

pub fn autonomy_warning() -> &'static str {
    "Free thinking mode gives the agent autonomous command execution, file editing, unlimited subagent spawning, and full network access. It can break the system and it can consume API bandwidth aggressively."
}

pub fn trust_summary(policy: &TrustPolicy) -> String {
    format!(
        "trusted_paths={}, shell={}, network={}, full_disk={}, self_edit={}",
        policy.trusted_paths.len(),
        policy.allow_shell,
        policy.allow_network,
        policy.allow_full_disk,
        policy.allow_self_edit
    )
}

pub fn autonomy_summary(state: AutonomyState) -> &'static str {
    match state {
        AutonomyState::Disabled => "disabled",
        AutonomyState::Enabled => "enabled",
        AutonomyState::Paused => "paused",
    }
}

pub fn autonomy_mode_summary(mode: AutonomyMode) -> &'static str {
    match mode {
        AutonomyMode::Assisted => "assisted",
        AutonomyMode::FreeThinking => "free-thinking",
        AutonomyMode::Evolve => "evolve",
    }
}

pub fn permission_summary(preset: PermissionPreset) -> &'static str {
    match preset {
        PermissionPreset::Suggest => "suggest",
        PermissionPreset::AutoEdit => "auto-edit",
        PermissionPreset::FullAuto => "full-auto",
    }
}

pub fn is_high_risk(policy: &TrustPolicy) -> bool {
    policy.allow_full_disk || policy.allow_self_edit || policy.allow_network
}

pub fn allow_shell(policy: &TrustPolicy, autonomy: &AutonomyProfile) -> bool {
    policy.allow_shell
        || (autonomy.state == AutonomyState::Enabled
            && !matches!(autonomy.mode, AutonomyMode::Assisted))
}

pub fn allow_network(policy: &TrustPolicy, autonomy: &AutonomyProfile) -> bool {
    policy.allow_network
        || (autonomy.state == AutonomyState::Enabled
            && !matches!(autonomy.mode, AutonomyMode::Assisted)
            && autonomy.full_network)
}

pub fn allow_self_edit(policy: &TrustPolicy, autonomy: &AutonomyProfile) -> bool {
    policy.allow_self_edit
        || (autonomy.state == AutonomyState::Enabled
            && !matches!(autonomy.mode, AutonomyMode::Assisted)
            && autonomy.allow_self_edit)
}

pub fn tool_allowed_by_preset(tool_name: &str, preset: PermissionPreset) -> bool {
    match preset {
        PermissionPreset::Suggest => {
            !is_mutating_tool(tool_name) && !is_shell_tool(tool_name) && !is_network_tool(tool_name)
        }
        PermissionPreset::AutoEdit => !is_shell_tool(tool_name) && !is_network_tool(tool_name),
        PermissionPreset::FullAuto => true,
    }
}

fn is_mutating_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "apply_patch"
            | "write_file"
            | "append_file"
            | "replace_in_file"
            | "make_dir"
            | "copy_path"
            | "move_path"
            | "delete_path"
    )
}

fn is_shell_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "run_shell" | "git_status" | "git_diff" | "git_log" | "git_show"
    )
}

fn is_network_tool(tool_name: &str) -> bool {
    matches!(tool_name, "fetch_url" | "http_request")
}

pub fn path_is_trusted(
    policy: &TrustPolicy,
    autonomy: &AutonomyProfile,
    cwd: &Path,
    path: &Path,
) -> bool {
    if policy.allow_full_disk
        || (autonomy.state == AutonomyState::Enabled
            && !matches!(autonomy.mode, AutonomyMode::Assisted))
    {
        return true;
    }

    is_path_within(path, cwd)
        || policy
            .trusted_paths
            .iter()
            .any(|trusted| is_path_within(path, trusted))
}

fn is_path_within(path: &Path, root: &Path) -> bool {
    let normalized_path = comparable_path(path);
    let normalized_root = comparable_path(root);
    normalized_path == normalized_root || normalized_path.starts_with(&normalized_root)
}

fn comparable_path(path: &Path) -> PathBuf {
    let normalized = path
        .canonicalize()
        .unwrap_or_else(|_| lexical_normalize(path));
    strip_windows_verbatim_prefix(normalized)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    use std::{ffi::OsString, path::Component};

    let mut prefix: Option<OsString> = None;
    let mut has_root = false;
    let mut parts: Vec<OsString> = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => has_root = true,
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(last) if last != ".." => {
                    parts.pop();
                }
                _ if !has_root => parts.push(OsString::from("..")),
                _ => {}
            },
            Component::Normal(part) => parts.push(part.to_os_string()),
        }
    }
    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(std::path::MAIN_SEPARATOR_STR);
    }
    for part in parts {
        normalized.push(part);
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    normalized
}

#[cfg(target_os = "windows")]
fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

#[cfg(not(target_os = "windows"))]
fn strip_windows_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_autonomy_does_not_keep_network_or_self_edit_enabled() {
        let policy = TrustPolicy::default();
        let autonomy = AutonomyProfile {
            state: AutonomyState::Paused,
            mode: AutonomyMode::FreeThinking,
            unlimited_usage: true,
            full_network: true,
            allow_self_edit: true,
            consented_at: None,
        };

        assert!(!allow_network(&policy, &autonomy));
        assert!(!allow_self_edit(&policy, &autonomy));
    }

    #[test]
    fn enabled_autonomy_keeps_network_and_self_edit_enabled() {
        let policy = TrustPolicy::default();
        let autonomy = AutonomyProfile {
            state: AutonomyState::Enabled,
            mode: AutonomyMode::FreeThinking,
            unlimited_usage: true,
            full_network: true,
            allow_self_edit: true,
            consented_at: None,
        };

        assert!(allow_network(&policy, &autonomy));
        assert!(allow_self_edit(&policy, &autonomy));
    }

    #[test]
    fn missing_paths_with_parent_traversal_are_not_treated_as_trusted() {
        let unique = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir()
            .join(format!("autism-policy-test-{unique}"))
            .join("workspace");
        std::fs::create_dir_all(&root).unwrap();

        let outside = root
            .join("..")
            .join("outside")
            .join("newdir")
            .join("file.txt");
        let policy = TrustPolicy::default();
        let autonomy = AutonomyProfile::default();

        assert!(!path_is_trusted(&policy, &autonomy, &root, &outside));

        std::fs::remove_dir_all(root.parent().unwrap()).ok();
    }
}
