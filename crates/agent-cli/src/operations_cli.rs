use std::collections::HashSet;

use super::*;

pub(crate) async fn session_command(storage: &Storage, command: SessionCommands) -> Result<()> {
    match command {
        SessionCommands::List => {
            for session in storage.list_sessions(50)? {
                println!(
                    "{} {} {} {} {} {} {}",
                    session.id,
                    session.title.as_deref().unwrap_or("(untitled)"),
                    session.alias,
                    session.provider_id,
                    session.model,
                    session
                        .cwd
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    session.updated_at
                );
            }
        }
        SessionCommands::Resume { id } => {
            let session = storage
                .get_session(&id)?
                .ok_or_else(|| anyhow!("unknown session"))?;
            let transcript = SessionTranscript {
                session,
                messages: storage.list_session_messages(&id)?,
            };
            println!(
                "session={} title={} alias={} provider={} model={}",
                transcript.session.id,
                transcript.session.title.as_deref().unwrap_or("(untitled)"),
                transcript.session.alias,
                transcript.session.provider_id,
                transcript.session.model
            );
            for message in transcript.messages {
                println!(
                    "[{:?}] {}",
                    message.role,
                    format_session_message_for_display(&message)
                );
            }
        }
        SessionCommands::ResumePacket { id } => {
            let packet = load_session_resume_packet(storage, &id).await?;
            println!("{}", format_session_resume_packet(&packet));
        }
        SessionCommands::Rename { id, title } => {
            let title = title.trim();
            if title.is_empty() {
                bail!("session title cannot be empty");
            }
            storage.rename_session(&id, title)?;
            println!("renamed session={} title={}", id, title);
        }
    }
    Ok(())
}

pub(crate) async fn autonomy_command(storage: &Storage, command: AutonomyCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        AutonomyCommands::Enable {
            mode,
            allow_self_edit,
        } => {
            let theme = ColorfulTheme::default();
            println!("{}", autonomy_warning());
            let first = Confirm::with_theme(&theme)
                .with_prompt("Enable Think For Yourself mode?")
                .default(false)
                .interact()?;
            if !first {
                bail!("autonomy enable cancelled");
            }
            let second = Confirm::with_theme(&theme)
                .with_prompt("Confirm that this mode can damage the system and burn API bandwidth without limits")
                .default(false)
                .interact()?;
            if !second {
                bail!("autonomy enable cancelled");
            }
            let status: agent_core::AutonomyProfile = client
                .post(
                    "/v1/autonomy/enable",
                    &AutonomyEnableRequest {
                        mode: Some(mode.into()),
                        allow_self_edit,
                    },
                )
                .await?;
            println!(
                "autonomy={} mode={} unlimited_usage={} full_network={} self_edit={}",
                autonomy_summary(status.state),
                agent_policy::autonomy_mode_summary(status.mode),
                status.unlimited_usage,
                status.full_network,
                status.allow_self_edit
            );
        }
        AutonomyCommands::Pause => {
            let status: agent_core::AutonomyProfile = client
                .post("/v1/autonomy/pause", &serde_json::json!({}))
                .await?;
            println!("autonomy={}", autonomy_summary(status.state));
        }
        AutonomyCommands::Resume => {
            let status: agent_core::AutonomyProfile = client
                .post("/v1/autonomy/resume", &serde_json::json!({}))
                .await?;
            println!("autonomy={}", autonomy_summary(status.state));
        }
        AutonomyCommands::Status => {
            let status: agent_core::AutonomyProfile = client.get("/v1/autonomy/status").await?;
            println!(
                "state={} mode={} unlimited_usage={} full_network={} self_edit={} consented_at={:?}",
                autonomy_summary(status.state),
                agent_policy::autonomy_mode_summary(status.mode),
                status.unlimited_usage,
                status.full_network,
                status.allow_self_edit,
                status.consented_at
            );
        }
    }
    Ok(())
}

