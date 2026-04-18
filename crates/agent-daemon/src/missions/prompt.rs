use std::{fs, path::Path as FsPath};

use agent_core::{Mission, MissionCheckpoint, MissionPhase, MissionStatus};
use chrono::Utc;

use crate::normalize_memory_sentence;

pub(crate) const EVOLVE_DIRECTIVE_SCHEMA: &str = r#"{
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

pub(crate) fn build_mission_prompt(mission: &Mission, checkpoints: &[MissionCheckpoint]) -> String {
    if mission.evolve {
        let workspace_root = mission
            .workspace_key
            .as_deref()
            .map(FsPath::new)
            .unwrap_or_else(|| FsPath::new("."));
        let signals = gather_evolve_signals(workspace_root);
        return build_evolve_prompt(mission, checkpoints, &signals);
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

fn build_evolve_prompt(
    mission: &Mission,
    checkpoints: &[MissionCheckpoint],
    signals: &[String],
) -> String {
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
    if !signals.is_empty() {
        prompt.push_str("\nImprovement signals gathered from the workspace:\n");
        for signal in signals.iter().take(10) {
            prompt.push_str(&format!("- {}\n", signal));
        }
        prompt.push_str("Use these signals to choose your improvement target for this cycle.\n");
    }
    prompt.push_str(
        "\nBefore finishing this cycle, run `git diff` (or equivalent) to review ALL changes you made. Include the review in the diff_summary field.\n\nReturn a single JSON object only. Use snake_case fields with this shape:\n{\n  \"status\": \"waiting|blocked|completed|failed|running|scheduled|queued|cancelled\",\n  \"next_wake_seconds\": 30,\n  \"next_phase\": \"planner|executor|reviewer\",\n  \"handoff_summary\": \"condensed context for the next evolve cycle\",\n  \"summary\": \"short status summary\",\n  \"error\": \"optional blocker description\",\n  \"continue_evolving\": true,\n  \"improvement_goal\": \"one bounded target for the cycle\",\n  \"verification_summary\": \"what verification you ran and the result\",\n  \"diff_summary\": \"review of all changes made in this cycle and why\",\n  \"restart_required\": false,\n  \"follow_up_title\": \"optional child mission title\",\n  \"follow_up_details\": \"optional child mission details\",\n  \"follow_up_after_seconds\": 300\n}\nSet continue_evolving=false only if you are satisfied or blocked. Always include improvement_goal, verification_summary, and diff_summary in evolve mode.",
    );
    prompt
}

fn gather_evolve_signals(workspace_root: &FsPath) -> Vec<String> {
    let mut signals = Vec::new();

    let mut todo_count = 0usize;
    for crate_dir in [
        "crates/agent-core",
        "crates/agent-daemon",
        "crates/agent-storage",
        "crates/agent-providers",
        "crates/agent-cli",
        "crates/agent-policy",
    ] {
        let src = workspace_root.join(crate_dir).join("src");
        if let Ok(entries) = fs::read_dir(&src) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for line in content.lines() {
                            let upper = line.to_ascii_uppercase();
                            if upper.contains("TODO")
                                || upper.contains("FIXME")
                                || upper.contains("HACK")
                            {
                                todo_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    if todo_count > 0 {
        signals.push(format!(
            "Found {} TODO/FIXME/HACK comments in Rust source files",
            todo_count
        ));
    }

    for entry in [
        "crates/agent-daemon/src/lib.rs",
        "crates/agent-daemon/src/missions.rs",
        "crates/agent-daemon/src/runtime.rs",
        "crates/agent-daemon/src/memory.rs",
        "crates/agent-storage/src/lib.rs",
        "crates/agent-core/src/lib.rs",
    ] {
        let path = workspace_root.join(entry);
        if let Ok(content) = fs::read_to_string(&path) {
            let line_count = content.lines().count();
            if line_count > 800 {
                signals.push(format!(
                    "{} has {} lines - consider splitting",
                    entry, line_count
                ));
            }
        }
    }

    if let Ok(output) = std::process::Command::new("cargo")
        .args(["clippy", "--workspace", "--message-format=short", "--quiet"])
        .current_dir(workspace_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let warning_lines: Vec<&str> = stderr
            .lines()
            .filter(|line| line.contains("warning:"))
            .collect();
        if !warning_lines.is_empty() {
            signals.push(format!("{} clippy warnings detected", warning_lines.len()));
            for warning in warning_lines.iter().take(3) {
                signals.push(format!("  - {}", warning.trim()));
            }
        }
    }

    signals
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
            "handoff_summary" if !value.is_empty() => {
                directive.handoff_summary = Some(value.to_string());
            }
            "summary" => directive.summary = Some(value.to_string()),
            "error" if !value.is_empty() => {
                directive.error = Some(value.to_string());
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
            "improvement_goal" if !value.is_empty() => {
                directive.improvement_goal = Some(value.to_string());
            }
            "verification_summary" if !value.is_empty() => {
                directive.verification_summary = Some(value.to_string());
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
