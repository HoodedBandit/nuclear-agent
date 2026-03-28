use agent_core::{WorkspaceInspectRequest, WorkspaceInspectResponse};
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use tokio::{process::Command as TokioCommand, time::timeout};

use super::{normalize_prompt_input, try_daemon, ReviewArgs, Storage, DEFAULT_GIT_CAPTURE_TIMEOUT};

#[derive(Subcommand)]
pub(crate) enum RepoCommands {
    Inspect(RepoInspectArgs),
}

#[derive(Args)]
pub(crate) struct RepoInspectArgs {
    #[arg(value_name = "PATH")]
    path: Option<String>,
    #[arg(long, default_value_t = false)]
    json: bool,
}

pub(crate) async fn repo_command(storage: &Storage, command: RepoCommands) -> Result<()> {
    match command {
        RepoCommands::Inspect(args) => {
            let report = inspect_workspace(storage, args.path.as_deref()).await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_workspace_report(&report);
            }
        }
    }
    Ok(())
}

pub(crate) fn build_uncommitted_review_prompt(custom_prompt: Option<String>) -> Result<String> {
    build_review_prompt(&ReviewArgs {
        uncommitted: true,
        base: None,
        commit: None,
        commit_title: None,
        prompt: custom_prompt,
        thinking: None,
    })
}

pub(crate) fn build_uncommitted_diff() -> Result<String> {
    collect_review_target(&ReviewArgs {
        uncommitted: true,
        base: None,
        commit: None,
        commit_title: None,
        prompt: None,
        thinking: None,
    })
}

pub(crate) fn build_review_prompt(args: &ReviewArgs) -> Result<String> {
    let review_target = collect_review_target(args)?;
    let custom_prompt = normalize_prompt_input(args.prompt.clone())?;
    let instructions = custom_prompt.unwrap_or_else(|| {
        "Review these code changes. Focus on bugs, regressions, security issues, and missing tests. Put findings first, ordered by severity, and be concise.".to_string()
    });
    Ok(format!(
        "{instructions}\n\nReview target:\n```\n{review_target}\n```"
    ))
}

fn collect_review_target(args: &ReviewArgs) -> Result<String> {
    if let Some(base) = &args.base {
        return capture_git_output(
            &[
                "diff",
                "--no-ext-diff",
                "--stat",
                "--patch",
                &format!("{base}...HEAD"),
            ],
            120_000,
        );
    }
    if let Some(commit) = &args.commit {
        let mut output = capture_git_output(
            &["show", "--stat", "--patch", "--format=medium", commit],
            120_000,
        )?;
        if let Some(title) = &args.commit_title {
            output = format!("Commit title: {title}\n\n{output}");
        }
        return Ok(output);
    }

    let staged = capture_git_output(&["diff", "--no-ext-diff", "--cached"], 60_000)
        .unwrap_or_else(|_| String::new());
    let unstaged =
        capture_git_output(&["diff", "--no-ext-diff"], 60_000).unwrap_or_else(|_| String::new());
    let untracked = capture_git_output(&["ls-files", "--others", "--exclude-standard"], 10_000)
        .unwrap_or_else(|_| String::new());

    let combined = format!(
        "Staged changes:\n{staged}\n\nUnstaged changes:\n{unstaged}\n\nUntracked files:\n{untracked}"
    );
    if combined.trim().is_empty() {
        bail!("no reviewable git changes found");
    }
    Ok(super::truncate_for_prompt(combined, 120_000))
}

fn capture_git_output(args: &[&str], max_len: usize) -> Result<String> {
    async fn run_git_capture(args: Vec<String>) -> Result<std::process::Output> {
        let mut command = TokioCommand::new("git");
        command.kill_on_drop(true);
        command.args(&args);
        timeout(DEFAULT_GIT_CAPTURE_TIMEOUT, command.output())
            .await
            .with_context(|| format!("git {} timed out", args.join(" ")))?
            .with_context(|| format!("failed to run git {}", args.join(" ")))
    }

    let args_vec = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(run_git_capture(args_vec))),
        Err(_) => tokio::runtime::Runtime::new()
            .context("failed to create runtime for git capture")?
            .block_on(run_git_capture(args_vec)),
    }?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(super::truncate_for_prompt(
        String::from_utf8_lossy(&output.stdout).to_string(),
        max_len,
    ))
}

async fn inspect_workspace(
    storage: &Storage,
    path: Option<&str>,
) -> Result<WorkspaceInspectResponse> {
    if let Some(client) = try_daemon(storage).await? {
        return client
            .post(
                "/v1/workspace/inspect",
                &WorkspaceInspectRequest {
                    path: path.map(ToOwned::to_owned),
                },
            )
            .await;
    }
    agent_daemon::inspect_workspace_path(path)
}

fn print_workspace_report(report: &WorkspaceInspectResponse) {
    println!("workspace_root={}", report.workspace_root);
    println!(
        "git_root={}",
        report.git_root.as_deref().unwrap_or("not-a-git-repository")
    );
    if let Some(branch) = report.git_branch.as_deref() {
        println!("git_branch={branch}");
    }
    if let Some(commit) = report.git_commit.as_deref() {
        println!("git_commit={commit}");
    }
    println!(
        "git_status=staged:{} dirty:{} untracked:{}",
        report.staged_files, report.dirty_files, report.untracked_files
    );

    if !report.manifests.is_empty() {
        println!("manifests:");
        for manifest in &report.manifests {
            println!("  {manifest}");
        }
    }
    if !report.language_breakdown.is_empty() {
        println!("languages:");
        for entry in &report.language_breakdown {
            println!("  {}: {}", entry.label, entry.files);
        }
    }
    if !report.focus_paths.is_empty() {
        println!("focus_paths:");
        for entry in &report.focus_paths {
            println!("  {}: {} source file(s)", entry.path, entry.source_files);
        }
    }
    if !report.large_source_files.is_empty() {
        println!("large_source_files:");
        for entry in &report.large_source_files {
            println!("  {}: {} line(s)", entry.path, entry.lines);
        }
    }
    if !report.recent_commits.is_empty() {
        println!("recent_commits:");
        for commit in &report.recent_commits {
            println!("  {commit}");
        }
    }
    if !report.notes.is_empty() {
        println!("notes:");
        for note in &report.notes {
            println!("  {note}");
        }
    }
}
