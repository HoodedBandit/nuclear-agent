use std::{collections::HashMap, sync::Arc};

use agent_core::{
    AuthMode, BrowserProviderAuthKind, BrowserProviderAuthSessionStatus,
    BrowserProviderAuthStartRequest, BrowserProviderAuthStartResponse,
    BrowserProviderAuthStatusResponse, KeyValuePair, ModelAlias, OAuthConfig, ProviderConfig,
    ProviderKind, ProviderProfile, ProviderUpsertRequest, DEFAULT_CHATGPT_CODEX_MODEL,
    DEFAULT_CHATGPT_CODEX_URL,
};
use agent_providers::{
    build_oauth_authorization_url, delete_secret, exchange_oauth_code, store_api_key,
    store_oauth_token,
};
use anyhow::{anyhow, bail, Context, Result};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::RwLock,
    time::timeout,
};
use url::Url;
use uuid::Uuid;

use crate::{append_log, ApiError, AppState};

const OAUTH_TIMEOUT_SECS: i64 = 300;
const BROWSER_AUTH_TERMINAL_TTL_SECS: i64 = 300;
const OPENAI_BROWSER_AUTH_ISSUER: &str = "https://auth.openai.com";
const OPENAI_BROWSER_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_BROWSER_ORIGINATOR: &str = "codex_cli_rs";
const OPENAI_BROWSER_CALLBACK_PORT: u16 = 1455;
const OPENAI_BROWSER_CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, Clone)]
pub(crate) struct BrowserAuthSessionRecord {
    session_id: String,
    kind: BrowserProviderAuthKind,
    provider: ProviderConfig,
    alias: Option<ModelAlias>,
    set_as_main: bool,
    code_verifier: Option<String>,
    oauth_state: Option<String>,
    redirect_uri: Option<String>,
    created_at: DateTime<Utc>,
    terminal_at: Option<DateTime<Utc>>,
    status: BrowserProviderAuthSessionStatus,
    error: Option<String>,
}

pub(crate) type BrowserAuthStore = Arc<RwLock<HashMap<String, BrowserAuthSessionRecord>>>;

