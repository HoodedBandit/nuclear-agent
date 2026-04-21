use super::*;

pub(crate) async fn daemon_command(storage: &Storage, command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Start => {
            if try_daemon(storage).await?.is_some() {
                println!("Daemon already running.");
                return Ok(());
            }
            start_daemon_process()?;
            let config = storage.load_config()?;
            wait_for_daemon(&config).await?;
            println!(
                "Daemon started at http://{}:{}",
                config.daemon.host, config.daemon.port
            );
        }
        DaemonCommands::Stop => {
            let Some(client) = try_daemon(storage).await? else {
                println!("Daemon is not running.");
                return Ok(());
            };
            let _: serde_json::Value = client.post("/v1/shutdown", &serde_json::json!({})).await?;
            println!("Daemon stop requested.");
        }
        DaemonCommands::Status => {
            let Some(client) = try_daemon(storage).await? else {
                let config = storage.load_config()?;
                println!("running: false");
                println!("persistence: {:?}", config.daemon.persistence_mode);
                println!("auto_start: {}", config.daemon.auto_start);
                return Ok(());
            };
            let status: DaemonStatus = client.get("/v1/status").await?;
            println!("running: true");
            println!("pid: {}", status.pid);
            println!("started_at: {}", status.started_at);
            println!("persistence: {:?}", status.persistence_mode);
            println!("auto_start: {}", status.auto_start);
            println!("autonomy: {}", autonomy_summary(status.autonomy.state));
            println!("providers: {}", status.providers);
            println!("aliases: {}", status.aliases);
            println!("plugins: {}", status.plugins);
            println!("webhooks: {}", status.webhook_connectors);
            println!("inboxes: {}", status.inbox_connectors);
            println!("telegram: {}", status.telegram_connectors);
            println!("discord: {}", status.discord_connectors);
            println!(
                "pending_connector_approvals: {}",
                status.pending_connector_approvals
            );
            println!("missions: {}", status.missions);
            println!("active_missions: {}", status.active_missions);
            println!("memories: {}", status.memories);
            println!("pending_memory_reviews: {}", status.pending_memory_reviews);
            println!("skill_drafts: {}", status.skill_drafts);
            println!("published_skills: {}", status.published_skills);
        }
        DaemonCommands::Config(args) => {
            if args.mode.is_none() && args.auto_start.is_none() {
                bail!("provide --mode and/or --auto-start");
            }
            let current_config = storage.load_config()?;
            let next_mode = args
                .mode
                .map(Into::into)
                .unwrap_or(current_config.daemon.persistence_mode);
            let next_auto_start = args.auto_start.unwrap_or(current_config.daemon.auto_start);
            if matches!(next_mode, PersistenceMode::OnDemand) && next_auto_start {
                bail!("auto-start requires always-on daemon mode");
            }
            if let Some(client) = try_daemon(storage).await? {
                let config: agent_core::DaemonConfig = client
                    .put(
                        "/v1/daemon/config",
                        &DaemonConfigUpdateRequest {
                            persistence_mode: args.mode.map(Into::into),
                            auto_start: args.auto_start,
                        },
                    )
                    .await?;
                println!(
                    "daemon config updated: mode={:?}, auto_start={}",
                    config.persistence_mode, config.auto_start
                );
            } else {
                let mut config = storage.load_config()?;
                if let Some(mode) = args.mode {
                    config.daemon.persistence_mode = mode.into();
                }
                if let Some(auto_start) = args.auto_start {
                    config.daemon.auto_start = auto_start;
                }
                storage.save_config(&config)?;
                storage.sync_autostart(
                    &current_executable_path()?,
                    &[INTERNAL_DAEMON_ARG],
                    config.daemon.auto_start,
                )?;
                println!(
                    "daemon config updated locally: mode={:?}, auto_start={}",
                    config.daemon.persistence_mode, config.daemon.auto_start
                );
            }
        }
    }

    Ok(())
}

