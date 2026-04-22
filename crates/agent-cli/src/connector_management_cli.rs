use super::*;

pub(crate) fn cleanup_replaced_connector_secret(
    new_account: Option<&str>,
    previous_account: Option<&str>,
) {
    let new_account = new_account.map(str::trim).filter(|value| !value.is_empty());
    let previous_account = previous_account
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(account) = new_account.filter(|account| Some(*account) != previous_account) {
        let _ = delete_secret(account);
    }
}

pub(crate) fn ensure_discord_monitored_channel_ids(channel_ids: &[String]) -> Result<()> {
    if channel_ids.is_empty() {
        bail!("at least one --monitored-channel-id is required for discord connectors");
    }
    Ok(())
}

pub(crate) fn ensure_slack_monitored_channel_ids(channel_ids: &[String]) -> Result<()> {
    if channel_ids.is_empty() {
        bail!("at least one --monitored-channel-id is required for slack connectors");
    }
    Ok(())
}

pub(crate) fn ensure_home_assistant_monitored_entity_ids(entity_ids: &[String]) -> Result<()> {
    if entity_ids.is_empty() {
        bail!("at least one --entity-id is required for home assistant connectors");
    }
    Ok(())
}

pub(crate) async fn webhook_command(storage: &Storage, command: WebhookCommands) -> Result<()> {
    match command {
        WebhookCommands::List { json } => {
            let connectors = load_webhook_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] enabled={} alias={} model={} token={} cwd={}",
                        connector.id,
                        connector.name,
                        connector.enabled,
                        connector.alias.as_deref().unwrap_or("-"),
                        connector.requested_model.as_deref().unwrap_or("-"),
                        connector.token_sha256.is_some(),
                        connector
                            .cwd
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "-".to_string())
                    );
                }
            }
        }
        WebhookCommands::Get { id, json } => {
            let connector = load_webhook_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown webhook connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
                println!("token_configured={}", connector.token_sha256.is_some());
                println!("prompt_template={}", connector.prompt_template);
            }
        }
        WebhookCommands::Add(args) => {
            let prompt_template =
                load_prompt_template(args.prompt_template.as_deref(), args.prompt_file.as_ref())?;
            let generated_token = args.token.is_none();
            let token = args.token.unwrap_or_else(|| Uuid::new_v4().to_string());
            let connector = WebhookConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                prompt_template,
                enabled: args.enabled,
                token_sha256: Some(hash_webhook_token_local(&token)),
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: WebhookConnectorConfig = client
                    .post(
                        "/v1/webhooks",
                        &WebhookConnectorUpsertRequest {
                            connector: connector.clone(),
                            webhook_token: Some(token.clone()),
                            clear_webhook_token: false,
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_webhook_connector(connector.clone());
                storage.save_config(&config)?;
            }
            let config = storage.load_config()?;
            println!("webhook='{}' configured", args.id);
            println!(
                "url=http://{}:{}/v1/hooks/{}",
                config.daemon.host, config.daemon.port, args.id
            );
            if generated_token {
                let token_path = write_webhook_token_fallback_file(&args.id, &token)?;
                println!("token_file={}", token_path.display());
                println!("token_fingerprint={}", display_safe_id(&token));
            } else {
                println!("token=provided");
                println!("token_fingerprint={}", display_safe_id(&token));
            }
        }
        WebhookCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/webhooks/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_webhook_connector(&id) {
                    bail!("unknown webhook connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("webhook='{}' removed", id);
        }
        WebhookCommands::Enable { id } => {
            set_webhook_enabled(storage, &id, true).await?;
            println!("webhook='{}' enabled", id);
        }
        WebhookCommands::Disable { id } => {
            set_webhook_enabled(storage, &id, false).await?;
            println!("webhook='{}' disabled", id);
        }
        WebhookCommands::Deliver(args) => {
            let config = storage.load_config()?;
            let base_url = format!("http://{}:{}", config.daemon.host, config.daemon.port);
            let mut request = build_http_client()
                .post(format!("{base_url}/v1/hooks/{}", args.id))
                .json(&WebhookEventRequest {
                    summary: args.summary,
                    prompt: args.prompt,
                    details: args.details,
                    payload: match args.payload_file {
                        Some(path) => Some(load_json_file(&path)?),
                        None => None,
                    },
                });
            if let Some(token) = args.token {
                request = request.header("x-agent-webhook-token", token);
            }
            let response = request.send().await?;
            let status = response.status();
            let body = response.text().await?;
            if !status.is_success() {
                bail!("webhook delivery failed: {} {}", status, body);
            }
            let parsed: WebhookEventResponse =
                serde_json::from_str(&body).context("failed to parse webhook response")?;
            println!(
                "queued webhook mission={} title={} status={:?}",
                parsed.mission_id, parsed.title, parsed.status
            );
        }
    }
    Ok(())
}