pub(crate) fn new_browser_auth_store() -> BrowserAuthStore {
    Arc::new(RwLock::new(HashMap::new()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct BrowserAuthCompleteQuery {
    session: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct BrowserAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub(crate) async fn start_provider_browser_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BrowserProviderAuthStartRequest>,
) -> Result<Json<BrowserProviderAuthStartResponse>, ApiError> {
    let origin = request_origin(&headers, &state).await;
    let session_id = Uuid::new_v4().to_string();
    let completion_url = format!("{origin}/auth/provider/complete?session={session_id}");
    let (provider_request, alias) = build_provider_auth_request(&payload)?;

    let code_verifier = generate_code_verifier();
    let oauth_state = Uuid::new_v4().to_string();
    let listener = bind_provider_browser_listener(payload.kind)
        .await
        .map_err(ApiError::from)?;
    let redirect_uri = provider_browser_redirect_uri(payload.kind, &listener)?;
    let authorization_url = build_oauth_authorization_url(
        &provider_request.provider,
        &redirect_uri,
        &oauth_state,
        &pkce_challenge(&code_verifier),
    )?;
    let session = BrowserAuthSessionRecord {
        session_id: session_id.clone(),
        kind: payload.kind,
        provider: provider_request.provider,
        alias,
        set_as_main: payload.set_as_main,
        code_verifier: Some(code_verifier),
        oauth_state: Some(oauth_state),
        redirect_uri: Some(redirect_uri),
        created_at: Utc::now(),
        terminal_at: None,
        status: BrowserProviderAuthSessionStatus::Pending,
        error: None,
    };
    let mut sessions = state.browser_auth_sessions.write().await;
    prune_expired_browser_auth_sessions(&mut sessions);
    sessions.insert(session_id.clone(), session);
    drop(sessions);
    tokio::spawn(run_provider_browser_callback_listener(
        state.clone(),
        session_id.clone(),
        payload.kind,
        listener,
        completion_url,
    ));
    append_log(
        &state,
        "info",
        "providers",
        format!(
            "started GUI browser sign-in for provider '{}' ({})",
            payload.provider_id.trim(),
            auth_kind_label(payload.kind)
        ),
    )?;
    Ok(Json(BrowserProviderAuthStartResponse {
        session_id,
        status: BrowserProviderAuthSessionStatus::Pending,
        authorization_url: Some(authorization_url),
    }))
}

pub(crate) async fn get_provider_browser_auth_status(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<BrowserProviderAuthStatusResponse>, ApiError> {
    let mut sessions = state.browser_auth_sessions.write().await;
    prune_expired_browser_auth_sessions(&mut sessions);
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown browser auth session"))?;
    expire_pending_session(session);
    Ok(Json(to_status_response(session)))
}

pub(crate) async fn provider_browser_auth_callback(
    State(state): State<AppState>,
    Query(query): Query<BrowserAuthCallbackQuery>,
) -> Response {
    let Some(oauth_state) = query.state.as_deref() else {
        return auth_popup_response(
            None,
            "Sign-in failed",
            "The callback did not include a login state.",
            false,
        )
        .into_response();
    };

    let session_id = {
        let mut sessions = state.browser_auth_sessions.write().await;
        prune_expired_browser_auth_sessions(&mut sessions);
        sessions
            .iter()
            .find(|(_, session)| session.oauth_state.as_deref() == Some(oauth_state))
            .map(|(id, _)| id.clone())
    };

    let Some(session_id) = session_id else {
        return auth_popup_response(
            None,
            "Sign-in failed",
            "This sign-in session could not be matched to the daemon state.",
            false,
        )
        .into_response();
    };

    let result = finalize_provider_browser_auth(&state, &session_id, &query).await;
    if let Err(error) = result {
        let _ = mark_browser_auth_failed(&state, &session_id, &error.to_string()).await;
    }

    Redirect::temporary(&format!("/auth/provider/complete?session={session_id}")).into_response()
}

pub(crate) async fn provider_browser_auth_complete(
    State(state): State<AppState>,
    Query(query): Query<BrowserAuthCompleteQuery>,
) -> impl IntoResponse {
    let Some(session_id) = query.session.as_deref() else {
        return auth_popup_response(
            None,
            "Sign-in failed",
            "The completion page did not receive a sign-in session id.",
            false,
        );
    };

    let session = {
        let mut sessions = state.browser_auth_sessions.write().await;
        prune_expired_browser_auth_sessions(&mut sessions);
        let Some(session) = sessions.get_mut(session_id) else {
            return auth_popup_response(
                None,
                "Sign-in failed",
                "The daemon could not find this sign-in session anymore.",
                false,
            );
        };
        expire_pending_session(session);
        session.clone()
    };

    match session.status {
        BrowserProviderAuthSessionStatus::Completed => auth_popup_response(
            Some(&session.session_id),
            &format!("{} connected", auth_kind_label(session.kind)),
            &format!(
                "{} credentials were saved for provider '{}'. You can return to the dashboard.",
                auth_kind_label(session.kind),
                session.provider.display_name
            ),
            true,
        ),
        BrowserProviderAuthSessionStatus::Failed => auth_popup_response(
            Some(&session.session_id),
            "Sign-in failed",
            session
                .error
                .as_deref()
                .unwrap_or("The daemon could not complete the browser sign-in."),
            false,
        ),
        BrowserProviderAuthSessionStatus::Pending => auth_popup_response(
            Some(&session.session_id),
            "Waiting for sign-in",
            "The daemon is still waiting for the provider callback.",
            false,
        ),
    }
}

fn build_provider_auth_request(
    payload: &BrowserProviderAuthStartRequest,
) -> Result<(ProviderUpsertRequest, Option<ModelAlias>), ApiError> {
    let provider_id = payload.provider_id.trim();
    let display_name = payload.display_name.trim();
    if provider_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "provider_id must not be empty",
        ));
    }
    if display_name.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "display_name must not be empty",
        ));
    }

    let default_model = payload
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| default_browser_auth_model(payload.kind).map(ToOwned::to_owned));

    let alias_name = payload
        .alias_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let alias = match alias_name {
        Some(alias_name) => {
            let alias_model = payload
                .alias_model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| default_model.clone())
                .ok_or_else(|| {
                    ApiError::new(
                        StatusCode::BAD_REQUEST,
                        "alias_model or default_model is required when alias_name is set",
                    )
                })?;
            Some(ModelAlias {
                alias: alias_name,
                provider_id: provider_id.to_string(),
                model: alias_model,
                description: payload
                    .alias_description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned),
            })
        }
        None => None,
    };

    let request = match payload.kind {
        BrowserProviderAuthKind::Codex => ProviderUpsertRequest {
            provider: ProviderConfig {
                id: provider_id.to_string(),
                display_name: display_name.to_string(),
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                provider_profile: Some(ProviderProfile::OpenAi),
                auth_mode: AuthMode::OAuth,
                default_model,
                keychain_account: None,
                oauth: Some(openai_browser_oauth_config()),
                local: false,
            },
            api_key: None,
            oauth_token: None,
        },
    };

    Ok((request, alias))
}

