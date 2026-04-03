async fn run(cli: Cli) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("warn")
        .with_target(false)
        .compact()
        .init();

    if matches!(cli.command, Some(Commands::InternalDaemon)) {
        return agent_daemon::run_daemon().await;
    }
    if let Some(cwd) = &cli.cwd {
        std::env::set_current_dir(cwd)
            .with_context(|| format!("failed to change directory to {}", cwd.display()))?;
    }
    let storage = Storage::open()?;

    match cli.command {
        None => {
            launch_chat_session(
                &storage,
                None,
                None,
                cli.prompt,
                None,
                None,
                Vec::new(),
                None,
                false,
            )
            .await?
        }
        Some(Commands::Exec(args)) => run_command(&storage, args).await?,
        Some(Commands::Review(args)) => review_command(&storage, args).await?,
        Some(Commands::Resume(args)) => resume_command(&storage, args).await?,
        Some(Commands::Fork(args)) => fork_command(&storage, args).await?,
        Some(Commands::Completion(args)) => completion_command(args),
        Some(Commands::Logout(args)) => logout_command(&storage, args).await?,
        Some(Commands::Reset(args)) => reset_command(&storage, args).await?,
        Some(Commands::Setup) => setup(&storage).await?,
        Some(Commands::Daemon { command }) => daemon_command(&storage, command).await?,
        Some(Commands::Login(args)) => login_command(&storage, args).await?,
        Some(Commands::Provider { command }) => provider_command(&storage, command).await?,
        Some(Commands::Mcp { command }) => mcp_command(&storage, command).await?,
        Some(Commands::App { command }) => app_command(&storage, command).await?,
        Some(Commands::Plugin { command }) => plugin_command(&storage, command).await?,
        Some(Commands::Repo { command }) => repo_command(&storage, command).await?,
        Some(Commands::Telegram { command }) => telegram_command(&storage, command).await?,
        Some(Commands::Discord { command }) => discord_command(&storage, command).await?,
        Some(Commands::Slack { command }) => slack_command(&storage, command).await?,
        Some(Commands::Signal { command }) => signal_command(&storage, command).await?,
        Some(Commands::HomeAssistant { command }) => {
            home_assistant_command(&storage, command).await?
        }
        Some(Commands::Webhook { command }) => webhook_command(&storage, command).await?,
        Some(Commands::Inbox { command }) => inbox_command(&storage, command).await?,
        Some(Commands::Skills { command }) => skills_command(&storage, command).await?,
        Some(Commands::Model { command }) => model_command(&storage, command).await?,
        Some(Commands::Alias { command }) => alias_command(&storage, command).await?,
        Some(Commands::Permissions(args)) => permissions_command(&storage, args).await?,
        Some(Commands::Trust(args)) => trust_command(&storage, args).await?,
        Some(Commands::Run(args)) => run_command(&storage, args).await?,
        Some(Commands::Chat(args)) => chat_command(&storage, args).await?,
        Some(Commands::Session { command }) => session_command(&storage, command).await?,
        Some(Commands::Autonomy { command }) => autonomy_command(&storage, command).await?,
        Some(Commands::Evolve { command }) => evolve_command(&storage, command).await?,
        Some(Commands::Autopilot { command }) => autopilot_command(&storage, command).await?,
        Some(Commands::Mission { command }) => mission_command(&storage, command).await?,
        Some(Commands::Memory { command }) => memory_command(&storage, command).await?,
        Some(Commands::Logs { limit, follow }) => logs_command(&storage, limit, follow).await?,
        Some(Commands::Dashboard(args)) => dashboard_command(&storage, args).await?,
        Some(Commands::Doctor) => doctor_command(&storage).await?,
        Some(Commands::SupportBundle(args)) => support_bundle_command(&storage, args).await?,
        Some(Commands::InternalDaemon) => unreachable!(),
    }

    Ok(())
}

