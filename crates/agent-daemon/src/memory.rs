use std::{collections::HashSet, path::Path as FsPath};

use crate::{append_log, ApiError, AppState, LimitQuery, SkillDraftQuery};
use agent_core::{
    MemoryKind, MemoryRebuildRequest, MemoryRebuildResponse, MemoryRecord, MemoryReviewStatus,
    MemoryReviewUpdateRequest, MemoryScope, MemorySearchQuery, MemorySearchResponse,
    MemoryUpsertRequest, SessionTranscript, SkillDraft, SkillDraftStatus,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
mod types;
use types::{MemoryConflictResolution, MemoryRebuildStats};
mod guidance;
pub(crate) use guidance::load_enabled_skill_guidance;
use guidance::{env_value, find_git_root};
mod embeddings;
mod learning;
use chrono::Utc;
use embeddings::{embedding_search, maybe_compute_embedding};
pub(crate) use learning::learn_from_interaction;
#[cfg(test)]
use learning::{
    extract_memory_candidates_from_source, learning_observations_from_session_messages,
    MemoryObservation,
};
use learning::{rebuild_memory_from_transcript, summarize_preview};
use tracing::warn;

pub(crate) async fn list_skill_drafts(
    State(state): State<AppState>,
    Query(query): Query<SkillDraftQuery>,
) -> Result<Json<Vec<SkillDraft>>, ApiError> {
    Ok(Json(state.storage.list_skill_drafts(
        query.limit.unwrap_or(25).clamp(1, 100),
        query.status,
        None,
        None,
    )?))
}

pub(crate) async fn get_skill_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<SkillDraft>, ApiError> {
    let draft = state
        .storage
        .get_skill_draft(&draft_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown skill draft"))?;
    Ok(Json(draft))
}

pub(crate) async fn publish_skill_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<SkillDraft>, ApiError> {
    let mut draft = state
        .storage
        .get_skill_draft(&draft_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown skill draft"))?;
    draft.status = SkillDraftStatus::Published;
    draft.updated_at = Utc::now();
    state.storage.upsert_skill_draft(&draft)?;
    append_log(
        &state,
        "info",
        "skills",
        format!("published skill draft '{}'", draft.title),
    )?;
    Ok(Json(draft))
}

pub(crate) async fn reject_skill_draft(
    State(state): State<AppState>,
    Path(draft_id): Path<String>,
) -> Result<Json<SkillDraft>, ApiError> {
    let mut draft = state
        .storage
        .get_skill_draft(&draft_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown skill draft"))?;
    draft.status = SkillDraftStatus::Rejected;
    draft.updated_at = Utc::now();
    state.storage.upsert_skill_draft(&draft)?;
    append_log(
        &state,
        "warn",
        "skills",
        format!("rejected skill draft '{}'", draft.title),
    )?;
    Ok(Json(draft))
}

pub(crate) async fn list_memories(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MemoryRecord>>, ApiError> {
    Ok(Json(
        state.storage.list_memories(query.limit.unwrap_or(50))?,
    ))
}

pub(crate) async fn list_memory_review_queue(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MemoryRecord>>, ApiError> {
    Ok(Json(state.storage.list_memories_by_review_status(
        MemoryReviewStatus::Candidate,
        query.limit.unwrap_or(50).clamp(1, 100),
    )?))
}

pub(crate) async fn list_profile_memories(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<MemoryRecord>>, ApiError> {
    let limit = query.limit.unwrap_or(25).clamp(1, 100);
    let mut seen = HashSet::new();
    let mut memories = state
        .storage
        .list_memories_by_tag("system_profile", limit, None, None)?;
    memories.extend(
        state
            .storage
            .list_memories_by_tag("workspace_profile", limit, None, None)?,
    );
    memories.retain(|memory| seen.insert(memory.id.clone()));
    memories.sort_by_key(|memory| std::cmp::Reverse(memory.updated_at));
    memories.truncate(limit);
    Ok(Json(memories))
}

pub(crate) async fn upsert_memory(
    State(state): State<AppState>,
    Json(payload): Json<MemoryUpsertRequest>,
) -> Result<Json<MemoryRecord>, ApiError> {
    let subject = payload.subject.trim();
    let content = payload.content.trim();
    if subject.is_empty() || content.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "memory subject and content are required",
        ));
    }

    let mut memory = MemoryRecord::new(
        payload.kind,
        payload.scope,
        subject.to_string(),
        content.to_string(),
    );
    memory.confidence = payload.confidence.unwrap_or(100).clamp(1, 100);
    memory.updated_at = Utc::now();
    memory.source_session_id = payload.source_session_id;
    memory.source_message_id = payload.source_message_id;
    memory.provider_id = payload.provider_id;
    memory.workspace_key = payload.workspace_key;
    memory.evidence_refs = payload.evidence_refs;
    memory.tags = payload.tags;
    memory.identity_key = payload.identity_key;
    memory.observation_source = payload.observation_source;
    memory.review_status = payload.review_status.unwrap_or(memory.review_status);
    memory.review_note = payload.review_note;
    memory.reviewed_at = payload.reviewed_at;
    memory.supersedes = payload.supersedes;
    let existing = load_conflicting_memories(&state, &memory)?;
    let resolution = resolve_memory_conflicts(&mut memory, existing);
    state.storage.upsert_memory(&memory)?;
    mark_memory_supersessions(&state, &memory, resolution.supersede_ids)?;

    // Compute embedding asynchronously if configured.
    if let Err(err) = maybe_compute_embedding(&state, &memory).await {
        warn!(
            "failed to compute embedding for memory '{}': {err}",
            memory.id
        );
    }

    append_log(
        &state,
        "info",
        "memory",
        format!("memory '{}' saved", memory.subject),
    )?;
    Ok(Json(memory))
}

pub(crate) async fn search_memory(
    State(state): State<AppState>,
    Json(payload): Json<MemorySearchQuery>,
) -> Result<Json<MemorySearchResponse>, ApiError> {
    let limit = payload.limit.unwrap_or(10).clamp(1, 50);
    let (mut memories, transcript_hits) = state.storage.search_memories(
        &payload.query,
        payload.workspace_key.as_deref(),
        payload.provider_id.as_deref(),
        &payload.review_statuses,
        payload.include_superseded,
        limit,
    )?;

    // Supplement with embedding-based semantic search if configured.
    if memories.len() < limit && payload.review_statuses.is_empty() && !payload.include_superseded {
        if let Ok(extra) = embedding_search(
            &state,
            &payload.query,
            payload.workspace_key.as_deref(),
            payload.provider_id.as_deref(),
            limit - memories.len(),
            &memories.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        )
        .await
        {
            memories.extend(extra);
        }
    }

    Ok(Json(MemorySearchResponse {
        memories,
        transcript_hits,
    }))
}

pub(crate) async fn forget_memory(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let forgotten = state.storage.forget_memory(&memory_id)?;
    if !forgotten {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown memory"));
    }
    append_log(
        &state,
        "warn",
        "memory",
        format!("memory '{}' forgotten", memory_id),
    )?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn approve_memory(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
    Json(payload): Json<MemoryReviewUpdateRequest>,
) -> Result<Json<MemoryRecord>, ApiError> {
    if payload.status != MemoryReviewStatus::Accepted {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "approve_memory requires accepted status",
        ));
    }
    let updated = state.storage.update_memory_review_status(
        &memory_id,
        MemoryReviewStatus::Accepted,
        payload.note.as_deref(),
    )?;
    if !updated {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown memory"));
    }
    let memory = state
        .storage
        .get_memory(&memory_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown memory"))?;
    mark_memory_supersessions(&state, &memory, Vec::new())?;
    append_log(
        &state,
        "info",
        "memory",
        format!("memory '{}' approved", memory.subject),
    )?;
    Ok(Json(memory))
}

pub(crate) async fn reject_memory(
    State(state): State<AppState>,
    Path(memory_id): Path<String>,
    Json(payload): Json<MemoryReviewUpdateRequest>,
) -> Result<Json<MemoryRecord>, ApiError> {
    if payload.status != MemoryReviewStatus::Rejected {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "reject_memory requires rejected status",
        ));
    }
    let updated = state.storage.update_memory_review_status(
        &memory_id,
        MemoryReviewStatus::Rejected,
        payload.note.as_deref(),
    )?;
    if !updated {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "unknown memory"));
    }
    let memory = state
        .storage
        .get_memory(&memory_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown memory"))?;
    append_log(
        &state,
        "warn",
        "memory",
        format!("memory '{}' rejected", memory.subject),
    )?;
    Ok(Json(memory))
}

fn workspace_key(cwd: Option<&FsPath>) -> Option<String> {
    cwd.map(|path| path.display().to_string())
}

fn memory_review_rank(status: MemoryReviewStatus) -> u8 {
    match status {
        MemoryReviewStatus::Accepted => 0,
        MemoryReviewStatus::Candidate => 1,
        MemoryReviewStatus::Rejected => 2,
    }
}

fn load_conflicting_memories(
    state: &AppState,
    memory: &MemoryRecord,
) -> Result<Vec<MemoryRecord>, ApiError> {
    let workspace_key = memory.workspace_key.as_deref();
    let provider_id = memory.provider_id.as_deref();
    if let Some(identity_key) = memory
        .identity_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let memories = state.storage.list_active_memories_by_identity_key(
            identity_key,
            workspace_key,
            provider_id,
        )?;
        if !memories.is_empty() {
            return Ok(memories);
        }
    }
    Ok(state
        .storage
        .find_memory_by_subject(&memory.subject, workspace_key, provider_id)?
        .into_iter()
        .collect())
}

