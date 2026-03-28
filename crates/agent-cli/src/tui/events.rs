use std::path::PathBuf;

use agent_core::{
    InputAttachment, LogEntry, PermissionPreset, RunTaskRequest, RunTaskResponse,
    RunTaskStreamEvent, TaskMode, ThinkingLevel,
};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use url::form_urlencoded;

use crate::DaemonClient;

pub(super) enum AppEvent {
    PromptProgress(RunTaskStreamEvent),
    PromptFinished(Result<RunTaskResponse, String>),
    DaemonEvents(Vec<LogEntry>),
}

pub(super) type AppEventSender = UnboundedSender<AppEvent>;

#[derive(Clone)]
pub(super) struct PromptTask {
    pub(super) prompt: String,
    pub(super) alias: Option<String>,
    pub(super) requested_model: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) cwd: PathBuf,
    pub(super) thinking_level: Option<ThinkingLevel>,
    pub(super) task_mode: Option<TaskMode>,
    pub(super) attachments: Vec<InputAttachment>,
    pub(super) permission_preset: Option<PermissionPreset>,
    pub(super) output_schema_json: Option<String>,
    pub(super) ephemeral: bool,
}

pub(super) fn spawn_prompt_task(client: DaemonClient, task: PromptTask, sender: AppEventSender) {
    tokio::spawn(async move {
        let mut final_response = None;
        let mut stream_error = None;
        let result = client
            .post_stream::<_, RunTaskStreamEvent, _>(
                "/v1/run/stream",
                &RunTaskRequest {
                    prompt: task.prompt,
                    alias: task.alias,
                    requested_model: task.requested_model,
                    session_id: task.session_id,
                    cwd: Some(task.cwd),
                    thinking_level: task.thinking_level,
                    task_mode: task.task_mode,
                    attachments: task.attachments,
                    permission_preset: task.permission_preset,
                    output_schema_json: task.output_schema_json,
                    ephemeral: task.ephemeral,
                },
                |event| {
                    if let RunTaskStreamEvent::Completed { response } = &event {
                        final_response = Some(response.clone());
                    }
                    if let RunTaskStreamEvent::Error { message } = &event {
                        stream_error = Some(message.clone());
                    }
                    sender
                        .send(AppEvent::PromptProgress(event))
                        .map_err(|error| anyhow!("failed to deliver tui prompt event: {error}"))?;
                    Ok(())
                },
            )
            .await;
        let result = match result {
            Ok(()) if stream_error.is_some() => Err(stream_error.unwrap_or_default()),
            Ok(()) => final_response
                .ok_or_else(|| "stream ended without a completed response".to_string()),
            Err(error) => Err(format!("{error:#}")),
        };
        let _ = sender.send(AppEvent::PromptFinished(result));
    });
}

pub(super) fn spawn_daemon_event_poller(
    client: DaemonClient,
    after: Option<DateTime<Utc>>,
    sender: AppEventSender,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut cursor = after;
        loop {
            let path = daemon_event_path(cursor.as_ref(), 25, 20);
            match client.get::<Vec<LogEntry>>(&path).await {
                Ok(events) => {
                    if let Some(last) = events.last() {
                        cursor = Some(last.created_at);
                    }
                    if !events.is_empty() && sender.send(AppEvent::DaemonEvents(events)).is_err() {
                        break;
                    }
                }
                Err(_) => sleep(Duration::from_secs(2)).await,
            }
        }
    })
}

fn daemon_event_path(cursor: Option<&DateTime<Utc>>, limit: usize, wait_seconds: u64) -> String {
    let mut path = format!("/v1/events?limit={limit}&wait_seconds={wait_seconds}");
    if let Some(cursor) = cursor {
        let encoded: String =
            form_urlencoded::byte_serialize(cursor.to_rfc3339().as_bytes()).collect();
        path.push_str("&after=");
        path.push_str(&encoded);
    }
    path
}
