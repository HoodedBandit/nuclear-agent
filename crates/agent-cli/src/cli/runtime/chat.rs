pub(crate) async fn run_command(storage: &Storage, args: RunArgs) -> Result<()> {
    ensure_onboarded(storage).await?;
    let client = ensure_daemon(storage).await?;
    let cwd = current_request_cwd()?;
    let thinking_level = resolve_thinking_level(storage, args.thinking)?;
    let task_mode = args.mode.map(Into::into);
    let attachments = collect_image_attachments(&cwd, &args.images)?;
    let output_schema_json = args
        .output_schema
        .as_deref()
        .map(load_schema_file)
        .transpose()?;
    if !args.tasks.is_empty() {
        let tasks = args
            .tasks
            .into_iter()
            .map(parse_task)
            .collect::<Result<Vec<_>>>()?;
        let response: BatchTaskResponse = client
            .post(
                "/v1/batch",
                &BatchTaskRequest {
                    tasks,
                    cwd: Some(cwd),
                    thinking_level,
                    task_mode,
                    strategy: None,
                    parent_alias: None,
                },
            )
            .await?;
        if !args.json && !response.summary.is_empty() {
            println!("{}\n", response.summary);
        }
        for result in response.results {
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "event": "batch_result",
                        "alias": result.alias,
                        "provider_id": result.provider_id,
                        "model": result.model,
                        "success": result.success,
                        "response": result.response,
                        "error": result.error,
                    })
                );
            } else if result.success {
                println!(
                    "[{} | {} | {}]\n{}\n",
                    result.alias, result.provider_id, result.model, result.response
                );
            } else {
                println!(
                    "[{} | {} | {} | error]\n{}\n",
                    result.alias,
                    result.provider_id,
                    result.model,
                    result
                        .error
                        .as_deref()
                        .unwrap_or("subagent task failed without an error message")
                );
            }
        }
        return Ok(());
    }

    let prompt = normalize_prompt_input(args.prompt)?
        .ok_or_else(|| anyhow!("prompt is required when --task is not used"))?;
    let response = execute_prompt(
        &client,
        prompt,
        args.alias,
        None,
        None,
        cwd,
        thinking_level,
        task_mode,
        attachments,
        args.permissions.map(Into::into),
        output_schema_json,
        args.ephemeral,
    )
    .await?;
    maybe_write_last_message(args.output_last_message.as_deref(), &response.response)?;
    if args.json {
        print_json_run_response(&response)?;
    } else {
        println!("{}", response.response);
        println!(
            "\nsession={} alias={} provider={} model={}",
            response.session_id, response.alias, response.provider_id, response.model
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn launch_chat_session(
    storage: &Storage,
    alias: Option<String>,
    session_id: Option<String>,
    initial_prompt: Option<String>,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    attachments: Vec<InputAttachment>,
    permission_preset: Option<PermissionPreset>,
    no_tui: bool,
) -> Result<()> {
    ensure_onboarded(storage).await?;
    if !no_tui && io::stdout().is_terminal() && io::stdin().is_terminal() {
        return tui::run_tui_session(
            storage,
            alias,
            session_id,
            initial_prompt,
            thinking_level,
            task_mode,
            attachments,
            permission_preset,
        )
        .await;
    }
    interactive_session(
        storage,
        alias,
        session_id,
        initial_prompt,
        thinking_level,
        task_mode,
        attachments,
        permission_preset,
    )
    .await
}

pub(crate) async fn chat_command(storage: &Storage, args: ChatArgs) -> Result<()> {
    let cwd = current_request_cwd()?;
    let attachments = collect_image_attachments(&cwd, &args.images)?;
    launch_chat_session(
        storage,
        args.alias,
        None,
        None,
        resolve_thinking_level(storage, args.thinking)?,
        args.mode.map(Into::into),
        attachments,
        args.permissions.map(Into::into),
        args.no_tui,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn interactive_session(
    storage: &Storage,
    mut alias: Option<String>,
    mut session_id: Option<String>,
    initial_prompt: Option<String>,
    mut thinking_level: Option<ThinkingLevel>,
    mut task_mode: Option<TaskMode>,
    mut attachments: Vec<InputAttachment>,
    mut permission_preset: Option<PermissionPreset>,
) -> Result<()> {
    let mut client = ensure_daemon(storage).await?;
    let mut cwd =
        load_session_cwd(storage, session_id.as_deref())?.unwrap_or(current_request_cwd()?);
    if task_mode.is_none() {
        task_mode = load_session_task_mode(storage, session_id.as_deref())?;
    }
    let mut last_output = load_last_assistant_output(storage, session_id.as_deref())?;
    let mut requested_model =
        resolve_session_model_override(storage, session_id.as_deref(), alias.as_deref())?;
    if thinking_level.is_none() {
        thinking_level = storage.load_config()?.thinking_level;
    }
    if permission_preset.is_none() {
        permission_preset = Some(storage.load_config()?.permission_preset);
    }
    println!(
        "Interactive chat. Use /help for commands, /model or Ctrl+P for alias/main switching, /provider for provider switching, /mode to switch between build and daily presets, /onboard for a fresh setup reset, and /thinking to adjust reasoning."
    );

    if let Some(prompt) = normalize_prompt_input(initial_prompt)? {
        let response = execute_prompt(
            &client,
            prompt,
            alias.clone(),
            requested_model.clone(),
            session_id.clone(),
            cwd.clone(),
            thinking_level,
            task_mode,
            attachments.clone(),
            permission_preset,
            None,
            false,
        )
        .await?;
        session_id = Some(response.session_id.clone());
        requested_model =
            resolve_requested_model_override(storage, alias.as_deref(), &response.model)?;
        last_output = Some(response.response.clone());
        println!("\n{}\n", response.response);
    }

    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(shell_command) = line.strip_prefix('!') {
            match run_bang_command(storage, shell_command.trim(), &mut cwd).await {
                Ok(output) => {
                    if !output.is_empty() {
                        println!("{output}");
                    }
                }
                Err(error) => println!("error: {error:#}"),
            }
            continue;
        }

        if line.starts_with('/') {
            match parse_interactive_command(line) {
                Ok(Some(InteractiveCommand::Exit)) => break,
                Ok(Some(command)) => {
                    let command_result: Result<()> = async {
                        match command {
                            InteractiveCommand::Exit => unreachable!(),
                            InteractiveCommand::Help => print_interactive_help(),
                            InteractiveCommand::Status => {
                                print_interactive_status(
                                    storage,
                                    &client,
                                    alias.as_deref(),
                                    requested_model.as_deref(),
                                    session_id.as_deref(),
                                    thinking_level,
                                    task_mode,
                                    permission_preset,
                                    &attachments,
                                    cwd.as_path(),
                                )
                                .await?;
                            }
                            InteractiveCommand::ConfigShow => {
                                println!(
                                    "Settings:\n  /config opens the categorized settings menu in the TUI.\n  /dashboard opens the localhost web control room.\n  /model opens the alias/main switcher, /provider switches logged-in providers, and /mode, /thinking, and /permissions remain quick shortcuts."
                                );
                            }
                            InteractiveCommand::DashboardOpen => {
                                dashboard_command(
                                    storage,
                                    DashboardArgs {
                                        no_open: false,
                                        print_url: true,
                                    },
                                )
                                .await?;
                            }
                            InteractiveCommand::TelegramsShow => {
                                let connectors = load_telegram_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No telegram connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} chats={} users={} alias={} model={} last_update_id={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_i64_list(&connector.allowed_chat_ids),
                                            format_i64_list(&connector.allowed_user_ids),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .last_update_id
                                                .map(|value| value.to_string())
                                                .unwrap_or_else(|| "-".to_string()),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::DiscordsShow => {
                                let connectors = load_discord_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No discord connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_string_list(&connector.monitored_channel_ids),
                                            format_string_list(&connector.allowed_channel_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.channel_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::SlacksShow => {
                                let connectors = load_slack_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No slack connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} monitored_channels={} allowed_channels={} users={} tracked_channels={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            format_string_list(&connector.monitored_channel_ids),
                                            format_string_list(&connector.allowed_channel_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.channel_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::SignalsShow => {
                                let connectors = load_signal_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No signal connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} require_pairing_approval={} account={} cli_path={} groups={} allowed_groups={} users={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.require_pairing_approval,
                                            connector.account,
                                            connector
                                                .cli_path
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "signal-cli".to_string()),
                                            format_string_list(&connector.monitored_group_ids),
                                            format_string_list(&connector.allowed_group_ids),
                                            format_string_list(&connector.allowed_user_ids),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::HomeAssistantsShow => {
                                let connectors = load_home_assistant_connectors(storage).await?;
                                if connectors.is_empty() {
                                    println!("No Home Assistant connectors configured.");
                                } else {
                                    for connector in connectors {
                                        println!(
                                            "{} [{}] enabled={} token={} base_url={} monitored_entities={} service_domains={} service_entities={} tracked_entities={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.access_token_keychain_account.is_some(),
                                            connector.base_url,
                                            format_string_list(&connector.monitored_entity_ids),
                                            format_string_list(&connector.allowed_service_domains),
                                            format_string_list(&connector.allowed_service_entity_ids),
                                            connector.entity_cursors.len(),
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::DiscordApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Discord, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::DiscordApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved discord pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::DiscordReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected discord pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::SlackApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Slack, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::SlackApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved slack pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::SlackReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected slack pairing={} connector={} channel={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::TelegramApprovalsShow => {
                                let approvals =
                                    load_connector_approvals(storage, ConnectorKind::Telegram, 25)
                                        .await?;
                                println!("{}", format_connector_approvals(&approvals));
                            }
                            InteractiveCommand::TelegramApprove { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Approved,
                                    note,
                                )
                                .await?;
                                println!(
                                    "approved telegram pairing={} connector={} chat={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::TelegramReject { id, note } => {
                                let approval = update_connector_approval_status(
                                    storage,
                                    &id,
                                    ConnectorApprovalStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!(
                                    "rejected telegram pairing={} connector={} chat={} user={}",
                                    approval.id,
                                    approval.connector_id,
                                    approval.external_chat_display.as_deref().unwrap_or("-"),
                                    approval.external_user_display.as_deref().unwrap_or("-")
                                );
                            }
                            InteractiveCommand::WebhooksShow => {
                                let webhooks = load_webhook_connectors(storage).await?;
                                if webhooks.is_empty() {
                                    println!("No webhook connectors configured.");
                                } else {
                                    for connector in webhooks {
                                        println!(
                                            "{} [{}] enabled={} alias={} model={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::InboxesShow => {
                                let inboxes = load_inbox_connectors(storage).await?;
                                if inboxes.is_empty() {
                                    println!("No inbox connectors configured.");
                                } else {
                                    for connector in inboxes {
                                        println!(
                                            "{} [{}] enabled={} delete_after_read={} alias={} model={} path={} cwd={}",
                                            connector.id,
                                            connector.name,
                                            connector.enabled,
                                            connector.delete_after_read,
                                            connector.alias.as_deref().unwrap_or("-"),
                                            connector.requested_model.as_deref().unwrap_or("-"),
                                            connector.path.display(),
                                            connector
                                                .cwd
                                                .as_ref()
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| "-".to_string())
                                        );
                                    }
                                }
                            }
                            InteractiveCommand::AutopilotShow => {
                                let status: AutopilotConfig =
                                    client.get("/v1/autopilot/status").await?;
                                println!("{}", autopilot_summary(&status));
                            }
                            InteractiveCommand::AutopilotEnable => {
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
                            InteractiveCommand::AutopilotPause => {
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
                            InteractiveCommand::AutopilotResume => {
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
                            InteractiveCommand::MissionsShow => {
                                let missions: Vec<Mission> = client.get("/v1/missions").await?;
                                for mission in missions {
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
                                            .map(|value| value.display().to_string())
                                            .unwrap_or_else(|| "-".to_string()),
                                        mission.retries,
                                        mission.max_retries
                                    );
                                }
                            }
                            InteractiveCommand::EventsShow(limit) => {
                                let events: Vec<agent_core::LogEntry> = client
                                    .get(&format!("/v1/events?limit={limit}"))
                                    .await?;
                                for entry in events {
                                    print_log_entry(&entry);
                                }
                            }
                            InteractiveCommand::Schedule {
                                after_seconds,
                                title,
                            } => {
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Scheduled;
                                mission.wake_at = Some(
                                    chrono::Utc::now()
                                        + chrono::Duration::seconds(after_seconds as i64),
                                );
                                mission.wake_trigger = Some(agent_core::WakeTrigger::Timer);
                                mission.workspace_key =
                                    Some(cwd.display().to_string());
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} wake_at={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .wake_at
                                        .map(|value| value.to_rfc3339())
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::Repeat {
                                every_seconds,
                                title,
                            } => {
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Scheduled;
                                mission.wake_at = Some(
                                    chrono::Utc::now()
                                        + chrono::Duration::seconds(every_seconds as i64),
                                );
                                mission.repeat_interval_seconds = Some(every_seconds);
                                mission.wake_trigger = Some(agent_core::WakeTrigger::Timer);
                                mission.workspace_key =
                                    Some(cwd.display().to_string());
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} wake_at={} repeat={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .wake_at
                                        .map(|value| value.to_rfc3339())
                                        .unwrap_or_else(|| "-".to_string()),
                                    mission
                                        .repeat_interval_seconds
                                        .map(|value| format!("{value}s"))
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::Watch { path, title } => {
                                let watch_path = resolve_watch_path(Some(path.as_path()), &cwd)?
                                    .ok_or_else(|| anyhow!("watch path is required"))?;
                                let mut mission = Mission::new(title, String::new());
                                mission.status = MissionStatus::Waiting;
                                mission.wake_trigger = Some(WakeTrigger::FileChange);
                                mission.workspace_key = Some(cwd.display().to_string());
                                mission.watch_path = Some(watch_path);
                                mission.watch_recursive = true;
                                let mission: Mission = client.post("/v1/missions", &mission).await?;
                                println!(
                                    "mission={} status={:?} watch={}",
                                    mission.id,
                                    mission.status,
                                    mission
                                        .watch_path
                                        .as_deref()
                                        .map(|value| value.display().to_string())
                                        .unwrap_or_else(|| "-".to_string())
                                );
                            }
                            InteractiveCommand::ProfileShow => {
                                let memories = load_profile_memories(storage, 20).await?;
                                println!("{}", format_memory_records(&memories));
                            }
                            InteractiveCommand::MemoryReviewShow => {
                                let memories = load_memory_review_queue(storage, 20).await?;
                                println!("{}", format_memory_records(&memories));
                            }
                            InteractiveCommand::MemoryRebuild { session_id } => {
                                let response: MemoryRebuildResponse = client
                                    .post(
                                        "/v1/memory/rebuild",
                                        &MemoryRebuildRequest {
                                            session_id,
                                            recompute_embeddings: false,
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
                            InteractiveCommand::MemoryShow(query) => {
                                if let Some(query) = query {
                                    let result: MemorySearchResponse = client
                                        .post(
                                            "/v1/memory/search",
                                            &MemorySearchQuery {
                                                query,
                                                limit: Some(10),
                                                workspace_key: Some(cwd.display().to_string()),
                                                provider_id: None,
                                                review_statuses: Vec::new(),
                                                include_superseded: false,
                                            },
                                        )
                                        .await?;
                                    if !result.memories.is_empty() {
                                        println!("{}", format_memory_records(&result.memories));
                                    }
                                    if !result.transcript_hits.is_empty() {
                                        if !result.memories.is_empty() {
                                            println!();
                                        }
                                        println!(
                                            "{}",
                                            format_session_search_hits(&result.transcript_hits)
                                        );
                                    }
                                } else {
                                    let memories: Vec<MemoryRecord> =
                                        client.get("/v1/memory?limit=10").await?;
                                    println!("{}", format_memory_records(&memories));
                                }
                            }
                            InteractiveCommand::MemoryApprove { id, note } => {
                                let memory = update_memory_review_status(
                                    storage,
                                    &id,
                                    MemoryReviewStatus::Accepted,
                                    note,
                                )
                                .await?;
                                println!("approved memory={} subject={}", memory.id, memory.subject);
                            }
                            InteractiveCommand::MemoryReject { id, note } => {
                                let memory = update_memory_review_status(
                                    storage,
                                    &id,
                                    MemoryReviewStatus::Rejected,
                                    note,
                                )
                                .await?;
                                println!("rejected memory={} subject={}", memory.id, memory.subject);
                            }
                            InteractiveCommand::Skills(command) => match command {
                                InteractiveSkillCommand::Show(status) => {
                                    let drafts = load_skill_drafts(storage, 20, status).await?;
                                    println!("{}", format_skill_drafts(&drafts));
                                }
                                InteractiveSkillCommand::Publish(id) => {
                                    let draft = update_skill_draft_status(
                                        storage,
                                        &id,
                                        SkillDraftStatus::Published,
                                    )
                                    .await?;
                                    println!(
                                        "published skill draft={} title={}",
                                        draft.id, draft.title
                                    );
                                }
                                InteractiveSkillCommand::Reject(id) => {
                                    let draft = update_skill_draft_status(
                                        storage,
                                        &id,
                                        SkillDraftStatus::Rejected,
                                    )
                                    .await?;
                                    println!(
                                        "rejected skill draft={} title={}",
                                        draft.id, draft.title
                                    );
                                }
                            },
                            InteractiveCommand::Remember(content) => {
                                let subject = manual_memory_subject(&content);
                                let memory: MemoryRecord = client
                                    .post(
                                        "/v1/memory",
                                        &MemoryUpsertRequest {
                                            kind: MemoryKind::Note,
                                            scope: MemoryScope::Global,
                                            subject,
                                            content,
                                            confidence: Some(100),
                                            source_session_id: session_id.clone(),
                                            source_message_id: None,
                                            provider_id: None,
                                            workspace_key: Some(cwd.display().to_string()),
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
                            InteractiveCommand::Forget(id) => {
                                let _: serde_json::Value =
                                    client.delete(&format!("/v1/memory/{id}")).await?;
                                println!("forgot memory={id}");
                            }
                            InteractiveCommand::PermissionsShow => {
                                print_permissions_status(storage, &client).await?;
                            }
                            InteractiveCommand::PermissionsSet(new_preset) => {
                                let next_preset = match new_preset {
                                    Some(preset) => preset,
                                    None => storage.load_config()?.permission_preset,
                                };
                                let updated: PermissionPreset = client
                                    .put(
                                        "/v1/permissions",
                                        &PermissionUpdateRequest {
                                            permission_preset: next_preset,
                                        },
                                    )
                                    .await?;
                                permission_preset = Some(updated);
                                println!("permission_preset={}", permission_summary(updated));
                            }
                            InteractiveCommand::Attach(path) => {
                                let mut new = collect_image_attachments(&cwd, &[path])?;
                                attachments.append(&mut new);
                                println!("attachments={}", attachments.len());
                            }
                            InteractiveCommand::AttachmentsShow => {
                                if attachments.is_empty() {
                                    println!("attachments=(none)");
                                } else {
                                    for attachment in &attachments {
                                        println!("{}", attachment.path.display());
                                    }
                                }
                            }
                            InteractiveCommand::AttachmentsClear => {
                                attachments.clear();
                                println!("attachments cleared");
                            }
                            InteractiveCommand::New => {
                                session_id = None;
                                last_output = None;
                                requested_model = None;
                                println!("Started a new chat session.");
                            }
                            InteractiveCommand::Clear => {
                                clear_terminal();
                                session_id = None;
                                last_output = None;
                                requested_model = None;
                                println!("Started a new chat session.");
                            }
                            InteractiveCommand::Diff => {
                                println!("{}", build_uncommitted_diff()?);
                            }
                            InteractiveCommand::Copy => {
                                let text = last_output.as_deref().ok_or_else(|| {
                                    anyhow!("no assistant output available to copy")
                                })?;
                                copy_to_clipboard(text)?;
                                println!("Copied the latest assistant output to the clipboard.");
                            }
                            InteractiveCommand::Compact => {
                                let current_session = session_id
                                    .as_deref()
                                    .ok_or_else(|| anyhow!("no active session to compact"))?;
                                let transcript = load_session_for_command(
                                    storage,
                                    Some(current_session.to_string()),
                                    false,
                                    false,
                                )?;
                                let prompt = build_compact_prompt(&transcript)?;
                                let response = execute_prompt(
                                    &client,
                                    prompt,
                                    alias.clone(),
                                    requested_model.clone(),
                                    None,
                                    cwd.clone(),
                                    thinking_level,
                                    task_mode,
                                    Vec::new(),
                                    permission_preset,
                                    None,
                                    true,
                                )
                                .await?;
                                let new_session_id =
                                    compact_session(storage, &transcript, &response.response)?;
                                session_id = Some(new_session_id.clone());
                                println!(
                                    "Compacted session {} -> {}",
                                    transcript.session.id, new_session_id
                                );
                            }
                            InteractiveCommand::Init => {
                                let path = cwd.join("AGENTS.md");
                                if init_agents_file(&path)? {
                                    println!("Initialized {}", path.display());
                                } else {
                                    println!(
                                        "{} already exists; leaving it unchanged.",
                                        path.display()
                                    );
                                }
                            }
                            InteractiveCommand::Onboard => {
                                run_onboarding_reset(storage, true).await?;
                                client = ensure_daemon(storage).await?;
                                let config = storage.load_config()?;
                                alias = config.main_agent_alias.clone();
                                session_id = None;
                                requested_model = None;
                                thinking_level = config.thinking_level;
                                task_mode = None;
                                permission_preset = Some(config.permission_preset);
                                attachments.clear();
                                last_output = None;
                                cwd = current_request_cwd()?;
                                println!(
                                    "Onboarding reset complete. Started fresh setup with main alias {}.",
                                    config.main_agent_alias.as_deref().unwrap_or("(not configured)")
                                );
                            }
                            InteractiveCommand::ModelShow => {
                                println!(
                                    "{}",
                                    interactive_model_choices_text(
                                        storage,
                                        alias.as_deref(),
                                        requested_model.as_deref(),
                                    )
                                    .await?
                                );
                            }
                            InteractiveCommand::ProviderShow => {
                                println!(
                                    "{}",
                                    interactive_provider_choices_text(storage, alias.as_deref())?
                                );
                            }
                            InteractiveCommand::ModelSet(selection) => {
                                match resolve_interactive_model_selection(
                                    storage,
                                    alias.as_deref(),
                                    &selection,
                                )
                                .await?
                                {
                                    InteractiveModelSelection::Alias(new_alias) => {
                                        alias = Some(new_alias.clone());
                                        requested_model = None;
                                        println!("model alias set to {new_alias}");
                                    }
                                    InteractiveModelSelection::Explicit(model_id) => {
                                        requested_model = Some(model_id.clone());
                                        println!("model override set to {model_id}");
                                    }
                                }
                            }
                            InteractiveCommand::ProviderSet(selection) => {
                                let new_alias = resolve_interactive_provider_selection(
                                    storage,
                                    alias.as_deref(),
                                    &selection,
                                )?;
                                let config = storage.load_config()?;
                                let summary = config
                                    .alias_target_summary(&new_alias)
                                    .ok_or_else(|| anyhow!("unknown alias '{new_alias}'"))?;
                                alias = Some(new_alias.clone());
                                requested_model = None;
                                println!(
                                    "provider set to {} via alias {} ({})",
                                    summary.provider_display_name, summary.alias, summary.model
                                );
                            }
                            InteractiveCommand::ThinkingShow => {
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::ModeShow => {
                                println!("mode={}", task_mode_label(task_mode));
                            }
                            InteractiveCommand::ThinkingSet(new_level) => {
                                thinking_level = new_level;
                                persist_thinking_level(storage, thinking_level)?;
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::ModeSet(new_mode) => {
                                task_mode = new_mode;
                                println!("mode={}", task_mode_label(task_mode));
                            }
                            InteractiveCommand::Fast => {
                                thinking_level = Some(ThinkingLevel::Minimal);
                                persist_thinking_level(storage, thinking_level)?;
                                println!("thinking={}", thinking_level_label(thinking_level));
                            }
                            InteractiveCommand::Rename(new_title) => {
                                let current_session = session_id
                                    .as_deref()
                                    .ok_or_else(|| anyhow!("no active session to rename"))?;
                                let title = match new_title {
                                    Some(title) => title,
                                    None => Input::<String>::with_theme(&ColorfulTheme::default())
                                        .with_prompt("Session title")
                                        .interact_text()?,
                                };
                                let title = title.trim();
                                if title.is_empty() {
                                    bail!("session title cannot be empty");
                                }
                                storage.rename_session(current_session, title)?;
                                println!("renamed session={} title={}", current_session, title);
                            }
                            InteractiveCommand::Review(custom_prompt) => {
                                let prompt = build_uncommitted_review_prompt(custom_prompt)?;
                                let response = execute_prompt(
                                    &client,
                                    prompt,
                                    alias.clone(),
                                    requested_model.clone(),
                                    session_id.clone(),
                                    cwd.clone(),
                                    thinking_level,
                                    Some(TaskMode::Build),
                                    attachments.clone(),
                                    permission_preset,
                                    None,
                                    false,
                                )
                                .await?;
                                session_id = Some(response.session_id.clone());
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &response.model,
                                )?;
                                last_output = Some(response.response.clone());
                                println!("\n{}\n", response.response);
                            }
                            InteractiveCommand::Resume(target) => {
                                let transcript = load_transcript_for_interactive_resume(
                                    storage,
                                    target.as_deref(),
                                )?;
                                println!(
                                    "Resumed session={} title={} alias={} provider={} model={} mode={}",
                                    transcript.session.id,
                                    transcript.session.title.as_deref().unwrap_or("(untitled)"),
                                    transcript.session.alias,
                                    transcript.session.provider_id,
                                    transcript.session.model,
                                    task_mode_label(transcript.session.task_mode),
                                );
                                last_output = latest_assistant_output_from_transcript(&transcript);
                                alias = Some(transcript.session.alias.clone());
                                session_id = Some(transcript.session.id.clone());
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &transcript.session.model,
                                )?;
                                task_mode = transcript.session.task_mode;
                                cwd = transcript
                                    .session
                                    .cwd
                                    .clone()
                                    .unwrap_or_else(|| cwd.clone());
                                attachments.clear();
                            }
                            InteractiveCommand::Fork(target) => {
                                let transcript = load_transcript_for_interactive_fork(
                                    storage,
                                    session_id.as_deref(),
                                    target.as_deref(),
                                )?;
                                let new_session_id = fork_session(storage, &transcript)?;
                                println!(
                                    "Forked session {} ({}) -> {}",
                                    transcript.session.id,
                                    transcript.session.title.as_deref().unwrap_or("(untitled)"),
                                    new_session_id
                                );
                                last_output = latest_assistant_output_from_transcript(&transcript);
                                alias = Some(transcript.session.alias.clone());
                                session_id = Some(new_session_id);
                                requested_model = resolve_requested_model_override(
                                    storage,
                                    alias.as_deref(),
                                    &transcript.session.model,
                                )?;
                                task_mode = transcript.session.task_mode;
                                cwd = transcript
                                    .session
                                    .cwd
                                    .clone()
                                    .unwrap_or_else(|| cwd.clone());
                            }
                        }
                        Ok(())
                    }
                    .await;
                    if let Err(error) = command_result {
                        println!("error: {error:#}");
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    println!("error: {error:#}");
                    continue;
                }
            }
        }

        let response = execute_prompt(
            &client,
            line.to_string(),
            alias.clone(),
            requested_model.clone(),
            session_id.clone(),
            cwd.clone(),
            thinking_level,
            task_mode,
            attachments.clone(),
            permission_preset,
            None,
            false,
        )
        .await?;
        session_id = Some(response.session_id.clone());
        requested_model =
            resolve_requested_model_override(storage, alias.as_deref(), &response.model)?;
        last_output = Some(response.response.clone());
        println!("\n{}\n", response.response);
    }

    Ok(())
}

pub(crate) async fn review_command(storage: &Storage, args: ReviewArgs) -> Result<()> {
    ensure_onboarded(storage).await?;
    let prompt = build_review_prompt(&args)?;
    let client = ensure_daemon(storage).await?;
    let thinking_level = resolve_thinking_level(storage, args.thinking)?;
    let response = execute_prompt(
        &client,
        prompt,
        None,
        None,
        None,
        current_request_cwd()?,
        thinking_level,
        Some(TaskMode::Build),
        Vec::new(),
        None,
        None,
        false,
    )
    .await?;
    println!("{}", response.response);
    println!(
        "\nsession={} alias={} provider={} model={}",
        response.session_id, response.alias, response.provider_id, response.model
    );
    Ok(())
}

pub(crate) async fn resume_command(storage: &Storage, args: ResumeArgs) -> Result<()> {
    let transcript = load_session_for_command(storage, args.session_id, args.last, args.all)?;
    println!(
        "Resuming session={} title={} alias={} provider={} model={} mode={}",
        transcript.session.id,
        transcript.session.title.as_deref().unwrap_or("(untitled)"),
        transcript.session.alias,
        transcript.session.provider_id,
        transcript.session.model,
        task_mode_label(transcript.session.task_mode),
    );
    launch_chat_session(
        storage,
        Some(transcript.session.alias),
        Some(transcript.session.id),
        args.prompt,
        resolve_thinking_level(storage, args.thinking)?,
        transcript.session.task_mode,
        Vec::new(),
        None,
        false,
    )
    .await
}

pub(crate) async fn fork_command(storage: &Storage, args: ForkArgs) -> Result<()> {
    let transcript = load_session_for_command(storage, args.session_id, args.last, args.all)?;
    let new_session_id = fork_session(storage, &transcript)?;
    println!(
        "Forked session {} ({}) -> {}",
        transcript.session.id,
        transcript.session.title.as_deref().unwrap_or("(untitled)"),
        new_session_id
    );
    launch_chat_session(
        storage,
        Some(transcript.session.alias),
        Some(new_session_id),
        args.prompt,
        resolve_thinking_level(storage, args.thinking)?,
        transcript.session.task_mode,
        Vec::new(),
        None,
        false,
    )
    .await
}

pub(crate) fn completion_command(args: CompletionArgs) {
    let mut command = Cli::command();
    generate(
        args.shell,
        &mut command,
        PRIMARY_COMMAND_NAME,
        &mut io::stdout(),
    );
}

