use super::*;

pub fn build_oauth_authorization_url(
    provider: &ProviderConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String> {
    let oauth = oauth_config(provider)?;
    let mut url = oauth_authorization_url(provider)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", &oauth.client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        if !oauth.scopes.is_empty() {
            query.append_pair("scope", &oauth.scopes.join(" "));
        }
        for extra in &oauth.extra_authorize_params {
            query.append_pair(&extra.key, &extra.value);
        }
    }
    Ok(url.into())
}

pub async fn exchange_oauth_code(
    client: &Client,
    provider: &ProviderConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    let oauth = oauth_config(provider)?;
    let token_request = post_validated_token_endpoint(client, provider)?;
    let form = base_token_form(oauth)
        .into_iter()
        .chain([
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("code_verifier".to_string(), code_verifier.to_string()),
        ])
        .collect::<Vec<_>>();

    let response = token_request
        .form(&form)
        .send()
        .await
        .context("failed to exchange OAuth authorization code")?;
    let status = response.status();
    let raw = response
        .text()
        .await
        .context("failed to read OAuth token response")?;
    if !status.is_success() {
        bail!(
            "OAuth token exchange failed: {}",
            parse_token_endpoint_error(&raw)
        );
    }
    let body: Value = serde_json::from_str(&raw).context("failed to parse OAuth token response")?;

    Ok(finalize_oauth_token(
        provider,
        parse_oauth_token(oauth, &body)?,
        None,
    ))
}

pub(crate) async fn apply_auth(
    client: &Client,
    provider: &ProviderConfig,
    request: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder> {
    apply_auth_with_overrides(client, provider, request, None, None).await
}

pub(crate) async fn apply_auth_with_overrides(
    client: &Client,
    provider: &ProviderConfig,
    request: reqwest::RequestBuilder,
    api_key_override: Option<&str>,
    oauth_token_override: Option<&OAuthToken>,
) -> Result<reqwest::RequestBuilder> {
    match provider.auth_mode {
        AuthMode::None => Ok(request),
        AuthMode::ApiKey => {
            let api_key = match api_key_override {
                Some(api_key) => api_key.to_string(),
                None => api_key_for(provider)?,
            };
            Ok(request.header(header::AUTHORIZATION, format!("Bearer {api_key}")))
        }
        AuthMode::OAuth => {
            let token = match oauth_token_override {
                Some(token) => token.clone(),
                None => oauth_token_for_request(client, provider).await?,
            };
            if uses_openai_api_key_exchange(provider) {
                let api_key = exchange_openai_api_key(client, provider, &token).await?;
                return Ok(request.header(header::AUTHORIZATION, format!("Bearer {api_key}")));
            }
            let token_type = token.token_type.as_deref().unwrap_or("Bearer");
            Ok(request.header(
                header::AUTHORIZATION,
                format!("{token_type} {}", token.access_token),
            ))
        }
    }
}

pub(crate) async fn oauth_token_for_request(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<OAuthToken> {
    let account = provider
        .keychain_account
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' is missing keychain metadata", provider.id))?;
    let lock = oauth_refresh_lock_for(account);
    let _guard = lock.lock().await;
    let token = load_oauth_token(account)?;
    let token = if token_needs_refresh(&token) {
        let refreshed = refresh_oauth_token(client, provider, &token).await?;
        store_oauth_token_for_account(account, &refreshed)?;
        refreshed
    } else {
        token
    };
    Ok(token)
}

pub(crate) async fn force_refresh_oauth_token_for_request(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<OAuthToken> {
    let account = provider
        .keychain_account
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' is missing keychain metadata", provider.id))?;
    let lock = oauth_refresh_lock_for(account);
    let _guard = lock.lock().await;
    let token = load_oauth_token(account)?;
    let refreshed = refresh_oauth_token(client, provider, &token).await?;
    store_oauth_token_for_account(account, &refreshed)?;
    Ok(refreshed)
}

pub(crate) async fn refresh_oauth_token(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<OAuthToken> {
    if is_openai_browser_oauth(provider) {
        return refresh_openai_oauth_token(client, provider, token).await;
    }

    let oauth = oauth_config(provider)?;
    let token_request = post_validated_token_endpoint(client, provider)?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' has no refresh token", provider.id))?;
    let form = base_token_form(oauth)
        .into_iter()
        .chain([
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("refresh_token".to_string(), refresh_token.to_string()),
        ])
        .collect::<Vec<_>>();

    let response = token_request
        .form(&form)
        .send()
        .await
        .context("failed to refresh OAuth token")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OAuth refresh response")?;
    if !status.is_success() {
        bail!("OAuth token refresh failed: {}", extract_error(&body));
    }

    let mut refreshed = parse_oauth_token(oauth, &body)?;
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = token.refresh_token.clone();
    }
    if refreshed.id_token.is_none() {
        refreshed.id_token = token.id_token.clone();
    }
    Ok(finalize_oauth_token(provider, refreshed, Some(token)))
}

pub(crate) fn token_needs_refresh(token: &OAuthToken) -> bool {
    token
        .expires_at
        .map(|expires_at| expires_at <= Utc::now() + Duration::seconds(OAUTH_REFRESH_SKEW_SECONDS))
        .unwrap_or(false)
}

pub(crate) fn parse_oauth_token(oauth: &OAuthConfig, body: &Value) -> Result<OAuthToken> {
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("OAuth response missing access_token"))?
        .to_string();
    let id_token = body
        .get("id_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let refresh_token = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let token_type = body
        .get("token_type")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let scopes = body
        .get("scope")
        .and_then(Value::as_str)
        .map(|scope| scope.split_whitespace().map(ToOwned::to_owned).collect())
        .unwrap_or_else(|| oauth.scopes.clone());
    let expires_at = parse_expires_in(body)
        .map(|seconds| Utc::now() + Duration::seconds(seconds))
        .filter(|expiry| *expiry > Utc::now());

    Ok(OAuthToken {
        access_token,
        refresh_token,
        expires_at,
        token_type,
        scopes,
        id_token,
        account_id: None,
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    })
}

pub(crate) fn parse_expires_in(body: &Value) -> Option<i64> {
    body.get("expires_in").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().map(|value| value as i64))
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    })
}

