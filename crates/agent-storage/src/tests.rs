use super::*;
use agent_core::{
    ConnectorApprovalRecord, ConnectorApprovalStatus, ConnectorKind, MemoryEvidenceRef, MemoryKind,
    MemoryReviewStatus, MemoryScope, MessageRole, Mission, MissionStatus, SkillDraftStatus,
    TaskMode, ToolCall,
};

fn temp_storage() -> Storage {
    let root = std::env::temp_dir().join(format!("agent-storage-test-{}", Uuid::new_v4()));
    Storage::open_at(root).unwrap()
}

#[test]
fn persist_session_turn_round_trips_tool_metadata() {
    let storage = temp_storage();
    let alias = ModelAlias {
        alias: "main".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-4.1".to_string(),
        description: None,
    };
    let tool_call = ToolCall {
        id: "call-1".to_string(),
        name: "read_file".to_string(),
        arguments: "{\"path\":\"README.md\"}".to_string(),
    };
    let messages = vec![
        SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "inspect the file".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        ),
        SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Assistant,
            String::new(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )
        .with_tool_calls(vec![tool_call.clone()])
        .with_provider_payload(Some("{\"id\":\"resp-1\"}".to_string())),
        SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Tool,
            "file contents".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )
        .with_tool_metadata(Some("call-1".to_string()), Some("read_file".to_string())),
        SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Assistant,
            "done".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        ),
    ];

    storage
        .persist_session_turn(PersistSessionTurnInput {
            session_id: "session-1",
            title: Some("Test Session"),
            alias: &alias,
            provider_id: "openai",
            model: "gpt-4.1",
            task_mode: Some(TaskMode::Build),
            cwd: None,
            messages: &messages,
        })
        .unwrap();

    let persisted = storage.list_session_messages("session-1").unwrap();
    assert_eq!(persisted.len(), 4);
    assert_eq!(persisted[1].tool_calls, vec![tool_call]);
    assert_eq!(
        persisted[1].provider_payload_json.as_deref(),
        Some("{\"id\":\"resp-1\"}")
    );
    assert_eq!(persisted[2].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(persisted[2].tool_name.as_deref(), Some("read_file"));

    let session = storage.get_session("session-1").unwrap().unwrap();
    assert_eq!(session.title.as_deref(), Some("Test Session"));
    assert_eq!(session.alias, "main");
    assert_eq!(session.task_mode, Some(TaskMode::Build));
}

#[test]
fn persist_session_turn_preserves_existing_task_mode_when_unspecified() {
    let storage = temp_storage();
    let alias = ModelAlias {
        alias: "main".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-4.1".to_string(),
        description: None,
    };
    storage
        .ensure_session(
            "session-1",
            &alias,
            "openai",
            "gpt-4.1",
            Some(TaskMode::Daily),
        )
        .unwrap();
    let messages = vec![SessionMessage::new(
        "session-1".to_string(),
        MessageRole::User,
        "continue".to_string(),
        Some("openai".to_string()),
        Some("gpt-4.1".to_string()),
    )];

    storage
        .persist_session_turn(PersistSessionTurnInput {
            session_id: "session-1",
            title: None,
            alias: &alias,
            provider_id: "openai",
            model: "gpt-4.1",
            task_mode: None,
            cwd: None,
            messages: &messages,
        })
        .unwrap();

    let session = storage.get_session("session-1").unwrap().unwrap();
    assert_eq!(session.task_mode, Some(TaskMode::Daily));
}