fn merge_memory_tags(memory: &mut MemoryRecord, existing: &MemoryRecord) {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for tag in memory.tags.iter().chain(existing.tags.iter()) {
        if seen.insert(tag.clone()) {
            tags.push(tag.clone());
        }
    }
    memory.tags = tags;
}

fn merge_memory_evidence_refs(memory: &mut MemoryRecord, existing: &MemoryRecord) {
    for evidence in &existing.evidence_refs {
        if !memory.evidence_refs.contains(evidence) {
            memory.evidence_refs.push(evidence.clone());
        }
    }
}

fn reinforce_exact_memory(memory: &mut MemoryRecord, existing: &MemoryRecord) {
    let observation_changed = memory.observation_source != existing.observation_source;
    let session_changed = memory.source_session_id != existing.source_session_id;
    if observation_changed || session_changed {
        memory.confidence = memory
            .confidence
            .max(existing.confidence)
            .saturating_add(15)
            .min(100);
        if !memory.tags.iter().any(|tag| tag == "reinforced") {
            memory.tags.push("reinforced".to_string());
        }
        if observation_changed && !memory.tags.iter().any(|tag| tag == "multi_source") {
            memory.tags.push("multi_source".to_string());
        }
        if session_changed && !memory.tags.iter().any(|tag| tag == "cross_session") {
            memory.tags.push("cross_session".to_string());
        }
    } else {
        memory.confidence = memory.confidence.max(existing.confidence);
    }

    let assistant_derived = memory
        .tags
        .iter()
        .any(|tag| tag == "assistant_reply" || tag == "assistant_transcript");
    if matches!(existing.review_status, MemoryReviewStatus::Candidate)
        && (observation_changed || session_changed)
        && !assistant_derived
        && memory.confidence >= 85
    {
        memory.review_status = MemoryReviewStatus::Accepted;
        memory.reviewed_at = Some(Utc::now());
        if memory.review_note.is_none() {
            memory.review_note = Some(
                "Auto-accepted after repeated observations across sessions or sources.".to_string(),
            );
        }
    }
}

