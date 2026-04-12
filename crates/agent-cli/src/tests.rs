use super::*;
use crate::test_support::{spawn_mock_http_server, temp_storage, MockHttpExpectation};
use agent_core::{
    plugin_provider_id, AppConfig, AutonomyProfile, BraveConnectorConfig, DashboardLaunchResponse,
    DelegationConfig, DiscordSendResponse, GmailConnectorConfig, HomeAssistantServiceCallResponse,
    InstalledPluginConfig, MainTargetSummary, MemorySearchResponse, PermissionPreset,
    PluginCompatibility, PluginManifest, PluginPermissions, PluginProviderAdapterManifest,
    PluginSourceKind, ProviderKind, RunTaskResponse, SessionMessage, SessionSummary,
    SignalSendResponse, SlackSendResponse, PLUGIN_SCHEMA_VERSION,
};
use clap::Parser;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

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

#[tokio::test]
async fn autonomy_evolve_and_autopilot_status_commands_hit_daemon() {
    let storage = temp_storage();
    let token = "test-status-token";
    let server = spawn_mock_http_server(
        vec![
            MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
            MockHttpExpectation::json("GET", "/v1/autonomy/status", &AutonomyProfile::default()),
            MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
            MockHttpExpectation::json("GET", "/v1/evolve/status", &EvolveConfig::default()),
            MockHttpExpectation::json("GET", "/v1/status", &daemon_status_fixture()),
            MockHttpExpectation::json("GET", "/v1/autopilot/status", &AutopilotConfig::default()),
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
    let ha_body: HomeAssistantServiceCallRequest = serde_json::from_str(&requests[9].body).unwrap();
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
    let error =
        drain_ndjson_buffer::<StreamFixture, _>(&mut buffer, true, &mut |_| Ok(())).unwrap_err();
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
    let redirect_uri =
        format!("http://localhost:{OPENAI_BROWSER_CALLBACK_PORT}{OPENAI_BROWSER_CALLBACK_PATH}");
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
    let redirect_uri =
        format!("http://localhost:{CLAUDE_BROWSER_CALLBACK_PORT}{CLAUDE_BROWSER_CALLBACK_PATH}");
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

#[test]
fn cli_parses_resume_last() {
    let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "resume", "--last"]);
    match cli.command {
        Some(Commands::Resume(args)) => {
            assert!(args.last);
            assert!(args.session_id.is_none());
        }
        _ => panic!("expected resume command"),
    }
}

#[test]
fn cli_parses_browser_auth_for_login() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "login",
        "--id",
        "openrouter",
        "--name",
        "OpenRouter",
        "--kind",
        "openrouter",
        "--auth",
        "browser",
        "--model",
        "openai/gpt-4.1",
    ]);
    match cli.command {
        Some(Commands::Login(args)) => {
            assert_eq!(args.kind, Some(HostedKindArg::Openrouter));
            assert_eq!(args.auth, Some(AuthMethodArg::Browser));
        }
        _ => panic!("expected login command"),
    }
}

#[test]
fn cli_parses_daemon_config_bool_value() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "daemon",
        "config",
        "--mode",
        "always-on",
        "--auto-start",
        "true",
    ]);
    match cli.command {
        Some(Commands::Daemon {
            command: DaemonCommands::Config(args),
        }) => {
            assert_eq!(args.mode, Some(PersistenceModeArg::AlwaysOn));
            assert_eq!(args.auto_start, Some(true));
        }
        _ => panic!("expected daemon config command"),
    }
}

#[test]
fn cli_parses_trust_bool_values() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "trust",
        "--allow-shell",
        "true",
        "--allow-network",
        "false",
    ]);
    match cli.command {
        Some(Commands::Trust(args)) => {
            assert_eq!(args.allow_shell, Some(true));
            assert_eq!(args.allow_network, Some(false));
        }
        _ => panic!("expected trust command"),
    }
}

