use agent_core::{UpdateRunRequest, UpdateStatusResponse};
use axum::{extract::State, Json};

use crate::{update, ApiError, AppState};

pub(crate) async fn update_status(
    State(state): State<AppState>,
) -> Result<Json<UpdateStatusResponse>, ApiError> {
    Ok(Json(update::resolve_update_status(&state).await?))
}

pub(crate) async fn run_update(
    State(state): State<AppState>,
    Json(payload): Json<UpdateRunRequest>,
) -> Result<Json<UpdateStatusResponse>, ApiError> {
    Ok(Json(update::trigger_update(&state, payload).await?))
}
