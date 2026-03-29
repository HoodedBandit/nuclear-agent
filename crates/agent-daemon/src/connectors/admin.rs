use super::*;
use agent_providers::{delete_secret, load_api_key, store_api_key};
use sha2::{Digest, Sha256};

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn store_connector_secret(
    scope: &str,
    id: &str,
    secret: Option<String>,
) -> Result<Option<String>, ApiError> {
    let Some(secret) = optional_trimmed(secret) else {
        return Ok(None);
    };
    store_api_key(&format!("connector:{scope}:{id}"), &secret)
        .map(Some)
        .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
}

fn validate_connector_secret_account(
    account: Option<&str>,
    missing_message: &str,
    invalid_message: &str,
) -> Result<String, ApiError> {
    let account = account
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, missing_message))?;
    let _ = load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("{invalid_message}: {error}"),
        )
    })?;
    Ok(account.to_string())
}

fn should_cleanup_upserted_secret(
    new_account: Option<&str>,
    previous_account: Option<&str>,
) -> bool {
    let new_account = new_account.map(str::trim).filter(|value| !value.is_empty());
    let previous_account = previous_account
        .map(str::trim)
        .filter(|value| !value.is_empty());
    new_account.is_some() && new_account != previous_account
}

fn cleanup_upserted_secret(new_account: Option<&str>, previous_account: Option<&str>) {
    if should_cleanup_upserted_secret(new_account, previous_account) {
        if let Some(account) = new_account.map(str::trim).filter(|value| !value.is_empty()) {
            let _ = delete_secret(account);
        }
    }
}

fn brave_api_key_account_in_use(connectors: &[BraveConnectorConfig], account: &str) -> bool {
    connectors.iter().any(|connector| {
        connector.api_key_keychain_account.as_deref().map(str::trim) == Some(account)
    })
}

fn hash_webhook_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn resolve_webhook_token_sha256(
    existing_sha256: Option<&str>,
    webhook_token: Option<&str>,
    clear_webhook_token: bool,
) -> Option<String> {
    if let Some(token) = webhook_token {
        return Some(hash_webhook_token(token));
    }
    if clear_webhook_token {
        return None;
    }
    existing_sha256.map(ToOwned::to_owned)
}

pub(crate) async fn list_app_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<AppConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.app_connectors.clone()))
}

pub(crate) async fn upsert_app_connector(
    State(state): State<AppState>,
    Json(payload): Json<AppConnectorUpsertRequest>,
) -> Result<Json<AppConnectorConfig>, ApiError> {
    {
        let mut config = state.config.write().await;
        config.upsert_app_connector(payload.connector.clone());
        state.storage.save_config(&config)?;
    }
    append_log(
        &state,
        "info",
        "apps",
        format!("app connector '{}' updated", payload.connector.id),
    )?;
    Ok(Json(payload.connector))
}

pub(crate) async fn delete_app_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config.remove_app_connector(&connector_id);
        if removed {
            state.storage.save_config(&config)?;
        }
        removed
    };
    if !removed {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown app connector",
        ));
    }
    append_log(
        &state,
        "info",
        "apps",
        format!("app connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_webhook_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.webhook_connectors.clone()))
}

pub(crate) async fn get_webhook_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<WebhookConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .webhook_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown webhook connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_webhook_connector(
    State(state): State<AppState>,
    Json(payload): Json<WebhookConnectorUpsertRequest>,
) -> Result<Json<WebhookConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    if connector.prompt_template.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "webhook connector prompt_template must not be empty",
        ));
    }
    {
        let mut config = state.config.write().await;
        let existing_sha256 = config
            .webhook_connectors
            .iter()
            .find(|existing| existing.id == connector.id)
            .and_then(|existing| existing.token_sha256.as_deref());
        connector.token_sha256 = resolve_webhook_token_sha256(
            existing_sha256,
            optional_trimmed(payload.webhook_token).as_deref(),
            payload.clear_webhook_token,
        );
        config.upsert_webhook_connector(connector.clone());
        state.storage.save_config(&config)?;
    }
    append_log(
        &state,
        "info",
        "webhooks",
        format!("webhook connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn delete_webhook_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config.remove_webhook_connector(&connector_id);
        if removed {
            state.storage.save_config(&config)?;
        }
        removed
    };
    if !removed {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown webhook connector",
        ));
    }
    append_log(
        &state,
        "info",
        "webhooks",
        format!("webhook connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_inbox_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<InboxConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.inbox_connectors.clone()))
}

pub(crate) async fn get_inbox_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<InboxConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .inbox_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown inbox connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_inbox_connector(
    State(state): State<AppState>,
    Json(payload): Json<InboxConnectorUpsertRequest>,
) -> Result<Json<InboxConnectorConfig>, ApiError> {
    if payload.connector.path.as_os_str().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "inbox connector path must not be empty",
        ));
    }
    {
        let mut config = state.config.write().await;
        config.upsert_inbox_connector(payload.connector.clone());
        state.storage.save_config(&config)?;
    }
    append_log(
        &state,
        "info",
        "inboxes",
        format!("inbox connector '{}' updated", payload.connector.id),
    )?;
    Ok(Json(payload.connector))
}

