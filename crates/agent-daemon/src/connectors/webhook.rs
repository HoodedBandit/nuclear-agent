use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

use super::*;

const WEBHOOK_TOKEN_HEADER: &str = "x-agent-webhook-token";

pub(super) fn verify_webhook_token(
    connector: &WebhookConnectorConfig,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let Some(expected_hash) = connector.token_sha256.as_deref() else {
        return Ok(());
    };
    let provided = headers
        .get(WEBHOOK_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "missing webhook token"))?;
    if hash_webhook_token(provided) != expected_hash {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid webhook token",
        ));
    }
    Ok(())
}

pub(super) fn render_webhook_prompt(
    connector: &WebhookConnectorConfig,
    payload: &WebhookEventRequest,
) -> String {
    let payload_json = payload
        .payload
        .as_ref()
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|| "null".to_string());
    let summary = payload.summary.as_deref().unwrap_or("");
    let details = payload.details.as_deref().unwrap_or("");
    let prompt = payload.prompt.as_deref().unwrap_or("");
    connector
        .prompt_template
        .replace("{connector_name}", &connector.name)
        .replace("{summary}", summary)
        .replace("{details}", details)
        .replace("{prompt}", prompt)
        .replace("{payload_json}", &payload_json)
}

pub(super) fn hash_webhook_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
