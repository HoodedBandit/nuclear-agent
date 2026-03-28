use agent_providers::load_api_key;
use anyhow::Context as _;
use reqwest::Client;
use serde::Serialize;
use url::Url;

use super::super::argument_helpers::{
    optional_bool, optional_string, optional_u64, required_string, truncate,
};
use super::*;

const BRAVE_API_BASE_URL: &str = "https://api.search.brave.com";
const DEFAULT_RESULT_COUNT: usize = 5;
const MAX_RESULT_COUNT: usize = 10;

#[derive(Clone, Copy)]
enum BraveSearchKind {
    Web,
    News,
    Images,
    Local,
}

impl BraveSearchKind {
    fn endpoint_path(self) -> &'static str {
        match self {
            BraveSearchKind::Web | BraveSearchKind::Local => "/res/v1/web/search",
            BraveSearchKind::News => "/res/v1/news/search",
            BraveSearchKind::Images => "/res/v1/images/search",
        }
    }

    fn endpoint_label(self) -> &'static str {
        match self {
            BraveSearchKind::Web => "web",
            BraveSearchKind::News => "news",
            BraveSearchKind::Images => "images",
            BraveSearchKind::Local => "local",
        }
    }
}

#[derive(Debug, Serialize)]
struct BraveToolResult {
    connector_id: String,
    endpoint: String,
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    more_results_available: Option<bool>,
    results: Vec<BraveResultItem>,
}

#[derive(Debug, Serialize)]
struct BraveResultItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail_url: Option<String>,
}

pub(super) fn tool_definitions(context: &ToolContext) -> Vec<ToolDefinition> {
    if !context.brave_connectors.iter().any(|connector| {
        connector.enabled && has_connector_secret(connector.api_key_keychain_account.as_deref())
    }) {
        return Vec::new();
    }

    vec![
        tool(
            "brave_web_search",
            "Search the web through a configured Brave Search connector. Use this to discover relevant pages before reading them with fetch_url.",
            common_search_schema(true, false),
        ),
        tool(
            "brave_news_search",
            "Search recent news through a configured Brave Search connector.",
            common_search_schema(true, false),
        ),
        tool(
            "brave_image_search",
            "Search images through a configured Brave Search connector.",
            common_search_schema(false, true),
        ),
        tool(
            "brave_local_search",
            "Search for local businesses and places through a configured Brave Search connector.",
            common_search_schema(false, false),
        ),
    ]
}

pub(super) async fn execute_tool_call(
    context: &ToolContext,
    tool_name: &str,
    args: &Value,
) -> Result<Option<String>> {
    let kind = match tool_name {
        "brave_web_search" => BraveSearchKind::Web,
        "brave_news_search" => BraveSearchKind::News,
        "brave_image_search" => BraveSearchKind::Images,
        "brave_local_search" => BraveSearchKind::Local,
        _ => return Ok(None),
    };
    Ok(Some(run_brave_search_tool(context, args, kind).await?))
}

fn common_search_schema(include_extra_snippets: bool, include_image_flags: bool) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert("query".to_string(), json!({ "type": "string" }));
    properties.insert("connector_id".to_string(), json!({ "type": "string" }));
    properties.insert(
        "count".to_string(),
        json!({ "type": "integer", "minimum": 1, "maximum": MAX_RESULT_COUNT }),
    );
    properties.insert("country".to_string(), json!({ "type": "string" }));
    properties.insert("search_lang".to_string(), json!({ "type": "string" }));
    properties.insert("freshness".to_string(), json!({ "type": "string" }));
    if include_extra_snippets {
        properties.insert("extra_snippets".to_string(), json!({ "type": "boolean" }));
    }
    if include_image_flags {
        properties.insert("safesearch".to_string(), json!({ "type": "string" }));
        properties.insert("spellcheck".to_string(), json!({ "type": "boolean" }));
    }
    json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": ["query"],
        "additionalProperties": false
    })
}