pub(crate) async fn evolve_command(storage: &Storage, command: EvolveCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        EvolveCommands::Start {
            alias,
            model,
            budget_friendly,
        } => {
            let theme = ColorfulTheme::default();
            println!(
                "Evolve mode will let the agent methodically improve its own code with free thinking, self-edit, background shell/network access, and test-gated iterative changes."
            );
            let confirmed = Confirm::with_theme(&theme)
                .with_prompt("Start evolve mode?")
                .default(false)
                .interact()?;
            if !confirmed {
                bail!("evolve start cancelled");
            }
            let status: EvolveConfig = client
                .post(
                    "/v1/evolve/start",
                    &EvolveStartRequest {
                        alias,
                        requested_model: model,
                        budget_friendly: Some(budget_friendly),
                    },
                )
                .await?;
            println!(
                "evolve state={} mission={} iteration={} stop_policy={:?}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-"),
                status.iteration,
                status.stop_policy
            );
        }
        EvolveCommands::Pause => {
            let status: EvolveConfig = client
                .post("/v1/evolve/pause", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} mission={}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Resume => {
            let status: EvolveConfig = client
                .post("/v1/evolve/resume", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} mission={}",
                serde_json::to_value(&status.state)?,
                status.current_mission_id.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Stop => {
            let status: EvolveConfig = client
                .post("/v1/evolve/stop", &serde_json::json!({}))
                .await?;
            println!(
                "evolve state={} last_summary={}",
                serde_json::to_value(&status.state)?,
                status.last_summary.as_deref().unwrap_or("-")
            );
        }
        EvolveCommands::Status => {
            let status: EvolveConfig = client.get("/v1/evolve/status").await?;
            println!(
                "state={} stop_policy={:?} mission={} iteration={} pending_restart={} alias={} model={} last_goal={} last_summary={}",
                serde_json::to_value(&status.state)?,
                status.stop_policy,
                status.current_mission_id.as_deref().unwrap_or("-"),
                status.iteration,
                status.pending_restart,
                status.alias.as_deref().unwrap_or("-"),
                status.requested_model.as_deref().unwrap_or("-"),
                status.last_goal.as_deref().unwrap_or("-"),
                status.last_summary.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

pub(crate) async fn autopilot_command(storage: &Storage, command: AutopilotCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        AutopilotCommands::Enable => {
            let status: AutopilotConfig = client
                .put(
                    "/v1/autopilot/status",
                    &AutopilotUpdateRequest {
                        state: Some(AutopilotState::Enabled),
                        max_concurrent_missions: None,
                        wake_interval_seconds: None,
                        allow_background_shell: None,
                        allow_background_network: None,
                        allow_background_self_edit: None,
                    },
                )
                .await?;
            println!("{}", autopilot_summary(&status));
        }
        AutopilotCommands::Pause => {
            let status: AutopilotConfig = client
                .put(
                    "/v1/autopilot/status",
                    &AutopilotUpdateRequest {
                        state: Some(AutopilotState::Paused),
                        max_concurrent_missions: None,
                        wake_interval_seconds: None,
                        allow_background_shell: None,
                        allow_background_network: None,
                        allow_background_self_edit: None,
                    },
                )
                .await?;
            println!("{}", autopilot_summary(&status));
        }
        AutopilotCommands::Resume => {
            let status: AutopilotConfig = client
                .put(
                    "/v1/autopilot/status",
                    &AutopilotUpdateRequest {
                        state: Some(AutopilotState::Enabled),
                        max_concurrent_missions: None,
                        wake_interval_seconds: None,
                        allow_background_shell: None,
                        allow_background_network: None,
                        allow_background_self_edit: None,
                    },
                )
                .await?;
            println!("{}", autopilot_summary(&status));
        }
        AutopilotCommands::Status => {
            let status: AutopilotConfig = client.get("/v1/autopilot/status").await?;
            println!("{}", autopilot_summary(&status));
        }
        AutopilotCommands::Config {
            interval_seconds,
            max_concurrent,
            allow_shell,
            allow_network,
            allow_self_edit,
        } => {
            let status: AutopilotConfig = client
                .put(
                    "/v1/autopilot/status",
                    &AutopilotUpdateRequest {
                        state: None,
                        max_concurrent_missions: max_concurrent,
                        wake_interval_seconds: interval_seconds,
                        allow_background_shell: allow_shell,
                        allow_background_network: allow_network,
                        allow_background_self_edit: allow_self_edit,
                    },
                )
                .await?;
            println!("{}", autopilot_summary(&status));
        }
    }
    Ok(())
}

pub(crate) async fn mission_command(storage: &Storage, command: MissionCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        MissionCommands::Add {
            title,
            details,
            alias,
            model,
            after_seconds,
            every_seconds,
            at,
            watch,
            watch_nonrecursive,
        } => {
            let cwd = current_request_cwd()?;
            let watch_path = resolve_watch_path(watch.as_deref(), &cwd)?;
            if watch_path.is_some()
                && (after_seconds.is_some() || at.is_some() || every_seconds.is_some())
            {
                bail!("use either --watch or timer settings (--after-seconds/--at/--every-seconds), not both");
            }
            let mut mission = Mission::new(title, details);
            mission.alias = alias;
            mission.requested_model = model;
            mission.repeat_interval_seconds = every_seconds.filter(|seconds| *seconds > 0);
            mission.wake_at = resolve_mission_wake_at(after_seconds, at.as_deref())?;
            if mission.wake_at.is_some() || mission.repeat_interval_seconds.is_some() {
                mission.status = MissionStatus::Scheduled;
                mission.wake_trigger = Some(WakeTrigger::Timer);
            }
            mission.workspace_key = Some(cwd.display().to_string());
            mission.watch_path = watch_path;
            mission.watch_recursive = mission.watch_path.is_some() && !watch_nonrecursive;
            if mission.watch_path.is_some() {
                mission.status = MissionStatus::Waiting;
                mission.wake_trigger = Some(WakeTrigger::FileChange);
                mission.wake_at = None;
            }
            let mission: Mission = client.post("/v1/missions", &mission).await?;
            println!(
                "mission={} status={:?} created_at={} wake_at={} repeat={} watch={}",
                mission.id,
                mission.status,
                mission.created_at,
                mission
                    .wake_at
                    .map(|value| value.to_rfc3339())
                    .unwrap_or_else(|| "-".to_string()),
                mission
                    .repeat_interval_seconds
                    .map(|value| format!("{value}s"))
                    .unwrap_or_else(|| "-".to_string()),
                mission
                    .watch_path
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
        }
        MissionCommands::List => {
            for mission in client.get::<Vec<Mission>>("/v1/missions").await? {
                println!(
                    "{} [{:?}] {} wake_at={} repeat={} watch={} retries={}/{}",
                    mission.id,
                    mission.status,
                    mission.title,
                    mission
                        .wake_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "-".to_string()),
                    mission
                        .repeat_interval_seconds
                        .map(|value| format!("{value}s"))
                        .unwrap_or_else(|| "-".to_string()),
                    mission
                        .watch_path
                        .as_deref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    mission.retries,
                    mission.max_retries
                );
                if !mission.details.is_empty() {
                    println!("  {}", mission.details);
                }
            }
        }
        MissionCommands::Pause { id, note } => {
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/pause"),
                    &MissionControlRequest {
                        wake_at: None,
                        clear_wake_at: false,
                        repeat_interval_seconds: None,
                        clear_repeat_interval_seconds: false,
                        watch_path: None,
                        clear_watch_path: false,
                        watch_recursive: None,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Resume {
            id,
            after_seconds,
            every_seconds,
            at,
            watch,
            watch_nonrecursive,
            note,
        } => {
            let cwd = current_request_cwd()?;
            let watch_path = resolve_watch_path(watch.as_deref(), &cwd)?;
            if watch_path.is_some()
                && (after_seconds.is_some() || at.is_some() || every_seconds.is_some())
            {
                bail!("use either --watch or timer settings (--after-seconds/--at/--every-seconds), not both");
            }
            let wake_at = resolve_mission_wake_at(after_seconds, at.as_deref())?;
            let watch_recursive = watch_path.as_ref().map(|_| !watch_nonrecursive);
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/resume"),
                    &MissionControlRequest {
                        wake_at,
                        clear_wake_at: false,
                        repeat_interval_seconds: every_seconds,
                        clear_repeat_interval_seconds: false,
                        watch_path,
                        clear_watch_path: false,
                        watch_recursive,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Cancel { id, note } => {
            let mission: Mission = client
                .post(
                    &format!("/v1/missions/{id}/cancel"),
                    &MissionControlRequest {
                        wake_at: None,
                        clear_wake_at: false,
                        repeat_interval_seconds: None,
                        clear_repeat_interval_seconds: false,
                        watch_path: None,
                        clear_watch_path: false,
                        watch_recursive: None,
                        clear_session_id: false,
                        clear_handoff_summary: false,
                        note,
                    },
                )
                .await?;
            println!("mission={} status={:?}", mission.id, mission.status);
        }
        MissionCommands::Checkpoints { id, limit } => {
            let checkpoints: Vec<MissionCheckpoint> = client
                .get(&format!("/v1/missions/{id}/checkpoints?limit={limit}"))
                .await?;
            for checkpoint in checkpoints.into_iter().rev() {
                println!(
                    "{} [{:?}] {}",
                    checkpoint.created_at, checkpoint.status, checkpoint.summary
                );
                if let Some(session_id) = checkpoint.session_id {
                    println!("  session={}", session_id);
                }
            }
        }
    }
    Ok(())
}

pub(crate) async fn memory_command(storage: &Storage, command: MemoryCommands) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    match command {
        MemoryCommands::List { limit } => {
            let memories: Vec<MemoryRecord> =
                client.get(&format!("/v1/memory?limit={limit}")).await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Review { limit } => {
            let memories: Vec<MemoryRecord> = client
                .get(&format!("/v1/memory/review?limit={limit}"))
                .await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Approve { id, note } => {
            let memory: MemoryRecord = client
                .post(
                    &format!("/v1/memory/{id}/approve"),
                    &MemoryReviewUpdateRequest {
                        status: MemoryReviewStatus::Accepted,
                        note,
                    },
                )
                .await?;
            println!("approved memory={} subject={}", memory.id, memory.subject);
        }
        MemoryCommands::Reject { id, note } => {
            let memory: MemoryRecord = client
                .post(
                    &format!("/v1/memory/{id}/reject"),
                    &MemoryReviewUpdateRequest {
                        status: MemoryReviewStatus::Rejected,
                        note,
                    },
                )
                .await?;
            println!("rejected memory={} subject={}", memory.id, memory.subject);
        }
        MemoryCommands::Profile { limit } => {
            let memories = load_profile_memories(storage, limit).await?;
            println!("{}", format_memory_records(&memories));
        }
        MemoryCommands::Search { query, limit } => {
            let response: MemorySearchResponse = client
                .post(
                    "/v1/memory/search",
                    &MemorySearchQuery {
                        query,
                        limit: Some(limit),
                        workspace_key: Some(current_request_cwd()?.display().to_string()),
                        provider_id: None,
                        review_statuses: Vec::new(),
                        include_superseded: false,
                    },
                )
                .await?;
            if !response.memories.is_empty() {
                println!("{}", format_memory_records(&response.memories));
            }
            if !response.transcript_hits.is_empty() {
                if !response.memories.is_empty() {
                    println!();
                }
                println!("{}", format_session_search_hits(&response.transcript_hits));
            }
        }
        MemoryCommands::Rebuild {
            session_id,
            recompute_embeddings,
        } => {
            let response: MemoryRebuildResponse = client
                .post(
                    "/v1/memory/rebuild",
                    &MemoryRebuildRequest {
                        session_id,
                        recompute_embeddings,
                    },
                )
                .await?;
            println!(
                "generated_at={} sessions_scanned={} observations_scanned={} memories_upserted={} embeddings_refreshed={}",
                response.generated_at,
                response.sessions_scanned,
                response.observations_scanned,
                response.memories_upserted,
                response.embeddings_refreshed
            );
        }
        MemoryCommands::Remember {
            subject,
            content,
            kind,
            scope,
        } => {
            let memory: MemoryRecord = client
                .post(
                    "/v1/memory",
                    &MemoryUpsertRequest {
                        kind: kind.into(),
                        scope: scope.into(),
                        subject,
                        content,
                        confidence: Some(100),
                        source_session_id: None,
                        source_message_id: None,
                        provider_id: None,
                        workspace_key: Some(current_request_cwd()?.display().to_string()),
                        evidence_refs: Vec::new(),
                        tags: vec!["manual".to_string()],
                        identity_key: None,
                        observation_source: None,
                        review_status: Some(MemoryReviewStatus::Accepted),
                        review_note: None,
                        reviewed_at: None,
                        supersedes: None,
                    },
                )
                .await?;
            println!("memory={} subject={}", memory.id, memory.subject);
        }
        MemoryCommands::Forget { id } => {
            let _: serde_json::Value = client.delete(&format!("/v1/memory/{id}")).await?;
            println!("forgot memory={}", id);
        }
    }
    Ok(())
}

pub(crate) async fn logs_command(storage: &Storage, limit: usize, follow: bool) -> Result<()> {
    if follow {
        return follow_events_command(storage, limit).await;
    }

    for entry in storage.list_logs(limit)?.into_iter().rev() {
        print_log_entry(&entry);
    }
    Ok(())
}

async fn follow_events_command(storage: &Storage, limit: usize) -> Result<()> {
    let client = ensure_daemon(storage).await?;
    let mut seen = HashSet::new();
    let mut cursor = None;

    loop {
        let path = event_feed_path(cursor.as_ref(), limit, 30);
        let events: Vec<agent_core::LogEntry> = client.get(&path).await?;
        if events.is_empty() {
            continue;
        }

        for entry in events {
            if !seen.insert(entry.id.clone()) {
                continue;
            }
            cursor = Some(entry.created_at);
            print_log_entry(&entry);
        }
        io::stdout().flush().ok();
    }
}

pub(crate) async fn dashboard_command(storage: &Storage, args: DashboardArgs) -> Result<()> {
    let ui_url = dashboard_ui_url(storage)?;
    let launch_url = dashboard_launch_url(storage).await?;

    if args.print_url || args.no_open {
        println!("Reusable dashboard URL: {ui_url}");
        println!("Immediate one-time connect URL (expires soon): {launch_url}");
    }

    if !args.no_open {
        match opener::open_browser(&launch_url) {
            Ok(_) => {
                if !args.print_url {
                    println!("Reusable dashboard URL: {ui_url}");
                }
            }
            Err(error) => {
                println!("Reusable dashboard URL: {ui_url}");
                println!("Immediate one-time connect URL (expires soon): {launch_url}");
                return Err(anyhow!("failed to open dashboard in browser: {error}"));
            }
        }
    }

    Ok(())
}

pub(crate) fn dashboard_ui_url(storage: &Storage) -> Result<String> {
    let config = storage.load_config()?;
    Ok(format!(
        "http://{}:{}/ui",
        config.daemon.host, config.daemon.port
    ))
}

pub(crate) async fn dashboard_launch_url(storage: &Storage) -> Result<String> {
    let client = ensure_daemon(storage).await?;
    let launch: DashboardLaunchResponse = client
        .post("/v1/dashboard/launch", &serde_json::json!({}))
        .await?;
    Ok(format!(
        "{}{}",
        dashboard_origin(storage)?,
        launch.launch_path
    ))
}

fn dashboard_origin(storage: &Storage) -> Result<String> {
    let config = storage.load_config()?;
    Ok(format!(
        "http://{}:{}",
        config.daemon.host, config.daemon.port
    ))
}

pub(crate) async fn doctor_command(storage: &Storage) -> Result<()> {
    let report = gather_health_report(storage).await?;
    print_health_report(report);
    Ok(())
}

pub(crate) async fn support_bundle_command(
    storage: &Storage,
    args: SupportBundleArgs,
) -> Result<()> {
    let bundle_dir = args.output_dir.unwrap_or_else(|| {
        storage
            .paths()
            .data_dir
            .join("support-bundles")
            .join(chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string())
    });
    fs::create_dir_all(&bundle_dir).with_context(|| {
        format!(
            "failed to create support bundle directory {}",
            bundle_dir.display()
        )
    })?;

    let config = storage.load_config()?;
    let doctor = gather_health_report(storage).await?;
    let daemon_status = if let Some(client) = try_daemon(storage).await? {
        Some(client.get::<DaemonStatus>("/v1/status").await?)
    } else {
        None
    };
    let install_state = load_optional_json_file(
        &std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("install-state.json"))
            })
            .unwrap_or_else(|| PathBuf::from("install-state.json")),
    )?;
    let migration_state = load_optional_json_file(&storage.paths().migration_path)?;
    let logs = storage.list_logs(args.log_limit)?;
    let sessions = storage.list_sessions(args.session_limit)?;
    let config_summary = serde_json::json!({
        "version": config.version,
        "onboarding_complete": config.onboarding_complete,
        "config_path": storage.paths().config_path.display().to_string(),
        "data_path": storage.paths().data_dir.display().to_string(),
        "main_agent_alias": config.main_agent_alias,
        "providers": config.providers.iter().map(|provider| serde_json::json!({
            "id": provider.id,
            "kind": provider.kind,
            "display_name": provider.display_name,
            "base_url": provider.base_url,
            "auth_mode": provider.auth_mode,
        })).collect::<Vec<_>>(),
        "aliases": config.aliases,
        "plugin_count": config.plugins.len(),
        "webhook_connectors": config.webhook_connectors.len(),
        "inbox_connectors": config.inbox_connectors.len(),
        "telegram_connectors": config.telegram_connectors.len(),
        "discord_connectors": config.discord_connectors.len(),
        "slack_connectors": config.slack_connectors.len(),
        "home_assistant_connectors": config.home_assistant_connectors.len(),
        "signal_connectors": config.signal_connectors.len(),
        "gmail_connectors": config.gmail_connectors.len(),
        "brave_connectors": config.brave_connectors.len(),
        "enabled_skills": config.enabled_skills,
        "permission_preset": config.permission_preset,
        "trust_policy": config.trust_policy,
        "autonomy": config.autonomy,
        "autopilot": config.autopilot,
        "delegation": config.delegation,
        "embedding": config.embedding,
    });
    let manifest = serde_json::json!({
        "generated_at": chrono::Utc::now(),
        "bundle_dir": bundle_dir.display().to_string(),
        "cli_version": env!("CARGO_PKG_VERSION"),
        "doctor_file": "doctor.json",
        "daemon_status_file": if daemon_status.is_some() { Some("daemon-status.json") } else { None::<&str> },
        "config_summary_file": "config-summary.json",
        "sessions_file": "sessions.json",
        "logs_file": "logs.json",
        "install_state_file": install_state.as_ref().map(|_| "install-state.json"),
        "path_migration_file": migration_state.as_ref().map(|_| "path-migration.json"),
    });

    write_json_file(&bundle_dir.join("doctor.json"), &doctor)?;
    write_json_file(&bundle_dir.join("config-summary.json"), &config_summary)?;
    write_json_file(&bundle_dir.join("sessions.json"), &sessions)?;
    write_json_file(&bundle_dir.join("logs.json"), &logs)?;
    write_json_file(&bundle_dir.join("manifest.json"), &manifest)?;
    if let Some(status) = daemon_status.as_ref() {
        write_json_file(&bundle_dir.join("daemon-status.json"), status)?;
    }
    if let Some(value) = install_state.as_ref() {
        write_json_file(&bundle_dir.join("install-state.json"), value)?;
    }
    if let Some(value) = migration_state.as_ref() {
        write_json_file(&bundle_dir.join("path-migration.json"), value)?;
    }

    let readme = [
        "# Nuclear Agent Support Bundle".to_string(),
        String::new(),
        format!("- generated_at: `{}`", chrono::Utc::now().to_rfc3339()),
        format!("- bundle_dir: `{}`", bundle_dir.display()),
        format!("- daemon_running: `{}`", doctor.daemon_running),
        format!("- config_path: `{}`", doctor.config_path),
        format!("- data_path: `{}`", doctor.data_path),
        format!("- logs: `{}`", args.log_limit),
        format!("- sessions: `{}`", args.session_limit),
        String::new(),
        "Files:".to_string(),
        "- `manifest.json`".to_string(),
        "- `doctor.json`".to_string(),
        "- `config-summary.json`".to_string(),
        "- `sessions.json`".to_string(),
        "- `logs.json`".to_string(),
    ]
    .join("\n");
    fs::write(bundle_dir.join("README.md"), readme)
        .with_context(|| format!("failed to write {}", bundle_dir.join("README.md").display()))?;

    println!("support_bundle={}", bundle_dir.display());
    Ok(())
}

async fn gather_health_report(storage: &Storage) -> Result<HealthReport> {
    if let Some(client) = try_daemon(storage).await? {
        let report: HealthReport = client.get("/v1/doctor").await?;
        return Ok(report);
    }

    let config = storage.load_config()?;
    let client = build_http_client();
    let providers = futures::future::join_all(
        config
            .providers
            .iter()
            .map(|provider| health_check(&client, provider)),
    )
    .await;
    Ok(HealthReport {
        daemon_running: try_daemon(storage).await?.is_some(),
        config_path: storage.paths().config_path.display().to_string(),
        data_path: storage.paths().data_dir.display().to_string(),
        keyring_ok: keyring_available(),
        providers,
        plugins: config
            .plugins
            .iter()
            .map(storage_plugins::doctor_plugin)
            .collect(),
        remote_content_policy: config.remote_content_policy,
        provider_capabilities: Vec::new(),
    })
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<()> {
    let content =
        serde_json::to_string_pretty(value).context("failed to serialize bundle content")?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn load_optional_json_file(path: &Path) -> Result<Option<serde_json::Value>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read optional json file {}", path.display()))?;
    let value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse optional json file {}", path.display()))?;
    Ok(Some(value))
}

fn print_health_report(report: HealthReport) {
    println!("daemon_running={}", report.daemon_running);
    println!("config_path={}", report.config_path);
    println!("data_path={}", report.data_path);
    println!("keyring_ok={}", report.keyring_ok);
    for provider in report.providers {
        println!(
            "{} ok={} detail={}",
            provider.id, provider.ok, provider.detail
        );
    }
    for plugin in report.plugins {
        println!(
            "{} ok={} enabled={} trusted={} detail={}",
            plugin.id, plugin.ok, plugin.enabled, plugin.trusted, plugin.detail
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::LogEntry;

    fn temp_storage() -> (Storage, PathBuf) {
        let root = std::env::temp_dir().join(format!("nuclear-support-bundle-{}", Uuid::new_v4()));
        let storage = Storage::open_at(&root).unwrap();
        (storage, root)
    }

    #[tokio::test]
    async fn support_bundle_command_writes_expected_artifacts() {
        let (storage, root) = temp_storage();
        storage
            .append_log(&LogEntry::new("info", "test", "hello bundle"))
            .unwrap();
        let output_dir = root.join("bundle");

        support_bundle_command(
            &storage,
            SupportBundleArgs {
                output_dir: Some(output_dir.clone()),
                log_limit: 10,
                session_limit: 10,
            },
        )
        .await
        .unwrap();

        assert!(output_dir.join("manifest.json").exists());
        assert!(output_dir.join("doctor.json").exists());
        assert!(output_dir.join("config-summary.json").exists());
        assert!(output_dir.join("logs.json").exists());
        assert!(output_dir.join("sessions.json").exists());
        assert!(output_dir.join("README.md").exists());
    }
}
