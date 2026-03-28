use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use agent_core::{
    AutonomyProfile, TrustPolicy, WorkspaceFileStat, WorkspaceInspectRequest,
    WorkspaceInspectResponse, WorkspaceLanguageStat, WorkspacePathStat,
};
use agent_policy::{allow_shell, path_is_trusted};
use anyhow::{anyhow, Context, Result};
use axum::http::StatusCode;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tokio::{process::Command as TokioCommand, time::timeout};

use crate::{ApiError, AppState};

const MAX_SCAN_DEPTH: usize = 4;
const MAX_MANIFESTS: usize = 12;
const MAX_LARGE_FILES: usize = 8;
const MAX_RECENT_COMMITS: usize = 5;
const MAX_LARGE_FILE_BYTES: u64 = 512 * 1024;
const MAX_GIT_CAPTURE_BYTES: usize = 120_000;
const SHELL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceActionRequest {
    #[serde(default)]
    pub(crate) cwd: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceDiffResponse {
    pub(crate) cwd: String,
    pub(crate) git_root: String,
    pub(crate) diff: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceInitResponse {
    pub(crate) cwd: String,
    pub(crate) path: String,
    pub(crate) created: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceShellRequest {
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) cwd: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WorkspaceShellResponse {
    pub(crate) cwd: String,
    pub(crate) output: String,
}

pub(crate) async fn inspect_workspace_route(
    State(_state): State<AppState>,
    Json(payload): Json<WorkspaceInspectRequest>,
) -> Result<Json<WorkspaceInspectResponse>, ApiError> {
    inspect_workspace_path(payload.path.as_deref())
        .map(Json)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}

pub(crate) async fn workspace_diff_route(
    State(_state): State<AppState>,
    Json(payload): Json<WorkspaceActionRequest>,
) -> Result<Json<WorkspaceDiffResponse>, ApiError> {
    build_uncommitted_diff(payload.cwd)
        .map(Json)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}

pub(crate) async fn workspace_init_agents_route(
    State(_state): State<AppState>,
    Json(payload): Json<WorkspaceActionRequest>,
) -> Result<Json<WorkspaceInitResponse>, ApiError> {
    initialize_agents_file(payload.cwd)
        .map(Json)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}

pub(crate) async fn workspace_shell_route(
    State(state): State<AppState>,
    Json(payload): Json<WorkspaceShellRequest>,
) -> Result<Json<WorkspaceShellResponse>, ApiError> {
    execute_workspace_shell(&state, payload)
        .await
        .map(Json)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}

pub fn inspect_workspace_path(path: Option<&str>) -> Result<WorkspaceInspectResponse> {
    let requested = resolve_workspace_request(path)?;
    let git_root = git_root_for(&requested);
    let workspace_root = git_root.clone().unwrap_or_else(|| requested.clone());

    let mut manifests = Vec::new();
    let mut language_counts = BTreeMap::<String, usize>::new();
    let mut focus_counts = BTreeMap::<String, usize>::new();
    let mut large_files = Vec::<WorkspaceFileStat>::new();

    scan_workspace(
        &workspace_root,
        &workspace_root,
        0,
        &mut manifests,
        &mut language_counts,
        &mut focus_counts,
        &mut large_files,
    )?;

    manifests.sort();
    manifests.truncate(MAX_MANIFESTS);

    large_files.sort_by(|left, right| {
        right
            .lines
            .cmp(&left.lines)
            .then_with(|| left.path.cmp(&right.path))
    });
    large_files.truncate(MAX_LARGE_FILES);

    let language_breakdown = language_counts
        .into_iter()
        .map(|(label, files)| WorkspaceLanguageStat { label, files })
        .collect::<Vec<_>>();
    let mut language_breakdown = language_breakdown;
    language_breakdown.sort_by(|left, right| {
        right
            .files
            .cmp(&left.files)
            .then_with(|| left.label.cmp(&right.label))
    });

    let mut focus_paths = focus_counts
        .into_iter()
        .map(|(path, source_files)| WorkspacePathStat { path, source_files })
        .collect::<Vec<_>>();
    focus_paths.sort_by(|left, right| {
        right
            .source_files
            .cmp(&left.source_files)
            .then_with(|| left.path.cmp(&right.path))
    });
    focus_paths.truncate(8);

    let git_branch = git_root
        .as_ref()
        .and_then(|root| git_line(root, &["rev-parse", "--abbrev-ref", "HEAD"]));
    let git_commit = git_root
        .as_ref()
        .and_then(|root| git_line(root, &["rev-parse", "--short", "HEAD"]));
    let (staged_files, dirty_files, untracked_files) = git_root
        .as_ref()
        .map(|root| git_status_counts(root))
        .unwrap_or((0, 0, 0));
    let recent_commits = git_root
        .as_ref()
        .map(|root| git_lines(root, &["log", "--oneline", "-n", "5"]))
        .unwrap_or_default()
        .into_iter()
        .take(MAX_RECENT_COMMITS)
        .collect::<Vec<_>>();

    let mut notes = Vec::new();
    if manifests.is_empty() {
        notes.push(
            "No common project manifest files were found near the workspace root.".to_string(),
        );
    }
    if staged_files > 0 || dirty_files > 0 || untracked_files > 0 {
        notes.push(format!(
            "Git worktree has {staged_files} staged, {dirty_files} modified, and {untracked_files} untracked file(s)."
        ));
    }
    if let Some(file) = large_files.first() {
        if file.lines >= 2_000 {
            notes.push(format!(
                "Largest source file is {} ({} lines); that is a likely refactor hotspot.",
                file.path, file.lines
            ));
        }
    }
    if language_breakdown.is_empty() {
        notes.push("No recognized source files were found in the scanned workspace.".to_string());
    }

    Ok(WorkspaceInspectResponse {
        requested_path: display_path(&requested),
        workspace_root: display_path(&workspace_root),
        git_root: git_root.as_deref().map(display_path),
        git_branch,
        git_commit,
        staged_files,
        dirty_files,
        untracked_files,
        manifests,
        focus_paths,
        language_breakdown,
        large_source_files: large_files,
        recent_commits,
        notes,
    })
}

fn build_uncommitted_diff(cwd: Option<PathBuf>) -> Result<WorkspaceDiffResponse> {
    let cwd = crate::resolve_request_cwd(cwd).map_err(|error| anyhow!(error.message))?;
    let git_root = git_root_for(&cwd)
        .ok_or_else(|| anyhow!("no git repository found for '{}'", cwd.display()))?;
    let staged = git_capture(
        &git_root,
        &["diff", "--no-ext-diff", "--cached"],
        MAX_GIT_CAPTURE_BYTES,
    )
    .unwrap_or_else(|_| String::new());
    let unstaged = git_capture(&git_root, &["diff", "--no-ext-diff"], MAX_GIT_CAPTURE_BYTES)
        .unwrap_or_else(|_| String::new());
    let untracked = git_capture(
        &git_root,
        &["ls-files", "--others", "--exclude-standard"],
        10_000,
    )
    .unwrap_or_else(|_| String::new());

    let diff = truncate_text(
        format!(
            "Staged changes:\n{staged}\n\nUnstaged changes:\n{unstaged}\n\nUntracked files:\n{untracked}"
        ),
        MAX_GIT_CAPTURE_BYTES,
    );
    if diff.trim().is_empty() {
        return Err(anyhow!("no reviewable git changes found"));
    }

    Ok(WorkspaceDiffResponse {
        cwd: display_path(&cwd),
        git_root: display_path(&git_root),
        diff,
    })
}

fn initialize_agents_file(cwd: Option<PathBuf>) -> Result<WorkspaceInitResponse> {
    let cwd = crate::resolve_request_cwd(cwd).map_err(|error| anyhow!(error.message))?;
    let path = cwd.join("AGENTS.md");
    let created = init_agents_file(&path)?;
    Ok(WorkspaceInitResponse {
        cwd: display_path(&cwd),
        path: display_path(&path),
        created,
    })
}

async fn execute_workspace_shell(
    state: &AppState,
    payload: WorkspaceShellRequest,
) -> Result<WorkspaceShellResponse> {
    let mut cwd =
        crate::resolve_request_cwd(payload.cwd).map_err(|error| anyhow!(error.message))?;
    let command = payload.command.trim();
    if command.is_empty() {
        return Err(anyhow!("shell command is empty"));
    }

    let config = state.config.read().await.clone();
    if !allow_shell(&config.trust_policy, &config.autonomy) {
        return Err(anyhow!(
            "shell access is disabled by the current trust policy"
        ));
    }
    ensure_workspace_shell_cwd_trusted(&config.trust_policy, &config.autonomy, &cwd)?;

    if command == "cd" {
        cwd = home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        ensure_workspace_shell_cwd_trusted(&config.trust_policy, &config.autonomy, &cwd)?;
        return Ok(WorkspaceShellResponse {
            cwd: display_path(&cwd),
            output: format!("cwd={}", cwd.display()),
        });
    }

    if let Some(target) = command.strip_prefix("cd ") {
        let target = target.trim();
        if target.is_empty() {
            return Err(anyhow!("cd target is empty"));
        }
        cwd = resolve_shell_cd_target(&cwd, target)?;
        ensure_workspace_shell_cwd_trusted(&config.trust_policy, &config.autonomy, &cwd)?;
        return Ok(WorkspaceShellResponse {
            cwd: display_path(&cwd),
            output: format!("cwd={}", cwd.display()),
        });
    }

    let output = execute_local_shell_command(command, &cwd).await?;
    Ok(WorkspaceShellResponse {
        cwd: display_path(&cwd),
        output,
    })
}

fn resolve_workspace_request(path: Option<&str>) -> Result<PathBuf> {
    let raw = path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let resolved = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(raw)
    };
    fs::canonicalize(&resolved).map_err(|error| {
        anyhow!(
            "failed to resolve workspace path '{}': {error}",
            resolved.display()
        )
    })
}

fn display_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.display().to_string();
        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{}", stripped);
        }
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return stripped.to_string();
        }
        text
    }
    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

