use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use agent_core::{
    ConnectorApprovalRecord, ConnectorApprovalStatus, ConnectorApprovalUpdateRequest,
    ConnectorKind, MemoryRecord, MemoryReviewStatus, MemoryReviewUpdateRequest, SkillDraft,
    SkillDraftStatus, SkillUpdateRequest,
};
use agent_storage::Storage;
use anyhow::{anyhow, bail, Context, Result};

use super::{home_dir, try_daemon};

#[derive(Debug, Clone)]
pub(crate) struct SkillInfo {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) path: PathBuf,
}

pub(crate) async fn load_enabled_skills(storage: &Storage) -> Result<Vec<String>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/skills").await
    } else {
        Ok(storage.load_config()?.enabled_skills)
    }
}

pub(crate) async fn load_skill_drafts(
    storage: &Storage,
    limit: usize,
    status: Option<SkillDraftStatus>,
) -> Result<Vec<SkillDraft>> {
    if let Some(client) = try_daemon(storage).await? {
        let mut path = format!("/v1/skills/drafts?limit={limit}");
        if let Some(status) = status {
            path.push_str("&status=");
            path.push_str(match status {
                SkillDraftStatus::Draft => "draft",
                SkillDraftStatus::Published => "published",
                SkillDraftStatus::Rejected => "rejected",
            });
        }
        client.get(&path).await
    } else {
        storage.list_skill_drafts(limit, status, None, None)
    }
}

pub(crate) async fn load_profile_memories(
    storage: &Storage,
    limit: usize,
) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/profile?limit={limit}"))
            .await
    } else {
        let mut seen = HashSet::new();
        let mut memories = storage.list_memories_by_tag("system_profile", limit, None, None)?;
        memories.extend(storage.list_memories_by_tag("workspace_profile", limit, None, None)?);
        memories.retain(|memory| seen.insert(memory.id.clone()));
        memories.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        memories.truncate(limit);
        Ok(memories)
    }
}

pub(crate) async fn load_memory_review_queue(
    storage: &Storage,
    limit: usize,
) -> Result<Vec<MemoryRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!("/v1/memory/review?limit={limit}"))
            .await
    } else {
        storage.list_memories_by_review_status(MemoryReviewStatus::Candidate, limit)
    }
}

pub(crate) async fn load_connector_approvals(
    storage: &Storage,
    kind: ConnectorKind,
    limit: usize,
) -> Result<Vec<ConnectorApprovalRecord>> {
    if let Some(client) = try_daemon(storage).await? {
        client
            .get(&format!(
                "/v1/connector-approvals?kind={}&status=pending&limit={limit}",
                serde_json::to_string(&kind)?.trim_matches('"')
            ))
            .await
    } else {
        storage.list_connector_approvals(Some(kind), Some(ConnectorApprovalStatus::Pending), limit)
    }
}

pub(crate) async fn update_memory_review_status(
    storage: &Storage,
    id: &str,
    status: MemoryReviewStatus,
    note: Option<String>,
) -> Result<MemoryRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            MemoryReviewStatus::Accepted => format!("/v1/memory/{id}/approve"),
            MemoryReviewStatus::Rejected => format!("/v1/memory/{id}/reject"),
            MemoryReviewStatus::Candidate => {
                bail!("cannot set memory back to candidate from CLI")
            }
        };
        client
            .post(&path, &MemoryReviewUpdateRequest { status, note })
            .await
    } else {
        let updated = storage.update_memory_review_status(id, status, note.as_deref())?;
        if !updated {
            bail!("unknown memory '{id}'");
        }
        storage
            .get_memory(id)?
            .ok_or_else(|| anyhow!("unknown memory '{id}'"))
    }
}

pub(crate) async fn update_connector_approval_status(
    storage: &Storage,
    id: &str,
    status: ConnectorApprovalStatus,
    note: Option<String>,
) -> Result<ConnectorApprovalRecord> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            ConnectorApprovalStatus::Pending => {
                bail!("cannot set connector approval back to pending from CLI")
            }
            ConnectorApprovalStatus::Approved => {
                format!("/v1/connector-approvals/{id}/approve")
            }
            ConnectorApprovalStatus::Rejected => {
                format!("/v1/connector-approvals/{id}/reject")
            }
        };
        client
            .post(&path, &ConnectorApprovalUpdateRequest { note })
            .await
    } else {
        let updated =
            storage.update_connector_approval_status(id, status, note.as_deref(), None)?;
        if !updated {
            bail!("unknown connector approval '{id}'");
        }
        storage
            .get_connector_approval(id)?
            .ok_or_else(|| anyhow!("unknown connector approval '{id}'"))
    }
}

pub(crate) async fn update_skill_draft_status(
    storage: &Storage,
    id: &str,
    status: SkillDraftStatus,
) -> Result<SkillDraft> {
    if let Some(client) = try_daemon(storage).await? {
        let path = match status {
            SkillDraftStatus::Draft => bail!("cannot set skill draft back to draft from CLI"),
            SkillDraftStatus::Published => format!("/v1/skills/drafts/{id}/publish"),
            SkillDraftStatus::Rejected => format!("/v1/skills/drafts/{id}/reject"),
        };
        client.post(&path, &serde_json::json!({})).await
    } else {
        let mut draft = storage
            .get_skill_draft(id)?
            .ok_or_else(|| anyhow!("unknown skill draft '{id}'"))?;
        draft.status = status;
        draft.updated_at = chrono::Utc::now();
        storage.upsert_skill_draft(&draft)?;
        Ok(draft)
    }
}

