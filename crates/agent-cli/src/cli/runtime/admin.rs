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

#[derive(Debug, Clone)]
pub(crate) struct SkillInfo {
    name: String,
    description: String,
    path: PathBuf,
}

pub(crate) async fn load_enabled_skills(storage: &Storage) -> Result<Vec<String>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/skills").await
    } else {
        Ok(storage.load_config()?.enabled_skills)
    }
}

pub(crate) async fn load_skill_drafts(
    storage: &Storage,
    limit: usize,
    status: Option<SkillDraftStatus>,
) -> Result<Vec<SkillDraft>> {
    if let Some(client) = try_daemon(storage).await? {
        let mut path = format!("/v1/skills/drafts?limit={limit}");
        if let Some(status) = status {
            path.push_str("&status=");
            path.push_str(match status {
                SkillDraftStatus::Draft => "draft",
                SkillDraftStatus::Published => "published",
                SkillDraftStatus::Rejected => "rejected",
            });
        }
        client.get(&path).await
    } else {
        storage.list_skill_drafts(limit, status, None, None)
    }
}

pub(crate) async fn load_profile_memories(storage: &Storage, limit: usize) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/profile?limit={limit}"))
            .await
    } else {
        let mut seen = HashSet::new();
        let mut memories = storage.list_memories_by_tag("system_profile", limit, None, None)?;
        memories.extend(storage.list_memories_by_tag("workspace_profile", limit, None, None)?);
        memories.retain(|memory| seen.insert(memory.id.clone()));
        memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        memories.truncate(limit);
        Ok(memories)
    }
}

pub(crate) async fn load_memory_review_queue(storage: &Storage, limit: usize) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/review?limit={limit}"))
            .await
    } else {
        storage.list_memories_by_review_status(MemoryReviewStatus::Candidate, limit)
    }
}

pub(crate) async fn load_connector_approvals(
    storage: &Storage,
    kind: ConnectorKind,
    limit: usize,
) -> Result<Vec<ConnectorApprovalRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!(
                "/v1/connector-approvals?kind={}&status=pending&limit={limit}",
                serde_json::to_string(&kind)?.trim_matches('"')
            ))
            .await
    } else {
        storage.list_connector_approvals(Some(kind), Some(ConnectorApprovalStatus::Pending), limit)
    }
}

pub(crate) async fn update_memory_review_status(
    storage: &Storage,
    id: &str,
    status: MemoryReviewStatus,
    note: Option<String>,
) -> Result<MemoryRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            MemoryReviewStatus::Accepted => format!("/v1/memory/{id}/approve"),
            MemoryReviewStatus::Rejected => format!("/v1/memory/{id}/reject"),
            MemoryReviewStatus::Candidate => {
                bail!("cannot set memory back to candidate from CLI")
            }
        };
        client
            .post(&path, &MemoryReviewUpdateRequest { status, note })
            .await
    } else {
        let updated = storage.update_memory_review_status(id, status, note.as_deref())?;
        if !updated {
            bail!("unknown memory '{id}'");
        }
        storage
            .get_memory(id)?
            .ok_or_else(|| anyhow!("unknown memory '{id}'"))
    }
}

pub(crate) async fn update_connector_approval_status(
    storage: &Storage,
    id: &str,
    status: ConnectorApprovalStatus,
    note: Option<String>,
) -> Result<ConnectorApprovalRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            ConnectorApprovalStatus::Pending => {
                bail!("cannot set connector approval back to pending from CLI")
            }
            ConnectorApprovalStatus::Approved => {
                format!("/v1/connector-approvals/{id}/approve")
            }
            ConnectorApprovalStatus::Rejected => {
                format!("/v1/connector-approvals/{id}/reject")
            }
        };
        client
            .post(&path, &ConnectorApprovalUpdateRequest { note })
            .await
    } else {
        let updated =
            storage.update_connector_approval_status(id, status, note.as_deref(), None)?;
        if !updated {
            bail!("unknown connector approval '{id}'");
        }
        storage
            .get_connector_approval(id)?
            .ok_or_else(|| anyhow!("unknown connector approval '{id}'"))
    }
}

pub(crate) async fn update_skill_draft_status(
    storage: &Storage,
    id: &str,
    status: SkillDraftStatus,
) -> Result<SkillDraft> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            SkillDraftStatus::Draft => bail!("cannot set skill draft back to draft from CLI"),
            SkillDraftStatus::Published => format!("/v1/skills/drafts/{id}/publish"),
            SkillDraftStatus::Rejected => format!("/v1/skills/drafts/{id}/reject"),
        };
        client.post(&path, &serde_json::json!({})).await
    } else {
        let mut draft = storage
            .get_skill_draft(id)?
            .ok_or_else(|| anyhow!("unknown skill draft '{id}'"))?;
        draft.status = status;
        draft.updated_at = chrono::Utc::now();
        storage.upsert_skill_draft(&draft)?;
        Ok(draft)
    }
}

