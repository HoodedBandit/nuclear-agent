use agent_providers::load_api_key;
use serde::Deserialize;

use super::super::argument_helpers::{
    optional_bool, optional_string, optional_string_array, required_string, required_string_array,
};
use super::*;

pub(super) fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        tool(
            "configure_home_assistant_connector",
            "Create or update a Home Assistant connector from a base URL and long-lived access token. Defaults to the current alias when alias is omitted.",
            json!({
                "type": "object",
                "properties": {
                    "base_url": {"type": "string"},
                    "access_token": {"type": "string"},
                    "id": {"type": "string"},
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "enabled": {"type": "boolean"},
                    "monitored_entity_ids": {"type": "array", "items": {"type": "string"}},
                    "allowed_service_domains": {"type": "array", "items": {"type": "string"}},
                    "allowed_service_entity_ids": {"type": "array", "items": {"type": "string"}},
                    "alias": {"type": "string"},
                    "requested_model": {"type": "string"},
                    "cwd": {"type": "string"}
                },
                "required": ["base_url", "access_token", "monitored_entity_ids"],
                "additionalProperties": false
            }),
        ),
        tool(
            "read_home_assistant_entity",
            "Read the current state of a Home Assistant entity through a configured connector.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "entity_id": {"type": "string"}
                },
                "required": ["entity_id"],
                "additionalProperties": false
            }),
        ),
        tool(
            "call_home_assistant_service",
            "Call an official Home Assistant service through a configured connector. Use this for device actions and automations.",
            json!({
                "type": "object",
                "properties": {
                    "connector_id": {"type": "string"},
                    "domain": {"type": "string"},
                    "service": {"type": "string"},
                    "entity_id": {"type": "string"},
                    "service_data": {"type": "object"}
                },
                "required": ["domain", "service"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub(super) async fn execute_tool_call(
    context: &ToolContext,
    tool_name: &str,
    args: &Value,
) -> Result<Option<String>> {
    let output = match tool_name {
        "configure_home_assistant_connector" => {
            configure_home_assistant_connector(context, args).await?
        }
        "read_home_assistant_entity" => read_home_assistant_entity_tool(context, args).await?,
        "call_home_assistant_service" => call_home_assistant_service_tool(context, args).await?,
        _ => return Ok(None),
    };
    Ok(Some(output))
}

fn canonical_home_assistant_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn ensure_home_assistant_entity_allowed_tool(
    connector: &HomeAssistantConnectorConfig,
    entity_id: &str,
    action: &str,
) -> Result<()> {
    if connector.monitored_entity_ids.is_empty()
        || connector
            .monitored_entity_ids
            .iter()
            .any(|allowed| allowed.trim() == entity_id)
    {
        Ok(())
    } else {
        bail!(
            "home assistant entity '{}' is not allowed for connector '{}' {action}",
            entity_id,
            connector.id
        )
    }
}

fn collect_home_assistant_target_entities_tool(
    request: &HomeAssistantServiceCallRequest,
) -> Result<Vec<String>> {
    let mut entities = Vec::new();
    if let Some(entity_id) = request
        .entity_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        entities.push(entity_id.to_string());
    }
    if let Some(service_data) = request.service_data.as_ref() {
        collect_home_assistant_entities_from_value_tool(service_data, &mut entities)?;
    }
    entities.sort();
    entities.dedup();
    Ok(entities)
}

fn collect_home_assistant_entities_from_value_tool(
    value: &Value,
    entities: &mut Vec<String>,
) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if key == "entity_id" {
                    match nested {
                        Value::String(text) => {
                            let text = text.trim();
                            if !text.is_empty() {
                                entities.push(text.to_string());
                            }
                        }
                        Value::Array(values) => {
                            for value in values {
                                if let Some(text) = value.as_str().map(str::trim).filter(|v| !v.is_empty()) {
                                    entities.push(text.to_string());
                                }
                            }
                        }
                        _ => bail!("home assistant service_data.entity_id must be a string or array of strings"),
                    }
                } else {
                    collect_home_assistant_entities_from_value_tool(nested, entities)?;
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_home_assistant_entities_from_value_tool(value, entities)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn home_assistant_uses_unsupported_target_selector_tool(service_data: Option<&Value>) -> bool {
    service_data
        .and_then(Value::as_object)
        .and_then(|map| map.get("target"))
        .and_then(Value::as_object)
        .is_some_and(|target| {
            target.keys().any(|key| {
                matches!(
                    key.as_str(),
                    "device_id" | "area_id" | "label_id" | "floor_id"
                )
            })
        })
}

fn ensure_home_assistant_service_targets_allowed_tool(
    connector: &HomeAssistantConnectorConfig,
    targeted_entities: &[String],
    request: &HomeAssistantServiceCallRequest,
) -> Result<()> {
    if !connector.allowed_service_domains.is_empty()
        && !connector
            .allowed_service_domains
            .iter()
            .any(|value| value.trim() == request.domain.trim())
    {
        bail!(
            "home assistant service domain '{}' is not allowed for connector '{}'",
            request.domain,
            connector.id
        );
    }
    if connector.allowed_service_entity_ids.is_empty() {
        return Ok(());
    }
    if home_assistant_uses_unsupported_target_selector_tool(request.service_data.as_ref()) {
        bail!(
            "home assistant target selectors like device_id/area_id are not allowed for connector '{}'",
            connector.id
        );
    }
    for entity in targeted_entities {
        if !connector
            .allowed_service_entity_ids
            .iter()
            .any(|value| value.trim() == entity)
        {
            bail!(
                "home assistant service target '{}' is not allowed for connector '{}'",
                entity,
                connector.id
            );
        }
    }
    Ok(())
}

#[derive(Deserialize, Clone, Serialize)]
struct HomeAssistantProfile {
    #[serde(default)]
    location_name: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

async fn fetch_home_assistant_profile(
    client: &Client,
    base_url: &str,
    access_token: &str,
) -> Result<HomeAssistantProfile> {
    let response = client
        .get(format!(
            "{}/api/config",
            canonical_home_assistant_base_url(base_url)
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .context("home assistant config request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read home assistant config response")?;
    if !status.is_success() {
        bail!("home assistant config request failed: {status} {body}");
    }
    serde_json::from_str(&body).context("failed to parse home assistant config response")
}

async fn resolve_home_assistant_connector_for_use(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<HomeAssistantConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .home_assistant_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown home assistant connector '{connector_id}'"));
    }
    match config.home_assistant_connectors.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no home assistant connectors are configured"),
        _ => bail!("multiple home assistant connectors are configured; specify connector_id"),
    }
}

async fn configure_home_assistant_connector(context: &ToolContext, args: &Value) -> Result<String> {
    ensure_connector_admin_allowed(context)?;
    let base_url = required_string(args, "base_url")?.trim();
    if base_url.is_empty() {
        bail!("base_url must not be empty");
    }
    let access_token = required_string(args, "access_token")?.trim();
    if access_token.is_empty() {
        bail!("access_token must not be empty");
    }
    let monitored_entity_ids = required_string_array(args, "monitored_entity_ids")?;
    if monitored_entity_ids.is_empty() {
        bail!("monitored_entity_ids must not be empty");
    }
    let profile =
        fetch_home_assistant_profile(&context.http_client, base_url, access_token).await?;
    let requested_id = optional_string(args, "id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            sanitize_connector_id(
                profile
                    .location_name
                    .clone()
                    .unwrap_or_else(|| "home-assistant".to_string()),
                "home-assistant",
            )
        });
    let existing = {
        let config = context.state.config.read().await;
        config
            .home_assistant_connectors
            .iter()
            .find(|entry| entry.id == requested_id)
            .cloned()
    };
    let account = store_api_key(
        &format!("connector:home-assistant:{requested_id}"),
        access_token,
    )?;
    let connector = HomeAssistantConnectorConfig {
        id: requested_id.clone(),
        name: optional_string(args, "name")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.name.clone()))
            .unwrap_or_else(|| {
                profile
                    .location_name
                    .clone()
                    .unwrap_or_else(|| "Home Assistant".to_string())
            }),
        description: optional_string(args, "description")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().map(|entry| entry.description.clone()))
            .unwrap_or_else(|| {
                format!(
                    "Home Assistant instance{}",
                    profile
                        .version
                        .as_deref()
                        .map(|version| format!(" v{version}"))
                        .unwrap_or_default()
                )
            }),
        enabled: optional_bool(args, "enabled")
            .or_else(|| existing.as_ref().map(|entry| entry.enabled))
            .unwrap_or(true),
        base_url: canonical_home_assistant_base_url(base_url),
        access_token_keychain_account: Some(account),
        monitored_entity_ids,
        allowed_service_domains: optional_string_array(args, "allowed_service_domains")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_service_domains.clone())
            })
            .unwrap_or_default(),
        allowed_service_entity_ids: optional_string_array(args, "allowed_service_entity_ids")
            .or_else(|| {
                existing
                    .as_ref()
                    .map(|entry| entry.allowed_service_entity_ids.clone())
            })
            .unwrap_or_default(),
        entity_cursors: existing
            .as_ref()
            .map(|entry| entry.entity_cursors.clone())
            .unwrap_or_default(),
        alias: optional_string(args, "alias")
            .map(ToOwned::to_owned)
            .or_else(|| existing.as_ref().and_then(|entry| entry.alias.clone()))
            .or_else(|| context.current_alias.clone()),
        requested_model: optional_string(args, "requested_model")
            .map(ToOwned::to_owned)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|entry| entry.requested_model.clone())
            }),
        cwd: optional_string(args, "cwd")
            .map(PathBuf::from)
            .or_else(|| existing.as_ref().and_then(|entry| entry.cwd.clone())),
    };
    {
        let mut config = context.state.config.write().await;
        config.upsert_home_assistant_connector(connector.clone());
        context.state.storage.save_config(&config)?;
    }
    append_log(
        &context.state,
        "info",
        "home_assistant",
        format!(
            "home assistant connector '{}' configured by agent tool",
            connector.id
        ),
    )?;
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector": connector,
        "profile": profile,
    }))
    .context("failed to serialize home assistant connector result")
}

