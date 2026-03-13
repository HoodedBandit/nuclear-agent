use agent_core::HomeAssistantEntityCursor;
use agent_providers::load_api_key;

use super::*;

pub(super) async fn poll_home_assistant_connectors(state: &AppState) -> Result<usize, ApiError> {
    let connectors = {
        let config = state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .filter(|connector| connector.enabled)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut queued = 0usize;
    for connector in connectors {
        let response = process_home_assistant_connector(state, &connector).await?;
        queued += response.queued_missions;
    }
    Ok(queued)
}

pub(super) async fn process_home_assistant_connector(
    state: &AppState,
    connector: &HomeAssistantConnectorConfig,
) -> Result<HomeAssistantPollResponse, ApiError> {
    if !connector.enabled {
        return Ok(HomeAssistantPollResponse {
            connector_id: connector.id.clone(),
            processed_entities: 0,
            queued_missions: 0,
            updated_entities: 0,
        });
    }

    let token = load_home_assistant_token(connector)?;
    let mut processed_entities = 0usize;
    let mut queued_missions = 0usize;
    let mut updated_entities = 0usize;

    for entity_id in connector.monitored_entity_ids.iter().map(String::as_str) {
        let entity =
            fetch_home_assistant_entity_state(&state.http_client, connector, &token, entity_id)
                .await?;
        processed_entities += 1;

        let previous = connector
            .entity_cursors
            .iter()
            .find(|cursor| cursor.entity_id == entity.entity_id)
            .cloned();
        let changed = previous.as_ref().is_some_and(|cursor| {
            cursor.last_changed != entity.last_changed
                || cursor.last_state.as_deref() != Some(entity.state.as_str())
        });

        persist_home_assistant_entity_cursor(state, &connector.id, &entity).await?;

        if previous.is_none() {
            updated_entities += 1;
            continue;
        }
        if !changed {
            continue;
        }

        let title = format!(
            "Home Assistant: {} changed to {}",
            entity
                .friendly_name
                .as_deref()
                .unwrap_or(entity.entity_id.as_str()),
            entity.state
        );
        let details = build_home_assistant_mission_details(connector, previous.as_ref(), &entity);
        let mut mission = Mission::new(title.clone(), details);
        mission.id = home_assistant_mission_id(&connector.id, &entity);
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::HomeAssistant);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        if state.storage.get_mission(&mission.id)?.is_none() {
            state.storage.upsert_mission(&mission)?;
            append_log(
                state,
                "info",
                "home_assistant",
                format!(
                    "Home Assistant '{}' queued mission '{}' for entity {}",
                    connector.id, mission.title, entity.entity_id
                ),
            )?;
            queued_missions += 1;
        }
        updated_entities += 1;
    }

    if queued_missions > 0 {
        state.autopilot_wake.notify_waiters();
    }

    Ok(HomeAssistantPollResponse {
        connector_id: connector.id.clone(),
        processed_entities,
        queued_missions,
        updated_entities,
    })
}

pub(super) fn load_home_assistant_token(
    connector: &HomeAssistantConnectorConfig,
) -> Result<String, ApiError> {
    let account = connector
        .access_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "Home Assistant connector '{}' has no access token configured",
                    connector.id
                ),
            )
        })?;
    load_api_key(account).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to load Home Assistant token for connector '{}': {error}",
                connector.id
            ),
        )
    })
}

pub(super) fn home_assistant_token_account_in_use(
    connectors: &[HomeAssistantConnectorConfig],
    account: &str,
) -> bool {
    let account = account.trim();
    !account.is_empty()
        && connectors.iter().any(|connector| {
            connector
                .access_token_keychain_account
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| value == account)
        })
}

fn canonical_home_assistant_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

pub(super) async fn fetch_home_assistant_entity_state(
    client: &reqwest::Client,
    connector: &HomeAssistantConnectorConfig,
    token: &str,
    entity_id: &str,
) -> Result<HomeAssistantEntityState, ApiError> {
    let entity_id = entity_id.trim();
    if entity_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant entity_id must not be empty",
        ));
    }
    let response = client
        .get(format!(
            "{}/api/states/{}",
            canonical_home_assistant_base_url(&connector.base_url),
            entity_id
        ))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("Home Assistant state request failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read Home Assistant state response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("Home Assistant state request failed: {status} {body}"),
        ));
    }
    serde_json::from_str::<HomeAssistantEntityState>(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse Home Assistant state response: {error}"),
        )
    })
}