pub(crate) async fn delete_inbox_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config.remove_inbox_connector(&connector_id);
        if removed {
            state.storage.save_config(&config)?;
        }
        removed
    };
    if !removed {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown inbox connector",
        ));
    }
    append_log(
        &state,
        "info",
        "inboxes",
        format!("inbox connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_telegram_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<TelegramConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.telegram_connectors.clone()))
}

pub(crate) async fn get_telegram_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<TelegramConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .telegram_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown telegram connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn list_discord_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<DiscordConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.discord_connectors.clone()))
}

pub(crate) async fn get_discord_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<DiscordConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .discord_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown discord connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn list_slack_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<SlackConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.slack_connectors.clone()))
}

pub(crate) async fn get_slack_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<SlackConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .slack_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown slack connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn list_home_assistant_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<HomeAssistantConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.home_assistant_connectors.clone()))
}

pub(crate) async fn list_signal_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<SignalConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.signal_connectors.clone()))
}

pub(crate) async fn get_home_assistant_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<HomeAssistantConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .home_assistant_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown Home Assistant connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn get_signal_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<SignalConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .signal_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown signal connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_discord_connector(
    State(state): State<AppState>,
    Json(payload): Json<DiscordConnectorUpsertRequest>,
) -> Result<Json<DiscordConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    if connector.monitored_channel_ids.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "discord connector monitored_channel_ids must not be empty",
        ));
    }
    let previous_account = {
        let config = state.config.read().await;
        config
            .discord_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.bot_token_keychain_account.clone())
    };
    if let Some(account) = store_connector_secret("discord", &connector.id, payload.bot_token)? {
        connector.bot_token_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.bot_token_keychain_account.as_deref(),
        "discord connector bot_token_keychain_account must not be empty",
        "failed to load discord bot token from keychain",
    ) {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_discord_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "discord",
        format!("discord connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_slack_connector(
    State(state): State<AppState>,
    Json(payload): Json<SlackConnectorUpsertRequest>,
) -> Result<Json<SlackConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    if connector.monitored_channel_ids.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "slack connector monitored_channel_ids must not be empty",
        ));
    }
    let previous_account = {
        let config = state.config.read().await;
        config
            .slack_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.bot_token_keychain_account.clone())
    };
    if let Some(account) = store_connector_secret("slack", &connector.id, payload.bot_token)? {
        connector.bot_token_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.bot_token_keychain_account.as_deref(),
        "slack connector bot_token_keychain_account must not be empty",
        "failed to load slack bot token from keychain",
    ) {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_slack_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "slack",
        format!("slack connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_home_assistant_connector(
    State(state): State<AppState>,
    Json(payload): Json<HomeAssistantConnectorUpsertRequest>,
) -> Result<Json<HomeAssistantConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    let base_url = connector.base_url.trim();
    if base_url.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant connector base_url must not be empty",
        ));
    }
    if connector.monitored_entity_ids.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant connector monitored_entity_ids must not be empty",
        ));
    }
    let previous_account = {
        let config = state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.access_token_keychain_account.clone())
    };
    if let Some(account) =
        store_connector_secret("home-assistant", &connector.id, payload.access_token)?
    {
        connector.access_token_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.access_token_keychain_account.as_deref(),
        "Home Assistant connector access_token_keychain_account must not be empty",
        "failed to load Home Assistant token from keychain",
    ) {
        cleanup_upserted_secret(
            connector.access_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_home_assistant_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.access_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "home_assistant",
        format!("Home Assistant connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_signal_connector(
    State(state): State<AppState>,
    Json(payload): Json<SignalConnectorUpsertRequest>,
) -> Result<Json<SignalConnectorConfig>, ApiError> {
    if payload.connector.account.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "signal connector account must not be empty",
        ));
    }
    if let Some(cli_path) = payload.connector.cli_path.as_deref() {
        if cli_path.as_os_str().is_empty() || !cli_path.exists() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "signal connector cli_path '{}' does not exist",
                    cli_path.display()
                ),
            ));
        }
    }
    {
        let mut config = state.config.write().await;
        config.upsert_signal_connector(payload.connector.clone());
        state.storage.save_config(&config)?;
    }
    append_log(
        &state,
        "info",
        "signal",
        format!("signal connector '{}' updated", payload.connector.id),
    )?;
    Ok(Json(payload.connector))
}