fn resolve_memory_conflicts(
    memory: &mut MemoryRecord,
    mut existing: Vec<MemoryRecord>,
) -> MemoryConflictResolution {
    if existing.is_empty() {
        return MemoryConflictResolution::default();
    }
    existing.sort_by(|left, right| {
        memory_review_rank(left.review_status.clone())
            .cmp(&memory_review_rank(right.review_status.clone()))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });

    let normalized_content = normalize_memory_sentence(&memory.content);
    if let Some(exact) = existing
        .iter()
        .find(|existing| normalize_memory_sentence(&existing.content) == normalized_content)
    {
        memory.id = exact.id.clone();
        memory.created_at = exact.created_at;
        memory.updated_at = Utc::now();
        if memory.identity_key.is_none() {
            memory.identity_key = exact.identity_key.clone();
        }
        if memory.observation_source.is_none() {
            memory.observation_source = exact.observation_source.clone();
        }
        if memory.source_session_id.is_none() {
            memory.source_session_id = exact.source_session_id.clone();
        }
        if memory.source_message_id.is_none() {
            memory.source_message_id = exact.source_message_id.clone();
        }
        if memory.provider_id.is_none() {
            memory.provider_id = exact.provider_id.clone();
        }
        if memory.workspace_key.is_none() {
            memory.workspace_key = exact.workspace_key.clone();
        }
        if memory.review_note.is_none() {
            memory.review_note = exact.review_note.clone();
        }
        if memory.reviewed_at.is_none() {
            memory.reviewed_at = exact.reviewed_at;
        }
        if memory.supersedes.is_none() {
            memory.supersedes = exact.supersedes.clone();
        }
        if matches!(exact.review_status, MemoryReviewStatus::Accepted)
            && matches!(memory.review_status, MemoryReviewStatus::Candidate)
        {
            memory.review_status = MemoryReviewStatus::Accepted;
        }
        merge_memory_tags(memory, exact);
        merge_memory_evidence_refs(memory, exact);
        reinforce_exact_memory(memory, exact);
        return MemoryConflictResolution::default();
    }

    let accepted = existing
        .iter()
        .find(|existing| matches!(existing.review_status, MemoryReviewStatus::Accepted));
    let candidate_ids = existing
        .iter()
        .filter(|existing| matches!(existing.review_status, MemoryReviewStatus::Candidate))
        .map(|existing| existing.id.clone())
        .collect::<Vec<_>>();

    if let Some(accepted) = accepted {
        if memory.supersedes.is_none() {
            memory.supersedes = Some(accepted.id.clone());
        }
        if !matches!(memory.review_status, MemoryReviewStatus::Accepted) {
            memory.review_status = MemoryReviewStatus::Candidate;
            if memory.review_note.is_none() {
                memory.review_note = Some(format!(
                    "Candidate update contradicts accepted memory '{}'. Review before superseding.",
                    accepted.subject
                ));
            }
            return MemoryConflictResolution {
                supersede_ids: candidate_ids,
            };
        }
        return MemoryConflictResolution::default();
    }

    let latest_candidate = existing
        .iter()
        .find(|existing| matches!(existing.review_status, MemoryReviewStatus::Candidate));
    if let Some(latest_candidate) = latest_candidate {
        if memory.supersedes.is_none() {
            memory.supersedes = latest_candidate
                .supersedes
                .clone()
                .or_else(|| Some(latest_candidate.id.clone()));
        }
        if !matches!(memory.review_status, MemoryReviewStatus::Accepted) {
            return MemoryConflictResolution {
                supersede_ids: vec![latest_candidate.id.clone()],
            };
        }
    }

    MemoryConflictResolution::default()
}

