pub(crate) async fn logout_command(storage: &Storage, args: LogoutArgs) -> Result<()> {
    let mut config = storage.load_config()?;
    let provider_ids = determine_logout_targets(&config, &args)?;
    let mut removed = 0usize;

    for provider_id in provider_ids {
        let Some(provider) = config
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
        else {
            continue;
        };

        if let Some(account) = provider.keychain_account.take() {
            delete_secret(&account)?;
            removed += 1;
        }
    }

    storage.save_config(&config)?;
    println!("Removed stored credentials for {} provider(s).", removed);
    Ok(())
}

pub(crate) async fn run_onboarding_reset(
    storage: &Storage,
    require_confirmation: bool,
) -> Result<()> {
    if require_confirmation {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            bail!(
                "reset is destructive; rerun with `{} reset --yes` in an interactive terminal",
                PRIMARY_COMMAND_NAME
            );
        }

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("This will wipe saved config, sessions, logs, and credentials. Continue?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Reset cancelled.");
            return Ok(());
        }
    }

    stop_daemon_for_reset(storage).await?;

    let config = storage.load_config()?;
    storage.sync_autostart(&current_executable_path()?, &[INTERNAL_DAEMON_ARG], false)?;

    let keychain_accounts = configured_keychain_accounts(&config);
    let mut removed_credentials = 0usize;
    let mut credential_warnings = Vec::new();
    for account in keychain_accounts {
        match delete_secret(&account) {
            Ok(()) => removed_credentials += 1,
            Err(error) => credential_warnings.push(format!("{account}: {error}")),
        }
    }

    storage.reset_all()?;

    println!(
        "Reset complete. Cleared configuration, sessions, logs, and {} credential entry(s).",
        removed_credentials
    );
    for warning in credential_warnings {
        println!("warning: failed to delete keychain entry {warning}");
    }

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!(
            "Run `{} setup` in an interactive terminal to complete onboarding again.",
            PRIMARY_COMMAND_NAME
        );
        return Ok(());
    }

    println!();
    println!("Restarting onboarding.");
    setup(storage).await
}

pub(crate) async fn reset_command(storage: &Storage, args: ResetArgs) -> Result<()> {
    run_onboarding_reset(storage, !args.yes).await
}

pub(crate) async fn ensure_daemon(storage: &Storage) -> Result<DaemonClient> {
    if let Some(client) = try_daemon(storage).await? {
        return Ok(client);
    }

    start_daemon_process()?;
    let config = storage.load_config()?;
    wait_for_daemon(&config).await?;
    Ok(DaemonClient::new(&config))
}