fn default_browser_auth_model(kind: BrowserProviderAuthKind) -> Option<&'static str> {
    match kind {
        BrowserProviderAuthKind::Codex => Some(DEFAULT_CHATGPT_CODEX_MODEL),
    }
}

async fn bind_provider_browser_listener(kind: BrowserProviderAuthKind) -> Result<TcpListener> {
    let (preferred_port, label) = match kind {
        BrowserProviderAuthKind::Codex => (OPENAI_BROWSER_CALLBACK_PORT, "OpenAI browser callback"),
    };
    bind_preferred_callback_listener(preferred_port, label).await
}

async fn bind_preferred_callback_listener(preferred_port: u16, label: &str) -> Result<TcpListener> {
    match TcpListener::bind(("127.0.0.1", preferred_port)).await {
        Ok(listener) => Ok(listener),
        Err(error) => {
            append_bind_fallback_log(label, preferred_port, &error);
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .with_context(|| format!("failed to bind local {label} listener"))
        }
    }
}

fn append_bind_fallback_log(label: &str, preferred_port: u16, error: &std::io::Error) {
    tracing::warn!(
        "{label} could not bind preferred port {} ({}); falling back to an ephemeral local port",
        preferred_port,
        error
    );
}

fn provider_browser_redirect_uri(
    kind: BrowserProviderAuthKind,
    listener: &TcpListener,
) -> Result<String> {
    Ok(format!(
        "http://localhost:{}{}",
        listener
            .local_addr()
            .context("failed to inspect browser callback listener")?
            .port(),
        provider_browser_callback_path(kind)
    ))
}

fn provider_browser_callback_path(kind: BrowserProviderAuthKind) -> &'static str {
    match kind {
        BrowserProviderAuthKind::Codex => OPENAI_BROWSER_CALLBACK_PATH,
    }
}

async fn run_provider_browser_callback_listener(
    state: AppState,
    session_id: String,
    kind: BrowserProviderAuthKind,
    listener: TcpListener,
    completion_url: String,
) {
    let accept = timeout(
        std::time::Duration::from_secs(OAUTH_TIMEOUT_SECS as u64),
        listener.accept(),
    )
    .await;

    let (mut stream, _) = match accept {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            let message = format!(
                "failed to accept {} connection: {error}",
                auth_kind_label(kind)
            );
            let _ = mark_browser_auth_failed(&state, &session_id, &message).await;
            let _ = append_log(&state, "warn", "providers", &message);
            return;
        }
        Err(_) => {
            let message = format!(
                "{} sign-in timed out waiting for the local browser callback.",
                auth_kind_label(kind)
            );
            let _ = mark_browser_auth_failed(&state, &session_id, &message).await;
            let _ = append_log(&state, "warn", "providers", &message);
            return;
        }
    };

    let result = async {
        let request = read_local_http_request(&mut stream).await?;
        let url = parse_callback_request_url(&request, auth_kind_label(kind))?;
        if url.path() != provider_browser_callback_path(kind) {
            bail!(
                "{} browser callback used unexpected path '{}'",
                auth_kind_label(kind),
                url.path()
            );
        }
        let query = parse_browser_callback_query(&url);
        finalize_provider_browser_auth(&state, &session_id, &query).await
    }
    .await;

    if let Err(error) = result {
        let _ = mark_browser_auth_failed(&state, &session_id, &error.to_string()).await;
        let _ = append_log(
            &state,
            "warn",
            "providers",
            format!(
                "{} browser sign-in callback failed for session {}: {error}",
                auth_kind_label(kind),
                session_id
            ),
        );
    }

    if write_redirect_response(&mut stream, &completion_url)
        .await
        .is_err()
    {
        let _ = write_html_response(
            &mut stream,
            "200 OK",
            "<html><body><h1>Login complete</h1><p>You can return to the dashboard.</p></body></html>",
        )
        .await;
    }
}