#[test]
fn persist_session_turn_updates_existing_task_mode_when_explicit() {
    let storage = temp_storage();
    let alias = ModelAlias {
        alias: "main".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-4.1".to_string(),
        description: None,
    };
    storage
        .ensure_session(
            "session-1",
            &alias,
            "openai",
            "gpt-4.1",
            Some(TaskMode::Daily),
        )
        .unwrap();
    let messages = vec![SessionMessage::new(
        "session-1".to_string(),
        MessageRole::Assistant,
        "switched".to_string(),
        Some("openai".to_string()),
        Some("gpt-4.1".to_string()),
    )];

    storage
        .persist_session_turn(PersistSessionTurnInput {
            session_id: "session-1",
            title: None,
            alias: &alias,
            provider_id: "openai",
            model: "gpt-4.1",
            task_mode: Some(TaskMode::Build),
            cwd: None,
            messages: &messages,
        })
        .unwrap();

    let session = storage.get_session("session-1").unwrap().unwrap();
    assert_eq!(session.task_mode, Some(TaskMode::Build));
}

#[test]
fn reset_all_recreates_default_config_and_empty_database() {
    let storage = temp_storage();
    let config = AppConfig {
        onboarding_complete: true,
        ..AppConfig::default()
    };
    storage.save_config(&config).unwrap();
    storage
        .append_log(&LogEntry {
            id: "log-1".to_string(),
            level: "info".to_string(),
            scope: "test".to_string(),
            message: "hello".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let alias = ModelAlias {
        alias: "main".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-4.1".to_string(),
        description: None,
    };
    storage
        .ensure_session("session-1", &alias, "openai", "gpt-4.1", None)
        .unwrap();

    storage.reset_all().unwrap();

    let reset = storage.load_config().unwrap();
    let expected = AppConfig::default();
    assert_eq!(reset.version, expected.version);
    assert_eq!(reset.daemon.host, expected.daemon.host);
    assert_eq!(reset.daemon.port, expected.daemon.port);
    assert!(!reset.daemon.token.is_empty());
    assert_ne!(reset.daemon.token, config.daemon.token);
    assert_eq!(
        reset.daemon.persistence_mode,
        expected.daemon.persistence_mode
    );
    assert_eq!(reset.daemon.auto_start, expected.daemon.auto_start);
    assert_eq!(reset.main_agent_alias, expected.main_agent_alias);
    assert!(reset.providers.is_empty());
    assert!(reset.aliases.is_empty());
    assert_eq!(reset.thinking_level, expected.thinking_level);
    assert_eq!(reset.permission_preset, expected.permission_preset);
    assert_eq!(reset.trust_policy, expected.trust_policy);
    assert_eq!(reset.autonomy, expected.autonomy);
    assert!(reset.mcp_servers.is_empty());
    assert!(reset.app_connectors.is_empty());
    assert!(reset.enabled_skills.is_empty());
    assert!(!reset.onboarding_complete);
    assert!(storage.list_sessions(10).unwrap().is_empty());
    assert!(storage.list_logs(10).unwrap().is_empty());
    assert!(storage.paths().config_path.exists());
    assert!(storage.paths().db_path.exists());
}

#[test]
fn list_logs_after_returns_chronological_results_from_cursor() {
    let storage = temp_storage();
    let first = LogEntry {
        id: "log-1".to_string(),
        level: "info".to_string(),
        scope: "test".to_string(),
        message: "first".to_string(),
        created_at: Utc::now(),
    };
    let second = LogEntry {
        id: "log-2".to_string(),
        level: "info".to_string(),
        scope: "test".to_string(),
        message: "second".to_string(),
        created_at: first.created_at + chrono::Duration::seconds(1),
    };
    let third = LogEntry {
        id: "log-3".to_string(),
        level: "warn".to_string(),
        scope: "test".to_string(),
        message: "third".to_string(),
        created_at: second.created_at + chrono::Duration::seconds(1),
    };

    storage.append_log(&first).unwrap();
    storage.append_log(&second).unwrap();
    storage.append_log(&third).unwrap();

    let logs = storage.list_logs_after(second.created_at, 10).unwrap();
    let ids = logs.into_iter().map(|entry| entry.id).collect::<Vec<_>>();
    assert_eq!(ids, vec!["log-2".to_string(), "log-3".to_string()]);
}

#[test]
fn list_logs_after_cursor_skips_duplicate_entry_and_keeps_same_timestamp_peers() {
    let storage = temp_storage();
    let created_at = Utc::now();
    let first = LogEntry {
        id: "log-a".to_string(),
        level: "info".to_string(),
        scope: "test".to_string(),
        message: "first".to_string(),
        created_at,
    };
    let second = LogEntry {
        id: "log-b".to_string(),
        level: "info".to_string(),
        scope: "test".to_string(),
        message: "second".to_string(),
        created_at,
    };
    let third = LogEntry {
        id: "log-c".to_string(),
        level: "warn".to_string(),
        scope: "test".to_string(),
        message: "third".to_string(),
        created_at: created_at + chrono::Duration::seconds(1),
    };

    storage.append_log(&first).unwrap();
    storage.append_log(&second).unwrap();
    storage.append_log(&third).unwrap();

    let logs = storage
        .list_logs_after_cursor(created_at, Some("log-a"), 10)
        .unwrap();
    let ids = logs.into_iter().map(|entry| entry.id).collect::<Vec<_>>();
    assert_eq!(ids, vec!["log-b".to_string(), "log-c".to_string()]);
}

#[test]
fn mission_counts_and_limited_listing_round_trip() {
    let storage = temp_storage();

    let mut queued = Mission::new("Queued".to_string(), "Pending".to_string());
    queued.status = MissionStatus::Queued;
    queued.updated_at = Utc::now();
    storage.upsert_mission(&queued).unwrap();

    let mut waiting = Mission::new("Waiting".to_string(), "Sleeping".to_string());
    waiting.status = MissionStatus::Waiting;
    waiting.updated_at = queued.updated_at + chrono::Duration::seconds(1);
    storage.upsert_mission(&waiting).unwrap();

    let mut completed = Mission::new("Completed".to_string(), "Done".to_string());
    completed.status = MissionStatus::Completed;
    completed.updated_at = waiting.updated_at + chrono::Duration::seconds(1);
    storage.upsert_mission(&completed).unwrap();

    assert_eq!(storage.count_missions().unwrap(), 3);
    assert_eq!(storage.count_active_missions().unwrap(), 2);

    let limited = storage.list_missions_limited(Some(2)).unwrap();
    let ids = limited
        .into_iter()
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&completed.id));
    assert!(ids.contains(&waiting.id));
    assert!(!ids.contains(&queued.id));
}