pub(crate) fn base_token_form(oauth: &OAuthConfig) -> Vec<(String, String)> {
    let mut form = vec![("client_id".to_string(), oauth.client_id.clone())];
    form.extend(
        oauth
            .extra_token_params
            .iter()
            .map(|param| (param.key.clone(), param.value.clone())),
    );
    form
}

pub(crate) fn oauth_config(provider: &ProviderConfig) -> Result<&OAuthConfig> {
    provider.validate_oauth_configuration()?;
    provider
        .oauth
        .as_ref()
        .ok_or_else(|| anyhow!("provider '{}' is missing OAuth configuration", provider.id))
}

fn oauth_authorization_url(provider: &ProviderConfig) -> Result<Url> {
    let oauth = oauth_config(provider)?;
    Url::parse(&oauth.authorization_url).context("failed to parse OAuth authorization URL")
}

fn oauth_token_url(provider: &ProviderConfig) -> Result<Url> {
    let oauth = oauth_config(provider)?;
    Url::parse(&oauth.token_url).context("failed to parse OAuth token URL")
}

fn validated_token_endpoint_url(provider: &ProviderConfig) -> Result<Url> {
    let url = oauth_token_url(provider)?;
    validate_oauth_post_endpoint(provider, &url)?;
    Ok(url)
}

fn post_validated_token_endpoint(
    client: &Client,
    provider: &ProviderConfig,
) -> Result<reqwest::RequestBuilder> {
    let url = oauth_token_url(provider)?;
    let host = url.host_str().ok_or_else(|| {
        anyhow!(
            "provider '{}' OAuth token URL is missing a host",
            provider.id
        )
    })?;
    match url.scheme() {
        "https" => Ok(client.post(url)),
        "http" if is_loopback_host(host) => Ok(client.post(url)),
        "http" => bail!(
            "provider '{}' OAuth token URL must use https unless it targets localhost or a loopback address",
            provider.id
        ),
        scheme => bail!(
            "provider '{}' OAuth token URL must use https or loopback-local http; found '{}'",
            provider.id,
            scheme
        ),
    }
}

fn validate_oauth_post_endpoint(provider: &ProviderConfig, url: &Url) -> Result<()> {
    let host = url.host_str().ok_or_else(|| {
        anyhow!(
            "provider '{}' OAuth token URL is missing a host",
            provider.id
        )
    })?;
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback_host(host) => Ok(()),
        "http" => bail!(
            "provider '{}' OAuth token URL must use https unless it targets localhost or a loopback address",
            provider.id
        ),
        scheme => bail!(
            "provider '{}' OAuth token URL must use https or loopback-local http; found '{}'",
            provider.id,
            scheme
        ),
    }
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let candidate = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    candidate
        .parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