fn scan_workspace(
    root: &Path,
    current: &Path,
    depth: usize,
    manifests: &mut Vec<String>,
    language_counts: &mut BTreeMap<String, usize>,
    focus_counts: &mut BTreeMap<String, usize>,
    large_files: &mut Vec<WorkspaceFileStat>,
) -> Result<()> {
    if depth > MAX_SCAN_DEPTH {
        return Ok(());
    }

    let entries = fs::read_dir(current).map_err(|error| {
        anyhow!(
            "failed to scan workspace directory '{}': {error}",
            current.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            anyhow!(
                "failed to read workspace directory entry in '{}': {error}",
                current.display()
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            anyhow!(
                "failed to inspect workspace path '{}': {error}",
                path.display()
            )
        })?;

        if file_type.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            scan_workspace(
                root,
                &path,
                depth + 1,
                manifests,
                language_counts,
                focus_counts,
                large_files,
            )?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.clone());
        let relative_text = relative.display().to_string();

        if is_manifest_name(&path) {
            manifests.push(relative_text.clone());
        }

        let Some(label) = language_label(&path) else {
            continue;
        };
        *language_counts.entry(label.to_string()).or_default() += 1;

        let focus_key = relative
            .components()
            .next()
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .filter(|component| !component.is_empty())
            .unwrap_or_else(|| ".".to_string());
        *focus_counts.entry(focus_key).or_default() += 1;

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() > MAX_LARGE_FILE_BYTES {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            let lines = content.lines().count();
            large_files.push(WorkspaceFileStat {
                path: relative_text,
                lines,
            });
        }
    }

    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | "target"
            | "node_modules"
            | "dist"
            | "dist-test"
            | ".next"
            | ".turbo"
            | "coverage"
            | "vendor"
    )
}