pub(crate) async fn stop_daemon_for_reset(storage: &Storage) -> Result<()> {
    let Some(client) = try_daemon(storage).await? else {
        return Ok(());
    };

    let _: serde_json::Value = client.post("/v1/shutdown", &serde_json::json!({})).await?;
    for _ in 0..20 {
        if try_daemon(storage).await?.is_none() {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }

    bail!(
        "daemon did not stop in time; run `{} daemon stop` and retry reset",
        PRIMARY_COMMAND_NAME
    )
}

pub(crate) fn configured_keychain_accounts(config: &AppConfig) -> BTreeSet<String> {
    let mut accounts = config
        .providers
        .iter()
        .filter_map(|provider| provider.keychain_account.clone())
        .collect::<BTreeSet<_>>();
    accounts.extend(
        config
            .telegram_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .discord_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .slack_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .home_assistant_connectors
            .iter()
            .filter_map(|connector| connector.access_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .brave_connectors
            .iter()
            .filter_map(|connector| connector.api_key_keychain_account.clone()),
    );
    accounts.extend(
        config
            .gmail_connectors
            .iter()
            .filter_map(|connector| connector.oauth_keychain_account.clone()),
    );
    accounts
}

pub(crate) async fn try_daemon(storage: &Storage) -> Result<Option<DaemonClient>> {
    let config = storage.load_config()?;
    let client = DaemonClient::new(&config);
    if client.get::<DaemonStatus>("/v1/status").await.is_ok() {
        Ok(Some(client))
    } else {
        Ok(None)
    }
}

pub(crate) async fn wait_for_daemon(config: &AppConfig) -> Result<()> {
    let client = DaemonClient::new(config);
    for _ in 0..20 {
        if client.get::<DaemonStatus>("/v1/status").await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(300)).await;
    }

    bail!("daemon did not become ready in time")
}

pub(crate) fn start_daemon_process() -> Result<()> {
    let current_exe = current_executable_path()?;
    let mut command = Command::new(&current_exe);
    command
        .arg(INTERNAL_DAEMON_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().with_context(|| {
        format!(
            "failed to start daemon using {} {}",
            current_exe.display(),
            INTERNAL_DAEMON_ARG
        )
    })?;
    Ok(())
}

pub(crate) fn current_executable_path() -> Result<PathBuf> {
    std::env::current_exe().context("failed to locate current executable")
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_prompt(
    client: &DaemonClient,
    prompt: String,
    alias: Option<String>,
    requested_model: Option<String>,
    session_id: Option<String>,
    cwd: PathBuf,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    attachments: Vec<InputAttachment>,
    permission_preset: Option<PermissionPreset>,
    output_schema_json: Option<String>,
    ephemeral: bool,
) -> Result<RunTaskResponse> {
    client
        .post(
            "/v1/run",
            &RunTaskRequest {
                prompt,
                alias,
                requested_model,
                session_id,
                cwd: Some(cwd),
                thinking_level,
                task_mode,
                attachments,
                permission_preset,
                output_schema_json,
                ephemeral,
                remote_content_policy_override: None,
            },
        )
        .await
}

pub(crate) fn current_request_cwd() -> Result<PathBuf> {
    std::env::current_dir().context("failed to resolve current working directory")
}

pub(crate) fn load_session_cwd(storage: &Storage, session_id: Option<&str>) -> Result<Option<PathBuf>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    Ok(storage
        .get_session(session_id)?
        .and_then(|session| session.cwd))
}

pub(crate) fn load_session_task_mode(storage: &Storage, session_id: Option<&str>) -> Result<Option<TaskMode>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    Ok(storage
        .get_session(session_id)?
        .and_then(|session| session.task_mode))
}

pub(crate) fn resolve_thinking_level(
    storage: &Storage,
    thinking: Option<ThinkingLevelArg>,
) -> Result<Option<ThinkingLevel>> {
    match thinking {
        Some(thinking) => Ok(Some(thinking.into())),
        None => Ok(storage.load_config()?.thinking_level),
    }
}

pub(crate) fn persist_thinking_level(storage: &Storage, thinking_level: Option<ThinkingLevel>) -> Result<()> {
    let mut config = storage.load_config()?;
    config.thinking_level = thinking_level;
    storage.save_config(&config)
}

pub(crate) fn resolve_mission_wake_at(
    after_seconds: Option<u64>,
    at: Option<&str>,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    if after_seconds.is_some() && at.is_some() {
        bail!("use either --after-seconds or --at, not both");
    }

    if let Some(seconds) = after_seconds {
        return Ok(Some(
            chrono::Utc::now() + chrono::Duration::seconds(seconds as i64),
        ));
    }

    let Some(at) = at else {
        return Ok(None);
    };

    chrono::DateTime::parse_from_rfc3339(at)
        .map(|value| value.with_timezone(&chrono::Utc))
        .with_context(|| format!("invalid RFC3339 timestamp '{at}'"))
        .map(Some)
}

pub(crate) fn resolve_watch_path(path: Option<&Path>, cwd: &Path) -> Result<Option<PathBuf>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    Ok(Some(absolute))
}

pub(crate) fn event_feed_path(
    cursor: Option<&chrono::DateTime<chrono::Utc>>,
    limit: usize,
    wait_seconds: u64,
) -> String {
    let mut path = format!("/v1/events?limit={limit}&wait_seconds={wait_seconds}");
    if let Some(cursor) = cursor {
        let encoded: String =
            form_urlencoded::byte_serialize(cursor.to_rfc3339().as_bytes()).collect();
        path.push_str("&after=");
        path.push_str(&encoded);
    }
    path
}

pub(crate) fn print_log_entry(entry: &agent_core::LogEntry) {
    println!(
        "{} [{}] {} {}",
        entry.created_at, entry.level, entry.scope, entry.message
    );
}

pub(crate) fn thinking_level_label(level: Option<ThinkingLevel>) -> &'static str {
    match level {
        None => "default",
        Some(level) => level.as_str(),
    }
}

pub(crate) fn task_mode_label(mode: Option<TaskMode>) -> &'static str {
    match mode {
        None => "default",
        Some(mode) => mode.as_str(),
    }
}