async fn run_brave_search_tool(
    context: &ToolContext,
    args: &Value,
    kind: BraveSearchKind,
) -> Result<String> {
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        bail!("network access is disabled by trust policy");
    }

    let connector =
        resolve_brave_connector_for_use(context, optional_string(args, "connector_id")).await?;
    ensure_connector_enabled_tool(
        connector.enabled,
        "brave",
        &connector.id,
        "searching the internet",
    )?;
    let api_key = load_brave_api_key(&connector)?;
    let query = required_string(args, "query")?.trim();
    if query.is_empty() {
        bail!("query must not be empty");
    }

    let response = search_brave(
        &context.http_client,
        BRAVE_API_BASE_URL,
        &api_key,
        query,
        args,
        kind,
    )
    .await?;

    let output = BraveToolResult {
        connector_id: connector.id,
        endpoint: kind.endpoint_label().to_string(),
        query: query.to_string(),
        more_results_available: response
            .body
            .get("web")
            .and_then(|value| value.get("more_results_available"))
            .and_then(Value::as_bool)
            .or_else(|| {
                response
                    .body
                    .get("more_results_available")
                    .and_then(Value::as_bool)
            }),
        results: normalize_results(kind, &response.body),
    };
    serde_json::to_string_pretty(&output).context("failed to serialize brave search result")
}

struct BraveApiResponse {
    body: Value,
}

async fn search_brave(
    client: &Client,
    base_url: &str,
    api_key: &str,
    query: &str,
    args: &Value,
    kind: BraveSearchKind,
) -> Result<BraveApiResponse> {
    let count = optional_u64(args, "count")
        .map(|value| value.clamp(1, MAX_RESULT_COUNT as u64) as usize)
        .unwrap_or(DEFAULT_RESULT_COUNT);
    let mut request = client
        .get(format!("{base_url}{}", kind.endpoint_path()))
        .header("X-Subscription-Token", api_key)
        .query(&[("q", query)])
        .query(&[("count", count.to_string())]);

    if let Some(country) = optional_string(args, "country")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.query(&[("country", country)]);
    }
    if let Some(search_lang) = optional_string(args, "search_lang")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.query(&[("search_lang", search_lang)]);
    }
    if let Some(freshness) = optional_string(args, "freshness")
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if matches!(kind, BraveSearchKind::Web | BraveSearchKind::News) {
            request = request.query(&[("freshness", freshness)]);
        }
    }
    if matches!(kind, BraveSearchKind::Web | BraveSearchKind::News) {
        if let Some(extra_snippets) = optional_bool(args, "extra_snippets") {
            request = request.query(&[(
                "extra_snippets",
                if extra_snippets { "true" } else { "false" },
            )]);
        }
    }
    if matches!(kind, BraveSearchKind::Images) {
        if let Some(safesearch) = optional_string(args, "safesearch")
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            request = request.query(&[("safesearch", safesearch)]);
        }
        if let Some(spellcheck) = optional_bool(args, "spellcheck") {
            request = request.query(&[("spellcheck", if spellcheck { "true" } else { "false" })]);
        }
    }

    let response = request
        .send()
        .await
        .context("brave search request failed")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read brave search response")?;
    if !status.is_success() {
        bail!(
            "brave search failed: {status} {}",
            truncate(body.trim(), 400)
        );
    }
    let body =
        serde_json::from_str::<Value>(&body).context("failed to parse brave search response")?;
    Ok(BraveApiResponse { body })
}

async fn resolve_brave_connector_for_use(
    context: &ToolContext,
    connector_id: Option<&str>,
) -> Result<BraveConnectorConfig> {
    let config = context.state.config.read().await;
    if let Some(connector_id) = connector_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return config
            .brave_connectors
            .iter()
            .find(|entry| entry.id == connector_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown brave connector '{connector_id}'"));
    }

    let enabled = config
        .brave_connectors
        .iter()
        .filter(|entry| entry.enabled)
        .cloned()
        .collect::<Vec<_>>();
    match enabled.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no enabled brave connectors are configured"),
        _ => bail!("multiple brave connectors are enabled; specify connector_id"),
    }
}

fn load_brave_api_key(connector: &BraveConnectorConfig) -> Result<String> {
    let account = connector
        .api_key_keychain_account
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "brave connector '{}' has no API key configured",
                connector.id
            )
        })?;
    load_api_key(account).with_context(|| {
        format!(
            "failed to load brave API key for connector '{}'",
            connector.id
        )
    })
}

