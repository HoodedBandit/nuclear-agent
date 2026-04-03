use super::*;
use agent_core::{
    MessageRole, ProviderOutputItem, RemoteContentArtifact, RemoteContentAssessment,
    RemoteContentRisk, RemoteContentSource, RemoteContentSourceKind, TaskMode, ToolCall,
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
        SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Assistant,
            "research note".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )
        .with_provider_output_items(vec![ProviderOutputItem::RemoteContent {
            artifact: RemoteContentArtifact {
                id: "artifact-1".to_string(),
                source: RemoteContentSource {
                    kind: RemoteContentSourceKind::HostedWebSearch,
                    label: Some("Example".to_string()),
                    url: Some("https://example.com/article".to_string()),
                    host: Some("example.com".to_string()),
                },
                title: Some("Example".to_string()),
                mime_type: Some("text/plain".to_string()),
                excerpt: Some("example content".to_string()),
                content_sha256: Some("abc".to_string()),
                assessment: RemoteContentAssessment {
                    risk: RemoteContentRisk::Low,
                    blocked: false,
                    reasons: vec!["plain reference content".to_string()],
                    warnings: Vec::new(),
                },
            },
        }]),
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
    assert_eq!(persisted.len(), 5);
    assert_eq!(persisted[1].tool_calls, vec![tool_call]);
    assert_eq!(
        persisted[1].provider_payload_json.as_deref(),
        Some("{\"id\":\"resp-1\"}")
    );
    assert_eq!(persisted[2].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(persisted[2].tool_name.as_deref(), Some("read_file"));
    assert!(matches!(
        persisted[4].provider_output_items.as_slice(),
        [ProviderOutputItem::RemoteContent { artifact: remote }]
            if matches!(remote.assessment.risk, RemoteContentRisk::Low)
                && remote.source.kind == RemoteContentSourceKind::HostedWebSearch
                && remote.source.host.as_deref() == Some("example.com")
                && remote.excerpt.as_deref() == Some("example content")
    ));

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