pub(crate) fn format_memory_records(records: &[MemoryRecord]) -> String {
    if records.is_empty() {
        return "No stored memory.".to_string();
    }

    records
        .iter()
        .map(|memory| {
            let tags = if memory.tags.is_empty() {
                String::new()
            } else {
                format!(" tags={}", memory.tags.join(","))
            };
            let review = if matches!(memory.review_status, MemoryReviewStatus::Accepted) {
                String::new()
            } else {
                format!(" review={:?}", memory.review_status)
            };
            let note = memory
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            let source = match (
                memory.source_session_id.as_deref(),
                memory.source_message_id.as_deref(),
            ) {
                (Some(session_id), Some(message_id)) => {
                    format!("\n  source: session={session_id} message={message_id}")
                }
                (Some(session_id), None) => format!("\n  source: session={session_id}"),
                (None, Some(message_id)) => format!("\n  source: message={message_id}"),
                (None, None) => String::new(),
            };
            let evidence = if memory.evidence_refs.is_empty() {
                String::new()
            } else {
                let mut lines = memory
                    .evidence_refs
                    .iter()
                    .take(3)
                    .map(|evidence| {
                        let role = evidence
                            .role
                            .as_ref()
                            .map(|role| format!(" role={role:?}"))
                            .unwrap_or_default();
                        let message = evidence
                            .message_id
                            .as_deref()
                            .map(|value| format!(" message={value}"))
                            .unwrap_or_default();
                        let tool = match (
                            evidence.tool_name.as_deref(),
                            evidence.tool_call_id.as_deref(),
                        ) {
                            (Some(name), Some(call_id)) => format!(" tool={name}#{call_id}"),
                            (Some(name), None) => format!(" tool={name}"),
                            (None, Some(call_id)) => format!(" tool_call={call_id}"),
                            (None, None) => String::new(),
                        };
                        format!(
                            "\n    - session={}{}{}{} @ {}",
                            evidence.session_id, role, message, tool, evidence.created_at
                        )
                    })
                    .collect::<String>();
                if memory.evidence_refs.len() > 3 {
                    lines.push_str(&format!(
                        "\n    - ... {} more",
                        memory.evidence_refs.len() - 3
                    ));
                }
                format!("\n  evidence:{lines}")
            };
            format!(
                "{} [{:?}/{:?}] {}{}{}\n  {}{}{}{}",
                memory.id,
                memory.kind,
                memory.scope,
                memory.subject,
                tags,
                review,
                memory.content,
                source,
                note,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_connector_approvals(records: &[ConnectorApprovalRecord]) -> String {
    if records.is_empty() {
        return "No pending connector approvals.".to_string();
    }

    records
        .iter()
        .map(|approval| {
            let note = approval
                .review_note
                .as_deref()
                .map(|value| format!("\n  note: {value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] {} chat={} user={}\n  {}\n  {}{}",
                approval.id,
                approval.status,
                approval.connector_name,
                approval.external_chat_display.as_deref().unwrap_or("-"),
                approval.external_user_display.as_deref().unwrap_or("-"),
                approval.title,
                approval
                    .message_preview
                    .as_deref()
                    .unwrap_or(approval.details.as_str()),
                note
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn format_skill_drafts(drafts: &[SkillDraft]) -> String {
    if drafts.is_empty() {
        return "No learned skill drafts.".to_string();
    }

    drafts
        .iter()
        .map(|draft| {
            let trigger = draft
                .trigger_hint
                .as_deref()
                .map(|value| format!(" trigger={value}"))
                .unwrap_or_default();
            format!(
                "{} [{:?}] usage={}{}\n  {}\n  {}",
                draft.id, draft.status, draft.usage_count, trigger, draft.title, draft.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) async fn update_enabled_skill(
    storage: &Storage,
    name: &str,
    enabled: bool,
) -> Result<()> {
    let available = discover_skills()?;
    if enabled && !available.iter().any(|skill| skill.name == name) {
        bail!("unknown skill '{name}'");
    }
    let mut enabled_skills = load_enabled_skills(storage).await?;
    if enabled {
        if !enabled_skills.iter().any(|skill| skill == name) {
            enabled_skills.push(name.to_string());
        }
    } else {
        enabled_skills.retain(|skill| skill != name);
    }
    if let Some(client) = try_daemon(storage).await? {
        let _: Vec<String> = client
            .put(
                "/v1/skills",
                &SkillUpdateRequest {
                    enabled_skills: enabled_skills.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.enabled_skills = enabled_skills;
        storage.save_config(&config)?;
    }
    Ok(())
}

pub(crate) fn discover_skills() -> Result<Vec<SkillInfo>> {
    let Some(root) = codex_skills_root() else {
        return Ok(Vec::new());
    };
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    discover_skills_in_dir(&root, &mut skills)?;
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

fn discover_skills_in_dir(root: &Path, output: &mut Vec<SkillInfo>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            discover_skills_in_dir(&path, output)?;
            continue;
        }
        if entry.file_name().to_string_lossy() != "SKILL.md" {
            continue;
        }
        let name = path
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("failed to infer skill name from {}", path.display()))?;
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let description = extract_skill_description(&content);
        output.push(SkillInfo {
            name,
            description,
            path,
        });
    }
    Ok(())
}

fn codex_skills_root() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".codex").join("skills"))
}

fn extract_skill_description(content: &str) -> String {
    let lines = content.lines().map(str::trim).collect::<Vec<_>>();
    if lines.first().copied() == Some("---") {
        let mut in_frontmatter = true;
        for line in &lines[1..] {
            if *line == "---" {
                in_frontmatter = false;
                continue;
            }
            if in_frontmatter {
                if let Some(value) = line.strip_prefix("description:") {
                    return value.trim().trim_matches('"').to_string();
                }
            }
        }
    }

    lines
        .into_iter()
        .find(|line| !line.is_empty() && !line.starts_with('#') && *line != "---" && *line != "```")
        .unwrap_or("No description available.")
        .to_string()
}