fn has_connector_secret(account: Option<&str>) -> bool {
    account
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn normalize_results(kind: BraveSearchKind, body: &Value) -> Vec<BraveResultItem> {
    match kind {
        BraveSearchKind::Web => result_array(body, &[&["web", "results"]])
            .into_iter()
            .take(MAX_RESULT_COUNT)
            .map(normalize_generic_result)
            .collect(),
        BraveSearchKind::News => result_array(body, &[&["results"]])
            .into_iter()
            .take(MAX_RESULT_COUNT)
            .map(normalize_generic_result)
            .collect(),
        BraveSearchKind::Images => result_array(body, &[&["results"]])
            .into_iter()
            .take(MAX_RESULT_COUNT)
            .map(normalize_image_result)
            .collect(),
        BraveSearchKind::Local => normalize_local_results(body),
    }
}

fn normalize_local_results(body: &Value) -> Vec<BraveResultItem> {
    let locations = result_array(body, &[&["locations", "results"]]);
    if !locations.is_empty() {
        return locations
            .into_iter()
            .take(MAX_RESULT_COUNT)
            .map(normalize_local_result)
            .collect();
    }
    result_array(body, &[&["web", "results"]])
        .into_iter()
        .take(MAX_RESULT_COUNT)
        .map(normalize_generic_result)
        .collect()
}

fn result_array<'a>(body: &'a Value, paths: &[&[&str]]) -> Vec<&'a Value> {
    for path in paths {
        if let Some(results) = value_at_path(body, path).and_then(Value::as_array) {
            return results.iter().collect();
        }
    }
    Vec::new()
}

fn normalize_generic_result(value: &Value) -> BraveResultItem {
    BraveResultItem {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        title: first_string(
            value,
            &[&["title"], &["name"], &["meta_url", "hostname"], &["url"]],
        )
        .unwrap_or_else(|| "Untitled result".to_string()),
        url: first_string(
            value,
            &[&["url"], &["profile", "url"], &["meta_url", "url"]],
        ),
        snippet: first_string(
            value,
            &[&["description"], &["snippet"], &["extra_snippets", "0"]],
        )
        .map(|text| truncate(&text, 280)),
        source: first_string(value, &[&["meta_url", "hostname"], &["profile", "name"]]).or_else(
            || {
                first_string(value, &[&["url"]]).and_then(|url| {
                    Url::parse(&url)
                        .ok()
                        .and_then(|parsed| parsed.host_str().map(ToOwned::to_owned))
                })
            },
        ),
        address: None,
        thumbnail_url: None,
    }
}

fn normalize_image_result(value: &Value) -> BraveResultItem {
    BraveResultItem {
        thumbnail_url: first_string(
            value,
            &[
                &["thumbnail", "src"],
                &["thumbnail", "url"],
                &["properties", "url"],
            ],
        ),
        ..normalize_generic_result(value)
    }
}

fn normalize_local_result(value: &Value) -> BraveResultItem {
    BraveResultItem {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        title: first_string(value, &[&["title"], &["name"], &["address"], &["url"]])
            .unwrap_or_else(|| "Untitled place".to_string()),
        url: first_string(value, &[&["url"]]),
        snippet: first_string(value, &[&["description"], &["snippet"]])
            .map(|text| truncate(&text, 280)),
        source: first_string(value, &[&["profile", "name"], &["meta_url", "hostname"]]),
        address: first_string(
            value,
            &[&["address"], &["postal_address"], &["location", "address"]],
        ),
        thumbnail_url: first_string(value, &[&["thumbnail", "src"], &["thumbnail", "url"]]),
    }
}

fn first_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| {
        value_at_path(value, path).and_then(|candidate| match candidate {
            Value::String(text) => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Value::Array(values) => values.iter().find_map(|entry| {
                entry
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(ToOwned::to_owned)
            }),
            _ => None,
        })
    })
}

fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        match current {
            Value::Object(map) => {
                current = map.get(*segment)?;
            }
            Value::Array(items) => {
                let index = segment.parse::<usize>().ok()?;
                current = items.get(index)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Json, Router};
    use std::{
        net::{Ipv4Addr, SocketAddr},
        sync::{atomic::AtomicBool, Arc},
    };
    use tokio::{
        net::TcpListener,
        sync::{mpsc, Notify, RwLock},
    };

    use agent_core::{AppConfig, AutonomyProfile, PermissionPreset, TrustPolicy};

    use crate::{
        new_browser_auth_store, new_dashboard_session_store, AppState, ProviderRateLimiter,
    };

    fn test_state_with_brave(connectors: Vec<BraveConnectorConfig>) -> AppState {
        let storage = agent_storage::Storage::open_at(
            std::env::temp_dir().join(format!("agent-brave-tools-test-{}", uuid::Uuid::new_v4())),
        )
        .unwrap();
        let config = AppConfig {
            brave_connectors: connectors,
            ..AppConfig::default()
        };
        AppState {
            storage,
            config: Arc::new(RwLock::new(config)),
            http_client: Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: crate::new_dashboard_launch_store(),
            mission_cancellations: crate::new_mission_cancellation_store(),
            started_at: chrono::Utc::now(),
            shutdown: mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    fn test_context_with_brave(connectors: Vec<BraveConnectorConfig>) -> ToolContext {
        ToolContext {
            state: test_state_with_brave(connectors.clone()),
            cwd: std::env::temp_dir(),
            trust_policy: TrustPolicy {
                trusted_paths: vec![std::env::temp_dir()],
                allow_shell: true,
                allow_network: true,
                allow_full_disk: false,
                allow_self_edit: false,
            },
            autonomy: AutonomyProfile::default(),
            permission_preset: PermissionPreset::FullAuto,
            http_client: Client::new(),
            mcp_servers: Vec::new(),
            app_connectors: Vec::new(),
            plugin_tools: Vec::new(),
            brave_connectors: connectors,
            current_alias: Some("main".to_string()),
            default_thinking_level: None,
            task_mode: None,
            delegation: agent_core::DelegationConfig::default(),
            delegation_targets: Vec::new(),
            delegation_depth: 0,
            background: false,
            background_shell_allowed: true,
            background_network_allowed: true,
            background_self_edit_allowed: true,
        }
    }

    async fn start_mock_server() -> (String, tokio::task::JoinHandle<()>) {
        async fn web_handler() -> Json<Value> {
            Json(json!({
                "web": {
                    "more_results_available": true,
                    "results": [
                        {
                            "title": "Brave Result",
                            "url": "https://example.com/article",
                            "description": "Example description",
                            "meta_url": { "hostname": "example.com" }
                        }
                    ]
                }
            }))
        }

        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let app = Router::new().route("/res/v1/web/search", get(web_handler));
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), handle)
    }

    #[test]
    fn brave_tools_only_exist_when_enabled_connector_is_present() {
        let no_tools = tool_definitions(&test_context_with_brave(Vec::new()));
        assert!(no_tools.is_empty());

        let tools = tool_definitions(&test_context_with_brave(vec![BraveConnectorConfig {
            id: "brave".to_string(),
            name: "Brave".to_string(),
            description: String::new(),
            enabled: true,
            api_key_keychain_account: Some("account".to_string()),
            alias: None,
            requested_model: None,
            cwd: None,
        }]));
        assert!(tools.iter().any(|tool| tool.name == "brave_web_search"));
    }

    #[tokio::test]
    async fn brave_search_requires_connector_id_when_multiple_enabled() {
        let context = test_context_with_brave(vec![
            BraveConnectorConfig {
                id: "brave-1".to_string(),
                name: "Brave 1".to_string(),
                description: String::new(),
                enabled: true,
                api_key_keychain_account: Some("a".to_string()),
                alias: None,
                requested_model: None,
                cwd: None,
            },
            BraveConnectorConfig {
                id: "brave-2".to_string(),
                name: "Brave 2".to_string(),
                description: String::new(),
                enabled: true,
                api_key_keychain_account: Some("b".to_string()),
                alias: None,
                requested_model: None,
                cwd: None,
            },
        ]);
        let error = resolve_brave_connector_for_use(&context, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("multiple brave connectors are enabled"));
    }

    #[tokio::test]
    async fn brave_search_normalizes_mock_response() {
        let (base_url, handle) = start_mock_server().await;
        let result = search_brave(
            &Client::new(),
            &base_url,
            "test-token",
            "rust",
            &json!({ "count": 1 }),
            BraveSearchKind::Web,
        )
        .await
        .unwrap();
        let items = normalize_results(BraveSearchKind::Web, &result.body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Brave Result");
        assert_eq!(items[0].source.as_deref(), Some("example.com"));
        handle.abort();
    }
}
