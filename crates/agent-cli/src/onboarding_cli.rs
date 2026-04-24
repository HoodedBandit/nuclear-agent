use super::*;

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
    println!("+{}+", "-".repeat(width));
    for line in lines {
        println!("| {:width$} |", line, width = width.saturating_sub(1));
    }
    println!("+{}+", "-".repeat(width));
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