pub(crate) async fn upsert_telegram_connector(
    State(state): State<AppState>,
    Json(payload): Json<TelegramConnectorUpsertRequest>,
) -> Result<Json<TelegramConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    let previous_account = {
        let config = state.config.read().await;
        config
            .telegram_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.bot_token_keychain_account.clone())
    };
    if let Some(account) = store_connector_secret("telegram", &connector.id, payload.bot_token)? {
        connector.bot_token_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.bot_token_keychain_account.as_deref(),
        "telegram connector bot_token_keychain_account must not be empty",
        "failed to load telegram bot token from keychain",
    ) {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_telegram_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.bot_token_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "telegram",
        format!("telegram connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn delete_discord_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .discord_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_discord_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.bot_token_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| {
                !discord::discord_bot_token_account_in_use(&config.discord_connectors, account)
            })
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown discord connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete discord bot token: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "discord",
        format!("discord connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn delete_slack_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .slack_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_slack_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.bot_token_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| {
                !slack::slack_bot_token_account_in_use(&config.slack_connectors, account)
            })
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown slack connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete slack bot token: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "slack",
        format!("slack connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn delete_home_assistant_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .home_assistant_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_home_assistant_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.access_token_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| {
                !home_assistant::home_assistant_token_account_in_use(
                    &config.home_assistant_connectors,
                    account,
                )
            })
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown Home Assistant connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete Home Assistant token: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "home_assistant",
        format!("Home Assistant connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn delete_signal_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = {
        let mut config = state.config.write().await;
        let removed = config
            .signal_connectors
            .iter()
            .any(|connector| connector.id == connector_id);
        if removed {
            config.remove_signal_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        removed
    };
    if !removed {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown signal connector",
        ));
    }
    append_log(
        &state,
        "info",
        "signal",
        format!("signal connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_brave_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<BraveConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.brave_connectors.clone()))
}

pub(crate) async fn get_brave_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<BraveConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .brave_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown brave connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_brave_connector(
    State(state): State<AppState>,
    Json(payload): Json<BraveConnectorUpsertRequest>,
) -> Result<Json<BraveConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    let previous_account = {
        let config = state.config.read().await;
        config
            .brave_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.api_key_keychain_account.clone())
    };
    if let Some(account) = store_connector_secret("brave", &connector.id, payload.api_key)? {
        connector.api_key_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.api_key_keychain_account.as_deref(),
        "brave connector api_key_keychain_account must not be empty",
        "failed to load brave api key from keychain",
    ) {
        cleanup_upserted_secret(
            connector.api_key_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_brave_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.api_key_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "brave",
        format!("brave connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

pub(crate) async fn delete_brave_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .brave_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_brave_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.api_key_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| !brave_api_key_account_in_use(&config.brave_connectors, account))
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown brave connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete brave api key: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "brave",
        format!("brave connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn list_gmail_connectors(
    State(state): State<AppState>,
) -> Result<Json<Vec<GmailConnectorConfig>>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.gmail_connectors.clone()))
}