pub(crate) fn format_memory_records(records: &[MemoryRecord]) -> String {
    if records.is_empty() {
        return "No stored memory.".to_string();
    }

    records
        .iter()
        .map(|memory| {
            let tags = if memory.tags.is_empty() {
                String::new()
            } else {
                format!(" tags={}", memory.tags.join(","))
            };
            let review = if matches!(memory.review_status, MemoryReviewStatus::Accepted) {
                String::new()
            } else {
                format!(" review={:?}", memory.review_status)
            };
            let note = memory
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            let source = match (
                memory.source_session_id.as_deref(),
                memory.source_message_id.as_deref(),
            ) {
                (Some(session_id), Some(message_id)) => {
                    format!("\n  source: session={session_id} message={message_id}")
                }
                (Some(session_id), None) => format!("\n  source: session={session_id}"),
                (None, Some(message_id)) => format!("\n  source: message={message_id}"),
                (None, None) => String::new(),
            };
            let evidence = if memory.evidence_refs.is_empty() {
                String::new()
            } else {
                let mut lines = memory
                    .evidence_refs
                    .iter()
                    .take(3)
                    .map(|evidence| {
                        let role = evidence
                            .role
                            .as_ref()
                            .map(|role| format!(" role={role:?}"))
                            .unwrap_or_default();
                        let message = evidence
                            .message_id
                            .as_deref()
                            .map(|value| format!(" message={value}"))
                            .unwrap_or_default();
                        let tool = match (
                            evidence.tool_name.as_deref(),
                            evidence.tool_call_id.as_deref(),
                        ) {
                            (Some(name), Some(call_id)) => format!(" tool={name}#{call_id}"),
                            (Some(name), None) => format!(" tool={name}"),
                            (None, Some(call_id)) => format!(" tool_call={call_id}"),
                            (None, None) => String::new(),
                        };
                        format!(
                            "\n    - session={}{}{}{} @ {}",
                            evidence.session_id, role, message, tool, evidence.created_at
                        )
                    })
                    .collect::<String>();
                if memory.evidence_refs.len() > 3 {
                    lines.push_str(&format!(
                        "\n    - ... {} more",
                        memory.evidence_refs.len() - 3
                    ));
                }
                format!("\n  evidence:{lines}")
            };
            format!(
                "{} [{:?}/{:?}] {}{}{}\n  {}{}{}{}",
                memory.id,
                memory.kind,
                memory.scope,
                memory.subject,
                tags,
                review,
                memory.content,
                source,
                note,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_connector_approvals(records: &[ConnectorApprovalRecord]) -> String {
    if records.is_empty() {
        return "No pending connector approvals.".to_string();
    }

    records
        .iter()
        .map(|approval| {
            let note = approval
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] {} chat={} user={}\n  {}\n  {}{}",
                approval.id,
                approval.status,
                approval.connector_name,
                approval.external_chat_display.as_deref().unwrap_or("-"),
                approval.external_user_display.as_deref().unwrap_or("-"),
                approval.title,
                approval
                    .message_preview
                    .as_deref()
                    .unwrap_or(approval.details.as_str()),
                note
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_skill_drafts(drafts: &[SkillDraft]) -> String {
    if drafts.is_empty() {
        return "No learned skill drafts.".to_string();
    }

    drafts
        .iter()
        .map(|draft| {
            let trigger = draft
                .trigger_hint
                .as_deref()
                .map(|value| format!(" trigger={value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] usage={}{}\n  {}\n  {}",
                draft.id, draft.status, draft.usage_count, trigger, draft.title, draft.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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

pub(crate) async fn update_enabled_skill(storage: &Storage, name: &str, enabled: bool) -> Result<()> {
    let available = discover_skills()?;
    if enabled && !available.iter().any(|skill| skill.name == name) {
        bail!("unknown skill '{name}'");
    }
    let mut enabled_skills = load_enabled_skills(storage).await?;
    if enabled {
        if !enabled_skills.iter().any(|skill| skill == name) {
            enabled_skills.push(name.to_string());
        }
    } else {
        enabled_skills.retain(|skill| skill != name);
    }
    if let Some(client) = try_daemon(storage).await? {
        let _: Vec<String> = client
            .put(
                "/v1/skills",
                &SkillUpdateRequest {
                    enabled_skills: enabled_skills.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.enabled_skills = enabled_skills;
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) fn discover_skills() -> Result<Vec<SkillInfo>> {
    let Some(root) = codex_skills_root() else {
        return Ok(Vec::new());
    };
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    discover_skills_in_dir(&root, &mut skills)?;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

pub(crate) fn discover_skills_in_dir(root: &Path, output: &mut Vec<SkillInfo>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            discover_skills_in_dir(&path, output)?;
            continue;
        }
        if entry.file_name().to_string_lossy() != "SKILL.md" {
            continue;
        }
        let name = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("failed to infer skill name from {}", path.display()))?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let description = extract_skill_description(&content);
        output.push(SkillInfo {
            name,
            description,
            path,
        });
    }
    Ok(())
}

pub(crate) fn codex_skills_root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".codex").join("skills"))
}

pub(crate) fn extract_skill_description(content: &str) -> String {
    let lines = content.lines().map(str::trim).collect::<Vec<_>>();
    if lines.first().copied() == Some("---") {
        let mut in_frontmatter = true;
        for line in &lines[1..] {
            if *line == "---" {
                in_frontmatter = false;
                continue;
            }
            if in_frontmatter {
                if let Some(value) = line.strip_prefix("description:") {
                    return value.trim().trim_matches('"').to_string();
                }
            }
        }
    }

    lines
        .into_iter()
        .find(|line| !line.is_empty() && !line.starts_with('#') && *line != "---" && *line != "```")
        .unwrap_or("No description available.")
        .to_string()
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
                println!("{model}");
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

pub(crate) fn collect_image_attachments(base_cwd: &Path, paths: &[PathBuf]) -> Result<Vec<InputAttachment>> {
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

