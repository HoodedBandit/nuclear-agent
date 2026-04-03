use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

use agent_core::{truncate_with_suffix, SkillDraft, SkillDraftStatus, ToolExecutionRecord};

use crate::AppState;

use super::{summarize_preview, workspace_key};

pub(super) fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn find_git_root(start: &FsPath) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".git").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

pub(super) fn workflow_title_from_prompt(
    prompt: &str,
    tool_events: &[ToolExecutionRecord],
) -> String {
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

pub(super) fn workflow_instructions(
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
        content = truncate_with_suffix(&content, 8_000, "\n\n[truncated]");
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
