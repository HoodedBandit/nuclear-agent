use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

mod auth;
mod connectors;
mod control;
mod control_socket;
mod dashboard;
mod delegation;
mod memory;
mod missions;
mod patch;
mod patterns;
mod plugins;
mod routes;
mod runtime;
mod sessions;
mod support_bundle;
mod tools;
mod workspace;

#[cfg(test)]
use crate::delegation::{
    provider_pool_candidates, resolve_delegation_tasks, resolve_subagent_candidates,
};
use crate::routes::{build_protected_routes, build_public_routes};
use agent_core::{
    AppConfig, LogEntry, ModelAlias, ProviderConfig, SkillDraftStatus, INTERNAL_DAEMON_ARG,
};
#[cfg(test)]
use agent_core::{
    AuthMode, BatchTaskRequest, DelegationLimit, ProviderSuggestionRequest, SubAgentStrategy,
    SubAgentTask, TaskMode, ThinkingLevel,
};
use agent_storage::Storage;
use anyhow::{Context, Result};
pub(crate) use auth::{
    get_provider_browser_auth_status, new_browser_auth_store, provider_browser_auth_callback,
    provider_browser_auth_complete, start_provider_browser_auth,
};
#[cfg(test)]
use axum::extract::State;
use axum::{http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
pub(crate) use connectors::{
    approve_connector_approval, call_home_assistant_service_route, delete_app_connector,
    delete_brave_connector, delete_discord_connector, delete_gmail_connector,
    delete_home_assistant_connector, delete_inbox_connector, delete_signal_connector,
    delete_slack_connector, delete_telegram_connector, delete_webhook_connector,
    get_brave_connector, get_discord_connector, get_gmail_connector, get_home_assistant_connector,
    get_home_assistant_entity_state_route, get_inbox_connector, get_signal_connector,
    get_slack_connector, get_telegram_connector, get_webhook_connector, list_app_connectors,
    list_brave_connectors, list_connector_approvals, list_discord_connectors,
    list_gmail_connectors, list_home_assistant_connectors, list_inbox_connectors,
    list_signal_connectors, list_slack_connectors, list_telegram_connectors,
    list_webhook_connectors, poll_discord_connector_route, poll_gmail_connector_route,
    poll_home_assistant_connector_route, poll_inbox_connector_route, poll_inbox_connectors,
    poll_signal_connector_route, poll_slack_connector_route, poll_telegram_connector_route,
    receive_webhook_event, reject_connector_approval, send_discord_message_route,
    send_gmail_message_route, send_signal_message_route, send_slack_message_route,
    send_telegram_message_route, upsert_app_connector, upsert_brave_connector,
    upsert_discord_connector, upsert_gmail_connector, upsert_home_assistant_connector,
    upsert_inbox_connector, upsert_signal_connector, upsert_slack_connector,
    upsert_telegram_connector, upsert_webhook_connector,
};
pub(crate) use control::{
    autonomy_status, autopilot_status, clear_provider_credentials, dashboard_bootstrap,
    delegation_status, delete_alias, delete_mcp_server, delete_provider, doctor, enable_autonomy,
    export_config, get_permission_preset, get_trust, import_config, list_aliases,
    list_delegation_targets, list_enabled_skills, list_events, list_logs, list_mcp_servers,
    list_provider_model_descriptors, list_provider_models, list_providers, pause_autonomy,
    reset_onboarding, resume_autonomy, shutdown, status, suggest_provider_defaults,
    update_autopilot, update_daemon_config, update_delegation_config, update_enabled_skills,
    update_main_alias, update_permission_preset, update_trust, upsert_alias, upsert_mcp_server,
    upsert_provider,
};
pub(crate) use delegation::{
    delegation_targets_from_config, normalize_delegation_limit,
    resolve_alias_and_provider_from_config,
};
pub(crate) use memory::{
    approve_memory, build_memory_context, forget_memory, get_skill_draft, learn_from_interaction,
    list_memories, list_memory_review_queue, list_profile_memories, list_skill_drafts,
    load_enabled_skill_guidance, normalize_memory_sentence, publish_skill_draft, rebuild_memory,
    reject_memory, reject_skill_draft, search_memory, sync_system_profile_memories, upsert_memory,
};
pub(crate) use missions::{
    add_mission, autopilot_loop, cancel_mission, evolve_status, get_mission,
    list_mission_checkpoints, list_missions, pause_evolve_mode, pause_mission, resume_evolve_mode,
    resume_mission, start_evolve_mode, stop_evolve_mode,
};
#[cfg(test)]
pub(crate) use missions::{build_mission_prompt, file_change_ready, parse_mission_directive};
pub(crate) use patterns::{detect_patterns, load_pattern_guidance, record_patterns};
pub(crate) use plugins::{
    collect_hosted_plugin_tools, collect_plugin_doctor_reports, delete_plugin, get_plugin,
    get_plugin_doctor_report, install_plugin, list_plugin_doctor_reports, list_plugins,
    update_plugin, update_plugin_state, HostedPluginTool,
};
use reqwest::Client;
pub(crate) use runtime::{
    execute_batch_request, execute_task_request, execute_task_request_with_events,
    resolve_request_cwd, summarize_tool_output, DelegationExecutionOptions, ResolvedSubAgentTask,
    TaskRequestInput,
};
#[cfg(test)]
pub(crate) use runtime::{
    maybe_validate_structured_output, repeated_tool_loop_resolution, ToolBatchExecution,
    ToolLoopResolution,
};
use serde::Deserialize;
pub(crate) use sessions::{
    compact_session, fork_session, get_session, get_session_resume_packet, list_sessions,
    rename_session,
};
#[cfg(test)]
use std::path::PathBuf;
pub(crate) use support_bundle::create_support_bundle;
use tokio::{
    net::TcpListener,
    sync::{mpsc, Notify, RwLock},
};
use tracing::{error, info};
#[cfg(test)]
use uuid::Uuid;
pub use workspace::inspect_workspace_path;
pub(crate) use workspace::{
    inspect_workspace_route, workspace_diff_route, workspace_init_agents_route,
    workspace_shell_route,
};

const MAX_SUBAGENT_TASKS_PER_REQUEST: usize = 8;
const MAX_RESOLVED_SUBAGENT_RUNS: usize = 16;
const MAX_TOOL_LOOP_ITERATIONS: usize = 8;
const REPEATED_TOOL_BATCH_LIMIT: usize = 2;
const DEFAULT_DAEMON_HTTP_TIMEOUT_SECS: u64 = 20;
pub(crate) type DashboardSessionStore = Arc<RwLock<HashMap<String, DateTime<Utc>>>>;
pub(crate) type DashboardLaunchStore = Arc<RwLock<HashMap<String, DateTime<Utc>>>>;
pub(crate) type MissionCancellationStore = Arc<Mutex<HashMap<String, ExecutionCancellation>>>;
pub(crate) const AUTOPILOT_DIRECTIVE_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "status": {
      "type": "string",
      "enum": ["queued", "running", "waiting", "scheduled", "blocked", "completed", "failed", "cancelled"]
    },
    "next_wake_seconds": {
      "type": "integer",
      "minimum": 0
    },
    "next_phase": {
      "type": "string",
      "enum": ["planner", "executor", "reviewer"]
    },
    "handoff_summary": {
      "type": "string",
      "minLength": 1
    },
    "summary": {
      "type": "string",
      "minLength": 1
    },
    "error": {
      "type": "string"
    },
    "follow_up_title": {
      "type": "string",
      "minLength": 1
    },
    "follow_up_details": {
      "type": "string",
      "minLength": 1
    },
    "follow_up_after_seconds": {
      "type": "integer",
      "minimum": 0
    }
  },
  "required": ["status", "summary"],
  "additionalProperties": false
}"#;

