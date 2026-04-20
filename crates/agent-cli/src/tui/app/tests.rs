use super::{
    browser_action_label, build_thinking_picker_entries, cursor_line_and_column,
    hosted_kind_for_provider, line_column_to_offset, next_char_boundary, previous_char_boundary,
    GenericPickerEntry, ModelPickerEntry, PickerAction, PickerMode, PickerState, TuiApp,
};
use crate::test_support::{spawn_mock_http_server, MockHttpExpectation};
use crate::HostedKindArg;
use agent_core::{
    plugin_provider_id, AppConfig, AuthMode, InstalledPluginConfig, MainTargetSummary, ModelAlias,
    PermissionPreset, PluginCompatibility, PluginManifest, PluginPermissions,
    PluginProviderAdapterManifest, PluginSourceKind, ProviderConfig, ProviderKind, SessionSummary,
    TaskMode, ThinkingLevel, UpdateAvailabilityState, UpdateInstallKind, UpdateInstallTarget,
    UpdateStatusResponse, DEFAULT_OPENROUTER_URL, PLUGIN_SCHEMA_VERSION,
};
use agent_providers::{ModelDescriptor, ReasoningLevelDescriptor};
use agent_storage::Storage;
use uuid::Uuid;

fn temp_storage() -> Storage {
    Storage::open_at(std::env::temp_dir().join(format!("nuclear-tui-test-{}", Uuid::new_v4())))
        .unwrap()
}

fn sample_provider(id: &str, display_name: &str, model: &str) -> ProviderConfig {
    ProviderConfig {
        id: id.to_string(),
        display_name: display_name.to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://127.0.0.1:11434".to_string(),
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

fn build_test_app<'a>(storage: &'a Storage) -> TuiApp<'a> {
    let config = AppConfig {
        onboarding_complete: true,
        providers: vec![
            sample_provider("codex", "Codex", "gpt-5-codex"),
            sample_provider("anthropic", "Anthropic", "claude-sonnet"),
        ],
        aliases: vec![
            sample_alias("main", "codex", "gpt-5-codex"),
            sample_alias("claude", "anthropic", "claude-sonnet"),
        ],
        main_agent_alias: Some("main".to_string()),
        ..AppConfig::default()
    };
    storage.save_config(&config).unwrap();

    TuiApp {
        storage,
        client: crate::DaemonClient {
            base_url: "http://127.0.0.1:9".to_string(),
            token: "test-token".to_string(),
            http: reqwest::Client::new(),
        },
        alias: Some("main".to_string()),
        session_id: None,
        thinking_level: None,
        task_mode: None,
        permission_preset: Some(PermissionPreset::AutoEdit),
        attachments: Vec::new(),
        cwd: std::env::current_dir().unwrap(),
        transcript: Vec::new(),
        input: String::new(),
        input_cursor: 0,
        overlay: None,
        picker: None,
        pending_external_action: None,
        exit_requested: false,
        busy: false,
        busy_since: None,
        transcript_scroll_back: 0,
        requested_model: None,
        active_model: Some("gpt-5-codex".to_string()),
        active_provider_name: Some("Codex".to_string()),
        context_window_tokens: None,
        context_window_percent: None,
        recent_events: Vec::new(),
        last_event_cursor: None,
        pending_tool_calls: Vec::new(),
        main_target: Some(MainTargetSummary {
            alias: "main".to_string(),
            provider_id: "codex".to_string(),
            provider_display_name: "Codex".to_string(),
            model: "gpt-5-codex".to_string(),
        }),
        restart_event_poller: false,
        pending_prompt_snapshot: None,
    }
}

fn build_projected_plugin_app<'a>(storage: &'a Storage) -> TuiApp<'a> {
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
                id: "echo-provider".to_string(),
                provider_kind: ProviderKind::OpenAiCompatible,
                description: "projected provider".to_string(),
                command: "plugin-host".to_string(),
                args: Vec::new(),
                cwd: None,
                permissions: PluginPermissions::default(),
                default_model: Some("echo-1".to_string()),
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
    let provider_id = plugin_provider_id(&plugin.id, "echo-provider");
    let config = AppConfig {
        onboarding_complete: true,
        plugins: vec![plugin],
        aliases: vec![sample_alias("main", &provider_id, "echo-1")],
        main_agent_alias: Some("main".to_string()),
        ..AppConfig::default()
    };
    storage.save_config(&config).unwrap();

    TuiApp {
        storage,
        client: crate::DaemonClient {
            base_url: "http://127.0.0.1:9".to_string(),
            token: "test-token".to_string(),
            http: reqwest::Client::new(),
        },
        alias: Some("main".to_string()),
        session_id: None,
        thinking_level: None,
        task_mode: None,
        permission_preset: Some(PermissionPreset::AutoEdit),
        attachments: Vec::new(),
        cwd: std::env::current_dir().unwrap(),
        transcript: Vec::new(),
        input: String::new(),
        input_cursor: 0,
        overlay: None,
        picker: None,
        pending_external_action: None,
        exit_requested: false,
        busy: false,
        busy_since: None,
        transcript_scroll_back: 0,
        requested_model: None,
        active_model: Some("echo-1".to_string()),
        active_provider_name: Some("Echo Toolkit / echo-provider".to_string()),
        context_window_tokens: None,
        context_window_percent: None,
        recent_events: Vec::new(),
        last_event_cursor: None,
        pending_tool_calls: Vec::new(),
        main_target: Some(MainTargetSummary {
            alias: "main".to_string(),
            provider_id,
            provider_display_name: "Echo Toolkit / echo-provider".to_string(),
            model: "echo-1".to_string(),
        }),
        restart_event_poller: false,
        pending_prompt_snapshot: None,
    }
}

