#[cfg(windows)]
use std::os::windows::process::CommandExt as _;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};

use agent_core::{
    redact_sensitive_json_value, redact_sensitive_text, resolve_path_from_existing_parent,
    resolve_path_within_root, resolve_relative_path_within_root, validate_relative_path,
    validate_single_path_component, UpdateAvailabilityState, UpdateInstallKind,
    UpdateInstallTarget, UpdateOperationStep, UpdateRunRequest, UpdateRunState, UpdateRunSummary,
    UpdateStatusResponse, INTERNAL_DAEMON_ARG, INTERNAL_UPDATE_HELPER_ARG,
};
use agent_storage::Storage;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::USER_AGENT;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    process::Command,
    time::{sleep, Instant},
};
use uuid::Uuid;

use crate::{append_log, ApiError, AppState};

const UPDATE_STATE_SCHEMA_VERSION: u32 = 1;
const UPDATE_RELEASES_URL: &str =
    "https://api.github.com/repos/HoodedBandit/nuclear-agent/releases/latest";
const UPDATE_STATE_FILE_NAME: &str = "update-state.json";
const UPDATE_STAGING_DIR_NAME: &str = "updates";
const UPDATE_PLAN_FILE_NAME: &str = "plan.json";
const UPDATE_HELPER_BINARY_BASENAME: &str = "nuclear-update-helper";
const UPDATE_HELPER_SHUTDOWN_DELAY_MS: u64 = 350;
const UPDATE_HELPER_WAIT_TIMEOUT_SECS: u64 = 180;
const UPDATE_HELPER_WAIT_POLL_MS: u64 = 250;
const RELEASE_ONLY_UPDATE_MESSAGE: &str =
    "Remote updates are available only for packaged installs with a published GitHub Release bundle.";

#[derive(Debug, Clone)]
enum InstallProbeKind {
    Packaged { install_dir: PathBuf },
    Source,
    Unsupported { reason: String },
}

#[derive(Debug, Clone)]
struct InstallProbe {
    target: UpdateInstallTarget,
    current_version: String,
    current_commit: Option<String>,
    kind: InstallProbeKind,
}

#[derive(Debug, Clone)]
struct UpdateCheckOutcome {
    status: UpdateStatusResponse,
    candidate: Option<UpdateCandidate>,
}

#[derive(Debug, Clone, Default)]
struct BuildStatusArgs {
    step: Option<UpdateOperationStep>,
    candidate_version: Option<String>,
    candidate_tag: Option<String>,
    candidate_commit: Option<String>,
    detail: Option<String>,
    last_run: Option<UpdateRunSummary>,
}

#[derive(Debug, Clone)]
enum UpdateCandidate {
    Packaged(PackagedCandidate),
}

