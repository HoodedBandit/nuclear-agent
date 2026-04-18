use std::collections::HashSet;

use agent_core::{
    truncate_with_suffix, MessageRole, ModelAlias, PermissionPreset, RunTaskResponse,
    SessionMessage, SessionRenameRequest, SessionResumePacket, SessionSummary, SessionTranscript,
    TaskMode, ThinkingLevel,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::memory::flush_memory_from_transcript;
use crate::{
    execute_task_request, resolve_alias_and_provider, ApiError, AppState, LimitQuery,
    TaskRequestInput,
};

#[derive(Debug, Deserialize)]
pub(crate) struct SessionForkRequest {
    #[serde(default)]
    pub(crate) target_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SessionCompactRequest {
    #[serde(default)]
    pub(crate) alias: Option<String>,
    #[serde(default)]
    pub(crate) requested_model: Option<String>,
    #[serde(default)]
    pub(crate) cwd: Option<std::path::PathBuf>,
    #[serde(default)]
    pub(crate) thinking_level: Option<ThinkingLevel>,
    #[serde(default)]
    pub(crate) permission_preset: Option<PermissionPreset>,
    #[serde(default)]
    pub(crate) task_mode: Option<TaskMode>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SessionMutationResponse {
    pub(crate) session: SessionSummary,
    pub(crate) messages: Vec<SessionMessage>,
}

pub(crate) fn list_sessions_from_state(
    state: &AppState,
    limit: usize,
) -> Result<Vec<SessionSummary>, ApiError> {
    Ok(state.storage.list_sessions(limit)?)
}

pub(crate) fn get_session_transcript(
    state: &AppState,
    session_id: &str,
) -> Result<SessionTranscript, ApiError> {
    let session = state
        .storage
        .get_session(session_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown session"))?;
    let messages = state.storage.list_session_messages(&session.id)?;
    Ok(SessionTranscript { session, messages })
}

pub(crate) fn rename_session_title(
    state: &AppState,
    session_id: &str,
    title: &str,
) -> Result<(), ApiError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "session title must not be empty",
        ));
    }
    state.storage.rename_session(session_id, trimmed)?;
    Ok(())
}

fn load_target_session_transcript(
    state: &AppState,
    requested_id: &str,
    target_session_id: Option<&str>,
) -> Result<SessionTranscript, ApiError> {
    let session_id = target_session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(requested_id);
    get_session_transcript(state, session_id)
}

fn format_session_message_for_display(message: &SessionMessage) -> String {
    let mut lines = Vec::new();
    if message.role == MessageRole::Tool {
        let label = match (&message.tool_name, &message.tool_call_id) {
            (Some(name), Some(call_id)) => format!("[tool:{name} id={call_id}]"),
            (Some(name), None) => format!("[tool:{name}]"),
            (None, Some(call_id)) => format!("[tool id={call_id}]"),
            (None, None) => "[tool]".to_string(),
        };
        lines.push(label);
    }
    if !message.content.trim().is_empty() {
        lines.push(message.content.trim().to_string());
    }
    for tool_call in &message.tool_calls {
        lines.push(format!(
            "[tool_call:{} id={}]",
            tool_call.name, tool_call.id
        ));
    }
    if !message.attachments.is_empty() {
        let attachments = message
            .attachments
            .iter()
            .map(|attachment| attachment.path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("[images: {attachments}]"));
    }
    if lines.is_empty() {
        "(empty)".to_string()
    } else {
        lines.join("\n")
    }
}

fn truncate_prompt(text: String, max_len: usize) -> String {
    truncate_with_suffix(&text, max_len, "\n\n[truncated]")
}

fn build_compact_prompt(transcript: &SessionTranscript) -> Result<String, ApiError> {
    let mut history = String::new();
    for message in &transcript.messages {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        history.push_str(&format!(
            "[{role}]\n{}\n\n",
            format_session_message_for_display(message)
        ));
    }

    if history.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "current session has no transcript to compact",
        ));
    }

    let history = truncate_prompt(history, 100_000);
    Ok(format!(
        "Summarize this session so a fresh agent can continue it with minimal context loss. Preserve the user's goals, decisions, current status, important files or artifacts, unresolved questions, and any concrete next steps. Be concise but complete.\n\nTranscript:\n```text\n{history}\n```"
    ))
}