fn update_status_fixture(availability: UpdateAvailabilityState) -> UpdateStatusResponse {
    UpdateStatusResponse {
        install: UpdateInstallTarget {
            kind: UpdateInstallKind::Packaged,
            executable_path: "C:/Program Files/Nuclear/nuclear.exe".to_string(),
            install_dir: Some("C:/Program Files/Nuclear".to_string()),
            repo_root: None,
            build_profile: None,
        },
        current_version: "0.8.3".to_string(),
        current_commit: None,
        availability,
        checked_at: chrono::Utc::now(),
        step: None,
        candidate_version: Some("0.8.4".to_string()),
        candidate_tag: Some("v0.8.4".to_string()),
        candidate_commit: None,
        published_at: None,
        detail: Some("0.8.4 is available for windows-x64.".to_string()),
        last_run: None,
    }
}

#[test]
fn cursor_boundaries_follow_utf8_chars() {
    let input = "aÃ©";
    assert_eq!(next_char_boundary(input, 0), 1);
    assert_eq!(next_char_boundary(input, 1), 3);
    assert_eq!(previous_char_boundary(input, 3), 1);
}

#[test]
fn line_column_round_trips_for_multiline_input() {
    let input = "alpha\nbeta\ngamma";
    let offset = line_column_to_offset(input, 1, 2);
    assert_eq!(cursor_line_and_column(input, offset), (1, 2));
}

#[test]
fn picker_state_filters_models_by_query() {
    let picker = PickerState {
        mode: PickerMode::Model,
        title: "Models".to_string(),
        hint: String::new(),
        empty_message: String::new(),
        query: "frontier".to_string(),
        selected: 0,
        sessions: Vec::new(),
        models: vec![
            ModelPickerEntry {
                id: "gpt-5.4".to_string(),
                display_name: "gpt-5.4".to_string(),
                description: Some("Latest frontier agentic coding model.".to_string()),
                context_window: Some(272_000),
                effective_context_window_percent: Some(90),
            },
            ModelPickerEntry {
                id: "gpt-oss-20b".to_string(),
                display_name: "gpt-oss-20b".to_string(),
                description: Some("Open weights".to_string()),
                context_window: None,
                effective_context_window_percent: None,
            },
        ],
        items: Vec::new(),
    };

    let filtered = picker.filtered_models();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "gpt-5.4");
    assert_eq!(picker.filtered_len(), 1);
}

#[test]
fn picker_state_filters_generic_items_by_query() {
    let picker = PickerState {
        mode: PickerMode::Config,
        title: "Config".to_string(),
        hint: String::new(),
        empty_message: String::new(),
        query: "network".to_string(),
        selected: 0,
        sessions: Vec::new(),
        models: Vec::new(),
        items: vec![
            GenericPickerEntry {
                label: "Shell access".to_string(),
                detail: Some("enabled".to_string()),
                search_text: "shell".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
            GenericPickerEntry {
                label: "Network access".to_string(),
                detail: Some("disabled".to_string()),
                search_text: "network".to_string(),
                current: false,
                action: PickerAction::OpenConfig,
            },
        ],
    };

    let filtered = picker.filtered_items();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].label, "Network access");
    assert_eq!(picker.filtered_len(), 1);
}

#[test]
fn picker_state_filters_sessions_by_task_mode() {
    let now = chrono::Utc::now();
    let picker = PickerState {
        mode: PickerMode::Resume,
        title: "Sessions".to_string(),
        hint: String::new(),
        empty_message: String::new(),
        query: "daily".to_string(),
        selected: 0,
        sessions: vec![
            SessionSummary {
                id: "session-build".to_string(),
                title: Some("Build work".to_string()),
                alias: "main".to_string(),
                provider_id: "local".to_string(),
                model: "qwen".to_string(),
                task_mode: Some(TaskMode::Build),
                message_count: 4,
                cwd: None,
                created_at: now,
                updated_at: now,
            },
            SessionSummary {
                id: "session-daily".to_string(),
                title: Some("Daily tasks".to_string()),
                alias: "main".to_string(),
                provider_id: "local".to_string(),
                model: "qwen".to_string(),
                task_mode: Some(TaskMode::Daily),
                message_count: 2,
                cwd: None,
                created_at: now,
                updated_at: now + chrono::Duration::seconds(1),
            },
        ],
        models: Vec::new(),
        items: Vec::new(),
    };

    let filtered = picker.filtered_sessions();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "session-daily");
}