pub(crate) fn autopilot_summary(config: &AutopilotConfig) -> String {
    format!(
        "autopilot={} interval={}s concurrency={} shell={} network={} self_edit={}",
        match config.state {
            AutopilotState::Disabled => "disabled",
            AutopilotState::Enabled => "enabled",
            AutopilotState::Paused => "paused",
        },
        config.wake_interval_seconds,
        config.max_concurrent_missions,
        config.allow_background_shell,
        config.allow_background_network,
        config.allow_background_self_edit
    )
}

pub(crate) fn manual_memory_subject(content: &str) -> String {
    let slug = content
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "memory".to_string()
    } else {
        format!("memory:{slug}")
    }
}

pub(crate) fn parse_permission_preset(value: &str) -> Result<PermissionPreset> {
    match value.to_ascii_lowercase().as_str() {
        "suggest" => Ok(PermissionPreset::Suggest),
        "auto-edit" | "auto_edit" | "autoedit" => Ok(PermissionPreset::AutoEdit),
        "full-auto" | "full_auto" | "fullauto" => Ok(PermissionPreset::FullAuto),
        _ => bail!("unknown permission preset '{value}'"),
    }
}

pub(crate) fn resolve_active_alias<'a>(
    config: &'a AppConfig,
    alias: Option<&'a str>,
) -> Result<&'a ModelAlias> {
    if let Some(alias) = alias {
        return config
            .get_alias(alias)
            .ok_or_else(|| anyhow!("unknown alias '{alias}'"));
    }
    config.main_alias()
}

pub(crate) fn resolved_requested_model<'a>(
    active_alias: &'a ModelAlias,
    requested_model: Option<&'a str>,
) -> &'a str {
    requested_model.unwrap_or(active_alias.model.as_str())
}

pub(crate) async fn print_permissions_status(storage: &Storage, client: &DaemonClient) -> Result<()> {
    let config = storage.load_config()?;
    let autonomy: agent_core::AutonomyProfile = client.get("/v1/autonomy/status").await?;
    let preset: PermissionPreset = client.get("/v1/permissions").await?;
    println!("{}", trust_summary(&config.trust_policy));
    println!("permission_preset={}", permission_summary(preset));
    println!("autonomy={}", autonomy_summary(autonomy.state));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn print_interactive_status(
    storage: &Storage,
    client: &DaemonClient,
    alias: Option<&str>,
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    permission_preset: Option<PermissionPreset>,
    attachments: &[InputAttachment],
    cwd: &Path,
) -> Result<()> {
    let config = storage.load_config()?;
    let current_session = session_id.and_then(|id| storage.get_session(id).ok().flatten());
    let active_alias = resolve_active_alias(&config, alias)?;
    let provider = config
        .resolve_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let selected_model = resolved_requested_model(active_alias, requested_model);
    let daemon_status: DaemonStatus = client.get("/v1/status").await?;
    println!("session={}", session_id.unwrap_or("(new)"));
    if let Some(session) = current_session {
        println!("title={}", session.title.as_deref().unwrap_or("(untitled)"));
    }
    println!("alias={}", active_alias.alias);
    println!("provider={}", provider.id);
    println!("model={}", selected_model);
    if let Some(main_target) = daemon_status.main_target.as_ref() {
        println!(
            "main={} ({}/{})",
            main_target.alias, main_target.provider_id, main_target.model
        );
    }
    println!("thinking={}", thinking_level_label(thinking_level));
    println!("mode={}", task_mode_label(task_mode));
    println!(
        "permission_preset={}",
        permission_summary(permission_preset.unwrap_or(config.permission_preset))
    );
    println!("attachments={}", attachments.len());
    println!("cwd={}", cwd.display());
    println!(
        "daemon={} auto_start={} autonomy={} autopilot={} active_missions={} memories={}",
        match daemon_status.persistence_mode {
            PersistenceMode::OnDemand => "on-demand",
            PersistenceMode::AlwaysOn => "always-on",
        },
        daemon_status.auto_start,
        autonomy_summary(daemon_status.autonomy.state),
        match daemon_status.autopilot.state {
            AutopilotState::Disabled => "disabled",
            AutopilotState::Enabled => "enabled",
            AutopilotState::Paused => "paused",
        },
        daemon_status.active_missions,
        daemon_status.memories
    );
    println!("{}", trust_summary(&config.trust_policy));
    Ok(())
}

pub(crate) async fn run_bang_command(storage: &Storage, command: &str, cwd: &mut PathBuf) -> Result<String> {
    if command.is_empty() {
        bail!("shell command is empty");
    }
    if command == "cd" {
        *cwd = home_dir().ok_or_else(|| anyhow!("failed to resolve home directory"))?;
        return Ok(format!("cwd={}", cwd.display()));
    }
    if let Some(target) = command.strip_prefix("cd ") {
        let target = target.trim();
        if target.is_empty() {
            bail!("cd target is empty");
        }
        let next = resolve_shell_cd_target(cwd, target)?;
        *cwd = next;
        return Ok(format!("cwd={}", cwd.display()));
    }

    let config = storage.load_config()?;
    if !allow_shell(&config.trust_policy, &config.autonomy) {
        bail!("shell access is disabled by the current trust policy");
    }
    execute_local_shell_command(command, cwd).await
}

pub(crate) fn resolve_shell_cd_target(current: &Path, target: &str) -> Result<PathBuf> {
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
        bail!("{} is not a directory", canonical.display());
    }
    Ok(canonical)
}

