use agent_core::{PatternType, ToolExecutionOutcome, ToolExecutionRecord, UsagePattern};
use chrono::Utc;

use crate::AppState;

/// Detect recurring usage patterns from tool execution events.
pub(crate) fn detect_patterns(
    tool_events: &[ToolExecutionRecord],
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
) -> Vec<UsagePattern> {
    let mut patterns = Vec::new();

    // 1. Detect tool sequences (2+ consecutive tools used together).
    if tool_events.len() >= 2 {
        let sequence_names: Vec<&str> = tool_events
            .iter()
            .filter(|e| matches!(e.outcome, ToolExecutionOutcome::Success))
            .map(|e| e.name.as_str())
            .collect();
        if sequence_names.len() >= 2 {
            let desc = format!(
                "Tool sequence: {}",
                sequence_names
                    .iter()
                    .take(6)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" → ")
            );
            let trigger = tool_events
                .first()
                .map(|e| truncate(&e.arguments, 120))
                .unwrap_or_default();
            let mut pattern = UsagePattern::new(PatternType::ToolSequence, desc, trigger);
            pattern.workspace_key = workspace_key.map(ToOwned::to_owned);
            pattern.provider_id = provider_id.map(ToOwned::to_owned);
            pattern.confidence = 40;
            patterns.push(pattern);
        }
    }

    // 2. Detect error-recovery patterns (tool fails then a similar tool succeeds).
    let mut i = 0;
    while i + 1 < tool_events.len() {
        let current = &tool_events[i];
        let next = &tool_events[i + 1];
        if matches!(current.outcome, ToolExecutionOutcome::Error)
            && matches!(next.outcome, ToolExecutionOutcome::Success)
            && current.name == next.name
        {
            let desc = format!(
                "Error recovery for {}: retried with different args after failure",
                current.name
            );
            let trigger = format!(
                "When {} fails with: {}",
                current.name,
                truncate(&current.output, 80)
            );
            let mut pattern = UsagePattern::new(PatternType::ErrorRecovery, desc, trigger);
            pattern.workspace_key = workspace_key.map(ToOwned::to_owned);
            pattern.provider_id = provider_id.map(ToOwned::to_owned);
            pattern.confidence = 55;
            patterns.push(pattern);
            i += 2;
        } else {
            i += 1;
        }
    }

    // 3. Detect preferred workflows — tools used with consistent argument patterns.
    let mut tool_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for event in tool_events
        .iter()
        .filter(|e| matches!(e.outcome, ToolExecutionOutcome::Success))
    {
        *tool_counts.entry(&event.name).or_default() += 1;
    }
    for (tool_name, count) in &tool_counts {
        if *count >= 3 {
            let desc = format!(
                "Preferred workflow: {} used {} times in this interaction",
                tool_name, count
            );
            let mut pattern =
                UsagePattern::new(PatternType::PreferredWorkflow, desc, String::new());
            pattern.workspace_key = workspace_key.map(ToOwned::to_owned);
            pattern.provider_id = provider_id.map(ToOwned::to_owned);
            pattern.confidence = 45;
            patterns.push(pattern);
        }
    }

    patterns
}

/// Record detected patterns, incrementing frequency for known ones.
pub(crate) fn record_patterns(
    state: &AppState,
    patterns: Vec<UsagePattern>,
) -> Result<(), anyhow::Error> {
    for mut pattern in patterns {
        let workspace_key = pattern.workspace_key.as_deref();
        if let Some(existing) = state
            .storage
            .find_pattern_by_description(&pattern.description, workspace_key)?
        {
            state.storage.increment_pattern_frequency(&existing.id)?;
        } else {
            pattern.last_seen_at = Utc::now();
            state.storage.upsert_pattern(&pattern)?;
        }
    }
    Ok(())
}

/// Build a guidance string from stored patterns for system prompt injection.
pub(crate) fn load_pattern_guidance(
    state: &AppState,
    workspace_key: Option<&str>,
    _provider_id: Option<&str>,
    limit: usize,
) -> Result<String, anyhow::Error> {
    let patterns = state.storage.list_patterns(limit, workspace_key)?;
    // Only surface patterns seen at least twice with reasonable confidence.
    let relevant: Vec<&UsagePattern> = patterns
        .iter()
        .filter(|p| p.frequency >= 2 && p.confidence >= 40)
        .take(limit)
        .collect();
    if relevant.is_empty() {
        return Ok(String::new());
    }
    let mut lines = vec!["Observed usage patterns from prior interactions:".to_string()];
    for pattern in relevant {
        let kind_label = match pattern.pattern_type {
            PatternType::ToolSequence => "sequence",
            PatternType::ErrorRecovery => "recovery",
            PatternType::PreferredWorkflow => "workflow",
            PatternType::AvoidedAction => "avoid",
        };
        lines.push(format!(
            "- [{}] {} (seen {}x)",
            kind_label, pattern.description, pattern.frequency
        ));
        if !pattern.trigger_hint.is_empty() {
            lines.push(format!("  trigger: {}", pattern.trigger_hint));
        }
    }
    Ok(lines.join("\n"))
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut s = input.chars().take(max_chars).collect::<String>();
    s.push_str("...");
    s
}