#[derive(Debug, Clone)]
struct PackagedCandidate {
    version: String,
    tag: String,
    published_at: Option<DateTime<Utc>>,
    archive_name: String,
    archive_url: String,
    checksum_name: String,
    checksum_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateStatusEnvelope {
    schema_version: u32,
    status: UpdateStatusResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateExecutionPlan {
    schema_version: u32,
    status_path: PathBuf,
    wait_for_pids: Vec<u32>,
    install: UpdateInstallTarget,
    current_version: String,
    #[serde(default)]
    current_commit: Option<String>,
    #[serde(default)]
    candidate_version: Option<String>,
    #[serde(default)]
    candidate_tag: Option<String>,
    #[serde(default)]
    candidate_commit: Option<String>,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    detail: Option<String>,
    kind: UpdateExecutionPlanKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UpdateExecutionPlanKind {
    Packaged(PackagedUpdatePlan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackagedUpdatePlan {
    install_dir: PathBuf,
    archive_path: PathBuf,
    extract_root: PathBuf,
    target_executable: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub(crate) async fn resolve_update_status(
    state: &AppState,
) -> Result<UpdateStatusResponse, ApiError> {
    let last_run = read_persisted_status(&state.storage)?.and_then(|status| status.last_run);
    let probe = probe_install()?;
    match &probe.kind {
        InstallProbeKind::Packaged { .. } => {
            let outcome = check_packaged_update(&state.http_client, &probe, last_run).await?;
            Ok(outcome.status)
        }
        InstallProbeKind::Source => Ok(build_status(
            &probe,
            UpdateAvailabilityState::Unsupported,
            BuildStatusArgs {
                detail: Some(RELEASE_ONLY_UPDATE_MESSAGE.to_string()),
                last_run,
                ..BuildStatusArgs::default()
            },
        )),
        InstallProbeKind::Unsupported { reason } => Ok(build_status(
            &probe,
            UpdateAvailabilityState::Unsupported,
            BuildStatusArgs {
                detail: Some(reason.clone()),
                last_run,
                ..BuildStatusArgs::default()
            },
        )),
    }
}

pub(crate) async fn trigger_update(
    state: &AppState,
    request: UpdateRunRequest,
) -> Result<UpdateStatusResponse, ApiError> {
    let probe = probe_install()?;
    let previous = read_persisted_status(&state.storage)?;
    let last_run = previous.as_ref().and_then(|status| status.last_run.clone());

    match &probe.kind {
        InstallProbeKind::Packaged { install_dir } => {
            let outcome =
                check_packaged_update(&state.http_client, &probe, last_run.clone()).await?;
            let Some(UpdateCandidate::Packaged(candidate)) = outcome.candidate else {
                return Ok(outcome.status);
            };

            append_log(
                state,
                "info",
                "update",
                format!("staging packaged update to {}", candidate.version),
            )?;

            let run_root = create_update_run_root(&state.storage)?;
            let archive_path =
                staged_asset_path(&run_root, &candidate.archive_name, "update archive path")?;
            let checksum_path =
                staged_asset_path(&run_root, &candidate.checksum_name, "update checksum path")?;
            let extract_root = resolve_relative_path_within_root(
                &run_root,
                Path::new("extract"),
                "update extract directory",
            )
            .map_err(ApiError::from)?;

            let downloading = build_status(
                &probe,
                UpdateAvailabilityState::InProgress,
                BuildStatusArgs {
                    step: Some(UpdateOperationStep::Downloading),
                    candidate_version: Some(candidate.version.clone()),
                    candidate_tag: Some(candidate.tag.clone()),
                    detail: Some(format!("Downloading {}.", candidate.archive_name)),
                    last_run: last_run.clone(),
                    ..BuildStatusArgs::default()
                },
            );
            write_persisted_status(&state.storage, &downloading)?;

            download_asset(&state.http_client, &candidate.archive_url, &archive_path).await?;
            download_asset(&state.http_client, &candidate.checksum_url, &checksum_path).await?;

            let verifying = build_status(
                &probe,
                UpdateAvailabilityState::InProgress,
                BuildStatusArgs {
                    step: Some(UpdateOperationStep::Verifying),
                    candidate_version: Some(candidate.version.clone()),
                    candidate_tag: Some(candidate.tag.clone()),
                    detail: Some(format!("Verifying {}.", candidate.archive_name)),
                    last_run: last_run.clone(),
                    ..BuildStatusArgs::default()
                },
            );
            write_persisted_status(&state.storage, &verifying)?;
            verify_checksum(&archive_path, &checksum_path)?;

            let plan = UpdateExecutionPlan {
                schema_version: UPDATE_STATE_SCHEMA_VERSION,
                status_path: update_state_path(&state.storage)?,
                wait_for_pids: collect_wait_pids(std::process::id(), request.wait_for_pid),
                install: probe.target.clone(),
                current_version: probe.current_version.clone(),
                current_commit: probe.current_commit.clone(),
                candidate_version: Some(candidate.version.clone()),
                candidate_tag: Some(candidate.tag.clone()),
                candidate_commit: None,
                published_at: candidate.published_at,
                detail: Some(format!("Applying {}.", candidate.archive_name)),
                kind: UpdateExecutionPlanKind::Packaged(PackagedUpdatePlan {
                    install_dir: install_dir.clone(),
                    archive_path,
                    extract_root,
                    target_executable: install_dir.join(binary_name()),
                }),
            };
            let plan_path = write_plan(&run_root, &plan)?;

            let applying = build_status(
                &probe,
                UpdateAvailabilityState::InProgress,
                BuildStatusArgs {
                    step: Some(UpdateOperationStep::Applying),
                    candidate_version: plan.candidate_version.clone(),
                    candidate_tag: plan.candidate_tag.clone(),
                    detail: Some("Applying staged package and restarting the daemon.".to_string()),
                    last_run,
                    ..BuildStatusArgs::default()
                },
            );
            write_persisted_status(&state.storage, &applying)?;
            spawn_update_helper(&plan_path)?;
            schedule_daemon_shutdown(state);
            Ok(applying)
        }
        InstallProbeKind::Source => Ok(build_status(
            &probe,
            UpdateAvailabilityState::Unsupported,
            BuildStatusArgs {
                detail: Some(RELEASE_ONLY_UPDATE_MESSAGE.to_string()),
                last_run,
                ..BuildStatusArgs::default()
            },
        )),
        InstallProbeKind::Unsupported { reason } => Ok(build_status(
            &probe,
            UpdateAvailabilityState::Unsupported,
            BuildStatusArgs {
                detail: Some(reason.clone()),
                last_run,
                ..BuildStatusArgs::default()
            },
        )),
    }
}

pub async fn run_update_helper_from_plan(plan_path: &Path) -> Result<()> {
    let plan = read_plan(plan_path)?;
    wait_for_pids(&plan.wait_for_pids).await?;

    let apply_result = match &plan.kind {
        UpdateExecutionPlanKind::Packaged(packaged) => {
            write_helper_status(
                &plan,
                UpdateAvailabilityState::InProgress,
                Some(UpdateOperationStep::Applying),
                plan.detail.clone(),
                None,
            )?;
            apply_packaged_update(packaged).await
        }
    };

    if let Err(error) = apply_result {
        let detail = redact_sensitive_text(&format!("{error:#}"));
        let restart_path = match &plan.kind {
            UpdateExecutionPlanKind::Packaged(packaged) => packaged.target_executable.clone(),
        };
        let _ = spawn_daemon_process(&restart_path);
        write_helper_status(
            &plan,
            UpdateAvailabilityState::Blocked,
            None,
            Some(detail.clone()),
            Some(UpdateRunSummary {
                state: UpdateRunState::Failed,
                started_at: Utc::now(),
                finished_at: Some(Utc::now()),
                from_version: Some(plan.current_version.clone()),
                to_version: plan.candidate_version.clone(),
                from_commit: plan.current_commit.clone(),
                to_commit: plan.candidate_commit.clone(),
                detail: Some(detail),
            }),
        )?;
        return Err(error);
    }

    write_helper_status(
        &plan,
        UpdateAvailabilityState::InProgress,
        Some(UpdateOperationStep::Restarting),
        Some("Restarting daemon with updated build.".to_string()),
        None,
    )?;

    let restart_path = match &plan.kind {
        UpdateExecutionPlanKind::Packaged(packaged) => packaged.target_executable.clone(),
    };
    spawn_daemon_process(&restart_path)?;

    write_helper_status(
        &plan,
        UpdateAvailabilityState::UpToDate,
        None,
        Some("Update applied successfully.".to_string()),
        Some(UpdateRunSummary {
            state: UpdateRunState::Succeeded,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            from_version: Some(plan.current_version.clone()),
            to_version: plan
                .candidate_version
                .clone()
                .or_else(|| Some(plan.current_version.clone())),
            from_commit: plan.current_commit.clone(),
            to_commit: plan
                .candidate_commit
                .clone()
                .or_else(|| plan.current_commit.clone()),
            detail: Some("Update applied successfully.".to_string()),
        }),
    )?;
    Ok(())
}

fn current_executable_path() -> Result<PathBuf, ApiError> {
    std::env::current_exe().map_err(|error| ApiError::from(anyhow!(error)))
}

fn collect_wait_pids(daemon_pid: u32, request_pid: Option<u32>) -> Vec<u32> {
    let mut pids = vec![daemon_pid];
    if let Some(request_pid) = request_pid {
        if request_pid != daemon_pid {
            pids.push(request_pid);
        }
    }
    pids
}

fn schedule_daemon_shutdown(state: &AppState) {
    let shutdown = state.shutdown.clone();
    tokio::spawn(async move {
        sleep(Duration::from_millis(UPDATE_HELPER_SHUTDOWN_DELAY_MS)).await;
        let _ = shutdown.send(());
    });
}

fn probe_install() -> Result<InstallProbe, ApiError> {
    let executable_path = current_executable_path()?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let install_dir = executable_path
        .parent()
        .ok_or_else(|| ApiError::from(anyhow!("failed to resolve executable directory")))?;
    let install_state_path = install_dir.join("install-state.json");

    if install_state_path.exists() {
        let target = UpdateInstallTarget {
            kind: UpdateInstallKind::Packaged,
            executable_path: executable_path.display().to_string(),
            install_dir: Some(install_dir.display().to_string()),
            repo_root: None,
            build_profile: None,
        };
        return Ok(InstallProbe {
            target,
            current_version,
            current_commit: None,
            kind: InstallProbeKind::Packaged {
                install_dir: install_dir.to_path_buf(),
            },
        });
    }

    if let Some((repo_root, build_profile)) = detect_source_checkout(&executable_path) {
        let current_commit = git_output(&repo_root, &["rev-parse", "HEAD"]).ok();
        let target = UpdateInstallTarget {
            kind: UpdateInstallKind::Source,
            executable_path: executable_path.display().to_string(),
            install_dir: None,
            repo_root: Some(repo_root.display().to_string()),
            build_profile: Some(build_profile.clone()),
        };
        return Ok(InstallProbe {
            target,
            current_version,
            current_commit,
            kind: InstallProbeKind::Source,
        });
    }

    Ok(InstallProbe {
        target: UpdateInstallTarget {
            kind: UpdateInstallKind::Unsupported,
            executable_path: executable_path.display().to_string(),
            install_dir: None,
            repo_root: None,
            build_profile: None,
        },
        current_version,
        current_commit: None,
        kind: InstallProbeKind::Unsupported {
            reason: "This install is not a managed package and is not running from a clean source checkout target binary.".to_string(),
        },
    })
}

fn detect_source_checkout(executable_path: &Path) -> Option<(PathBuf, String)> {
    let build_profile = executable_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| *name == "debug" || *name == "release")
        .map(ToOwned::to_owned)?;

    executable_path.ancestors().find_map(|ancestor| {
        let cargo_toml = ancestor.join("Cargo.toml");
        let git_dir = ancestor.join(".git");
        (cargo_toml.exists() && git_dir.exists())
            .then_some((ancestor.to_path_buf(), build_profile.clone()))
    })
}

async fn check_packaged_update(
    client: &reqwest::Client,
    probe: &InstallProbe,
    last_run: Option<UpdateRunSummary>,
) -> Result<UpdateCheckOutcome, ApiError> {
    let release = fetch_latest_release(client).await?;
    let candidate_version = normalize_release_version(&release.tag_name).map_err(|error| {
        ApiError::from(anyhow!(
            "invalid release tag '{}': {error}",
            release.tag_name
        ))
    })?;
    let current_version = Version::parse(&probe.current_version).map_err(|error| {
        ApiError::from(anyhow!(
            "invalid current version '{}': {error}",
            probe.current_version
        ))
    })?;
    let release_version = Version::parse(&candidate_version).map_err(|error| {
        ApiError::from(anyhow!(
            "invalid release version '{candidate_version}': {error}"
        ))
    })?;

    let checked_at = Utc::now();
    if release_version <= current_version {
        return Ok(UpdateCheckOutcome {
            status: build_status(
                probe,
                UpdateAvailabilityState::UpToDate,
                BuildStatusArgs {
                    detail: Some(format!("{} is already current.", probe.current_version)),
                    last_run,
                    ..BuildStatusArgs::default()
                },
            ),
            candidate: None,
        });
    }

    let platform_tag = current_platform_tag();
    let Some((archive_asset, checksum_asset)) =
        select_packaged_assets(&release.assets, &platform_tag)?
    else {
        return Ok(UpdateCheckOutcome {
            status: UpdateStatusResponse {
                install: probe.target.clone(),
                current_version: probe.current_version.clone(),
                current_commit: probe.current_commit.clone(),
                availability: UpdateAvailabilityState::Unsupported,
                checked_at,
                step: None,
                candidate_version: Some(candidate_version),
                candidate_tag: Some(release.tag_name.clone()),
                candidate_commit: None,
                published_at: release.published_at,
                detail: Some(format!(
                    "The latest release does not publish a {} package for {}.",
                    archive_extension(),
                    platform_tag
                )),
                last_run,
            },
            candidate: None,
        });
    };

    Ok(UpdateCheckOutcome {
        status: UpdateStatusResponse {
            install: probe.target.clone(),
            current_version: probe.current_version.clone(),
            current_commit: probe.current_commit.clone(),
            availability: UpdateAvailabilityState::Available,
            checked_at,
            step: None,
            candidate_version: Some(candidate_version.clone()),
            candidate_tag: Some(release.tag_name.clone()),
            candidate_commit: None,
            published_at: release.published_at,
            detail: Some(format!(
                "{} is available for {}.",
                candidate_version, platform_tag
            )),
            last_run: last_run.clone(),
        },
        candidate: Some(UpdateCandidate::Packaged(PackagedCandidate {
            version: candidate_version,
            tag: release.tag_name,
            published_at: release.published_at,
            archive_name: archive_asset.name,
            archive_url: archive_asset.browser_download_url,
            checksum_name: checksum_asset.name,
            checksum_url: checksum_asset.browser_download_url,
        })),
    })
}

fn build_status(
    probe: &InstallProbe,
    availability: UpdateAvailabilityState,
    args: BuildStatusArgs,
) -> UpdateStatusResponse {
    UpdateStatusResponse {
        install: probe.target.clone(),
        current_version: probe.current_version.clone(),
        current_commit: probe.current_commit.clone(),
        availability,
        checked_at: Utc::now(),
        step: args.step,
        candidate_version: args.candidate_version,
        candidate_tag: args.candidate_tag,
        candidate_commit: args.candidate_commit,
        published_at: None,
        detail: args.detail,
        last_run: args.last_run,
    }
}

async fn fetch_latest_release(client: &reqwest::Client) -> Result<GitHubRelease, ApiError> {
    let releases_url = env::var("NUCLEAR_UPDATE_RELEASES_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| UPDATE_RELEASES_URL.to_string());
    let response = client
        .get(&releases_url)
        .header(USER_AGENT, format!("nuclear/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_GATEWAY, error.to_string()))?;
    if !response.status().is_success() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("release endpoint returned {}", response.status()),
        ));
    }
    let release = response
        .json::<GitHubRelease>()
        .await
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_GATEWAY, error.to_string()))?;
    if release.draft || release.prerelease {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "latest release endpoint resolved to a draft or prerelease",
        ));
    }
    Ok(release)
}

fn select_packaged_assets(
    assets: &[GitHubReleaseAsset],
    platform_tag: &str,
) -> Result<Option<(GitHubReleaseAsset, GitHubReleaseAsset)>, ApiError> {
    let archive_suffix = format!("-{platform_tag}-full{}", archive_extension());
    let checksum_suffix = format!("{archive_suffix}.sha256.txt");
    let Some(archive) = assets
        .iter()
        .find(|asset| asset.name.ends_with(&archive_suffix))
        .cloned()
    else {
        return Ok(None);
    };
    let Some(checksum) = assets
        .iter()
        .find(|asset| asset.name.ends_with(&checksum_suffix))
        .cloned()
    else {
        return Ok(None);
    };
    validate_single_path_component(&archive.name, "release archive asset name")
        .map_err(ApiError::from)?;
    validate_single_path_component(&checksum.name, "release checksum asset name")
        .map_err(ApiError::from)?;
    Ok(Some((archive, checksum)))
}

fn normalize_release_version(tag_name: &str) -> Result<String> {
    let trimmed = tag_name.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        bail!("release tag is empty");
    }
    Ok(trimmed.to_string())
}