#[test]
fn blocked_missions_are_not_runnable() {
    let storage = temp_storage();
    let now = Utc::now();

    let mut blocked = Mission::new("Blocked".to_string(), "Paused".to_string());
    blocked.status = MissionStatus::Blocked;
    blocked.updated_at = now;
    storage.upsert_mission(&blocked).unwrap();

    let mut queued = Mission::new("Queued".to_string(), "Ready".to_string());
    queued.status = MissionStatus::Queued;
    queued.updated_at = now + chrono::Duration::seconds(1);
    storage.upsert_mission(&queued).unwrap();

    let runnable = storage.list_runnable_missions(now, 10).unwrap();
    let ids = runnable
        .into_iter()
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    assert!(ids.contains(&queued.id));
    assert!(!ids.contains(&blocked.id));
}

#[test]
fn skill_draft_round_trips_and_filters_by_status() {
    let storage = temp_storage();
    let mut draft = SkillDraft::new(
        "Review auth workflow".to_string(),
        "Observed reusable auth workflow.".to_string(),
        "1. Read files\n2. Run tests".to_string(),
    );
    draft.workspace_key = Some("J:/repo".to_string());
    draft.provider_id = Some("openai".to_string());
    draft.status = SkillDraftStatus::Published;
    storage.upsert_skill_draft(&draft).unwrap();

    let stored = storage.get_skill_draft(&draft.id).unwrap().unwrap();
    assert_eq!(stored.title, draft.title);
    assert_eq!(stored.status, SkillDraftStatus::Published);

    let published = storage
        .list_skill_drafts(10, Some(SkillDraftStatus::Published), None, None)
        .unwrap();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].id, draft.id);
}

