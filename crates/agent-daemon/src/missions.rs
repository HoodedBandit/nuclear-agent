use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path as FsPath, PathBuf},
    time::Duration,
};

use agent_core::{
    AutonomyMode, AutonomyState, AutopilotConfig, EvolveConfig, EvolveStartRequest, EvolveState,
    EvolveStopPolicy, Mission, MissionCheckpoint, MissionControlRequest, MissionPhase,
    MissionStatus, PermissionPreset, ToolExecutionRecord, WakeTrigger,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use tokio::task::JoinSet;
use tokio::time::sleep;

use crate::{
    append_log, execute_task_request, normalize_memory_sentence, poll_inbox_connectors,
    request_daemon_restart, resolve_alias_and_provider, resolve_request_cwd, summarize_tool_output,
    ApiError, AppState, LimitQuery, TaskRequestInput, AUTOPILOT_DIRECTIVE_SCHEMA,
};

const EVOLVE_DIRECTIVE_SCHEMA: &str = r#"{
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
    },
    "continue_evolving": {
      "type": "boolean"
    },
    "improvement_goal": {
      "type": "string",
      "minLength": 1
    },
    "verification_summary": {
      "type": "string",
      "minLength": 1
    },
    "restart_required": {
      "type": "boolean"
    },
    "diff_summary": {
      "type": "string",
      "minLength": 1
    }
  },
  "required": ["status", "summary"],
  "additionalProperties": false
}"#;

pub(crate) async fn list_missions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Mission>>, ApiError> {
    Ok(Json(state.storage.list_missions()?))
}

pub(crate) async fn evolve_status(
    State(state): State<AppState>,
) -> Result<Json<EvolveConfig>, ApiError> {
    let config = state.config.read().await;
    Ok(Json(config.evolve.clone()))
}

pub(crate) async fn start_evolve_mode(
    State(state): State<AppState>,
    Json(payload): Json<EvolveStartRequest>,
) -> Result<Json<EvolveConfig>, ApiError> {
    let (evolve, mission) = {
        let mut config = state.config.write().await;
        let alias = payload
            .alias
            .clone()
            .or_else(|| config.main_agent_alias.clone())
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "no main alias configured"))?;
        if config.get_alias(&alias).is_none() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("unknown evolve alias '{alias}'"),
            ));
        }

        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;
        config.autonomy.unlimited_usage = true;
        config.autonomy.full_network = true;
        config.autonomy.allow_self_edit = true;
        config.autonomy.consented_at = Some(Utc::now());
        config.autopilot.state = agent_core::AutopilotState::Enabled;
        config.autopilot.allow_background_shell = true;
        config.autopilot.allow_background_network = true;
        config.autopilot.allow_background_self_edit = true;

        let mut mission = Mission::new(
            "Evolve the agent".to_string(),
            "Methodically improve the agent's own code for functionality first, speed second, and bug fixes third. Pick one bounded improvement per cycle, verify it, checkpoint it, and continue only if further work is justified.".to_string(),
        );
        mission.alias = Some(alias.clone());
        mission.requested_model = payload.requested_model.clone();
        mission.workspace_key = Some(
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
                .to_string(),
        );
        mission.phase = Some(MissionPhase::Planner);
        mission.wake_trigger = Some(WakeTrigger::Manual);
        mission.evolve = true;
        mission.status = MissionStatus::Queued;
        state.storage.upsert_mission(&mission)?;

        config.evolve.state = EvolveState::Running;
        config.evolve.stop_policy = if payload.budget_friendly.unwrap_or(false) {
            EvolveStopPolicy::BudgetFriendly
        } else {
            EvolveStopPolicy::AgentDecides
        };
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.evolve.alias = Some(alias);
        config.evolve.requested_model = payload.requested_model.clone();
        config.evolve.iteration = 0;
        config.evolve.last_goal = None;
        config.evolve.last_summary = None;
        config.evolve.last_verified_at = None;
        config.evolve.pending_restart = false;

        state.storage.save_config(&config)?;
        (config.evolve.clone(), mission)
    };

    state.autopilot_wake.notify_waiters();
    append_log(
        &state,
        "warn",
        "evolve",
        format!(
            "evolve mode started with mission '{}' ({})",
            mission.title, mission.id
        ),
    )?;
    Ok(Json(evolve))
}

pub(crate) async fn pause_evolve_mode(
    State(state): State<AppState>,
) -> Result<Json<EvolveConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.evolve.state = EvolveState::Paused;
        if matches!(config.autonomy.mode, AutonomyMode::Evolve) {
            config.autonomy.state = AutonomyState::Paused;
        }
        if let Some(mission_id) = config.evolve.current_mission_id.clone() {
            if let Some(mut mission) = state.storage.get_mission(&mission_id)? {
                mission.status = MissionStatus::Blocked;
                mission.updated_at = Utc::now();
                mission.last_error = Some("Evolve mode paused by operator".to_string());
                state.storage.upsert_mission(&mission)?;
            }
        }
        state.storage.save_config(&config)?;
        config.evolve.clone()
    };
    append_log(&state, "warn", "evolve", "evolve mode paused")?;
    Ok(Json(updated))
}

pub(crate) async fn resume_evolve_mode(
    State(state): State<AppState>,
) -> Result<Json<EvolveConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        config.evolve.state = EvolveState::Running;
        config.autonomy.state = AutonomyState::Enabled;
        config.autonomy.mode = AutonomyMode::Evolve;
        config.autonomy.unlimited_usage = true;
        config.autonomy.full_network = true;
        config.autonomy.allow_self_edit = true;
        if let Some(mission_id) = config.evolve.current_mission_id.clone() {
            if let Some(mut mission) = state.storage.get_mission(&mission_id)? {
                if matches!(
                    mission.status,
                    MissionStatus::Blocked | MissionStatus::Failed
                ) {
                    mission.status = MissionStatus::Queued;
                    mission.updated_at = Utc::now();
                    mission.last_error = None;
                    state.storage.upsert_mission(&mission)?;
                }
            }
        }
        state.storage.save_config(&config)?;
        config.evolve.clone()
    };
    state.autopilot_wake.notify_waiters();
    append_log(&state, "warn", "evolve", "evolve mode resumed")?;
    Ok(Json(updated))
}

pub(crate) async fn stop_evolve_mode(
    State(state): State<AppState>,
) -> Result<Json<EvolveConfig>, ApiError> {
    let updated = {
        let mut config = state.config.write().await;
        if let Some(mission_id) = config.evolve.current_mission_id.clone() {
            if let Some(mut mission) = state.storage.get_mission(&mission_id)? {
                mission.status = MissionStatus::Cancelled;
                mission.updated_at = Utc::now();
                mission.last_error = Some("Evolve mode stopped by operator".to_string());
                state.storage.upsert_mission(&mission)?;
            }
        }
        config.evolve.state = EvolveState::Completed;
        config.evolve.current_mission_id = None;
        config.evolve.pending_restart = false;
        config.autonomy.state = AutonomyState::Disabled;
        config.autonomy.mode = AutonomyMode::Assisted;
        state.storage.save_config(&config)?;
        config.evolve.clone()
    };
    append_log(&state, "warn", "evolve", "evolve mode stopped")?;
    Ok(Json(updated))
}