#[test]
fn parse_interactive_command_supports_model_mode_and_thinking() {
    assert_eq!(
        parse_interactive_command("/model claude").unwrap(),
        Some(InteractiveCommand::ModelSet("claude".to_string()))
    );
    assert_eq!(
        parse_interactive_command("/provider anthropic").unwrap(),
        Some(InteractiveCommand::ProviderSet("anthropic".to_string()))
    );
    assert_eq!(
        parse_interactive_command("/provider").unwrap(),
        Some(InteractiveCommand::ProviderShow)
    );
    assert_eq!(
        parse_interactive_command("/onboard").unwrap(),
        Some(InteractiveCommand::Onboard)
    );
    assert_eq!(
        parse_interactive_command("/thinking high").unwrap(),
        Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
    );
    assert_eq!(
        parse_interactive_command("/thinking default").unwrap(),
        Some(InteractiveCommand::ThinkingSet(None))
    );
    assert_eq!(
        parse_interactive_command("/mode daily").unwrap(),
        Some(InteractiveCommand::ModeSet(Some(TaskMode::Daily)))
    );
    assert_eq!(
        parse_interactive_command("/mode default").unwrap(),
        Some(InteractiveCommand::ModeSet(None))
    );
}

#[test]
fn resolve_interactive_provider_selection_prefers_logged_in_provider_aliases() {
    let storage = temp_storage();
    let config = AppConfig {
        providers: vec![
            ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                auth_mode: AuthMode::None,
                default_model: Some("gpt-5".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            },
            ProviderConfig {
                id: "anthropic".to_string(),
                display_name: "Claude".to_string(),
                kind: ProviderKind::Anthropic,
                base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                auth_mode: AuthMode::None,
                default_model: Some("claude-sonnet".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            },
        ],
        aliases: vec![
            ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-5".to_string(),
                description: None,
            },
            ModelAlias {
                alias: "claude".to_string(),
                provider_id: "anthropic".to_string(),
                model: "claude-sonnet".to_string(),
                description: None,
            },
        ],
        main_agent_alias: Some("main".to_string()),
        ..AppConfig::default()
    };
    storage.save_config(&config).unwrap();

    assert_eq!(
        resolve_interactive_provider_selection(&storage, Some("main"), "anthropic").unwrap(),
        "claude"
    );
    assert_eq!(
        resolve_interactive_provider_selection(&storage, Some("main"), "Claude").unwrap(),
        "claude"
    );
    assert_eq!(
        resolve_interactive_provider_selection(&storage, Some("main"), "claude").unwrap(),
        "claude"
    );
}

#[test]
fn normalize_model_selection_value_ignores_punctuation() {
    assert_eq!(normalize_model_selection_value("gpt-5.4"), "gpt54");
    assert_eq!(normalize_model_selection_value("GPT 5_4"), "gpt54");
}

#[test]
fn resolved_requested_model_prefers_explicit_override() {
    let alias = ModelAlias {
        alias: "main".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-5.2".to_string(),
        description: None,
    };

    assert_eq!(resolved_requested_model(&alias, Some("gpt-5.4")), "gpt-5.4");
    assert_eq!(resolved_requested_model(&alias, None), "gpt-5.2");
}

#[test]
fn parse_interactive_command_supports_review_and_status() {
    assert_eq!(
        parse_interactive_command("/review focus on tests").unwrap(),
        Some(InteractiveCommand::Review(Some(
            "focus on tests".to_string()
        )))
    );
    assert_eq!(
        parse_interactive_command("/status").unwrap(),
        Some(InteractiveCommand::Status)
    );
    assert_eq!(
        parse_interactive_command("/config").unwrap(),
        Some(InteractiveCommand::ConfigShow)
    );
    assert_eq!(
        parse_interactive_command("/dashboard").unwrap(),
        Some(InteractiveCommand::DashboardOpen)
    );
    assert_eq!(
        parse_interactive_command("/telegrams").unwrap(),
        Some(InteractiveCommand::TelegramsShow)
    );
    assert_eq!(
        parse_interactive_command("/telegram approvals").unwrap(),
        Some(InteractiveCommand::TelegramApprovalsShow)
    );
    assert_eq!(
        parse_interactive_command("/discords").unwrap(),
        Some(InteractiveCommand::DiscordsShow)
    );
    assert_eq!(
        parse_interactive_command("/home-assistant").unwrap(),
        Some(InteractiveCommand::HomeAssistantsShow)
    );
    assert_eq!(
        parse_interactive_command("/telegram approve req-1 looks good").unwrap(),
        Some(InteractiveCommand::TelegramApprove {
            id: "req-1".to_string(),
            note: Some("looks good".to_string()),
        })
    );
    assert_eq!(
        parse_interactive_command("/webhooks").unwrap(),
        Some(InteractiveCommand::WebhooksShow)
    );
    assert_eq!(
        parse_interactive_command("/inboxes").unwrap(),
        Some(InteractiveCommand::InboxesShow)
    );
}

