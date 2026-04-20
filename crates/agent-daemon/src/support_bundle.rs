use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use agent_core::{
    redact_sensitive_json_value, resolve_operator_path, resolve_relative_path_within_root,
    validate_relative_path, validate_single_path_component, HealthReport,
};
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
    let bundle_dir = resolve_support_bundle_dir(
        &state.storage.paths().data_dir,
        generated_at,
        payload.output_dir.as_deref(),
    )?;

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

    write_json_file(&bundle_file_path(&bundle_dir, "doctor.json")?, &doctor)?;
    write_json_file(
        &bundle_file_path(&bundle_dir, "daemon-status.json")?,
        &daemon_status,
    )?;
    write_json_file(
        &bundle_file_path(&bundle_dir, "config-summary.json")?,
        &config_summary,
    )?;
    write_json_file(&bundle_file_path(&bundle_dir, "sessions.json")?, &sessions)?;
    write_json_file(&bundle_file_path(&bundle_dir, "logs.json")?, &logs)?;
    write_json_file(&bundle_file_path(&bundle_dir, "manifest.json")?, &manifest)?;
    if let Some(value) = install_state.as_ref() {
        write_json_file(&bundle_file_path(&bundle_dir, "install-state.json")?, value)?;
        files.push("install-state.json".to_string());
    }
    if let Some(value) = migration_state.as_ref() {
        write_json_file(
            &bundle_file_path(&bundle_dir, "path-migration.json")?,
            value,
        )?;
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
    fs::write(bundle_file_path(&bundle_dir, "README.md")?, readme).map_err(internal_error)?;

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
    let value = serde_json::to_value(value).map_err(internal_error)?;
    let content = serde_json::to_string_pretty(&redact_sensitive_json_value(&value))
        .map_err(internal_error)?;
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

fn resolve_support_bundle_dir(
    data_dir: &Path,
    generated_at: chrono::DateTime<Utc>,
    requested: Option<&Path>,
) -> Result<PathBuf, ApiError> {
    match requested {
        Some(path) => resolve_requested_support_bundle_dir(path),
        None => resolve_relative_path_within_root(
            data_dir,
            &PathBuf::from("support-bundles")
                .join(generated_at.format("%Y%m%d-%H%M%S").to_string()),
            "support bundle output directory",
        )
        .map_err(internal_error),
    }
}

fn resolve_requested_support_bundle_dir(path: &Path) -> Result<PathBuf, ApiError> {
    reject_parent_components(path, "support bundle output directory")?;
    if path.is_absolute() {
        resolve_operator_path(path, "support bundle output directory")
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
    } else {
        let relative = validate_relative_path(path, "support bundle output directory")
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))?;
        let cwd = std::env::current_dir().map_err(internal_error)?;
        resolve_operator_path(&cwd.join(relative), "support bundle output directory")
            .map_err(|error| ApiError::new(StatusCode::BAD_REQUEST, error.to_string()))
    }
}

fn reject_parent_components(path: &Path, label: &str) -> Result<(), ApiError> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("{label} must not contain traversal segments"),
        ));
    }
    Ok(())
}

fn bundle_file_path(bundle_dir: &Path, file_name: &str) -> Result<PathBuf, ApiError> {
    let file_name = validate_single_path_component(file_name, "support bundle file name")
        .map_err(internal_error)?;
    resolve_relative_path_within_root(
        bundle_dir,
        Path::new(&file_name),
        "support bundle file path",
    )
    .map_err(internal_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolve_support_bundle_dir_accepts_default_managed_output() {
        let data_dir = temp_dir("support-bundle-data");
        let generated_at = Utc::now();
        let normalized_data_dir =
            resolve_operator_path(&data_dir, "support bundle data directory").unwrap();

        let bundle_dir = resolve_support_bundle_dir(&data_dir, generated_at, None).unwrap();

        assert!(bundle_dir.starts_with(&normalized_data_dir));
        assert!(bundle_dir.ends_with(generated_at.format("%Y%m%d-%H%M%S").to_string()));
    }

    #[test]
    fn resolve_support_bundle_dir_accepts_valid_operator_output_path() {
        let data_dir = temp_dir("support-bundle-data");
        let export_root = temp_dir("support-bundle-export");
        let requested = export_root.join("nested").join("bundle");
        let normalized_requested =
            resolve_operator_path(&requested, "support bundle output directory").unwrap();

        let bundle_dir =
            resolve_support_bundle_dir(&data_dir, Utc::now(), Some(requested.as_path())).unwrap();

        assert_eq!(bundle_dir, normalized_requested);
    }

    #[test]
    fn resolve_support_bundle_dir_rejects_traversal_output_path() {
        let data_dir = temp_dir("support-bundle-data");
        let error = resolve_support_bundle_dir(
            &data_dir,
            Utc::now(),
            Some(Path::new("..").join("escape").as_path()),
        )
        .unwrap_err();

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.message.contains("traversal"));
    }

    #[test]
    fn write_json_file_redacts_sensitive_fields() {
        let dir = temp_dir("support-bundle-json");
        let path = dir.join("artifact.json");

        write_json_file(
            &path,
            &json!({
                "access_token": "access-secret",
                "nested": {
                    "refresh_token": "refresh-secret"
                }
            }),
        )
        .unwrap();

        let content = fs::read_to_string(path).unwrap();
        assert!(!content.contains("access-secret"));
        assert!(!content.contains("refresh-secret"));
        assert!(content.contains("[REDACTED]"));
    }
}