async fn download_asset(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
) -> Result<(), ApiError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(ApiError::from)?;
    }
    let response =
        client.get(url).send().await.map_err(|error| {
            ApiError::new(axum::http::StatusCode::BAD_GATEWAY, error.to_string())
        })?;
    if !response.status().is_success() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!(
                "download failed for {} with {}",
                destination.display(),
                response.status()
            ),
        ));
    }

    let mut file = File::create(destination).await.map_err(ApiError::from)?;
    let bytes = response
        .bytes()
        .await
        .map_err(|error| ApiError::new(axum::http::StatusCode::BAD_GATEWAY, error.to_string()))?;
    file.write_all(&bytes).await.map_err(ApiError::from)?;
    file.flush().await.map_err(ApiError::from)?;
    Ok(())
}

fn verify_checksum(archive_path: &Path, checksum_path: &Path) -> Result<(), ApiError> {
    let checksum_content = fs::read_to_string(checksum_path).map_err(ApiError::from)?;
    let expected_hash = checksum_content
        .split_whitespace()
        .next()
        .ok_or_else(|| ApiError::from(anyhow!("checksum file is empty")))?;
    let digest = Sha256::digest(fs::read(archive_path).map_err(ApiError::from)?);
    let actual_hash = format!("{:x}", digest);
    if actual_hash != expected_hash.to_ascii_lowercase() {
        return Err(ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!(
                "checksum mismatch for {}",
                archive_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("archive")
            ),
        ));
    }
    Ok(())
}