#[test]
fn parse_interactive_command_supports_events_and_schedule() {
    assert_eq!(
        parse_interactive_command("/events 25").unwrap(),
        Some(InteractiveCommand::EventsShow(25))
    );
    assert_eq!(
        parse_interactive_command("/schedule 300 review auth flow").unwrap(),
        Some(InteractiveCommand::Schedule {
            after_seconds: 300,
            title: "review auth flow".to_string(),
        })
    );
    assert_eq!(
        parse_interactive_command("/repeat 600 weekly cleanup").unwrap(),
        Some(InteractiveCommand::Repeat {
            every_seconds: 600,
            title: "weekly cleanup".to_string(),
        })
    );
    assert_eq!(
        parse_interactive_command("/watch src watch auth changes").unwrap(),
        Some(InteractiveCommand::Watch {
            path: PathBuf::from("src"),
            title: "watch auth changes".to_string(),
        })
    );
}

#[test]
fn parse_interactive_command_supports_profile_and_skills() {
    assert_eq!(
        parse_interactive_command("/profile").unwrap(),
        Some(InteractiveCommand::ProfileShow)
    );
    assert_eq!(
        parse_interactive_command("/skills published").unwrap(),
        Some(InteractiveCommand::Skills(InteractiveSkillCommand::Show(
            Some(SkillDraftStatus::Published)
        )))
    );
    assert_eq!(
        parse_interactive_command("/skills publish draft-1").unwrap(),
        Some(InteractiveCommand::Skills(
            InteractiveSkillCommand::Publish("draft-1".to_string())
        ))
    );
}

#[test]
fn parse_interactive_command_supports_memory_review_actions() {
    assert_eq!(
        parse_interactive_command("/memory review").unwrap(),
        Some(InteractiveCommand::MemoryReviewShow)
    );
    assert_eq!(
        parse_interactive_command("/memory rebuild session-1").unwrap(),
        Some(InteractiveCommand::MemoryRebuild {
            session_id: Some("session-1".to_string()),
        })
    );
    assert_eq!(
        parse_interactive_command("/memory approve mem-1 looks good").unwrap(),
        Some(InteractiveCommand::MemoryApprove {
            id: "mem-1".to_string(),
            note: Some("looks good".to_string()),
        })
    );
    assert_eq!(
        parse_interactive_command("/memory reject mem-2 duplicate").unwrap(),
        Some(InteractiveCommand::MemoryReject {
            id: "mem-2".to_string(),
            note: Some("duplicate".to_string()),
        })
    );
}

#[test]
fn parse_interactive_command_supports_discord_review_actions() {
    assert_eq!(
        parse_interactive_command("/discord approvals").unwrap(),
        Some(InteractiveCommand::DiscordApprovalsShow)
    );
    assert_eq!(
        parse_interactive_command("/discord approve appr-1 trusted").unwrap(),
        Some(InteractiveCommand::DiscordApprove {
            id: "appr-1".to_string(),
            note: Some("trusted".to_string()),
        })
    );
    assert_eq!(
        parse_interactive_command("/discord reject appr-2 spam").unwrap(),
        Some(InteractiveCommand::DiscordReject {
            id: "appr-2".to_string(),
            note: Some("spam".to_string()),
        })
    );
}

#[test]
fn cli_parses_mission_schedule_flags() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "mission",
        "add",
        "Follow up",
        "--after-seconds",
        "120",
    ]);
    match cli.command {
        Some(Commands::Mission {
            command:
                MissionCommands::Add {
                    title,
                    after_seconds,
                    at,
                    ..
                },
        }) => {
            assert_eq!(title, "Follow up");
            assert_eq!(after_seconds, Some(120));
            assert_eq!(at, None);
        }
        _ => panic!("expected scheduled mission add command"),
    }
}

#[test]
fn cli_parses_mission_watch_flags() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "mission",
        "add",
        "Watch repo",
        "--watch",
        "src",
        "--watch-nonrecursive",
    ]);
    match cli.command {
        Some(Commands::Mission {
            command:
                MissionCommands::Add {
                    watch,
                    watch_nonrecursive,
                    ..
                },
        }) => {
            assert_eq!(watch, Some(PathBuf::from("src")));
            assert!(watch_nonrecursive);
        }
        _ => panic!("expected watched mission add command"),
    }
}

