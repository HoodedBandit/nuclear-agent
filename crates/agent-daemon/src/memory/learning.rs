use std::path::Path as FsPath;

use agent_core::{
    ConversationMessage, MemoryEvidenceRef, MemoryKind, MemoryRecord, MemoryReviewStatus,
    MemoryScope, MessageRole, PermissionPreset, SessionMessage, SessionTranscript, SkillDraft,
    SkillDraftStatus, ToolExecutionOutcome, ToolExecutionRecord,
};
use chrono::{DateTime, Utc};
use tracing::warn;

use crate::{append_log, ApiError, AppState};

use super::{
    guidance::{workflow_instructions, workflow_title_from_prompt},
    load_conflicting_memories, mark_memory_supersessions, maybe_compute_embedding,
    normalize_memory_sentence, resolve_memory_conflicts, workspace_key, MemoryRebuildStats,
};

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

    if tool_events.len() >= 2 {
        let workspace_key = cwd.map(|path| path.display().to_string());
        let detected = crate::detect_patterns(tool_events, workspace_key.as_deref(), provider_id);
        if !detected.is_empty() {
            if let Err(err) = crate::record_patterns(state, detected) {
                warn!("failed to record usage patterns: {err}");
            }
        }
    }

    Ok(())
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
        .filter(|event| matches!(event.outcome, ToolExecutionOutcome::Success))
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
pub(super) struct MemoryObservation {
    text: String,
    pub(super) source_tag: String,
    confidence_adjustment: i16,
    pub(super) source_message_id: Option<String>,
    role: Option<MessageRole>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    created_at: DateTime<Utc>,
}

impl MemoryObservation {
    pub(super) fn new(
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
            role: None,
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }
    }

    fn with_message_evidence(
        mut self,
        role: MessageRole,
        source_message_id: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        self.role = Some(role);
        if source_message_id.is_some() {
            self.source_message_id = source_message_id;
        }
        self.created_at = created_at;
        self
    }

    fn with_tool_evidence(
        mut self,
        source_message_id: Option<String>,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        self = self.with_message_evidence(MessageRole::Tool, source_message_id, created_at);
        self.tool_call_id = tool_call_id;
        self.tool_name = tool_name;
        self
    }

    pub(super) fn evidence_refs(&self, session_id: &str) -> Vec<MemoryEvidenceRef> {
        if self.role.is_none()
            && self.source_message_id.is_none()
            && self.tool_call_id.is_none()
            && self.tool_name.is_none()
        {
            return Vec::new();
        }
        vec![MemoryEvidenceRef {
            session_id: session_id.to_string(),
            message_id: self.source_message_id.clone(),
            role: self.role.clone(),
            tool_call_id: self.tool_call_id.clone(),
            tool_name: self.tool_name.clone(),
            created_at: self.created_at,
        }]
    }
}

fn learning_observations(
    prompt: &str,
    response: &str,
    transcript_messages: &[ConversationMessage],
) -> Vec<MemoryObservation> {
    let mut observations = Vec::new();
    if !prompt.trim().is_empty() {
        observations.push(
            MemoryObservation::new(prompt, "user_prompt", 0, None).with_message_evidence(
                MessageRole::User,
                None,
                Utc::now(),
            ),
        );
    }
    if !response.trim().is_empty() {
        observations.push(
            MemoryObservation::new(response, "assistant_reply", -8, None).with_message_evidence(
                MessageRole::Assistant,
                None,
                Utc::now(),
            ),
        );
    }
    for message in transcript_messages.iter().rev().take(8).rev() {
        match message.role {
            MessageRole::Tool => {
                if let Some(text) = learning_text_from_tool_message(message) {
                    let source_tag = match message.tool_name.as_deref() {
                        Some(name) if !name.trim().is_empty() => format!("tool:{name}"),
                        _ => "tool_output".to_string(),
                    };
                    observations.push(
                        MemoryObservation::new(text, source_tag, -12, None).with_tool_evidence(
                            None,
                            message.tool_call_id.clone(),
                            message.tool_name.clone(),
                            Utc::now(),
                        ),
                    );
                }
            }
            MessageRole::Assistant => {
                if !message.content.trim().is_empty() {
                    observations.push(
                        MemoryObservation::new(
                            message.content.clone(),
                            "assistant_transcript",
                            -10,
                            None,
                        )
                        .with_message_evidence(
                            MessageRole::Assistant,
                            None,
                            Utc::now(),
                        ),
                    );
                }
            }
            _ => {}
        }
    }
    observations
}