pub(crate) async fn refresh_openai_oauth_token(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<OAuthToken> {
    let oauth = oauth_config(provider)?;
    let token_request = post_validated_token_endpoint(client, provider)?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' has no refresh token", provider.id))?;
    let response = token_request
        .json(&json!({
            "client_id": oauth.client_id,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .context("failed to refresh OpenAI browser token")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OpenAI refresh response")?;
    if !status.is_success() {
        bail!(
            "OpenAI browser token refresh failed: {}",
            extract_error(&body)
        );
    }

    let mut refreshed = parse_oauth_token(oauth, &body)?;
    if refreshed.refresh_token.is_none() {
        refreshed.refresh_token = token.refresh_token.clone();
    }
    if refreshed.id_token.is_none() {
        refreshed.id_token = token.id_token.clone();
    }
    Ok(finalize_oauth_token(provider, refreshed, Some(token)))
}

pub(crate) async fn exchange_openai_api_key(
    client: &Client,
    provider: &ProviderConfig,
    token: &OAuthToken,
) -> Result<String> {
    let oauth = oauth_config(provider)?;
    let id_token = token.id_token.as_deref().ok_or_else(|| {
        anyhow!(
            "provider '{}' is missing OpenAI id_token state",
            provider.id
        )
    })?;
    let token_url = validated_token_endpoint_url(provider)?;
    let issuer = format!(
        "{}://{}",
        token_url.scheme(),
        token_url
            .host_str()
            .ok_or_else(|| anyhow!("OpenAI token URL is missing a host"))?
    );
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "grant_type",
            "urn:ietf:params:oauth:grant-type:token-exchange",
        )
        .append_pair("client_id", &oauth.client_id)
        .append_pair("requested_token", "openai-api-key")
        .append_pair("subject_token", id_token)
        .append_pair(
            "subject_token_type",
            "urn:ietf:params:oauth:token-type:id_token",
        )
        .finish();
    let exchange_url = Url::parse(&format!("{issuer}/oauth/token"))
        .context("failed to build OpenAI API key exchange URL")?;
    validate_oauth_post_endpoint(provider, &exchange_url)?;
    let response = client
        .post(exchange_url)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .context("failed to exchange OpenAI browser token for API key")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("failed to parse OpenAI API key exchange response")?;
    if !status.is_success() {
        let error = extract_error(&body);
        if error.contains("missing organization_id") {
            bail!(
                "OpenAI browser sign-in succeeded, but this account is missing the organization access required to mint a platform API key. Finish setup at https://platform.openai.com/ or use API-key auth instead."
            );
        }
        bail!("OpenAI API key exchange failed: {error}");
    }

    body.get("access_token")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("OpenAI API key exchange response missing access_token"))
}

pub(crate) fn finalize_oauth_token(
    provider: &ProviderConfig,
    mut token: OAuthToken,
    previous: Option<&OAuthToken>,
) -> OAuthToken {
    if is_openai_browser_oauth(provider) {
        hydrate_openai_browser_token_metadata(&mut token);
    }
    if let Some(previous) = previous {
        preserve_oauth_token_metadata(&mut token, previous);
    }
    token
}

pub(crate) fn preserve_oauth_token_metadata(token: &mut OAuthToken, previous: &OAuthToken) {
    if token.account_id.is_none() {
        token.account_id = previous.account_id.clone();
    }
    if token.user_id.is_none() {
        token.user_id = previous.user_id.clone();
    }
    if token.org_id.is_none() {
        token.org_id = previous.org_id.clone();
    }
    if token.project_id.is_none() {
        token.project_id = previous.project_id.clone();
    }
    if token.display_email.is_none() {
        token.display_email = previous.display_email.clone();
    }
    if token.subscription_type.is_none() {
        token.subscription_type = previous.subscription_type.clone();
    }
}

pub(crate) fn hydrate_openai_browser_token_metadata(token: &mut OAuthToken) {
    let Some(id_token) = token.id_token.as_deref() else {
        return;
    };
    let Some(claims) = parse_openai_browser_claims(id_token) else {
        return;
    };

    token.account_id = claims.account_id.or(token.account_id.take());
    token.user_id = claims.user_id.or(token.user_id.take());
    token.org_id = claims.org_id.or(token.org_id.take());
    token.project_id = claims.project_id.or(token.project_id.take());
    token.display_email = claims.email.or(token.display_email.take());
    token.subscription_type = claims.subscription_type.or(token.subscription_type.take());
}

#[derive(Debug)]
pub(crate) struct OpenAiBrowserClaims {
    account_id: Option<String>,
    user_id: Option<String>,
    org_id: Option<String>,
    project_id: Option<String>,
    email: Option<String>,
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiIdClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/profile", default)]
    profile: Option<OpenAiProfileClaims>,
    #[serde(rename = "https://api.openai.com/auth", default)]
    auth: Option<OpenAiAuthClaims>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiProfileClaims {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiAuthClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    chatgpt_user_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    chatgpt_plan_type: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
    #[serde(default)]
    org_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

pub(crate) fn parse_openai_browser_claims(jwt: &str) -> Option<OpenAiBrowserClaims> {
    let payload = jwt.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: OpenAiIdClaims = serde_json::from_slice(&decoded).ok()?;
    let auth = claims.auth;
    let profile_email = claims.profile.and_then(|profile| profile.email);

    Some(OpenAiBrowserClaims {
        account_id: auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_account_id.clone()),
        user_id: auth.as_ref().and_then(|auth| {
            auth.chatgpt_user_id
                .clone()
                .or_else(|| auth.user_id.clone())
        }),
        org_id: auth
            .as_ref()
            .and_then(|auth| auth.organization_id.clone().or_else(|| auth.org_id.clone())),
        project_id: auth.as_ref().and_then(|auth| auth.project_id.clone()),
        email: claims.email.or(profile_email),
        subscription_type: auth.and_then(|auth| auth.chatgpt_plan_type),
    })
}

pub(crate) fn parse_token_endpoint_error(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "unknown error".to_string();
    }

    let parsed = match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => value,
        Err(_) => return redact_sensitive_text(trimmed),
    };

    if let Some(text) = parsed
        .get("error_description")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return redact_sensitive_text(text);
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return redact_sensitive_text(text);
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return redact_sensitive_text(text);
    }

    if let Some(text) = parsed
        .get("error")
        .and_then(Value::as_object)
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return redact_sensitive_text(text);
    }

    serde_json::to_string(&redact_sensitive_json_value(&parsed))
        .map(|text| redact_sensitive_text(&text))
        .unwrap_or_else(|_| "[REDACTED]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth_provider(authorization_url: &str, token_url: &str) -> ProviderConfig {
        ProviderConfig {
            id: "oauth-test".to_string(),
            display_name: "OAuth Test".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://example.invalid".to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: Some("oauth-test".to_string()),
            oauth: Some(OAuthConfig {
                client_id: "client-id".to_string(),
                authorization_url: authorization_url.to_string(),
                token_url: token_url.to_string(),
                scopes: vec!["openid".to_string()],
                extra_authorize_params: Vec::new(),
                extra_token_params: Vec::new(),
            }),
            local: false,
        }
    }

    #[test]
    fn build_oauth_authorization_url_rejects_remote_http() {
        let provider = oauth_provider(
            "http://example.invalid/oauth/authorize",
            "https://example.invalid/oauth/token",
        );

        let error = build_oauth_authorization_url(
            &provider,
            "http://127.0.0.1:8123/callback",
            "state",
            "challenge",
        )
        .unwrap_err();

        assert!(error.to_string().contains("https"));
    }

    #[test]
    fn build_oauth_authorization_url_allows_loopback_http() {
        let provider = oauth_provider(
            "http://127.0.0.1:8123/oauth/authorize",
            "http://127.0.0.1:8123/oauth/token",
        );

        let url = build_oauth_authorization_url(
            &provider,
            "http://127.0.0.1:3000/callback",
            "state",
            "challenge",
        )
        .unwrap();

        assert!(url.starts_with("http://127.0.0.1:8123/oauth/authorize"));
        assert!(url.contains("code_challenge=challenge"));
    }

    #[tokio::test]
    async fn exchange_oauth_code_rejects_invalid_token_url_before_request() {
        let provider = oauth_provider(
            "https://example.invalid/oauth/authorize",
            "http://example.invalid/oauth/token",
        );

        let error = exchange_oauth_code(
            &Client::new(),
            &provider,
            "code",
            "verifier",
            "http://127.0.0.1:3000/callback",
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("https"));
    }

    #[tokio::test]
    async fn refresh_oauth_token_rejects_invalid_token_url_before_request() {
        let provider = oauth_provider(
            "https://example.invalid/oauth/authorize",
            "http://example.invalid/oauth/token",
        );
        let token = OAuthToken {
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: None,
            token_type: Some("Bearer".to_string()),
            scopes: Vec::new(),
            id_token: None,
            account_id: None,
            user_id: None,
            org_id: None,
            project_id: None,
            display_email: None,
            subscription_type: None,
        };

        let error = refresh_oauth_token(&Client::new(), &provider, &token)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("https"));
    }

    #[test]
    fn parse_token_endpoint_error_redacts_nested_secret_fields() {
        let rendered = parse_token_endpoint_error(
            r#"{"error":{"message":"bad bearer Bearer sk-live-123456"},"refresh_token":"refresh-secret"}"#,
        );

        assert!(!rendered.contains("sk-live-123456"));
        assert!(!rendered.contains("refresh-secret"));
        assert!(rendered.contains("[REDACTED]"));
    }
}