#[test]
fn cli_parses_run_and_chat_modes() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "run",
        "--mode",
        "daily",
        "plan my week",
    ]);
    match cli.command {
        Some(Commands::Run(args)) => {
            assert_eq!(args.mode, Some(TaskModeArg::Daily));
            assert_eq!(args.prompt.as_deref(), Some("plan my week"));
        }
        _ => panic!("expected run command"),
    }

    let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "chat", "--mode", "build"]);
    match cli.command {
        Some(Commands::Chat(args)) => {
            assert_eq!(args.mode, Some(TaskModeArg::Build));
        }
        _ => panic!("expected chat command"),
    }
}

#[test]
fn cli_parses_skill_draft_listing() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "skills",
        "drafts",
        "--status",
        "published",
        "--limit",
        "5",
    ]);
    match cli.command {
        Some(Commands::Skills {
            command: SkillCommands::Drafts { limit, status },
        }) => {
            assert_eq!(limit, 5);
            assert_eq!(status, Some(SkillDraftStatusArg::Published));
        }
        _ => panic!("expected skill drafts command"),
    }
}

#[test]
fn cli_parses_memory_profile_command() {
    let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "memory", "profile", "--limit", "7"]);
    match cli.command {
        Some(Commands::Memory {
            command: MemoryCommands::Profile { limit },
        }) => {
            assert_eq!(limit, 7);
        }
        _ => panic!("expected memory profile command"),
    }
}

#[test]
fn cli_parses_memory_rebuild_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "memory",
        "rebuild",
        "--session-id",
        "session-1",
        "--recompute-embeddings",
    ]);
    match cli.command {
        Some(Commands::Memory {
            command:
                MemoryCommands::Rebuild {
                    session_id,
                    recompute_embeddings,
                },
        }) => {
            assert_eq!(session_id.as_deref(), Some("session-1"));
            assert!(recompute_embeddings);
        }
        _ => panic!("expected memory rebuild command"),
    }
}

#[test]
fn cli_parses_session_resume_packet_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "session",
        "resume-packet",
        "session-1",
    ]);
    match cli.command {
        Some(Commands::Session {
            command: SessionCommands::ResumePacket { id },
        }) => {
            assert_eq!(id, "session-1");
        }
        _ => panic!("expected session resume-packet command"),
    }
}

#[test]
fn cli_parses_telegram_add_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "telegram",
        "add",
        "--id",
        "ops-bot",
        "--name",
        "Ops Bot",
        "--description",
        "telegram resident connector",
        "--bot-token",
        "123:abc",
        "--chat-id",
        "42",
        "--user-id",
        "7",
    ]);
    match cli.command {
        Some(Commands::Telegram {
            command: TelegramCommands::Add(args),
        }) => {
            assert_eq!(args.id, "ops-bot");
            assert_eq!(args.chat_ids, vec![42]);
            assert_eq!(args.user_ids, vec![7]);
            assert!(args.require_pairing_approval);
        }
        _ => panic!("expected telegram add command"),
    }
}

#[test]
fn cli_parses_webhook_add_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "webhook",
        "add",
        "--id",
        "alerts",
        "--name",
        "Alerts",
        "--description",
        "system alerts",
        "--prompt-template",
        "Check {payload_json}",
    ]);
    match cli.command {
        Some(Commands::Webhook {
            command: WebhookCommands::Add(args),
        }) => {
            assert_eq!(args.id, "alerts");
            assert_eq!(args.name, "Alerts");
        }
        _ => panic!("expected webhook add command"),
    }
}

#[test]
fn cli_parses_discord_add_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "discord",
        "add",
        "--id",
        "ops-discord",
        "--name",
        "Ops Discord",
        "--description",
        "discord resident connector",
        "--bot-token",
        "discord-secret",
        "--monitored-channel-id",
        "123",
        "--allowed-channel-id",
        "456",
        "--user-id",
        "789",
    ]);
    match cli.command {
        Some(Commands::Discord {
            command: DiscordCommands::Add(args),
        }) => {
            assert_eq!(args.id, "ops-discord");
            assert_eq!(args.monitored_channel_ids, vec!["123".to_string()]);
            assert_eq!(args.allowed_channel_ids, vec!["456".to_string()]);
            assert_eq!(args.user_ids, vec!["789".to_string()]);
            assert!(args.require_pairing_approval);
        }
        _ => panic!("expected discord add command"),
    }
}