fn compact_session_summary(
    state: &AppState,
    transcript: &SessionTranscript,
    summary: &str,
    task_mode: Option<TaskMode>,
) -> Result<SessionTranscript, ApiError> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    state.storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        task_mode,
        transcript.session.cwd.as_deref(),
    )?;
    state.storage.append_message(&SessionMessage::new(
        new_session_id.clone(),
        MessageRole::User,
        format!(
            "This session is a compacted continuation of session {}.\nUse the following summary as prior context:\n\n{}",
            transcript.session.id,
            summary.trim()
        ),
        Some(transcript.session.provider_id.clone()),
        Some(transcript.session.model.clone()),
    ))?;
    get_session_transcript(state, &new_session_id)
}

fn fork_transcript(
    state: &AppState,
    transcript: &SessionTranscript,
) -> Result<SessionTranscript, ApiError> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    state.storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        transcript.session.task_mode,
        transcript.session.cwd.as_deref(),
    )?;
    for message in &transcript.messages {
        state
            .storage
            .append_message(&message.fork_to_session(new_session_id.clone()))?;
    }
    get_session_transcript(state, &new_session_id)
}

fn recent_messages(messages: &[SessionMessage], limit: usize) -> Vec<SessionMessage> {
    let mut recent = messages
        .iter()
        .rev()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    recent.reverse();
    recent
}

fn resume_query(messages: &[SessionMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|message| {
            matches!(message.role, MessageRole::User) && !message.content.trim().is_empty()
        })
        .or_else(|| {
            messages.iter().rev().find(|message| {
                matches!(message.role, MessageRole::Assistant) && !message.content.trim().is_empty()
            })
        })
        .map(|message| message.content.trim().to_string())
        .filter(|query| !query.is_empty())
}

fn build_session_resume_packet(
    state: &AppState,
    transcript: &SessionTranscript,
) -> Result<SessionResumePacket, ApiError> {
    let workspace_key = transcript
        .session
        .cwd
        .as_ref()
        .map(|path| path.display().to_string());
    let recent_messages = recent_messages(&transcript.messages, 12);
    let mut linked_memories = state
        .storage
        .list_memories_by_source_session(&transcript.session.id, 12)?;
    let mut seen_memory_ids = linked_memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<HashSet<_>>();
    let mut related_transcript_hits = Vec::new();

    if let Some(query) = resume_query(&transcript.messages).filter(|query| query.len() >= 3) {
        let (searched_memories, transcript_hits) = state.storage.search_memories(
            &query,
            workspace_key.as_deref(),
            Some(transcript.session.provider_id.as_str()),
            &[],
            false,
            6,
        )?;
        for memory in searched_memories {
            if seen_memory_ids.insert(memory.id.clone()) {
                linked_memories.push(memory);
            }
        }
        related_transcript_hits = transcript_hits
            .into_iter()
            .filter(|hit| hit.session_id != transcript.session.id)
            .take(6)
            .collect();
    }

    linked_memories.sort_by_key(|memory| std::cmp::Reverse(memory.updated_at));
    linked_memories.truncate(12);
    for memory in &linked_memories {
        let _ = state.storage.touch_memory(&memory.id);
    }

    Ok(SessionResumePacket {
        session: transcript.session.clone(),
        generated_at: Utc::now(),
        recent_messages,
        linked_memories,
        related_transcript_hits,
    })
}

pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    Ok(Json(list_sessions_from_state(
        &state,
        query.limit.unwrap_or(25),
    )?))
}

pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionTranscript>, ApiError> {
    Ok(Json(get_session_transcript(&state, &session_id)?))
}

pub(crate) async fn get_session_resume_packet(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResumePacket>, ApiError> {
    let transcript = get_session_transcript(&state, &session_id)?;
    Ok(Json(build_session_resume_packet(&state, &transcript)?))
}

pub(crate) async fn rename_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(payload): Json<SessionRenameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let title = payload.title.trim().to_string();
    rename_session_title(&state, &session_id, &title)?;
    Ok(Json(serde_json::json!({ "ok": true, "title": title })))
}

pub(crate) async fn fork_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(payload): Json<SessionForkRequest>,
) -> Result<Json<SessionMutationResponse>, ApiError> {
    let transcript =
        load_target_session_transcript(&state, &session_id, payload.target_session_id.as_deref())?;
    let forked = fork_transcript(&state, &transcript)?;
    Ok(Json(SessionMutationResponse {
        session: forked.session,
        messages: forked.messages,
    }))
}