pub(crate) async fn login_command(storage: &Storage, args: LoginArgs) -> Result<()> {
    let theme = ColorfulTheme::default();
    let kind = args.kind.unwrap_or(select_hosted_kind(&theme)?);
    let (default_provider_id, default_provider_name) = hosted_kind_defaults(kind);
    let provider_id = args.id.unwrap_or_else(|| default_provider_id.to_string());
    let provider_name = args
        .name
        .unwrap_or_else(|| default_provider_name.to_string());
    let auth_method = match args.auth {
        Some(auth) => auth,
        None => select_auth_method(&theme, kind)?,
    };
    let default_url = match auth_method {
        AuthMethodArg::Browser => default_browser_hosted_url(kind),
        AuthMethodArg::ApiKey | AuthMethodArg::Oauth => default_hosted_url(kind),
    };
    let base_url = args.base_url.unwrap_or_else(|| default_url.to_string());
    let main_alias = resolve_main_alias(storage, args.main_alias)?;

    let mut request = match auth_method {
        AuthMethodArg::Browser => match complete_browser_login(kind, &provider_name).await? {
            BrowserLoginResult::ApiKey(api_key) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: provider_id.clone(),
                    display_name: provider_name,
                    kind: hosted_kind_to_provider_kind(kind),
                    base_url,
                    auth_mode: AuthMode::ApiKey,
                    default_model: None,
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                api_key: Some(api_key),
                oauth_token: None,
            },
            BrowserLoginResult::OAuthToken(token) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: provider_id.clone(),
                    display_name: provider_name,
                    kind: browser_hosted_kind_to_provider_kind(kind),
                    base_url,
                    auth_mode: AuthMode::OAuth,
                    default_model: None,
                    keychain_account: None,
                    oauth: Some(openai_browser_oauth_config()),
                    local: false,
                },
                api_key: None,
                oauth_token: Some(token),
            },
        },
        AuthMethodArg::ApiKey => ProviderUpsertRequest {
            provider: ProviderConfig {
                id: provider_id.clone(),
                display_name: provider_name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                auth_mode: AuthMode::ApiKey,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: false,
            },
            api_key: Some(match args.api_key {
                Some(api_key) => api_key,
                None => Password::with_theme(&theme)
                    .with_prompt("API key")
                    .allow_empty_password(false)
                    .interact()?,
            }),
            oauth_token: None,
        },
        AuthMethodArg::Oauth => {
            let provider = ProviderConfig {
                id: provider_id.clone(),
                display_name: provider_name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                auth_mode: AuthMode::OAuth,
                default_model: None,
                keychain_account: None,
                oauth: Some(OAuthConfig {
                    client_id: prompt_or_value(&theme, "OAuth client id", args.client_id, None)?,
                    authorization_url: prompt_or_value(
                        &theme,
                        "OAuth authorization URL",
                        args.authorization_url,
                        None,
                    )?,
                    token_url: prompt_or_value(&theme, "OAuth token URL", args.token_url, None)?,
                    scopes: collect_scopes(&theme, args.scopes)?,
                    extra_authorize_params: collect_key_value_params(
                        &theme,
                        "Additional authorization params (k=v, comma separated)",
                        args.auth_params,
                    )?,
                    extra_token_params: collect_key_value_params(
                        &theme,
                        "Additional token params (k=v, comma separated)",
                        args.token_params,
                    )?,
                }),
                local: false,
            };
            let token = complete_oauth_login(&provider).await?;
            ProviderUpsertRequest {
                provider,
                api_key: None,
                oauth_token: Some(token),
            }
        }
    };

    let default_model = resolve_hosted_model_after_auth(&theme, &request, args.model).await?;
    request.provider.default_model = Some(default_model.clone());
    upsert_provider_with_optional_alias(storage, request, main_alias, default_model).await
}

