use super::*;

pub(super) fn row_to_pattern(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsagePattern> {
    let pattern_type_json: String = row.get(1)?;
    let last_seen_at: String = row.get(6)?;
    let created_at: String = row.get(7)?;
    Ok(UsagePattern {
        id: row.get(0)?,
        pattern_type: serde_json::from_str(&pattern_type_json).unwrap_or(PatternType::ToolSequence),
        description: row.get(2)?,
        trigger_hint: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        frequency: row.get::<_, i64>(4).unwrap_or(1) as u32,
        confidence: row.get::<_, i64>(5).unwrap_or(50) as u8,
        last_seen_at: parse_datetime(&last_seen_at).unwrap_or_else(|_| Utc::now()),
        created_at: parse_datetime(&created_at).unwrap_or_else(|_| Utc::now()),
        workspace_key: row.get(8)?,
        provider_id: row.get(9)?,
    })
}

pub(super) fn configure_connection(connection: &Connection) -> Result<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .context("failed to configure SQLite busy timeout")?;
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .context("failed to enable SQLite WAL mode")?;
    connection
        .pragma_update(None, "synchronous", "NORMAL")
        .context("failed to configure SQLite synchronous mode")?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .context("failed to enable SQLite foreign keys")?;
    Ok(())
}

pub(super) fn row_to_mission(row: &rusqlite::Row<'_>) -> rusqlite::Result<Mission> {
    let created_at: String = row.get(4)?;
    let updated_at: Option<String> = row.get(5)?;
    let phase_json: Option<String> = row.get(9)?;
    let watch_path: Option<String> = row.get(12)?;
    let watch_recursive: Option<i64> = row.get(13)?;
    let watch_fingerprint: Option<String> = row.get(14)?;
    let wake_trigger_json: Option<String> = row.get(15)?;
    let wake_at: Option<String> = row.get(16)?;
    let scheduled_for_at: Option<String> = row.get(17)?;
    let repeat_interval_seconds: Option<i64> = row.get(18)?;
    let repeat_anchor_at: Option<String> = row.get(19)?;
    let evolve: Option<i64> = row.get(23)?;
    let status_json: String = row.get(3)?;
    let created_at = parse_datetime(&created_at)?;
    Ok(Mission {
        id: row.get(0)?,
        title: row.get(1)?,
        details: row.get(2)?,
        status: serde_json::from_str(&status_json).map_err(json_decode_error)?,
        created_at,
        updated_at: updated_at
            .as_deref()
            .map(parse_datetime)
            .transpose()?
            .unwrap_or(created_at),
        alias: row.get(6)?,
        requested_model: row.get(7)?,
        session_id: row.get(8)?,
        phase: phase_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(json_decode_error)?,
        handoff_summary: row.get(10)?,
        workspace_key: row.get(11)?,
        watch_path: watch_path.map(PathBuf::from),
        watch_recursive: watch_recursive.unwrap_or_default() != 0,
        watch_fingerprint,
        wake_trigger: wake_trigger_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(json_decode_error)?,
        wake_at: wake_at.as_deref().map(parse_datetime).transpose()?,
        scheduled_for_at: scheduled_for_at
            .as_deref()
            .map(parse_datetime)
            .transpose()?,
        repeat_interval_seconds: repeat_interval_seconds.map(|value| value as u64),
        repeat_anchor_at: repeat_anchor_at
            .as_deref()
            .map(parse_datetime)
            .transpose()?,
        last_error: row.get(20)?,
        retries: row.get::<_, Option<i64>>(21)?.unwrap_or_default() as u32,
        max_retries: row.get::<_, Option<i64>>(22)?.unwrap_or(3) as u32,
        evolve: evolve.unwrap_or_default() != 0,
    })
}

