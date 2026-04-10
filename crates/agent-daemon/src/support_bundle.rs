use std::{
    fs,
    path::{Path, PathBuf},
};

use agent_core::HealthReport;
use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use futures::future::join_all;
use serde::{Deserialize, Serialize};

use crate::{
    append_log, collect_plugin_doctor_reports, control::build_daemon_status, ApiError, AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SupportBundleRequest {
    #[serde(default)]
    pub(crate) output_dir: Option<PathBuf>,
    #[serde(default)]
    pub(crate) log_limit: Option<usize>,
    #[serde(default)]
    pub(crate) session_limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SupportBundleResponse {
    pub(crate) bundle_dir: String,
    pub(crate) generated_at: chrono::DateTime<Utc>,
    pub(crate) files: Vec<String>,
}

pub(crate) async fn create_support_bundle(
    State(state): State<AppState>,
    Json(payload): Json<SupportBundleRequest>,
) -> Result<Json<SupportBundleResponse>, ApiError> {
    let generated_at = Utc::now();
    let log_limit = payload.log_limit.unwrap_or(200).clamp(1, 2000);
    let session_limit = payload.session_limit.unwrap_or(25).clamp(1, 500);
    let bundle_dir = payload.output_dir.unwrap_or_else(|| {
        state
            .storage
            .paths()
            .data_dir
            .join("support-bundles")
            .join(generated_at.format("%Y%m%d-%H%M%S").to_string())
    });

    fs::create_dir_all(&bundle_dir).map_err(internal_error)?;

    let config = state.config.read().await.clone();
    let doctor = gather_health_report(&state, &config).await?;
    let daemon_status = build_daemon_status(&state, &config)?;
    let logs = state.storage.list_logs(log_limit)?;
    let sessions = state.storage.list_sessions(session_limit)?;
    let install_state = load_optional_json_file(
        &std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.parent()
                    .map(|parent| parent.join("install-state.json"))
            })
            .unwrap_or_else(|| PathBuf::from("install-state.json")),
    )?;
    let migration_state = load_optional_json_file(&state.storage.paths().migration_path)?;
    let config_summary = serde_json::json!({
        "version": config.version,
        "onboarding_complete": config.onboarding_complete,
        "config_path": state.storage.paths().config_path.display().to_string(),
        "data_path": state.storage.paths().data_dir.display().to_string(),
        "main_agent_alias": config.main_agent_alias,
        "providers": config.providers.iter().map(|provider| serde_json::json!({
            "id": provider.id,
            "kind": provider.kind,
            "display_name": provider.display_name,
            "base_url": provider.base_url,
            "auth_mode": provider.auth_mode,
        })).collect::<Vec<_>>(),
        "aliases": config.aliases,
        "plugin_count": config.plugins.len(),
        "webhook_connectors": config.webhook_connectors.len(),
        "inbox_connectors": config.inbox_connectors.len(),
        "telegram_connectors": config.telegram_connectors.len(),
        "discord_connectors": config.discord_connectors.len(),
        "slack_connectors": config.slack_connectors.len(),
        "home_assistant_connectors": config.home_assistant_connectors.len(),
        "signal_connectors": config.signal_connectors.len(),
        "gmail_connectors": config.gmail_connectors.len(),
        "brave_connectors": config.brave_connectors.len(),
        "enabled_skills": config.enabled_skills,
        "permission_preset": config.permission_preset,
        "trust_policy": config.trust_policy,
        "autonomy": config.autonomy,
        "autopilot": config.autopilot,
        "delegation": config.delegation,
        "embedding": config.embedding,
    });
    let manifest = serde_json::json!({
        "generated_at": generated_at,
        "bundle_dir": bundle_dir.display().to_string(),
        "doctor_file": "doctor.json",
        "daemon_status_file": "daemon-status.json",
        "config_summary_file": "config-summary.json",
        "sessions_file": "sessions.json",
        "logs_file": "logs.json",
        "install_state_file": install_state.as_ref().map(|_| "install-state.json"),
        "path_migration_file": migration_state.as_ref().map(|_| "path-migration.json"),
    });

    let mut files = vec![
        "manifest.json".to_string(),
        "doctor.json".to_string(),
        "daemon-status.json".to_string(),
        "config-summary.json".to_string(),
        "sessions.json".to_string(),
        "logs.json".to_string(),
        "README.md".to_string(),
    ];

    write_json_file(&bundle_dir.join("doctor.json"), &doctor)?;
    write_json_file(&bundle_dir.join("daemon-status.json"), &daemon_status)?;
    write_json_file(&bundle_dir.join("config-summary.json"), &config_summary)?;
    write_json_file(&bundle_dir.join("sessions.json"), &sessions)?;
    write_json_file(&bundle_dir.join("logs.json"), &logs)?;
    write_json_file(&bundle_dir.join("manifest.json"), &manifest)?;
    if let Some(value) = install_state.as_ref() {
        write_json_file(&bundle_dir.join("install-state.json"), value)?;
        files.push("install-state.json".to_string());
    }
    if let Some(value) = migration_state.as_ref() {
        write_json_file(&bundle_dir.join("path-migration.json"), value)?;
        files.push("path-migration.json".to_string());
    }

    let readme = [
        "# Nuclear Agent Support Bundle".to_string(),
        String::new(),
        format!("- generated_at: `{}`", generated_at.to_rfc3339()),
        format!("- bundle_dir: `{}`", bundle_dir.display()),
        format!("- daemon_running: `{}`", doctor.daemon_running),
        format!("- config_path: `{}`", doctor.config_path),
        format!("- data_path: `{}`", doctor.data_path),
        format!("- logs: `{}`", log_limit),
        format!("- sessions: `{}`", session_limit),
        String::new(),
        "Files:".to_string(),
        files
            .iter()
            .map(|file| format!("- `{file}`"))
            .collect::<Vec<_>>()
            .join("\n"),
    ]
    .join("\n");
    fs::write(bundle_dir.join("README.md"), readme).map_err(internal_error)?;

    append_log(
        &state,
        "info",
        "support-bundle",
        format!("support bundle created at {}", bundle_dir.display()),
    )?;

    Ok(Json(SupportBundleResponse {
        bundle_dir: bundle_dir.display().to_string(),
        generated_at,
        files,
    }))
}

async fn gather_health_report(
    state: &AppState,
    config: &agent_core::AppConfig,
) -> Result<HealthReport, ApiError> {
    let providers = join_all(
        config
            .all_providers()
            .iter()
            .map(|provider| crate::plugins::provider_health(state, provider)),
    )
    .await;
    Ok(HealthReport {
        daemon_running: true,
        config_path: state.storage.paths().config_path.display().to_string(),
        data_path: state.storage.paths().data_dir.display().to_string(),
        keyring_ok: agent_providers::keyring_available(),
        providers,
        plugins: collect_plugin_doctor_reports(config),
        remote_content_policy: config.remote_content_policy,
        provider_capabilities: crate::control::provider_capability_summaries(config),
    })
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<(), ApiError> {
    let content = serde_json::to_string_pretty(value).map_err(internal_error)?;
    fs::write(path, content).map_err(internal_error)
}

fn load_optional_json_file(path: &Path) -> Result<Option<serde_json::Value>, ApiError> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(internal_error)?;
    let value = serde_json::from_str(&content).map_err(internal_error)?;
    Ok(Some(value))
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