pub(crate) async fn compact_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(payload): Json<SessionCompactRequest>,
) -> Result<Json<SessionMutationResponse>, ApiError> {
    let transcript = get_session_transcript(&state, &session_id)?;
    flush_memory_from_transcript(&state, &transcript).await?;
    let prompt = build_compact_prompt(&transcript)?;
    let fallback_alias = transcript.session.alias.clone();
    let (alias, provider) = resolve_alias_and_provider(
        &state,
        payload.alias.as_deref().or(Some(fallback_alias.as_str())),
    )
    .await?;
    let response: RunTaskResponse = execute_task_request(
        &state,
        &alias,
        &provider,
        TaskRequestInput {
            prompt,
            requested_model: payload.requested_model,
            session_id: None,
            cwd: payload.cwd.or_else(|| transcript.session.cwd.clone()),
            thinking_level: payload.thinking_level,
            attachments: Vec::new(),
            permission_preset: payload.permission_preset,
            task_mode: payload.task_mode,
            output_schema_json: None,
            remote_content_policy_override: None,
            persist: false,
            background: false,
            delegation_depth: 0,
            cancellation: None,
        },
    )
    .await?;
    let compacted = compact_session_summary(
        &state,
        &transcript,
        &response.response,
        payload.task_mode.or(transcript.session.task_mode),
    )?;
    Ok(Json(SessionMutationResponse {
        session: compacted.session,
        messages: compacted.messages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicBool, Arc};

    use agent_core::{AppConfig, MemoryEvidenceRef, MemoryKind, MemoryRecord, MemoryScope};
    use agent_storage::Storage;
    use tokio::sync::{mpsc, Notify, RwLock};
    use uuid::Uuid;

    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, ProviderRateLimiter,
    };

    fn test_state() -> AppState {
        AppState {
            storage: Storage::open_at(
                std::env::temp_dir().join(format!("agent-daemon-sessions-test-{}", Uuid::new_v4())),
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
    fn build_session_resume_packet_includes_recent_messages_and_memory_links() {
        let state = test_state();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        let cwd = std::env::temp_dir().join(format!("agent-resume-packet-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&cwd).unwrap();

        state
            .storage
            .ensure_session_with_title(
                "session-1",
                Some("Daily session"),
                &alias,
                "openai",
                "gpt-4.1",
                Some(TaskMode::Daily),
                Some(cwd.as_path()),
            )
            .unwrap();
        state
            .storage
            .append_message(&SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                "I prefer concise output.".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ))
            .unwrap();
        state
            .storage
            .append_message(&SessionMessage::new(
                "session-1".to_string(),
                MessageRole::Assistant,
                "I will keep responses concise.".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ))
            .unwrap();

        state
            .storage
            .ensure_session_with_title(
                "session-2",
                Some("Related session"),
                &alias,
                "openai",
                "gpt-4.1",
                Some(TaskMode::Daily),
                Some(cwd.as_path()),
            )
            .unwrap();
        state
            .storage
            .append_message(&SessionMessage::new(
                "session-2".to_string(),
                MessageRole::User,
                "I prefer concise output in future sessions.".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ))
            .unwrap();

        let mut memory = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output.".to_string(),
        );
        memory.source_session_id = Some("session-1".to_string());
        memory.evidence_refs = vec![MemoryEvidenceRef {
            session_id: "session-1".to_string(),
            message_id: Some("message-1".to_string()),
            role: Some(MessageRole::User),
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }];
        state.storage.upsert_memory(&memory).unwrap();

        let transcript = get_session_transcript(&state, "session-1").unwrap();
        let packet = build_session_resume_packet(&state, &transcript).unwrap();

        assert_eq!(packet.session.id, "session-1");
        assert_eq!(packet.recent_messages.len(), 2);
        assert!(packet
            .linked_memories
            .iter()
            .any(|linked| linked.id == memory.id));
        assert!(packet
            .related_transcript_hits
            .iter()
            .any(|hit| hit.session_id == "session-2"));

        let _ = std::fs::remove_dir_all(cwd);
    }

    #[test]
    fn build_compact_prompt_truncates_multibyte_transcript_safely() {
        let transcript = SessionTranscript {
            session: SessionSummary {
                id: "session-1".to_string(),
                title: Some("Unicode session".to_string()),
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-5".to_string(),
                message_count: 1,
                updated_at: Utc::now(),
                created_at: Utc::now(),
                cwd: None,
                task_mode: None,
            },
            messages: vec![SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                format!("{}😀tail", "a".repeat(99_990)),
                Some("openai".to_string()),
                Some("gpt-5".to_string()),
            )],
        };

        let prompt = build_compact_prompt(&transcript).unwrap();
        assert!(prompt.contains("[truncated]"));
        assert!(!prompt.contains("😀tail"));
    }
}