pub(super) fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let kind_json: String = row.get(1)?;
    let scope_json: String = row.get(2)?;
    let created_at: String = row.get(6)?;
    let updated_at: String = row.get(7)?;
    let last_used_at: Option<String> = row.get(8)?;
    let tags_json: Option<String> = row.get(13)?;
    let review_status_json: Option<String> = row.get(17)?;
    let reviewed_at: Option<String> = row.get(19)?;
    let evidence_refs_json = optional_json_column(row, 21)?;
    let tags = tags_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(json_decode_error)?
        .unwrap_or_default();
    Ok(MemoryRecord {
        id: row.get(0)?,
        kind: serde_json::from_str(&kind_json).map_err(json_decode_error)?,
        scope: serde_json::from_str(&scope_json).map_err(json_decode_error)?,
        subject: row.get(3)?,
        content: row.get(4)?,
        confidence: row.get::<_, i64>(5)?.clamp(0, 100) as u8,
        created_at: parse_datetime(&created_at)?,
        updated_at: parse_datetime(&updated_at)?,
        last_used_at: last_used_at.as_deref().map(parse_datetime).transpose()?,
        source_session_id: row.get(9)?,
        source_message_id: row.get(10)?,
        provider_id: row.get(11)?,
        workspace_key: row.get(12)?,
        evidence_refs: parse_optional_json_column(evidence_refs_json)?,
        tags,
        identity_key: row.get(14)?,
        observation_source: row.get(15)?,
        superseded_by: row.get(16)?,
        review_status: review_status_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(json_decode_error)?
            .unwrap_or_default(),
        review_note: row.get(18)?,
        reviewed_at: reviewed_at.as_deref().map(parse_datetime).transpose()?,
        supersedes: row.get(20)?,
    })
}