fn is_manifest_name(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|value| value.to_str()),
        Some("Cargo.toml")
            | Some("package.json")
            | Some("pyproject.toml")
            | Some("go.mod")
            | Some("README.md")
            | Some("AGENTS.md")
    )
}

fn language_label(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|value| value.to_str()) {
        Some("rs") => Some("Rust"),
        Some("js") | Some("cjs") | Some("mjs") => Some("JavaScript"),
        Some("ts") | Some("tsx") => Some("TypeScript"),
        Some("py") => Some("Python"),
        Some("go") => Some("Go"),
        Some("java") => Some("Java"),
        Some("cs") => Some("C#"),
        Some("c") | Some("cc") | Some("cpp") | Some("cxx") | Some("h") | Some("hpp") => {
            Some("C/C++")
        }
        Some("html") => Some("HTML"),
        Some("css") | Some("scss") => Some("CSS"),
        Some("md") => Some("Markdown"),
        _ => None,
    }
}

fn git_root_for(path: &Path) -> Option<PathBuf> {
    git_line(path, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn git_status_counts(root: &Path) -> (usize, usize, usize) {
    let Some(output) = git_stdout(root, &["status", "--short"]) else {
        return (0, 0, 0);
    };

    let mut staged = 0usize;
    let mut dirty = 0usize;
    let mut untracked = 0usize;
    for line in output.lines() {
        let mut chars = line.chars();
        let first = chars.next().unwrap_or(' ');
        let second = chars.next().unwrap_or(' ');
        if first == '?' && second == '?' {
            untracked += 1;
            continue;
        }
        if first != ' ' {
            staged += 1;
        }
        if second != ' ' {
            dirty += 1;
        }
    }
    (staged, dirty, untracked)
}

fn git_line(root: &Path, args: &[&str]) -> Option<String> {
    git_stdout(root, args).and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn git_lines(root: &Path, args: &[&str]) -> Vec<String> {
    git_stdout(root, args)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn git_stdout(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(text)
}

fn git_capture(root: &Path, args: &[&str], max_len: usize) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| anyhow!("failed to run git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(truncate_text(
        String::from_utf8_lossy(&output.stdout).to_string(),
        max_len,
    ))
}

fn truncate_text(mut text: String, max_len: usize) -> String {
    if text.len() <= max_len {
        return text;
    }
    text.truncate(max_len);
    text.push_str("\n\n[truncated]");
    text
}

fn init_agents_file(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, build_agents_template(path.parent()))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

fn build_agents_template(parent: Option<&Path>) -> String {
    let location = parent
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    format!(
        "# AGENTS.md\n\n## Project Guidance\n- Describe what lives under {location}.\n- List the most important build, test, and run commands.\n- Call out code style, review expectations, and risky areas.\n\n## Guardrails\n- Document paths or systems the agent should avoid editing.\n- Note approval expectations for destructive changes.\n\n## Verification\n- List the commands the agent should run before considering work complete.\n"
    )
}

fn resolve_shell_cd_target(current: &Path, target: &str) -> Result<PathBuf> {
    let expanded = if target == "~" || target.starts_with("~/") || target.starts_with("~\\") {
        let home = home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        if target.len() == 1 {
            home
        } else {
            home.join(&target[2..])
        }
    } else {
        PathBuf::from(target)
    };

    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        current.join(expanded)
    };
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("failed to access {}", resolved.display()))?;
    if !canonical.is_dir() {
        return Err(anyhow!("{} is not a directory", canonical.display()));
    }
    Ok(canonical)
}

