use agent_core::{SessionSummary, SessionTranscript};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::{ApiError, AppState, LimitQuery};

pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    Ok(Json(
        state.storage.list_sessions(query.limit.unwrap_or(25))?,
    ))
}

pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionTranscript>, ApiError> {
    let session = state
        .storage
        .get_session(&session_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown session"))?;
    let messages = state.storage.list_session_messages(&session.id)?;
    Ok(Json(SessionTranscript { session, messages }))
}
