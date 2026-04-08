use super::*;
use agent_core::ProviderProfile;
use dialoguer::FuzzySelect;
use sha2::{Digest, Sha256};

pub(crate) async fn interactive_provider_setup(
    theme: &ColorfulTheme,
    config: &AppConfig,
) -> Result<(ProviderUpsertRequest, ModelAlias)> {
    let choice = Select::with_theme(theme)
        .with_prompt("Choose a provider type")
        .items([
            "OpenAI hosted",
            "Anthropic hosted",
            "Moonshot hosted",
            "OpenRouter hosted",
            "Venice AI hosted",
            "Ollama local",
            "Local OpenAI-compatible endpoint (Kobold-style)",
        ])
        .default(0)
        .interact()?;

    let (default_id, default_name) = match choice {
        0 => ("openai", "OpenAI"),
        1 => ("anthropic", "Anthropic"),
        2 => ("moonshot", "Moonshot"),
        3 => ("openrouter", "OpenRouter"),
        4 => ("venice", "Venice AI"),
        5 => ("ollama-local", "Ollama"),
        6 => ("local-openai", "Local OpenAI-compatible"),
        _ => unreachable!("invalid provider selection"),
    };

    let id = next_available_provider_id(config, default_id);
    let name = default_name.to_string();

    let (request, model) = match choice {
        0 => {
            interactive_hosted_provider_request(
                theme,
                id.clone(),
                name,
                HostedKindArg::OpenaiCompatible,
            )
            .await?
        }
        1 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Anthropic)
                .await?
        }
        2 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Moonshot)
                .await?
        }
        3 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Openrouter)
                .await?
        }
        4 => {
            interactive_hosted_provider_request(theme, id.clone(), name, HostedKindArg::Venice)
                .await?
        }
        5 => {
            let base_url = ask_url(theme, DEFAULT_OLLAMA_URL)?;
            let mut provider = ProviderConfig {
                id: id.clone(),
                display_name: name,
                kind: ProviderKind::Ollama,
                base_url,
                provider_profile: Some(ProviderProfile::Ollama),
                auth_mode: AuthMode::None,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: true,
            };
            let model = determine_local_model(&provider, None, Some(theme)).await?;
            provider.default_model = Some(model.clone());
            (
                ProviderUpsertRequest {
                    provider,
                    api_key: None,
                    oauth_token: None,
                },
                model,
            )
        }
        6 => {
            let requires_key = Confirm::with_theme(theme)
                .with_prompt("Does the local endpoint require an API key?")
                .default(false)
                .interact()?;
            let base_url = ask_url(theme, DEFAULT_LOCAL_OPENAI_URL)?;
            if requires_key {
                let api_key = prompt_visible_api_key(theme, "API key")?;
                let mut request = ProviderUpsertRequest {
                    provider: ProviderConfig {
                        id: id.clone(),
                        display_name: name,
                        kind: ProviderKind::OpenAiCompatible,
                        base_url,
                        provider_profile: Some(ProviderProfile::LocalOpenAiCompatible),
                        auth_mode: AuthMode::ApiKey,
                        default_model: None,
                        keychain_account: None,
                        oauth: None,
                        local: true,
                    },
                    api_key: Some(api_key),
                    oauth_token: None,
                };
                let model = resolve_hosted_model_after_auth(theme, &request, None).await?;
                request.provider.default_model = Some(model.clone());
                (request, model)
            } else {
                let mut provider = ProviderConfig {
                    id: id.clone(),
                    display_name: name,
                    kind: ProviderKind::OpenAiCompatible,
                    base_url,
                    provider_profile: Some(ProviderProfile::LocalOpenAiCompatible),
                    auth_mode: AuthMode::None,
                    default_model: None,
                    keychain_account: None,
                    oauth: None,
                    local: true,
                };
                let model = determine_local_model(&provider, None, Some(theme)).await?;
                provider.default_model = Some(model.clone());
                (
                    ProviderUpsertRequest {
                        provider,
                        api_key: None,
                        oauth_token: None,
                    },
                    model,
                )
            }
        }
        _ => unreachable!("invalid provider selection"),
    };

    let alias_name: String = Input::with_theme(theme)
        .with_prompt("Alias for this model")
        .with_initial_text(default_alias_name(config, &request.provider, &model))
        .interact_text()?;

    let alias = ModelAlias {
        alias: alias_name,
        provider_id: id,
        model,
        description: None,
    };
    Ok((request, alias))
}

pub(crate) fn ask_url(theme: &ColorfulTheme, default_url: &str) -> Result<String> {
    Ok(Input::with_theme(theme)
        .with_prompt("Base URL")
        .with_initial_text(default_url)
        .interact_text()?)
}

pub(crate) fn prompt_for_model(theme: &ColorfulTheme) -> Result<String> {
    Ok(Input::with_theme(theme)
        .with_prompt("Default model")
        .interact_text()?)
}