#[test]
fn touch_skill_draft_updates_last_used_without_mutating_updated_at() {
    let storage = temp_storage();
    let draft = SkillDraft::new(
        "Review auth workflow".to_string(),
        "Observed reusable auth workflow.".to_string(),
        "1. Read files\n2. Run tests".to_string(),
    );
    storage.upsert_skill_draft(&draft).unwrap();

    let before = storage.get_skill_draft(&draft.id).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    storage.touch_skill_draft(&draft.id).unwrap();
    let after = storage.get_skill_draft(&draft.id).unwrap().unwrap();

    assert_eq!(after.updated_at, before.updated_at);
    assert!(after.last_used_at.is_some());
    assert!(after.last_used_at >= before.last_used_at);
}

#[test]
fn candidate_memories_are_excluded_from_active_retrieval() {
    let storage = temp_storage();

    let accepted = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:theme".to_string(),
        "User prefers concise output.".to_string(),
    );
    storage.upsert_memory(&accepted).unwrap();

    let mut candidate = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Workspace,
        "workspace:stack".to_string(),
        "Project might use Rust and Tauri.".to_string(),
    );
    candidate.review_status = MemoryReviewStatus::Candidate;
    storage.upsert_memory(&candidate).unwrap();

    let active = storage.list_memories(10).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, accepted.id);

    let review_queue = storage
        .list_memories_by_review_status(MemoryReviewStatus::Candidate, 10)
        .unwrap();
    assert_eq!(review_queue.len(), 1);
    assert_eq!(review_queue[0].id, candidate.id);
}

#[test]
fn updating_memory_review_status_sets_review_metadata() {
    let storage = temp_storage();
    let mut memory = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "memory:test".to_string(),
        "Candidate memory".to_string(),
    );
    memory.review_status = MemoryReviewStatus::Candidate;
    storage.upsert_memory(&memory).unwrap();

    let updated = storage
        .update_memory_review_status(&memory.id, MemoryReviewStatus::Accepted, Some("validated"))
        .unwrap();
    assert!(updated);

    let stored = storage.get_memory(&memory.id).unwrap().unwrap();
    assert_eq!(stored.review_status, MemoryReviewStatus::Accepted);
    assert_eq!(stored.review_note.as_deref(), Some("validated"));
    assert!(stored.reviewed_at.is_some());
}

#[test]
fn touch_memory_updates_last_used_without_mutating_updated_at() {
    let storage = temp_storage();
    let memory = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:verbosity".to_string(),
        "User prefers concise output.".to_string(),
    );
    storage.upsert_memory(&memory).unwrap();

    let before = storage.get_memory(&memory.id).unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    storage.touch_memory(&memory.id).unwrap();
    let after = storage.get_memory(&memory.id).unwrap().unwrap();

    assert_eq!(after.updated_at, before.updated_at);
    assert!(after.last_used_at.is_some());
    assert!(after.last_used_at >= before.last_used_at);
}

#[test]
fn memory_evidence_refs_round_trip_through_storage() {
    let storage = temp_storage();
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
    storage.upsert_memory(&memory).unwrap();

    let stored = storage.get_memory(&memory.id).unwrap().unwrap();
    assert_eq!(stored.evidence_refs, memory.evidence_refs);

    let from_session = storage
        .list_memories_by_source_session("session-1", 10)
        .unwrap();
    assert_eq!(from_session.len(), 1);
    assert_eq!(from_session[0].evidence_refs, memory.evidence_refs);

    let (searched, transcript_hits) = storage
        .search_memories("concise output", None, None, &[], false, 10)
        .unwrap();
    assert!(transcript_hits.is_empty());
    assert_eq!(searched.len(), 1);
    assert_eq!(searched[0].evidence_refs, memory.evidence_refs);
}