#[test]
fn thinking_picker_uses_model_advertised_levels() {
    let descriptor = ModelDescriptor {
        id: "gpt-5.4".to_string(),
        display_name: None,
        description: None,
        context_window: None,
        effective_context_window_percent: None,
        show_in_picker: true,
        default_reasoning_effort: Some("medium".to_string()),
        supported_reasoning_levels: vec![
            ReasoningLevelDescriptor {
                effort: "low".to_string(),
                description: Some("low desc".to_string()),
            },
            ReasoningLevelDescriptor {
                effort: "high".to_string(),
                description: Some("high desc".to_string()),
            },
        ],
        supports_reasoning_summaries: false,
        default_reasoning_summary: None,
        support_verbosity: false,
        default_verbosity: None,
        supports_parallel_tool_calls: false,
        priority: None,
        capabilities: agent_core::ModelToolCapabilities::default(),
    };

    let entries = build_thinking_picker_entries(Some(&descriptor), Some(ThinkingLevel::High));
    let labels = entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["default", "none", "minimal", "low", "high"]);
    assert!(entries
        .iter()
        .any(|entry| entry.label == "high" && entry.current));
}

#[test]
fn hosted_kind_for_provider_maps_known_remote_urls() {
    let provider = ProviderConfig {
        id: "openrouter".to_string(),
        display_name: "OpenRouter".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: DEFAULT_OPENROUTER_URL.to_string(),
        auth_mode: AuthMode::ApiKey,
        default_model: None,
        keychain_account: None,
        oauth: None,
        local: false,
    };
    assert!(matches!(
        hosted_kind_for_provider(&provider),
        Some(HostedKindArg::Openrouter)
    ));
    assert_eq!(browser_action_label(&provider), "Browser sign-in");
}

#[tokio::test]
async fn dashboard_slash_command_sets_external_action() {
    let storage = temp_storage();
    let mut app = build_test_app(&storage);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.handle_slash_command(crate::InteractiveCommand::DashboardOpen, &tx)
        .await
        .unwrap();

    assert!(matches!(
        app.pending_external_action,
        Some(super::ExternalAction::OpenDashboard)
    ));
}

#[tokio::test]
async fn onboard_slash_command_sets_external_action() {
    let storage = temp_storage();
    let mut app = build_test_app(&storage);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.handle_slash_command(crate::InteractiveCommand::Onboard, &tx)
        .await
        .unwrap();

    assert!(matches!(
        app.pending_external_action,
        Some(super::ExternalAction::OnboardReset)
    ));
}

#[tokio::test]
async fn update_slash_command_requests_daemon_and_exits() {
    let storage = temp_storage();
    let mut app = build_test_app(&storage);
    let server = spawn_mock_http_server(
        vec![MockHttpExpectation::json(
            "POST",
            "/v1/update/run",
            &update_status_fixture(UpdateAvailabilityState::InProgress),
        )],
        Some("Bearer test-token".to_string()),
    )
    .await;
    app.client.base_url = server.origin.clone();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.handle_slash_command(crate::InteractiveCommand::UpdateRun, &tx)
        .await
        .unwrap();

    assert!(app.exit_requested);
    let overlay = app.overlay.expect("update overlay should open");
    let super::OverlayState::Static { title, body, .. } = overlay else {
        panic!("expected static overlay");
    };
    assert_eq!(title, "Update");
    assert!(body.contains("install_kind=packaged"));
    assert!(body.contains("Closing this CLI session"));

    let requests = server.finish().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
    assert_eq!(
        body.get("wait_for_pid").and_then(serde_json::Value::as_u64),
        Some(std::process::id() as u64)
    );
}

#[tokio::test]
async fn mode_slash_command_updates_task_mode() {
    let storage = temp_storage();
    let mut app = build_test_app(&storage);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.handle_slash_command(
        crate::InteractiveCommand::ModeSet(Some(TaskMode::Daily)),
        &tx,
    )
    .await
    .unwrap();

    assert_eq!(app.task_mode, Some(TaskMode::Daily));
}

#[tokio::test]
async fn provider_show_opens_switch_picker_with_logged_in_providers() {
    let storage = temp_storage();
    let mut app = build_test_app(&storage);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.handle_slash_command(crate::InteractiveCommand::ProviderShow, &tx)
        .await
        .unwrap();

    let picker = app.picker.expect("provider picker should open");
    assert!(matches!(picker.mode, PickerMode::Provider));
    let labels = picker
        .items
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["Anthropic", "Codex"]);
    assert!(picker.items.iter().any(|item| item
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("alias main"))));
}

#[tokio::test]
async fn queue_prompt_accepts_projected_plugin_provider_alias() {
    let storage = temp_storage();
    let mut app = build_projected_plugin_app(&storage);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    app.queue_prompt("Plan the week".to_string(), &tx).unwrap();

    assert!(app.busy);
    assert_eq!(app.transcript.len(), 1);
    assert_eq!(
        app.transcript[0].provider_id.as_deref(),
        app.main_target
            .as_ref()
            .map(|target| target.provider_id.as_str())
    );
}