fn parse_browser_callback_query(url: &Url) -> BrowserAuthCallbackQuery {
    let mut query = BrowserAuthCallbackQuery::default();
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => query.code = Some(value.into_owned()),
            "state" => query.state = Some(value.into_owned()),
            "error" => query.error = Some(value.into_owned()),
            "error_description" => query.error_description = Some(value.into_owned()),
            _ => {}
        }
    }
    query
}

async fn finalize_provider_browser_auth(
    state: &AppState,
    session_id: &str,
    query: &BrowserAuthCallbackQuery,
) -> Result<()> {
    let session_snapshot = {
        let mut sessions = state.browser_auth_sessions.write().await;
        prune_expired_browser_auth_sessions(&mut sessions);
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("unknown browser auth session"))?;
        expire_pending_session(session);
        if session.status != BrowserProviderAuthSessionStatus::Pending {
            match session.status {
                BrowserProviderAuthSessionStatus::Completed => return Ok(()),
                BrowserProviderAuthSessionStatus::Failed => {
                    bail!(
                        "{}",
                        session
                            .error
                            .clone()
                            .unwrap_or_else(|| "browser sign-in already failed".to_string())
                    );
                }
                BrowserProviderAuthSessionStatus::Pending => {}
            }
        }
        session.clone()
    };

    if let Some(error_code) = query.error.as_deref() {
        let message = oauth_callback_error_message(error_code, query.error_description.as_deref());
        mark_browser_auth_failed(state, session_id, &message)
            .await
            .map_err(|error| anyhow!(error.message))?;
        append_log(
            state,
            "warn",
            "providers",
            format!(
                "GUI browser sign-in failed for provider '{}': {message}",
                session_snapshot.provider.id
            ),
        )?;
        return Ok(());
    }

    let code = query
        .code
        .as_deref()
        .ok_or_else(|| anyhow!("OAuth callback missing authorization code"))?;
    let returned_state = query
        .state
        .as_deref()
        .ok_or_else(|| anyhow!("OAuth callback missing state"))?;
    if session_snapshot.oauth_state.as_deref() != Some(returned_state) {
        let message = "OAuth callback state did not match expected login state".to_string();
        mark_browser_auth_failed(state, session_id, &message)
            .await
            .map_err(|error| anyhow!(error.message))?;
        bail!("{message}");
    }

    let redirect_uri = session_snapshot
        .redirect_uri
        .as_deref()
        .ok_or_else(|| anyhow!("browser sign-in session was missing redirect_uri"))?;
    let code_verifier = session_snapshot
        .code_verifier
        .as_deref()
        .ok_or_else(|| anyhow!("browser sign-in session was missing code_verifier"))?;

    let mut completed_request = ProviderUpsertRequest {
        provider: session_snapshot.provider.clone(),
        api_key: None,
        oauth_token: None,
    };
    match session_snapshot.kind {
        BrowserProviderAuthKind::Codex => {
            let token = exchange_oauth_code(
                &state.http_client,
                &completed_request.provider,
                code,
                code_verifier,
                redirect_uri,
            )
            .await?;
            completed_request.oauth_token = Some(token);
        }
    }

    persist_provider_browser_auth_result(
        state,
        completed_request.clone(),
        session_snapshot.alias.clone(),
        session_snapshot.set_as_main,
    )
    .await
    .map_err(|error| anyhow!(error.message))?;

    {
        let mut sessions = state.browser_auth_sessions.write().await;
        prune_expired_browser_auth_sessions(&mut sessions);
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow!("unknown browser auth session"))?;
        session.provider = completed_request.provider;
        session.status = BrowserProviderAuthSessionStatus::Completed;
        session.error = None;
        session.code_verifier = None;
        session.oauth_state = None;
        session.redirect_uri = None;
        session.terminal_at = Some(Utc::now());
    }

    append_log(
        state,
        "info",
        "providers",
        format!(
            "completed GUI browser sign-in for provider '{}' ({})",
            session_snapshot.provider.id,
            auth_kind_label(session_snapshot.kind)
        ),
    )?;
    Ok(())
}