/// Simple per-provider token bucket rate limiter.
/// Defaults to 60 requests/minute per provider.
const DEFAULT_RATE_LIMIT_RPM: u32 = 60;

#[derive(Clone)]
pub(crate) struct ProviderRateLimiter {
    buckets: Arc<Mutex<HashMap<String, RateBucket>>>,
}

struct RateBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl ProviderRateLimiter {
    fn new() -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Wait until a token is available for the given provider, then consume it.
    pub(crate) async fn acquire(&self, provider_id: &str) {
        loop {
            let wait = {
                let mut buckets = self.buckets.lock().expect("rate limiter lock poisoned");
                let bucket = buckets
                    .entry(provider_id.to_owned())
                    .or_insert_with(|| RateBucket {
                        tokens: DEFAULT_RATE_LIMIT_RPM as f64,
                        max_tokens: DEFAULT_RATE_LIMIT_RPM as f64,
                        refill_rate: DEFAULT_RATE_LIMIT_RPM as f64 / 60.0,
                        last_refill: Instant::now(),
                    });

                // Refill based on elapsed time.
                let now = Instant::now();
                let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
                bucket.tokens =
                    (bucket.tokens + elapsed * bucket.refill_rate).min(bucket.max_tokens);
                bucket.last_refill = now;

                if bucket.tokens >= 1.0 {
                    bucket.tokens -= 1.0;
                    None // acquired
                } else {
                    // Time until next token is available.
                    Some(Duration::from_secs_f64(
                        (1.0 - bucket.tokens) / bucket.refill_rate,
                    ))
                }
            };

            match wait {
                None => return,
                Some(delay) => tokio::time::sleep(delay).await,
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    storage: Storage,
    config: Arc<RwLock<AppConfig>>,
    http_client: Client,
    browser_auth_sessions: auth::BrowserAuthStore,
    dashboard_sessions: DashboardSessionStore,
    dashboard_launches: DashboardLaunchStore,
    mission_cancellations: MissionCancellationStore,
    started_at: DateTime<Utc>,
    shutdown: mpsc::UnboundedSender<()>,
    autopilot_wake: Arc<Notify>,
    log_wake: Arc<Notify>,
    restart_requested: Arc<AtomicBool>,
    rate_limiter: ProviderRateLimiter,
}

#[derive(Clone, Default)]
pub(crate) struct ExecutionCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl ExecutionCancellation {
    pub(crate) fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        loop {
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
            if self.is_cancelled() {
                return;
            }
        }
    }
}

pub(crate) fn new_dashboard_session_store() -> DashboardSessionStore {
    Arc::new(RwLock::new(HashMap::new()))
}

pub(crate) fn new_dashboard_launch_store() -> DashboardLaunchStore {
    Arc::new(RwLock::new(HashMap::new()))
}

pub(crate) fn new_mission_cancellation_store() -> MissionCancellationStore {
    Arc::new(Mutex::new(HashMap::new()))
}

#[derive(Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
pub(crate) struct SkillDraftQuery {
    limit: Option<usize>,
    status: Option<SkillDraftStatus>,
}

pub async fn run_daemon() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .try_init()
        .ok();

    let storage = Storage::open()?;
    let mut config = storage.load_config()?;
    if config.evolve.pending_restart {
        config.evolve.pending_restart = false;
        storage.save_config(&config)?;
    }
    let address = format!("{}:{}", config.daemon.host, config.daemon.port);
    let socket_addr: SocketAddr = address.parse().context("invalid daemon bind address")?;
    let listener = TcpListener::bind(socket_addr)
        .await
        .with_context(|| format!("failed to bind daemon at {address}"))?;
    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel();
    let autopilot_wake = Arc::new(Notify::new());
    let log_wake = Arc::new(Notify::new());
    let restart_requested = Arc::new(AtomicBool::new(false));

    let state = AppState {
        storage: storage.clone(),
        config: Arc::new(RwLock::new(config.clone())),
        http_client: Client::builder()
            .timeout(Duration::from_secs(DEFAULT_DAEMON_HTTP_TIMEOUT_SECS))
            .build()?,
        browser_auth_sessions: new_browser_auth_store(),
        dashboard_sessions: new_dashboard_session_store(),
        dashboard_launches: new_dashboard_launch_store(),
        mission_cancellations: new_mission_cancellation_store(),
        started_at: Utc::now(),
        shutdown: shutdown_tx,
        autopilot_wake: autopilot_wake.clone(),
        log_wake: log_wake.clone(),
        restart_requested: restart_requested.clone(),
        rate_limiter: ProviderRateLimiter::new(),
    };

    append_log(
        &state,
        "info",
        "daemon",
        format!("daemon listening on {address}"),
    )?;

    tokio::spawn(autopilot_loop(state.clone()));

    let protected_routes = build_protected_routes(state.clone());
    let public_routes = build_public_routes(state.clone());

    let app = public_routes.merge(protected_routes);

    info!("daemon started");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await
        .context("daemon server failed")?;

    if restart_requested.load(Ordering::SeqCst) {
        append_log(
            &state,
            "warn",
            "daemon",
            "restarting daemon after evolve cycle",
        )?;
        spawn_replacement_daemon_process()?;
    }

    Ok(())
}

pub(crate) fn request_daemon_restart(state: &AppState) -> Result<()> {
    state.restart_requested.store(true, Ordering::SeqCst);
    let _ = state.shutdown.send(());
    Ok(())
}

fn spawn_replacement_daemon_process() -> Result<()> {
    let current_exe = std::env::current_exe().context("failed to locate current executable")?;
    let mut command = std::process::Command::new(&current_exe);
    command
        .arg(INTERNAL_DAEMON_ARG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().with_context(|| {
        format!(
            "failed to restart daemon using {} {}",
            current_exe.display(),
            INTERNAL_DAEMON_ARG
        )
    })?;
    Ok(())
}

pub(crate) async fn resolve_alias_and_provider(
    state: &AppState,
    requested_alias: Option<&str>,
) -> Result<(ModelAlias, ProviderConfig), ApiError> {
    let config = state.config.read().await;
    resolve_alias_and_provider_from_config(&config, requested_alias)
}

pub(crate) fn append_log(
    state: &AppState,
    level: &str,
    scope: &str,
    message: impl Into<String>,
) -> Result<()> {
    state
        .storage
        .append_log(&LogEntry::new(level, scope, message.into()))?;
    state.log_wake.notify_waiters();
    Ok(())
}

#[derive(Debug)]
struct ApiError {
    pub(crate) status: StatusCode,
    pub(crate) message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        let error = error.into();
        error!("{error:#}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let mut response = Json(serde_json::json!({
            "error": self.message,
        }))
        .into_response();
        *response.status_mut() = self.status;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        AutonomyMode, AutonomyState, EvolveStartRequest, EvolveState, MainAliasUpdateRequest,
        MemoryEvidenceRef, MemoryKind, MemoryRebuildRequest, MemoryRecord, MessageRole, Mission,
        MissionControlRequest, MissionStatus, ProviderKind, ProviderUpsertRequest, SessionMessage,
        TrustPolicy, WakeTrigger,
    };
    use std::sync::Arc;

    fn provider(id: &str, auth_mode: AuthMode, keychain_account: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            display_name: id.to_string(),
            kind: match id {
                "ollama" => ProviderKind::Ollama,
                "anthropic" => ProviderKind::Anthropic,
                "chatgpt" => ProviderKind::ChatGptCodex,
                _ => ProviderKind::OpenAiCompatible,
            },
            base_url: "https://example.test".to_string(),
            auth_mode,
            default_model: Some(format!("{id}-model")),
            keychain_account: keychain_account.map(ToOwned::to_owned),
            oauth: None,
            local: false,
        }
    }

    fn alias(alias: &str, provider_id: &str, model: &str) -> ModelAlias {
        ModelAlias {
            alias: alias.to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            description: None,
        }
    }

    fn config_with_aliases() -> AppConfig {
        let mut config = AppConfig {
            trust_policy: TrustPolicy::default(),
            ..AppConfig::default()
        };
        config.providers = vec![
            provider("openai", AuthMode::None, None),
            provider("anthropic", AuthMode::None, None),
            provider("ollama", AuthMode::None, None),
        ];
        config.aliases = vec![
            alias("main", "openai", "gpt-5.4"),
            alias("research", "openai", "gpt-5.2"),
            alias("claude", "anthropic", "claude-sonnet"),
            alias("local", "ollama", "qwen"),
        ];
        config.main_agent_alias = Some("main".to_string());
        config
    }

    fn test_state_with_config(config: AppConfig) -> AppState {
        AppState {
            storage: Storage::open_at(
                std::env::temp_dir().join(format!("agent-daemon-test-{}", Uuid::new_v4())),
            )
            .unwrap(),
            config: Arc::new(tokio::sync::RwLock::new(config)),
            http_client: reqwest::Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: Utc::now(),
            shutdown: tokio::sync::mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(tokio::sync::Notify::new()),
            log_wake: Arc::new(tokio::sync::Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[tokio::test]
    async fn status_reports_mission_counts_from_storage() {
        let state = test_state_with_config(config_with_aliases());

        let mut queued = Mission::new("Queued mission".to_string(), "Inspect repo".to_string());
        queued.status = MissionStatus::Queued;
        queued.updated_at = Utc::now();
        state.storage.upsert_mission(&queued).unwrap();

        let mut waiting = Mission::new(
            "Waiting mission".to_string(),
            "Wait for a timer".to_string(),
        );
        waiting.status = MissionStatus::Waiting;
        waiting.updated_at = queued.updated_at + chrono::Duration::seconds(1);
        state.storage.upsert_mission(&waiting).unwrap();

        let mut completed =
            Mission::new("Completed mission".to_string(), "Already done".to_string());
        completed.status = MissionStatus::Completed;
        completed.updated_at = waiting.updated_at + chrono::Duration::seconds(1);
        state.storage.upsert_mission(&completed).unwrap();

        let Json(response) = status(State(state)).await.unwrap();
        assert_eq!(response.missions, 3);
        assert_eq!(response.active_missions, 2);
    }

    #[tokio::test]
    async fn dashboard_bootstrap_returns_summary_state_and_recent_activity() {
        let mut config = config_with_aliases();
        config.trust_policy.allow_network = true;
        config.delegation.disabled_provider_ids = vec!["anthropic".to_string()];
        let main_alias = config
            .aliases
            .iter()
            .find(|alias| alias.alias == "main")
            .cloned()
            .unwrap();

        let state = test_state_with_config(config.clone());
        state
            .storage
            .ensure_session("session-1", &main_alias, "openai", "gpt-5.4", None)
            .unwrap();
        state
            .storage
            .append_log(&LogEntry {
                id: "event-1".to_string(),
                level: "info".to_string(),
                scope: "tests".to_string(),
                message: "dashboard bootstrap".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();

        let Json(response) = dashboard_bootstrap(State(state)).await.unwrap();
        assert_eq!(response.providers.len(), config.providers.len());
        assert_eq!(response.aliases.len(), config.aliases.len());
        assert_eq!(response.sessions.len(), 1);
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.permissions, config.permission_preset);
        assert!(response.trust.allow_network);
        assert_eq!(
            response.delegation_config.disabled_provider_ids,
            vec!["anthropic".to_string()]
        );
        assert_eq!(response.status.providers, config.providers.len());
        assert_eq!(response.status.aliases, config.aliases.len());
    }

    #[tokio::test]
    async fn rebuild_memory_route_creates_evidence_backed_memories_from_session_transcript() {
        let config = config_with_aliases();
        let main_alias = config
            .aliases
            .iter()
            .find(|alias| alias.alias == "main")
            .cloned()
            .unwrap();
        let state = test_state_with_config(config);

        state
            .storage
            .ensure_session(
                "session-1",
                &main_alias,
                "openai",
                "gpt-5.4",
                Some(TaskMode::Daily),
            )
            .unwrap();

        let user_preference = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "I prefer concise output.".to_string(),
            Some("openai".to_string()),
            Some("gpt-5.4".to_string()),
        );
        let user_project = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "Our project uses Rust and tokio.".to_string(),
            Some("openai".to_string()),
            Some("gpt-5.4".to_string()),
        );
        let tool_observation = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Tool,
            "Project uses Rust and tokio.".to_string(),
            Some("openai".to_string()),
            Some("gpt-5.4".to_string()),
        )
        .with_tool_metadata(Some("call-1".to_string()), Some("run_shell".to_string()));

        state.storage.append_message(&user_preference).unwrap();
        state.storage.append_message(&user_project).unwrap();
        state.storage.append_message(&tool_observation).unwrap();

        let Json(response) = rebuild_memory(
            State(state.clone()),
            Json(MemoryRebuildRequest {
                session_id: Some("session-1".to_string()),
                recompute_embeddings: false,
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.sessions_scanned, 1);
        assert!(response.observations_scanned >= 3);
        assert!(response.memories_upserted >= 2);

        let accepted = state.storage.list_memories(10).unwrap();
        let preference = accepted
            .iter()
            .find(|memory| matches!(memory.kind, MemoryKind::Preference))
            .unwrap();
        assert_eq!(preference.source_session_id.as_deref(), Some("session-1"));
        assert!(preference.evidence_refs.iter().any(|evidence| {
            evidence.message_id.as_deref() == Some(user_preference.id.as_str())
                && evidence.role == Some(MessageRole::User)
        }));

        let project = accepted
            .iter()
            .find(|memory| matches!(memory.kind, MemoryKind::ProjectFact))
            .unwrap();
        assert!(project.evidence_refs.iter().any(|evidence| {
            evidence.message_id.as_deref() == Some(user_project.id.as_str())
                && evidence.role == Some(MessageRole::User)
        }));
        assert!(project.evidence_refs.iter().any(|evidence| {
            evidence.tool_call_id.as_deref() == Some("call-1")
                && evidence.tool_name.as_deref() == Some("run_shell")
        }));
    }

    #[tokio::test]
    async fn session_resume_packet_includes_recent_messages_memories_and_related_hits() {
        let config = config_with_aliases();
        let main_alias = config
            .aliases
            .iter()
            .find(|alias| alias.alias == "main")
            .cloned()
            .unwrap();
        let state = test_state_with_config(config);

        state
            .storage
            .ensure_session(
                "session-1",
                &main_alias,
                "openai",
                "gpt-5.4",
                Some(TaskMode::Daily),
            )
            .unwrap();
        let session_one_user = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "Summarize the migration plan for the daemon.".to_string(),
            Some("openai".to_string()),
            Some("gpt-5.4".to_string()),
        );
        let session_one_assistant = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Assistant,
            "The migration plan covers storage, daemon, and UI work.".to_string(),
            Some("openai".to_string()),
            Some("gpt-5.4".to_string()),
        );
        state.storage.append_message(&session_one_user).unwrap();
        state
            .storage
            .append_message(&session_one_assistant)
            .unwrap();

        let mut linked_memory = MemoryRecord::new(
            MemoryKind::Workflow,
            agent_core::MemoryScope::Workspace,
            "workflow:migration-plan".to_string(),
            "Migration plan tracks storage, daemon, and UI follow-through.".to_string(),
        );
        linked_memory.source_session_id = Some("session-1".to_string());
        linked_memory.evidence_refs = vec![MemoryEvidenceRef {
            session_id: "session-1".to_string(),
            message_id: Some(session_one_user.id.clone()),
            role: Some(MessageRole::User),
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }];
        state.storage.upsert_memory(&linked_memory).unwrap();

        state
            .storage
            .ensure_session(
                "session-2",
                &main_alias,
                "openai",
                "gpt-5.4",
                Some(TaskMode::Daily),
            )
            .unwrap();
        state
            .storage
            .append_message(&SessionMessage::new(
                "session-2".to_string(),
                MessageRole::User,
                "Summarize the migration plan for the daemon rollout.".to_string(),
                Some("openai".to_string()),
                Some("gpt-5.4".to_string()),
            ))
            .unwrap();

        let Json(packet) = get_session_resume_packet(
            State(state.clone()),
            axum::extract::Path("session-1".to_string()),
        )
        .await
        .unwrap();

        assert_eq!(packet.session.id, "session-1");
        assert_eq!(packet.recent_messages.len(), 2);
        assert!(packet
            .linked_memories
            .iter()
            .any(|memory| memory.id == linked_memory.id));
        assert!(packet
            .related_transcript_hits
            .iter()
            .any(|hit| hit.session_id == "session-2"));
    }

    #[tokio::test]
    async fn status_includes_resolved_main_target_summary() {
        let state = test_state_with_config(config_with_aliases());

        let Json(response) = status(State(state)).await.unwrap();
        let main_target = response.main_target.expect("main target summary");
        assert_eq!(main_target.alias, "main");
        assert_eq!(main_target.provider_id, "openai");
        assert_eq!(main_target.provider_display_name, "openai");
        assert_eq!(main_target.model, "gpt-5.4");
    }

    #[tokio::test]
    async fn status_omits_unreadable_main_target_summary() {
        let config = AppConfig {
            trust_policy: TrustPolicy::default(),
            main_agent_alias: Some("main".to_string()),
            aliases: vec![alias("main", "openai", "gpt-5.4")],
            providers: vec![provider(
                "openai",
                AuthMode::ApiKey,
                Some("missing-provider-account"),
            )],
            ..AppConfig::default()
        };
        let state = test_state_with_config(config);

        let Json(response) = status(State(state)).await.unwrap();
        assert!(response.main_target.is_none());
    }

    #[tokio::test]
    async fn update_main_alias_changes_default_without_removing_other_aliases() {
        let state = test_state_with_config(config_with_aliases());

        let Json(summary) = update_main_alias(
            State(state.clone()),
            Json(MainAliasUpdateRequest {
                alias: "claude".to_string(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(summary.alias, "claude");
        assert_eq!(summary.provider_id, "anthropic");

        let saved = state.storage.load_config().unwrap();
        assert_eq!(saved.main_agent_alias.as_deref(), Some("claude"));
        assert!(saved.get_alias("main").is_some());
        assert!(saved.get_alias("claude").is_some());
        assert_eq!(saved.aliases.len(), 4);
    }

    #[tokio::test]
    async fn cancel_mission_clears_active_evolve_state() {
        let mut config = config_with_aliases();
        let mut mission = Mission::new("Evolve mission".to_string(), "Keep improving".to_string());
        mission.evolve = true;
        mission.status = MissionStatus::Running;
        config.evolve.state = EvolveState::Running;
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&mission).unwrap();

        let Json(cancelled) = cancel_mission(
            State(state.clone()),
            axum::extract::Path(mission.id.clone()),
        )
        .await
        .unwrap();

        assert_eq!(cancelled.status, MissionStatus::Cancelled);
        assert_eq!(
            cancelled.last_error.as_deref(),
            Some("Mission cancelled by operator")
        );

        let saved = state.config.read().await.clone();
        assert_eq!(saved.evolve.state, EvolveState::Completed);
        assert_eq!(saved.evolve.current_mission_id, None);
        assert_eq!(saved.autonomy.state, AutonomyState::Disabled);
        assert_eq!(saved.autonomy.mode, AutonomyMode::Assisted);
    }

    #[tokio::test]
    async fn start_evolve_mode_rejects_existing_active_mission() {
        let mut config = config_with_aliases();
        let mut mission = Mission::new("Existing evolve".to_string(), "Keep improving".to_string());
        mission.evolve = true;
        mission.status = MissionStatus::Queued;
        config.evolve.state = EvolveState::Running;
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&mission).unwrap();

        let error = start_evolve_mode(
            State(state),
            Json(EvolveStartRequest {
                alias: Some("main".to_string()),
                requested_model: None,
                budget_friendly: None,
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(error.message.contains("active mission"));
    }

    #[tokio::test]
    async fn pause_mission_updates_active_evolve_state() {
        let mut config = config_with_aliases();
        let mut mission = Mission::new("Evolve mission".to_string(), "Keep improving".to_string());
        mission.evolve = true;
        mission.status = MissionStatus::Running;
        mission.alias = Some("main".to_string());
        config.evolve.state = EvolveState::Running;
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&mission).unwrap();

        let Json(paused) = pause_mission(
            State(state.clone()),
            axum::extract::Path(mission.id.clone()),
            Json(MissionControlRequest {
                wake_at: None,
                clear_wake_at: false,
                repeat_interval_seconds: None,
                clear_repeat_interval_seconds: false,
                watch_path: None,
                clear_watch_path: false,
                watch_recursive: None,
                clear_session_id: false,
                clear_handoff_summary: false,
                note: Some("Pause it".to_string()),
            }),
        )
        .await
        .unwrap();

        assert_eq!(paused.status, MissionStatus::Blocked);
        let saved = state.config.read().await.clone();
        assert_eq!(saved.evolve.state, EvolveState::Paused);
        assert_eq!(
            saved.evolve.current_mission_id.as_deref(),
            Some(mission.id.as_str())
        );
        assert_eq!(saved.autonomy.state, AutonomyState::Paused);
        assert_eq!(saved.autonomy.mode, AutonomyMode::Evolve);
    }

    #[tokio::test]
    async fn pause_mission_signals_in_flight_cancellation() {
        let mut config = config_with_aliases();
        let mut mission = Mission::new("Pauseable mission".to_string(), "Pause me".to_string());
        mission.status = MissionStatus::Running;
        config.main_agent_alias = Some("main".to_string());

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&mission).unwrap();
        let cancellation = ExecutionCancellation::default();
        state
            .mission_cancellations
            .lock()
            .unwrap()
            .insert(mission.id.clone(), cancellation.clone());

        let Json(paused) = pause_mission(
            State(state.clone()),
            axum::extract::Path(mission.id.clone()),
            Json(MissionControlRequest {
                wake_at: None,
                clear_wake_at: false,
                repeat_interval_seconds: None,
                clear_repeat_interval_seconds: false,
                watch_path: None,
                clear_watch_path: false,
                watch_recursive: None,
                clear_session_id: false,
                clear_handoff_summary: false,
                note: Some("Pause it".to_string()),
            }),
        )
        .await
        .unwrap();

        assert_eq!(paused.status, MissionStatus::Blocked);
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn dashboard_bootstrap_redacts_provider_keychain_metadata() {
        let state = test_state_with_config(config_with_aliases());

        let Json(bootstrap) = dashboard_bootstrap(State(state)).await.unwrap();

        assert!(bootstrap
            .providers
            .iter()
            .all(|provider| provider.keychain_account.is_none()));
    }

    #[tokio::test]
    async fn upsert_provider_preserves_existing_saved_credentials_without_new_secret() {
        let mut config = config_with_aliases();
        config.providers.push(provider(
            "custom",
            AuthMode::ApiKey,
            Some("custom-keychain-account"),
        ));

        let state = test_state_with_config(config);

        let Json(saved) = upsert_provider(
            State(state.clone()),
            Json(ProviderUpsertRequest {
                provider: ProviderConfig {
                    id: "custom".to_string(),
                    display_name: "Custom".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: "https://example.test".to_string(),
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("custom-model".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
                api_key: None,
                oauth_token: None,
            }),
        )
        .await
        .unwrap();

        assert!(saved.keychain_account.is_none());
        let config = state.config.read().await;
        assert_eq!(
            config
                .get_provider("custom")
                .and_then(|provider| provider.keychain_account.as_deref()),
            Some("custom-keychain-account")
        );
    }

    #[tokio::test]
    async fn resume_mission_updates_active_evolve_state() {
        let mut config = config_with_aliases();
        let mut mission = Mission::new("Evolve mission".to_string(), "Keep improving".to_string());
        mission.evolve = true;
        mission.status = MissionStatus::Blocked;
        mission.alias = Some("main".to_string());
        config.evolve.state = EvolveState::Paused;
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.autonomy.state = AutonomyState::Paused;
        config.autonomy.mode = AutonomyMode::Evolve;

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&mission).unwrap();

        let Json(resumed) = resume_mission(
            State(state.clone()),
            axum::extract::Path(mission.id.clone()),
            Json(MissionControlRequest {
                wake_at: None,
                clear_wake_at: false,
                repeat_interval_seconds: None,
                clear_repeat_interval_seconds: false,
                watch_path: None,
                clear_watch_path: false,
                watch_recursive: None,
                clear_session_id: false,
                clear_handoff_summary: false,
                note: Some("Resume it".to_string()),
            }),
        )
        .await
        .unwrap();

        assert!(matches!(resumed.status, MissionStatus::Queued));
        let saved = state.config.read().await.clone();
        assert_eq!(saved.evolve.state, EvolveState::Running);
        assert_eq!(
            saved.evolve.current_mission_id.as_deref(),
            Some(mission.id.as_str())
        );
        assert_eq!(saved.autonomy.state, AutonomyState::Enabled);
        assert_eq!(saved.autonomy.mode, AutonomyMode::Evolve);
    }

    #[tokio::test]
    async fn resume_mission_rejects_when_another_evolve_mission_is_active() {
        let mut config = config_with_aliases();
        let mut active = Mission::new("Active evolve".to_string(), "Keep improving".to_string());
        active.evolve = true;
        active.status = MissionStatus::Running;
        active.alias = Some("main".to_string());

        let mut paused = Mission::new("Paused evolve".to_string(), "Old run".to_string());
        paused.evolve = true;
        paused.status = MissionStatus::Blocked;
        paused.alias = Some("main".to_string());

        config.evolve.state = EvolveState::Running;
        config.evolve.current_mission_id = Some(active.id.clone());
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;

        let state = test_state_with_config(config);
        state.storage.upsert_mission(&active).unwrap();
        state.storage.upsert_mission(&paused).unwrap();

        let error = resume_mission(
            State(state),
            axum::extract::Path(paused.id.clone()),
            Json(MissionControlRequest {
                wake_at: None,
                clear_wake_at: false,
                repeat_interval_seconds: None,
                clear_repeat_interval_seconds: false,
                watch_path: None,
                clear_watch_path: false,
                watch_recursive: None,
                clear_session_id: false,
                clear_handoff_summary: false,
                note: Some("Resume it".to_string()),
            }),
        )
        .await
        .unwrap_err();

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(error.message.contains("already active"));
    }

    #[test]
    fn resolve_alias_and_provider_uses_main_alias_and_requires_credentials() {
        let config = config_with_aliases();
        let (alias, provider) = resolve_alias_and_provider_from_config(&config, None).unwrap();
        assert_eq!(alias.alias, "main");
        assert_eq!(provider.id, "openai");

        let mut missing = config;
        missing.providers[0].auth_mode = AuthMode::ApiKey;
        missing.providers[0].keychain_account = None;
        let error = resolve_alias_and_provider_from_config(&missing, None).unwrap_err();
        assert!(error.message.contains("usable saved credentials"));
    }

    #[test]
    fn provider_pool_prefers_parent_alias_and_dedupes_by_provider() {
        let config = config_with_aliases();
        let pool = provider_pool_candidates(&config, Some("claude")).unwrap();
        let aliases = pool
            .into_iter()
            .map(|(alias, _)| alias.alias)
            .collect::<Vec<_>>();
        assert_eq!(aliases, vec!["claude", "main", "local"]);
    }

    #[test]
    fn provider_pool_uses_local_provider_without_keychain() {
        let config = config_with_aliases();
        let pool = provider_pool_candidates(&config, None).unwrap();
        assert!(pool.iter().any(|(_, provider)| provider.id == "ollama"));
    }

    #[test]
    fn resolve_delegation_tasks_expands_parallel_requests_across_provider_pool() {
        let config = config_with_aliases();
        let request = BatchTaskRequest {
            tasks: vec![SubAgentTask {
                prompt: "Summarize this repo".to_string(),
                target: None,
                alias: None,
                provider_id: None,
                requested_model: None,
                cwd: None,
                thinking_level: None,
                task_mode: None,
                output_schema_json: None,
                strategy: Some(SubAgentStrategy::ParallelBestEffort),
            }],
            cwd: Some(PathBuf::from("J:\\repo")),
            thinking_level: Some(ThinkingLevel::Medium),
            task_mode: None,
            strategy: None,
            parent_alias: Some("claude".to_string()),
        };

        let resolved = resolve_delegation_tasks(&config, &request).unwrap();
        let aliases = resolved
            .iter()
            .map(|task| task.alias.alias.clone())
            .collect::<Vec<_>>();
        assert_eq!(aliases, vec!["claude", "main", "local"]);
        assert!(resolved
            .iter()
            .all(|task| task.cwd.as_ref() == Some(&PathBuf::from("J:\\repo"))));
        assert!(resolved
            .iter()
            .all(|task| task.thinking_level == Some(ThinkingLevel::Medium)));
    }

    #[test]
    fn resolve_delegation_tasks_inherits_batch_task_mode() {
        let config = config_with_aliases();
        let request = BatchTaskRequest {
            tasks: vec![SubAgentTask {
                prompt: "Plan the next sprint".to_string(),
                target: Some("claude".to_string()),
                alias: None,
                provider_id: None,
                requested_model: None,
                cwd: None,
                thinking_level: None,
                task_mode: None,
                output_schema_json: None,
                strategy: None,
            }],
            cwd: None,
            thinking_level: None,
            task_mode: Some(TaskMode::Daily),
            strategy: None,
            parent_alias: Some("main".to_string()),
        };

        let resolved = resolve_delegation_tasks(&config, &request).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].task_mode, Some(TaskMode::Daily));
    }

    #[test]
    fn resolve_subagent_candidates_can_target_provider_without_hardcoding_alias() {
        let config = config_with_aliases();
        let task = SubAgentTask {
            prompt: "Review this patch".to_string(),
            target: None,
            alias: None,
            provider_id: Some("anthropic".to_string()),
            requested_model: None,
            cwd: None,
            thinking_level: None,
            task_mode: None,
            output_schema_json: None,
            strategy: None,
        };

        let resolved = resolve_subagent_candidates(&config, Some("main"), &task).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.alias, "claude");
        assert_eq!(resolved[0].1.id, "anthropic");
    }

    #[test]
    fn resolve_subagent_candidates_supports_friendly_target_names() {
        let config = config_with_aliases();
        let task = SubAgentTask {
            prompt: "Review this patch".to_string(),
            target: Some("claude".to_string()),
            alias: None,
            provider_id: None,
            requested_model: None,
            cwd: None,
            thinking_level: None,
            task_mode: None,
            output_schema_json: None,
            strategy: Some(SubAgentStrategy::SingleBest),
        };

        let resolved = resolve_subagent_candidates(&config, Some("main"), &task).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.alias, "claude");
        assert_eq!(resolved[0].1.id, "anthropic");
    }

    #[test]
    fn resolve_subagent_candidates_supports_model_and_host_derived_targets() {
        let mut config = config_with_aliases();
        config.providers.push(ProviderConfig {
            id: "moonshot".to_string(),
            display_name: "Moonshot Hosted".to_string(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: "https://api.moonshot.ai/v1".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("kimi-k2".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        });
        config.aliases.push(alias("kimi", "moonshot", "kimi-k2"));

        let by_model = SubAgentTask {
            prompt: "Summarize this repo".to_string(),
            target: Some("k2".to_string()),
            alias: None,
            provider_id: None,
            requested_model: None,
            cwd: None,
            thinking_level: None,
            task_mode: None,
            output_schema_json: None,
            strategy: Some(SubAgentStrategy::SingleBest),
        };
        let resolved_by_model =
            resolve_subagent_candidates(&config, Some("main"), &by_model).unwrap();
        assert_eq!(resolved_by_model.len(), 1);
        assert_eq!(resolved_by_model[0].0.alias, "kimi");
        assert_eq!(resolved_by_model[0].1.id, "moonshot");

        let by_host = SubAgentTask {
            prompt: "Summarize this repo".to_string(),
            target: Some("moonshot".to_string()),
            alias: None,
            provider_id: None,
            requested_model: None,
            cwd: None,
            thinking_level: None,
            task_mode: None,
            output_schema_json: None,
            strategy: Some(SubAgentStrategy::SingleBest),
        };
        let resolved_by_host =
            resolve_subagent_candidates(&config, Some("main"), &by_host).unwrap();
        assert_eq!(resolved_by_host.len(), 1);
        assert_eq!(resolved_by_host[0].0.alias, "kimi");
        assert_eq!(resolved_by_host[0].1.id, "moonshot");
    }

    #[test]
    fn provider_pool_excludes_delegation_disabled_providers() {
        let mut config = config_with_aliases();
        config.delegation.disabled_provider_ids = vec!["anthropic".to_string()];
        let pool = provider_pool_candidates(&config, Some("main")).unwrap();
        let aliases = pool
            .into_iter()
            .map(|(alias, _)| alias.alias)
            .collect::<Vec<_>>();
        assert_eq!(aliases, vec!["main", "local"]);
    }

    #[tokio::test]
    async fn suggest_provider_defaults_avoids_logged_in_provider_collisions() {
        let state = test_state_with_config(config_with_aliases());
        let response = suggest_provider_defaults(
            State(state),
            Json(ProviderSuggestionRequest {
                preferred_provider_id: "anthropic".to_string(),
                preferred_alias_name: None,
                default_model: Some("claude-sonnet-4-20250514".to_string()),
                editing_provider_id: None,
                editing_alias_name: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response.provider_id, "anthropic-2");
        assert_eq!(
            response.alias_name.as_deref(),
            Some("anthropic-2-claude-sonnet-4")
        );
        assert_eq!(
            response.alias_model.as_deref(),
            Some("claude-sonnet-4-20250514")
        );
        assert!(!response.would_be_first_main);
    }

    #[tokio::test]
    async fn suggest_provider_defaults_preserves_existing_ids_in_edit_mode() {
        let state = test_state_with_config(config_with_aliases());
        let response = suggest_provider_defaults(
            State(state),
            Json(ProviderSuggestionRequest {
                preferred_provider_id: "anthropic".to_string(),
                preferred_alias_name: Some("claude".to_string()),
                default_model: Some("claude-sonnet".to_string()),
                editing_provider_id: Some("anthropic".to_string()),
                editing_alias_name: Some("claude".to_string()),
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response.provider_id, "anthropic");
        assert_eq!(response.alias_name.as_deref(), Some("claude"));
        assert_eq!(response.alias_model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn provider_pool_includes_secondary_logged_in_provider_for_parallel_spawns() {
        let mut config = config_with_aliases();
        config.providers.push(ProviderConfig {
            id: "anthropic-2".to_string(),
            display_name: "Anthropic Backup".to_string(),
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".to_string(),
            auth_mode: AuthMode::None,
            default_model: Some("claude-opus-4-1".to_string()),
            keychain_account: None,
            oauth: None,
            local: false,
        });
        config
            .aliases
            .push(alias("claude-backup", "anthropic-2", "claude-opus-4-1"));

        let pool = provider_pool_candidates(&config, Some("main")).unwrap();
        assert!(pool.iter().any(|(_, provider)| provider.id == "anthropic"));
        assert!(pool
            .iter()
            .any(|(_, provider)| provider.id == "anthropic-2"));

        let resolved = resolve_subagent_candidates(
            &config,
            Some("main"),
            &SubAgentTask {
                prompt: "Review this patch".to_string(),
                target: None,
                alias: None,
                provider_id: Some("anthropic-2".to_string()),
                requested_model: None,
                cwd: None,
                thinking_level: None,
                task_mode: None,
                output_schema_json: None,
                strategy: None,
            },
        )
        .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0.alias, "claude-backup");
        assert_eq!(resolved[0].1.id, "anthropic-2");
    }

    #[test]
    fn execute_batch_request_rejects_depth_beyond_limit() {
        let config = config_with_aliases();
        let state = test_state_with_config(config);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(execute_batch_request(
                &state,
                BatchTaskRequest {
                    tasks: vec![SubAgentTask {
                        prompt: "Review this patch".to_string(),
                        target: Some("claude".to_string()),
                        alias: None,
                        provider_id: None,
                        requested_model: None,
                        cwd: None,
                        thinking_level: None,
                        task_mode: None,
                        output_schema_json: None,
                        strategy: None,
                    }],
                    cwd: None,
                    thinking_level: None,
                    task_mode: None,
                    strategy: None,
                    parent_alias: Some("main".to_string()),
                },
                DelegationExecutionOptions {
                    background: false,
                    permission_preset: None,
                    delegation_depth: 1,
                },
            ))
            .unwrap_err();
        assert!(error.message.contains("delegation depth limit exceeded"));
    }

    #[test]
    fn execute_batch_request_rejects_parallel_runs_beyond_limit() {
        let mut config = config_with_aliases();
        config.delegation.max_parallel_subagents = DelegationLimit::Limited { value: 2 };
        let state = test_state_with_config(config);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime
            .block_on(execute_batch_request(
                &state,
                BatchTaskRequest {
                    tasks: vec![SubAgentTask {
                        prompt: "Review this patch".to_string(),
                        target: None,
                        alias: None,
                        provider_id: None,
                        requested_model: None,
                        cwd: None,
                        thinking_level: None,
                        task_mode: None,
                        output_schema_json: None,
                        strategy: Some(SubAgentStrategy::ParallelBestEffort),
                    }],
                    cwd: None,
                    thinking_level: None,
                    task_mode: None,
                    strategy: None,
                    parent_alias: Some("main".to_string()),
                },
                DelegationExecutionOptions {
                    background: false,
                    permission_preset: None,
                    delegation_depth: 0,
                },
            ))
            .unwrap_err();
        assert!(error.message.contains("parallel subagent limit exceeded"));
    }

    #[test]
    fn parse_mission_directive_accepts_structured_json() {
        let directive = parse_mission_directive(
            r#"{"status":"waiting","next_wake_seconds":90,"next_phase":"executor","handoff_summary":"Carry the repo status forward","summary":"Checked repo status","error":""}"#,
        );
        assert_eq!(directive.status, Some(MissionStatus::Waiting));
        assert_eq!(directive.next_wake_seconds, Some(90));
        assert_eq!(
            directive.next_phase,
            Some(agent_core::MissionPhase::Executor)
        );
        assert_eq!(
            directive.handoff_summary.as_deref(),
            Some("Carry the repo status forward")
        );
        assert_eq!(directive.summary.as_deref(), Some("Checked repo status"));
        assert_eq!(directive.error, None);
    }

    #[test]
    fn parse_mission_directive_falls_back_to_legacy_block() {
        let directive = parse_mission_directive(
            "Worked a step.\n[AUTOPILOT]\nstatus: blocked\nnext_phase: reviewer\nhandoff_summary: Review the last failed attempt\nsummary: Missing token\nerror: Need a token\n[/AUTOPILOT]",
        );
        assert_eq!(directive.status, Some(MissionStatus::Blocked));
        assert_eq!(
            directive.next_phase,
            Some(agent_core::MissionPhase::Reviewer)
        );
        assert_eq!(
            directive.handoff_summary.as_deref(),
            Some("Review the last failed attempt")
        );
        assert_eq!(directive.summary.as_deref(), Some("Missing token"));
        assert_eq!(directive.error.as_deref(), Some("Need a token"));
    }

    #[test]
    fn structured_output_validation_enforces_required_fields() {
        let schema = r#"{
          "type": "object",
          "properties": {
            "name": { "type": "string" },
            "count": { "type": "integer", "minimum": 1 }
          },
          "required": ["name", "count"],
          "additionalProperties": false
        }"#;

        let error =
            maybe_validate_structured_output(r#"{"name":"repo"}"#, Some(schema)).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert!(error.message.contains("count"));
    }

    #[test]
    fn structured_output_validation_enforces_enum_values() {
        let schema = r#"{
          "type": "object",
          "properties": {
            "status": { "type": "string", "enum": ["queued", "completed"] }
          },
          "required": ["status"],
          "additionalProperties": false
        }"#;

        let error =
            maybe_validate_structured_output(r#"{"status":"waiting"}"#, Some(schema)).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_GATEWAY);
        assert!(error.message.contains("enum") || error.message.contains("queued"));
    }

    #[test]
    fn file_change_ready_primes_then_detects_changes() {
        let root = std::env::temp_dir().join(format!("agent-watch-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let watched = root.join("notes.txt");
        std::fs::write(&watched, "alpha").unwrap();

        let state = test_state_with_config(config_with_aliases());
        let mut mission = Mission::new("Watch files".to_string(), String::new());
        mission.status = MissionStatus::Waiting;
        mission.wake_trigger = Some(WakeTrigger::FileChange);
        mission.workspace_key = Some(root.display().to_string());
        mission.watch_path = Some(PathBuf::from("notes.txt"));
        mission.watch_recursive = false;

        assert!(!file_change_ready(&state, &mut mission).unwrap());
        assert!(mission.watch_fingerprint.is_some());

        std::fs::write(&watched, "beta").unwrap();
        assert!(file_change_ready(&state, &mut mission).unwrap());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_mission_prompt_mentions_watch_path() {
        let mut mission = Mission::new("Watch files".to_string(), "Summarize changes".to_string());
        mission.watch_path = Some(PathBuf::from("src"));
        mission.watch_recursive = true;
        let prompt = build_mission_prompt(&mission, &[]);
        assert!(prompt.contains("filesystem watch"));
        assert!(prompt.contains("Watched path: src"));
    }

    #[test]
    fn repeated_safe_tool_batch_can_short_circuit_to_success() {
        let batch = vec![ToolBatchExecution {
            name: "write_file".to_string(),
            arguments: r#"{"path":"C:\\Users\\me\\Desktop\\test","content":""}"#.to_string(),
            outcome: "success",
            output: "wrote 0 bytes to C:\\Users\\me\\Desktop\\test".to_string(),
        }];

        let resolution = repeated_tool_loop_resolution(&batch, None);
        match resolution {
            ToolLoopResolution::Success(message) => {
                assert!(message.contains("Completed the requested filesystem change"));
                assert!(message.contains("write_file"));
            }
            ToolLoopResolution::Error(message) => {
                panic!("expected success, got error: {message}");
            }
        }
    }

    #[test]
    fn repeated_non_mutating_tool_batch_reports_diagnostic_error() {
        let batch = vec![ToolBatchExecution {
            name: "read_file".to_string(),
            arguments: r#"{"path":"README.md"}"#.to_string(),
            outcome: "success",
            output: "contents".to_string(),
        }];

        let resolution = repeated_tool_loop_resolution(&batch, None);
        match resolution {
            ToolLoopResolution::Success(message) => {
                panic!("expected error, got success: {message}");
            }
            ToolLoopResolution::Error(message) => {
                assert!(message.contains("read_file"));
                assert!(message.contains("without making progress"));
            }
        }
    }
}
