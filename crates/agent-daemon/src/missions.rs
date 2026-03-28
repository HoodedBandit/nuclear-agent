use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
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
use tokio::task::{Id as TaskId, JoinSet};
use tokio::time::sleep;

use crate::{
    append_log, execute_task_request, normalize_memory_sentence, poll_inbox_connectors,
    request_daemon_restart, resolve_alias_and_provider, resolve_request_cwd, summarize_tool_output,
    ApiError, AppState, ExecutionCancellation, LimitQuery, TaskRequestInput,
    AUTOPILOT_DIRECTIVE_SCHEMA,
};

mod prompt;
mod watch;
pub(crate) use prompt::{build_mission_prompt, parse_mission_directive};
use prompt::{MissionDirective, EVOLVE_DIRECTIVE_SCHEMA};
#[cfg(test)]
pub(crate) use watch::file_change_ready;
use watch::{
    collect_runnable_missions, initialize_repeat_schedule, maybe_rotate_mission_session,
    normalize_watch_settings, prime_watch_fingerprint_if_needed, schedule_next_repeat_run,
};

pub(crate) async fn list_missions(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<Mission>>, ApiError> {
    Ok(Json(state.storage.list_missions_limited(
        query.limit.map(|limit| limit.clamp(1, 200)),
    )?))
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
        if let Some(existing) = sync_active_evolve_mission_reference(&state, &mut config)? {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                format!(
                    "evolve already has an active mission '{}' ({})",
                    existing.title, existing.id
                ),
            ));
        }
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
    let (updated, mission_id) = {
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
        (
            config.evolve.clone(),
            config.evolve.current_mission_id.clone(),
        )
    };
    if let Some(mission_id) = mission_id.as_deref() {
        signal_mission_cancellation(&state, mission_id);
    }
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
    let (updated, mission_id) = {
        let mut config = state.config.write().await;
        let active_mission_id = config.evolve.current_mission_id.clone();
        if let Some(mission_id) = active_mission_id.clone() {
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
        (config.evolve.clone(), active_mission_id)
    };
    if let Some(mission_id) = mission_id.as_deref() {
        signal_mission_cancellation(&state, mission_id);
    }
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
    signal_mission_cancellation(&state, &mission.id);
    reconcile_blocked_evolve_state(&state, &mission).await?;
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
    ensure_no_other_active_evolve_mission(&state, &mission)?;
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
    reconcile_resumed_evolve_state(&state, &mission).await?;
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
    mission.wake_at = None;
    mission.scheduled_for_at = None;
    mission.last_error = Some("Mission cancelled by operator".to_string());
    state.storage.upsert_mission(&mission)?;
    signal_mission_cancellation(&state, &mission.id);
    reconcile_cancelled_evolve_state(&state, &mission).await?;
    state
        .storage
        .save_mission_checkpoint(&MissionCheckpoint::new(
            mission.id.clone(),
            mission.status.clone(),
            "mission cancelled".to_string(),
        ))?;
    Ok(Json(mission))
}

async fn reconcile_cancelled_evolve_state(state: &AppState, mission: &Mission) -> Result<()> {
    if !mission.evolve {
        return Ok(());
    }

    let mut config = state.config.write().await;
    if config.evolve.current_mission_id.as_deref() != Some(mission.id.as_str()) {
        return Ok(());
    }

    config.evolve.state = EvolveState::Completed;
    config.evolve.current_mission_id = None;
    config.evolve.pending_restart = false;
    config.evolve.last_summary = mission.last_error.clone();
    config.autonomy.state = AutonomyState::Disabled;
    config.autonomy.mode = AutonomyMode::Assisted;
    state.storage.save_config(&config)?;
    Ok(())
}

async fn reconcile_blocked_evolve_state(state: &AppState, mission: &Mission) -> Result<()> {
    if !mission.evolve {
        return Ok(());
    }

    let mut config = state.config.write().await;
    if config.evolve.current_mission_id.as_deref() != Some(mission.id.as_str()) {
        return Ok(());
    }

    config.evolve.state = EvolveState::Paused;
    config.evolve.alias = mission.alias.clone();
    config.evolve.requested_model = mission.requested_model.clone();
    config.autonomy.mode = AutonomyMode::Evolve;
    config.autonomy.state = AutonomyState::Paused;
    state.storage.save_config(&config)?;
    Ok(())
}