fn create_update_run_root(storage: &Storage) -> Result<PathBuf, ApiError> {
    let data_dir = storage
        .paths()
        .validated_data_dir()
        .map_err(ApiError::from)?;
    let staging_root = resolve_relative_path_within_root(
        &data_dir,
        Path::new(UPDATE_STAGING_DIR_NAME),
        "update staging directory",
    )
    .map_err(ApiError::from)?;
    fs::create_dir_all(&staging_root).map_err(ApiError::from)?;
    let root = resolve_relative_path_within_root(
        &staging_root,
        Path::new(&Uuid::new_v4().to_string()),
        "update run root",
    )
    .map_err(ApiError::from)?;
    fs::create_dir_all(&root).map_err(ApiError::from)?;
    Ok(root)
}

fn update_state_path(storage: &Storage) -> Result<PathBuf, ApiError> {
    let data_dir = storage
        .paths()
        .validated_data_dir()
        .map_err(ApiError::from)?;
    resolve_relative_path_within_root(
        &data_dir,
        Path::new(UPDATE_STATE_FILE_NAME),
        "update status file",
    )
    .map_err(ApiError::from)
}

fn write_plan(run_root: &Path, plan: &UpdateExecutionPlan) -> Result<PathBuf, ApiError> {
    let path = resolve_relative_path_within_root(
        run_root,
        Path::new(UPDATE_PLAN_FILE_NAME),
        "update plan path",
    )
    .map_err(ApiError::from)?;
    write_json_file(&path, plan).map_err(ApiError::from)?;
    Ok(path)
}