async fn persist_provider_browser_auth_result(
    state: &AppState,
    mut request: ProviderUpsertRequest,
    alias: Option<ModelAlias>,
    set_as_main: bool,
) -> Result<(), ApiError> {
    let existing_account = {
        let config = state.config.read().await;
        config
            .get_provider(&request.provider.id)
            .and_then(|provider| provider.keychain_account.clone())
    };

    if let Some(api_key) = request.api_key.take() {
        let account = store_api_key(&request.provider.id, &api_key)?;
        request.provider.keychain_account = Some(account);
    }
    if let Some(token) = request.oauth_token.take() {
        let account = store_oauth_token(&request.provider.id, &token)?;
        request.provider.keychain_account = Some(account);
    }

    {
        let mut config = state.config.write().await;
        config.upsert_provider(request.provider.clone());
        if let Some(alias) = alias {
            if set_as_main {
                config.main_agent_alias = Some(alias.alias.clone());
            }
            config.upsert_alias(alias);
        }
        state.storage.save_config(&config)?;
    }

    if let Some(previous_account) = existing_account
        .filter(|account| Some(account) != request.provider.keychain_account.as_ref())
    {
        if let Err(error) = delete_secret(&previous_account) {
            append_log(
                state,
                "warn",
                "providers",
                format!(
                    "failed to delete replaced credentials for provider '{}': {error}",
                    request.provider.id
                ),
            )?;
        }
    }

    Ok(())
}

async fn mark_browser_auth_failed(
    state: &AppState,
    session_id: &str,
    message: &str,
) -> Result<(), ApiError> {
    let mut sessions = state.browser_auth_sessions.write().await;
    prune_expired_browser_auth_sessions(&mut sessions);
    let session = sessions
        .get_mut(session_id)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown browser auth session"))?;
    mark_terminal_browser_auth_session(
        session,
        BrowserProviderAuthSessionStatus::Failed,
        Some(message.to_string()),
    );
    Ok(())
}

fn to_status_response(session: &BrowserAuthSessionRecord) -> BrowserProviderAuthStatusResponse {
    BrowserProviderAuthStatusResponse {
        session_id: session.session_id.clone(),
        kind: session.kind,
        provider_id: session.provider.id.clone(),
        display_name: session.provider.display_name.clone(),
        status: session.status,
        error: session.error.clone(),
    }
}

fn expire_pending_session(session: &mut BrowserAuthSessionRecord) {
    if session.status != BrowserProviderAuthSessionStatus::Pending {
        return;
    }
    if Utc::now() - session.created_at > Duration::seconds(OAUTH_TIMEOUT_SECS) {
        mark_terminal_browser_auth_session(
            session,
            BrowserProviderAuthSessionStatus::Failed,
            Some("Timed out waiting for the provider callback.".to_string()),
        );
    }
}

fn mark_terminal_browser_auth_session(
    session: &mut BrowserAuthSessionRecord,
    status: BrowserProviderAuthSessionStatus,
    error: Option<String>,
) {
    session.status = status;
    session.error = error;
    session.code_verifier = None;
    session.oauth_state = None;
    session.redirect_uri = None;
    session.terminal_at = Some(Utc::now());
}