async fn reconcile_resumed_evolve_state(state: &AppState, mission: &Mission) -> Result<()> {
    if !mission.evolve {
        return Ok(());
    }

    let mut config = state.config.write().await;
    config.evolve.state = EvolveState::Running;
    config.evolve.current_mission_id = Some(mission.id.clone());
    config.evolve.alias = mission.alias.clone();
    config.evolve.requested_model = mission.requested_model.clone();
    config.autonomy.mode = AutonomyMode::Evolve;
    config.autonomy.state = AutonomyState::Enabled;
    config.autonomy.unlimited_usage = true;
    config.autonomy.full_network = true;
    config.autonomy.allow_self_edit = true;
    state.storage.save_config(&config)?;
    Ok(())
}

async fn sync_controlled_mission_state(state: &AppState, mission: &mut Mission) -> Result<bool> {
    let Some(latest) = state.storage.get_mission(&mission.id)? else {
        return Ok(false);
    };
    if !matches!(
        latest.status,
        MissionStatus::Cancelled | MissionStatus::Blocked
    ) {
        return Ok(false);
    };

    *mission = latest;
    if matches!(mission.status, MissionStatus::Cancelled) {
        reconcile_cancelled_evolve_state(state, mission).await?;
    } else {
        reconcile_blocked_evolve_state(state, mission).await?;
    }
    Ok(true)
}

fn mission_is_terminal(status: &MissionStatus) -> bool {
    matches!(
        status,
        MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
    )
}

fn evolve_state_for_mission_status(status: &MissionStatus) -> EvolveState {
    if matches!(status, MissionStatus::Blocked) {
        EvolveState::Paused
    } else {
        EvolveState::Running
    }
}

fn ensure_no_other_active_evolve_mission(
    state: &AppState,
    mission: &Mission,
) -> Result<(), ApiError> {
    if !mission.evolve {
        return Ok(());
    }

    let other_active = state
        .storage
        .list_missions()?
        .into_iter()
        .find(|candidate| {
            candidate.id != mission.id
                && candidate.evolve
                && !mission_is_terminal(&candidate.status)
        });
    if let Some(other) = other_active {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "another evolve mission '{}' ({}) is already active",
                other.title, other.id
            ),
        ));
    }
    Ok(())
}

fn sync_active_evolve_mission_reference(
    state: &AppState,
    config: &mut agent_core::AppConfig,
) -> Result<Option<Mission>, ApiError> {
    if let Some(mission_id) = config.evolve.current_mission_id.clone() {
        if let Some(mission) = state.storage.get_mission(&mission_id)? {
            if mission.evolve && !mission_is_terminal(&mission.status) {
                config.evolve.state = evolve_state_for_mission_status(&mission.status);
                config.evolve.alias = mission.alias.clone();
                config.evolve.requested_model = mission.requested_model.clone();
                return Ok(Some(mission));
            }
        }
    }

    let latest_active = state
        .storage
        .list_missions()?
        .into_iter()
        .filter(|mission| mission.evolve && !mission_is_terminal(&mission.status))
        .max_by_key(|mission| mission.updated_at);

    if let Some(mission) = latest_active.clone() {
        config.evolve.current_mission_id = Some(mission.id.clone());
        config.evolve.state = evolve_state_for_mission_status(&mission.status);
        config.evolve.alias = mission.alias.clone();
        config.evolve.requested_model = mission.requested_model.clone();
        return Ok(Some(mission));
    }

    config.evolve.current_mission_id = None;
    Ok(None)
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

fn normalized_repeat_interval(value: Option<u64>) -> Option<u64> {
    value.filter(|seconds| *seconds > 0)
}

type MissionTaskResult = (String, String, std::result::Result<(), String>);
type InFlightJoinResult = std::result::Result<(TaskId, MissionTaskResult), tokio::task::JoinError>;

fn mission_cancellation(state: &AppState, mission_id: &str) -> ExecutionCancellation {
    let mut cancellations = state
        .mission_cancellations
        .lock()
        .expect("mission cancellation lock poisoned");
    cancellations
        .entry(mission_id.to_string())
        .or_default()
        .clone()
}

fn signal_mission_cancellation(state: &AppState, mission_id: &str) {
    let cancellation = {
        let cancellations = state
            .mission_cancellations
            .lock()
            .expect("mission cancellation lock poisoned");
        cancellations.get(mission_id).cloned()
    };
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }
}

fn clear_mission_cancellation(state: &AppState, mission_id: &str) {
    let mut cancellations = state
        .mission_cancellations
        .lock()
        .expect("mission cancellation lock poisoned");
    cancellations.remove(mission_id);
}