fn spawn_update_helper(plan_path: &Path) -> Result<(), ApiError> {
    let run_root = plan_path.parent().ok_or_else(|| {
        ApiError::from(anyhow!(
            "update plan path '{}' has no parent",
            plan_path.display()
        ))
    })?;
    let plan_path = resolve_path_within_root(run_root, plan_path, "update plan path")
        .map_err(ApiError::from)?;
    let current_executable = std::env::current_exe().map_err(ApiError::from)?;
    let helper_path = resolve_relative_path_within_root(
        run_root,
        Path::new(&helper_binary_name()),
        "update helper path",
    )
    .map_err(ApiError::from)?;
    fs::copy(&current_executable, &helper_path).map_err(ApiError::from)?;
    make_binary_executable(&helper_path).map_err(ApiError::from)?;

    let mut command = Command::new(&helper_path);
    command
        .arg(INTERNAL_UPDATE_HELPER_ARG)
        .arg("--plan")
        .arg(plan_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_no_window(&mut command);
    command.spawn().map_err(ApiError::from)?;
    Ok(())
}

async fn apply_packaged_update(plan: &PackagedUpdatePlan) -> Result<()> {
    extract_archive(&plan.archive_path, &plan.extract_root).await?;
    let bundle_root = locate_bundle_root(&plan.extract_root)?;

    #[cfg(windows)]
    {
        let installer = bundle_root.join("install.ps1");
        if !installer.exists() {
            bail!(
                "packaged installer was not found at {}",
                installer.display()
            );
        }
        let mut command = Command::new("powershell");
        command
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-File")
            .arg(&installer)
            .arg("-InstallDir")
            .arg(&plan.install_dir)
            .arg("-NoPathPersist")
            .arg("-SkipPlaywrightSetup");
        configure_no_window(&mut command);
        run_command(command, "run packaged installer").await?;
    }

    #[cfg(not(windows))]
    {
        let installer = bundle_root.join("install");
        if !installer.exists() {
            bail!(
                "packaged installer was not found at {}",
                installer.display()
            );
        }
        make_binary_executable(&installer)?;
        let mut command = Command::new(&installer);
        command.env("NUCLEAR_INSTALL_DIR", &plan.install_dir);
        run_command(command, "run packaged installer").await?;
    }

    Ok(())
}

async fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    validate_archive_contents(archive_path, destination).await?;
    if destination.exists() {
        fs::remove_dir_all(destination).with_context(|| {
            format!(
                "failed to remove existing extract directory {}",
                destination.display()
            )
        })?;
    }
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    #[cfg(windows)]
    {
        let command = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            escape_powershell_literal(archive_path),
            escape_powershell_literal(destination)
        );
        let mut process = Command::new("powershell");
        process
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(command);
        configure_no_window(&mut process);
        run_command(process, "extract packaged archive").await?;
    }

    #[cfg(not(windows))]
    {
        let mut process = Command::new("tar");
        process
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(destination);
        run_command(process, "extract packaged archive").await?;
    }

    Ok(())
}