fn ensure_workspace_shell_cwd_trusted(
    trust_policy: &TrustPolicy,
    autonomy: &AutonomyProfile,
    cwd: &Path,
) -> Result<()> {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ensure_workspace_shell_cwd_trusted_from_root(&root, trust_policy, autonomy, cwd)
}

fn ensure_workspace_shell_cwd_trusted_from_root(
    root: &Path,
    trust_policy: &TrustPolicy,
    autonomy: &AutonomyProfile,
    cwd: &Path,
) -> Result<()> {
    if !path_is_trusted(trust_policy, autonomy, root, cwd) {
        return Err(anyhow!("path '{}' is outside trusted roots", cwd.display()));
    }
    Ok(())
}

async fn execute_local_shell_command(command: &str, cwd: &Path) -> Result<String> {
    let mut process = if cfg!(windows) {
        let mut command_process = TokioCommand::new("powershell");
        command_process
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(command);
        command_process
    } else {
        let mut command_process = TokioCommand::new("sh");
        command_process.arg("-lc").arg(command);
        command_process
    };
    process.kill_on_drop(true);
    process.current_dir(cwd);

    let output = timeout(SHELL_TIMEOUT, process.output())
        .await
        .context("shell command timed out")?
        .with_context(|| format!("failed to run shell command '{command}'"))?;

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
    Ok(truncate_text(text, 20_000))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn inspect_workspace_path_summarizes_source_tree_without_git() {
        let root =
            std::env::temp_dir().join(format!("agent-workspace-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("crates").join("demo").join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
        fs::write(root.join("README.md"), "# demo\n").unwrap();
        let mut main_rs =
            fs::File::create(root.join("crates").join("demo").join("src").join("main.rs")).unwrap();
        writeln!(main_rs, "fn main() {{}}").unwrap();

        let report = inspect_workspace_path(Some(root.to_str().unwrap())).unwrap();

        assert_eq!(report.workspace_root, root.display().to_string());
        assert!(report.git_root.is_none());
        assert!(report.manifests.iter().any(|entry| entry == "Cargo.toml"));
        assert!(report
            .language_breakdown
            .iter()
            .any(|entry| entry.label == "Rust" && entry.files >= 1));
        assert!(report
            .focus_paths
            .iter()
            .any(|entry| entry.path == "crates" && entry.source_files >= 1));
    }

    #[test]
    fn init_agents_file_creates_template_once() {
        let root = std::env::temp_dir().join(format!(
            "agent-workspace-init-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("AGENTS.md");

        assert!(init_agents_file(&path).unwrap());
        assert!(path.exists());
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("## Project Guidance"));
        assert!(!init_agents_file(&path).unwrap());
    }

    #[test]
    fn workspace_shell_trust_rejects_untrusted_cwd() {
        let root = std::env::temp_dir().join(format!(
            "agent-workspace-shell-root-{}",
            uuid::Uuid::new_v4()
        ));
        let trusted = root.join("trusted");
        let outside = root.join("outside");
        fs::create_dir_all(&trusted).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let policy = TrustPolicy::default();
        let autonomy = AutonomyProfile::default();

        assert!(ensure_workspace_shell_cwd_trusted_from_root(
            &trusted, &policy, &autonomy, &trusted
        )
        .is_ok());
        assert!(ensure_workspace_shell_cwd_trusted_from_root(
            &trusted, &policy, &autonomy, &outside
        )
        .is_err());
    }

    #[test]
    fn workspace_shell_trust_allows_explicit_trusted_paths() {
        let root = std::env::temp_dir().join(format!(
            "agent-workspace-shell-trusted-{}",
            uuid::Uuid::new_v4()
        ));
        let trusted = root.join("trusted");
        let outside = root.join("outside");
        fs::create_dir_all(&trusted).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let policy = TrustPolicy {
            trusted_paths: vec![outside.clone()],
            ..TrustPolicy::default()
        };
        let autonomy = AutonomyProfile::default();

        assert!(ensure_workspace_shell_cwd_trusted_from_root(
            &trusted, &policy, &autonomy, &outside
        )
        .is_ok());
    }
}