pub(crate) async fn inbox_command(storage: &Storage, command: InboxCommands) -> Result<()> {
    match command {
        InboxCommands::List { json } => {
            let connectors = load_inbox_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
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
        InboxCommands::Get { id, json } => {
            let connector = load_inbox_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown inbox connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("enabled={}", connector.enabled);
                println!("delete_after_read={}", connector.delete_after_read);
                println!("alias={}", connector.alias.as_deref().unwrap_or("-"));
                println!(
                    "model={}",
                    connector.requested_model.as_deref().unwrap_or("-")
                );
                println!("path={}", connector.path.display());
                println!(
                    "cwd={}",
                    connector
                        .cwd
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        InboxCommands::Add(args) => {
            let connector = InboxConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                path: args.path,
                enabled: args.enabled,
                delete_after_read: args.delete_after_read,
                alias: args.alias,
                requested_model: args.model,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: InboxConnectorConfig = client
                    .post(
                        "/v1/inboxes",
                        &InboxConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_inbox_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!(
                "inbox='{}' configured path={}",
                args.id,
                connector.path.display()
            );
        }
        InboxCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/inboxes/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_inbox_connector(&id) {
                    bail!("unknown inbox connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("inbox='{}' removed", id);
        }
        InboxCommands::Enable { id } => {
            set_inbox_enabled(storage, &id, true).await?;
            println!("inbox='{}' enabled", id);
        }
        InboxCommands::Disable { id } => {
            set_inbox_enabled(storage, &id, false).await?;
            println!("inbox='{}' disabled", id);
        }
        InboxCommands::Poll { id } => {
            let client = ensure_daemon(storage).await?;
            let response: InboxPollResponse = client
                .post(&format!("/v1/inboxes/{id}/poll"), &serde_json::json!({}))
                .await?;
            println!(
                "polled inbox={} processed_files={} queued_missions={}",
                response.connector_id, response.processed_files, response.queued_missions
            );
        }
    }
    Ok(())
}

pub(crate) async fn skills_command(storage: &Storage, command: SkillCommands) -> Result<()> {
    match command {
        SkillCommands::List => {
            let enabled = load_enabled_skills(storage).await?;
            for skill in discover_skills()? {
                let marker = if enabled.contains(&skill.name) {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{} {} - {} ({})",
                    marker,
                    skill.name,
                    skill.description,
                    skill.path.display()
                );
            }
        }
        SkillCommands::Enable { name } => {
            update_enabled_skill(storage, &name, true).await?;
            println!("skill='{}' enabled", name);
        }
        SkillCommands::Disable { name } => {
            update_enabled_skill(storage, &name, false).await?;
            println!("skill='{}' disabled", name);
        }
        SkillCommands::Drafts { limit, status } => {
            let drafts = load_skill_drafts(storage, limit, status.map(Into::into)).await?;
            if drafts.is_empty() {
                println!("no skill drafts");
            } else {
                for draft in drafts {
                    println!(
                        "{} [{:?}] uses={} provider={} workspace={}",
                        draft.id,
                        draft.status,
                        draft.usage_count,
                        draft.provider_id.as_deref().unwrap_or("-"),
                        draft.workspace_key.as_deref().unwrap_or("-")
                    );
                    println!("  {}", draft.title);
                    println!("  {}", draft.summary);
                }
            }
        }
        SkillCommands::Publish { id } => {
            let draft =
                update_skill_draft_status(storage, &id, SkillDraftStatus::Published).await?;
            println!("published skill draft={} title={}", draft.id, draft.title);
        }
        SkillCommands::Reject { id } => {
            let draft = update_skill_draft_status(storage, &id, SkillDraftStatus::Rejected).await?;
            println!("rejected skill draft={} title={}", draft.id, draft.title);
        }
    }
    Ok(())
}

pub(crate) async fn load_telegram_connectors(
    storage: &Storage,
) -> Result<Vec<TelegramConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/telegram").await
    } else {
        Ok(storage.load_config()?.telegram_connectors)
    }
}

pub(crate) async fn load_discord_connectors(
    storage: &Storage,
) -> Result<Vec<DiscordConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/discord").await
    } else {
        Ok(storage.load_config()?.discord_connectors)
    }
}

pub(crate) async fn load_slack_connectors(storage: &Storage) -> Result<Vec<SlackConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/slack").await
    } else {
        Ok(storage.load_config()?.slack_connectors)
    }
}

