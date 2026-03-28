use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path as FsPath, PathBuf},
};

use agent_core::{Mission, MissionCheckpoint, MissionStatus, WakeTrigger};
use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{normalize_memory_sentence, AppState};

pub(super) fn normalize_watch_settings(mission: &mut Mission) {
    if mission.watch_path.is_none() {
        mission.watch_recursive = false;
        mission.watch_fingerprint = None;
    } else {
        mission.repeat_interval_seconds = None;
        mission.repeat_anchor_at = None;
        mission.scheduled_for_at = None;
    }
}

pub(super) fn initialize_repeat_schedule(
    mission: &mut Mission,
    requested_wake_at: Option<DateTime<Utc>>,
) {
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

pub(super) fn schedule_next_repeat_run(mission: &mut Mission) {
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

pub(super) fn maybe_rotate_mission_session(
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

const CONTENT_HASH_FULL_THRESHOLD: u64 = 1_048_576;
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
        use std::io::Seek;

        let mut head = vec![0u8; CONTENT_HASH_PARTIAL_SIZE];
        file.read_exact(&mut head)?;
        head.hash(hasher);

        let tail_offset = len.saturating_sub(CONTENT_HASH_PARTIAL_SIZE as u64);
        file.seek(std::io::SeekFrom::Start(tail_offset))?;
        let mut tail = vec![0u8; CONTENT_HASH_PARTIAL_SIZE];
        let count = file.read(&mut tail)?;
        tail[..count].hash(hasher);

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

pub(super) fn prime_watch_fingerprint_if_needed(mission: &mut Mission) -> Result<()> {
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

pub(super) fn collect_runnable_missions(
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