pub(crate) async fn add_mission(
    State(state): State<AppState>,
    Json(mut payload): Json<Mission>,
) -> Result<Json<Mission>, ApiError> {
    if payload.workspace_key.is_none() {
        payload.workspace_key = Some(resolve_request_cwd(None)?.display().to_string());
    }
    if payload.phase.is_none() {
        payload.phase = Some(MissionPhase::Planner);
    }
    normalize_watch_settings(&mut payload);
    prime_watch_fingerprint_if_needed(&mut payload)?;
    if payload.updated_at < payload.created_at {
        payload.updated_at = payload.created_at;
    }
    if matches!(
        payload.status,
        MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
    ) {
        payload.status = MissionStatus::Queued;
    }
    payload.repeat_interval_seconds = normalized_repeat_interval(payload.repeat_interval_seconds);
    if payload.watch_path.is_some() && payload.repeat_interval_seconds.is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "mission cannot use both watch_path and repeat_interval_seconds",
        ));
    }
    if payload.watch_path.is_some() {
        payload.status = MissionStatus::Waiting;
        payload.wake_trigger = Some(WakeTrigger::FileChange);
        payload.wake_at = None;
        payload.scheduled_for_at = None;
    } else if payload.repeat_interval_seconds.is_some() && payload.wake_at.is_none() {
        payload.status = MissionStatus::Scheduled;
        initialize_repeat_schedule(&mut payload, None);
    } else if payload.wake_at.is_some() {
        payload.status = MissionStatus::Scheduled;
        payload.wake_trigger.get_or_insert(WakeTrigger::Timer);
        payload.scheduled_for_at = payload.wake_at;
        payload.repeat_anchor_at = None;
    } else if matches!(payload.status, MissionStatus::Scheduled) {
        payload.status = MissionStatus::Queued;
        payload.scheduled_for_at = None;
        payload.repeat_anchor_at = None;
    } else {
        payload.scheduled_for_at = None;
        payload.repeat_anchor_at = None;
    }
    state.storage.upsert_mission(&payload)?;
    state.autopilot_wake.notify_waiters();
    append_log(
        &state,
        "info",
        "missions",
        format!("mission '{}' recorded", payload.title),
    )?;
    Ok(Json(payload))
}

pub(crate) async fn get_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
) -> Result<Json<Mission>, ApiError> {
    let mission = state
        .storage
        .get_mission(&mission_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown mission"))?;
    Ok(Json(mission))
}

pub(crate) async fn pause_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Json(payload): Json<MissionControlRequest>,
) -> Result<Json<Mission>, ApiError> {
    let mut mission = state
        .storage
        .get_mission(&mission_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown mission"))?;
    mission.status = MissionStatus::Blocked;
    mission.updated_at = Utc::now();
    mission.last_error = payload.note.clone();
    state.storage.upsert_mission(&mission)?;
    state
        .storage
        .save_mission_checkpoint(&MissionCheckpoint::new(
            mission.id.clone(),
            mission.status.clone(),
            payload.note.unwrap_or_else(|| "mission paused".to_string()),
        ))?;
    Ok(Json(mission))
}

pub(crate) async fn resume_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Json(payload): Json<MissionControlRequest>,
) -> Result<Json<Mission>, ApiError> {
    let mut mission = state
        .storage
        .get_mission(&mission_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown mission"))?;
    if payload.clear_watch_path {
        mission.watch_path = None;
    }
    if let Some(watch_path) = payload.watch_path {
        mission.watch_path = Some(watch_path);
    }
    if let Some(watch_recursive) = payload.watch_recursive {
        mission.watch_recursive = watch_recursive;
    }
    if payload.clear_repeat_interval_seconds {
        mission.repeat_interval_seconds = None;
        mission.repeat_anchor_at = None;
    }
    if let Some(repeat_interval_seconds) = payload.repeat_interval_seconds {
        mission.repeat_interval_seconds = normalized_repeat_interval(Some(repeat_interval_seconds));
    }
    if payload.clear_wake_at {
        mission.wake_at = None;
        mission.scheduled_for_at = None;
    }
    if let Some(wake_at) = payload.wake_at {
        mission.wake_at = Some(wake_at);
        mission.scheduled_for_at = Some(wake_at);
    }
    if payload.clear_session_id {
        mission.session_id = None;
    }
    if payload.clear_handoff_summary {
        mission.handoff_summary = None;
    }
    if mission.phase.is_none() {
        mission.phase = Some(MissionPhase::Planner);
    }
    normalize_watch_settings(&mut mission);
    if mission.watch_path.is_some() && mission.repeat_interval_seconds.is_some() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "mission cannot use both watch_path and repeat_interval_seconds",
        ));
    }
    mission.status = if mission.watch_path.is_some() {
        MissionStatus::Waiting
    } else if mission.repeat_interval_seconds.is_some() {
        let requested_wake_at = mission.wake_at;
        initialize_repeat_schedule(&mut mission, requested_wake_at);
        MissionStatus::Scheduled
    } else {
        mission
            .wake_at
            .map(|_| MissionStatus::Scheduled)
            .unwrap_or(MissionStatus::Queued)
    };
    mission.wake_trigger = Some(if mission.watch_path.is_some() {
        WakeTrigger::FileChange
    } else if mission.wake_at.is_some() {
        WakeTrigger::Timer
    } else {
        WakeTrigger::Manual
    });
    mission.updated_at = Utc::now();
    mission.last_error = None;
    if mission.watch_path.is_some() {
        mission.scheduled_for_at = None;
    } else if mission.wake_at.is_some() {
        mission.scheduled_for_at = mission.wake_at;
    } else {
        mission.scheduled_for_at = None;
        mission.repeat_anchor_at = None;
    }
    prime_watch_fingerprint_if_needed(&mut mission)?;
    state.storage.upsert_mission(&mission)?;
    state
        .storage
        .save_mission_checkpoint(&MissionCheckpoint::new(
            mission.id.clone(),
            mission.status.clone(),
            payload
                .note
                .unwrap_or_else(|| "mission resumed".to_string()),
        ))?;
    state.autopilot_wake.notify_waiters();
    Ok(Json(mission))
}

