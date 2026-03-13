use std::{
    collections::HashSet,
    fs,
    path::{Path as FsPath, PathBuf},
};

use agent_core::{
    ConversationMessage, MemoryKind, MemoryRecord, MemoryReviewStatus, MemoryReviewUpdateRequest,
    MemoryScope, MemorySearchQuery, MemorySearchResponse, MemoryUpsertRequest, MessageRole,
    PermissionPreset, SkillDraft, SkillDraftStatus, ToolExecutionRecord,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;

use tracing::warn;

use crate::{append_log, ApiError, AppState, LimitQuery, SkillDraftQuery};

#[derive(Debug, Default)]
struct MemoryConflictResolution {
    supersede_ids: Vec<String>,
}

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
    memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
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
        warn!("failed to compute embedding for memory '{}': {err}", memory.id);
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
        limit,
    )?;

    // Supplement with embedding-based semantic search if configured.
    if memories.len() < limit {
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
    let (memories, transcript_hits) =
        state
            .storage
            .search_memories(query, workspace_key.as_deref(), provider_id, 6)?;
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
    memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    memories.truncate(6);
    for memory in &memories {
        let _ = state.storage.touch_memory(&memory.id);
    }
    Ok(memories
        .into_iter()
        .map(|memory| format!("- [{}] {}", memory.subject, memory.content))
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn learn_from_interaction(
    state: &AppState,
    prompt: &str,
    response: &str,
    transcript_messages: &[ConversationMessage],
    tool_events: &[ToolExecutionRecord],
    permission_preset: PermissionPreset,
    session_id: &str,
    provider_id: Option<&str>,
    cwd: Option<&FsPath>,
    background: bool,
) -> Result<(), ApiError> {
    let mut learned = 0usize;
    let mut candidates = Vec::new();
    for observation in learning_observations(prompt, response, transcript_messages) {
        candidates.extend(extract_memory_candidates_from_source(
            &observation,
            session_id,
            provider_id,
            cwd,
        ));
    }

    for mut memory in dedupe_memories(candidates) {
        memory.review_status = classify_memory_review_status(&memory);
        let existing = load_conflicting_memories(state, &memory)?;
        let resolution = resolve_memory_conflicts(&mut memory, existing);
        state.storage.upsert_memory(&memory)?;
        mark_memory_supersessions(state, &memory, resolution.supersede_ids)?;
        learned += 1;
    }
    if learned > 0 {
        append_log(
            state,
            "info",
            "memory",
            format!("learned {} memory item(s)", learned),
        )?;
    }
    if let Some((title, status)) = learn_skill_draft_from_execution(
        state,
        prompt,
        response,
        tool_events,
        session_id,
        provider_id,
        cwd,
        permission_preset,
        background,
    )? {
        append_log(
            state,
            "info",
            "skills",
            format!("learned skill draft '{}' as {:?}", title, status),
        )?;
    }

    // Detect and record usage patterns from tool events.
    if tool_events.len() >= 2 {
        let workspace_key = cwd.map(|p| p.display().to_string());
        let detected = crate::detect_patterns(tool_events, workspace_key.as_deref(), provider_id);
        if !detected.is_empty() {
            if let Err(err) = crate::record_patterns(state, detected) {
                warn!("failed to record usage patterns: {err}");
            }
        }
    }

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

#[allow(clippy::too_many_arguments)]
fn learn_skill_draft_from_execution(
    state: &AppState,
    prompt: &str,
    response: &str,
    tool_events: &[ToolExecutionRecord],
    session_id: &str,
    provider_id: Option<&str>,
    cwd: Option<&FsPath>,
    permission_preset: PermissionPreset,
    _background: bool,
) -> Result<Option<(String, SkillDraftStatus)>, ApiError> {
    let successful = tool_events
        .iter()
        .filter(|event| matches!(event.outcome, agent_core::ToolExecutionOutcome::Success))
        .take(6)
        .cloned()
        .collect::<Vec<_>>();
    if successful.len() < 2 {
        return Ok(None);
    }

    let workspace_key = workspace_key(cwd);
    let title = workflow_title_from_prompt(prompt, &successful);
    let summary = format!(
        "Reusable workflow observed from {} successful tool step(s).",
        successful.len()
    );
    let instructions = workflow_instructions(prompt, response, &successful);
    let existing =
        state
            .storage
            .find_skill_draft_by_title(&title, workspace_key.as_deref(), provider_id)?;
    let mut draft = existing
        .clone()
        .unwrap_or_else(|| SkillDraft::new(title.clone(), summary.clone(), instructions.clone()));

    draft.title = title.clone();
    draft.summary = summary;
    draft.instructions = instructions;
    draft.trigger_hint = Some(summarize_preview(prompt, 180));
    draft.workspace_key = workspace_key;
    draft.provider_id = provider_id.map(ToOwned::to_owned);
    draft.source_session_id = Some(session_id.to_string());
    draft.source_message_ids = successful
        .iter()
        .map(|event| event.call_id.clone())
        .collect();
    draft.updated_at = Utc::now();
    draft.usage_count = existing
        .as_ref()
        .map(|existing| existing.usage_count.saturating_add(1))
        .unwrap_or(1);
    if matches!(permission_preset, PermissionPreset::FullAuto) && draft.usage_count >= 2 {
        draft.status = SkillDraftStatus::Published;
    } else if !matches!(draft.status, SkillDraftStatus::Published) {
        draft.status = SkillDraftStatus::Draft;
    }
    state.storage.upsert_skill_draft(&draft)?;
    Ok(Some((draft.title, draft.status)))
}

#[derive(Debug, Clone)]
struct MemoryObservation {
    text: String,
    source_tag: String,
    confidence_adjustment: i16,
    source_message_id: Option<String>,
}

impl MemoryObservation {
    fn new(
        text: impl Into<String>,
        source_tag: impl Into<String>,
        confidence_adjustment: i16,
        source_message_id: Option<String>,
    ) -> Self {
        Self {
            text: text.into(),
            source_tag: source_tag.into(),
            confidence_adjustment,
            source_message_id,
        }
    }
}

fn learning_observations(
    prompt: &str,
    response: &str,
    transcript_messages: &[ConversationMessage],
) -> Vec<MemoryObservation> {
    let mut observations = Vec::new();
    if !prompt.trim().is_empty() {
        observations.push(MemoryObservation::new(prompt, "user_prompt", 0, None));
    }
    if !response.trim().is_empty() {
        observations.push(MemoryObservation::new(
            response,
            "assistant_reply",
            -8,
            None,
        ));
    }
    for message in transcript_messages.iter().rev().take(8).rev() {
        match message.role {
            MessageRole::Tool => {
                if let Some(text) = learning_text_from_tool_message(message) {
                    let source_tag = match message.tool_name.as_deref() {
                        Some(name) if !name.trim().is_empty() => format!("tool:{name}"),
                        _ => "tool_output".to_string(),
                    };
                    observations.push(MemoryObservation::new(
                        text,
                        source_tag,
                        -12,
                        message.tool_call_id.clone(),
                    ));
                }
            }
            MessageRole::Assistant => {
                if !message.content.trim().is_empty() {
                    observations.push(MemoryObservation::new(
                        message.content.clone(),
                        "assistant_transcript",
                        -10,
                        None,
                    ));
                }
            }
            _ => {}
        }
    }
    observations
}

fn learning_text_from_tool_message(message: &ConversationMessage) -> Option<String> {
    let trimmed = message.content.trim();
    if trimmed.is_empty() || trimmed.starts_with("ERROR:") {
        return None;
    }

    let mut combined = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    if combined.is_empty() {
        combined = trimmed.to_string();
    }

    Some(truncate_for_learning(&combined, 400))
}

fn truncate_for_learning(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let truncated = input.chars().take(max_chars).collect::<String>();
    format!("{truncated}...")
}

fn extract_memory_candidates_from_source(
    observation: &MemoryObservation,
    session_id: &str,
    provider_id: Option<&str>,
    cwd: Option<&FsPath>,
) -> Vec<MemoryRecord> {
    let trimmed = observation.text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let workspace_key = workspace_key(cwd);
    let mut candidates = Vec::new();
    let mut push_candidate = |kind: MemoryKind,
                              scope: MemoryScope,
                              prefix: &str,
                              content: String,
                              confidence: u8,
                              tags: &[&str],
                              identity_key: Option<String>| {
        let normalized = normalize_memory_sentence(&content);
        if normalized.is_empty() {
            return;
        }
        let identity_key = identity_key.unwrap_or_else(|| {
            memory_identity_key(prefix, &normalized, kind.clone(), scope.clone())
        });
        let mut memory = MemoryRecord::new(
            kind,
            scope,
            memory_subject(prefix, &normalized),
            normalized.clone(),
        );
        memory.confidence =
            (i16::from(confidence) + observation.confidence_adjustment).clamp(1, 100) as u8;
        memory.source_session_id = Some(session_id.to_string());
        memory.source_message_id = observation.source_message_id.clone();
        memory.provider_id = provider_id.map(ToOwned::to_owned);
        memory.workspace_key = workspace_key.clone();
        memory.identity_key = Some(identity_key);
        memory.observation_source = Some(observation.source_tag.clone());
        memory.tags = tags.iter().map(|tag| (*tag).to_string()).collect();
        if !observation.source_tag.is_empty() {
            memory.tags.push(observation.source_tag.clone());
            if let Some((category, _)) = observation.source_tag.split_once(':') {
                memory.tags.push(category.to_string());
            }
        }
        candidates.push(memory);
    };

    if let Some(value) = extract_after_phrase(trimmed, &["remember that", "remember"]) {
        push_candidate(
            MemoryKind::Note,
            MemoryScope::Global,
            "memory",
            value.to_string(),
            95,
            &["manual", "remember"],
            None,
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["i prefer", "please prefer"]) {
        push_candidate(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference",
            format!("User prefers {}", value),
            90,
            &["preference"],
            Some(memory_identity_key(
                "preference",
                value,
                MemoryKind::Preference,
                MemoryScope::Global,
            )),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["always"]) {
        push_candidate(
            MemoryKind::Constraint,
            MemoryScope::Global,
            "constraint",
            format!("Constraint: always {}", value),
            85,
            &["constraint"],
            Some(memory_identity_key(
                "constraint",
                value,
                MemoryKind::Constraint,
                MemoryScope::Global,
            )),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["never", "do not", "don't"]) {
        push_candidate(
            MemoryKind::Constraint,
            MemoryScope::Global,
            "constraint",
            format!("Constraint: do not {}", value),
            85,
            &["constraint"],
            Some(memory_identity_key(
                "constraint",
                value,
                MemoryKind::Constraint,
                MemoryScope::Global,
            )),
        );
    }
    if let Some(value) = extract_after_phrase(
        trimmed,
        &[
            "project uses",
            "this project uses",
            "our project uses",
            "the repo uses",
            "we use",
            "our stack is",
        ],
    ) {
        push_candidate(
            MemoryKind::ProjectFact,
            MemoryScope::Workspace,
            "project",
            format!("Project uses {}", value),
            88,
            &["project", "stack"],
            Some(memory_identity_key(
                "project",
                value,
                MemoryKind::ProjectFact,
                MemoryScope::Workspace,
            )),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["when working here", "for this repo"]) {
        push_candidate(
            MemoryKind::Workflow,
            MemoryScope::Workspace,
            "workflow",
            value.to_string(),
            80,
            &["workflow"],
            Some(memory_identity_key(
                "workflow",
                value,
                MemoryKind::Workflow,
                MemoryScope::Workspace,
            )),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["running on", "os is"]) {
        push_candidate(
            MemoryKind::Note,
            MemoryScope::Global,
            "system",
            format!("System detail: OS is {}", value),
            72,
            &["system"],
            Some("note:global:system-os".to_string()),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["shell is"]) {
        push_candidate(
            MemoryKind::Note,
            MemoryScope::Global,
            "system",
            format!("System detail: shell is {}", value),
            72,
            &["system"],
            Some("note:global:system-shell".to_string()),
        );
    }
    if let Some(value) = extract_after_phrase(trimmed, &["this machine uses", "the machine uses"]) {
        push_candidate(
            MemoryKind::Note,
            MemoryScope::Global,
            "system",
            format!("System detail: machine uses {}", value),
            72,
            &["system"],
            Some("note:global:system-machine".to_string()),
        );
    }

    dedupe_memories(candidates)
}

fn dedupe_memories(memories: Vec<MemoryRecord>) -> Vec<MemoryRecord> {
    let mut deduped: Vec<MemoryRecord> = Vec::new();
    for memory in memories {
        if let Some(existing) = deduped.iter_mut().find(|existing| {
            existing
                .identity_key
                .as_deref()
                .filter(|value| !value.is_empty())
                == memory
                    .identity_key
                    .as_deref()
                    .filter(|value| !value.is_empty())
                || existing.subject == memory.subject
        }) {
            if memory.confidence > existing.confidence {
                existing.kind = memory.kind.clone();
                existing.scope = memory.scope.clone();
                existing.content = memory.content.clone();
                existing.confidence = memory.confidence;
                existing.source_message_id = memory.source_message_id.clone();
                existing.observation_source = memory.observation_source.clone();
            }
            if existing.source_session_id.is_none() {
                existing.source_session_id = memory.source_session_id.clone();
            }
            if existing.provider_id.is_none() {
                existing.provider_id = memory.provider_id.clone();
            }
            if existing.workspace_key.is_none() {
                existing.workspace_key = memory.workspace_key.clone();
            }
            if existing.identity_key.is_none() {
                existing.identity_key = memory.identity_key.clone();
            }
            for tag in memory.tags {
                if !existing.tags.contains(&tag) {
                    existing.tags.push(tag);
                }
            }
            continue;
        }
        deduped.push(memory);
    }
    deduped
}

fn classify_memory_review_status(memory: &MemoryRecord) -> MemoryReviewStatus {
    if memory
        .tags
        .iter()
        .any(|tag| tag == "manual" || tag == "remember")
    {
        return MemoryReviewStatus::Accepted;
    }
    if memory.confidence >= 85
        && !memory
            .tags
            .iter()
            .any(|tag| tag == "assistant_reply" || tag == "assistant_transcript")
    {
        MemoryReviewStatus::Accepted
    } else {
        MemoryReviewStatus::Candidate
    }
}

fn extract_after_phrase<'a>(input: &'a str, phrases: &[&str]) -> Option<&'a str> {
    let lowercase = input.to_ascii_lowercase();
    for phrase in phrases {
        let phrase_lower = phrase.to_ascii_lowercase();
        if let Some(index) = lowercase.find(&phrase_lower) {
            let start = index + phrase_lower.len();
            let value = input
                .get(start..)?
                .trim_matches(|ch: char| ch == ':' || ch == '-' || ch.is_whitespace());
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

pub(crate) fn normalize_memory_sentence(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| ch == '.' || ch == ',' || ch == ';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn summarize_preview(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut preview = trimmed.chars().take(max_chars).collect::<String>();
    preview.push_str("...");
    preview
}

fn memory_subject(prefix: &str, content: &str) -> String {
    let slug = content
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}:{slug}")
    }
}

fn memory_identity_key(
    prefix: &str,
    content: &str,
    kind: MemoryKind,
    scope: MemoryScope,
) -> String {
    let anchor = content
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join("-");
    let anchor = if anchor.is_empty() { prefix } else { &anchor };
    format!("{kind:?}:{scope:?}:{anchor}").to_ascii_lowercase()
}

fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn find_git_root(start: &FsPath) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".git").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

fn workflow_title_from_prompt(prompt: &str, tool_events: &[ToolExecutionRecord]) -> String {
    let prompt_hint = prompt
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    let tool_hint = tool_events
        .iter()
        .map(|event| event.name.as_str())
        .collect::<Vec<_>>()
        .join(" -> ");
    if prompt_hint.is_empty() {
        format!("Workflow via {}", tool_hint)
    } else {
        format!("Workflow: {} via {}", prompt_hint, tool_hint)
    }
}

fn workflow_instructions(
    prompt: &str,
    response: &str,
    tool_events: &[ToolExecutionRecord],
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Trigger: {}", summarize_preview(prompt, 160)));
    lines.push(String::new());
    lines.push("Suggested steps:".to_string());
    for (index, event) in tool_events.iter().enumerate() {
        let arguments = summarize_preview(&event.arguments.replace('\n', " "), 100);
        let output = summarize_preview(&event.output.replace('\n', " "), 120);
        lines.push(format!(
            "{}. Use `{}` with arguments like `{}`.",
            index + 1,
            event.name,
            arguments
        ));
        if !output.is_empty() {
            lines.push(format!("   Expected result: {}", output));
        }
    }
    if !response.trim().is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Desired outcome: {}",
            summarize_preview(response, 180)
        ));
    }
    lines.join("\n")
}

pub(crate) async fn load_enabled_skill_guidance(
    state: &AppState,
    query: &str,
    cwd: Option<&FsPath>,
    provider_id: Option<&str>,
) -> String {
    let enabled = {
        let config = state.config.read().await;
        config.enabled_skills.clone()
    };
    let mut blocks = Vec::new();
    let static_skills = if enabled.is_empty() {
        String::new()
    } else {
        load_skill_guidance_blocks(&enabled)
    };
    if !static_skills.is_empty() {
        blocks.push(static_skills);
    }
    let learned = load_published_skill_draft_guidance(state, query, cwd, provider_id);
    if !learned.is_empty() {
        blocks.push(learned);
    }
    blocks.join("\n")
}

fn load_skill_guidance_blocks(enabled_skills: &[String]) -> String {
    const MAX_TOTAL_BYTES: usize = 32_000;
    let Some(root) = home_dir().map(|home| home.join(".codex").join("skills")) else {
        return String::new();
    };
    let mut output = String::new();
    for skill_name in enabled_skills {
        let Some(path) = find_skill_markdown(&root, skill_name) else {
            continue;
        };
        let Ok(mut content) = fs::read_to_string(&path) else {
            continue;
        };
        if content.len() > 8_000 {
            content.truncate(8_000);
            content.push_str("\n\n[truncated]");
        }
        let block = format!(
            "--- skill:{} ({})\n{}\n",
            skill_name,
            path.display(),
            content.trim()
        );
        if output.len() + block.len() > MAX_TOTAL_BYTES {
            output.push_str("\n--- [additional skill content truncated]");
            break;
        }
        output.push_str(&block);
    }
    output
}

fn load_published_skill_draft_guidance(
    state: &AppState,
    query: &str,
    cwd: Option<&FsPath>,
    provider_id: Option<&str>,
) -> String {
    let workspace_key = workspace_key(cwd);
    let Ok(drafts) = state.storage.list_skill_drafts(
        32,
        Some(SkillDraftStatus::Published),
        workspace_key.as_deref(),
        provider_id,
    ) else {
        return String::new();
    };
    let query_terms = relevant_query_terms(query);
    let mut selected = drafts
        .into_iter()
        .filter(|draft| skill_draft_relevant(draft, &query_terms))
        .take(3)
        .collect::<Vec<_>>();
    for draft in &selected {
        let _ = state.storage.touch_skill_draft(&draft.id);
    }
    if selected.is_empty() {
        return String::new();
    }
    selected.sort_by(|left, right| {
        right
            .usage_count
            .cmp(&left.usage_count)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    selected
        .into_iter()
        .map(|draft| {
            format!(
                "--- learned_workflow:{}\nsummary: {}\ntrigger: {}\ninstructions:\n{}\n",
                draft.title,
                draft.summary,
                draft
                    .trigger_hint
                    .as_deref()
                    .unwrap_or("apply when the task closely matches this workflow"),
                draft.instructions.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn relevant_query_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| part.len() >= 4)
        .take(8)
        .collect()
}

fn skill_draft_relevant(draft: &SkillDraft, query_terms: &[String]) -> bool {
    if query_terms.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {}",
        draft.title,
        draft.summary,
        draft.instructions,
        draft.trigger_hint.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    query_terms.iter().any(|term| haystack.contains(term))
}

fn find_skill_markdown(root: &FsPath, skill_name: &str) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().ok()?.is_dir() {
            if path
                .file_name()
                .map(|name| name.to_string_lossy() == skill_name)
                .unwrap_or(false)
            {
                let candidate = path.join("SKILL.md");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            if let Some(found) = find_skill_markdown(&path, skill_name) {
                return Some(found);
            }
        }
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Compute and store an embedding for the given memory if the embedding provider is configured.
async fn maybe_compute_embedding(state: &AppState, memory: &MemoryRecord) -> anyhow::Result<()> {
    let config = state.config.read().await;
    if !config.embedding.enabled {
        return Ok(());
    }
    let provider_id = config
        .embedding
        .provider_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("embedding provider_id not configured"))?;
    let provider = config
        .get_provider(provider_id)
        .ok_or_else(|| anyhow::anyhow!("embedding provider '{}' not found", provider_id))?
        .clone();
    let model = config
        .embedding
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("embedding model not configured"))?;
    let dimensions = config.embedding.dimensions;
    drop(config);

    let text = format!("{}: {}", memory.subject, memory.content);
    let dims = if dimensions > 0 {
        Some(dimensions)
    } else {
        None
    };
    let embedding =
        agent_providers::compute_embedding(&state.http_client, &provider, &model, &text, dims)
            .await?;
    state
        .storage
        .upsert_memory_embedding(&memory.id, &embedding, &model)?;
    Ok(())
}

/// Search for memories using embedding similarity.
async fn embedding_search(
    state: &AppState,
    query: &str,
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
    limit: usize,
    exclude_ids: &[String],
) -> anyhow::Result<Vec<MemoryRecord>> {
    let config = state.config.read().await;
    if !config.embedding.enabled {
        return Ok(Vec::new());
    }
    // Don't bother if there are no embeddings stored yet.
    if !state.storage.has_memory_embeddings()? {
        return Ok(Vec::new());
    }

    let emb_provider_id = config
        .embedding
        .provider_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("embedding provider_id not configured"))?;
    let emb_provider = config
        .get_provider(emb_provider_id)
        .ok_or_else(|| {
            anyhow::anyhow!("embedding provider '{}' not found", emb_provider_id)
        })?
        .clone();
    let model = config
        .embedding
        .model
        .clone()
        .ok_or_else(|| anyhow::anyhow!("embedding model not configured"))?;
    let dimensions = config.embedding.dimensions;
    drop(config);

    let dims = if dimensions > 0 {
        Some(dimensions)
    } else {
        None
    };
    let query_embedding =
        agent_providers::compute_embedding(&state.http_client, &emb_provider, &model, query, dims)
            .await?;
    state.storage.search_memories_by_embedding(
        &query_embedding,
        workspace_key,
        provider_id,
        limit,
        exclude_ids,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
                    arguments: "cargo test -p autism".to_string(),
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