pub(crate) async fn provider_command(storage: &Storage, command: ProviderCommands) -> Result<()> {
    match command {
        ProviderCommands::Add(args) => {
            let api_key = args
                .api_key
                .ok_or_else(|| anyhow!("--api-key is required for hosted provider add"))?;
            let provider = ProviderConfig {
                id: args.id,
                display_name: args.name,
                kind: hosted_kind_to_provider_kind(args.kind),
                base_url: args
                    .base_url
                    .unwrap_or_else(|| default_hosted_url(args.kind).to_string()),
                auth_mode: AuthMode::ApiKey,
                default_model: Some(args.model.clone()),
                keychain_account: None,
                oauth: None,
                local: false,
            };
            let request = ProviderUpsertRequest {
                provider,
                api_key: Some(api_key),
                oauth_token: None,
            };
            let main_alias = resolve_main_alias(storage, args.main_alias)?;
            upsert_provider_with_optional_alias(storage, request, main_alias, args.model).await?;
        }
        ProviderCommands::AddLocal(args) => {
            let base_url = args.base_url.unwrap_or_else(|| match args.kind {
                LocalKindArg::Ollama => DEFAULT_OLLAMA_URL.to_string(),
                LocalKindArg::OpenaiCompatible => DEFAULT_LOCAL_OPENAI_URL.to_string(),
            });
            let mut provider = ProviderConfig {
                id: args.id,
                display_name: args.name,
                kind: match args.kind {
                    LocalKindArg::Ollama => ProviderKind::Ollama,
                    LocalKindArg::OpenaiCompatible => ProviderKind::OpenAiCompatible,
                },
                base_url,
                auth_mode: if args.api_key.is_some() {
                    AuthMode::ApiKey
                } else {
                    AuthMode::None
                },
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: true,
            };
            let model = determine_local_model(&provider, args.model, None).await?;
            provider.default_model = Some(model.clone());
            let request = ProviderUpsertRequest {
                provider,
                api_key: args.api_key,
                oauth_token: None,
            };
            let main_alias = resolve_main_alias(storage, args.main_alias)?;
            upsert_provider_with_optional_alias(storage, request, main_alias, model).await?;
        }
        ProviderCommands::List => {
            let config = storage.load_config()?;
            for provider in config.providers {
                println!(
                    "{} [{}] auth={:?} model={} url={}",
                    provider.id,
                    if provider.local { "local" } else { "remote" },
                    provider.auth_mode,
                    provider.default_model.unwrap_or_else(|| "-".to_string()),
                    provider.base_url
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn load_prompt_template(inline: Option<&str>, file: Option<&PathBuf>) -> Result<String> {
    match (inline, file) {
        (Some(value), None) => Ok(value.to_string()),
        (None, Some(path)) => {
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
        }
        (Some(_), Some(_)) => bail!("specify either --prompt-template or --prompt-file"),
        (None, None) => bail!("one of --prompt-template or --prompt-file is required"),
    }
}

pub(crate) fn load_json_file(path: &Path) -> Result<serde_json::Value> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))
}

pub(crate) async fn model_command(storage: &Storage, command: ModelCommands) -> Result<()> {
    match command {
        ModelCommands::List { provider } => {
            let config = storage.load_config()?;
            let provider = config
                .get_provider(&provider)
                .cloned()
                .ok_or_else(|| anyhow!("unknown provider"))?;
            let models = provider_list_models(&build_http_client(), &provider).await?;
            for model in models {
                println!("{}", agent_core::redact_sensitive_text(&model));
            }
        }
    }

    Ok(())
}

pub(crate) async fn alias_command(storage: &Storage, command: AliasCommands) -> Result<()> {
    match command {
        AliasCommands::Add(args) => {
            let payload = AliasUpsertRequest {
                alias: ModelAlias {
                    alias: args.alias.clone(),
                    provider_id: args.provider,
                    model: args.model,
                    description: args.description,
                },
                set_as_main: args.main,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: ModelAlias = client.post("/v1/aliases", &payload).await?;
            } else {
                let mut config = storage.load_config()?;
                if config
                    .resolve_provider(&payload.alias.provider_id)
                    .is_none()
                {
                    bail!("alias references unknown provider");
                }
                if payload.set_as_main {
                    config.main_agent_alias = Some(payload.alias.alias.clone());
                }
                config.upsert_alias(payload.alias.clone());
                storage.save_config(&config)?;
            }
            println!("alias '{}' configured", args.alias);
        }
        AliasCommands::List => {
            let config = storage.load_config()?;
            for alias in config.aliases {
                println!(
                    "{} -> {} / {}{}",
                    alias.alias,
                    alias.provider_id,
                    alias.model,
                    alias
                        .description
                        .as_deref()
                        .map(|text| format!(" ({text})"))
                        .unwrap_or_default()
                );
            }
        }
    }

    Ok(())
}

pub(crate) async fn trust_command(storage: &Storage, args: TrustArgs) -> Result<()> {
    let update = TrustUpdateRequest {
        trusted_path: args.path,
        allow_shell: args.allow_shell,
        allow_network: args.allow_network,
        allow_full_disk: args.allow_full_disk,
        allow_self_edit: args.allow_self_edit,
    };
    let policy: agent_core::TrustPolicy = if let Some(client) = try_daemon(storage).await? {
        client.put("/v1/trust", &update).await?
    } else {
        let mut config = storage.load_config()?;
        apply_trust_update(&mut config.trust_policy, &update);
        storage.save_config(&config)?;
        config.trust_policy
    };
    println!("{}", trust_summary(&policy));
    Ok(())
}

pub(crate) async fn permissions_command(storage: &Storage, args: PermissionsArgs) -> Result<()> {
    if let Some(preset) = args.preset {
        let preset: PermissionPreset = preset.into();
        let updated: PermissionPreset = if let Some(client) = try_daemon(storage).await? {
            client
                .put(
                    "/v1/permissions",
                    &PermissionUpdateRequest {
                        permission_preset: preset,
                    },
                )
                .await?
        } else {
            let mut config = storage.load_config()?;
            config.permission_preset = preset;
            storage.save_config(&config)?;
            config.permission_preset
        };
        println!("permission_preset={}", permission_summary(updated));
    } else {
        let preset = if let Some(client) = try_daemon(storage).await? {
            client.get::<PermissionPreset>("/v1/permissions").await?
        } else {
            storage.load_config()?.permission_preset
        };
        println!("permission_preset={}", permission_summary(preset));
    }
    Ok(())
}

pub(crate) fn load_schema_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

pub(crate) fn collect_image_attachments(
    base_cwd: &Path,
    paths: &[PathBuf],
) -> Result<Vec<InputAttachment>> {
    paths
        .iter()
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                base_cwd.join(path)
            };
            let canonical = absolute
                .canonicalize()
                .with_context(|| format!("failed to access attachment {}", absolute.display()))?;
            if !canonical.is_file() {
                bail!("attachment {} is not a file", canonical.display());
            }
            Ok(InputAttachment {
                kind: agent_core::AttachmentKind::Image,
                path: canonical,
            })
        })
        .collect()
}

pub(crate) fn maybe_write_last_message(path: Option<&Path>, content: &str) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn print_json_run_response(response: &RunTaskResponse) -> Result<()> {
    for event in &response.tool_events {
        println!(
            "{}",
            serde_json::json!({
                "event": "tool",
                "call_id": event.call_id,
                "name": event.name,
                "arguments": event.arguments,
                "outcome": event.outcome,
                "output": event.output,
            })
        );
    }
    let structured_output = response
        .structured_output_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok());
    println!(
        "{}",
        serde_json::json!({
            "event": "response",
            "session_id": response.session_id,
            "alias": response.alias,
            "provider_id": response.provider_id,
            "model": response.model,
            "response": response.response,
            "structured_output": structured_output,
        })
    );
    Ok(())
}

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

async fn upsert_provider_with_optional_alias(
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

fn resolve_main_alias(storage: &Storage, requested: Option<String>) -> Result<Option<String>> {
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