pub(crate) async fn cancel_mission(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
) -> Result<Json<Mission>, ApiError> {
    let mut mission = state
        .storage
        .get_mission(&mission_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown mission"))?;
    mission.status = MissionStatus::Cancelled;
    mission.updated_at = Utc::now();
    state.storage.upsert_mission(&mission)?;
    state
        .storage
        .save_mission_checkpoint(&MissionCheckpoint::new(
            mission.id.clone(),
            mission.status.clone(),
            "mission cancelled".to_string(),
        ))?;
    Ok(Json(mission))
}

pub(crate) async fn list_mission_checkpoints(
    State(state): State<AppState>,
    Path(mission_id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MissionCheckpoint>>, ApiError> {
    Ok(Json(state.storage.list_mission_checkpoints(
        &mission_id,
        query.limit.unwrap_or(25),
    )?))
}

fn normalize_watch_settings(mission: &mut Mission) {
    if mission.watch_path.is_none() {
        mission.watch_recursive = false;
        mission.watch_fingerprint = None;
    } else {
        mission.repeat_interval_seconds = None;
        mission.repeat_anchor_at = None;
        mission.scheduled_for_at = None;
    }
}

fn normalized_repeat_interval(value: Option<u64>) -> Option<u64> {
    value.filter(|seconds| *seconds > 0)
}

fn initialize_repeat_schedule(mission: &mut Mission, requested_wake_at: Option<DateTime<Utc>>) {
    let Some(seconds) = mission.repeat_interval_seconds else {
        mission.repeat_anchor_at = None;
        return;
    };
    let anchor = requested_wake_at
        .or(mission.repeat_anchor_at)
        .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(seconds as i64));
    mission.repeat_anchor_at = Some(anchor);
    mission.scheduled_for_at = Some(anchor);
    mission.wake_at = Some(anchor);
    mission.wake_trigger = Some(WakeTrigger::Timer);
}

fn next_repeat_wake_at_from_anchor(anchor: DateTime<Utc>, seconds: u64) -> DateTime<Utc> {
    let interval = chrono::Duration::seconds(seconds as i64);
    let mut next = anchor;
    let now = Utc::now();
    while next <= now {
        next += interval;
    }
    next
}

fn schedule_next_repeat_run(mission: &mut Mission) {
    let Some(seconds) = mission.repeat_interval_seconds else {
        mission.repeat_anchor_at = None;
        mission.scheduled_for_at = None;
        return;
    };
    let anchor = mission.repeat_anchor_at.unwrap_or_else(|| {
        mission
            .scheduled_for_at
            .or(mission.wake_at)
            .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(seconds as i64))
    });
    let interval = chrono::Duration::seconds(seconds as i64);
    let mut next = mission
        .scheduled_for_at
        .or(mission.wake_at)
        .map(|scheduled| scheduled + interval)
        .unwrap_or_else(|| next_repeat_wake_at_from_anchor(anchor, seconds));
    let now = Utc::now();
    if next < anchor {
        next = anchor;
    }
    while next <= now {
        next += interval;
    }
    mission.repeat_anchor_at = Some(anchor);
    mission.scheduled_for_at = Some(next);
    mission.wake_at = Some(next);
    mission.wake_trigger = Some(WakeTrigger::Timer);
    mission.status = MissionStatus::Scheduled;
}

fn should_rotate_mission_session(mission: &Mission, checkpoints: &[MissionCheckpoint]) -> bool {
    let Some(session_id) = mission.session_id.as_deref() else {
        return false;
    };
    if mission.repeat_interval_seconds.is_none() && mission.watch_path.is_none() {
        return false;
    }
    checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.session_id.as_deref() == Some(session_id))
        .count()
        >= 8
}

fn synthesize_handoff_summary(
    mission: &Mission,
    checkpoints: &[MissionCheckpoint],
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(summary) = mission
        .handoff_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        parts.push(summary.to_string());
    }
    for checkpoint in checkpoints.iter().take(3).rev() {
        parts.push(format!("{:?}: {}", checkpoint.status, checkpoint.summary));
    }
    if parts.is_empty() {
        None
    } else {
        Some(normalize_memory_sentence(&parts.join(" | ")))
    }
}

fn maybe_rotate_mission_session(
    state: &AppState,
    mission: &mut Mission,
    checkpoints: &[MissionCheckpoint],
) -> Result<()> {
    if !should_rotate_mission_session(mission, checkpoints) {
        return Ok(());
    }
    mission.handoff_summary = synthesize_handoff_summary(mission, checkpoints);
    mission.session_id = None;
    mission.updated_at = Utc::now();
    state.storage.upsert_mission(mission)?;
    let mut checkpoint = MissionCheckpoint::new(
        mission.id.clone(),
        MissionStatus::Waiting,
        "Mission session rotated to keep recurring context bounded".to_string(),
    );
    checkpoint.phase = mission.phase.clone();
    checkpoint.handoff_summary = mission.handoff_summary.clone();
    checkpoint.scheduled_for_at = mission.scheduled_for_at;
    state.storage.save_mission_checkpoint(&checkpoint)?;
    Ok(())
}

fn resolved_watch_path(mission: &Mission) -> Option<PathBuf> {
    mission.watch_path.as_ref().map(|path| {
        if path.is_absolute() {
            path.clone()
        } else if let Some(workspace_key) = mission.workspace_key.as_ref() {
            PathBuf::from(workspace_key).join(path)
        } else {
            path.clone()
        }
    })
}

fn fingerprint_path(path: &FsPath, recursive: bool) -> Result<String> {
    let mut hasher = DefaultHasher::new();
    hash_path_state(&mut hasher, path, recursive)?;
    Ok(format!("{:016x}", hasher.finish()))
}

/// Max file size for which we hash full contents. Above this we hash
/// the first and last 4 KB plus the file size as a fast approximation.
const CONTENT_HASH_FULL_THRESHOLD: u64 = 1_048_576; // 1 MB
const CONTENT_HASH_PARTIAL_SIZE: usize = 4096;

fn hash_file_content(hasher: &mut DefaultHasher, path: &FsPath, len: u64) -> Result<()> {
    use std::io::Read;
    if len == 0 {
        return Ok(());
    }
    let mut file = fs::File::open(path)?;
    if len <= CONTENT_HASH_FULL_THRESHOLD {
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        buf.hash(hasher);
    } else {
        // Hash first 4 KB + last 4 KB + length for large files.
        use std::io::Seek;
        let mut head = vec![0u8; CONTENT_HASH_PARTIAL_SIZE];
        file.read_exact(&mut head)?;
        head.hash(hasher);

        let tail_offset = len.saturating_sub(CONTENT_HASH_PARTIAL_SIZE as u64);
        file.seek(std::io::SeekFrom::Start(tail_offset))?;
        let mut tail = vec![0u8; CONTENT_HASH_PARTIAL_SIZE];
        let n = file.read(&mut tail)?;
        tail[..n].hash(hasher);

        len.hash(hasher);
    }
    Ok(())
}

fn hash_path_state(hasher: &mut DefaultHasher, path: &FsPath, recursive: bool) -> Result<()> {
    if path.is_file() {
        path.to_string_lossy().hash(hasher);
        let metadata = fs::metadata(path)?;
        let len = metadata.len();
        len.hash(hasher);
        hash_file_content(hasher, path, len)?;
        return Ok(());
    }

    if path.is_dir() {
        let mut entries = fs::read_dir(path)?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            entry.to_string_lossy().hash(hasher);
            if entry.is_file() {
                let metadata = fs::metadata(&entry)?;
                let len = metadata.len();
                len.hash(hasher);
                hash_file_content(hasher, &entry, len)?;
            } else if recursive && entry.is_dir() {
                hash_path_state(hasher, &entry, true)?;
            }
        }
    }

    Ok(())
}

fn prime_watch_fingerprint_if_needed(mission: &mut Mission) -> Result<()> {
    if mission.watch_path.is_some() && mission.watch_fingerprint.is_none() {
        if let Some(path) = resolved_watch_path(mission) {
            mission.watch_fingerprint = Some(fingerprint_path(&path, mission.watch_recursive)?);
        }
    }
    Ok(())
}

pub(crate) fn file_change_ready(state: &AppState, mission: &mut Mission) -> Result<bool> {
    if mission.watch_path.is_none() {
        return Ok(false);
    }
    let Some(path) = resolved_watch_path(mission) else {
        return Ok(false);
    };
    let next = fingerprint_path(&path, mission.watch_recursive)?;
    if mission.watch_fingerprint.is_none() {
        mission.watch_fingerprint = Some(next);
        state.storage.upsert_mission(mission)?;
        return Ok(false);
    }
    if mission.watch_fingerprint.as_deref() == Some(next.as_str()) {
        return Ok(false);
    }
    mission.watch_fingerprint = Some(next);
    mission.updated_at = Utc::now();
    state.storage.upsert_mission(mission)?;
    Ok(true)
}

