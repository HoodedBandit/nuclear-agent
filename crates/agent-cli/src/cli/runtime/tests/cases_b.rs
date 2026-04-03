    #[tokio::test]
    async fn autonomy_evolve_and_autopilot_status_commands_hit_daemon() {
        let storage = temp_storage();
        let token = "test-status-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "GET",
                    "/v1/autonomy/status",
                    &AutonomyProfile::default(),
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("GET", "/v1/evolve/status", &EvolveConfig::default()),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "GET",
                    "/v1/autopilot/status",
                    &AutopilotConfig::default(),
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        autonomy_command(&storage, AutonomyCommands::Status)
            .await
            .unwrap();
        evolve_command(&storage, EvolveCommands::Status)
            .await
            .unwrap();
        autopilot_command(&storage, AutopilotCommands::Status)
            .await
            .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|req| req.path == "/v1/status")
                .count(),
            3
        );
        assert!(requests.iter().any(|req| req.path == "/v1/autonomy/status"));
        assert!(requests.iter().any(|req| req.path == "/v1/evolve/status"));
        assert!(requests
            .iter()
            .any(|req| req.path == "/v1/autopilot/status"));
    }

    #[tokio::test]
    async fn connector_command_paths_hit_daemon_routes() {
        let storage = temp_storage();
        let token = "test-connector-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/telegram/ops/poll",
                    &TelegramPollResponse {
                        connector_id: "ops".to_string(),
                        processed_updates: 1,
                        queued_missions: 0,
                        pending_approvals: 0,
                        last_update_id: Some(99),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/discord/ops/send",
                    &DiscordSendResponse {
                        connector_id: "ops".to_string(),
                        channel_id: "123".to_string(),
                        message_id: Some("m-1".to_string()),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/slack/ops/send",
                    &SlackSendResponse {
                        connector_id: "ops".to_string(),
                        channel_id: "C123".to_string(),
                        message_ts: Some("123.45".to_string()),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/signal/ops/send",
                    &SignalSendResponse {
                        connector_id: "ops".to_string(),
                        target: "group:team".to_string(),
                    },
                ),
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/home-assistant/ops/services",
                    &HomeAssistantServiceCallResponse {
                        connector_id: "ops".to_string(),
                        domain: "light".to_string(),
                        service: "turn_on".to_string(),
                        changed_entities: 1,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        telegram_command(
            &storage,
            TelegramCommands::Poll {
                id: "ops".to_string(),
            },
        )
        .await
        .unwrap();
        discord_command(
            &storage,
            DiscordCommands::Send(DiscordSendArgs {
                id: "ops".to_string(),
                channel_id: "123".to_string(),
                content: "deploy now".to_string(),
            }),
        )
        .await
        .unwrap();
        slack_command(
            &storage,
            SlackCommands::Send(SlackSendArgs {
                id: "ops".to_string(),
                channel_id: "C123".to_string(),
                text: "ship it".to_string(),
            }),
        )
        .await
        .unwrap();
        signal_command(
            &storage,
            SignalCommands::Send(SignalSendArgs {
                id: "ops".to_string(),
                recipient: None,
                group_id: Some("team".to_string()),
                text: "hello".to_string(),
            }),
        )
        .await
        .unwrap();
        home_assistant_command(
            &storage,
            HomeAssistantCommands::CallService(HomeAssistantServiceArgs {
                id: "ops".to_string(),
                domain: "light".to_string(),
                service: "turn_on".to_string(),
                entity_id: Some("light.office".to_string()),
                service_data_json: Some("{\"brightness\":200}".to_string()),
            }),
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let discord_body: DiscordSendRequest = serde_json::from_str(&requests[3].body).unwrap();
        assert_eq!(discord_body.channel_id, "123");
        let slack_body: SlackSendRequest = serde_json::from_str(&requests[5].body).unwrap();
        assert_eq!(slack_body.text, "ship it");
        let signal_body: SignalSendRequest = serde_json::from_str(&requests[7].body).unwrap();
        assert_eq!(signal_body.group_id.as_deref(), Some("team"));
        let ha_body: HomeAssistantServiceCallRequest =
            serde_json::from_str(&requests[9].body).unwrap();
        assert_eq!(ha_body.domain, "light");
        assert_eq!(
            ha_body
                .service_data
                .as_ref()
                .and_then(|value| value.get("brightness"))
                .and_then(serde_json::Value::as_i64),
            Some(200)
        );
    }

    #[tokio::test]
    async fn webhook_and_inbox_add_commands_persist_locally_without_daemon() {
        let storage = temp_storage();
        let inbox_path = std::env::temp_dir().join(format!("nuclear-inbox-{}", Uuid::new_v4()));
        fs::create_dir_all(&inbox_path).unwrap();

        webhook_command(
            &storage,
            WebhookCommands::Add(WebhookAddArgs {
                id: "webhook".to_string(),
                name: "Webhook".to_string(),
                description: "Inbound webhook".to_string(),
                prompt_template: Some("Handle {{summary}}".to_string()),
                prompt_file: None,
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                cwd: None,
                token: Some("hook-token".to_string()),
                enabled: true,
            }),
        )
        .await
        .unwrap();

        inbox_command(
            &storage,
            InboxCommands::Add(InboxAddArgs {
                id: "inbox".to_string(),
                name: "Inbox".to_string(),
                description: "Watch a folder".to_string(),
                path: inbox_path.clone(),
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                cwd: None,
                delete_after_read: true,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.webhook_connectors.len(), 1);
        assert_eq!(config.inbox_connectors.len(), 1);
        assert_eq!(config.inbox_connectors[0].path, inbox_path);
        assert!(config.webhook_connectors[0].token_sha256.is_some());
    }

    #[tokio::test]
    async fn skills_enable_and_disable_update_local_config() {
        let storage = temp_storage();
        let home_dir = std::env::temp_dir().join(format!("nuclear-home-{}", Uuid::new_v4()));
        let skill_dir = home_dir.join(".codex").join("skills").join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Test Skill\nA skill used for tests.\n",
        )
        .unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home_dir);

        skills_command(
            &storage,
            SkillCommands::Enable {
                name: "test-skill".to_string(),
            },
        )
        .await
        .unwrap();

        let enabled = storage.load_config().unwrap().enabled_skills;
        assert_eq!(enabled, vec!["test-skill".to_string()]);

        skills_command(
            &storage,
            SkillCommands::Disable {
                name: "test-skill".to_string(),
            },
        )
        .await
        .unwrap();

        let enabled = storage.load_config().unwrap().enabled_skills;
        assert!(enabled.is_empty());

        if let Some(previous) = previous_home {
            std::env::set_var("HOME", previous);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[tokio::test]
    async fn session_command_rename_updates_stored_session() {
        let storage = temp_storage();
        let alias = sample_alias("main", "local", "qwen");
        storage
            .ensure_session("session-1", &alias, "local", "qwen", None)
            .unwrap();

        session_command(
            &storage,
            SessionCommands::Rename {
                id: "session-1".to_string(),
                title: "Renamed".to_string(),
            },
        )
        .await
        .unwrap();

        let session = storage.get_session("session-1").unwrap().unwrap();
        assert_eq!(session.title.as_deref(), Some("Renamed"));
    }

    #[tokio::test]
    async fn run_command_posts_request_when_onboarded() {
        let storage = temp_storage();
        let token = "test-run-token";
        let provider = sample_local_provider("http://127.0.0.1:11434", "local", "qwen");
        let alias = sample_alias("main", "local", "qwen");
        storage
            .save_config(&local_onboarded_config(provider, alias))
            .unwrap();

        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/run",
                    &RunTaskResponse {
                        session_id: "session-1".to_string(),
                        alias: "main".to_string(),
                        provider_id: "local".to_string(),
                        model: "qwen".to_string(),
                        response: "done".to_string(),
                        tool_events: Vec::new(),
                        structured_output_json: None,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        run_command(
            &storage,
            RunArgs {
                prompt: Some("hello".to_string()),
                alias: Some("main".to_string()),
                tasks: Vec::new(),
                thinking: None,
                mode: None,
                images: Vec::new(),
                output_schema: None,
                output_last_message: None,
                json: false,
                ephemeral: false,
                permissions: None,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: RunTaskRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.prompt, "hello");
        assert_eq!(body.alias.as_deref(), Some("main"));
        assert_eq!(body.requested_model, None);
    }

    #[test]
    fn drain_ndjson_buffer_handles_split_utf8_boundaries() {
        let mut buffer = Vec::new();
        let mut values = Vec::new();
        let first = b"{\"value\":\"snowman ".to_vec();
        let second = vec![0xE2, 0x98];
        let mut third = vec![0x83];
        third.extend_from_slice(b"\"}\n");

        buffer.extend_from_slice(&first);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, false, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();
        assert!(values.is_empty());

        buffer.extend_from_slice(&second);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, false, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();
        assert!(values.is_empty());

        buffer.extend_from_slice(&third);
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, true, &mut |event| {
            values.push(event);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            values,
            vec![StreamFixture {
                value: "snowman \u{2603}".to_string()
            }]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_ndjson_buffer_rejects_invalid_trailing_utf8() {
        let mut buffer = vec![0xE2, 0x98];
        let error = drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, true, &mut |_| Ok(()))
            .unwrap_err();
        assert!(error.to_string().contains("utf-8"));
    }

    #[test]
    fn parse_task_requires_alias_separator() {
        let task = parse_task("coder=write code".to_string()).unwrap();
        assert_eq!(task.target.as_deref(), Some("coder"));
        assert!(task.alias.is_none());
        assert!(parse_task("missing".to_string()).is_err());
    }

    #[test]
    fn parse_key_value_list_handles_empty_and_multiple_pairs() {
        assert!(parse_key_value_list("").unwrap().is_empty());
        let parsed = parse_key_value_list("a=1,b=2").unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "a");
        assert_eq!(parsed[1].value, "2");
    }

    #[test]
    fn pkce_challenge_is_url_safe() {
        let challenge = pkce_challenge("abc123");
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
    }

    #[test]
    fn apply_trust_update_preserves_unspecified_values() {
        let mut policy = TrustPolicy {
            allow_shell: false,
            allow_network: false,
            ..TrustPolicy::default()
        };
        apply_trust_update(
            &mut policy,
            &TrustUpdateRequest {
                trusted_path: None,
                allow_shell: None,
                allow_network: Some(true),
                allow_full_disk: None,
                allow_self_edit: None,
            },
        );
        assert!(!policy.allow_shell);
        assert!(policy.allow_network);
    }

    #[test]
    fn first_provider_defaults_to_main_alias_only_once() {
        let config = AppConfig::default();
        assert_eq!(default_main_alias(&config, None), Some("main".to_string()));

        let configured = AppConfig {
            main_agent_alias: Some("claude".to_string()),
            ..AppConfig::default()
        };
        assert_eq!(default_main_alias(&configured, None), None);
        assert_eq!(
            default_main_alias(&configured, Some("writer".to_string())),
            Some("writer".to_string())
        );
    }

    #[test]
    fn next_available_provider_id_appends_suffix_when_needed() {
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("gpt-4.1".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "openai-2".to_string(),
                    display_name: "OpenAI 2".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("gpt-4.1-mini".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
            ],
            ..AppConfig::default()
        };

        assert_eq!(next_available_provider_id(&config, "openai"), "openai-3");
        assert_eq!(
            next_available_provider_id(&config, "anthropic"),
            "anthropic"
        );
    }

    #[test]
    fn default_alias_name_uses_main_then_model_slug() {
        let empty = AppConfig::default();
        let provider = ProviderConfig {
            id: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENROUTER_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: Some("openai/gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        };
        assert_eq!(
            default_alias_name(&empty, &provider, "openai/gpt-4.1"),
            "main"
        );

        let configured = AppConfig {
            main_agent_alias: Some("main".to_string()),
            aliases: vec![
                ModelAlias {
                    alias: "main".to_string(),
                    provider_id: "openai".to_string(),
                    model: "gpt-4.1".to_string(),
                    description: None,
                },
                ModelAlias {
                    alias: "openrouter-openai-gpt".to_string(),
                    provider_id: "openrouter".to_string(),
                    model: "openai/gpt-4.1".to_string(),
                    description: None,
                },
            ],
            ..AppConfig::default()
        };
        assert_eq!(
            default_alias_name(&configured, &provider, "openai/gpt-4.1"),
            "openrouter-openai-gpt-4"
        );
    }

    #[test]
    fn moonshot_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Moonshot),
            DEFAULT_MOONSHOT_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Moonshot),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn openrouter_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Openrouter),
            DEFAULT_OPENROUTER_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Openrouter),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn venice_uses_its_own_default_url() {
        assert_eq!(
            default_hosted_url(HostedKindArg::Venice),
            DEFAULT_VENICE_URL
        );
        assert_eq!(
            hosted_kind_to_provider_kind(HostedKindArg::Venice),
            ProviderKind::OpenAiCompatible
        );
    }

    #[test]
    fn openai_browser_login_uses_chatgpt_codex_provider_defaults() {
        assert_eq!(
            browser_hosted_kind_to_provider_kind(HostedKindArg::OpenaiCompatible),
            ProviderKind::ChatGptCodex
        );
        assert_eq!(
            default_browser_hosted_url(HostedKindArg::OpenaiCompatible),
            DEFAULT_CHATGPT_CODEX_URL
        );
        assert_eq!(
            browser_hosted_kind_to_provider_kind(HostedKindArg::Anthropic),
            ProviderKind::Anthropic
        );
    }

    #[test]
    fn automatic_browser_capture_is_native_for_openai_anthropic_and_openrouter() {
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::OpenaiCompatible
        ));
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Anthropic
        ));
        assert!(!hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Moonshot
        ));
        assert!(hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Openrouter
        ));
        assert!(!hosted_kind_supports_automatic_browser_capture(
            HostedKindArg::Venice
        ));
    }

    #[test]
    fn openai_browser_oauth_config_requests_org_enriched_claims() {
        let config = openai_browser_oauth_config();
        assert!(config.scopes.iter().any(|scope| scope == "openid"));
        assert!(config.scopes.iter().any(|scope| scope == "offline_access"));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| { param.key == "id_token_add_organizations" && param.value == "true" }));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| { param.key == "codex_cli_simplified_flow" && param.value == "true" }));
    }

    #[test]
    fn openai_browser_authorization_url_uses_loopback_contract() {
        let provider = ProviderConfig {
            id: "openai-browser".to_string(),
            display_name: "OpenAI Browser Session".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(openai_browser_oauth_config()),
            local: false,
        };
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
        let query = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(parsed.host_str(), Some("auth.openai.com"));
        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(redirect_uri.as_str())
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile email offline_access api.connectors.read api.connectors.invoke")
        );
        assert_eq!(
            query.get("originator").map(String::as_str),
            Some(OPENAI_BROWSER_ORIGINATOR)
        );
    }

    #[test]
    fn claude_browser_oauth_config_matches_packaged_claude_constants() {
        let config = claude_browser_oauth_config();
        assert_eq!(config.client_id, CLAUDE_BROWSER_CLIENT_ID);
        assert_eq!(config.authorization_url, CLAUDE_BROWSER_AUTHORIZE_URL);
        assert_eq!(config.token_url, CLAUDE_BROWSER_TOKEN_URL);
        assert!(config
            .scopes
            .iter()
            .any(|scope| scope == "org:create_api_key"));
        assert!(config.scopes.iter().any(|scope| scope == "user:inference"));
        assert!(config
            .scopes
            .iter()
            .any(|scope| scope == "user:sessions:claude_code"));
        assert!(config
            .extra_authorize_params
            .iter()
            .any(|param| param.key == "code" && param.value == "true"));
    }

    #[test]
    fn claude_browser_authorization_url_uses_loopback_contract() {
        let provider = ProviderConfig {
            id: "claude-browser".to_string(),
            display_name: "Claude Browser Session".to_string(),
            kind: ProviderKind::Anthropic,
            base_url: DEFAULT_ANTHROPIC_URL.to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: None,
            keychain_account: None,
            oauth: Some(claude_browser_oauth_config()),
            local: false,
        };
        let redirect_uri = format!(
            "http://localhost:{CLAUDE_BROWSER_CALLBACK_PORT}{CLAUDE_BROWSER_CALLBACK_PATH}"
        );
        let authorization_url = build_oauth_authorization_url(
            &provider,
            &redirect_uri,
            "state-456",
            &pkce_challenge("verifier-456"),
        )
        .expect("authorization URL should build");
        let parsed = Url::parse(&authorization_url).expect("authorization URL should parse");
        let query = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(parsed.host_str(), Some("claude.ai"));
        assert_eq!(parsed.path(), "/oauth/authorize");
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(redirect_uri.as_str())
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers")
        );
        assert_eq!(query.get("code").map(String::as_str), Some("true"));
    }

    #[test]
    fn claude_scope_error_triggers_oauth_fallback() {
        assert!(should_fallback_to_claude_browser_oauth(
            "Claude browser API key mint failed: OAuth token does not meet scope requirement org:create_api_key"
        ));
        assert!(!should_fallback_to_claude_browser_oauth(
            "Claude browser API key mint failed: service unavailable"
        ));
    }

    #[test]
    fn claude_settings_parser_reads_existing_browser_credentials() {
        let parsed = parse_claude_browser_credentials_from_settings(
            r#"{
                "primaryApiKey": " sk-ant-managed ",
                "oauthAccount": {
                    "emailAddress": "user@example.com",
                    "organizationUuid": "org_123",
                    "organizationName": "Acme"
                }
            }"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(parsed.api_key, "sk-ant-managed");
        assert_eq!(parsed.email.as_deref(), Some("user@example.com"));
        assert_eq!(parsed.org_id.as_deref(), Some("org_123"));
        assert_eq!(parsed.org_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn oauth_callback_error_message_prefers_description() {
        assert_eq!(
            oauth_callback_error_message("access_denied", Some("unknown authentication error")),
            "Sign-in failed: unknown authentication error"
        );
    }

    #[test]
    fn oauth_callback_error_message_maps_missing_codex_entitlement() {
        assert_eq!(
            oauth_callback_error_message(
                "access_denied",
                Some("missing_codex_entitlement for this workspace")
            ),
            "OpenAI browser sign-in is not enabled for this workspace account yet."
        );
    }

    #[test]
    fn rejects_plaintext_oauth_secret_params() {
        let error = reject_plaintext_oauth_secrets(&[KeyValuePair {
            key: "client_secret".to_string(),
            value: "secret".to_string(),
        }])
        .unwrap_err();
        assert!(error.to_string().contains("plaintext config"));
    }

    #[test]
    fn jwt_expiry_reads_exp_claim() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(br#"{"exp":4102444800}"#);
        let token = format!("{header}.{payload}.sig");
        let expiry = jwt_expiry(&token).unwrap();
        assert_eq!(expiry.timestamp(), 4_102_444_800);
    }

    #[test]
    fn cli_uses_default_prompt_without_subcommand() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "write a summary"]);
        assert_eq!(cli.prompt.as_deref(), Some("write a summary"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn hosted_kind_accepts_openai_alias() {
        assert_eq!(
            HostedKindArg::from_str("openai", true).unwrap(),
            HostedKindArg::OpenaiCompatible
        );
        assert_eq!(
            HostedKindArg::from_str("openai-compatible", true).unwrap(),
            HostedKindArg::OpenaiCompatible
        );
    }

    #[test]
    fn cli_parses_exec_subcommand() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "exec",
            "--alias",
            "claude",
            "--thinking",
            "high",
            "fix the bug",
        ]);
        match cli.command {
            Some(Commands::Exec(args)) => {
                assert_eq!(args.alias.as_deref(), Some("claude"));
                assert_eq!(args.thinking, Some(ThinkingLevelArg::High));
                assert_eq!(args.prompt.as_deref(), Some("fix the bug"));
            }
            _ => panic!("expected exec command"),
        }
    }