#[test]
fn list_active_memories_by_identity_key_prefers_accepted_and_skips_superseded() {
    let storage = temp_storage();
    let identity_key = "preference:global:output";

    let mut accepted = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:output".to_string(),
        "User prefers concise output.".to_string(),
    );
    accepted.identity_key = Some(identity_key.to_string());
    accepted.updated_at = Utc::now();
    storage.upsert_memory(&accepted).unwrap();

    let mut candidate = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:output".to_string(),
        "User prefers detailed output.".to_string(),
    );
    candidate.identity_key = Some(identity_key.to_string());
    candidate.review_status = MemoryReviewStatus::Candidate;
    candidate.updated_at = accepted.updated_at + chrono::Duration::seconds(1);
    storage.upsert_memory(&candidate).unwrap();

    let mut superseded = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:output".to_string(),
        "User prefers terse output.".to_string(),
    );
    superseded.identity_key = Some(identity_key.to_string());
    superseded.review_status = MemoryReviewStatus::Candidate;
    superseded.superseded_by = Some(candidate.id.clone());
    superseded.updated_at = candidate.updated_at + chrono::Duration::seconds(1);
    storage.upsert_memory(&superseded).unwrap();

    let listed = storage
        .list_active_memories_by_identity_key(identity_key, None, None)
        .unwrap();
    let ids = listed
        .into_iter()
        .map(|memory| memory.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![accepted.id, candidate.id]);
}

#[test]
fn find_memory_by_identity_key_prefers_accepted_memory() {
    let storage = temp_storage();
    let identity_key = "preference:global:verbosity";

    let mut accepted = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:verbosity".to_string(),
        "User prefers concise output.".to_string(),
    );
    accepted.identity_key = Some(identity_key.to_string());
    accepted.updated_at = Utc::now();
    storage.upsert_memory(&accepted).unwrap();

    let mut newer_candidate = MemoryRecord::new(
        MemoryKind::Preference,
        MemoryScope::Global,
        "preference:verbosity".to_string(),
        "User prefers very detailed output.".to_string(),
    );
    newer_candidate.identity_key = Some(identity_key.to_string());
    newer_candidate.review_status = MemoryReviewStatus::Candidate;
    newer_candidate.updated_at = accepted.updated_at + chrono::Duration::seconds(1);
    storage.upsert_memory(&newer_candidate).unwrap();

    let found = storage
        .find_memory_by_identity_key(identity_key, None, None)
        .unwrap()
        .unwrap();
    assert_eq!(found.id, accepted.id);
    assert_eq!(found.review_status, MemoryReviewStatus::Accepted);
}

#[test]
fn normalize_fts_query_strips_question_mark_safely() {
    assert_eq!(
        normalize_fts_query("can you check the weather in chicago?"),
        "can you check the weather in chicago"
    );
}

#[test]
fn normalize_fts_query_splits_period_delimited_tokens_safely() {
    assert_eq!(
        normalize_fts_query("status for api.openai.com."),
        "status for api openai com"
    );
}

#[test]
fn normalize_fts_query_strips_operator_like_symbols_safely() {
    assert_eq!(
        normalize_fts_query("gpt-5_status -- ??? !!!"),
        "gpt 5 status"
    );
}

#[test]
fn normalize_fts_query_returns_empty_for_symbol_only_input() {
    assert_eq!(normalize_fts_query("!@#$%^&*()_-+=[]{}|;:'\",.<>/?`~"), "");
}

#[test]
fn search_memories_accepts_period_and_question_mark_queries() {
    let storage = temp_storage();
    let memory = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "weather:chicago".to_string(),
        "Check the weather in chicago with api.openai.com before replying.".to_string(),
    );
    storage.upsert_memory(&memory).unwrap();

    let (memories, transcript_hits) = storage
        .search_memories(
            "weather in chicago. api.openai.com?",
            None,
            None,
            &[],
            false,
            10,
        )
        .unwrap();

    assert_eq!(transcript_hits.len(), 0);
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].id, memory.id);
}