fn mission_ready_now(state: &AppState, mission: &mut Mission, now: DateTime<Utc>) -> Result<bool> {
    if mission.status.is_terminal() || matches!(mission.status, MissionStatus::Blocked) {
        return Ok(false);
    }
    if mission.watch_path.is_some() {
        return file_change_ready(state, mission);
    }
    if let Some(wake_at) = mission.wake_at {
        return Ok(wake_at <= now);
    }
    Ok(matches!(
        mission.status,
        MissionStatus::Queued | MissionStatus::Running | MissionStatus::Waiting
    ))
}

fn collect_runnable_missions(
    state: &AppState,
    now: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<Mission>> {
    let mut missions = state.storage.list_missions()?;
    missions.sort_by(|left, right| left.updated_at.cmp(&right.updated_at));
    let mut runnable = Vec::new();
    for mut mission in missions {
        if mission_ready_now(state, &mut mission, now)? {
            runnable.push(mission);
        }
        if runnable.len() >= limit {
            break;
        }
    }
    Ok(runnable)
}

pub(crate) async fn autopilot_loop(state: AppState) {
    let mut in_flight = JoinSet::new();
    let mut in_flight_ids = HashSet::new();
    loop {
        while let Some(result) = in_flight.try_join_next() {
            handle_in_flight_result(&state, &mut in_flight_ids, result);
        }

        if let Err(error) = poll_inbox_connectors(&state).await {
            let _ = append_log(
                &state,
                "warn",
                "inboxes",
                format!("inbox polling failed: {}", error.message),
            );
        }
        let config = state.config.read().await.clone();
        if !matches!(config.autopilot.state, agent_core::AutopilotState::Enabled) {
            tokio::select! {
                Some(result) = in_flight.join_next(), if !in_flight_ids.is_empty() => {
                    handle_in_flight_result(&state, &mut in_flight_ids, result);
                }
                _ = state.autopilot_wake.notified() => {}
                _ = sleep(Duration::from_secs(30)) => {}
            }
            continue;
        }

        let available_slots = usize::from(config.autopilot.max_concurrent_missions.max(1))
            .saturating_sub(in_flight_ids.len());

        match collect_runnable_missions(&state, Utc::now(), available_slots.max(1)) {
            Ok(missions) if missions.is_empty() || available_slots == 0 => {
                tokio::select! {
                    Some(result) = in_flight.join_next(), if !in_flight_ids.is_empty() => {
                        handle_in_flight_result(&state, &mut in_flight_ids, result);
                    }
                    _ = state.autopilot_wake.notified() => {}
                    _ = sleep(Duration::from_secs(config.autopilot.wake_interval_seconds.max(5))) => {}
                }
            }
            Ok(missions) => {
                let missions = missions
                    .into_iter()
                    .filter(|mission| !in_flight_ids.contains(&mission.id))
                    .take(available_slots)
                    .collect::<Vec<_>>();
                for mission in missions {
                    let mission_id = mission.id.clone();
                    let state = state.clone();
                    in_flight_ids.insert(mission_id);
                    in_flight.spawn(async move {
                        let mission_id = mission.id.clone();
                        let title = mission.title.clone();
                        let result = run_mission_cycle(&state, mission)
                            .await
                            .map_err(|error| error.to_string());
                        (mission_id, title, result)
                    });
                }
            }
            Err(error) => {
                let _ = append_log(
                    &state,
                    "warn",
                    "autopilot",
                    format!("failed to load missions: {error:#}"),
                );
                sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

fn handle_in_flight_result(
    state: &AppState,
    in_flight_ids: &mut HashSet<String>,
    result: std::result::Result<
        (String, String, std::result::Result<(), String>),
        tokio::task::JoinError,
    >,
) {
    match result {
        Ok((mission_id, title, Err(error))) => {
            in_flight_ids.remove(&mission_id);
            let _ = append_log(
                state,
                "warn",
                "autopilot",
                format!("mission cycle failed for '{title}': {error}"),
            );
        }
        Ok((mission_id, _, Ok(()))) => {
            in_flight_ids.remove(&mission_id);
        }
        Err(error) => {
            let _ = append_log(
                state,
                "warn",
                "autopilot",
                format!("mission task join failed: {error}"),
            );
        }
    }
}

async fn run_mission_cycle(state: &AppState, mut mission: Mission) -> Result<()> {
    let (alias, provider) = resolve_alias_and_provider(state, mission.alias.as_deref())
        .await
        .map_err(|error| anyhow!(error.message))?;
    let autopilot = {
        let config = state.config.read().await;
        config.autopilot.clone()
    };
    let checkpoints = state.storage.list_mission_checkpoints(&mission.id, 12)?;
    maybe_rotate_mission_session(state, &mut mission, &checkpoints)?;
    let checkpoints = state.storage.list_mission_checkpoints(&mission.id, 12)?;
    mission.status = MissionStatus::Running;
    mission.updated_at = Utc::now();
    mission.alias = Some(alias.alias.clone());
    mission.phase.get_or_insert(MissionPhase::Planner);
    mission.wake_trigger = Some(if mission.watch_path.is_some() {
        WakeTrigger::FileChange
    } else if mission.wake_at.is_some() {
        WakeTrigger::Timer
    } else {
        WakeTrigger::Manual
    });
    state.storage.upsert_mission(&mission)?;

    let prompt = build_mission_prompt(&mission, &checkpoints);
    let output_schema = if mission.evolve {
        EVOLVE_DIRECTIVE_SCHEMA
    } else {
        AUTOPILOT_DIRECTIVE_SCHEMA
    };
    let response = execute_task_request(
        state,
        &alias,
        &provider,
        TaskRequestInput {
            prompt,
            requested_model: mission.requested_model.clone(),
            session_id: mission.session_id.clone(),
            cwd: mission.workspace_key.as_ref().map(PathBuf::from),
            thinking_level: None,
            attachments: Vec::new(),
            permission_preset: Some(PermissionPreset::FullAuto),
            output_schema_json: Some(output_schema.to_string()),
            persist: true,
            background: true,
            delegation_depth: 0,
        },
    )
    .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            persist_execution_failure(state, &mut mission, &autopilot, &error.message).await?;
            return Err(anyhow!(error.message));
        }
    };

    let directive = parse_mission_directive(
        response
            .structured_output_json
            .as_deref()
            .unwrap_or(&response.response),
    );
    mission.session_id = Some(response.session_id.clone());
    mission.requested_model = Some(response.model.clone());
    mission.updated_at = Utc::now();
    mission.phase = directive
        .next_phase
        .clone()
        .or(mission.phase.clone())
        .or(Some(MissionPhase::Planner));
    if let Some(handoff_summary) = directive.handoff_summary.clone() {
        mission.handoff_summary = Some(handoff_summary);
    }
    mission.last_error = directive.error.clone();
    let follow_up = schedule_follow_up_mission(state, &mission, &directive)?;

    if let Some(next_seconds) = directive.next_wake_seconds {
        mission.wake_at = Some(Utc::now() + chrono::Duration::seconds(next_seconds as i64));
        mission.scheduled_for_at = mission.wake_at;
        mission.wake_trigger = Some(WakeTrigger::Timer);
    } else {
        mission.wake_at = None;
        mission.scheduled_for_at = None;
    }

    mission.status = directive.status.clone().unwrap_or({
        if mission.retries >= mission.max_retries {
            MissionStatus::Failed
        } else {
            MissionStatus::Waiting
        }
    });

    if matches!(
        mission.status,
        MissionStatus::Running | MissionStatus::Queued
    ) && mission.wake_at.is_none()
    {
        mission.status = MissionStatus::Waiting;
        if mission.watch_path.is_some() {
            mission.wake_at = None;
            mission.scheduled_for_at = None;
            mission.wake_trigger = Some(WakeTrigger::FileChange);
        } else {
            mission.wake_at = Some(
                Utc::now() + chrono::Duration::seconds(autopilot.wake_interval_seconds as i64),
            );
            mission.scheduled_for_at = mission.wake_at;
            mission.wake_trigger = Some(WakeTrigger::Timer);
        }
    }

    if matches!(mission.status, MissionStatus::Failed) && mission.retries < mission.max_retries {
        schedule_retry(&mut mission, &autopilot);
    } else if matches!(mission.status, MissionStatus::Failed) && mission.watch_path.is_some() {
        mission.status = MissionStatus::Waiting;
        mission.wake_at = None;
        mission.scheduled_for_at = None;
        mission.wake_trigger = Some(WakeTrigger::FileChange);
        mission.retries = 0;
    }

    if mission.repeat_interval_seconds.is_some()
        && !matches!(
            mission.status,
            MissionStatus::Cancelled | MissionStatus::Blocked
        )
        && mission.watch_path.is_none()
        && mission.wake_at.is_none()
    {
        schedule_next_repeat_run(&mut mission);
        mission.retries = 0;
    }

    if mission.watch_path.is_some()
        && matches!(
            mission.status,
            MissionStatus::Waiting | MissionStatus::Scheduled
        )
        && mission.wake_at.is_none()
    {
        mission.wake_trigger = Some(WakeTrigger::FileChange);
        mission.scheduled_for_at = None;
        prime_watch_fingerprint_if_needed(&mut mission)?;
    }

    if mission.evolve {
        let restart_requested =
            handle_evolve_cycle(state, &mut mission, &directive, &response.tool_events).await?;
        if restart_requested {
            append_log(
                state,
                "warn",
                "evolve",
                format!(
                    "evolve cycle requested daemon restart for mission '{}'",
                    mission.title
                ),
            )?;
        }
    }

    state.storage.upsert_mission(&mission)?;
    let mut checkpoint = MissionCheckpoint::new(
        mission.id.clone(),
        mission.status.clone(),
        directive
            .summary
            .unwrap_or_else(|| summarize_tool_output(&response.response)),
    );
    checkpoint.session_id = Some(response.session_id);
    checkpoint.phase = mission.phase.clone();
    checkpoint.handoff_summary = mission.handoff_summary.clone();
    checkpoint.response_excerpt = Some(summarize_tool_output(&response.response));
    checkpoint.next_wake_at = mission.wake_at;
    checkpoint.scheduled_for_at = mission.scheduled_for_at;
    state.storage.save_mission_checkpoint(&checkpoint)?;
    if mission.evolve && mission.wake_at.is_some() {
        let config = state.config.read().await;
        if config.evolve.pending_restart {
            drop(config);
            request_daemon_restart(state)?;
        }
    }
    append_log(
        state,
        "info",
        "autopilot",
        match follow_up {
            Some(ref child) => format!(
                "mission '{}' advanced to {:?} and queued follow-up '{}'",
                mission.title, mission.status, child.title
            ),
            None => format!(
                "mission '{}' advanced to {:?}",
                mission.title, mission.status
            ),
        },
    )?;
    Ok(())
}

fn schedule_follow_up_mission(
    state: &AppState,
    mission: &Mission,
    directive: &MissionDirective,
) -> Result<Option<Mission>> {
    let details = directive
        .follow_up_details
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(details) = details else {
        return Ok(None);
    };

    let title = directive
        .follow_up_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("Follow up: {}", mission.title));
    let mut follow_up = Mission::new(title, details.to_string());
    follow_up.alias = mission.alias.clone();
    follow_up.requested_model = mission.requested_model.clone();
    follow_up.workspace_key = mission.workspace_key.clone();
    follow_up.session_id = None;
    follow_up.max_retries = mission.max_retries;
    follow_up.phase = Some(MissionPhase::Planner);

    if let Some(after_seconds) = directive.follow_up_after_seconds {
        follow_up.status = MissionStatus::Scheduled;
        follow_up.wake_trigger = Some(WakeTrigger::FollowUp);
        follow_up.wake_at = Some(Utc::now() + chrono::Duration::seconds(after_seconds as i64));
        follow_up.scheduled_for_at = follow_up.wake_at;
    } else {
        follow_up.status = MissionStatus::Queued;
        follow_up.wake_trigger = Some(WakeTrigger::FollowUp);
        follow_up.scheduled_for_at = None;
    }

    state.storage.upsert_mission(&follow_up)?;
    state.autopilot_wake.notify_waiters();
    Ok(Some(follow_up))
}

async fn handle_evolve_cycle(
    state: &AppState,
    mission: &mut Mission,
    directive: &MissionDirective,
    tool_events: &[ToolExecutionRecord],
) -> Result<bool> {
    let mut config = state.config.write().await;

    if !evolve_verification_ran(tool_events) {
        mission.status = MissionStatus::Failed;
        mission.last_error =
            Some("Evolve cycle did not run verification commands before finishing".to_string());
        config.evolve.state = EvolveState::Failed;
        config.evolve.last_summary = mission.last_error.clone();
        state.storage.save_config(&config)?;
        return Ok(false);
    }

    // Require a diff review when the cycle mutated files.
    let mutated = tool_events.iter().any(tool_event_mutated_workspace);
    if mutated && config.evolve.diff_review_required {
        let has_diff_review = directive
            .diff_summary
            .as_deref()
            .map(str::trim)
            .is_some_and(|s| !s.is_empty());
        if !has_diff_review {
            mission.status = MissionStatus::Failed;
            mission.last_error = Some(
                "Evolve cycle mutated files but did not include a diff_summary reviewing the changes"
                    .to_string(),
            );
            config.evolve.state = EvolveState::Failed;
            config.evolve.last_summary = mission.last_error.clone();
            state.storage.save_config(&config)?;
            return Ok(false);
        }
    }

    config.evolve.iteration = config.evolve.iteration.saturating_add(1);
    config.evolve.last_goal = directive.improvement_goal.clone();
    config.evolve.last_summary = directive.summary.clone();
    config.evolve.last_verified_at = Some(Utc::now());
    config.evolve.pending_restart = directive.restart_required.unwrap_or(false)
        && tool_events.iter().any(tool_event_mutated_workspace);

    let stop_requested = directive.continue_evolving == Some(false)
        || matches!(
            mission.status,
            MissionStatus::Completed | MissionStatus::Cancelled
        );

    if stop_requested {
        if !matches!(
            mission.status,
            MissionStatus::Failed | MissionStatus::Cancelled
        ) {
            mission.status = MissionStatus::Completed;
            mission.wake_at = None;
            mission.scheduled_for_at = None;
        }
        config.evolve.state = if matches!(mission.status, MissionStatus::Failed) {
            EvolveState::Failed
        } else {
            EvolveState::Completed
        };
        config.evolve.current_mission_id = None;
        config.autonomy.state = AutonomyState::Disabled;
        config.autonomy.mode = AutonomyMode::Assisted;
    } else {
        config.evolve.state = EvolveState::Running;
        config.evolve.current_mission_id = Some(mission.id.clone());
        mission.status = MissionStatus::Scheduled;
        mission.wake_trigger = Some(WakeTrigger::Timer);
        mission.wake_at = Some(
            Utc::now()
                + chrono::Duration::seconds(
                    directive
                        .next_wake_seconds
                        .unwrap_or_else(|| evolve_default_wake_seconds(&config.evolve))
                        as i64,
                ),
        );
        mission.scheduled_for_at = mission.wake_at;
    }

    let restart_requested = config.evolve.pending_restart && !stop_requested;
    state.storage.save_config(&config)?;
    Ok(restart_requested)
}

fn evolve_default_wake_seconds(config: &EvolveConfig) -> u64 {
    match config.stop_policy {
        EvolveStopPolicy::AgentDecides => 20,
        EvolveStopPolicy::BudgetFriendly => 90,
    }
}

fn evolve_verification_ran(tool_events: &[ToolExecutionRecord]) -> bool {
    tool_events.iter().any(|event| {
        if event.name != "run_shell" {
            return false;
        }
        let haystack = format!("{} {}", event.arguments, event.output).to_ascii_lowercase();
        ["cargo check", "cargo test", "cargo clippy", "cargo build"]
            .iter()
            .any(|needle| haystack.contains(needle))
    })
}

fn tool_event_mutated_workspace(event: &ToolExecutionRecord) -> bool {
    matches!(
        event.name.as_str(),
        "apply_patch"
            | "write_file"
            | "append_file"
            | "replace_in_file"
            | "make_dir"
            | "copy_path"
            | "move_path"
            | "delete_path"
    )
}

async fn persist_execution_failure(
    state: &AppState,
    mission: &mut Mission,
    autopilot: &AutopilotConfig,
    error_message: &str,
) -> Result<()> {
    mission.updated_at = Utc::now();
    mission.last_error = Some(normalize_memory_sentence(error_message));

    if mission.retries < mission.max_retries {
        schedule_retry(mission, autopilot);
    } else if mission.repeat_interval_seconds.is_some() {
        if mission.watch_path.is_none() {
            schedule_next_repeat_run(mission);
            mission.retries = 0;
        } else {
            mission.status = MissionStatus::Waiting;
            mission.wake_at = None;
            mission.scheduled_for_at = None;
            mission.wake_trigger = Some(WakeTrigger::FileChange);
            mission.retries = 0;
        }
    } else if mission.watch_path.is_some() {
        mission.status = MissionStatus::Waiting;
        mission.wake_at = None;
        mission.scheduled_for_at = None;
        mission.wake_trigger = Some(WakeTrigger::FileChange);
        mission.retries = 0;
    } else {
        mission.status = MissionStatus::Failed;
        mission.wake_at = None;
        mission.scheduled_for_at = None;
    }

    state.storage.upsert_mission(mission)?;
    let mut checkpoint = MissionCheckpoint::new(
        mission.id.clone(),
        mission.status.clone(),
        format!("Mission execution failed: {}", error_message),
    );
    checkpoint.session_id = mission.session_id.clone();
    checkpoint.phase = mission.phase.clone();
    checkpoint.handoff_summary = mission.handoff_summary.clone();
    checkpoint.next_wake_at = mission.wake_at;
    checkpoint.scheduled_for_at = mission.scheduled_for_at;
    state.storage.save_mission_checkpoint(&checkpoint)?;
    if mission.evolve {
        let mut config = state.config.write().await;
        config.evolve.state = EvolveState::Failed;
        config.evolve.last_summary = mission.last_error.clone();
        config.evolve.current_mission_id = Some(mission.id.clone());
        state.storage.save_config(&config)?;
    }
    Ok(())
}

fn schedule_retry(mission: &mut Mission, autopilot: &AutopilotConfig) {
    mission.retries += 1;
    mission.status = MissionStatus::Scheduled;
    mission.wake_at = Some(next_retry_wake_at(autopilot, mission.retries));
    mission.scheduled_for_at = mission.wake_at;
    mission.wake_trigger = Some(WakeTrigger::Timer);
}

fn next_retry_wake_at(autopilot: &AutopilotConfig, retry_attempt: u32) -> DateTime<Utc> {
    let base_seconds = autopilot.wake_interval_seconds.max(5);
    let exponent = retry_attempt.saturating_sub(1).min(6);
    let multiplier = 1u64 << exponent;
    let delay_seconds = base_seconds.saturating_mul(multiplier).min(3600);
    Utc::now() + chrono::Duration::seconds(delay_seconds as i64)
}

pub(crate) fn build_mission_prompt(mission: &Mission, checkpoints: &[MissionCheckpoint]) -> String {
    if mission.evolve {
        return build_evolve_prompt(mission, checkpoints);
    }
    let mut prompt = format!(
        "You are continuing an autonomous background mission.\n\nMission title: {}\nMission details: {}\n\nAdvance the mission by one concrete step. Use tools when needed. Keep moving until you either finish, hit a blocker, or decide the next wake-up time.\n",
        mission.title, mission.details
    );
    if let Some(phase) = mission.phase.as_ref() {
        prompt.push_str(&format!("\nCurrent phase: {:?}.\n", phase));
    }
    if let Some(summary) = mission
        .handoff_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str(&format!(
            "\nCarry forward this handoff summary from earlier cycles: {}\n",
            summary
        ));
    }
    if let Some(path) = mission.watch_path.as_ref() {
        prompt.push_str(&format!(
            "\nThis mission is attached to a filesystem watch.\nWatched path: {}\nRecursive: {}\nTreat filesystem changes here as the wake condition when you are waiting for more work.\n",
            path.display(),
            mission.watch_recursive
        ));
    }
    if let Some(repeat_interval_seconds) = mission.repeat_interval_seconds {
        let repeat_anchor = mission
            .repeat_anchor_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        prompt.push_str(&format!(
            "\nThis mission is recurring.\nRepeat interval: {} seconds.\nRepeat anchor: {}.\nWhen work for this cycle is complete, summarize the outcome and let the scheduler wake the next cycle unless you need an earlier wake-up.\n",
            repeat_interval_seconds,
            repeat_anchor
        ));
    }
    if let Some(scheduled_for_at) = mission.scheduled_for_at {
        prompt.push_str(&format!(
            "\nCurrent scheduled run time: {}.\n",
            scheduled_for_at.to_rfc3339()
        ));
    }
    if !checkpoints.is_empty() {
        prompt.push_str("\nRecent checkpoints:\n");
        for checkpoint in checkpoints.iter().take(8) {
            prompt.push_str(&format!(
                "- {:?} at {} [{}]: {}\n",
                checkpoint.status,
                checkpoint.created_at.to_rfc3339(),
                checkpoint
                    .phase
                    .as_ref()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_else(|| "Unknown".to_string()),
                checkpoint.summary
            ));
        }
    }
    prompt.push_str(
        "\nReturn a single JSON object only for mission control. Use snake_case fields with this shape:\n{\n  \"status\": \"waiting|blocked|completed|failed|running|scheduled|queued|cancelled\",\n  \"next_wake_seconds\": 300,\n  \"next_phase\": \"planner|executor|reviewer\",\n  \"handoff_summary\": \"condensed context for the next cycle or rotated session\",\n  \"summary\": \"short status summary\",\n  \"error\": \"optional blocker description\",\n  \"follow_up_title\": \"optional next mission title\",\n  \"follow_up_details\": \"optional next mission details\",\n  \"follow_up_after_seconds\": 300\n}\nOnly include next_wake_seconds when the current mission should wake up later. Use follow_up_* only when you want to queue a separate child mission.",
    );
    prompt
}

fn build_evolve_prompt(mission: &Mission, checkpoints: &[MissionCheckpoint]) -> String {
    let mut prompt = format!(
        "You are running the agent's EVOLVE mode. Improve the agent methodically, not by shotgun edits.\n\nPrimary goals in order:\n1. functionality\n2. speed\n3. bug fixes in its own code\n\nCurrent evolve mission: {}\nMission details: {}\n\nRules:\n- Pick one bounded improvement target for this cycle.\n- You may inspect and modify the repo and use subagents if helpful.\n- You must verify each cycle with cargo check, cargo test, cargo clippy, and cargo build unless you can justify a narrower verification scope in verification_summary.\n- Prefer small, reversible changes.\n- Keep a clear handoff_summary for the next cycle.\n- Stop only when you are satisfied there is no clearly worthwhile next improvement, or when the current cycle exposes a blocker.\n",
        mission.title, mission.details
    );
    if let Some(summary) = mission
        .handoff_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str(&format!(
            "\nCarry forward this evolve handoff summary from earlier cycles: {}\n",
            summary
        ));
    }
    if !checkpoints.is_empty() {
        prompt.push_str("\nRecent evolve checkpoints:\n");
        for checkpoint in checkpoints.iter().take(8) {
            prompt.push_str(&format!(
                "- {:?} at {} [{}]: {}\n",
                checkpoint.status,
                checkpoint.created_at.to_rfc3339(),
                checkpoint
                    .phase
                    .as_ref()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_else(|| "Unknown".to_string()),
                checkpoint.summary
            ));
        }
    }
    prompt.push_str(
        "\nBefore finishing this cycle, run `git diff` (or equivalent) to review ALL changes you made. Include the review in the diff_summary field.\n\nReturn a single JSON object only. Use snake_case fields with this shape:\n{\n  \"status\": \"waiting|blocked|completed|failed|running|scheduled|queued|cancelled\",\n  \"next_wake_seconds\": 30,\n  \"next_phase\": \"planner|executor|reviewer\",\n  \"handoff_summary\": \"condensed context for the next evolve cycle\",\n  \"summary\": \"short status summary\",\n  \"error\": \"optional blocker description\",\n  \"continue_evolving\": true,\n  \"improvement_goal\": \"one bounded target for the cycle\",\n  \"verification_summary\": \"what verification you ran and the result\",\n  \"diff_summary\": \"review of all changes made in this cycle and why\",\n  \"restart_required\": false,\n  \"follow_up_title\": \"optional child mission title\",\n  \"follow_up_details\": \"optional child mission details\",\n  \"follow_up_after_seconds\": 300\n}\nSet continue_evolving=false only if you are satisfied or blocked. Always include improvement_goal, verification_summary, and diff_summary in evolve mode.",
    );
    prompt
}

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct MissionDirective {
    #[serde(default)]
    pub(crate) status: Option<MissionStatus>,
    #[serde(default)]
    pub(crate) next_wake_seconds: Option<u64>,
    #[serde(default)]
    pub(crate) next_phase: Option<MissionPhase>,
    #[serde(default)]
    pub(crate) handoff_summary: Option<String>,
    #[serde(default)]
    pub(crate) summary: Option<String>,
    #[serde(default)]
    pub(crate) error: Option<String>,
    #[serde(default)]
    pub(crate) follow_up_title: Option<String>,
    #[serde(default)]
    pub(crate) follow_up_details: Option<String>,
    #[serde(default)]
    pub(crate) follow_up_after_seconds: Option<u64>,
    #[serde(default)]
    pub(crate) continue_evolving: Option<bool>,
    #[serde(default)]
    pub(crate) improvement_goal: Option<String>,
    #[serde(default)]
    pub(crate) verification_summary: Option<String>,
    #[serde(default)]
    pub(crate) restart_required: Option<bool>,
    #[serde(default)]
    pub(crate) diff_summary: Option<String>,
}

