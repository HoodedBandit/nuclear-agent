use std::sync::Arc;

use agent_core::{
    ControlClientMessage, ControlConnected, ControlError, ControlEvent, ControlLogBatch,
    ControlRequest, ControlResponse, ControlServerMessage, ControlSessionRenameResult,
    ControlSubscriptionRequest, ControlSubscriptionTopic, ControlTaskStreamEvent, DaemonStatus,
    RunTaskRequest, RunTaskStreamEvent, CONTROL_PROTOCOL_VERSION,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex, Notify};

use crate::sessions::{get_session_transcript, list_sessions_from_state, rename_session_title};
use crate::{
    append_log,
    control::{
        build_daemon_status, build_dashboard_bootstrap_response, format_event_cursor, load_events,
        next_event_cursor, parse_event_cursor, EventCursor,
    },
    execute_batch_request, execute_task_request_with_events, resolve_alias_and_provider, ApiError,
    AppState, DelegationExecutionOptions, TaskRequestInput,
};

const DEFAULT_CONTROL_LOG_LIMIT: usize = 50;
const MAX_CONTROL_LOG_LIMIT: usize = 200;
const CONTROL_SOCKET_BUFFER: usize = 64;

#[derive(Clone, Debug)]
struct ActiveLogSubscription {
    after: Option<EventCursor>,
    limit: usize,
}

#[derive(Clone, Debug, Default)]
struct ControlConnectionState {
    connected: bool,
    logs: Option<ActiveLogSubscription>,
    status: bool,
    last_status: Option<DaemonStatus>,
}

type SharedConnectionState = Arc<Mutex<ControlConnectionState>>;

pub(crate) async fn control_socket_route(
    State(state): State<AppState>,
    websocket: WebSocketUpgrade,
) -> impl IntoResponse {
    websocket
        .max_message_size(1024 * 1024)
        .max_frame_size(1024 * 1024)
        .on_upgrade(move |socket| handle_control_socket(state, socket))
}

async fn handle_control_socket(state: AppState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (outbound_tx, mut outbound_rx) =
        mpsc::channel::<ControlServerMessage>(CONTROL_SOCKET_BUFFER);
    let connection = Arc::new(Mutex::new(ControlConnectionState::default()));
    let shutdown = Arc::new(Notify::new());

    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            let Ok(json) = serde_json::to_string(&message) else {
                continue;
            };
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    let watcher = tokio::spawn(watch_control_subscriptions(
        state.clone(),
        connection.clone(),
        outbound_tx.clone(),
        shutdown.clone(),
    ));

    while let Some(message) = receiver.next().await {
        let message = match message {
            Ok(message) => message,
            Err(_) => break,
        };

        match message {
            Message::Text(text) => {
                let parsed = match serde_json::from_str::<ControlClientMessage>(&text) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let _ = send_protocol_error(
                            &outbound_tx,
                            None,
                            StatusCode::BAD_REQUEST,
                            format!("invalid control message JSON: {error}"),
                        )
                        .await;
                        break;
                    }
                };
                if !handle_control_message(&state, &connection, &outbound_tx, parsed).await {
                    break;
                }
            }
            Message::Binary(_) => {
                let _ = send_protocol_error(
                    &outbound_tx,
                    None,
                    StatusCode::BAD_REQUEST,
                    "binary control messages are not supported",
                )
                .await;
                break;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) => {}
        }
    }

    shutdown.notify_waiters();
    drop(outbound_tx);
    let _ = watcher.await;
    let _ = writer.await;
}