pub(super) fn learning_observations_from_session_messages(
    messages: &[SessionMessage],
) -> Vec<MemoryObservation> {
    let latest_assistant_message_id = messages
        .iter()
        .rev()
        .find(|message| {
            matches!(message.role, MessageRole::Assistant) && !message.content.trim().is_empty()
        })
        .map(|message| message.id.clone());

    let mut observations = Vec::new();
    for message in messages {
        match message.role {
            MessageRole::System => {}
            MessageRole::User => {
                if !message.content.trim().is_empty() {
                    observations.push(
                        MemoryObservation::new(
                            message.content.clone(),
                            "user_prompt",
                            0,
                            Some(message.id.clone()),
                        )
                        .with_message_evidence(
                            MessageRole::User,
                            Some(message.id.clone()),
                            message.created_at,
                        ),
                    );
                }
            }
            MessageRole::Assistant if !message.content.trim().is_empty() => {
                let source_tag =
                    if latest_assistant_message_id.as_deref() == Some(message.id.as_str()) {
                        "assistant_reply"
                    } else {
                        "assistant_transcript"
                    };
                let confidence_adjustment = if source_tag == "assistant_reply" {
                    -8
                } else {
                    -10
                };
                observations.push(
                    MemoryObservation::new(
                        message.content.clone(),
                        source_tag,
                        confidence_adjustment,
                        Some(message.id.clone()),
                    )
                    .with_message_evidence(
                        MessageRole::Assistant,
                        Some(message.id.clone()),
                        message.created_at,
                    ),
                );
            }
            MessageRole::Assistant => {}
            MessageRole::Tool => {
                if let Some(text) = learning_text_from_session_tool_message(message) {
                    let source_tag = match message.tool_name.as_deref() {
                        Some(name) if !name.trim().is_empty() => format!("tool:{name}"),
                        _ => "tool_output".to_string(),
                    };
                    observations.push(
                        MemoryObservation::new(text, source_tag, -12, Some(message.id.clone()))
                            .with_tool_evidence(
                                Some(message.id.clone()),
                                message.tool_call_id.clone(),
                                message.tool_name.clone(),
                                message.created_at,
                            ),
                    );
                }
            }
        }
    }
    observations
}

fn learning_text_from_tool_message(message: &ConversationMessage) -> Option<String> {
    learning_text_from_tool_content(&message.content)
}

fn learning_text_from_session_tool_message(message: &SessionMessage) -> Option<String> {
    learning_text_from_tool_content(&message.content)
}

fn learning_text_from_tool_content(content: &str) -> Option<String> {
    let trimmed = content.trim();
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

pub(super) fn extract_memory_candidates_from_source(
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
        memory.evidence_refs = observation.evidence_refs(session_id);
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
            for evidence in memory.evidence_refs {
                if !existing.evidence_refs.contains(&evidence) {
                    existing.evidence_refs.push(evidence);
                }
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

pub(super) async fn rebuild_memory_from_transcript(
    state: &AppState,
    transcript: &SessionTranscript,
    recompute_embeddings: bool,
) -> Result<MemoryRebuildStats, ApiError> {
    let observations = learning_observations_from_session_messages(&transcript.messages);
    let mut candidates = Vec::new();
    for observation in &observations {
        candidates.extend(extract_memory_candidates_from_source(
            observation,
            &transcript.session.id,
            Some(transcript.session.provider_id.as_str()),
            transcript.session.cwd.as_deref(),
        ));
    }

    let mut stats = MemoryRebuildStats {
        observations_scanned: observations.len(),
        ..MemoryRebuildStats::default()
    };
    for mut memory in dedupe_memories(candidates) {
        memory.review_status = classify_memory_review_status(&memory);
        let existing = load_conflicting_memories(state, &memory)?;
        let resolution = resolve_memory_conflicts(&mut memory, existing);
        state.storage.upsert_memory(&memory)?;
        mark_memory_supersessions(state, &memory, resolution.supersede_ids)?;
        stats.memories_upserted += 1;

        if recompute_embeddings {
            if let Err(err) = maybe_compute_embedding(state, &memory).await {
                warn!(
                    "failed to recompute embedding for rebuilt memory '{}': {err}",
                    memory.id
                );
            } else {
                stats.embeddings_refreshed += 1;
            }
        }
    }

    Ok(stats)
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

pub(super) fn summarize_preview(content: &str, max_chars: usize) -> String {
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
