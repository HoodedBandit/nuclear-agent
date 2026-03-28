use serde::{Deserialize, Serialize};

use crate::{
    BatchTaskRequest, BatchTaskResponse, DaemonStatus, DashboardBootstrapResponse, LogEntry,
    RunTaskRequest, RunTaskResponse, RunTaskStreamEvent, SessionSummary, SessionTranscript,
};

pub const CONTROL_PROTOCOL_VERSION: u32 = 2;

fn default_control_subscription_limit() -> usize {
    50
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ControlSubscriptionTopic {
    Status,
    Logs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlSubscriptionRequest {
    pub topic: ControlSubscriptionTopic,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default = "default_control_subscription_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlConnectRequest {
    pub protocol_version: u32,
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(default)]
    pub subscriptions: Vec<ControlSubscriptionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlConnected {
    pub protocol_version: u32,
    #[serde(default)]
    pub subscriptions: Vec<ControlSubscriptionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlLogBatch {
    #[serde(default)]
    pub entries: Vec<LogEntry>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlTaskStreamEvent {
    pub request_id: String,
    pub event: RunTaskStreamEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlSessionRenameResult {
    pub session_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ControlRequest {
    Status,
    DashboardBootstrap,
    ListEvents {
        #[serde(default)]
        after: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    ListSessions {
        #[serde(default)]
        limit: Option<usize>,
    },
    GetSession {
        session_id: String,
    },
    RenameSession {
        session_id: String,
        title: String,
    },
    RunTask {
        request: RunTaskRequest,
    },
    RunBatch {
        request: BatchTaskRequest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ControlResponse {
    Status(Box<DaemonStatus>),
    DashboardBootstrap(Box<DashboardBootstrapResponse>),
    Events(ControlLogBatch),
    Sessions(Vec<SessionSummary>),
    Session(SessionTranscript),
    SessionRenamed(ControlSessionRenameResult),
    RunTask(RunTaskResponse),
    RunBatch(BatchTaskResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ControlEvent {
    Status(Box<DaemonStatus>),
    Logs(ControlLogBatch),
    TaskStream(Box<ControlTaskStreamEvent>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlError {
    pub message: String,
    #[serde(default)]
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlClientMessage {
    Connect {
        request: ControlConnectRequest,
    },
    Subscribe {
        subscriptions: Vec<ControlSubscriptionRequest>,
    },
    Unsubscribe {
        topics: Vec<ControlSubscriptionTopic>,
    },
    Request {
        request_id: String,
        request: ControlRequest,
    },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlServerMessage {
    Connected {
        connection: ControlConnected,
    },
    Response {
        request_id: String,
        response: Box<ControlResponse>,
    },
    Event {
        event: Box<ControlEvent>,
    },
    Error {
        #[serde(default)]
        request_id: Option<String>,
        error: ControlError,
    },
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_client_message_round_trips_with_connect_and_request_payloads() {
        let message = ControlClientMessage::Connect {
            request: ControlConnectRequest {
                protocol_version: CONTROL_PROTOCOL_VERSION,
                client_name: Some("dashboard".to_string()),
                subscriptions: vec![ControlSubscriptionRequest {
                    topic: ControlSubscriptionTopic::Logs,
                    after: Some("2026-03-22T00:00:00Z|log-1".to_string()),
                    limit: 25,
                }],
            },
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: ControlClientMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, message);

        let request = ControlClientMessage::Request {
            request_id: "req-1".to_string(),
            request: ControlRequest::ListSessions { limit: Some(10) },
        };
        let request_json = serde_json::to_string(&request).unwrap();
        let decoded_request: ControlClientMessage = serde_json::from_str(&request_json).unwrap();
        assert_eq!(decoded_request, request);
    }

    #[test]
    fn control_server_message_round_trips_task_stream_events() {
        let message = ControlServerMessage::Event {
            event: Box::new(ControlEvent::TaskStream(Box::new(ControlTaskStreamEvent {
                request_id: "req-2".to_string(),
                event: RunTaskStreamEvent::SessionStarted {
                    session_id: "session-1".to_string(),
                    alias: "main".to_string(),
                    provider_id: "openai".to_string(),
                    model: "gpt-5".to_string(),
                },
            }))),
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: ControlServerMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, message);
    }
}
