use std::{fs, path::Path as FsPath};

use super::*;

pub(super) fn process_inbox_connector(
    state: &AppState,
    connector: &InboxConnectorConfig,
) -> Result<(usize, usize), ApiError> {
    if !connector.enabled {
        return Ok((0, 0));
    }
    let inbox_path = resolve_request_cwd(Some(connector.path.clone()))?;
    if !inbox_path.exists() {
        fs::create_dir_all(&inbox_path).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "failed to create inbox directory '{}': {error}",
                    inbox_path.display()
                ),
            )
        })?;
    }
    if !inbox_path.is_dir() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("inbox path '{}' is not a directory", inbox_path.display()),
        ));
    }

    let mut entries = fs::read_dir(&inbox_path)
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "failed to read inbox directory '{}': {error}",
                    inbox_path.display()
                ),
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| inbox_entry_candidate(path.as_path()))
        .collect::<Vec<_>>();
    entries.sort();

    let mut processed = 0usize;
    let mut queued = 0usize;
    for entry in entries {
        let Some((title, details)) = read_inbox_entry(connector, &entry)? else {
            continue;
        };
        let mut mission = Mission::new(title.clone(), details);
        mission.alias = connector.alias.clone();
        mission.requested_model = connector.requested_model.clone();
        mission.status = MissionStatus::Queued;
        mission.wake_trigger = Some(WakeTrigger::Inbox);
        if let Some(cwd) = connector.cwd.clone() {
            mission.workspace_key = Some(resolve_request_cwd(Some(cwd))?.display().to_string());
        }
        state.storage.upsert_mission(&mission)?;
        archive_inbox_entry(connector, &entry, &inbox_path)?;
        append_log(
            state,
            "info",
            "inboxes",
            format!(
                "inbox '{}' queued mission '{}' from {}",
                connector.id,
                mission.title,
                entry.display()
            ),
        )?;
        processed += 1;
        queued += 1;
    }
    if queued > 0 {
        state.autopilot_wake.notify_waiters();
    }
    Ok((processed, queued))
}

fn inbox_entry_candidate(path: &FsPath) -> bool {
    path.is_file()
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !name.starts_with('.') && !name.ends_with(".processed"))
}

pub(super) fn read_inbox_entry(
    connector: &InboxConnectorConfig,
    path: &FsPath,
) -> Result<Option<(String, String)>, ApiError> {
    let content = fs::read_to_string(path).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("failed to read inbox file '{}': {error}", path.display()),
        )
    })?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        if let Ok(payload) = serde_json::from_str::<WebhookEventRequest>(trimmed) {
            let title = payload
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|summary| !summary.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{} inbox event", connector.name));
            let details = render_inbox_prompt(connector, &payload);
            if !details.trim().is_empty() {
                return Ok(Some((title, details)));
            }
        }
    }

    let title = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("{} inbox event", connector.name));
    Ok(Some((title, trimmed.to_string())))
}

fn render_inbox_prompt(connector: &InboxConnectorConfig, payload: &WebhookEventRequest) -> String {
    let payload_json = payload
        .payload
        .as_ref()
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
        .unwrap_or_else(|| "null".to_string());
    let summary = payload.summary.as_deref().unwrap_or("");
    let details = payload.details.as_deref().unwrap_or("");
    let prompt = payload.prompt.as_deref().unwrap_or("");
    format!(
        "Inbox connector: {}\nSummary: {}\nDetails: {}\nPrompt: {}\nPayload:\n{}",
        connector.name, summary, details, prompt, payload_json
    )
}

pub(super) fn archive_inbox_entry(
    connector: &InboxConnectorConfig,
    entry: &FsPath,
    inbox_path: &FsPath,
) -> Result<(), ApiError> {
    if connector.delete_after_read {
        fs::remove_file(entry).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("failed to delete inbox file '{}': {error}", entry.display()),
            )
        })?;
        return Ok(());
    }

    let processed_dir = inbox_path.join(".processed");
    fs::create_dir_all(&processed_dir).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to create processed inbox directory '{}': {error}",
                processed_dir.display()
            ),
        )
    })?;
    let archived_name = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S"),
        entry
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("event.txt")
    );
    let archived_path = processed_dir.join(archived_name);
    fs::rename(entry, &archived_path).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!(
                "failed to archive inbox file '{}' to '{}': {error}",
                entry.display(),
                archived_path.display()
            ),
        )
    })?;
    Ok(())
}