async fn handle_control_message(
    state: &AppState,
    connection: &SharedConnectionState,
    outbound: &mpsc::Sender<ControlServerMessage>,
    message: ControlClientMessage,
) -> bool {
    let connected = { connection.lock().await.connected };

    match message {
        ControlClientMessage::Connect { request } => {
            if connected {
                let _ = send_protocol_error(
                    outbound,
                    None,
                    StatusCode::CONFLICT,
                    "control connection is already established",
                )
                .await;
                return true;
            }
            if let Err(error) = validate_connect_request(&request) {
                let _ = send_protocol_error(outbound, None, error.status, error.message).await;
                return false;
            }
            let applied = match normalize_subscriptions(&request.subscriptions) {
                Ok(applied) => applied,
                Err(error) => {
                    let _ = send_protocol_error(outbound, None, error.status, error.message).await;
                    return false;
                }
            };

            let store_result = {
                let mut guard = connection.lock().await;
                guard.connected = true;
                set_connection_subscriptions(&mut guard, &applied)
            };
            if let Err(error) = store_result {
                let _ = send_protocol_error(outbound, None, error.status, error.message).await;
                return false;
            }

            let connected = send_protocol_message(
                outbound,
                ControlServerMessage::Connected {
                    connection: ControlConnected {
                        protocol_version: CONTROL_PROTOCOL_VERSION,
                        subscriptions: applied.clone(),
                    },
                },
            )
            .await;
            if !connected {
                return false;
            }

            match emit_initial_subscription_snapshots(state, connection, outbound, &applied).await {
                Ok(_) => true,
                Err(error) => {
                    let _ = send_protocol_error(outbound, None, error.status, error.message).await;
                    false
                }
            }
        }
        ControlClientMessage::Ping => {
            if !connected {
                let _ = send_protocol_error(
                    outbound,
                    None,
                    StatusCode::BAD_REQUEST,
                    "first control message must be connect",
                )
                .await;
                return false;
            }
            send_protocol_message(outbound, ControlServerMessage::Pong).await
        }
        ControlClientMessage::Subscribe { subscriptions } => {
            if !connected {
                let _ = send_protocol_error(
                    outbound,
                    None,
                    StatusCode::BAD_REQUEST,
                    "first control message must be connect",
                )
                .await;
                return false;
            }
            match apply_subscriptions(state, connection, outbound, &subscriptions).await {
                Ok(_) => true,
                Err(error) => {
                    let _ = send_protocol_error(outbound, None, error.status, error.message).await;
                    false
                }
            }
        }
        ControlClientMessage::Unsubscribe { topics } => {
            if !connected {
                let _ = send_protocol_error(
                    outbound,
                    None,
                    StatusCode::BAD_REQUEST,
                    "first control message must be connect",
                )
                .await;
                return false;
            }
            unsubscribe_topics(connection, &topics).await;
            true
        }
        ControlClientMessage::Request {
            request_id,
            request,
        } => {
            if !connected {
                let _ = send_protocol_error(
                    outbound,
                    None,
                    StatusCode::BAD_REQUEST,
                    "first control message must be connect",
                )
                .await;
                return false;
            }
            let state = state.clone();
            let outbound = outbound.clone();
            tokio::spawn(async move {
                handle_control_request(state, outbound, request_id, request).await;
            });
            true
        }
    }
}

fn validate_connect_request(request: &agent_core::ControlConnectRequest) -> Result<(), ApiError> {
    if request.protocol_version != CONTROL_PROTOCOL_VERSION {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            format!(
                "unsupported control protocol version {} (expected {})",
                request.protocol_version, CONTROL_PROTOCOL_VERSION
            ),
        ));
    }
    Ok(())
}

async fn apply_subscriptions(
    state: &AppState,
    connection: &SharedConnectionState,
    outbound: &mpsc::Sender<ControlServerMessage>,
    subscriptions: &[ControlSubscriptionRequest],
) -> Result<Vec<ControlSubscriptionRequest>, ApiError> {
    let normalized = normalize_subscriptions(subscriptions)?;
    {
        let mut guard = connection.lock().await;
        set_connection_subscriptions(&mut guard, &normalized)?;
    }

    emit_initial_subscription_snapshots(state, connection, outbound, &normalized).await?;
    Ok(normalized)
}

fn set_connection_subscriptions(
    guard: &mut ControlConnectionState,
    subscriptions: &[ControlSubscriptionRequest],
) -> Result<(), ApiError> {
    for subscription in subscriptions {
        match subscription.topic {
            ControlSubscriptionTopic::Logs => {
                let after = subscription
                    .after
                    .as_deref()
                    .map(parse_event_cursor)
                    .transpose()?;
                guard.logs = Some(ActiveLogSubscription {
                    after,
                    limit: subscription.limit,
                });
            }
            ControlSubscriptionTopic::Status => {
                guard.status = true;
            }
        }
    }
    Ok(())
}

fn normalize_subscriptions(
    subscriptions: &[ControlSubscriptionRequest],
) -> Result<Vec<ControlSubscriptionRequest>, ApiError> {
    let mut normalized = Vec::with_capacity(subscriptions.len());
    for subscription in subscriptions {
        let mut normalized_subscription = subscription.clone();
        normalized_subscription.limit = normalize_log_limit(subscription.limit);
        if matches!(
            normalized_subscription.topic,
            ControlSubscriptionTopic::Logs
        ) {
            if let Some(after) = normalized_subscription.after.as_deref() {
                parse_event_cursor(after)?;
            }
        } else {
            normalized_subscription.after = None;
        }
        normalized.push(normalized_subscription);
    }
    Ok(normalized)
}

