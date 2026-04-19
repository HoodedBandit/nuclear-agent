use std::{collections::HashSet, path::Path};

use agent_core::{
    MessageRole, ModelAlias, SessionResumePacket, SessionSearchHit, SessionSummary,
    SessionTranscript,
};
use agent_storage::Storage;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use copypasta::{ClipboardContext, ClipboardProvider};
use dialoguer::{theme::ColorfulTheme, FuzzySelect, Select};
use uuid::Uuid;

pub(crate) const SESSION_PICKER_LIMIT: usize = 5_000;

pub(crate) fn format_session_search_hits(hits: &[SessionSearchHit]) -> String {
    if hits.is_empty() {
        return "No related transcript hits.".to_string();
    }
    hits.iter()
        .map(|hit| {
            format!(
                "session={} [{:?}] {}\n  at {}",
                hit.session_id, hit.role, hit.preview, hit.created_at
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_session_resume_packet(packet: &SessionResumePacket) -> String {
    let recent_messages = if packet.recent_messages.is_empty() {
        "No recent messages.".to_string()
    } else {
        packet
            .recent_messages
            .iter()
            .map(|message| {
                format!(
                    "[{:?}] {}",
                    message.role,
                    format_session_message_for_display(message)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let linked_memories = crate::format_memory_records(&packet.linked_memories);
    let transcript_hits = format_session_search_hits(&packet.related_transcript_hits);
    format!(
        "session={} title={} alias={} provider={} model={} mode={}\n\nRecent messages:\n{}\n\nLinked memories:\n{}\n\nRelated transcript hits:\n{}",
        packet.session.id,
        packet.session.title.as_deref().unwrap_or("(untitled)"),
        packet.session.alias,
        packet.session.provider_id,
        packet.session.model,
        crate::task_mode_label(packet.session.task_mode),
        recent_messages,
        linked_memories,
        transcript_hits
    )
}

fn recent_session_messages(
    messages: &[agent_core::SessionMessage],
    limit: usize,
) -> Vec<agent_core::SessionMessage> {
    let mut recent = messages
        .iter()
        .rev()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    recent.reverse();
    recent
}

fn resume_query_from_messages(messages: &[agent_core::SessionMessage]) -> Option<String> {
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

fn build_local_session_resume_packet(
    storage: &Storage,
    transcript: &SessionTranscript,
) -> Result<SessionResumePacket> {
    let recent_messages = recent_session_messages(&transcript.messages, 12);
    let workspace_key = transcript
        .session
        .cwd
        .as_ref()
        .map(|path| path.display().to_string());
    let mut linked_memories =
        storage.list_memories_by_source_session(&transcript.session.id, 12)?;
    let mut seen_memory_ids = linked_memories
        .iter()
        .map(|memory| memory.id.clone())
        .collect::<HashSet<_>>();
    let mut related_transcript_hits = Vec::new();

    if let Some(query) =
        resume_query_from_messages(&transcript.messages).filter(|query| query.len() >= 3)
    {
        let (searched_memories, transcript_hits) = storage.search_memories(
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
        let _ = storage.touch_memory(&memory.id);
    }

    Ok(SessionResumePacket {
        session: transcript.session.clone(),
        generated_at: Utc::now(),
        recent_messages,
        linked_memories,
        related_transcript_hits,
    })
}

pub(crate) async fn load_session_resume_packet(
    storage: &Storage,
    session_id: &str,
) -> Result<SessionResumePacket> {
    if let Some(client) = crate::try_daemon(storage).await? {
        return client
            .get(&format!("/v1/sessions/{session_id}/resume-packet"))
            .await;
    }
    let session = storage
        .get_session(session_id)?
        .ok_or_else(|| anyhow!("unknown session"))?;
    let transcript = SessionTranscript {
        session,
        messages: storage.list_session_messages(session_id)?,
    };
    build_local_session_resume_packet(storage, &transcript)
}

pub(crate) fn load_last_assistant_output(
    storage: &Storage,
    session_id: Option<&str>,
) -> Result<Option<String>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let messages = storage.list_session_messages(session_id)?;
    Ok(latest_nonempty_assistant_message(messages.iter()))
}

pub(crate) fn latest_assistant_output_from_transcript(
    transcript: &SessionTranscript,
) -> Option<String> {
    latest_nonempty_assistant_message(transcript.messages.iter())
}

fn latest_nonempty_assistant_message<'a>(
    messages: impl DoubleEndedIterator<Item = &'a agent_core::SessionMessage>,
) -> Option<String> {
    let mut fallback = None;
    for message in messages.rev() {
        if message.role != MessageRole::Assistant {
            continue;
        }
        if !message.content.trim().is_empty() {
            return Some(message.content.clone());
        }
        if fallback.is_none() {
            fallback = Some(message.content.clone());
        }
    }
    fallback
}

pub(crate) fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = ClipboardContext::new()
        .map_err(|error| anyhow!("failed to access system clipboard: {error}"))?;
    clipboard
        .set_contents(text.to_string())
        .map_err(|error| anyhow!("failed to write to system clipboard: {error}"))
}

pub(crate) fn init_agents_file(path: &Path) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    std::fs::write(path, build_agents_template(path.parent()))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

fn build_agents_template(parent: Option<&Path>) -> String {
    let location = parent
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    format!(
        "# AGENTS.md\n\n## Project Guidance\n- Describe what lives under {location}.\n- List the most important build, test, and run commands.\n- Call out code style, review expectations, and risky areas.\n\n## Guardrails\n- Document paths or systems the agent should avoid editing.\n- Note approval expectations for destructive changes.\n\n## Verification\n- List the commands the agent should run before considering work complete.\n"
    )
}

pub(crate) fn build_compact_prompt(transcript: &SessionTranscript) -> Result<String> {
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
        bail!("current session has no transcript to compact");
    }

    let history = crate::truncate_for_prompt(history, 100_000);
    Ok(format!(
        "Summarize this session so a fresh agent can continue it with minimal context loss. Preserve the user's goals, decisions, current status, important files or artifacts, unresolved questions, and any concrete next steps. Be concise but complete.\n\nTranscript:\n```text\n{history}\n```"
    ))
}

pub(crate) fn format_session_message_for_display(message: &agent_core::SessionMessage) -> String {
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

pub(crate) fn compact_session(
    storage: &Storage,
    transcript: &SessionTranscript,
    summary: &str,
) -> Result<String> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        transcript.session.task_mode,
        transcript.session.cwd.as_deref(),
    )?;
    storage.append_message(&agent_core::SessionMessage::new(
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
    Ok(new_session_id)
}

pub(crate) fn load_transcript_for_interactive_resume(
    storage: &Storage,
    target: Option<&str>,
) -> Result<SessionTranscript> {
    match target {
        Some("last") => load_session_for_command(storage, None, true, false),
        Some(session_id) => {
            load_session_for_command(storage, Some(session_id.to_string()), false, false)
        }
        None => load_session_for_command(storage, None, false, true),
    }
}

pub(crate) fn load_transcript_for_interactive_fork(
    storage: &Storage,
    current_session_id: Option<&str>,
    target: Option<&str>,
) -> Result<SessionTranscript> {
    match target {
        Some("last") => load_session_for_command(storage, None, true, false),
        Some(session_id) => {
            load_session_for_command(storage, Some(session_id.to_string()), false, false)
        }
        None => {
            if let Some(current_session_id) = current_session_id {
                load_session_for_command(
                    storage,
                    Some(current_session_id.to_string()),
                    false,
                    false,
                )
            } else {
                load_session_for_command(storage, None, false, true)
            }
        }
    }
}

pub(crate) fn load_session_for_command(
    storage: &Storage,
    requested_id: Option<String>,
    last: bool,
    show_all: bool,
) -> Result<SessionTranscript> {
    let session = if let Some(session_id) = requested_id {
        storage
            .get_session(&session_id)?
            .ok_or_else(|| anyhow!("unknown session"))?
    } else if last {
        storage
            .list_sessions(1)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("no recorded sessions found"))?
    } else {
        let sessions =
            rank_sessions_for_picker(storage.list_sessions(SESSION_PICKER_LIMIT)?, show_all)?;
        if sessions.is_empty() {
            bail!("no recorded sessions found");
        }
        select_session_interactively(&sessions, show_all)?
    };

    Ok(SessionTranscript {
        messages: storage.list_session_messages(&session.id)?,
        session,
    })
}

fn select_session_interactively(
    sessions: &[SessionSummary],
    show_all: bool,
) -> Result<SessionSummary> {
    let theme = ColorfulTheme::default();
    let items = sessions
        .iter()
        .map(|session| {
            let title = session.title.as_deref().unwrap_or("(untitled)");
            let cwd = session
                .cwd
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_string());
            if show_all {
                format!(
                    "{} | {} | {} | {} | {} | {} | {}",
                    session.id,
                    title,
                    session.alias,
                    session.provider_id,
                    crate::task_mode_label(session.task_mode),
                    cwd,
                    session.updated_at
                )
            } else {
                format!(
                    "{} | {} | {} | {} | {} | {}",
                    session.id,
                    title,
                    session.alias,
                    crate::task_mode_label(session.task_mode),
                    cwd,
                    session.updated_at
                )
            }
        })
        .collect::<Vec<_>>();
    let choice = if items.len() > 8 {
        FuzzySelect::with_theme(&theme)
            .with_prompt("Select a session")
            .items(&items)
            .default(0)
            .interact()?
    } else {
        Select::with_theme(&theme)
            .with_prompt("Select a session")
            .items(&items)
            .default(0)
            .interact()?
    };
    Ok(sessions[choice].clone())
}

pub(crate) fn rank_sessions_for_picker(
    mut sessions: Vec<SessionSummary>,
    show_all: bool,
) -> Result<Vec<SessionSummary>> {
    if show_all {
        return Ok(sessions);
    }
    let cwd = crate::current_request_cwd().ok();
    if let Some(cwd) = cwd {
        let matching = sessions
            .iter()
            .filter(|session| {
                session
                    .cwd
                    .as_deref()
                    .is_some_and(|value| value.starts_with(&cwd) || cwd.starts_with(value))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !matching.is_empty() {
            return Ok(matching);
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    Ok(sessions)
}

pub(crate) fn fork_session(storage: &Storage, transcript: &SessionTranscript) -> Result<String> {
    let new_session_id = Uuid::new_v4().to_string();
    let alias = ModelAlias {
        alias: transcript.session.alias.clone(),
        provider_id: transcript.session.provider_id.clone(),
        model: transcript.session.model.clone(),
        description: None,
    };
    storage.ensure_session_with_title(
        &new_session_id,
        transcript.session.title.as_deref(),
        &alias,
        &transcript.session.provider_id,
        &transcript.session.model,
        transcript.session.task_mode,
        transcript.session.cwd.as_deref(),
    )?;
    for message in &transcript.messages {
        storage.append_message(&message.fork_to_session(new_session_id.clone()))?;
    }
    Ok(new_session_id)
}