fn mark_memory_supersessions(
    state: &AppState,
    memory: &MemoryRecord,
    mut supersede_ids: Vec<String>,
) -> Result<(), ApiError> {
    if matches!(memory.review_status, MemoryReviewStatus::Accepted) {
        if let Some(supersedes) = memory.supersedes.as_deref() {
            supersede_ids.push(supersedes.to_string());
        }
        supersede_ids.extend(
            load_conflicting_memories(state, memory)?
                .into_iter()
                .filter(|existing| existing.id != memory.id)
                .map(|existing| existing.id),
        );
    }
    supersede_ids.sort();
    supersede_ids.dedup();
    supersede_ids.retain(|memory_id| memory_id != &memory.id);
    for superseded_id in supersede_ids {
        let _ = state
            .storage
            .mark_memory_superseded(&superseded_id, &memory.id)?;
    }
    Ok(())
}

pub(crate) fn build_memory_context(
    state: &AppState,
    query: &str,
    session_id: Option<&str>,
    cwd: Option<&FsPath>,
    provider_id: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let query = query.trim();
    let profile_lines = load_profile_memory_lines(state, cwd, provider_id)?;
    if query.len() < 3 && profile_lines.is_empty() {
        return Ok(None);
    }
    if query.len() < 3 {
        return Ok(Some(profile_lines.join("\n")));
    }
    let workspace_key = workspace_key(cwd);
    let (memories, transcript_hits) = state.storage.search_memories(
        query,
        workspace_key.as_deref(),
        provider_id,
        &[],
        false,
        6,
    )?;
    if memories.is_empty() && transcript_hits.is_empty() && profile_lines.is_empty() {
        return Ok(None);
    }

    for memory in &memories {
        let _ = state.storage.touch_memory(&memory.id);
    }

    let mut lines = vec![
        "Use the following persisted memory only when it is relevant to the current task."
            .to_string(),
        "Prefer the current conversation over older notes when they conflict.".to_string(),
    ];

    if !profile_lines.is_empty() {
        lines.push(String::new());
        lines.push("Resident profile memory:".to_string());
        lines.extend(profile_lines);
    }

    if !memories.is_empty() {
        lines.push(String::new());
        lines.push("Long-term memory:".to_string());
        for memory in memories.iter().take(6) {
            lines.push(format!(
                "- [{:?}/{:?}] {}: {}",
                memory.kind, memory.scope, memory.subject, memory.content
            ));
        }
    }

    let filtered_hits = transcript_hits
        .into_iter()
        .filter(|hit| Some(hit.session_id.as_str()) != session_id)
        .take(4)
        .collect::<Vec<_>>();
    if !filtered_hits.is_empty() {
        lines.push(String::new());
        lines.push("Related prior transcript hits:".to_string());
        for hit in filtered_hits {
            lines.push(format!(
                "- session {} {:?}: {}",
                hit.session_id, hit.role, hit.preview
            ));
        }
    }

    Ok(Some(lines.join("\n")))
}