pub(crate) async fn autopilot_loop(state: AppState) {
    let mut in_flight = JoinSet::new();
    let mut in_flight_ids = HashSet::new();
    let mut task_ids = HashMap::new();
    loop {
        while let Some(result) = in_flight.try_join_next_with_id() {
            handle_in_flight_result(&state, &mut in_flight_ids, &mut task_ids, result);
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
                Some(result) = in_flight.join_next_with_id(), if !in_flight_ids.is_empty() => {
                    handle_in_flight_result(&state, &mut in_flight_ids, &mut task_ids, result);
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
                    Some(result) = in_flight.join_next_with_id(), if !in_flight_ids.is_empty() => {
                        handle_in_flight_result(&state, &mut in_flight_ids, &mut task_ids, result);
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
                    let cancellation = mission_cancellation(&state, &mission_id);
                    let state = state.clone();
                    in_flight_ids.insert(mission_id.clone());
                    let handle = in_flight.spawn(async move {
                        let mission_id = mission.id.clone();
                        let title = mission.title.clone();
                        let result = run_mission_cycle(&state, mission, cancellation)
                            .await
                            .map_err(|error| error.to_string());
                        (mission_id, title, result)
                    });
                    task_ids.insert(handle.id(), mission_id);
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
    task_ids: &mut HashMap<TaskId, String>,
    result: InFlightJoinResult,
) {
    match result {
        Ok((task_id, (mission_id, title, Err(error)))) => {
            task_ids.remove(&task_id);
            in_flight_ids.remove(&mission_id);
            clear_mission_cancellation(state, &mission_id);
            let _ = append_log(
                state,
                "warn",
                "autopilot",
                format!("mission cycle failed for '{title}': {error}"),
            );
        }
        Ok((task_id, (mission_id, _, Ok(())))) => {
            task_ids.remove(&task_id);
            in_flight_ids.remove(&mission_id);
            clear_mission_cancellation(state, &mission_id);
        }
        Err(error) => {
            if let Some(mission_id) = task_ids.remove(&error.id()) {
                in_flight_ids.remove(&mission_id);
                clear_mission_cancellation(state, &mission_id);
            }
            let _ = append_log(
                state,
                "warn",
                "autopilot",
                format!("mission task join failed: {error}"),
            );
        }
    }
}

async fn run_mission_cycle(
    state: &AppState,
    mut mission: Mission,
    cancellation: ExecutionCancellation,
) -> Result<()> {
    if sync_controlled_mission_state(state, &mut mission).await? {
        return Ok(());
    }
    let (alias, provider) = resolve_alias_and_provider(state, mission.alias.as_deref())
        .await
        .map_err(|error| anyhow!(error.message))?;
    if sync_controlled_mission_state(state, &mut mission).await? {
        return Ok(());
    }
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
            task_mode: None,
            output_schema_json: Some(output_schema.to_string()),
            persist: true,
            background: true,
            delegation_depth: 0,
            cancellation: Some(cancellation),
        },
    )
    .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            if sync_controlled_mission_state(state, &mut mission).await? {
                return Ok(());
            }
            persist_execution_failure(state, &mut mission, &autopilot, &error.message).await?;
            return Err(anyhow!(error.message));
        }
    };
    if sync_controlled_mission_state(state, &mut mission).await? {
        return Ok(());
    }

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

    if sync_controlled_mission_state(state, &mut mission).await? {
        return Ok(());
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
    if sync_controlled_mission_state(state, mission).await? {
        return Ok(());
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::AppConfig;
    use reqwest::Client;
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };
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
            browser_auth_sessions: crate::new_browser_auth_store(),
            dashboard_sessions: crate::new_dashboard_session_store(),
            dashboard_launches: crate::new_dashboard_launch_store(),
            mission_cancellations: crate::new_mission_cancellation_store(),
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

    #[tokio::test]
    async fn handle_in_flight_result_removes_join_error_slot() {
        let state = test_state();
        let mut in_flight = JoinSet::new();
        let mut in_flight_ids = HashSet::from(["mission-1".to_string()]);
        let mut task_ids = HashMap::new();

        let handle = in_flight.spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            ("mission-1".to_string(), "Mission".to_string(), Ok(()))
        });
        task_ids.insert(handle.id(), "mission-1".to_string());
        handle.abort();

        let result = in_flight.join_next_with_id().await.unwrap();
        assert!(result.is_err());
        handle_in_flight_result(&state, &mut in_flight_ids, &mut task_ids, result);

        assert!(!in_flight_ids.contains("mission-1"));
        assert!(task_ids.is_empty());
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
