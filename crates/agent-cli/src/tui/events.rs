use std::path::PathBuf;

use agent_core::{InputAttachment, LogEntry, PermissionPreset, RunTaskResponse, ThinkingLevel};
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{sleep, Duration};
use url::form_urlencoded;

use crate::{execute_prompt, DaemonClient};

pub(super) enum AppEvent {
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
    pub(super) attachments: Vec<InputAttachment>,
    pub(super) permission_preset: Option<PermissionPreset>,
    pub(super) output_schema_json: Option<String>,
    pub(super) ephemeral: bool,
}

pub(super) fn spawn_prompt_task(client: DaemonClient, task: PromptTask, sender: AppEventSender) {
    tokio::spawn(async move {
        let result = execute_prompt(
            &client,
            task.prompt,
            task.alias,
            task.requested_model,
            task.session_id,
            task.cwd,
            task.thinking_level,
            task.attachments,
            task.permission_preset,
            task.output_schema_json,
            task.ephemeral,
        )
        .await
        .map_err(|error| format!("{error:#}"));
        let _ = sender.send(AppEvent::PromptFinished(result));
    });
}

pub(super) fn spawn_daemon_event_poller(
    client: DaemonClient,
    after: Option<DateTime<Utc>>,
    sender: AppEventSender,
) {
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
    });
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