pub(super) fn row_to_connector_approval(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ConnectorApprovalRecord> {
    let connector_kind_json: String = row.get(1)?;
    let status_json: String = row.get(4)?;
    let created_at: String = row.get(15)?;
    let updated_at: String = row.get(16)?;
    let reviewed_at: Option<String> = row.get(17)?;
    Ok(ConnectorApprovalRecord {
        id: row.get(0)?,
        connector_kind: serde_json::from_str(&connector_kind_json).map_err(json_decode_error)?,
        connector_id: row.get(2)?,
        connector_name: row.get(3)?,
        status: serde_json::from_str(&status_json).map_err(json_decode_error)?,
        title: row.get(5)?,
        details: row.get(6)?,
        source_key: row.get(7)?,
        source_event_id: row.get(8)?,
        external_chat_id: row.get(9)?,
        external_chat_display: row.get(10)?,
        external_user_id: row.get(11)?,
        external_user_display: row.get(12)?,
        message_preview: row.get(13)?,
        queued_mission_id: row.get(14)?,
        created_at: parse_datetime(&created_at)?,
        updated_at: parse_datetime(&updated_at)?,
        reviewed_at: reviewed_at.as_deref().map(parse_datetime).transpose()?,
        review_note: row.get(18)?,
    })
}

pub(super) fn row_to_skill_draft(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillDraft> {
    let source_message_ids_json: Option<String> = row.get(8)?;
    let status_json: String = row.get(10)?;
    let created_at: String = row.get(11)?;
    let updated_at: String = row.get(12)?;
    let last_used_at: Option<String> = row.get(13)?;
    Ok(SkillDraft {
        id: row.get(0)?,
        title: row.get(1)?,
        summary: row.get(2)?,
        instructions: row.get(3)?,
        trigger_hint: row.get(4)?,
        workspace_key: row.get(5)?,
        provider_id: row.get(6)?,
        source_session_id: row.get(7)?,
        source_message_ids: source_message_ids_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(json_decode_error)?
            .unwrap_or_default(),
        usage_count: row.get::<_, i64>(9)?.max(0) as u32,
        status: serde_json::from_str(&status_json).map_err(json_decode_error)?,
        created_at: parse_datetime(&created_at)?,
        updated_at: parse_datetime(&updated_at)?,
        last_used_at: last_used_at.as_deref().map(parse_datetime).transpose()?,
    })
}

pub(super) fn memory_review_status_rank(status: MemoryReviewStatus) -> u8 {
    match status {
        MemoryReviewStatus::Accepted => 0,
        MemoryReviewStatus::Candidate => 1,
        MemoryReviewStatus::Rejected => 2,
    }
}

pub(super) fn memory_matches_scope(
    memory: &MemoryRecord,
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
) -> bool {
    let workspace_ok = workspace_key.is_none()
        || memory.scope == MemoryScope::Global
        || memory.workspace_key.as_deref() == workspace_key;
    let provider_ok = provider_id.is_none()
        || memory.provider_id.is_none()
        || memory.provider_id.as_deref() == provider_id;
    workspace_ok && provider_ok
}

pub(super) fn effective_memory_review_statuses(
    review_statuses: &[MemoryReviewStatus],
) -> Vec<MemoryReviewStatus> {
    if review_statuses.is_empty() {
        return vec![MemoryReviewStatus::Accepted];
    }

    let mut statuses = Vec::new();
    for status in review_statuses {
        if !statuses.contains(status) {
            statuses.push(status.clone());
        }
    }
    statuses
}

pub(super) fn memory_matches_query_filters(
    memory: &MemoryRecord,
    review_statuses: &[MemoryReviewStatus],
    include_superseded: bool,
) -> bool {
    (include_superseded || memory.superseded_by.is_none())
        && effective_memory_review_statuses(review_statuses).contains(&memory.review_status)
}

pub(super) fn skill_draft_matches_scope(
    draft: &SkillDraft,
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
) -> bool {
    let workspace_ok = workspace_key.is_none()
        || draft.workspace_key.is_none()
        || draft.workspace_key.as_deref() == workspace_key;
    let provider_ok = provider_id.is_none()
        || draft.provider_id.is_none()
        || draft.provider_id.as_deref() == provider_id;
    workspace_ok && provider_ok
}

pub(super) fn summarize_preview(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut preview = trimmed.chars().take(max_chars).collect::<String>();
    preview.push_str("...");
    preview
}

pub(super) fn normalize_fts_query(query: &str) -> String {
    let mut normalized = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            normalized.push(ch);
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Build an expanded FTS query that includes stemmed variants using OR.
/// Example: "running tests quickly" â†’ "running OR run OR tests OR test OR quickly OR quick"
pub(super) fn build_expanded_fts_query(query: &str) -> String {
    let base = normalize_fts_query(query);
    if base.is_empty() {
        return base;
    }

    let mut terms: Vec<String> = Vec::new();
    for word in base.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        terms.push(lower.clone());
        let stemmed = stem_english(&lower);
        if stemmed != lower {
            terms.push(stemmed);
        }
    }

    terms.dedup();
    terms.join(" OR ")
}

/// Minimal English suffix-stripping stemmer covering common inflections.
pub(super) fn stem_english(word: &str) -> String {
    if word.len() < 4 {
        return word.to_string();
    }
    // Order matters: check longer suffixes first.
    let suffixes: &[(&str, &str)] = &[
        ("iness", "y"),
        ("ation", ""),
        ("ement", ""),
        ("ments", ""),
        ("ness", ""),
        ("ting", "t"),
        ("ling", "l"),
        ("ning", "n"),
        ("ally", ""),
        ("ying", "y"),
        ("ries", "ry"),
        ("ings", ""),
        ("ment", ""),
        ("ably", ""),
        ("ibly", ""),
        ("ious", ""),
        ("eous", ""),
        ("ful", ""),
        ("ing", ""),
        ("ies", "y"),
        ("ied", "y"),
        ("ion", ""),
        ("ers", ""),
        ("est", ""),
        ("ous", ""),
        ("ble", ""),
        ("ly", ""),
        ("ed", ""),
        ("er", ""),
        ("es", ""),
        ("'s", ""),
        ("s", ""),
    ];

    for (suffix, replacement) in suffixes {
        if let Some(stem) = word.strip_suffix(suffix) {
            if stem.len() >= 2 {
                return format!("{stem}{replacement}");
            }
        }
    }
    word.to_string()
}

/// Fuzzy LIKE-based fallback search for when FTS returns too few results.
/// Uses substring matching on subject and content columns.
#[allow(clippy::too_many_arguments)]
pub(super) fn fuzzy_memory_search(
    connection: &Connection,
    query: &str,
    workspace_key: Option<&str>,
    provider_id: Option<&str>,
    review_statuses: &[MemoryReviewStatus],
    include_superseded: bool,
    limit: usize,
    exclude_ids: &[String],
) -> Result<Vec<MemoryRecord>> {
    let base = normalize_fts_query(query);
    if base.is_empty() {
        return Ok(Vec::new());
    }

    let words: Vec<String> = base
        .split_whitespace()
        .map(|w| format!("%{}%", w.to_ascii_lowercase()))
        .collect();

    if words.is_empty() {
        return Ok(Vec::new());
    }

    // Build a query that matches any word against subject or content.
    let mut conditions = Vec::new();
    let mut param_values: Vec<String> = Vec::new();
    for word in &words {
        let idx = param_values.len();
        conditions.push(format!(
            "(lower(subject) LIKE ?{} OR lower(content) LIKE ?{})",
            idx + 1,
            idx + 1
        ));
        param_values.push(word.clone());
    }

    let where_clause = conditions.join(" OR ");
    let superseded_clause = if include_superseded {
        String::new()
    } else {
        "AND superseded_by IS NULL".to_string()
    };
    let sql = format!(
        "
        SELECT
            id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
            last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
            tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
        FROM memory_records
        WHERE ({where_clause})
          {superseded_clause}
        ORDER BY updated_at DESC
        LIMIT ?{}
        ",
        param_values.len() + 1
    );

    let mut statement = connection.prepare(&sql)?;
    let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = param_values
        .iter()
        .map(|v| Box::new(v.clone()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    all_params.push(Box::new(limit as i64));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        all_params.iter().map(|p| p.as_ref()).collect();
    let rows = statement.query_map(param_refs.as_slice(), row_to_memory)?;

    Ok(rows
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|m| memory_matches_scope(m, workspace_key, provider_id))
        .filter(|m| memory_matches_query_filters(m, review_statuses, include_superseded))
        .filter(|m| !exclude_ids.contains(&m.id))
        .take(limit)
        .collect())
}

pub(super) fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub(super) fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub(super) fn vector_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

pub(super) fn cosine_similarity(a: &[f32], b: &[f32], a_norm: f32) -> f32 {
    if a.len() != b.len() || a_norm == 0.0 {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let b_norm = vector_norm(b);
    if b_norm == 0.0 {
        return 0.0;
    }
    dot / (a_norm * b_norm)
}

/// Like `row_to_memory` but reads columns starting at a given offset.
pub(super) fn row_to_memory_at_offset(
    row: &rusqlite::Row,
    offset: usize,
) -> rusqlite::Result<MemoryRecord> {
    let kind_json: String = row.get(offset + 1)?;
    let scope_json: String = row.get(offset + 2)?;
    let created_at: String = row.get(offset + 6)?;
    let updated_at: String = row.get(offset + 7)?;
    let last_used_at: Option<String> = row.get(offset + 8)?;
    let tags_json: Option<String> = row.get(offset + 13)?;
    let review_status_json: Option<String> = row.get(offset + 17)?;
    let reviewed_at: Option<String> = row.get(offset + 19)?;
    let evidence_refs_json = optional_json_column(row, offset + 21)?;

    Ok(MemoryRecord {
        id: row.get(offset)?,
        kind: serde_json::from_str(&kind_json).map_err(json_decode_error)?,
        scope: serde_json::from_str(&scope_json).map_err(json_decode_error)?,
        subject: row.get(offset + 3)?,
        content: row.get(offset + 4)?,
        confidence: row.get::<_, i64>(offset + 5)? as u8,
        created_at: parse_datetime(&created_at)?,
        updated_at: parse_datetime(&updated_at)?,
        last_used_at: last_used_at.as_deref().map(parse_datetime).transpose()?,
        source_session_id: row.get(offset + 9)?,
        source_message_id: row.get(offset + 10)?,
        provider_id: row.get(offset + 11)?,
        workspace_key: row.get(offset + 12)?,
        evidence_refs: parse_optional_json_column(evidence_refs_json)?,
        tags: tags_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default(),
        identity_key: row.get(offset + 14)?,
        observation_source: row.get(offset + 15)?,
        superseded_by: row.get(offset + 16)?,
        review_status: review_status_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or(MemoryReviewStatus::Accepted),
        review_note: row.get(offset + 18)?,
        reviewed_at: reviewed_at.as_deref().map(parse_datetime).transpose()?,
        supersedes: row.get(offset + 20)?,
    })
}

fn optional_json_column(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get(index) {
        Ok(value) => Ok(value),
        Err(rusqlite::Error::InvalidColumnIndex(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn parse_optional_json_column<T>(value: Option<String>) -> rusqlite::Result<Vec<T>>
where
    T: for<'de> serde::Deserialize<'de>,
{
    Ok(value
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(json_decode_error)?
        .unwrap_or_default())
}

pub(super) fn rebuild_messages_fts(connection: &Connection) -> Result<()> {
    connection.execute("DELETE FROM messages_fts", [])?;
    connection.execute(
        "
        INSERT INTO messages_fts(message_id, session_id, content)
        SELECT id, session_id, content FROM messages
        ",
        [],
    )?;
    Ok(())
}

pub(super) fn rebuild_memory_fts(connection: &Connection) -> Result<()> {
    connection.execute("DELETE FROM memory_records_fts", [])?;
    connection.execute(
        "
        INSERT INTO memory_records_fts(memory_id, subject, content, tags_text)
        SELECT id, subject, content, COALESCE(tags_text, '') FROM memory_records
        ",
        [],
    )?;
    Ok(())
}

pub(super) fn parse_datetime(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

pub(super) fn json_decode_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

pub(super) fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    column_definition: &str,
) -> Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    if columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    connection
        .execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {column_definition}"),
            [],
        )
        .or_else(|error| {
            let duplicate = matches!(
                &error,
                rusqlite::Error::SqliteFailure(_, Some(message))
                    if message.contains("duplicate column name")
            );
            if duplicate {
                Ok(0)
            } else {
                Err(error)
            }
        })
        .with_context(|| format!("failed to add column '{column}' to '{table}'"))?;
    Ok(())
}

pub(super) fn write_atomic(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("config"),
        Uuid::new_v4()
    ));

    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        file.write_all(content)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to flush {}", temp_path.display()))?;
    }

    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    sync_directory(parent)?;
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    fs::rename(source, target).with_context(|| {
        format!(
            "failed to replace {} with {}",
            target.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
pub(super) fn replace_file(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    // Retry with exponential backoff for ERROR_SHARING_VIOLATION (32) and
    // ERROR_LOCK_VIOLATION (33) which can occur when antivirus or other
    // processes briefly hold file handles.
    const MAX_RETRIES: u32 = 5;
    const SHARING_VIOLATION: u32 = 32;
    const LOCK_VIOLATION: u32 = 33;
    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        let replaced = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                target_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if replaced != 0 {
            return Ok(());
        }

        let error = std::io::Error::last_os_error();
        let raw_os_error = error.raw_os_error().unwrap_or(0) as u32;

        if attempt < MAX_RETRIES
            && (raw_os_error == SHARING_VIOLATION || raw_os_error == LOCK_VIOLATION)
        {
            std::thread::sleep(std::time::Duration::from_millis(50 << attempt));
            last_error = Some(error);
            continue;
        }

        return Err(error).with_context(|| {
            format!(
                "failed to replace {} with {}",
                target.display(),
                source.display()
            )
        });
    }

    Err(last_error.unwrap()).with_context(|| {
        format!(
            "failed to replace {} with {} after {} retries",
            target.display(),
            source.display(),
            MAX_RETRIES
        )
    })
}

#[cfg(not(windows))]
pub(super) fn sync_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)
        .with_context(|| format!("failed to open directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

#[cfg(windows)]
pub(super) fn sync_directory(_: &Path) -> Result<()> {
    Ok(())
}
