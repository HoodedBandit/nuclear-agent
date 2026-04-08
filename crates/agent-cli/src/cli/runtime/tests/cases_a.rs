    fn temp_storage() -> Storage {
        Storage::open_at(std::env::temp_dir().join(format!("nuclear-cli-test-{}", Uuid::new_v4())))
            .unwrap()
    }

    fn session_transcript_with_mode(storage: &Storage, task_mode: TaskMode) -> SessionTranscript {
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "local".to_string(),
            model: "qwen".to_string(),
            description: None,
        };
        storage
            .ensure_session("session-1", &alias, "local", "qwen", Some(task_mode))
            .unwrap();
        storage
            .append_message(&SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                "hello".to_string(),
                Some("local".to_string()),
                Some("qwen".to_string()),
            ))
            .unwrap();
        SessionTranscript {
            session: storage.get_session("session-1").unwrap().unwrap(),
            messages: storage.list_session_messages("session-1").unwrap(),
        }
    }

    #[test]
    fn load_session_task_mode_returns_persisted_mode() {
        let storage = temp_storage();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "local".to_string(),
            model: "qwen".to_string(),
            description: None,
        };
        storage
            .ensure_session("session-1", &alias, "local", "qwen", Some(TaskMode::Daily))
            .unwrap();

        assert_eq!(
            load_session_task_mode(&storage, Some("session-1")).unwrap(),
            Some(TaskMode::Daily)
        );
    }

    #[test]
    fn compact_session_preserves_task_mode() {
        let storage = temp_storage();
        let transcript = session_transcript_with_mode(&storage, TaskMode::Daily);

        let compacted_id = compact_session(&storage, &transcript, "Carry this forward").unwrap();
        let compacted = storage.get_session(&compacted_id).unwrap().unwrap();

        assert_eq!(compacted.task_mode, Some(TaskMode::Daily));
    }

    #[test]
    fn fork_session_preserves_task_mode() {
        let storage = temp_storage();
        let transcript = session_transcript_with_mode(&storage, TaskMode::Build);

        let forked_id = fork_session(&storage, &transcript).unwrap();
        let forked = storage.get_session(&forked_id).unwrap().unwrap();

        assert_eq!(forked.task_mode, Some(TaskMode::Build));
    }

    #[derive(Debug, Clone)]
    struct CapturedHttpRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: String,
    }

    #[derive(Debug, Clone)]
    struct MockHttpExpectation {
        method: &'static str,
        path: String,
        response_body: String,
        status_line: &'static str,
        content_type: &'static str,
    }

    impl MockHttpExpectation {
        fn json<T: Serialize>(method: &'static str, path: impl Into<String>, response: &T) -> Self {
            Self {
                method,
                path: path.into(),
                response_body: serde_json::to_string(response).unwrap(),
                status_line: "200 OK",
                content_type: "application/json",
            }
        }
    }

    struct MockHttpServer {
        origin: String,
        requests: Arc<Mutex<Vec<CapturedHttpRequest>>>,
        handle: JoinHandle<Result<()>>,
    }

    impl MockHttpServer {
        async fn finish(self) -> Result<Vec<CapturedHttpRequest>> {
            self.handle.await??;
            Ok(self.requests.lock().unwrap().clone())
        }
    }

    async fn spawn_mock_http_server(
        expectations: Vec<MockHttpExpectation>,
        expected_auth: Option<String>,
    ) -> MockHttpServer {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_clone = Arc::clone(&requests);
        let expected_auth_clone = expected_auth.clone();
        let mut queue = VecDeque::from(expectations);
        let handle = tokio::spawn(async move {
            while let Some(expected) = queue.pop_front() {
                let (mut stream, _) = listener.accept().await?;
                let raw = read_local_http_request(&mut stream).await?;
                let captured = parse_http_request(&raw);
                assert_eq!(captured.method, expected.method);
                assert_eq!(captured.path, expected.path);
                if let Some(expected_auth) = expected_auth_clone.as_deref() {
                    assert_eq!(
                        captured.headers.get("authorization").map(String::as_str),
                        Some(expected_auth)
                    );
                }
                requests_clone.lock().unwrap().push(captured);
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    expected.status_line,
                    expected.content_type,
                    expected.response_body.len(),
                    expected.response_body
                );
                stream.write_all(response.as_bytes()).await?;
            }
            Ok(())
        });

        MockHttpServer {
            origin: format!("http://{addr}"),
            requests,
            handle,
        }
    }

    async fn read_local_http_request(stream: &mut tokio::net::TcpStream) -> Result<String> {
        let mut buffer = Vec::new();
        let mut header_end = None;
        let mut content_length = 0usize;
        loop {
            let mut chunk = [0u8; 1024];
            let bytes = stream.read(&mut chunk).await?;
            if bytes == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..bytes]);
            if header_end.is_none() {
                if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(index + 4);
                    let headers = String::from_utf8_lossy(&buffer[..index + 4]);
                    for line in headers.lines() {
                        if let Some((name, value)) = line.split_once(':') {
                            if name.eq_ignore_ascii_case("content-length") {
                                content_length = value.trim().parse::<usize>().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            if let Some(end) = header_end {
                if buffer.len() >= end + content_length {
                    break;
                }
            }
        }
        Ok(String::from_utf8(buffer)?)
    }

    fn parse_http_request(raw: &str) -> CapturedHttpRequest {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or_default();
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts.next().unwrap_or_default().to_string();
        let path = request_parts.next().unwrap_or_default().to_string();
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        CapturedHttpRequest {
            method,
            path,
            headers,
            body: body.to_string(),
        }
    }

    fn daemon_status_fixture() -> DaemonStatus {
        DaemonStatus {
            pid: 4242,
            started_at: chrono::Utc::now(),
            persistence_mode: PersistenceMode::OnDemand,
            auto_start: false,
            main_agent_alias: Some("main".to_string()),
            main_target: Some(MainTargetSummary {
                alias: "main".to_string(),
                provider_id: "local".to_string(),
                provider_display_name: "Local".to_string(),
                model: "qwen".to_string(),
            }),
            onboarding_complete: true,
            autonomy: AutonomyProfile::default(),
            evolve: EvolveConfig::default(),
            autopilot: AutopilotConfig::default(),
            delegation: DelegationConfig::default(),
            providers: 1,
            aliases: 1,
            plugins: 0,
            delegation_targets: 1,
            webhook_connectors: 0,
            inbox_connectors: 0,
            telegram_connectors: 0,
            discord_connectors: 0,
            slack_connectors: 0,
            home_assistant_connectors: 0,
            signal_connectors: 0,
            gmail_connectors: 0,
            brave_connectors: 0,
            pending_connector_approvals: 0,
            missions: 0,
            active_missions: 0,
            memories: 0,
            pending_memory_reviews: 0,
            skill_drafts: 0,
            published_skills: 0,
        }
    }

    fn save_daemon_config(storage: &Storage, origin: &str, token: &str) {
        let parsed = Url::parse(origin).unwrap();
        let mut config = storage.load_config().unwrap();
        config.daemon.host = parsed.host_str().unwrap().to_string();
        config.daemon.port = parsed.port().unwrap();
        config.daemon.token = token.to_string();
        storage.save_config(&config).unwrap();
    }

    fn sample_remote_provider(id: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: id.to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENAI_URL.to_string(),
            provider_profile: None,
            auth_mode: AuthMode::ApiKey,
            default_model: Some(model.to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        }
    }

    fn sample_local_provider(origin: &str, id: &str, model: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: id.to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: origin.to_string(),
            provider_profile: None,
            auth_mode: AuthMode::None,
            default_model: Some(model.to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        }
    }

    fn sample_alias(alias: &str, provider_id: &str, model: &str) -> ModelAlias {
        ModelAlias {
            alias: alias.to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            description: None,
        }
    }

    fn local_onboarded_config(provider: ProviderConfig, alias: ModelAlias) -> AppConfig {
        AppConfig {
            onboarding_complete: true,
            providers: vec![provider],
            aliases: vec![alias.clone()],
            main_agent_alias: Some(alias.alias),
            ..AppConfig::default()
        }
    }

    fn projected_plugin_config(alias: &str, adapter_id: &str, model: &str) -> AppConfig {
        let plugin = InstalledPluginConfig {
            id: "echo-toolkit".to_string(),
            manifest: PluginManifest {
                schema_version: PLUGIN_SCHEMA_VERSION,
                id: "echo-toolkit".to_string(),
                name: "Echo Toolkit".to_string(),
                version: "0.1.0".to_string(),
                description: "test plugin".to_string(),
                homepage: None,
                compatibility: PluginCompatibility::default(),
                permissions: PluginPermissions::default(),
                tools: Vec::new(),
                connectors: Vec::new(),
                provider_adapters: vec![PluginProviderAdapterManifest {
                    id: adapter_id.to_string(),
                    provider_kind: ProviderKind::OpenAiCompatible,
                    description: "projected provider".to_string(),
                    command: "plugin-host".to_string(),
                    args: Vec::new(),
                    cwd: None,
                    permissions: PluginPermissions::default(),
                    default_model: Some(model.to_string()),
                    timeout_seconds: None,
                }],
            },
            source_kind: PluginSourceKind::LocalPath,
            install_dir: std::env::temp_dir().join(format!("plugin-install-{}", Uuid::new_v4())),
            source_reference: String::new(),
            source_path: std::env::temp_dir().join(format!("plugin-source-{}", Uuid::new_v4())),
            integrity_sha256: "reviewed".to_string(),
            enabled: true,
            trusted: true,
            granted_permissions: PluginPermissions::default(),
            reviewed_integrity_sha256: "reviewed".to_string(),
            reviewed_at: Some(chrono::Utc::now()),
            pinned: false,
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let provider_id = plugin_provider_id(&plugin.id, adapter_id);
        AppConfig {
            onboarding_complete: true,
            plugins: vec![plugin],
            aliases: vec![sample_alias(alias, &provider_id, model)],
            main_agent_alias: Some(alias.to_string()),
            ..AppConfig::default()
        }
    }

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{name}-{}", Uuid::new_v4()));
        fs::write(&path, content).unwrap();
        path
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct StreamFixture {
        value: String,
    }

    #[tokio::test]
    async fn dashboard_command_requests_launch_when_not_opening_browser() {
        let storage = temp_storage();
        let token = "test-dashboard-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/dashboard/launch",
                    &DashboardLaunchResponse {
                        launch_path: "/auth/dashboard/launch/mock".to_string(),
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        dashboard_command(
            &storage,
            DashboardArgs {
                print_url: true,
                no_open: true,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].path, "/v1/dashboard/launch");
    }

    #[tokio::test]
    async fn provider_add_posts_provider_and_alias() {
        let storage = temp_storage();
        let token = "test-login-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/providers",
                    &sample_remote_provider("openai", "gpt-5"),
                ),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/aliases",
                    &sample_alias("main", "openai", "gpt-5"),
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        provider_command(
            &storage,
            ProviderCommands::Add(ProviderAddArgs {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                kind: HostedKindArg::OpenaiCompatible,
                base_url: Some(server.origin.clone()),
                model: "gpt-5".to_string(),
                api_key: Some("secret-api-key".to_string()),
                main_alias: Some("main".to_string()),
            }),
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let provider_body: ProviderUpsertRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(provider_body.provider.id, "openai");
        assert_eq!(
            provider_body.provider.default_model.as_deref(),
            Some("gpt-5")
        );
        assert_eq!(provider_body.api_key.as_deref(), Some("secret-api-key"));

        let alias_body: AliasUpsertRequest = serde_json::from_str(&requests[2].body).unwrap();
        assert_eq!(alias_body.alias.alias, "main");
        assert_eq!(alias_body.alias.provider_id, "openai");
        assert_eq!(alias_body.alias.model, "gpt-5");
        assert!(alias_body.set_as_main);
    }

    #[tokio::test]
    async fn model_command_lists_models_for_local_provider() {
        let storage = temp_storage();
        let server = spawn_mock_http_server(
            vec![MockHttpExpectation::json(
                "GET",
                "/models",
                &json!({"data":[{"id":"qwen-coder"},{"id":"qwen-reasoner"}]}),
            )],
            None,
        )
        .await;

        let config = local_onboarded_config(
            sample_local_provider(&server.origin, "local", "qwen-coder"),
            sample_alias("main", "local", "qwen-coder"),
        );
        storage.save_config(&config).unwrap();

        model_command(
            &storage,
            ModelCommands::List {
                provider: "local".to_string(),
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/models");
    }

    #[tokio::test]
    async fn mcp_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let schema = temp_file("mcp-schema.json", "{\"type\":\"object\"}");

        mcp_command(
            &storage,
            McpCommands::Add(McpAddArgs {
                id: "filesystem".to_string(),
                name: "Filesystem".to_string(),
                description: "fs tools".to_string(),
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                tool_name: "fs_server".to_string(),
                schema_file: schema,
                cwd: None,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        assert_eq!(config.mcp_servers[0].id, "filesystem");
    }

    #[tokio::test]
    async fn app_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let schema = temp_file("app-schema.json", "{\"type\":\"object\"}");

        app_command(
            &storage,
            AppCommands::Add(AppAddArgs {
                id: "github".to_string(),
                name: "GitHub".to_string(),
                description: "github tools".to_string(),
                command: "node".to_string(),
                args: vec!["github.js".to_string()],
                tool_name: "github_app".to_string(),
                schema_file: schema,
                cwd: None,
                enabled: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.app_connectors.len(), 1);
        assert_eq!(config.app_connectors[0].id, "github");
    }

    #[tokio::test]
    async fn alias_command_add_persists_locally_without_daemon() {
        let storage = temp_storage();
        let config = AppConfig {
            providers: vec![sample_local_provider(
                "http://localhost:11434",
                "local",
                "qwen",
            )],
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        alias_command(
            &storage,
            AliasCommands::Add(AliasAddArgs {
                alias: "main".to_string(),
                provider: "local".to_string(),
                model: "qwen".to_string(),
                description: Some("Primary".to_string()),
                main: true,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.main_agent_alias.as_deref(), Some("main"));
        assert_eq!(config.aliases.len(), 1);
        assert_eq!(config.aliases[0].description.as_deref(), Some("Primary"));
    }

    #[tokio::test]
    async fn alias_command_add_accepts_projected_plugin_provider_without_daemon() {
        let storage = temp_storage();
        let config = projected_plugin_config("main", "echo-provider", "echo-1");
        let provider_id = config.aliases[0].provider_id.clone();
        storage.save_config(&config).unwrap();

        alias_command(
            &storage,
            AliasCommands::Add(AliasAddArgs {
                alias: "assistant".to_string(),
                provider: provider_id,
                model: "echo-1".to_string(),
                description: Some("Plugin-backed".to_string()),
                main: false,
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.aliases.len(), 2);
        assert_eq!(config.aliases[1].alias, "assistant");
    }

    #[tokio::test]
    async fn trust_command_updates_locally_without_daemon() {
        let storage = temp_storage();

        trust_command(
            &storage,
            TrustArgs {
                path: Some(PathBuf::from("C:\\workspace")),
                allow_shell: Some(true),
                allow_network: Some(true),
                allow_full_disk: None,
                allow_self_edit: Some(false),
            },
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert!(config.trust_policy.allow_shell);
        assert!(config.trust_policy.allow_network);
        assert!(!config.trust_policy.allow_self_edit);
        assert!(config
            .trust_policy
            .trusted_paths
            .contains(&PathBuf::from("C:\\workspace")));
    }

    #[tokio::test]
    async fn permissions_command_updates_locally_without_daemon() {
        let storage = temp_storage();

        permissions_command(
            &storage,
            PermissionsArgs {
                preset: Some(PermissionPresetArg::FullAuto),
            },
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.permission_preset, PermissionPreset::FullAuto);
    }

    #[tokio::test]
    async fn daemon_config_command_updates_local_config_without_daemon() {
        let storage = temp_storage();

        daemon_command(
            &storage,
            DaemonCommands::Config(DaemonConfigArgs {
                mode: Some(PersistenceModeArg::AlwaysOn),
                auto_start: Some(false),
            }),
        )
        .await
        .unwrap();

        let config = storage.load_config().unwrap();
        assert_eq!(config.daemon.persistence_mode, PersistenceMode::AlwaysOn);
        assert!(!config.daemon.auto_start);
    }

    #[tokio::test]
    async fn mission_command_add_posts_schedule_request() {
        let storage = temp_storage();
        let token = "test-mission-token";
        let mission = Mission::new("Ship it".to_string(), "details".to_string());
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("POST", "/v1/missions", &mission),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        mission_command(
            &storage,
            MissionCommands::Add {
                title: "Ship it".to_string(),
                details: "details".to_string(),
                alias: Some("main".to_string()),
                model: Some("gpt-5".to_string()),
                after_seconds: Some(60),
                every_seconds: None,
                at: None,
                watch: None,
                watch_nonrecursive: false,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: Mission = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.title, "Ship it");
        assert_eq!(body.alias.as_deref(), Some("main"));
        assert_eq!(body.requested_model.as_deref(), Some("gpt-5"));
        assert_eq!(body.status, MissionStatus::Scheduled);
        assert_eq!(body.wake_trigger, Some(WakeTrigger::Timer));
    }

    #[tokio::test]
    async fn memory_command_search_posts_workspace_scoped_query() {
        let storage = temp_storage();
        let token = "test-memory-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/memory/search",
                    &MemorySearchResponse {
                        memories: Vec::new(),
                        transcript_hits: Vec::new(),
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        memory_command(
            &storage,
            MemoryCommands::Search {
                query: "build output".to_string(),
                limit: 5,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: MemorySearchQuery = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.query, "build output");
        assert_eq!(body.limit, Some(5));
        assert!(body.workspace_key.is_some());
    }

    #[tokio::test]
    async fn memory_command_rebuild_posts_request() {
        let storage = temp_storage();
        let token = "test-memory-rebuild-token";
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json(
                    "POST",
                    "/v1/memory/rebuild",
                    &MemoryRebuildResponse {
                        generated_at: chrono::Utc::now(),
                        session_id: Some("session-1".to_string()),
                        sessions_scanned: 1,
                        observations_scanned: 4,
                        memories_upserted: 2,
                        embeddings_refreshed: 2,
                    },
                ),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        memory_command(
            &storage,
            MemoryCommands::Rebuild {
                session_id: Some("session-1".to_string()),
                recompute_embeddings: true,
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        let body: MemoryRebuildRequest = serde_json::from_str(&requests[1].body).unwrap();
        assert_eq!(body.session_id.as_deref(), Some("session-1"));
        assert!(body.recompute_embeddings);
    }

    #[tokio::test]
    async fn session_resume_packet_command_requests_resume_packet_from_daemon() {
        let storage = temp_storage();
        let token = "test-session-resume-packet-token";
        let packet = SessionResumePacket {
            session: SessionSummary {
                id: "session-1".to_string(),
                title: Some("Resume me".to_string()),
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-5".to_string(),
                task_mode: Some(TaskMode::Daily),
                message_count: 2,
                cwd: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            generated_at: chrono::Utc::now(),
            recent_messages: Vec::new(),
            linked_memories: Vec::new(),
            related_transcript_hits: Vec::new(),
        };
        let server = spawn_mock_http_server(
            vec![
                MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
                MockHttpExpectation::json("GET", "/v1/sessions/session-1/resume-packet", &packet),
            ],
            Some(format!("Bearer {token}")),
        )
        .await;
        save_daemon_config(&storage, &server.origin, token);

        session_command(
            &storage,
            SessionCommands::ResumePacket {
                id: "session-1".to_string(),
            },
        )
        .await
        .unwrap();

        let requests = server.finish().await.unwrap();
        assert_eq!(requests[1].path, "/v1/sessions/session-1/resume-packet");
    }