fn prune_expired_browser_auth_sessions(sessions: &mut HashMap<String, BrowserAuthSessionRecord>) {
    let now = Utc::now();
    sessions.retain(|_, session| {
        expire_pending_session(session);
        match session.status {
            BrowserProviderAuthSessionStatus::Pending => true,
            BrowserProviderAuthSessionStatus::Completed
            | BrowserProviderAuthSessionStatus::Failed => session
                .terminal_at
                .map(|timestamp| {
                    now - timestamp <= Duration::seconds(BROWSER_AUTH_TERMINAL_TTL_SECS)
                })
                .unwrap_or(false),
        }
    });
}

async fn request_origin(headers: &HeaderMap, state: &AppState) -> String {
    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return origin.trim_end_matches('/').to_string();
    }

    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http");
    if let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return format!("{scheme}://{host}");
    }

    let config = state.config.read().await;
    let host = if config.daemon.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        config.daemon.host.as_str()
    };
    format!("http://{}:{}", host, config.daemon.port)
}

fn openai_browser_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: OPENAI_BROWSER_CLIENT_ID.to_string(),
        authorization_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/authorize"),
        token_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/token"),
        scopes: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "offline_access".to_string(),
            "api.connectors.read".to_string(),
            "api.connectors.invoke".to_string(),
        ],
        extra_authorize_params: vec![
            KeyValuePair {
                key: "id_token_add_organizations".to_string(),
                value: "true".to_string(),
            },
            KeyValuePair {
                key: "codex_cli_simplified_flow".to_string(),
                value: "true".to_string(),
            },
            KeyValuePair {
                key: "originator".to_string(),
                value: OPENAI_BROWSER_ORIGINATOR.to_string(),
            },
        ],
        extra_token_params: Vec::new(),
    }
}

fn oauth_callback_error_message(error_code: &str, error_description: Option<&str>) -> String {
    if is_missing_codex_entitlement_error(error_code, error_description) {
        return "OpenAI browser sign-in is not enabled for this workspace account yet.".to_string();
    }
    if let Some(description) = error_description {
        if !description.trim().is_empty() {
            return format!("Sign-in failed: {description}");
        }
    }
    format!("Sign-in failed: {error_code}")
}

fn is_missing_codex_entitlement_error(error_code: &str, error_description: Option<&str>) -> bool {
    error_code == "access_denied"
        && error_description.is_some_and(|description| {
            description
                .to_ascii_lowercase()
                .contains("missing_codex_entitlement")
        })
}

fn generate_code_verifier() -> String {
    let mut verifier = String::new();
    while verifier.len() < 64 {
        verifier.push_str(&Uuid::new_v4().simple().to_string());
    }
    verifier.truncate(96);
    verifier
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buffer = vec![0_u8; 16_384];
    let bytes_read = timeout(
        std::time::Duration::from_secs(OAUTH_TIMEOUT_SECS as u64),
        stream.read(&mut buffer),
    )
    .await
    .context("timed out reading local browser callback")?
    .context("failed to read local browser callback")?;
    Ok(String::from_utf8_lossy(&buffer[..bytes_read]).to_string())
}

async fn write_html_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    body: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser response")
}

fn parse_callback_request_url(request: &str, label: &str) -> Result<Url> {
    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| anyhow!("{label} contained no request line"))?;
    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("{label} request line was invalid"))?;
    Url::parse(&format!("http://127.0.0.1{path}"))
        .with_context(|| format!("failed to parse {label} URL"))
}

async fn write_redirect_response(stream: &mut tokio::net::TcpStream, location: &str) -> Result<()> {
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser redirect")
}

fn auth_popup_response(
    session_id: Option<&str>,
    title: &str,
    message: &str,
    success: bool,
) -> impl IntoResponse {
    let payload = serde_json::json!({
        "type": "provider-auth",
        "sessionId": session_id,
        "success": success,
    });
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"Cache-Control\" content=\"no-store\" /><title>{}</title></head><body><main style=\"font-family: sans-serif; max-width: 560px; margin: 48px auto; padding: 0 16px;\"><h1>{}</h1><p>{}</p><p>You can return to the dashboard.</p></main><script>const payload = {}; if (window.opener && !window.opener.closed) {{ window.opener.postMessage(payload, window.location.origin); }} setTimeout(() => window.close(), 300);</script></body></html>",
        html_escape(title),
        html_escape(title),
        html_escape(message),
        payload
    );
    (
        [
            (header::CACHE_CONTROL, "no-store, max-age=0"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
            ),
        ],
        Html(body),
    )
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