#[test]
fn cli_parses_inbox_add_command() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "inbox",
        "add",
        "--id",
        "dropbox",
        "--name",
        "Drop Box",
        "--description",
        "local inbox",
        "--path",
        "tasks",
    ]);
    match cli.command {
        Some(Commands::Inbox {
            command: InboxCommands::Add(args),
        }) => {
            assert_eq!(args.id, "dropbox");
            assert_eq!(args.name, "Drop Box");
            assert_eq!(args.path, PathBuf::from("tasks"));
        }
        _ => panic!("expected inbox add command"),
    }
}

#[test]
fn interactive_thinking_command_parses_default_and_levels() {
    assert_eq!(
        parse_interactive_command("/thinking").unwrap(),
        Some(InteractiveCommand::ThinkingShow)
    );
    assert_eq!(
        parse_interactive_command("/thinking high").unwrap(),
        Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
    );
    assert_eq!(
        parse_interactive_command("/thinking default").unwrap(),
        Some(InteractiveCommand::ThinkingSet(None))
    );
    assert_eq!(
        parse_interactive_command("/mode").unwrap(),
        Some(InteractiveCommand::ModeShow)
    );
}

#[test]
fn interactive_model_and_review_commands_parse() {
    assert_eq!(
        parse_interactive_command("/model claude").unwrap(),
        Some(InteractiveCommand::ModelSet("claude".to_string()))
    );
    assert_eq!(
        parse_interactive_command("/settings").unwrap(),
        Some(InteractiveCommand::ConfigShow)
    );
    assert_eq!(
        parse_interactive_command("/review focus on auth").unwrap(),
        Some(InteractiveCommand::Review(Some(
            "focus on auth".to_string()
        )))
    );
}

#[test]
fn interactive_copy_compact_init_and_rename_commands_parse() {
    assert_eq!(
        parse_interactive_command("/copy").unwrap(),
        Some(InteractiveCommand::Copy)
    );
    assert_eq!(
        parse_interactive_command("/compact").unwrap(),
        Some(InteractiveCommand::Compact)
    );
    assert_eq!(
        parse_interactive_command("/init").unwrap(),
        Some(InteractiveCommand::Init)
    );
    assert_eq!(
        parse_interactive_command("/rename auth fixes").unwrap(),
        Some(InteractiveCommand::Rename(Some("auth fixes".to_string())))
    );
}

#[test]
fn build_compact_prompt_includes_transcript_content() {
    let transcript = SessionTranscript {
        session: agent_core::SessionSummary {
            id: "session-1".to_string(),
            title: Some("Test".to_string()),
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            task_mode: None,
            message_count: 0,
            cwd: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        messages: vec![
            agent_core::SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                "Fix auth".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ),
            agent_core::SessionMessage::new(
                "session-1".to_string(),
                MessageRole::Assistant,
                "Working on it".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ),
        ],
    };

    let prompt = build_compact_prompt(&transcript).unwrap();
    assert!(prompt.contains("Fix auth"));
    assert!(prompt.contains("Working on it"));
}

#[test]
fn truncate_for_prompt_preserves_utf8_boundaries() {
    let text = format!("{}😀tail", "a".repeat(19_997));
    let truncated = truncate_for_prompt(text, 20_000);
    assert!(truncated.ends_with("\n\n[truncated]"));
    assert!(!truncated.contains("😀tail"));
}

#[test]
fn determine_logout_targets_uses_main_alias_provider() {
    let config = AppConfig {
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "anthropic".to_string(),
            model: "claude-opus".to_string(),
            description: None,
        }],
        providers: vec![ProviderConfig {
            id: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
            kind: ProviderKind::Anthropic,
            base_url: DEFAULT_ANTHROPIC_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: Some("claude-opus".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        }],
        ..AppConfig::default()
    };

    let targets = determine_logout_targets(
        &config,
        &LogoutArgs {
            provider: None,
            all: false,
        },
    )
    .unwrap();

    assert_eq!(targets, vec!["anthropic".to_string()]);
}