pub(crate) async fn execute_local_shell_command(command: &str, cwd: &Path) -> Result<String> {
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

    let output = timeout(Duration::from_secs(60), process.output())
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
    Ok(truncate_for_prompt(text, 20_000))
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

pub(crate) fn normalize_prompt_input(prompt: Option<String>) -> Result<Option<String>> {
    let Some(prompt) = prompt else {
        return Ok(None);
    };
    if prompt != "-" {
        return Ok(Some(prompt));
    }

    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .context("failed to read prompt from stdin")?;
    let prompt = buffer.trim().to_string();
    if prompt.is_empty() {
        bail!("no prompt provided via stdin");
    }
    Ok(Some(prompt))
}

pub(crate) fn truncate_for_prompt(text: String, max_len: usize) -> String {
    agent_core::truncate_with_suffix(&text, max_len, "\n\n[truncated]")
}

pub(crate) fn determine_logout_targets(config: &AppConfig, args: &LogoutArgs) -> Result<Vec<String>> {
    if args.all {
        return Ok(config
            .providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect());
    }
    if let Some(provider) = &args.provider {
        return Ok(vec![provider.clone()]);
    }
    if let Some(alias_name) = config.main_agent_alias.as_deref() {
        if let Some(alias) = config.get_alias(alias_name) {
            return Ok(vec![alias.provider_id.clone()]);
        }
    }
    bail!("no provider specified and no main provider configured")
}

pub(crate) fn parse_task(value: String) -> Result<SubAgentTask> {
    let (target, prompt) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("task must use target=prompt format"))?;
    Ok(SubAgentTask {
        prompt: prompt.trim().to_string(),
        target: Some(target.trim().to_string()),
        alias: None,
        provider_id: None,
        requested_model: None,
        cwd: None,
        thinking_level: None,
        task_mode: None,
        output_schema_json: None,
        strategy: None,
    })
}

pub(crate) async fn upsert_provider_with_optional_alias(
    storage: &Storage,
    request: ProviderUpsertRequest,
    main_alias: Option<String>,
    model: String,
) -> Result<()> {
    let provider_id = request.provider.id.clone();
    if let Some(client) = try_daemon(storage).await? {
        let _: ProviderConfig = client.post("/v1/providers", &request).await?;
        if let Some(alias) = main_alias {
            set_alias(&client, alias, provider_id.clone(), model, true).await?;
        }
    } else {
        let mut config = storage.load_config()?;
        apply_provider_request_locally(&mut config, &request)?;
        if let Some(alias) = main_alias {
            config.main_agent_alias = Some(alias.clone());
            config.upsert_alias(ModelAlias {
                alias,
                provider_id: provider_id.clone(),
                model,
                description: None,
            });
        }
        storage.save_config(&config)?;
    }

    println!("provider '{}' configured", provider_id);
    Ok(())
}

pub(crate) fn apply_provider_request_locally(
    config: &mut AppConfig,
    request: &ProviderUpsertRequest,
) -> Result<()> {
    let mut provider = request.provider.clone();
    if let Some(api_key) = &request.api_key {
        provider.keychain_account = Some(store_api_key(&provider.id, api_key)?);
    }
    if let Some(token) = &request.oauth_token {
        provider.keychain_account = Some(store_oauth_token(&provider.id, token)?);
    }
    config.upsert_provider(provider);
    Ok(())
}