fn load_profile_memory_lines(
    state: &AppState,
    cwd: Option<&FsPath>,
    provider_id: Option<&str>,
) -> Result<Vec<String>, ApiError> {
    let workspace_key = workspace_key(cwd);
    let mut memories = state.storage.list_memories_by_tag(
        "system_profile",
        4,
        workspace_key.as_deref(),
        provider_id,
    )?;
    memories.extend(state.storage.list_memories_by_tag(
        "workspace_profile",
        4,
        workspace_key.as_deref(),
        provider_id,
    )?);
    memories.sort_by_key(|memory| std::cmp::Reverse(memory.updated_at));
    memories.truncate(6);
    for memory in &memories {
        let _ = state.storage.touch_memory(&memory.id);
    }
    Ok(memories
        .into_iter()
        .map(|memory| format!("- [{}] {}", memory.subject, memory.content))
        .collect())
}

pub(crate) async fn rebuild_memory(
    State(state): State<AppState>,
    Json(payload): Json<MemoryRebuildRequest>,
) -> Result<Json<MemoryRebuildResponse>, ApiError> {
    let transcripts = if let Some(session_id) = payload
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        vec![crate::sessions::get_session_transcript(&state, session_id)?]
    } else {
        state
            .storage
            .list_sessions(5_000)?
            .into_iter()
            .map(|session| crate::sessions::get_session_transcript(&state, &session.id))
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut stats = MemoryRebuildStats::default();
    for transcript in &transcripts {
        let next = rebuild_memory_from_transcript(&state, transcript, payload.recompute_embeddings)
            .await?;
        stats.observations_scanned += next.observations_scanned;
        stats.memories_upserted += next.memories_upserted;
        stats.embeddings_refreshed += next.embeddings_refreshed;
    }

    append_log(
        &state,
        "info",
        "memory",
        format!(
            "rebuilt memory from {} session(s), {} observation(s), {} memory item(s)",
            transcripts.len(),
            stats.observations_scanned,
            stats.memories_upserted
        ),
    )?;

    Ok(Json(MemoryRebuildResponse {
        generated_at: Utc::now(),
        session_id: payload.session_id,
        sessions_scanned: transcripts.len(),
        observations_scanned: stats.observations_scanned,
        memories_upserted: stats.memories_upserted,
        embeddings_refreshed: stats.embeddings_refreshed,
    }))
}

pub(crate) async fn flush_memory_from_transcript(
    state: &AppState,
    transcript: &SessionTranscript,
) -> Result<(), ApiError> {
    rebuild_memory_from_transcript(state, transcript, false).await?;
    Ok(())
}

pub(crate) fn sync_system_profile_memories(
    state: &AppState,
    cwd: Option<&FsPath>,
    provider_id: Option<&str>,
) -> Result<(), ApiError> {
    for mut memory in collect_system_profile_memories(cwd, provider_id) {
        let existing = state.storage.find_memory_by_subject(
            &memory.subject,
            memory.workspace_key.as_deref(),
            provider_id,
        )?;
        if let Some(existing) = existing {
            if existing.content == memory.content && existing.tags == memory.tags {
                continue;
            }
            memory.id = existing.id;
            memory.created_at = existing.created_at;
            memory.updated_at = Utc::now();
        }
        state.storage.upsert_memory(&memory)?;
    }
    Ok(())
}