pub(crate) async fn setup(storage: &Storage) -> Result<()> {
    let theme = ColorfulTheme::default();
    let mut config = storage.load_config()?;
    print_onboarding_banner_clean(&config)?;
    let cwd = current_request_cwd()?;

    let mode_idx = Select::with_theme(&theme)
        .with_prompt("How should the agent daemon run?")
        .items([
            "On-demand (starts when you use the CLI)",
            "Always-on (persistent daemon)",
        ])
        .default(matches!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn) as usize)
        .interact()?;
    config.daemon.persistence_mode = if mode_idx == 0 {
        PersistenceMode::OnDemand
    } else {
        PersistenceMode::AlwaysOn
    };
    config.daemon.auto_start =
        if matches!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn) {
            Confirm::with_theme(&theme)
                .with_prompt("Enable auto-start on boot/login?")
                .default(config.daemon.auto_start)
                .interact()?
        } else {
            println!("Auto-start is only available in always-on mode.");
            false
        };

    if needs_onboarding(&config) {
        println!();
        println!(
            "A main model must be configured before {} can start.",
            PRIMARY_COMMAND_NAME
        );
        let (request, alias) = interactive_provider_setup(&theme, &config).await?;
        apply_provider_request_locally(&mut config, &request)?;
        config.main_agent_alias = Some(alias.alias.clone());
        config.upsert_alias(alias);
    } else if Confirm::with_theme(&theme)
        .with_prompt("Configure another provider now?")
        .default(false)
        .interact()?
    {
        let (request, alias) = interactive_provider_setup(&theme, &config).await?;
        apply_provider_request_locally(&mut config, &request)?;
        if config.main_agent_alias.is_none() {
            config.main_agent_alias = Some(alias.alias.clone());
        }
        config.upsert_alias(alias);
    }

    if Confirm::with_theme(&theme)
        .with_prompt(format!(
            "Trust the current directory for project file access? ({})",
            cwd.display()
        ))
        .default(
            config.trust_policy.trusted_paths.is_empty()
                || config
                    .trust_policy
                    .trusted_paths
                    .iter()
                    .any(|path| path == &cwd),
        )
        .interact()?
        && !config
            .trust_policy
            .trusted_paths
            .iter()
            .any(|path| path == &cwd)
    {
        config.trust_policy.trusted_paths.push(cwd.clone());
    }

    config.permission_preset = select_permission_preset(&theme, config.permission_preset)?;
    config.trust_policy.allow_shell = Confirm::with_theme(&theme)
        .with_prompt("Allow shell commands by default inside trusted workspaces?")
        .default(config.trust_policy.allow_shell)
        .interact()?;
    config.trust_policy.allow_network = Confirm::with_theme(&theme)
        .with_prompt("Allow general network tools by default?")
        .default(config.trust_policy.allow_network)
        .interact()?;

    if !has_usable_main_alias(&config) {
        bail!("setup was not completed with a usable main alias");
    }

    config.onboarding_complete = true;
    storage.save_config(&config)?;
    storage.sync_autostart(
        &current_executable_path()?,
        &[INTERNAL_DAEMON_ARG],
        config.daemon.auto_start,
    )?;

    println!(
        "Saved configuration to {}",
        storage.paths().config_path.display()
    );
    println!(
        "Persistence mode: {:?}, auto-start: {}",
        config.daemon.persistence_mode, config.daemon.auto_start
    );

    if Confirm::with_theme(&theme)
        .with_prompt("Start the daemon now?")
        .default(true)
        .interact()?
    {
        start_daemon_process()?;
        wait_for_daemon(&config).await?;
        println!("Daemon started.");
    }

    Ok(())
}

pub(crate) fn needs_onboarding(config: &AppConfig) -> bool {
    !config.onboarding_complete || !has_usable_main_alias(config)
}

pub(crate) fn has_usable_main_alias(config: &AppConfig) -> bool {
    config
        .main_alias()
        .ok()
        .and_then(|alias| config.resolve_provider(&alias.provider_id))
        .is_some_and(|provider| provider_has_saved_access(&provider))
}

pub(crate) async fn ensure_onboarded(storage: &Storage) -> Result<()> {
    let config = storage.load_config()?;
    if !needs_onboarding(&config) {
        return Ok(());
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!(
            "no completed setup found; run `{} setup` in an interactive terminal first",
            PRIMARY_COMMAND_NAME
        );
    }

    println!("No completed setup found. Launching onboarding.");
    setup(storage).await?;

    let updated = storage.load_config()?;
    if needs_onboarding(&updated) {
        bail!("onboarding did not finish with a usable main model");
    }

    Ok(())
}

#[allow(dead_code)]
pub(crate) fn print_onboarding_banner(config: &AppConfig) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let directory = current_request_cwd()?;
    let model_label = config
        .main_agent_alias
        .as_deref()
        .unwrap_or("not configured");
    let lines = [
        format!(" >_ {DISPLAY_APP_NAME} CLI (v{version})"),
        String::new(),
        format!(" model:     {model_label}"),
        format!(" directory: {}", directory.display()),
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0) + 2;
    println!("â•­{}â•®", "â”€".repeat(width));
    for line in lines {
        println!("â”‚ {:width$} â”‚", line, width = width.saturating_sub(1));
    }
    println!("â•°{}â•¯", "â”€".repeat(width));
    Ok(())
}

pub(crate) fn select_permission_preset(
    theme: &ColorfulTheme,
    current: PermissionPreset,
) -> Result<PermissionPreset> {
    let items = [
        "Suggest (read-only tools only)",
        "Auto-edit (edit files without shell/network)",
        "Full-auto (all tools enabled by default)",
    ];
    let default_index = match current {
        PermissionPreset::Suggest => 0,
        PermissionPreset::AutoEdit => 1,
        PermissionPreset::FullAuto => 2,
    };
    let selection = Select::with_theme(theme)
        .with_prompt("Choose the default permission preset")
        .items(items)
        .default(default_index)
        .interact()?;
    Ok(match selection {
        0 => PermissionPreset::Suggest,
        1 => PermissionPreset::AutoEdit,
        2 => PermissionPreset::FullAuto,
        _ => unreachable!("invalid permission selection"),
    })
}

pub(crate) fn print_onboarding_banner_clean(config: &AppConfig) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let directory = current_request_cwd()?;
    let model_label = config
        .main_agent_alias
        .as_deref()
        .unwrap_or("not configured");
    let lines = [
        format!(" >_ {DISPLAY_APP_NAME} CLI (v{version})"),
        String::new(),
        format!(" model:     {model_label}"),
        format!(" directory: {}", directory.display()),
    ];
    let width = lines.iter().map(|line| line.len()).max().unwrap_or(0) + 2;
    println!(".{}.", "-".repeat(width));
    for line in lines {
        println!("| {:width$} |", line, width = width.saturating_sub(1));
    }
    println!("'{}'", "-".repeat(width));
    Ok(())
}