fn locate_bundle_root(extract_root: &Path) -> Result<PathBuf> {
    if extract_root.join("install").exists() || extract_root.join("install.ps1").exists() {
        return Ok(extract_root.to_path_buf());
    }

    let mut directories = fs::read_dir(extract_root)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    directories.sort();

    if directories.len() == 1 {
        return Ok(directories.remove(0));
    }

    bail!(
        "failed to locate extracted bundle root under {}",
        extract_root.display()
    )
}

fn read_persisted_status(storage: &Storage) -> Result<Option<UpdateStatusResponse>, ApiError> {
    let path = update_state_path(storage)?;
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ApiError::from(error)),
    };
    let envelope: UpdateStatusEnvelope = serde_json::from_str(&content).map_err(ApiError::from)?;
    Ok((envelope.schema_version == UPDATE_STATE_SCHEMA_VERSION).then_some(envelope.status))
}

fn write_persisted_status(
    storage: &Storage,
    status: &UpdateStatusResponse,
) -> Result<(), ApiError> {
    write_json_file(
        &update_state_path(storage)?,
        &UpdateStatusEnvelope {
            schema_version: UPDATE_STATE_SCHEMA_VERSION,
            status: status.clone(),
        },
    )
    .map_err(ApiError::from)
}

fn read_plan(plan_path: &Path) -> Result<UpdateExecutionPlan> {
    let run_root = plan_path
        .parent()
        .ok_or_else(|| anyhow!("update plan path '{}' has no parent", plan_path.display()))?;
    let plan_path = resolve_path_within_root(run_root, plan_path, "update plan path")?;
    let content = fs::read_to_string(&plan_path)
        .with_context(|| format!("failed to read {}", plan_path.display()))?;
    let plan: UpdateExecutionPlan = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", plan_path.display()))?;
    if plan.schema_version != UPDATE_STATE_SCHEMA_VERSION {
        bail!("unsupported update plan schema {}", plan.schema_version);
    }
    validate_update_plan_paths(&plan, run_root)?;
    Ok(plan)
}

fn write_helper_status(
    plan: &UpdateExecutionPlan,
    availability: UpdateAvailabilityState,
    step: Option<UpdateOperationStep>,
    detail: Option<String>,
    last_run: Option<UpdateRunSummary>,
) -> Result<()> {
    write_json_file(
        &plan.status_path,
        &UpdateStatusEnvelope {
            schema_version: UPDATE_STATE_SCHEMA_VERSION,
            status: UpdateStatusResponse {
                install: plan.install.clone(),
                current_version: plan.current_version.clone(),
                current_commit: plan.current_commit.clone(),
                availability,
                checked_at: Utc::now(),
                step,
                candidate_version: plan.candidate_version.clone(),
                candidate_tag: plan.candidate_tag.clone(),
                candidate_commit: plan.candidate_commit.clone(),
                published_at: plan.published_at,
                detail,
                last_run,
            },
        },
    )
}

fn write_json_file(path: &Path, value: &impl Serialize) -> Result<()> {
    let path = resolve_path_from_existing_parent(path, "update JSON file")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let temp_path =
        resolve_path_from_existing_parent(&path.with_extension("tmp"), "update JSON temp file")?;
    let value = serde_json::to_value(value)?;
    let content = serde_json::to_string_pretty(&redact_sensitive_json_value(&value))?;
    fs::write(&temp_path, content)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;
    fs::rename(&temp_path, &path)
        .with_context(|| format!("failed to move {} into place", path.display()))?;
    Ok(())
}

async fn wait_for_pids(pids: &[u32]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(UPDATE_HELPER_WAIT_TIMEOUT_SECS);
    loop {
        let mut any_alive = false;
        for pid in pids {
            if *pid == 0 {
                continue;
            }
            if pid_is_alive(*pid).await? {
                any_alive = true;
                break;
            }
        }
        if !any_alive {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for active Nuclear processes to exit");
        }
        sleep(Duration::from_millis(UPDATE_HELPER_WAIT_POLL_MS)).await;
    }
}

async fn pid_is_alive(pid: u32) -> Result<bool> {
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell");
        command
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(format!(
                "if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
            ));
        configure_no_window(&mut command);
        let status = command.status().await?;
        Ok(status.success())
    }

    #[cfg(not(windows))]
    {
        let status = Command::new("sh")
            .arg("-lc")
            .arg(format!("kill -0 {pid} >/dev/null 2>&1"))
            .status()
            .await?;
        Ok(status.success())
    }
}

async fn run_command(mut command: Command, context: &str) -> Result<()> {
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to {context}"))?;
    if output.status.success() {
        return Ok(());
    }
    bail!("{context}: {}", command_output_summary(&output));
}

