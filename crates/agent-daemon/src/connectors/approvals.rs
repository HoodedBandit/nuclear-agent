use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct ConnectorApprovalQuery {
    pub(super) limit: Option<usize>,
    pub(super) status: Option<ConnectorApprovalStatus>,
    pub(super) kind: Option<ConnectorKind>,
}

pub(crate) async fn list_connector_approvals(
    State(state): State<AppState>,
    Query(query): Query<ConnectorApprovalQuery>,
) -> Result<Json<Vec<ConnectorApprovalRecord>>, ApiError> {
    Ok(Json(state.storage.list_connector_approvals(
        query.kind,
        query.status.or(Some(ConnectorApprovalStatus::Pending)),
        query.limit.unwrap_or(50).clamp(1, 100),
    )?))
}

pub(crate) async fn approve_connector_approval(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
    Json(payload): Json<ConnectorApprovalUpdateRequest>,
) -> Result<Json<ConnectorApprovalRecord>, ApiError> {
    let approval = state
        .storage
        .get_connector_approval(&approval_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown connector approval"))?;
    if approval.status != ConnectorApprovalStatus::Pending {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "connector approval is not pending",
        ));
    }

    let queued_mission_id = match approval.connector_kind {
        ConnectorKind::Telegram => {
            let connector = {
                let mut config = state.config.write().await;
                let connector = config
                    .telegram_connectors
                    .iter_mut()
                    .find(|connector| connector.id == approval.connector_id)
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "telegram connector is missing")
                    })?;
                telegram::add_telegram_pairing_allowlist_entries(connector, &approval)?;
                let updated = connector.clone();
                state.storage.save_config(&config)?;
                updated
            };
            Some(telegram::queue_telegram_approval_mission(
                &state, &connector, &approval,
            )?)
        }
        ConnectorKind::Discord => {
            let connector = {
                let mut config = state.config.write().await;
                let connector = config
                    .discord_connectors
                    .iter_mut()
                    .find(|connector| connector.id == approval.connector_id)
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "discord connector is missing")
                    })?;
                discord::add_discord_pairing_allowlist_entries(connector, &approval)?;
                let updated = connector.clone();
                state.storage.save_config(&config)?;
                updated
            };
            Some(discord::queue_discord_approval_mission(
                &state, &connector, &approval,
            )?)
        }
        ConnectorKind::Slack => {
            let connector = {
                let mut config = state.config.write().await;
                let connector = config
                    .slack_connectors
                    .iter_mut()
                    .find(|connector| connector.id == approval.connector_id)
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "slack connector is missing")
                    })?;
                slack::add_slack_pairing_allowlist_entries(connector, &approval)?;
                let updated = connector.clone();
                state.storage.save_config(&config)?;
                updated
            };
            Some(slack::queue_slack_approval_mission(
                &state, &connector, &approval,
            )?)
        }
        ConnectorKind::Signal => {
            let connector = {
                let mut config = state.config.write().await;
                let connector = config
                    .signal_connectors
                    .iter_mut()
                    .find(|connector| connector.id == approval.connector_id)
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "signal connector is missing")
                    })?;
                signal::add_signal_pairing_allowlist_entries(connector, &approval)?;
                let updated = connector.clone();
                state.storage.save_config(&config)?;
                updated
            };
            Some(signal::queue_signal_approval_mission(
                &state, &connector, &approval,
            )?)
        }
        _ => None,
    };

    let updated = state.storage.update_connector_approval_status(
        &approval_id,
        ConnectorApprovalStatus::Approved,
        payload.note.as_deref(),
        queued_mission_id.as_deref(),
    )?;
    if !updated {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown connector approval",
        ));
    }
    let approval = state
        .storage
        .get_connector_approval(&approval_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown connector approval"))?;
    append_log(
        &state,
        "info",
        connector_log_category(approval.connector_kind),
        format!(
            "approved {} pairing for connector '{}' chat={} user={}",
            connector_display_name(approval.connector_kind),
            approval.connector_id,
            approval.external_chat_id.as_deref().unwrap_or("-"),
            approval.external_user_id.as_deref().unwrap_or("-")
        ),
    )?;
    Ok(Json(approval))
}

pub(crate) async fn reject_connector_approval(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
    Json(payload): Json<ConnectorApprovalUpdateRequest>,
) -> Result<Json<ConnectorApprovalRecord>, ApiError> {
    let approval = state
        .storage
        .get_connector_approval(&approval_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown connector approval"))?;
    if approval.status != ConnectorApprovalStatus::Pending {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "connector approval is not pending",
        ));
    }
    let updated = state.storage.update_connector_approval_status(
        &approval_id,
        ConnectorApprovalStatus::Rejected,
        payload.note.as_deref(),
        None,
    )?;
    if !updated {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "unknown connector approval",
        ));
    }
    let approval = state
        .storage
        .get_connector_approval(&approval_id)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "unknown connector approval"))?;
    append_log(
        &state,
        "warn",
        connector_log_category(approval.connector_kind),
        format!(
            "rejected {} pairing for connector '{}' chat={} user={}",
            connector_display_name(approval.connector_kind),
            approval.connector_id,
            approval.external_chat_id.as_deref().unwrap_or("-"),
            approval.external_user_id.as_deref().unwrap_or("-")
        ),
    )?;
    Ok(Json(approval))
}