pub(super) async fn call_home_assistant_service(
    client: &reqwest::Client,
    connector: &HomeAssistantConnectorConfig,
    token: &str,
    payload: &HomeAssistantServiceCallRequest,
) -> Result<usize, ApiError> {
    let domain = payload.domain.trim();
    let service = payload.service.trim();
    if domain.is_empty() || service.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant service domain and service must not be empty",
        ));
    }
    if !connector.allowed_service_domains.is_empty()
        && !connector
            .allowed_service_domains
            .iter()
            .any(|value| value.eq_ignore_ascii_case(domain))
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!(
                "Home Assistant service domain '{}' is not allowed for connector '{}'",
                domain, connector.id
            ),
        ));
    }
    if payload
        .service_data
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant service_data must be a JSON object when provided",
        ));
    }
    let targeted_entities = collect_home_assistant_target_entities(payload)?;
    ensure_home_assistant_service_targets_allowed(connector, &targeted_entities, payload)?;
    let mut body = payload
        .service_data
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(entity_id) = payload
        .entity_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(object) = body.as_object_mut() {
            object.insert(
                "entity_id".to_string(),
                serde_json::Value::String(entity_id.to_string()),
            );
        }
    }
    let response = client
        .post(format!(
            "{}/api/services/{}/{}",
            canonical_home_assistant_base_url(&connector.base_url),
            domain,
            service
        ))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                format!("Home Assistant service call failed: {error}"),
            )
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to read Home Assistant service response: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("Home Assistant service {domain}.{service} failed: {status} {body}"),
        ));
    }
    let parsed = serde_json::from_str::<serde_json::Value>(&body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("failed to parse Home Assistant service response: {error}"),
        )
    })?;
    Ok(parsed.as_array().map_or(0, Vec::len))
}

pub(super) fn ensure_home_assistant_entity_allowed(
    connector: &HomeAssistantConnectorConfig,
    entity_id: &str,
    action: &str,
) -> Result<(), ApiError> {
    let entity_id = entity_id.trim();
    if entity_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Home Assistant entity_id must not be empty",
        ));
    }
    if !connector.monitored_entity_ids.is_empty()
        && !connector
            .monitored_entity_ids
            .iter()
            .any(|value| value == entity_id)
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!(
                "Home Assistant entity '{}' is not allowed for connector '{}' {}",
                entity_id, connector.id, action
            ),
        ));
    }
    Ok(())
}

pub(super) fn collect_home_assistant_target_entities(
    payload: &HomeAssistantServiceCallRequest,
) -> Result<Vec<String>, ApiError> {
    let mut entities = Vec::new();
    if let Some(entity_id) = payload
        .entity_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        entities.push(entity_id.to_string());
    }
    if let Some(service_data) = payload.service_data.as_ref() {
        collect_home_assistant_entities_from_value(service_data.get("entity_id"), &mut entities)?;
        if let Some(target) = service_data.get("target") {
            if !target.is_object() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "Home Assistant target must be a JSON object when provided",
                ));
            }
            collect_home_assistant_entities_from_value(target.get("entity_id"), &mut entities)?;
        }
    }
    entities.sort();
    entities.dedup();
    Ok(entities)
}

fn collect_home_assistant_entities_from_value(
    value: Option<&serde_json::Value>,
    entities: &mut Vec<String>,
) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Ok(());
    };
    match value {
        serde_json::Value::String(entity_id) => {
            let entity_id = entity_id.trim();
            if entity_id.is_empty() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "Home Assistant entity_id values must not be empty",
                ));
            }
            entities.push(entity_id.to_string());
        }
        serde_json::Value::Array(values) => {
            for entry in values {
                let entity_id = entry
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        ApiError::new(
                            StatusCode::BAD_REQUEST,
                            "Home Assistant entity_id arrays must contain only non-empty strings",
                        )
                    })?;
                entities.push(entity_id.to_string());
            }
        }
        _ => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "Home Assistant entity_id must be a string or array of strings",
            ));
        }
    }
    Ok(())
}