pub(crate) async fn get_gmail_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<GmailConnectorConfig>, ApiError> {
    let config = state.config.read().await;
    let connector = config
        .gmail_connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown gmail connector"))?;
    Ok(Json(connector))
}

pub(crate) async fn upsert_gmail_connector(
    State(state): State<AppState>,
    Json(payload): Json<GmailConnectorUpsertRequest>,
) -> Result<Json<GmailConnectorConfig>, ApiError> {
    let mut connector = payload.connector;
    let previous_account = {
        let config = state.config.read().await;
        config
            .gmail_connectors
            .iter()
            .find(|entry| entry.id == connector.id)
            .and_then(|entry| entry.oauth_keychain_account.clone())
    };
    if let Some(account) = store_connector_secret("gmail", &connector.id, payload.oauth_token)? {
        connector.oauth_keychain_account = Some(account);
    }
    if let Err(error) = validate_connector_secret_account(
        connector.oauth_keychain_account.as_deref(),
        "gmail connector oauth_keychain_account must not be empty",
        "failed to load gmail OAuth token from keychain",
    ) {
        cleanup_upserted_secret(
            connector.oauth_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(error);
    }
    let save_error = {
        let mut config = state.config.write().await;
        config.upsert_gmail_connector(connector.clone());
        state.storage.save_config(&config).err()
    };
    if let Some(error) = save_error {
        cleanup_upserted_secret(
            connector.oauth_keychain_account.as_deref(),
            previous_account.as_deref(),
        );
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ));
    }
    append_log(
        &state,
        "info",
        "gmail",
        format!("gmail connector '{}' updated", connector.id),
    )?;
    Ok(Json(connector))
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{resolve_webhook_token_sha256, should_cleanup_upserted_secret};

    #[test]
    fn cleanup_only_runs_for_new_secret_accounts() {
        assert!(!should_cleanup_upserted_secret(
            Some("connector:slack:ops"),
            Some("connector:slack:ops")
        ));
        assert!(should_cleanup_upserted_secret(
            Some("connector:slack:ops"),
            None
        ));
        assert!(!should_cleanup_upserted_secret(None, None));
    }

    #[test]
    fn webhook_token_hash_preserves_existing_secret_by_default() {
        assert_eq!(
            resolve_webhook_token_sha256(Some("existing-hash"), None, false),
            Some("existing-hash".to_string())
        );
    }

    #[test]
    fn webhook_token_hash_can_rotate_or_clear_secret() {
        assert_eq!(
            resolve_webhook_token_sha256(Some("existing-hash"), Some("new-token"), false),
            Some(super::hash_webhook_token("new-token"))
        );
        assert_eq!(
            resolve_webhook_token_sha256(Some("existing-hash"), None, true),
            None
        );
    }
}

pub(crate) async fn delete_gmail_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .gmail_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_gmail_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.oauth_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| !gmail::gmail_oauth_account_in_use(&config.gmail_connectors, account))
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown gmail connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete gmail OAuth token: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "gmail",
        format!("gmail connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn delete_telegram_connector(
    State(state): State<AppState>,
    Path(connector_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (removed_connector, delete_secret_account) = {
        let mut config = state.config.write().await;
        let removed = config
            .telegram_connectors
            .iter()
            .find(|connector| connector.id == connector_id)
            .cloned();
        if removed.is_some() {
            config.remove_telegram_connector(&connector_id);
            state.storage.save_config(&config)?;
        }
        let delete_secret_account = removed
            .as_ref()
            .and_then(|connector| connector.bot_token_keychain_account.as_deref())
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .filter(|account| {
                !telegram::telegram_bot_token_account_in_use(&config.telegram_connectors, account)
            })
            .map(ToOwned::to_owned);
        (removed, delete_secret_account)
    };
    if removed_connector.is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown telegram connector",
        ));
    }
    if let Some(account) = delete_secret_account {
        delete_secret(&account).map_err(|error| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to delete telegram bot token: {error}"),
            )
        })?;
    }
    append_log(
        &state,
        "info",
        "telegram",
        format!("telegram connector '{}' removed", connector_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