#[test]
fn configured_keychain_accounts_deduplicates_and_skips_missing() {
    let config = AppConfig {
        providers: vec![
            ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                auth_mode: AuthMode::OAuth,
                default_model: Some("gpt-5".to_string()),
                keychain_account: Some("shared-account".to_string()),
                oauth: None,
                local: false,
            },
            ProviderConfig {
                id: "openai-2".to_string(),
                display_name: "OpenAI 2".to_string(),
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                auth_mode: AuthMode::OAuth,
                default_model: Some("gpt-5".to_string()),
                keychain_account: Some("shared-account".to_string()),
                oauth: None,
                local: false,
            },
            ProviderConfig {
                id: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                kind: ProviderKind::Anthropic,
                base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                auth_mode: AuthMode::ApiKey,
                default_model: Some("claude-opus".to_string()),
                keychain_account: None,
                oauth: None,
                local: false,
            },
        ],
        telegram_connectors: vec![TelegramConnectorConfig {
            id: "ops-bot".to_string(),
            name: "Ops Bot".to_string(),
            description: String::new(),
            enabled: true,
            bot_token_keychain_account: Some("telegram-account".to_string()),
            require_pairing_approval: true,
            allowed_chat_ids: Vec::new(),
            allowed_user_ids: Vec::new(),
            last_update_id: None,
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        discord_connectors: vec![DiscordConnectorConfig {
            id: "ops-discord".to_string(),
            name: "Ops Discord".to_string(),
            description: String::new(),
            enabled: true,
            bot_token_keychain_account: Some("discord-account".to_string()),
            require_pairing_approval: true,
            monitored_channel_ids: vec!["123".to_string()],
            allowed_channel_ids: vec!["456".to_string()],
            allowed_user_ids: vec!["789".to_string()],
            channel_cursors: vec![DiscordChannelCursor {
                channel_id: "123".to_string(),
                last_message_id: Some("999".to_string()),
            }],
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        slack_connectors: vec![SlackConnectorConfig {
            id: "ops-slack".to_string(),
            name: "Ops Slack".to_string(),
            description: String::new(),
            enabled: true,
            bot_token_keychain_account: Some("slack-account".to_string()),
            require_pairing_approval: true,
            monitored_channel_ids: vec!["C123".to_string()],
            allowed_channel_ids: Vec::new(),
            allowed_user_ids: Vec::new(),
            channel_cursors: Vec::new(),
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        home_assistant_connectors: vec![HomeAssistantConnectorConfig {
            id: "ops-home".to_string(),
            name: "Ops Home".to_string(),
            description: String::new(),
            enabled: true,
            base_url: "http://ha.local".to_string(),
            access_token_keychain_account: Some("home-account".to_string()),
            monitored_entity_ids: vec!["light.office".to_string()],
            allowed_service_domains: Vec::new(),
            allowed_service_entity_ids: Vec::new(),
            entity_cursors: Vec::new(),
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        brave_connectors: vec![BraveConnectorConfig {
            id: "brave-search".to_string(),
            name: "Brave Search".to_string(),
            description: String::new(),
            enabled: true,
            api_key_keychain_account: Some("brave-account".to_string()),
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        gmail_connectors: vec![GmailConnectorConfig {
            id: "gmail-ops".to_string(),
            name: "Ops Gmail".to_string(),
            description: String::new(),
            enabled: true,
            oauth_keychain_account: Some("gmail-account".to_string()),
            require_pairing_approval: true,
            allowed_sender_addresses: Vec::new(),
            label_filter: Some("INBOX".to_string()),
            last_history_id: None,
            alias: None,
            requested_model: None,
            cwd: None,
        }],
        ..AppConfig::default()
    };

    let accounts = configured_keychain_accounts(&config);

    assert_eq!(accounts.len(), 7);
    assert!(accounts.contains("shared-account"));
    assert!(accounts.contains("telegram-account"));
    assert!(accounts.contains("discord-account"));
    assert!(accounts.contains("slack-account"));
    assert!(accounts.contains("home-account"));
    assert!(accounts.contains("brave-account"));
    assert!(accounts.contains("gmail-account"));
}

#[test]
fn cli_parses_reset_yes() {
    let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "reset", "--yes"]);

    match cli.command {
        Some(Commands::Reset(args)) => assert!(args.yes),
        _ => panic!("expected reset command"),
    }
}

#[test]
fn cli_parses_openrouter_provider_add() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "provider",
        "add",
        "--id",
        "openrouter",
        "--name",
        "OpenRouter",
        "--kind",
        "openrouter",
        "--model",
        "openai/gpt-4.1",
        "--api-key",
        "secret",
    ]);

    match cli.command {
        Some(Commands::Provider {
            command: ProviderCommands::Add(args),
        }) => {
            assert_eq!(args.kind, HostedKindArg::Openrouter);
            assert_eq!(args.model, "openai/gpt-4.1");
        }
        _ => panic!("expected provider add command"),
    }
}

#[test]
fn cli_parses_openai_provider_add_alias() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "provider",
        "add",
        "--id",
        "openai",
        "--name",
        "OpenAI",
        "--kind",
        "openai",
        "--model",
        "gpt-4.1",
        "--api-key",
        "secret",
    ]);

    match cli.command {
        Some(Commands::Provider {
            command: ProviderCommands::Add(args),
        }) => {
            assert_eq!(args.kind, HostedKindArg::OpenaiCompatible);
            assert_eq!(args.model, "gpt-4.1");
        }
        _ => panic!("expected provider add command"),
    }
}

#[test]
fn cli_parses_venice_provider_add() {
    let cli = Cli::parse_from([
        PRIMARY_COMMAND_NAME,
        "provider",
        "add",
        "--id",
        "venice",
        "--name",
        "Venice",
        "--kind",
        "venice",
        "--model",
        "venice-large",
        "--api-key",
        "secret",
    ]);

    match cli.command {
        Some(Commands::Provider {
            command: ProviderCommands::Add(args),
        }) => {
            assert_eq!(args.kind, HostedKindArg::Venice);
            assert_eq!(args.model, "venice-large");
        }
        _ => panic!("expected provider add command"),
    }
}

#[test]
fn needs_onboarding_for_default_config() {
    assert!(needs_onboarding(&AppConfig::default()));
}

#[test]
fn onboarding_not_needed_when_main_alias_resolves() {
    let config = AppConfig {
        onboarding_complete: true,
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        }],
        providers: vec![ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::Ollama,
            base_url: DEFAULT_OPENAI_URL.to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        }],
        ..AppConfig::default()
    };

    assert!(!needs_onboarding(&config));
}