fn collect_system_profile_memories(
    cwd: Option<&FsPath>,
    _provider_id: Option<&str>,
) -> Vec<MemoryRecord> {
    let workspace_key = workspace_key(cwd);
    let mut memories = Vec::new();

    let mut push_memory =
        |kind: MemoryKind, scope: MemoryScope, subject: &str, content: String, tags: &[&str]| {
            if content.trim().is_empty() {
                return;
            }
            let mut memory = MemoryRecord::new(kind, scope, subject.to_string(), content);
            memory.workspace_key = workspace_key.clone();
            memory.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
            memories.push(memory);
        };

    push_memory(
        MemoryKind::Note,
        MemoryScope::Global,
        "system:host-os",
        format!(
            "Host OS is {} on {} architecture.",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        &["system_profile", "system", "profile"],
    );

    if let Some(hostname) = env_value(&["COMPUTERNAME", "HOSTNAME"]) {
        push_memory(
            MemoryKind::Note,
            MemoryScope::Global,
            "system:host-name",
            format!("Host machine name is {}.", hostname),
            &["system_profile", "system", "profile"],
        );
    }

    if let Some(username) = env_value(&["USERNAME", "USER"]) {
        push_memory(
            MemoryKind::Preference,
            MemoryScope::Global,
            "system:local-user",
            format!("Local operating-system user is {}.", username),
            &["system_profile", "user", "profile"],
        );
    }

    if let Some(shell) = env_value(&["SHELL", "ComSpec"]) {
        push_memory(
            MemoryKind::Note,
            MemoryScope::Global,
            "system:default-shell",
            format!("Default shell path is {}.", shell),
            &["system_profile", "system", "shell"],
        );
    }

    if let Ok(exe) = std::env::current_exe() {
        push_memory(
            MemoryKind::Note,
            MemoryScope::Global,
            "system:agent-executable",
            format!("The resident agent executable lives at {}.", exe.display()),
            &["system_profile", "agent", "profile"],
        );
    }

    if let Some(cwd) = cwd {
        push_memory(
            MemoryKind::ProjectFact,
            MemoryScope::Workspace,
            "workspace:current-path",
            format!("Current workspace path is {}.", cwd.display()),
            &["workspace_profile", "workspace", "profile"],
        );

        let agents_path = cwd.join("AGENTS.md");
        if agents_path.is_file() {
            push_memory(
                MemoryKind::Constraint,
                MemoryScope::Workspace,
                "workspace:agents-md",
                format!(
                    "Workspace has an AGENTS.md file at {}.",
                    agents_path.display()
                ),
                &["workspace_profile", "workspace", "agents"],
            );
        }

        if let Some(root) = find_git_root(cwd) {
            push_memory(
                MemoryKind::ProjectFact,
                MemoryScope::Workspace,
                "workspace:git-root",
                format!("Workspace git root is {}.", root.display()),
                &["workspace_profile", "workspace", "git"],
            );
        }
    }

    memories
}
pub(crate) fn normalize_memory_sentence(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};

    use agent_core::{
        AppConfig, MemoryEvidenceRef, MessageRole, ModelAlias, SessionMessage, TaskMode,
        ToolExecutionRecord,
    };
    use agent_storage::Storage;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    use super::guidance::workflow_instructions;
    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, ProviderRateLimiter,
    };

    fn test_state() -> AppState {
        AppState {
            storage: Storage::open_at(
                std::env::temp_dir().join(format!("agent-daemon-memory-test-{}", Uuid::new_v4())),
            )
            .unwrap(),
            config: Arc::new(RwLock::new(AppConfig::default())),
            http_client: reqwest::Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: Utc::now(),
            shutdown: mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[test]
    fn extract_memory_candidates_from_system_and_tool_sources() {
        let observation = MemoryObservation::new(
            "The project uses Rust and tokio. Running on Windows 11 with PowerShell.",
            "tool:run_shell",
            -12,
            Some("call-1".to_string()),
        );
        let memories = extract_memory_candidates_from_source(
            &observation,
            "session-1",
            Some("openai"),
            Some(std::path::Path::new("J:\\repo")),
        );
        assert!(memories.iter().any(|memory| {
            memory.kind == MemoryKind::ProjectFact
                && memory.scope == MemoryScope::Workspace
                && memory.tags.iter().any(|tag| tag == "tool:run_shell")
                && memory.source_message_id.as_deref() == Some("call-1")
        }));
        assert!(memories.iter().any(|memory| {
            memory.subject.starts_with("system:") && memory.tags.iter().any(|tag| tag == "tool")
        }));
    }

    #[test]
    fn workflow_instructions_include_tools_and_outcome() {
        let instructions = workflow_instructions(
            "Fix auth and run validation",
            "Auth flow updated and tests passed.",
            &[
                ToolExecutionRecord {
                    call_id: "call-1".to_string(),
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"src/auth.rs\"}".to_string(),
                    outcome: agent_core::ToolExecutionOutcome::Success,
                    output: "loaded file".to_string(),
                },
                ToolExecutionRecord {
                    call_id: "call-2".to_string(),
                    name: "run_shell".to_string(),
                    arguments: "cargo test -p nuclear".to_string(),
                    outcome: agent_core::ToolExecutionOutcome::Success,
                    output: "test result: ok".to_string(),
                },
            ],
        );
        assert!(instructions.contains("read_file"));
        assert!(instructions.contains("run_shell"));
        assert!(instructions.contains("Desired outcome"));
    }

    #[test]
    fn contradictory_constraints_keep_polarity_but_share_identity_key() {
        let always = MemoryObservation::new(
            "Always run cargo test before commit.",
            "user_prompt",
            0,
            None,
        );
        let never = MemoryObservation::new(
            "Do not run cargo test before commit.",
            "user_prompt",
            0,
            None,
        );
        let always_memory = extract_memory_candidates_from_source(
            &always,
            "session-1",
            Some("openai"),
            Some(std::path::Path::new("J:\\repo")),
        )
        .into_iter()
        .find(|memory| matches!(memory.kind, MemoryKind::Constraint))
        .unwrap();
        let never_memory = extract_memory_candidates_from_source(
            &never,
            "session-1",
            Some("openai"),
            Some(std::path::Path::new("J:\\repo")),
        )
        .into_iter()
        .find(|memory| matches!(memory.kind, MemoryKind::Constraint))
        .unwrap();
        assert_eq!(always_memory.identity_key, never_memory.identity_key);
        assert_ne!(always_memory.content, never_memory.content);
        assert!(always_memory.content.contains("always run cargo test"));
        assert!(never_memory.content.contains("do not run cargo test"));
    }

    #[test]
    fn resolve_memory_conflicts_keeps_accepted_anchor_and_supersedes_stale_candidates() {
        let identity_key = Some("preference:global:output".to_string());
        let mut accepted = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers concise output".to_string(),
        );
        accepted.identity_key = identity_key.clone();
        accepted.review_status = MemoryReviewStatus::Accepted;

        let mut stale_candidate = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers detailed output".to_string(),
        );
        stale_candidate.identity_key = identity_key.clone();
        stale_candidate.review_status = MemoryReviewStatus::Candidate;
        stale_candidate.updated_at = accepted.updated_at + chrono::Duration::seconds(1);

        let mut incoming = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers terse output".to_string(),
        );
        incoming.identity_key = identity_key;
        incoming.review_status = MemoryReviewStatus::Candidate;

        let resolution = resolve_memory_conflicts(
            &mut incoming,
            vec![stale_candidate.clone(), accepted.clone()],
        );
        assert_eq!(incoming.review_status, MemoryReviewStatus::Candidate);
        assert_eq!(incoming.supersedes.as_deref(), Some(accepted.id.as_str()));
        assert_eq!(resolution.supersede_ids, vec![stale_candidate.id]);
    }

    #[test]
    fn resolve_memory_conflicts_reuses_exact_candidate_and_preserves_supersedes() {
        let mut candidate = MemoryRecord::new(
            MemoryKind::Constraint,
            MemoryScope::Global,
            "constraint:run-tests".to_string(),
            "Constraint: do not skip cargo test".to_string(),
        );
        candidate.identity_key = Some("constraint:global:skip-cargo-test".to_string());
        candidate.review_status = MemoryReviewStatus::Candidate;
        candidate.supersedes = Some("accepted-memory".to_string());

        let mut incoming = MemoryRecord::new(
            MemoryKind::Constraint,
            MemoryScope::Global,
            "constraint:run-tests".to_string(),
            "Constraint: do not skip cargo test".to_string(),
        );
        incoming.identity_key = candidate.identity_key.clone();
        incoming.review_status = MemoryReviewStatus::Accepted;

        let resolution = resolve_memory_conflicts(&mut incoming, vec![candidate.clone()]);
        assert!(resolution.supersede_ids.is_empty());
        assert_eq!(incoming.id, candidate.id);
        assert_eq!(incoming.supersedes, candidate.supersedes);
        assert_eq!(incoming.review_status, MemoryReviewStatus::Accepted);
    }

    #[test]
    fn repeated_candidate_from_new_source_is_auto_accepted() {
        let mut candidate = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers concise output".to_string(),
        );
        candidate.identity_key = Some("preference:global:output".to_string());
        candidate.review_status = MemoryReviewStatus::Candidate;
        candidate.confidence = 74;
        candidate.observation_source = Some("user_prompt".to_string());
        candidate.source_session_id = Some("session-1".to_string());
        candidate.tags = vec!["preference".to_string(), "user_prompt".to_string()];

        let mut incoming = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers concise output".to_string(),
        );
        incoming.identity_key = candidate.identity_key.clone();
        incoming.review_status = MemoryReviewStatus::Candidate;
        incoming.confidence = 74;
        incoming.observation_source = Some("tool:run_shell".to_string());
        incoming.source_session_id = Some("session-2".to_string());
        incoming.tags = vec!["preference".to_string(), "tool:run_shell".to_string()];

        let resolution = resolve_memory_conflicts(&mut incoming, vec![candidate.clone()]);
        assert!(resolution.supersede_ids.is_empty());
        assert_eq!(incoming.id, candidate.id);
        assert_eq!(incoming.review_status, MemoryReviewStatus::Accepted);
        assert!(incoming.tags.iter().any(|tag| tag == "reinforced"));
        assert!(incoming.tags.iter().any(|tag| tag == "multi_source"));
        assert!(incoming.tags.iter().any(|tag| tag == "cross_session"));
        assert!(incoming.reviewed_at.is_some());
    }

    #[test]
    fn resolve_memory_conflicts_merges_evidence_refs_for_exact_match() {
        let mut candidate = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output".to_string(),
        );
        candidate.evidence_refs = vec![MemoryEvidenceRef {
            session_id: "session-1".to_string(),
            message_id: Some("message-1".to_string()),
            role: Some(MessageRole::User),
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }];

        let mut incoming = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output".to_string(),
        );
        incoming.evidence_refs = vec![MemoryEvidenceRef {
            session_id: "session-2".to_string(),
            message_id: Some("message-2".to_string()),
            role: Some(MessageRole::User),
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }];

        let resolution = resolve_memory_conflicts(&mut incoming, vec![candidate.clone()]);
        assert!(resolution.supersede_ids.is_empty());
        assert_eq!(incoming.id, candidate.id);
        assert_eq!(incoming.evidence_refs.len(), 2);
        assert!(incoming.evidence_refs.contains(&candidate.evidence_refs[0]));
    }

    #[test]
    fn learning_observations_from_session_messages_preserve_exact_evidence() {
        let created_at = Utc::now();
        let user = SessionMessage {
            id: "message-user".to_string(),
            session_id: "session-1".to_string(),
            role: MessageRole::User,
            content: "I prefer concise output.".to_string(),
            created_at,
            provider_id: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        };
        let tool = SessionMessage {
            id: "message-tool".to_string(),
            session_id: "session-1".to_string(),
            role: MessageRole::Tool,
            content: "Project uses Rust and tokio.".to_string(),
            created_at: created_at + chrono::Duration::seconds(1),
            provider_id: Some("openai".to_string()),
            model: Some("gpt-4.1".to_string()),
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("run_shell".to_string()),
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        };

        let observations = learning_observations_from_session_messages(&[user, tool]);
        let user_observation = observations
            .iter()
            .find(|observation| observation.source_tag == "user_prompt")
            .unwrap();
        assert_eq!(
            user_observation.source_message_id.as_deref(),
            Some("message-user")
        );
        assert_eq!(
            user_observation.evidence_refs("session-1")[0].role,
            Some(MessageRole::User)
        );

        let tool_observation = observations
            .iter()
            .find(|observation| observation.source_tag == "tool:run_shell")
            .unwrap();
        let evidence = &tool_observation.evidence_refs("session-1")[0];
        assert_eq!(evidence.message_id.as_deref(), Some("message-tool"));
        assert_eq!(evidence.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(evidence.tool_name.as_deref(), Some("run_shell"));
    }

    #[tokio::test]
    async fn rebuild_memory_from_transcript_persists_evidence_backed_memories() {
        let state = test_state();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        let cwd = std::env::temp_dir().join(format!("agent-memory-rebuild-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&cwd).unwrap();

        state
            .storage
            .ensure_session_with_title(
                "session-1",
                Some("Daily memory"),
                &alias,
                "openai",
                "gpt-4.1",
                Some(TaskMode::Daily),
                Some(cwd.as_path()),
            )
            .unwrap();

        let user = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "I prefer concise output.".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        );
        let tool = SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Tool,
            "This project uses Rust and tokio.".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )
        .with_tool_metadata(Some("call-1".to_string()), Some("run_shell".to_string()));
        state.storage.append_message(&user).unwrap();
        state.storage.append_message(&tool).unwrap();

        let transcript = crate::sessions::get_session_transcript(&state, "session-1").unwrap();
        let stats = rebuild_memory_from_transcript(&state, &transcript, false)
            .await
            .unwrap();
        assert!(stats.observations_scanned >= 2);
        assert!(stats.memories_upserted >= 2);

        let accepted_memories = state
            .storage
            .list_memories_by_source_session("session-1", 10)
            .unwrap();
        assert!(accepted_memories.iter().any(|memory| {
            matches!(memory.kind, MemoryKind::Preference)
                && memory
                    .evidence_refs
                    .iter()
                    .any(|evidence| evidence.message_id.as_deref() == Some(user.id.as_str()))
        }));
        let candidate_memories = state
            .storage
            .list_memories_by_review_status(MemoryReviewStatus::Candidate, 10)
            .unwrap();
        assert!(accepted_memories
            .iter()
            .chain(candidate_memories.iter())
            .any(|memory| {
                memory.source_session_id.as_deref() == Some("session-1")
                    && memory.evidence_refs.iter().any(|evidence| {
                        evidence.message_id.as_deref() == Some(tool.id.as_str())
                            && evidence.tool_call_id.as_deref() == Some("call-1")
                            && evidence.tool_name.as_deref() == Some("run_shell")
                    })
            }));

        let _ = std::fs::remove_dir_all(cwd);
    }

    #[test]
    fn collect_system_profile_memories_includes_host_and_workspace_facts() {
        let root = std::env::temp_dir().join(format!("agent-profile-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("AGENTS.md"), "test").unwrap();
        let memories = collect_system_profile_memories(Some(root.as_path()), Some("openai"));
        assert!(memories
            .iter()
            .any(|memory| memory.subject == "system:host-os"));
        assert!(memories
            .iter()
            .any(|memory| memory.subject == "workspace:git-root"));
        assert!(memories
            .iter()
            .any(|memory| memory.subject == "workspace:agents-md"));
        let _ = std::fs::remove_dir_all(root);
    }
}