pub(crate) fn parse_mission_directive(response: &str) -> MissionDirective {
    if let Ok(mut directive) = serde_json::from_str::<MissionDirective>(response.trim()) {
        normalize_mission_directive(&mut directive);
        return directive;
    }

    let mut directive = parse_legacy_mission_directive(response);
    normalize_mission_directive(&mut directive);
    directive
}

fn parse_legacy_mission_directive(response: &str) -> MissionDirective {
    let mut directive = MissionDirective::default();
    let Some(start) = response.find("[AUTOPILOT]") else {
        return directive;
    };
    let end = response.find("[/AUTOPILOT]").unwrap_or(response.len());
    let block = &response[start + "[AUTOPILOT]".len()..end];
    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "status" => {
                directive.status = match value.to_ascii_lowercase().as_str() {
                    "queued" => Some(MissionStatus::Queued),
                    "running" => Some(MissionStatus::Running),
                    "waiting" => Some(MissionStatus::Waiting),
                    "scheduled" => Some(MissionStatus::Scheduled),
                    "blocked" => Some(MissionStatus::Blocked),
                    "completed" => Some(MissionStatus::Completed),
                    "failed" => Some(MissionStatus::Failed),
                    "cancelled" => Some(MissionStatus::Cancelled),
                    _ => None,
                };
            }
            "next_wake_seconds" => {
                directive.next_wake_seconds = value.parse::<u64>().ok();
            }
            "next_phase" => {
                directive.next_phase = match value.to_ascii_lowercase().as_str() {
                    "planner" => Some(MissionPhase::Planner),
                    "executor" => Some(MissionPhase::Executor),
                    "reviewer" => Some(MissionPhase::Reviewer),
                    _ => None,
                };
            }
            "handoff_summary" => {
                if !value.is_empty() {
                    directive.handoff_summary = Some(value.to_string());
                }
            }
            "summary" => directive.summary = Some(value.to_string()),
            "error" => {
                if !value.is_empty() {
                    directive.error = Some(value.to_string());
                }
            }
            "follow_up_title" => directive.follow_up_title = Some(value.to_string()),
            "follow_up_details" => directive.follow_up_details = Some(value.to_string()),
            "follow_up_after_seconds" => {
                directive.follow_up_after_seconds = value.parse::<u64>().ok();
            }
            "continue_evolving" => {
                directive.continue_evolving = match value.to_ascii_lowercase().as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                };
            }
            "improvement_goal" => {
                if !value.is_empty() {
                    directive.improvement_goal = Some(value.to_string());
                }
            }
            "verification_summary" => {
                if !value.is_empty() {
                    directive.verification_summary = Some(value.to_string());
                }
            }
            "restart_required" => {
                directive.restart_required = match value.to_ascii_lowercase().as_str() {
                    "true" => Some(true),
                    "false" => Some(false),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    directive
}

fn normalize_mission_directive(directive: &mut MissionDirective) {
    directive.handoff_summary = directive
        .handoff_summary
        .take()
        .map(|summary| normalize_memory_sentence(&summary))
        .filter(|summary| !summary.is_empty());
    directive.summary = directive
        .summary
        .take()
        .map(|summary| normalize_memory_sentence(&summary))
        .filter(|summary| !summary.is_empty());
    directive.error = directive
        .error
        .take()
        .map(|error| normalize_memory_sentence(&error))
        .filter(|error| !error.is_empty());
    directive.follow_up_title = directive
        .follow_up_title
        .take()
        .map(|title| normalize_memory_sentence(&title))
        .filter(|title| !title.is_empty());
    directive.follow_up_details = directive
        .follow_up_details
        .take()
        .map(|details| details.trim().to_string())
        .filter(|details| !details.is_empty());
    directive.improvement_goal = directive
        .improvement_goal
        .take()
        .map(|goal| normalize_memory_sentence(&goal))
        .filter(|goal| !goal.is_empty());
    directive.verification_summary = directive
        .verification_summary
        .take()
        .map(|summary| normalize_memory_sentence(&summary))
        .filter(|summary| !summary.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::AppConfig;
    use reqwest::Client;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    fn test_state() -> AppState {
        AppState {
            storage: agent_storage::Storage::open_at(
                std::env::temp_dir().join(format!("agent-missions-test-{}", Uuid::new_v4())),
            )
            .unwrap(),
            config: Arc::new(RwLock::new(AppConfig::default())),
            http_client: Client::new(),
            started_at: Utc::now(),
            shutdown: mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            rate_limiter: crate::ProviderRateLimiter::new(),
        }
    }

    #[tokio::test]
    async fn execution_failure_schedules_retry_before_exhausting_retries() {
        let state = test_state();
        let autopilot = AutopilotConfig::default();
        let mut mission = Mission::new("Retry mission".to_string(), "do work".to_string());
        mission.max_retries = 3;

        persist_execution_failure(&state, &mut mission, &autopilot, "network timeout")
            .await
            .unwrap();

        assert_eq!(mission.status, MissionStatus::Scheduled);
        assert_eq!(mission.retries, 1);
        assert_eq!(mission.wake_trigger, Some(WakeTrigger::Timer));
        assert!(mission.wake_at.is_some());

        let stored = state.storage.get_mission(&mission.id).unwrap().unwrap();
        assert_eq!(stored.status, MissionStatus::Scheduled);
        assert_eq!(stored.retries, 1);
    }

    #[tokio::test]
    async fn execution_failure_on_watch_mission_returns_to_file_change_wait_after_exhaustion() {
        let state = test_state();
        let autopilot = AutopilotConfig::default();
        let mut mission = Mission::new("Watch mission".to_string(), "watch repo".to_string());
        mission.watch_path = Some(PathBuf::from("src"));
        mission.workspace_key = Some(std::env::temp_dir().display().to_string());
        mission.max_retries = 1;
        mission.retries = 1;

        persist_execution_failure(&state, &mut mission, &autopilot, "auth failed")
            .await
            .unwrap();

        assert_eq!(mission.status, MissionStatus::Waiting);
        assert_eq!(mission.retries, 0);
        assert_eq!(mission.wake_trigger, Some(WakeTrigger::FileChange));
        assert!(mission.wake_at.is_none());
    }

    #[test]
    fn parse_mission_directive_reads_follow_up_fields() {
        let directive = parse_mission_directive(
            r#"{
                "status":"completed",
                "next_phase":"reviewer",
                "handoff_summary":"Summarize the execution context",
                "summary":"cycle done",
                "follow_up_title":"Review release notes",
                "follow_up_details":"Draft release notes from the latest commits.",
                "follow_up_after_seconds":120
            }"#,
        );
        assert_eq!(directive.status, Some(MissionStatus::Completed));
        assert_eq!(directive.next_phase, Some(MissionPhase::Reviewer));
        assert_eq!(
            directive.handoff_summary.as_deref(),
            Some("Summarize the execution context")
        );
        assert_eq!(
            directive.follow_up_title.as_deref(),
            Some("Review release notes")
        );
        assert_eq!(
            directive.follow_up_details.as_deref(),
            Some("Draft release notes from the latest commits.")
        );
        assert_eq!(directive.follow_up_after_seconds, Some(120));
    }

    #[test]
    fn schedule_follow_up_mission_queues_child_with_follow_up_trigger() {
        let state = test_state();
        let mut mission =
            Mission::new("Parent mission".to_string(), "Do the main work".to_string());
        mission.alias = Some("main".to_string());
        mission.requested_model = Some("gpt-5.4".to_string());
        mission.workspace_key = Some("J:\\repo".to_string());
        mission.session_id = Some("session-1".to_string());

        let directive = MissionDirective {
            status: Some(MissionStatus::Completed),
            next_wake_seconds: None,
            next_phase: Some(MissionPhase::Reviewer),
            handoff_summary: Some("Carry this forward".to_string()),
            summary: Some("done".to_string()),
            error: None,
            follow_up_title: Some("Child mission".to_string()),
            follow_up_details: Some("Handle the follow-up task".to_string()),
            follow_up_after_seconds: Some(30),
            continue_evolving: None,
            improvement_goal: None,
            verification_summary: None,
            restart_required: None,
            diff_summary: None,
        };

        let follow_up = schedule_follow_up_mission(&state, &mission, &directive)
            .unwrap()
            .unwrap();
        assert_eq!(follow_up.alias.as_deref(), Some("main"));
        assert_eq!(follow_up.requested_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(follow_up.session_id, None);
        assert_eq!(follow_up.phase, Some(MissionPhase::Planner));
        assert_eq!(follow_up.wake_trigger, Some(WakeTrigger::FollowUp));
        assert_eq!(follow_up.status, MissionStatus::Scheduled);
        assert!(follow_up.wake_at.is_some());
        assert_eq!(follow_up.scheduled_for_at, follow_up.wake_at);

        let stored = state.storage.get_mission(&follow_up.id).unwrap().unwrap();
        assert_eq!(stored.title, "Child mission");
        assert_eq!(stored.details, "Handle the follow-up task");
        assert_eq!(stored.wake_trigger, Some(WakeTrigger::FollowUp));
    }

    #[test]
    fn schedule_next_repeat_run_uses_anchor_without_drift() {
        let mut mission = Mission::new("Recurring".to_string(), "Do work".to_string());
        mission.repeat_interval_seconds = Some(60);
        let anchor = Utc::now() - chrono::Duration::minutes(5);
        mission.repeat_anchor_at = Some(anchor);
        mission.scheduled_for_at = Some(anchor);

        schedule_next_repeat_run(&mut mission);

        assert_eq!(mission.status, MissionStatus::Scheduled);
        assert_eq!(mission.wake_trigger, Some(WakeTrigger::Timer));
        assert_eq!(mission.repeat_anchor_at, Some(anchor));
        assert!(mission.wake_at > Some(Utc::now()));
    }
}