pub(crate) fn apply_trust_update(policy: &mut TrustPolicy, update: &TrustUpdateRequest) {
    if let Some(allow_shell) = update.allow_shell {
        policy.allow_shell = allow_shell;
    }
    if let Some(allow_network) = update.allow_network {
        policy.allow_network = allow_network;
    }
    if let Some(allow_full_disk) = update.allow_full_disk {
        policy.allow_full_disk = allow_full_disk;
    }
    if let Some(allow_self_edit) = update.allow_self_edit {
        policy.allow_self_edit = allow_self_edit;
    }
    if let Some(path) = &update.trusted_path {
        if !policy.trusted_paths.contains(path) {
            policy.trusted_paths.push(path.clone());
        }
    }
}

pub(crate) fn resolve_main_alias(storage: &Storage, requested: Option<String>) -> Result<Option<String>> {
    let config = storage.load_config()?;
    Ok(default_main_alias(&config, requested))
}

pub(crate) fn default_main_alias(config: &AppConfig, requested: Option<String>) -> Option<String> {
    requested.or_else(|| {
        if config.main_agent_alias.is_none() && config.aliases.is_empty() {
            Some("main".to_string())
        } else {
            None
        }
    })
}

pub(crate) struct OAuthCallback {
    code: String,
    state: String,
}

pub(crate) struct BrowserCodeCallback {
    code: String,
}

pub(crate) async fn set_alias(
    client: &DaemonClient,
    alias: String,
    provider: String,
    model: String,
    set_main: bool,
) -> Result<()> {
    let payload = AliasUpsertRequest {
        alias: ModelAlias {
            alias: alias.clone(),
            provider_id: provider,
            model,
            description: None,
        },
        set_as_main: set_main,
    };
    let _: ModelAlias = client.post("/v1/aliases", &payload).await?;

    println!("alias '{}' configured", alias);
    Ok(())
}

#[derive(Clone)]
pub(crate) struct DaemonClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

impl DaemonClient {
    fn new(config: &AppConfig) -> Self {
        Self {
            base_url: format!("http://{}:{}", config.daemon.host, config.daemon.port),
            token: config.daemon.token.clone(),
            http: build_http_client(),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, Option::<&()>::None).await
    }

    async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        self.request(Method::POST, path, Some(body)).await
    }

    async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        self.request(Method::PUT, path, Some(body)).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::DELETE, path, Option::<&()>::None)
            .await
    }

    async fn post_stream<B, T, F>(&self, path: &str, body: &B, mut on_event: F) -> Result<()>
    where
        B: Serialize,
        T: DeserializeOwned,
        F: FnMut(T) -> Result<()>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .request(Method::POST, &url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("daemon returned {}: {}", status, body);
        }
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.with_context(|| format!("failed to read streamed response from {url}"))?;
            buffer.extend_from_slice(&chunk);
            drain_ndjson_buffer(&mut buffer, false, &mut on_event)
                .with_context(|| format!("failed to parse streamed daemon response from {url}"))?;
        }
        drain_ndjson_buffer(&mut buffer, true, &mut on_event)
            .with_context(|| format!("failed to parse streamed daemon response from {url}"))?;
        Ok(())
    }

    async fn request<T: DeserializeOwned, B: Serialize>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.request(method, &url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .with_context(|| format!("request failed: {url}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("daemon returned {}: {}", status, body);
        }
        response
            .json::<T>()
            .await
            .with_context(|| format!("failed to parse daemon response from {url}"))
    }
}

pub(crate) fn drain_ndjson_buffer<T, F>(
    buffer: &mut Vec<u8>,
    flush_trailing: bool,
    on_event: &mut F,
) -> Result<()>
where
    T: DeserializeOwned,
    F: FnMut(T) -> Result<()>,
{
    while let Some(newline_index) = buffer.iter().position(|byte| *byte == b'\n') {
        let line_bytes = buffer[..newline_index].to_vec();
        buffer.drain(..=newline_index);
        let line = std::str::from_utf8(&line_bytes)?.trim();
        if line.is_empty() {
            continue;
        }
        let value = serde_json::from_str::<T>(line)?;
        on_event(value)?;
    }

    if flush_trailing {
        if buffer.iter().all(|byte| byte.is_ascii_whitespace()) {
            buffer.clear();
            return Ok(());
        }
        let trailing = std::str::from_utf8(buffer)?.trim();
        if trailing.is_empty() {
            buffer.clear();
            return Ok(());
        }
        let value = serde_json::from_str::<T>(trailing)?;
        on_event(value)?;
        buffer.clear();
    }

    Ok(())
}