fn auth_kind_label(kind: BrowserProviderAuthKind) -> &'static str {
    match kind {
        BrowserProviderAuthKind::Codex => "Codex",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_with_oauth(
        kind: ProviderKind,
        base_url: &str,
        oauth: OAuthConfig,
    ) -> ProviderConfig {
        ProviderConfig {
            id: "test-provider".to_string(),
            display_name: "Test Provider".to_string(),
            kind,
            base_url: base_url.to_string(),
            provider_profile: None,
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(oauth),
            local: false,
        }
    }

    fn query_map(url: &Url) -> std::collections::HashMap<String, String> {
        url.query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    #[test]
    fn build_codex_browser_auth_request_uses_codex_provider_defaults() {
        let (request, alias) = build_provider_auth_request(&BrowserProviderAuthStartRequest {
            kind: BrowserProviderAuthKind::Codex,
            provider_id: "codex".to_string(),
            display_name: "Codex".to_string(),
            default_model: Some("gpt-5-codex".to_string()),
            alias_name: Some("main".to_string()),
            alias_model: None,
            alias_description: Some("Primary Codex alias".to_string()),
            set_as_main: true,
        })
        .expect("codex request should build");

        assert_eq!(request.provider.kind, ProviderKind::ChatGptCodex);
        assert_eq!(request.provider.base_url, DEFAULT_CHATGPT_CODEX_URL);
        assert_eq!(request.provider.auth_mode, AuthMode::OAuth);
        assert!(request.provider.oauth.is_some());
        assert_eq!(
            request.provider.default_model.as_deref(),
            Some("gpt-5-codex")
        );
        assert_eq!(
            alias.as_ref().map(|item| item.model.as_str()),
            Some("gpt-5-codex")
        );
    }

    #[tokio::test]
    async fn provider_browser_redirect_uri_uses_expected_loopback_contract() {
        let codex_listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("codex listener should bind");

        let codex_redirect =
            provider_browser_redirect_uri(BrowserProviderAuthKind::Codex, &codex_listener)
                .expect("codex redirect should build");

        let codex_url = Url::parse(&codex_redirect).expect("codex redirect should parse");
        assert_eq!(codex_url.scheme(), "http");
        assert_eq!(codex_url.host_str(), Some("localhost"));
        assert_eq!(codex_url.path(), OPENAI_BROWSER_CALLBACK_PATH);
    }

    #[test]
    fn openai_authorization_url_matches_codex_contract() {
        let provider = provider_with_oauth(
            ProviderKind::ChatGptCodex,
            DEFAULT_CHATGPT_CODEX_URL,
            openai_browser_oauth_config(),
        );
        let redirect_uri = format!(
            "http://localhost:{OPENAI_BROWSER_CALLBACK_PORT}{OPENAI_BROWSER_CALLBACK_PATH}"
        );
        let authorization_url = build_oauth_authorization_url(
            &provider,
            &redirect_uri,
            "state-123",
            &pkce_challenge("verifier-123"),
        )
        .expect("authorization URL should build");
        let parsed = Url::parse(&authorization_url).expect("authorization URL should parse");
        let query = query_map(&parsed);

        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("auth.openai.com"));
        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(redirect_uri.as_str())
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state-123"));
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile email offline_access api.connectors.read api.connectors.invoke")
        );
        assert_eq!(
            query.get("id_token_add_organizations").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            query.get("codex_cli_simplified_flow").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            query.get("originator").map(String::as_str),
            Some(OPENAI_BROWSER_ORIGINATOR)
        );
    }

    #[test]
    fn parse_callback_request_url_reads_http_request_line() {
        let url = parse_callback_request_url(
            "GET /callback?code=abc&state=xyz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            "test callback",
        )
        .expect("callback URL should parse");
        assert_eq!(url.path(), "/callback");
        let query = query_map(&url);
        assert_eq!(query.get("code").map(String::as_str), Some("abc"));
        assert_eq!(query.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn parse_browser_callback_query_extracts_fields() {
        let url = Url::parse(
            "http://127.0.0.1/callback?code=abc123&state=state-123&error=access_denied&error_description=nope",
        )
        .expect("test URL should parse");
        let query = parse_browser_callback_query(&url);
        assert_eq!(query.code.as_deref(), Some("abc123"));
        assert_eq!(query.state.as_deref(), Some("state-123"));
        assert_eq!(query.error.as_deref(), Some("access_denied"));
        assert_eq!(query.error_description.as_deref(), Some("nope"));
    }

    #[test]
    fn oauth_callback_error_message_maps_missing_codex_entitlement() {
        assert_eq!(
            oauth_callback_error_message(
                "access_denied",
                Some("user is missing_codex_entitlement in this workspace")
            ),
            "OpenAI browser sign-in is not enabled for this workspace account yet."
        );
    }

    fn test_browser_auth_provider() -> ProviderConfig {
        ProviderConfig {
            id: "codex".to_string(),
            display_name: "Codex".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
            provider_profile: Some(ProviderProfile::OpenAi),
            auth_mode: AuthMode::OAuth,
            default_model: Some("gpt-5-codex".to_string()),
            keychain_account: None,
            oauth: Some(openai_browser_oauth_config()),
            local: false,
        }
    }

    #[test]
    fn expire_pending_session_marks_failure_and_clears_callback_state() {
        let mut session = BrowserAuthSessionRecord {
            session_id: "session-1".to_string(),
            kind: BrowserProviderAuthKind::Codex,
            provider: test_browser_auth_provider(),
            alias: None,
            set_as_main: false,
            code_verifier: Some("verifier".to_string()),
            oauth_state: Some("state".to_string()),
            redirect_uri: Some("http://localhost/callback".to_string()),
            created_at: Utc::now() - Duration::seconds(OAUTH_TIMEOUT_SECS + 1),
            terminal_at: None,
            status: BrowserProviderAuthSessionStatus::Pending,
            error: None,
        };

        expire_pending_session(&mut session);

        assert_eq!(session.status, BrowserProviderAuthSessionStatus::Failed);
        assert!(session.error.is_some());
        assert!(session.code_verifier.is_none());
        assert!(session.oauth_state.is_none());
        assert!(session.redirect_uri.is_none());
        assert!(session.terminal_at.is_some());
    }

    #[test]
    fn prune_expired_browser_auth_sessions_removes_old_terminal_records() {
        let mut sessions = HashMap::new();
        sessions.insert(
            "recent".to_string(),
            BrowserAuthSessionRecord {
                session_id: "recent".to_string(),
                kind: BrowserProviderAuthKind::Codex,
                provider: test_browser_auth_provider(),
                alias: None,
                set_as_main: false,
                code_verifier: None,
                oauth_state: None,
                redirect_uri: None,
                created_at: Utc::now(),
                terminal_at: Some(Utc::now()),
                status: BrowserProviderAuthSessionStatus::Completed,
                error: None,
            },
        );
        sessions.insert(
            "expired".to_string(),
            BrowserAuthSessionRecord {
                session_id: "expired".to_string(),
                kind: BrowserProviderAuthKind::Codex,
                provider: test_browser_auth_provider(),
                alias: None,
                set_as_main: false,
                code_verifier: None,
                oauth_state: None,
                redirect_uri: None,
                created_at: Utc::now() - Duration::seconds(BROWSER_AUTH_TERMINAL_TTL_SECS + 10),
                terminal_at: Some(
                    Utc::now() - Duration::seconds(BROWSER_AUTH_TERMINAL_TTL_SECS + 10),
                ),
                status: BrowserProviderAuthSessionStatus::Failed,
                error: Some("timed out".to_string()),
            },
        );

        prune_expired_browser_auth_sessions(&mut sessions);

        assert!(sessions.contains_key("recent"));
        assert!(!sessions.contains_key("expired"));
    }
}