pub(crate) async fn load_home_assistant_connectors(
    storage: &Storage,
) -> Result<Vec<HomeAssistantConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/home-assistant").await
    } else {
        Ok(storage.load_config()?.home_assistant_connectors)
    }
}

pub(crate) async fn load_signal_connectors(
    storage: &Storage,
) -> Result<Vec<SignalConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/signal").await
    } else {
        Ok(storage.load_config()?.signal_connectors)
    }
}

pub(crate) async fn load_webhook_connectors(
    storage: &Storage,
) -> Result<Vec<WebhookConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/webhooks").await
    } else {
        Ok(storage.load_config()?.webhook_connectors)
    }
}

pub(crate) async fn load_inbox_connectors(storage: &Storage) -> Result<Vec<InboxConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/inboxes").await
    } else {
        Ok(storage.load_config()?.inbox_connectors)
    }
}

pub(crate) async fn set_telegram_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_telegram_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown telegram connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: TelegramConnectorConfig = client
            .post(
                "/v1/telegram",
                &TelegramConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: None,
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_telegram_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_discord_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_discord_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown discord connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: DiscordConnectorConfig = client
            .post(
                "/v1/discord",
                &DiscordConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: None,
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_discord_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_slack_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_slack_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown slack connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: SlackConnectorConfig = client
            .post(
                "/v1/slack",
                &SlackConnectorUpsertRequest {
                    connector: connector.clone(),
                    bot_token: None,
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_slack_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_home_assistant_enabled(
    storage: &Storage,
    id: &str,
    enabled: bool,
) -> Result<()> {
    let mut connectors = load_home_assistant_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown home assistant connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: HomeAssistantConnectorConfig = client
            .post(
                "/v1/home-assistant",
                &HomeAssistantConnectorUpsertRequest {
                    connector: connector.clone(),
                    access_token: None,
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_home_assistant_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_signal_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_signal_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown signal connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: SignalConnectorConfig = client
            .post(
                "/v1/signal",
                &SignalConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_signal_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_webhook_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_webhook_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown webhook connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: WebhookConnectorConfig = client
            .post(
                "/v1/webhooks",
                &WebhookConnectorUpsertRequest {
                    connector: connector.clone(),
                    webhook_token: None,
                    clear_webhook_token: false,
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_webhook_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) async fn set_inbox_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_inbox_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown inbox connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: InboxConnectorConfig = client
            .post(
                "/v1/inboxes",
                &InboxConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_inbox_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_connectors_require_monitored_channel_ids() {
        let error = ensure_discord_monitored_channel_ids(&[])
            .expect_err("expected discord validation failure");
        assert_eq!(
            error.to_string(),
            "at least one --monitored-channel-id is required for discord connectors"
        );
    }

    #[test]
    fn slack_connectors_require_monitored_channel_ids() {
        let error =
            ensure_slack_monitored_channel_ids(&[]).expect_err("expected slack validation failure");
        assert_eq!(
            error.to_string(),
            "at least one --monitored-channel-id is required for slack connectors"
        );
    }

    #[test]
    fn home_assistant_connectors_require_monitored_entity_ids() {
        let error = ensure_home_assistant_monitored_entity_ids(&[])
            .expect_err("expected Home Assistant validation failure");
        assert_eq!(
            error.to_string(),
            "at least one --entity-id is required for home assistant connectors"
        );
    }

    #[test]
    fn cleanup_replaced_connector_secret_only_deletes_new_accounts() {
        assert!(!should_cleanup_replaced_connector_secret(
            Some("connector:slack:ops"),
            Some("connector:slack:ops")
        ));
        assert!(should_cleanup_replaced_connector_secret(
            Some("connector:slack:ops"),
            None
        ));
    }

    fn should_cleanup_replaced_connector_secret(
        new_account: Option<&str>,
        previous_account: Option<&str>,
    ) -> bool {
        let new_account = new_account.map(str::trim).filter(|value| !value.is_empty());
        let previous_account = previous_account
            .map(str::trim)
            .filter(|value| !value.is_empty());
        new_account.is_some() && new_account != previous_account
    }
}
