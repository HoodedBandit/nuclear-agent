use super::*;

pub(crate) async fn update_command(storage: &Storage, args: UpdateArgs) -> Result<()> {
    let client = ensure_daemon(storage).await?;

    match args.command {
        Some(UpdateSubcommand::Status) => {
            let status = request_update_status(&client).await?;
            print_update_status(&status, args.json)?;
        }
        None => {
            let status = run_update_request(&client, Some(std::process::id())).await?;
            print_update_status(&status, args.json)?;
            if !args.json {
                match status.availability {
                    agent_core::UpdateAvailabilityState::InProgress => {
                        println!("Update helper started. The daemon will restart automatically.");
                    }
                    agent_core::UpdateAvailabilityState::UpToDate => {
                        println!("No update was applied because this install is already current.");
                    }
                    agent_core::UpdateAvailabilityState::Blocked
                    | agent_core::UpdateAvailabilityState::Unsupported => {
                        bail!(
                            "{}",
                            status.detail.clone().unwrap_or_else(|| {
                                "update is not available for this install".to_string()
                            })
                        );
                    }
                    agent_core::UpdateAvailabilityState::Available => {
                        println!("Update is available but was not started.");
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) async fn internal_update_helper_command(args: UpdateHelperArgs) -> Result<()> {
    agent_daemon::run_update_helper_from_plan(&args.plan).await
}

pub(crate) async fn request_update_status(
    client: &DaemonClient,
) -> Result<agent_core::UpdateStatusResponse> {
    client.get("/v1/update/status").await
}

pub(crate) async fn run_update_request(
    client: &DaemonClient,
    wait_for_pid: Option<u32>,
) -> Result<agent_core::UpdateStatusResponse> {
    client
        .post(
            "/v1/update/run",
            &agent_core::UpdateRunRequest { wait_for_pid },
        )
        .await
}

pub(crate) fn should_exit_for_update(status: &agent_core::UpdateStatusResponse) -> bool {
    matches!(
        status.availability,
        agent_core::UpdateAvailabilityState::InProgress
    )
}

pub(crate) fn render_update_status(status: &agent_core::UpdateStatusResponse) -> String {
    let mut lines = vec![
        format!("install_kind={}", install_kind_label(status.install.kind)),
        format!("executable={}", status.install.executable_path),
    ];

    if let Some(install_dir) = status.install.install_dir.as_deref() {
        lines.push(format!("install_dir={install_dir}"));
    }
    if let Some(repo_root) = status.install.repo_root.as_deref() {
        lines.push(format!("repo_root={repo_root}"));
    }
    if let Some(build_profile) = status.install.build_profile.as_deref() {
        lines.push(format!("build_profile={build_profile}"));
    }

    lines.push(format!("current_version={}", status.current_version));
    if let Some(current_commit) = status.current_commit.as_deref() {
        lines.push(format!("current_commit={current_commit}"));
    }

    lines.push(format!(
        "availability={}",
        availability_label(status.availability)
    ));
    if let Some(step) = status.step {
        lines.push(format!("step={}", step_label(step)));
    }
    if let Some(candidate_version) = status.candidate_version.as_deref() {
        lines.push(format!("candidate_version={candidate_version}"));
    }
    if let Some(candidate_tag) = status.candidate_tag.as_deref() {
        lines.push(format!("candidate_tag={candidate_tag}"));
    }
    if let Some(candidate_commit) = status.candidate_commit.as_deref() {
        lines.push(format!("candidate_commit={candidate_commit}"));
    }
    if let Some(published_at) = status.published_at {
        lines.push(format!("published_at={published_at}"));
    }
    if let Some(detail) = status.detail.as_deref() {
        lines.push(format!("detail={detail}"));
    }
    if let Some(last_run) = status.last_run.as_ref() {
        lines.push(format!(
            "last_run={} started_at={} finished_at={} detail={}",
            run_state_label(last_run.state),
            last_run.started_at,
            last_run
                .finished_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "-".to_string()),
            last_run.detail.as_deref().unwrap_or("-")
        ));
    }

    lines.join("\n")
}

fn print_update_status(status: &agent_core::UpdateStatusResponse, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(status)?);
        return Ok(());
    }

    println!("{}", render_update_status(status));
    Ok(())
}

fn install_kind_label(kind: agent_core::UpdateInstallKind) -> &'static str {
    match kind {
        agent_core::UpdateInstallKind::Packaged => "packaged",
        agent_core::UpdateInstallKind::Source => "source",
        agent_core::UpdateInstallKind::Unsupported => "unsupported",
    }
}

fn availability_label(state: agent_core::UpdateAvailabilityState) -> &'static str {
    match state {
        agent_core::UpdateAvailabilityState::UpToDate => "up_to_date",
        agent_core::UpdateAvailabilityState::Available => "available",
        agent_core::UpdateAvailabilityState::Blocked => "blocked",
        agent_core::UpdateAvailabilityState::Unsupported => "unsupported",
        agent_core::UpdateAvailabilityState::InProgress => "in_progress",
    }
}

fn step_label(step: agent_core::UpdateOperationStep) -> &'static str {
    match step {
        agent_core::UpdateOperationStep::Checking => "checking",
        agent_core::UpdateOperationStep::Downloading => "downloading",
        agent_core::UpdateOperationStep::Verifying => "verifying",
        agent_core::UpdateOperationStep::Applying => "applying",
        agent_core::UpdateOperationStep::Restarting => "restarting",
    }
}

fn run_state_label(state: agent_core::UpdateRunState) -> &'static str {
    match state {
        agent_core::UpdateRunState::Succeeded => "succeeded",
        agent_core::UpdateRunState::Failed => "failed",
    }
}