pub(crate) async fn interactive_hosted_provider_request(
    theme: &ColorfulTheme,
    id: String,
    name: String,
    kind: HostedKindArg,
) -> Result<(ProviderUpsertRequest, String)> {
    let auth_method = select_auth_method(theme, kind)?;
    if kind == HostedKindArg::Anthropic && auth_method != AuthMethodArg::ApiKey {
        bail!("Anthropic third-party access requires an API key");
    }
    let base_url = match auth_method {
        AuthMethodArg::Browser => default_browser_hosted_url(kind).to_string(),
        AuthMethodArg::ApiKey | AuthMethodArg::Oauth => default_hosted_url(kind).to_string(),
    };
    let mut request = match auth_method {
        AuthMethodArg::Browser => match complete_browser_login(kind, &name).await? {
            BrowserLoginResult::ApiKey(api_key) => ProviderUpsertRequest {
                provider: ProviderConfig {
                    id,
                    display_name: name,
                    kind: hosted_kind_to_provider_kind(kind),
                    base_url,
                    provider_profile: Some(hosted_kind_to_provider_profile(kind)),
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
                    id,
                    display_name: name,
                    kind: browser_hosted_kind_to_provider_kind(kind),
                    base_url,
                    provider_profile: Some(hosted_kind_to_provider_profile(kind)),
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
                id,
                display_name: name,
                kind: hosted_kind_to_provider_kind(kind),
                base_url,
                provider_profile: Some(hosted_kind_to_provider_profile(kind)),
                auth_mode: AuthMode::ApiKey,
                default_model: None,
                keychain_account: None,
                oauth: None,
                local: false,
            },
            api_key: Some(prompt_visible_api_key(theme, "API key")?),
            oauth_token: None,
        },
        AuthMethodArg::Oauth => {
            let provider = build_oauth_provider(
                theme,
                id,
                name,
                hosted_kind_to_provider_kind(kind),
                hosted_kind_to_provider_profile(kind),
                &base_url,
            )?;
            let token = complete_oauth_login(&provider).await?;
            ProviderUpsertRequest {
                provider,
                api_key: None,
                oauth_token: Some(token),
            }
        }
    };

    let model = resolve_hosted_model_after_auth(theme, &request, None).await?;
    request.provider.default_model = Some(model.clone());

    Ok((request, model))
}

pub(crate) fn hosted_kind_defaults(kind: HostedKindArg) -> (&'static str, &'static str) {
    match kind {
        HostedKindArg::OpenaiCompatible => ("openai", "OpenAI"),
        HostedKindArg::Anthropic => ("anthropic", "Anthropic"),
        HostedKindArg::Moonshot => ("moonshot", "Moonshot"),
        HostedKindArg::Openrouter => ("openrouter", "OpenRouter"),
        HostedKindArg::Venice => ("venice", "Venice AI"),
    }
}

pub(crate) fn next_available_provider_id(config: &AppConfig, preferred: &str) -> String {
    config.next_available_provider_id(preferred)
}

pub(crate) fn default_alias_name(
    config: &AppConfig,
    provider: &ProviderConfig,
    model: &str,
) -> String {
    config.default_alias_name_for(&provider.id, model)
}

pub(crate) async fn resolve_hosted_model_after_auth(
    theme: &ColorfulTheme,
    request: &ProviderUpsertRequest,
    provided: Option<String>,
) -> Result<String> {
    let discovered = provider_list_models_with_overrides(
        &build_http_client(),
        &request.provider,
        request.api_key.as_deref(),
        request.oauth_token.as_ref(),
    )
    .await;

    if let Some(model) = provided {
        match &discovered {
            Ok(models) => {
                if !models.is_empty() && !models.iter().any(|candidate| candidate == &model) {
                    println!(
                        "Model '{}' was not present in discovery results for provider '{}'; using the manual value anyway.",
                        model, request.provider.id
                    );
                }
            }
            Err(error) if should_abort_after_auth_discovery_error(request, error) => {
                return Err(anyhow::anyhow!(error.to_string()));
            }
            Err(_) => {}
        }
        return Ok(model);
    }

    match discovered {
        Ok(models) if !models.is_empty() => select_discovered_model(theme, &models),
        Ok(_) => {
            println!("No models were returned automatically for this provider.");
            prompt_for_model(theme)
        }
        Err(error) => {
            if should_abort_after_auth_discovery_error(request, &error) {
                return Err(error);
            }
            println!("Could not load models automatically after authentication: {error}");
            prompt_for_model(theme)
        }
    }
}

fn select_discovered_model(theme: &ColorfulTheme, models: &[String]) -> Result<String> {
    let manual_label = "Enter a model manually";
    if models.len() == 1 {
        let choices = vec![
            format!("Use detected model '{}'", models[0]),
            manual_label.to_string(),
        ];
        let selection = Select::with_theme(theme)
            .with_prompt("Choose a model")
            .items(&choices)
            .default(0)
            .interact()?;
        if selection == 0 {
            println!("Detected model '{}'.", models[0]);
            return Ok(models[0].clone());
        }
        return prompt_for_model(theme);
    }

    let mut choices = models.to_vec();
    choices.push(manual_label.to_string());
    let selection = FuzzySelect::with_theme(theme)
        .with_prompt("Choose a model")
        .items(&choices)
        .default(0)
        .interact()?;
    if selection == choices.len() - 1 {
        return prompt_for_model(theme);
    }
    Ok(models[selection].clone())
}

pub(crate) fn should_abort_after_auth_discovery_error(
    request: &ProviderUpsertRequest,
    error: &anyhow::Error,
) -> bool {
    request.provider.auth_mode == AuthMode::OAuth
        && request
            .provider
            .oauth
            .as_ref()
            .is_some_and(|oauth| oauth.authorization_url.contains(OPENAI_BROWSER_AUTH_ISSUER))
        && error
            .to_string()
            .contains("missing the organization access required to mint a platform API key")
}

pub(crate) async fn determine_local_model(
    provider: &ProviderConfig,
    provided: Option<String>,
    theme: Option<&ColorfulTheme>,
) -> Result<String> {
    if let Some(model) = provided {
        return Ok(model);
    }

    match provider_list_models(&build_http_client(), provider).await {
        Ok(models) if !models.is_empty() => {
            if let Some(theme) = theme {
                if models.len() == 1 {
                    println!("Detected local model '{}'.", models[0]);
                    return Ok(models[0].clone());
                }
                let index = Select::with_theme(theme)
                    .with_prompt("Choose a model")
                    .items(&models)
                    .default(0)
                    .interact()?;
                return Ok(models[index].clone());
            }

            println!("Detected local model '{}'.", models[0]);
            Ok(models[0].clone())
        }
        Ok(_) => {
            if let Some(theme) = theme {
                prompt_for_model(theme)
            } else {
                bail!("local provider returned no models; pass --model explicitly")
            }
        }
        Err(error) => {
            if let Some(theme) = theme {
                println!("Could not detect models automatically: {error}");
                prompt_for_model(theme)
            } else {
                Err(error).context("could not detect a local model; pass --model explicitly")
            }
        }
    }
}

pub(crate) fn build_oauth_provider(
    theme: &ColorfulTheme,
    id: String,
    name: String,
    kind: ProviderKind,
    provider_profile: ProviderProfile,
    default_url: &str,
) -> Result<ProviderConfig> {
    let client_id = Input::with_theme(theme)
        .with_prompt("OAuth client id")
        .interact_text()?;
    let authorization_url = Input::with_theme(theme)
        .with_prompt("OAuth authorization URL")
        .interact_text()?;
    let token_url = Input::with_theme(theme)
        .with_prompt("OAuth token URL")
        .interact_text()?;
    let scopes_input: String = Input::with_theme(theme)
        .with_prompt("Scopes (space or comma separated, optional)")
        .allow_empty(true)
        .interact_text()?;
    let auth_params_input: String = Input::with_theme(theme)
        .with_prompt("Extra authorization params k=v,k=v (optional)")
        .allow_empty(true)
        .interact_text()?;
    let token_params_input: String = Input::with_theme(theme)
        .with_prompt("Extra token params k=v,k=v (optional)")
        .allow_empty(true)
        .interact_text()?;

    Ok(ProviderConfig {
        id,
        display_name: name,
        kind,
        base_url: ask_url(theme, default_url)?,
        provider_profile: Some(provider_profile),
        auth_mode: AuthMode::OAuth,
        default_model: None,
        keychain_account: None,
        oauth: Some(OAuthConfig {
            client_id,
            authorization_url,
            token_url,
            scopes: split_scopes(&scopes_input),
            extra_authorize_params: parse_key_value_list(&auth_params_input)?,
            extra_token_params: parse_key_value_list(&token_params_input)?,
        }),
        local: false,
    })
}

pub(crate) async fn complete_browser_login(
    kind: HostedKindArg,
    provider_name: &str,
) -> Result<BrowserLoginResult> {
    match kind {
        HostedKindArg::OpenaiCompatible => Ok(BrowserLoginResult::OAuthToken(
            complete_openai_browser_login().await?,
        )),
        HostedKindArg::Openrouter => Ok(BrowserLoginResult::ApiKey(
            complete_openrouter_browser_login().await?,
        )),
        HostedKindArg::Anthropic => {
            bail!(
                "Anthropic third-party access requires an API key; browser sign-in is unsupported"
            )
        }
        HostedKindArg::Moonshot | HostedKindArg::Venice => Ok(BrowserLoginResult::ApiKey(
            capture_browser_api_key(kind, provider_name).await?,
        )),
    }
}

pub(crate) fn openai_browser_oauth_config() -> OAuthConfig {
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

pub(crate) async fn complete_openai_browser_login() -> Result<OAuthToken> {
    let provider = ProviderConfig {
        id: "openai-browser".to_string(),
        display_name: "OpenAI Browser Session".to_string(),
        kind: ProviderKind::ChatGptCodex,
        base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
        provider_profile: Some(ProviderProfile::OpenAi),
        auth_mode: AuthMode::OAuth,
        default_model: None,
        keychain_account: None,
        oauth: Some(openai_browser_oauth_config()),
        local: false,
    };
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let listener = bind_openai_browser_listener(OPENAI_BROWSER_CALLBACK_PORT).await?;
    let redirect_uri = format!(
        "http://localhost:{}{OPENAI_BROWSER_CALLBACK_PATH}",
        listener
            .local_addr()
            .context("failed to inspect OpenAI browser callback listener")?
            .port()
    );
    let authorization_url =
        build_oauth_authorization_url(&provider, &redirect_uri, &state, &challenge)?;

    match webbrowser::open(&authorization_url) {
        Ok(_) => println!("Opened browser for OpenAI sign-in."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{authorization_url}\n");

    timeout(
        OAUTH_TIMEOUT,
        run_openai_browser_callback_loop(
            &client,
            &provider,
            listener,
            &state,
            &verifier,
            &redirect_uri,
        ),
    )
    .await
    .context("timed out waiting for OpenAI browser callback")?
}

pub(crate) async fn bind_openai_browser_listener(port: u16) -> Result<TcpListener> {
    let bind_address = format!("127.0.0.1:{port}");
    let mut cancel_attempted = false;

    for _ in 0..10 {
        match TcpListener::bind(&bind_address).await {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if !cancel_attempted {
                    cancel_attempted = true;
                    if let Err(cancel_error) = send_openai_browser_cancel_request(port) {
                        eprintln!(
                            "Failed to cancel previous OpenAI browser login server: {cancel_error}"
                        );
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }
            Err(error) => {
                return Err(error).context("failed to bind OpenAI browser callback server");
            }
        }
    }

    bail!("OpenAI browser callback port {bind_address} is already in use")
}

pub(crate) fn send_openai_browser_cancel_request(port: u16) -> Result<()> {
    let address = format!("127.0.0.1:{port}");
    let mut stream = std::net::TcpStream::connect(&address).with_context(|| {
        format!("failed to connect to existing OpenAI browser callback server at {address}")
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .context("failed to set OpenAI browser callback read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .context("failed to set OpenAI browser callback write timeout")?;
    stream
        .write_all(
            format!(
                "GET {OPENAI_BROWSER_CANCEL_PATH} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .context("failed to send OpenAI browser cancel request")?;
    let mut buffer = [0_u8; 64];
    let _ = stream.read(&mut buffer);
    Ok(())
}

pub(crate) async fn run_openai_browser_callback_loop(
    client: &Client,
    provider: &ProviderConfig,
    listener: TcpListener,
    expected_state: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken> {
    let success_url = format!(
        "http://localhost:{}{OPENAI_BROWSER_SUCCESS_PATH}",
        listener
            .local_addr()
            .context("failed to inspect OpenAI browser callback listener")?
            .port()
    );
    let mut pending_token = None;

    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .context("failed to accept OpenAI browser callback connection")?;
        let request = read_local_http_request(&mut stream).await?;
        let url = parse_callback_request_url(&request, "OpenAI browser callback")?;

        match url.path() {
            OPENAI_BROWSER_CALLBACK_PATH => {
                let code = match parse_openai_browser_callback(&url, expected_state) {
                    Ok(code) => code,
                    Err(error) => {
                        write_html_response(
                            &mut stream,
                            "400 Bad Request",
                            &render_openai_browser_error_page(&error.to_string()),
                        )
                        .await?;
                        return Err(error);
                    }
                };

                let token = match exchange_oauth_code(
                    client,
                    provider,
                    &code,
                    verifier,
                    redirect_uri,
                )
                .await
                {
                    Ok(token) => token,
                    Err(error) => {
                        write_html_response(
                            &mut stream,
                            "500 Internal Server Error",
                            &render_openai_browser_error_page(&error.to_string()),
                        )
                        .await?;
                        return Err(
                            error.context("OpenAI browser sign-in failed during token exchange")
                        );
                    }
                };

                write_redirect_response(&mut stream, &success_url).await?;
                pending_token = Some(token);
            }
            OPENAI_BROWSER_SUCCESS_PATH => {
                write_html_response(&mut stream, "200 OK", &render_openai_browser_success_page())
                    .await?;

                if let Some(token) = pending_token.take() {
                    return Ok(token);
                }
            }
            OPENAI_BROWSER_CANCEL_PATH => {
                write_html_response(
                    &mut stream,
                    "200 OK",
                    "<html><body><h1>Login cancelled</h1><p>You can return to the terminal.</p></body></html>",
                )
                .await?;
                bail!("OpenAI browser sign-in was cancelled");
            }
            _ => {
                write_html_response(
                    &mut stream,
                    "404 Not Found",
                    "<html><body><h1>Not found</h1></body></html>",
                )
                .await?;
            }
        }
    }
}

pub(crate) fn parse_openai_browser_callback(url: &Url, expected_state: &str) -> Result<String> {
    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;

    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    if state.as_deref() != Some(expected_state) {
        bail!("OpenAI browser callback state did not match expected login state");
    }

    if let Some(error_code) = error {
        bail!(
            "{}",
            oauth_callback_error_message(&error_code, error_description.as_deref())
        );
    }

    code.ok_or_else(|| anyhow!("OpenAI browser callback missing authorization code"))
}

pub(crate) fn render_openai_browser_success_page() -> String {
    "<html><body><h1>Signed in to OpenAI</h1><p>You can return to the terminal.</p></body></html>"
        .to_string()
}

pub(crate) fn render_openai_browser_error_page(message: &str) -> String {
    format!(
        "<html><body><h1>OpenAI sign-in failed</h1><p>{}</p></body></html>",
        escape_html(message)
    )
}

#[cfg(test)]
pub(crate) fn jwt_expiry(token: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let payload = token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value = serde_json::from_slice::<serde_json::Value>(&decoded).ok()?;
    let exp = value.get("exp")?.as_i64()?;
    chrono::DateTime::<chrono::Utc>::from_timestamp(exp, 0)
}

pub(crate) async fn complete_openrouter_browser_login() -> Result<String> {
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local OpenRouter callback server")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .context("failed to inspect OpenRouter callback listener")?
            .port()
    );

    let mut authorization_url =
        Url::parse("https://openrouter.ai/auth").context("failed to parse OpenRouter auth URL")?;
    {
        let mut query = authorization_url.query_pairs_mut();
        query.append_pair("callback_url", &redirect_uri);
        query.append_pair("code_challenge", &challenge);
        query.append_pair("code_challenge_method", "S256");
    }

    let callback_task = tokio::spawn(wait_for_code_callback(listener));
    match webbrowser::open(authorization_url.as_str()) {
        Ok(_) => println!("Opened browser for OpenRouter login."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!(
        "If needed, open this URL manually:\n{}\n",
        authorization_url.as_str()
    );

    let callback = timeout(OAUTH_TIMEOUT, callback_task)
        .await
        .context("timed out waiting for OpenRouter callback")?
        .context("OpenRouter callback task failed")??;

    let response = client
        .post("https://openrouter.ai/api/v1/auth/keys")
        .json(&serde_json::json!({
            "code": callback.code,
            "code_verifier": verifier,
            "code_challenge_method": "S256"
        }))
        .send()
        .await
        .context("failed to exchange OpenRouter browser code for an API key")?;
    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .await
        .context("failed to parse OpenRouter browser login response")?;
    if !status.is_success() {
        bail!(
            "OpenRouter browser login failed: {}",
            body.get("error")
                .and_then(serde_json::Value::as_str)
                .or_else(|| body.get("message").and_then(serde_json::Value::as_str))
                .unwrap_or("unknown error")
        );
    }

    body.get("key")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("OpenRouter browser login response did not contain an API key"))
}

pub(crate) async fn capture_browser_api_key(
    kind: HostedKindArg,
    provider_name: &str,
) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local browser capture server")?;
    let helper_url = format!(
        "http://127.0.0.1:{}/",
        listener
            .local_addr()
            .context("failed to inspect browser capture listener")?
            .port()
    );

    let capture_task = tokio::spawn(wait_for_browser_api_key_submission(
        listener,
        kind,
        provider_name.to_string(),
    ));
    match webbrowser::open(&helper_url) {
        Ok(_) => println!("Opened browser helper for {} login.", provider_name),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{helper_url}\n");

    timeout(OAUTH_TIMEOUT, capture_task)
        .await
        .context("timed out waiting for browser credential submission")?
        .context("browser credential capture task failed")?
}

pub(crate) async fn wait_for_browser_api_key_submission(
    listener: TcpListener,
    kind: HostedKindArg,
    provider_name: String,
) -> Result<String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .context("failed to accept browser credential connection")?;
        let request = read_local_http_request(&mut stream).await?;
        let Some(first_line) = request.lines().next() else {
            write_html_response(
                &mut stream,
                "400 Bad Request",
                "<html><body><h1>Bad request</h1></body></html>",
            )
            .await?;
            continue;
        };
        let mut parts = first_line.split_whitespace();
        let method = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or("/");

        match (method, target) {
            ("GET", "/") => {
                let html = browser_capture_page(kind, &provider_name);
                write_html_response(&mut stream, "200 OK", &html).await?;
            }
            ("GET", "/favicon.ico") => {
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .context("failed to write favicon response")?;
            }
            ("POST", "/submit") => {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                let fields = url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect::<Vec<_>>();
                let credential = fields
                    .iter()
                    .find(|(key, _)| key == "credential")
                    .map(|(_, value)| value.trim().to_string())
                    .unwrap_or_default();
                if credential.is_empty() {
                    write_html_response(
                        &mut stream,
                        "400 Bad Request",
                        "<html><body><h1>Missing credential</h1><p>Return to the previous tab and paste the credential before submitting.</p></body></html>",
                    )
                    .await?;
                    continue;
                }

                write_html_response(
                    &mut stream,
                    "200 OK",
                    "<html><body><h1>Credential captured</h1><p>You can close this tab and return to the terminal.</p></body></html>",
                )
                .await?;
                return Ok(credential);
            }
            _ => {
                write_html_response(
                    &mut stream,
                    "404 Not Found",
                    "<html><body><h1>Not Found</h1></body></html>",
                )
                .await?;
            }
        }
    }
}

pub(crate) fn browser_capture_page(kind: HostedKindArg, provider_name: &str) -> String {
    let title = escape_html(provider_name);
    let portal_url = escape_html(hosted_kind_browser_portal_url(kind));
    let instructions = escape_html(hosted_kind_browser_instructions(kind));
    format!(
        "<html><body style=\"font-family: sans-serif; max-width: 760px; margin: 40px auto; line-height: 1.5;\">\
         <h1>{title} browser setup</h1>\
         <p>{instructions}</p>\
         <p><a href=\"{portal_url}\" target=\"_blank\" rel=\"noreferrer\">Open {title}</a></p>\
         <form method=\"POST\" action=\"/submit\">\
         <label for=\"credential\"><strong>Paste credential</strong></label><br/>\
         <input id=\"credential\" name=\"credential\" type=\"password\" style=\"width: 100%; padding: 10px; margin: 12px 0;\" autofocus />\
         <button type=\"submit\" style=\"padding: 10px 18px;\">Save credential</button>\
         </form>\
         <p>This sends the credential only to the local CLI callback on this machine.</p>\
         </body></html>"
    )
}

pub(crate) fn hosted_kind_browser_portal_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => "https://platform.openai.com/",
        HostedKindArg::Anthropic => "https://console.anthropic.com/",
        HostedKindArg::Moonshot => "https://platform.moonshot.ai/",
        HostedKindArg::Openrouter => "https://openrouter.ai/",
        HostedKindArg::Venice => "https://venice.ai/",
    }
}

pub(crate) fn hosted_kind_browser_instructions(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => {
            "Sign in to the OpenAI platform in another tab, generate or copy an API key, then paste it below."
        }
        HostedKindArg::Anthropic => {
            "Sign in to Anthropic Console in another tab, create or copy an API key, then paste it below."
        }
        HostedKindArg::Moonshot => {
            "Sign in to the Moonshot platform in another tab, create or copy an API key, then paste it below."
        }
        HostedKindArg::Openrouter => {
            "OpenRouter browser login is automatic and should not use the manual browser helper."
        }
        HostedKindArg::Venice => {
            "Sign in to Venice in another tab, create or copy an API key, then paste it below."
        }
    }
}

pub(crate) async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
    let mut buffer = vec![0_u8; 16_384];
    let bytes_read = timeout(OAUTH_TIMEOUT, stream.read(&mut buffer))
        .await
        .context("timed out reading local browser callback")?
        .context("failed to read local browser callback")?;
    Ok(String::from_utf8_lossy(&buffer[..bytes_read]).to_string())
}

pub(crate) async fn write_html_response(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    body: &str,
) -> Result<()> {
    let body_len = body.len();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body_len,
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser response")
}

pub(crate) fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

pub(crate) fn parse_callback_request_url(request: &str, label: &str) -> Result<Url> {
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

pub(crate) async fn write_redirect_response(
    stream: &mut tokio::net::TcpStream,
    location: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write local browser redirect")
}

pub(crate) fn is_missing_codex_entitlement_error(
    error_code: &str,
    error_description: Option<&str>,
) -> bool {
    error_code == "access_denied"
        && error_description.is_some_and(|description| {
            description
                .to_ascii_lowercase()
                .contains("missing_codex_entitlement")
        })
}

pub(crate) fn oauth_callback_error_message(
    error_code: &str,
    error_description: Option<&str>,
) -> String {
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

pub(crate) async fn complete_oauth_login(provider: &ProviderConfig) -> Result<OAuthToken> {
    let client = build_http_client();
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state = Uuid::new_v4().to_string();
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local OAuth callback server")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .context("failed to inspect OAuth callback listener")?
            .port()
    );
    let authorization_url =
        build_oauth_authorization_url(provider, &redirect_uri, &state, &challenge)?;

    let callback_task = tokio::spawn(wait_for_oauth_callback(listener));
    match webbrowser::open(&authorization_url) {
        Ok(_) => println!("Opened browser for OAuth login."),
        Err(error) => println!("Could not open browser automatically: {error}"),
    }
    println!("If needed, open this URL manually:\n{authorization_url}\n");

    let callback = timeout(OAUTH_TIMEOUT, callback_task)
        .await
        .context("timed out waiting for OAuth callback")?
        .context("OAuth callback task failed")??;
    if callback.state != state {
        bail!("OAuth callback state did not match expected login state");
    }

    exchange_oauth_code(&client, provider, &callback.code, &verifier, &redirect_uri).await
}

pub(crate) async fn wait_for_code_callback(listener: TcpListener) -> Result<BrowserCodeCallback> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept browser code callback connection")?;
    let request = read_local_http_request(&mut stream).await?;
    let url = parse_callback_request_url(&request, "browser callback")?;

    let mut code = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }

    let body = if let Some(error) = error.clone() {
        format!(
            "<html><body><h1>Browser login failed</h1><p>{}</p></body></html>",
            html_escape(&error)
        )
    } else {
        "<html><body><h1>Login complete</h1><p>You can return to the terminal.</p></body></html>"
            .to_string()
    };
    let status = if error.is_some() {
        "400 Bad Request"
    } else {
        "200 OK"
    };
    write_html_response(&mut stream, status, &body).await?;

    if let Some(error) = error {
        bail!("browser login failed: {error}");
    }

    Ok(BrowserCodeCallback {
        code: code.ok_or_else(|| anyhow!("browser callback missing authorization code"))?,
    })
}

pub(crate) async fn wait_for_oauth_callback(listener: TcpListener) -> Result<OAuthCallback> {
    let (mut stream, _) = listener
        .accept()
        .await
        .context("failed to accept OAuth callback connection")?;
    let mut buffer = vec![0_u8; 8192];
    let bytes_read = timeout(OAUTH_TIMEOUT, stream.read(&mut buffer))
        .await
        .context("timed out reading OAuth callback")?
        .context("failed to read OAuth callback request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let url = parse_callback_request_url(&request, "OAuth callback")?;

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    let (status_line, body) = if let Some(error) = error.clone() {
        let message = oauth_callback_error_message(&error, error_description.as_deref());
        (
            "HTTP/1.1 400 Bad Request",
            format!(
                "<html><body><h1>OAuth login failed</h1><p>{}</p></body></html>",
                html_escape(&message)
            ),
        )
    } else {
        (
            "HTTP/1.1 200 OK",
            "<html><body><h1>Login complete</h1><p>You can return to the terminal.</p></body></html>"
                .to_string(),
        )
    };
    let response = format!(
        "{status_line}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .context("failed to write OAuth callback response")?;

    if let Some(error) = error {
        bail!(
            "{}",
            oauth_callback_error_message(&error, error_description.as_deref())
        );
    }

    Ok(OAuthCallback {
        code: code.ok_or_else(|| anyhow!("OAuth callback missing authorization code"))?,
        state: state.ok_or_else(|| anyhow!("OAuth callback missing state"))?,
    })
}

pub(crate) fn generate_code_verifier() -> String {
    let mut verifier = String::new();
    while verifier.len() < 64 {
        verifier.push_str(&Uuid::new_v4().simple().to_string());
    }
    verifier.truncate(96);
    verifier
}

pub(crate) fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub(crate) fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn prompt_or_value(
    theme: &ColorfulTheme,
    prompt: &str,
    current: Option<String>,
    initial_text: Option<String>,
) -> Result<String> {
    if let Some(current) = current {
        return Ok(current);
    }

    let mut input = Input::with_theme(theme).with_prompt(prompt);
    if let Some(initial_text) = initial_text {
        input = input.with_initial_text(initial_text);
    }
    Ok(input.interact_text()?)
}

pub(crate) fn select_hosted_kind(theme: &ColorfulTheme) -> Result<HostedKindArg> {
    Ok(
        match Select::with_theme(theme)
            .with_prompt("Provider type")
            .items(["OpenAI", "Anthropic", "Moonshot", "OpenRouter", "Venice AI"])
            .default(0)
            .interact()?
        {
            0 => HostedKindArg::OpenaiCompatible,
            1 => HostedKindArg::Anthropic,
            2 => HostedKindArg::Moonshot,
            3 => HostedKindArg::Openrouter,
            _ => HostedKindArg::Venice,
        },
    )
}

pub(crate) fn select_auth_method(
    theme: &ColorfulTheme,
    kind: HostedKindArg,
) -> Result<AuthMethodArg> {
    if kind == HostedKindArg::Anthropic {
        println!("Anthropic third-party access uses API keys only.");
        return Ok(AuthMethodArg::ApiKey);
    }

    let browser_label = if hosted_kind_supports_automatic_browser_capture(kind) {
        match kind {
            HostedKindArg::OpenaiCompatible => {
                "Browser sign-in (use your OpenAI account, Recommended)"
            }
            HostedKindArg::Anthropic => unreachable!("Anthropic is API-key-only"),
            HostedKindArg::Openrouter => "Browser sign-in (automatic capture, Recommended)",
            HostedKindArg::Moonshot | HostedKindArg::Venice => {
                unreachable!("non-native browser login provider was routed incorrectly")
            }
        }
    } else {
        "Browser portal (open provider site, then paste credential)"
    };

    Ok(
        match Select::with_theme(theme)
            .with_prompt("Authentication method")
            .items([browser_label, "OAuth (advanced custom flow)", "API key"])
            .default(0)
            .interact()?
        {
            0 => AuthMethodArg::Browser,
            1 => AuthMethodArg::Oauth,
            _ => AuthMethodArg::ApiKey,
        },
    )
}

pub(crate) fn hosted_kind_to_provider_kind(kind: HostedKindArg) -> ProviderKind {
    match kind {
        HostedKindArg::OpenaiCompatible
        | HostedKindArg::Moonshot
        | HostedKindArg::Openrouter
        | HostedKindArg::Venice => ProviderKind::OpenAiCompatible,
        HostedKindArg::Anthropic => ProviderKind::Anthropic,
    }
}

pub(crate) fn browser_hosted_kind_to_provider_kind(kind: HostedKindArg) -> ProviderKind {
    match kind {
        HostedKindArg::OpenaiCompatible => ProviderKind::ChatGptCodex,
        _ => hosted_kind_to_provider_kind(kind),
    }
}

pub(crate) fn hosted_kind_to_provider_profile(kind: HostedKindArg) -> ProviderProfile {
    match kind {
        HostedKindArg::OpenaiCompatible => ProviderProfile::OpenAi,
        HostedKindArg::Anthropic => ProviderProfile::Anthropic,
        HostedKindArg::Moonshot => ProviderProfile::Moonshot,
        HostedKindArg::Openrouter => ProviderProfile::OpenRouter,
        HostedKindArg::Venice => ProviderProfile::Venice,
    }
}

pub(crate) fn default_hosted_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => DEFAULT_OPENAI_URL,
        HostedKindArg::Anthropic => DEFAULT_ANTHROPIC_URL,
        HostedKindArg::Moonshot => DEFAULT_MOONSHOT_URL,
        HostedKindArg::Openrouter => DEFAULT_OPENROUTER_URL,
        HostedKindArg::Venice => DEFAULT_VENICE_URL,
    }
}

pub(crate) fn default_browser_hosted_url(kind: HostedKindArg) -> &'static str {
    match kind {
        HostedKindArg::OpenaiCompatible => DEFAULT_CHATGPT_CODEX_URL,
        _ => default_hosted_url(kind),
    }
}

pub(crate) fn hosted_kind_supports_automatic_browser_capture(kind: HostedKindArg) -> bool {
    matches!(
        kind,
        HostedKindArg::Openrouter | HostedKindArg::OpenaiCompatible
    )
}

pub(crate) fn prompt_visible_api_key(theme: &ColorfulTheme, prompt: &str) -> Result<String> {
    let value = Input::<String>::with_theme(theme)
        .with_prompt(prompt)
        .interact_text()?;
    if value.trim().is_empty() {
        bail!("{prompt} cannot be empty");
    }
    Ok(value)
}

pub(crate) fn collect_scopes(theme: &ColorfulTheme, scopes: Vec<String>) -> Result<Vec<String>> {
    if !scopes.is_empty() {
        return Ok(scopes);
    }
    let input: String = Input::with_theme(theme)
        .with_prompt("Scopes (space or comma separated, optional)")
        .allow_empty(true)
        .interact_text()?;
    Ok(split_scopes(&input))
}

pub(crate) fn split_scopes(input: &str) -> Vec<String> {
    input
        .replace(',', " ")
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn collect_key_value_params(
    theme: &ColorfulTheme,
    prompt: &str,
    params: Vec<String>,
) -> Result<Vec<KeyValuePair>> {
    if !params.is_empty() {
        let pairs = params
            .into_iter()
            .map(parse_key_value_pair)
            .collect::<Result<Vec<_>>>()?;
        reject_plaintext_oauth_secrets(&pairs)?;
        return Ok(pairs);
    }
    let input: String = Input::with_theme(theme)
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()?;
    let pairs = parse_key_value_list(&input)?;
    reject_plaintext_oauth_secrets(&pairs)?;
    Ok(pairs)
}

pub(crate) fn parse_key_value_list(input: &str) -> Result<Vec<KeyValuePair>> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }
    input
        .split(',')
        .map(|entry| parse_key_value_pair(entry.trim().to_string()))
        .collect()
}

pub(crate) fn parse_key_value_pair(value: String) -> Result<KeyValuePair> {
    let (key, value) = value
        .split_once('=')
        .ok_or_else(|| anyhow!("expected key=value"))?;
    Ok(KeyValuePair {
        key: key.trim().to_string(),
        value: value.trim().to_string(),
    })
}

pub(crate) fn reject_plaintext_oauth_secrets(params: &[KeyValuePair]) -> Result<()> {
    let Some(secret_key) = params.iter().find_map(|param| {
        let key = param.key.trim().to_ascii_lowercase();
        ["secret", "password", "private_key", "api_key"]
            .iter()
            .any(|fragment| key.contains(fragment))
            .then_some(param.key.as_str())
    }) else {
        return Ok(());
    };
    bail!(
        "OAuth parameter '{}' looks secret and would be stored in plaintext config; browser/API-key flows are supported, but secret OAuth params are not",
        secret_key
    )
}