async fn read_home_assistant_entity_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }
    let connector =
        resolve_home_assistant_connector_for_use(context, optional_string(args, "connector_id"))
            .await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "home assistant",
        &connector.id,
        "reading entity state",
    )?;
    let entity_id = required_string(args, "entity_id")?.trim();
    if entity_id.is_empty() {
        bail!("entity_id must not be empty");
    }
    ensure_home_assistant_entity_allowed_tool(&connector, entity_id, "for reads")?;
    let token_account = connector
        .access_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "home assistant connector '{}' has no token configured",
                connector.id
            )
        })?;
    let token = load_api_key(token_account)
        .with_context(|| format!("failed to load home assistant token for '{}'", connector.id))?;
    let response = context
        .http_client
        .get(format!(
            "{}/api/states/{}",
            canonical_home_assistant_base_url(&connector.base_url),
            entity_id
        ))
        .bearer_auth(token)
        .send()
        .await
        .context("home assistant state request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read home assistant state response")?;
    if !status.is_success() {
        bail!("home assistant state request failed: {status} {body}");
    }
    let parsed: Value =
        serde_json::from_str(&body).context("failed to parse home assistant state response")?;
    serde_json::to_string_pretty(&parsed).context("failed to serialize home assistant state")
}

async fn call_home_assistant_service_tool(context: &ToolContext, args: &Value) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }
    let connector =
        resolve_home_assistant_connector_for_use(context, optional_string(args, "connector_id"))
            .await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "home assistant",
        &connector.id,
        "calling services",
    )?;
    let domain = required_string(args, "domain")?.trim();
    if domain.is_empty() {
        bail!("domain must not be empty");
    }
    let service = required_string(args, "service")?.trim();
    if service.is_empty() {
        bail!("service must not be empty");
    }
    let token_account = connector
        .access_token_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "home assistant connector '{}' has no token configured",
                connector.id
            )
        })?;
    let token = load_api_key(token_account)
        .with_context(|| format!("failed to load home assistant token for '{}'", connector.id))?;
    let request = HomeAssistantServiceCallRequest {
        domain: domain.to_string(),
        service: service.to_string(),
        entity_id: optional_string(args, "entity_id").map(ToOwned::to_owned),
        service_data: args.get("service_data").cloned(),
    };
    let targeted_entities = collect_home_assistant_target_entities_tool(&request)?;
    ensure_home_assistant_service_targets_allowed_tool(&connector, &targeted_entities, &request)?;
    let mut service_body = request.service_data.clone().unwrap_or_else(|| json!({}));
    if !service_body.is_object() {
        bail!("service_data must be a JSON object when provided");
    }
    if let Some(entity_id) = request
        .entity_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        service_body["entity_id"] = Value::String(entity_id.to_string());
    }
    let response = context
        .http_client
        .post(format!(
            "{}/api/services/{}/{}",
            canonical_home_assistant_base_url(&connector.base_url),
            domain,
            service
        ))
        .bearer_auth(token)
        .json(&service_body)
        .send()
        .await
        .context("home assistant service call failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read home assistant service response")?;
    if !status.is_success() {
        bail!("home assistant service call failed: {status} {body}");
    }
    append_log(
        &context.state,
        "info",
        "home_assistant",
        format!(
            "home assistant connector '{}' called service {}.{} via tool",
            connector.id, domain, service
        ),
    )?;
    let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({ "raw": body }));
    serde_json::to_string_pretty(&json!({
        "ok": true,
        "connector_id": connector.id,
        "domain": domain,
        "service": service,
        "result": parsed,
    }))
    .context("failed to serialize home assistant service result")
}