#[test]
fn onboarding_not_needed_when_projected_plugin_provider_resolves() {
    let config = projected_plugin_config("main", "echo-provider", "echo-1");

    assert!(has_usable_main_alias(&config));
    assert!(!needs_onboarding(&config));
}

#[test]
fn usable_main_alias_does_not_depend_on_onboarding_complete() {
    let config = AppConfig {
        onboarding_complete: false,
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        }],
        providers: vec![ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::Ollama,
            base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("gpt-4.1".to_string()),
            keychain_account: None,
            oauth: None,
            local: true,
        }],
        ..AppConfig::default()
    };

    assert!(has_usable_main_alias(&config));
    assert!(needs_onboarding(&config));
}

#[test]
fn main_alias_is_not_usable_when_provider_is_missing() {
    let config = AppConfig {
        onboarding_complete: true,
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "missing".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        }],
        ..AppConfig::default()
    };

    assert!(!has_usable_main_alias(&config));
    assert!(needs_onboarding(&config));
}

#[test]
fn main_alias_is_not_usable_when_provider_credentials_are_missing() {
    let config = AppConfig {
        onboarding_complete: true,
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        }],
        providers: vec![ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::ChatGptCodex,
            base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
            auth_mode: AuthMode::OAuth,
            default_model: Some("gpt-4.1".to_string()),
            keychain_account: None,
            oauth: Some(openai_browser_oauth_config()),
            local: false,
        }],
        ..AppConfig::default()
    };

    assert!(!has_usable_main_alias(&config));
    assert!(needs_onboarding(&config));
}

#[test]
fn main_alias_is_not_usable_when_saved_credentials_are_unreadable() {
    let config = AppConfig {
        onboarding_complete: true,
        main_agent_alias: Some("main".to_string()),
        aliases: vec![ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        }],
        providers: vec![ProviderConfig {
            id: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: DEFAULT_OPENAI_URL.to_string(),
            auth_mode: AuthMode::ApiKey,
            default_model: Some("gpt-4.1".to_string()),
            keychain_account: Some("missing-provider-account".to_string()),
            oauth: None,
            local: false,
        }],
        ..AppConfig::default()
    };

    assert!(!has_usable_main_alias(&config));
    assert!(needs_onboarding(&config));
}