fn git_output(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            command_output_summary(&output)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn command_output_summary(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let summary = match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => format!("exit={}", output.status),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    redact_sensitive_text(&summary)
}

fn spawn_daemon_process(executable: &Path) -> Result<()> {
    let mut command = std::process::Command::new(executable);
    command
        .arg(INTERNAL_DAEMON_ARG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_no_window_std(&mut command);
    command.spawn().with_context(|| {
        format!(
            "failed to start daemon using {} {}",
            executable.display(),
            INTERNAL_DAEMON_ARG
        )
    })?;
    Ok(())
}

fn staged_asset_path(run_root: &Path, file_name: &str, label: &str) -> Result<PathBuf, ApiError> {
    let file_name = validate_single_path_component(file_name, label).map_err(ApiError::from)?;
    resolve_relative_path_within_root(run_root, Path::new(&file_name), label)
        .map_err(ApiError::from)
}

fn update_data_dir_from_run_root(run_root: &Path) -> Result<&Path> {
    let staging_root = run_root.parent().ok_or_else(|| {
        anyhow!(
            "update run root '{}' has no staging parent",
            run_root.display()
        )
    })?;
    let data_dir = staging_root.parent().ok_or_else(|| {
        anyhow!(
            "update staging directory '{}' has no data root parent",
            staging_root.display()
        )
    })?;
    Ok(data_dir)
}

fn validate_update_plan_paths(plan: &UpdateExecutionPlan, run_root: &Path) -> Result<()> {
    let data_dir = update_data_dir_from_run_root(run_root)?;
    resolve_path_within_root(data_dir, &plan.status_path, "update status path")?;
    match &plan.kind {
        UpdateExecutionPlanKind::Packaged(packaged) => {
            resolve_path_within_root(run_root, &packaged.archive_path, "update archive path")?;
            resolve_path_within_root(run_root, &packaged.extract_root, "update extract directory")?;
        }
    }
    Ok(())
}

fn validate_archive_entry_destination(destination: &Path, entry_name: &str) -> Result<PathBuf> {
    let relative = validate_relative_path(Path::new(entry_name.trim()), "archive entry path")?;
    resolve_relative_path_within_root(destination, &relative, "archive entry destination")
}

async fn validate_archive_contents(archive_path: &Path, destination: &Path) -> Result<()> {
    let entries = list_archive_entries(archive_path).await?;
    for entry in entries {
        validate_archive_entry_destination(destination, &entry)?;
    }
    Ok(())
}

async fn list_archive_entries(archive_path: &Path) -> Result<Vec<String>> {
    #[cfg(windows)]
    {
        let command = format!(
            "Add-Type -AssemblyName System.IO.Compression.FileSystem; $zip=[IO.Compression.ZipFile]::OpenRead('{}'); try {{ $zip.Entries | ForEach-Object {{ $_.FullName }} }} finally {{ $zip.Dispose() }}",
            escape_powershell_literal(archive_path)
        );
        let mut process = Command::new("powershell");
        process
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(command);
        configure_no_window(&mut process);
        let output = process.output().await?;
        if !output.status.success() {
            bail!(
                "inspect packaged archive: {}",
                command_output_summary(&output)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("tar")
            .arg("-tzf")
            .arg(archive_path)
            .output()
            .await?;
        if !output.status.success() {
            bail!(
                "inspect packaged archive: {}",
                command_output_summary(&output)
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }
}

fn current_platform_tag() -> String {
    let arch = match env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    };
    format!("{}-{arch}", env::consts::OS)
}

fn archive_extension() -> &'static str {
    if cfg!(windows) {
        ".zip"
    } else {
        ".tar.gz"
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "nuclear.exe"
    } else {
        "nuclear"
    }
}

fn helper_binary_name() -> String {
    if cfg!(windows) {
        format!("{UPDATE_HELPER_BINARY_BASENAME}.exe")
    } else {
        UPDATE_HELPER_BINARY_BASENAME.to_string()
    }
}

#[cfg(unix)]
fn make_binary_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_binary_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn escape_powershell_literal(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

#[cfg(windows)]
fn configure_no_window(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_no_window(_command: &mut Command) {}

#[cfg(windows)]
fn configure_no_window_std(command: &mut std::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_no_window_std(_command: &mut std::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn run_std_command(cwd: &Path, program: &str, args: &[&str]) {
        let output = std::process::Command::new(program)
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {:?} failed: {}",
            program,
            args,
            command_output_summary(&output)
        );
    }

    #[tokio::test]
    async fn apply_packaged_update_runs_extracted_installer() {
        let root = temp_dir("nuclear-packaged-update");
        let bundle_name = format!(
            "nuclear-{}-{}-full",
            env!("CARGO_PKG_VERSION"),
            current_platform_tag()
        );
        let bundle_root = root.join(&bundle_name);
        let install_dir = root.join("install");
        fs::create_dir_all(&bundle_root).unwrap();

        let payload_path = bundle_root.join(binary_name());
        fs::write(&payload_path, b"updated-build").unwrap();

        #[cfg(windows)]
        {
            let installer = bundle_root.join("install.ps1");
            fs::write(
                &installer,
                format!(
                    "param([string]$InstallDir,[switch]$NoPathPersist,[switch]$SkipPlaywrightSetup)\nNew-Item -ItemType Directory -Force -Path $InstallDir | Out-Null\nCopy-Item -Force -LiteralPath (Join-Path $PSScriptRoot '{0}') -Destination (Join-Path $InstallDir '{0}')\n",
                    binary_name()
                ),
            )
            .unwrap();
            let archive_path = root.join(format!("{bundle_name}.zip"));
            run_std_command(
                &root,
                "powershell",
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
                        escape_powershell_literal(&bundle_root),
                        escape_powershell_literal(&archive_path)
                    ),
                ],
            );

            apply_packaged_update(&PackagedUpdatePlan {
                install_dir: install_dir.clone(),
                archive_path,
                extract_root: root.join("extract"),
                target_executable: install_dir.join(binary_name()),
            })
            .await
            .unwrap();
        }

        #[cfg(not(windows))]
        {
            let installer = bundle_root.join("install");
            fs::write(
                &installer,
                format!(
                    "#!/usr/bin/env bash\nset -euo pipefail\nmkdir -p \"$NUCLEAR_INSTALL_DIR\"\ncp \"$0_dir/{0}\" \"$NUCLEAR_INSTALL_DIR/{0}\"\n",
                    binary_name()
                )
                .replace("$0_dir", "$(cd \"$(dirname \"$0\")\" && pwd)"),
            )
            .unwrap();
            make_binary_executable(&installer).unwrap();

            let archive_path = root.join(format!("{bundle_name}.tar.gz"));
            run_std_command(
                &root,
                "tar",
                &[
                    "-czf",
                    archive_path.to_str().unwrap(),
                    "-C",
                    root.to_str().unwrap(),
                    &bundle_name,
                ],
            );

            apply_packaged_update(&PackagedUpdatePlan {
                install_dir: install_dir.clone(),
                archive_path,
                extract_root: root.join("extract"),
                target_executable: install_dir.join(binary_name()),
            })
            .await
            .unwrap();
        }

        assert_eq!(
            fs::read(install_dir.join(binary_name())).unwrap(),
            b"updated-build"
        );
    }

    #[test]
    fn validate_archive_entry_destination_rejects_traversal() {
        let root = temp_dir("nuclear-update-archive");
        let error = validate_archive_entry_destination(&root, "../escape").unwrap_err();

        assert!(error.to_string().contains("traversal"));
    }

    #[test]
    fn select_packaged_assets_rejects_separator_bearing_asset_names() {
        let platform_tag = current_platform_tag();
        let assets = vec![
            GitHubReleaseAsset {
                name: format!(
                    "nested/nuclear-1.0.0-{platform_tag}-full{}",
                    archive_extension()
                ),
                browser_download_url: "https://example.invalid/archive".to_string(),
            },
            GitHubReleaseAsset {
                name: format!(
                    "nuclear-1.0.0-{platform_tag}-full{}.sha256.txt",
                    archive_extension()
                ),
                browser_download_url: "https://example.invalid/checksum".to_string(),
            },
        ];

        let error = select_packaged_assets(&assets, &platform_tag).unwrap_err();

        assert!(error.message.contains("asset name"));
    }

    #[test]
    fn create_update_run_root_stays_under_managed_data_dir() {
        let storage = Storage::open_at(temp_dir("nuclear-update-storage")).unwrap();
        let normalized_data_dir =
            agent_core::resolve_operator_path(&storage.paths().data_dir, "managed data dir")
                .unwrap();

        let run_root = create_update_run_root(&storage).unwrap();

        assert!(run_root.starts_with(&normalized_data_dir));
        assert!(run_root
            .strip_prefix(normalized_data_dir.join(UPDATE_STAGING_DIR_NAME))
            .is_ok());
    }

    #[test]
    fn read_plan_rejects_archive_path_outside_run_root() {
        let root = temp_dir("nuclear-update-plan");
        let run_root = root
            .join("data")
            .join(UPDATE_STAGING_DIR_NAME)
            .join("run-1");
        fs::create_dir_all(&run_root).unwrap();
        let plan_path = run_root.join(UPDATE_PLAN_FILE_NAME);
        let status_path = root.join("data").join(UPDATE_STATE_FILE_NAME);
        let invalid_archive = root.join("outside").join("nuclear.zip");
        let plan = serde_json::json!({
            "schema_version": UPDATE_STATE_SCHEMA_VERSION,
            "status_path": status_path,
            "wait_for_pids": [],
            "install": {
                "kind": "packaged",
                "executable_path": "C:/Nuclear/nuclear.exe",
                "install_dir": "C:/Nuclear",
                "repo_root": null,
                "build_profile": null
            },
            "current_version": "0.8.3",
            "current_commit": null,
            "candidate_version": "0.8.4",
            "candidate_tag": "v0.8.4",
            "candidate_commit": null,
            "published_at": null,
            "detail": "Applying update.",
            "kind": {
                "kind": "packaged",
                "install_dir": root.join("install"),
                "archive_path": invalid_archive,
                "extract_root": run_root.join("extract"),
                "target_executable": root.join("install").join(binary_name())
            }
        });
        fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();

        let error = read_plan(&plan_path).unwrap_err();

        assert!(error.to_string().contains("escapes managed root"));
    }

    #[test]
    fn write_json_file_redacts_sensitive_values() {
        let root = temp_dir("nuclear-update-json");
        let path = root.join("status.json");

        write_json_file(
            &path,
            &serde_json::json!({
                "detail": "Bearer sk-live-123456 refresh_token=refresh-secret"
            }),
        )
        .unwrap();

        let content = fs::read_to_string(path).unwrap();
        assert!(!content.contains("sk-live-123456"));
        assert!(!content.contains("refresh-secret"));
        assert!(content.contains("[REDACTED]"));
    }
}