#[test]
fn search_memories_accepts_symbol_only_queries_without_error() {
    let storage = temp_storage();
    let memory = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "symbols:test".to_string(),
        "symbol stress note".to_string(),
    );
    storage.upsert_memory(&memory).unwrap();

    let (memories, transcript_hits) = storage
        .search_memories(
            "!@#$%^&*()_-+=[]{}|;:'\",.<>/?`~",
            None,
            None,
            &[],
            false,
            10,
        )
        .unwrap();

    assert!(memories.is_empty());
    assert!(transcript_hits.is_empty());
}

#[test]
fn search_memories_honors_review_status_and_superseded_filters() {
    let storage = temp_storage();

    let accepted = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "memory:accepted".to_string(),
        "alpha accepted memory".to_string(),
    );
    storage.upsert_memory(&accepted).unwrap();

    let mut candidate = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "memory:candidate".to_string(),
        "alpha candidate memory".to_string(),
    );
    candidate.review_status = MemoryReviewStatus::Candidate;
    storage.upsert_memory(&candidate).unwrap();

    let mut superseded = MemoryRecord::new(
        MemoryKind::Note,
        MemoryScope::Global,
        "memory:superseded".to_string(),
        "alpha superseded memory".to_string(),
    );
    superseded.superseded_by = Some(accepted.id.clone());
    storage.upsert_memory(&superseded).unwrap();

    let (default_memories, _) = storage
        .search_memories("alpha", None, None, &[], false, 10)
        .unwrap();
    let default_ids = default_memories
        .into_iter()
        .map(|memory| memory.id)
        .collect::<Vec<_>>();
    assert_eq!(default_ids, vec![accepted.id.clone()]);

    let (candidate_memories, _) = storage
        .search_memories(
            "alpha",
            None,
            None,
            &[MemoryReviewStatus::Candidate],
            false,
            10,
        )
        .unwrap();
    let candidate_ids = candidate_memories
        .into_iter()
        .map(|memory| memory.id)
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, vec![candidate.id.clone()]);

    let (all_memories, _) = storage
        .search_memories(
            "alpha",
            None,
            None,
            &[MemoryReviewStatus::Accepted, MemoryReviewStatus::Candidate],
            true,
            10,
        )
        .unwrap();
    let all_ids = all_memories
        .into_iter()
        .map(|memory| memory.id)
        .collect::<Vec<_>>();
    assert!(all_ids.contains(&accepted.id));
    assert!(all_ids.contains(&candidate.id));
    assert!(all_ids.contains(&superseded.id));
}

#[test]
fn connector_approvals_round_trip_and_count_pending() {
    let storage = temp_storage();
    let mut approval = ConnectorApprovalRecord::new(
        ConnectorKind::Telegram,
        "ops".to_string(),
        "Ops Bot".to_string(),
        "Ops telegram: hello".to_string(),
        "Telegram connector: Ops Bot".to_string(),
        "telegram:ops:chat:42:user:any".to_string(),
    );
    approval.external_chat_id = Some("42".to_string());
    approval.external_user_id = Some("7".to_string());
    approval.message_preview = Some("hello".to_string());
    storage.upsert_connector_approval(&approval).unwrap();

    assert_eq!(storage.count_pending_connector_approvals().unwrap(), 1);
    let listed = storage
        .list_connector_approvals(
            Some(ConnectorKind::Telegram),
            Some(ConnectorApprovalStatus::Pending),
            10,
        )
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].external_chat_id.as_deref(), Some("42"));

    let updated = storage
        .update_connector_approval_status(
            &approval.id,
            ConnectorApprovalStatus::Approved,
            Some("host approved"),
            Some("telegram:ops:10"),
        )
        .unwrap();
    assert!(updated);

    let stored = storage
        .get_connector_approval(&approval.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, ConnectorApprovalStatus::Approved);
    assert_eq!(stored.review_note.as_deref(), Some("host approved"));
    assert_eq!(stored.queued_mission_id.as_deref(), Some("telegram:ops:10"));
    assert_eq!(storage.count_pending_connector_approvals().unwrap(), 0);
}