async fn emit_initial_subscription_snapshots(
    state: &AppState,
    connection: &SharedConnectionState,
    outbound: &mpsc::Sender<ControlServerMessage>,
    subscriptions: &[ControlSubscriptionRequest],
) -> Result<(), ApiError> {
    for subscription in subscriptions {
        match subscription.topic {
            ControlSubscriptionTopic::Logs => {
                let after = subscription
                    .after
                    .as_deref()
                    .map(parse_event_cursor)
                    .transpose()?;
                let batch = load_log_batch(state, after.clone(), subscription.limit)?;
                if let Some(next_cursor) = batch.next_cursor.as_deref() {
                    let mut guard = connection.lock().await;
                    guard.logs = Some(ActiveLogSubscription {
                        after: Some(parse_event_cursor(next_cursor)?),
                        limit: subscription.limit,
                    });
                }
                if !batch.entries.is_empty()
                    && !send_protocol_message(
                        outbound,
                        ControlServerMessage::Event {
                            event: Box::new(ControlEvent::Logs(batch)),
                        },
                    )
                    .await
                {
                    return Ok(());
                }
            }
            ControlSubscriptionTopic::Status => {
                let status = current_daemon_status(state).await?;
                {
                    let mut guard = connection.lock().await;
                    guard.last_status = Some(status.clone());
                }
                if !send_protocol_message(
                    outbound,
                    ControlServerMessage::Event {
                        event: Box::new(ControlEvent::Status(Box::new(status))),
                    },
                )
                .await
                {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

async fn unsubscribe_topics(
    connection: &SharedConnectionState,
    topics: &[ControlSubscriptionTopic],
) {
    let mut guard = connection.lock().await;
    for topic in topics {
        match topic {
            ControlSubscriptionTopic::Logs => guard.logs = None,
            ControlSubscriptionTopic::Status => {
                guard.status = false;
                guard.last_status = None;
            }
        }
    }
}

async fn watch_control_subscriptions(
    state: AppState,
    connection: SharedConnectionState,
    outbound: mpsc::Sender<ControlServerMessage>,
    shutdown: Arc<Notify>,
) {
    loop {
        tokio::select! {
            _ = shutdown.notified() => break,
            _ = state.log_wake.notified() => {}
        }

        let (log_subscription, status_subscription) = {
            let guard = connection.lock().await;
            (guard.logs.clone(), guard.status)
        };

        if let Some(log_subscription) = log_subscription {
            match load_log_batch(
                &state,
                log_subscription.after.clone(),
                log_subscription.limit,
            ) {
                Ok(batch) => {
                    if let Some(next_cursor) = batch.next_cursor.as_deref() {
                        let mut guard = connection.lock().await;
                        if let Some(active) = guard.logs.as_mut() {
                            active.after = parse_event_cursor(next_cursor).ok();
                        }
                    }
                    if !batch.entries.is_empty()
                        && !send_protocol_message(
                            &outbound,
                            ControlServerMessage::Event {
                                event: Box::new(ControlEvent::Logs(batch)),
                            },
                        )
                        .await
                    {
                        break;
                    }
                }
                Err(error) => {
                    if !send_protocol_error(&outbound, None, error.status, error.message).await {
                        break;
                    }
                }
            }
        }

        if status_subscription {
            match current_daemon_status(&state).await {
                Ok(status) => {
                    let should_send = {
                        let mut guard = connection.lock().await;
                        if guard.last_status.as_ref() == Some(&status) {
                            false
                        } else {
                            guard.last_status = Some(status.clone());
                            true
                        }
                    };
                    if should_send
                        && !send_protocol_message(
                            &outbound,
                            ControlServerMessage::Event {
                                event: Box::new(ControlEvent::Status(Box::new(status))),
                            },
                        )
                        .await
                    {
                        break;
                    }
                }
                Err(error) => {
                    if !send_protocol_error(&outbound, None, error.status, error.message).await {
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_control_request(
    state: AppState,
    outbound: mpsc::Sender<ControlServerMessage>,
    request_id: String,
    request: ControlRequest,
) {
    let response = match execute_control_request(&state, &outbound, &request_id, request).await {
        Ok(response) => response,
        Err(error) => {
            let _ =
                send_protocol_error(&outbound, Some(request_id), error.status, error.message).await;
            return;
        }
    };

    let _ = send_protocol_message(
        &outbound,
        ControlServerMessage::Response {
            request_id,
            response: Box::new(response),
        },
    )
    .await;
}

async fn execute_control_request(
    state: &AppState,
    outbound: &mpsc::Sender<ControlServerMessage>,
    request_id: &str,
    request: ControlRequest,
) -> Result<ControlResponse, ApiError> {
    match request {
        ControlRequest::Status => Ok(ControlResponse::Status(Box::new(
            current_daemon_status(state).await?,
        ))),
        ControlRequest::DashboardBootstrap => {
            let config = state.config.read().await.clone();
            Ok(ControlResponse::DashboardBootstrap(Box::new(
                build_dashboard_bootstrap_response(state, &config)?,
            )))
        }
        ControlRequest::ListEvents { after, limit } => {
            let after = after.as_deref().map(parse_event_cursor).transpose()?;
            let batch = load_log_batch(state, after, limit.unwrap_or(DEFAULT_CONTROL_LOG_LIMIT))?;
            Ok(ControlResponse::Events(batch))
        }
        ControlRequest::ListSessions { limit } => Ok(ControlResponse::Sessions(
            list_sessions_from_state(state, limit.unwrap_or(25).clamp(1, 100))?,
        )),
        ControlRequest::GetSession { session_id } => Ok(ControlResponse::Session(
            get_session_transcript(state, &session_id)?,
        )),
        ControlRequest::RenameSession { session_id, title } => {
            rename_session_title(state, &session_id, &title)?;
            Ok(ControlResponse::SessionRenamed(
                ControlSessionRenameResult {
                    session_id,
                    title: title.trim().to_string(),
                },
            ))
        }
        ControlRequest::RunTask { request } => Ok(ControlResponse::RunTask(
            execute_run_task_request(state, outbound, request_id, request).await?,
        )),
        ControlRequest::RunBatch { request } => {
            let response = execute_batch_request(
                state,
                request,
                DelegationExecutionOptions {
                    background: false,
                    permission_preset: None,
                    delegation_depth: 0,
                },
            )
            .await?;
            append_log(
                state,
                "info",
                "batch",
                format!("executed {} subagent tasks", response.results.len()),
            )?;
            Ok(ControlResponse::RunBatch(response))
        }
    }
}

async fn execute_run_task_request(
    state: &AppState,
    outbound: &mpsc::Sender<ControlServerMessage>,
    request_id: &str,
    request: RunTaskRequest,
) -> Result<agent_core::RunTaskResponse, ApiError> {
    let (alias, provider) = resolve_alias_and_provider(state, request.alias.as_deref()).await?;
    let request_id = request_id.to_string();
    let outbound = outbound.clone();

    let mut emit = move |event: RunTaskStreamEvent| {
        let outbound = outbound.clone();
        let request_id = request_id.clone();
        async move {
            send_protocol_message(
                &outbound,
                ControlServerMessage::Event {
                    event: Box::new(ControlEvent::TaskStream(Box::new(ControlTaskStreamEvent {
                        request_id,
                        event,
                    }))),
                },
            )
            .await
        }
    };

    execute_task_request_with_events(
        state,
        &alias,
        &provider,
        TaskRequestInput {
            prompt: request.prompt,
            requested_model: request.requested_model,
            session_id: request.session_id,
            cwd: request.cwd,
            thinking_level: request.thinking_level,
            attachments: request.attachments,
            permission_preset: request.permission_preset,
            task_mode: request.task_mode,
            output_schema_json: request.output_schema_json,
            persist: !request.ephemeral,
            background: false,
            delegation_depth: 0,
            cancellation: None,
        },
        &mut emit,
    )
    .await
}

async fn current_daemon_status(state: &AppState) -> Result<DaemonStatus, ApiError> {
    let config = state.config.read().await.clone();
    build_daemon_status(state, &config)
}

fn load_log_batch(
    state: &AppState,
    after: Option<EventCursor>,
    limit: usize,
) -> Result<ControlLogBatch, ApiError> {
    let limit = normalize_log_limit(limit);
    let entries = load_events(state, after.clone(), limit)?;
    let next_cursor =
        next_event_cursor(&entries).or_else(|| after.as_ref().map(format_event_cursor));
    Ok(ControlLogBatch {
        entries,
        next_cursor,
    })
}

fn normalize_log_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_CONTROL_LOG_LIMIT)
}

async fn send_protocol_message(
    outbound: &mpsc::Sender<ControlServerMessage>,
    message: ControlServerMessage,
) -> bool {
    outbound.send(message).await.is_ok()
}

async fn send_protocol_error(
    outbound: &mpsc::Sender<ControlServerMessage>,
    request_id: Option<String>,
    status: StatusCode,
    message: impl Into<String>,
) -> bool {
    send_protocol_message(
        outbound,
        ControlServerMessage::Error {
            request_id,
            error: ControlError {
                message: message.into(),
                status_code: Some(status.as_u16()),
            },
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        new_browser_auth_store, new_dashboard_launch_store, new_dashboard_session_store,
        new_mission_cancellation_store, AppState, ProviderRateLimiter,
    };
    use agent_core::{AppConfig, LogEntry};
    use agent_storage::Storage;
    use chrono::Utc;
    use std::sync::{atomic::AtomicBool, Arc};
    use tokio::sync::RwLock;
    use uuid::Uuid;

    fn test_state() -> AppState {
        AppState {
            storage: Storage::open_at(std::env::temp_dir().join(format!(
                "agent-daemon-control-socket-test-{}",
                Uuid::new_v4()
            )))
            .unwrap(),
            config: Arc::new(RwLock::new(AppConfig::default())),
            http_client: reqwest::Client::new(),
            browser_auth_sessions: new_browser_auth_store(),
            dashboard_sessions: new_dashboard_session_store(),
            dashboard_launches: new_dashboard_launch_store(),
            mission_cancellations: new_mission_cancellation_store(),
            started_at: Utc::now(),
            shutdown: tokio::sync::mpsc::unbounded_channel().0,
            autopilot_wake: Arc::new(Notify::new()),
            log_wake: Arc::new(Notify::new()),
            restart_requested: Arc::new(AtomicBool::new(false)),
            rate_limiter: ProviderRateLimiter::new(),
        }
    }

    #[test]
    fn validate_connect_request_rejects_unknown_protocol_version() {
        let error = validate_connect_request(&agent_core::ControlConnectRequest {
            protocol_version: CONTROL_PROTOCOL_VERSION + 1,
            client_name: None,
            subscriptions: Vec::new(),
        })
        .unwrap_err();

        assert_eq!(error.status, StatusCode::CONFLICT);
        assert!(error
            .message
            .contains("unsupported control protocol version"));
    }

    #[test]
    fn load_log_batch_preserves_next_cursor_when_no_new_entries_exist() {
        let state = test_state();
        let cursor = EventCursor {
            created_at: Utc::now(),
            id: Some("log-1".to_string()),
        };
        let expected_cursor = format_event_cursor(&cursor);

        let batch = load_log_batch(&state, Some(cursor.clone()), 25).unwrap();
        assert!(batch.entries.is_empty());
        assert_eq!(batch.next_cursor.as_deref(), Some(expected_cursor.as_str()));
    }

    #[tokio::test]
    async fn apply_subscriptions_emits_backlog_and_advances_cursor() {
        let state = test_state();
        let first = LogEntry {
            id: "log-a".to_string(),
            level: "info".to_string(),
            scope: "tests".to_string(),
            message: "first".to_string(),
            created_at: Utc::now(),
        };
        let second = LogEntry {
            id: "log-b".to_string(),
            level: "info".to_string(),
            scope: "tests".to_string(),
            message: "second".to_string(),
            created_at: first.created_at + chrono::Duration::seconds(1),
        };
        state.storage.append_log(&first).unwrap();
        state.storage.append_log(&second).unwrap();

        let connection = Arc::new(Mutex::new(ControlConnectionState::default()));
        let (tx, mut rx) = mpsc::channel(8);
        let subscriptions = vec![ControlSubscriptionRequest {
            topic: ControlSubscriptionTopic::Logs,
            after: Some(format!("{}|{}", first.created_at.to_rfc3339(), first.id)),
            limit: 25,
        }];

        let applied = apply_subscriptions(&state, &connection, &tx, &subscriptions)
            .await
            .unwrap();
        assert_eq!(applied, subscriptions);

        let message = rx.recv().await.unwrap();
        let expected_cursor = format!("{}|{}", second.created_at.to_rfc3339(), second.id);
        match message {
            ControlServerMessage::Event { event } => match *event {
                ControlEvent::Logs(batch) => {
                    let ids = batch
                        .entries
                        .into_iter()
                        .map(|entry| entry.id)
                        .collect::<Vec<_>>();
                    assert_eq!(ids, vec!["log-b".to_string()]);
                    assert_eq!(batch.next_cursor.as_deref(), Some(expected_cursor.as_str()));
                }
                other => panic!("unexpected control event: {other:?}"),
            },
            other => panic!("unexpected control message: {other:?}"),
        }

        let guard = connection.lock().await;
        let active = guard.logs.as_ref().expect("log subscription");
        assert_eq!(
            active.after.as_ref().map(format_event_cursor).as_deref(),
            Some(expected_cursor.as_str())
        );
    }
}