fn home_assistant_uses_unsupported_target_selector(
    service_data: Option<&serde_json::Value>,
) -> bool {
    let Some(service_data) = service_data else {
        return false;
    };
    service_data
        .get("device_id")
        .or_else(|| service_data.get("area_id"))
        .is_some()
        || service_data
            .get("target")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|target| {
                target.contains_key("device_id") || target.contains_key("area_id")
            })
}

pub(super) fn ensure_home_assistant_service_targets_allowed(
    connector: &HomeAssistantConnectorConfig,
    targeted_entities: &[String],
    payload: &HomeAssistantServiceCallRequest,
) -> Result<(), ApiError> {
    if connector.allowed_service_entity_ids.is_empty() {
        return Ok(());
    }
    if home_assistant_uses_unsupported_target_selector(payload.service_data.as_ref()) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            format!(
                "Home Assistant connector '{}' does not allow device_id or area_id targets when entity restrictions are configured",
                connector.id
            ),
        ));
    }
    for entity_id in targeted_entities {
        if !connector
            .allowed_service_entity_ids
            .iter()
            .any(|value| value == entity_id)
        {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                format!(
                    "Home Assistant entity '{}' is not allowed for connector '{}'",
                    entity_id, connector.id
                ),
            ));
        }
    }
    Ok(())
}

async fn persist_home_assistant_entity_cursor(
    state: &AppState,
    connector_id: &str,
    entity_state: &HomeAssistantEntityState,
) -> Result<(), ApiError> {
    let mut config = state.config.write().await;
    let Some(connector) = config
        .home_assistant_connectors
        .iter_mut()
        .find(|connector| connector.id == connector_id)
    else {
        return Ok(());
    };
    if let Some(cursor) = connector
        .entity_cursors
        .iter_mut()
        .find(|cursor| cursor.entity_id == entity_state.entity_id)
    {
        cursor.last_state = Some(entity_state.state.clone());
        cursor.last_changed = entity_state.last_changed.clone();
    } else {
        connector.entity_cursors.push(HomeAssistantEntityCursor {
            entity_id: entity_state.entity_id.clone(),
            last_state: Some(entity_state.state.clone()),
            last_changed: entity_state.last_changed.clone(),
        });
    }
    connector
        .entity_cursors
        .sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
    state.storage.save_config(&config)?;
    Ok(())
}

fn home_assistant_mission_id(connector_id: &str, entity: &HomeAssistantEntityState) -> String {
    let marker = entity
        .last_changed
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(entity.last_updated.as_deref())
        .unwrap_or(entity.state.as_str());
    format!(
        "home-assistant:{}:{}:{}",
        connector_id.trim(),
        entity.entity_id.trim(),
        marker.trim()
    )
}

fn build_home_assistant_mission_details(
    connector: &HomeAssistantConnectorConfig,
    previous: Option<&HomeAssistantEntityCursor>,
    entity: &HomeAssistantEntityState,
) -> String {
    let attributes = serde_json::to_string_pretty(&entity.attributes)
        .unwrap_or_else(|_| entity.attributes.to_string());
    format!(
        "Home Assistant connector: {}\nEntity: {} ({})\nPrevious state: {}\nNew state: {}\nLast changed: {}\nLast updated: {}\nAttributes:\n{}\n\nInspect the change, decide whether follow-up action is needed, and use the configured tools if appropriate.",
        connector.name,
        entity
            .friendly_name
            .as_deref()
            .unwrap_or(entity.entity_id.as_str()),
        entity.entity_id,
        previous
            .and_then(|cursor| cursor.last_state.as_deref())
            .unwrap_or("unknown"),
        entity.state,
        entity.last_changed.as_deref().unwrap_or("unknown"),
        entity.last_updated.as_deref().unwrap_or("unknown"),
        attributes
    )
}
