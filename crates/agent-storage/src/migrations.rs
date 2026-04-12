use super::*;
use rusqlite::Connection;

const LATEST_SCHEMA_VERSION: i32 = 5;

pub(super) fn apply_schema_migrations(connection: &Connection) -> Result<()> {
    let current_version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("failed to read SQLite schema version")?;

    for next_version in (current_version + 1)..=LATEST_SCHEMA_VERSION {
        match next_version {
            1 => apply_core_schema(connection)?,
            2 => apply_session_and_message_migration(connection)?,
            3 => apply_mission_memory_and_skill_migration(connection)?,
            4 => apply_connector_approval_migration(connection)?,
            5 => apply_search_and_pattern_migration(connection)?,
            _ => unreachable!("unsupported schema migration version"),
        }
        connection
            .pragma_update(None, "user_version", next_version)
            .with_context(|| format!("failed to update SQLite schema version to {next_version}"))?;
    }

    rebuild_messages_fts(connection)?;
    rebuild_memory_fts(connection)?;
    Ok(())
}

fn apply_core_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT,
            alias TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            model TEXT NOT NULL,
            task_mode TEXT,
            cwd TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role_json TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            provider_id TEXT,
            model TEXT,
            attachments_json TEXT
        );
        CREATE TABLE IF NOT EXISTS missions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            details TEXT NOT NULL,
            status_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT,
            alias TEXT,
            requested_model TEXT,
            session_id TEXT,
            phase_json TEXT,
            handoff_summary TEXT,
            workspace_key TEXT,
            watch_path TEXT,
            watch_recursive INTEGER,
            watch_fingerprint TEXT,
            wake_trigger_json TEXT,
            wake_at TEXT,
            scheduled_for_at TEXT,
            repeat_interval_seconds INTEGER,
            repeat_anchor_at TEXT,
            last_error TEXT,
            retries INTEGER,
            max_retries INTEGER,
            evolve INTEGER
        );
        CREATE TABLE IF NOT EXISTS mission_checkpoints (
            id TEXT PRIMARY KEY,
            mission_id TEXT NOT NULL,
            status_json TEXT NOT NULL,
            summary TEXT NOT NULL,
            created_at TEXT NOT NULL,
            session_id TEXT,
            phase_json TEXT,
            handoff_summary TEXT,
            response_excerpt TEXT,
            next_wake_at TEXT,
            scheduled_for_at TEXT
        );
        CREATE TABLE IF NOT EXISTS memory_records (
            id TEXT PRIMARY KEY,
            kind_json TEXT NOT NULL,
            scope_json TEXT NOT NULL,
            subject TEXT NOT NULL,
            content TEXT NOT NULL,
            confidence INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_used_at TEXT,
            source_session_id TEXT,
            source_message_id TEXT,
            provider_id TEXT,
            workspace_key TEXT,
            tags_json TEXT,
            tags_text TEXT,
            identity_key TEXT,
            observation_source TEXT,
            superseded_by TEXT,
            review_status_json TEXT,
            review_note TEXT,
            reviewed_at TEXT,
            supersedes TEXT,
            evidence_refs_json TEXT
        );
        CREATE TABLE IF NOT EXISTS skill_drafts (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            summary TEXT NOT NULL,
            instructions TEXT NOT NULL,
            trigger_hint TEXT,
            workspace_key TEXT,
            provider_id TEXT,
            source_session_id TEXT,
            source_message_ids_json TEXT,
            usage_count INTEGER NOT NULL,
            status_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_used_at TEXT
        );
        CREATE TABLE IF NOT EXISTS logs (
            id TEXT PRIMARY KEY,
            level TEXT NOT NULL,
            scope TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS connector_approvals (
            id TEXT PRIMARY KEY,
            connector_kind_json TEXT NOT NULL,
            connector_id TEXT NOT NULL,
            connector_name TEXT NOT NULL,
            status_json TEXT NOT NULL,
            title TEXT NOT NULL,
            details TEXT NOT NULL,
            source_key TEXT NOT NULL,
            source_event_id TEXT,
            external_chat_id TEXT,
            external_chat_display TEXT,
            external_user_id TEXT,
            external_user_display TEXT,
            message_preview TEXT,
            queued_mission_id TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            reviewed_at TEXT,
            review_note TEXT
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            message_id UNINDEXED,
            session_id UNINDEXED,
            content
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_records_fts USING fts5(
            memory_id UNINDEXED,
            subject,
            content,
            tags_text
        );
        ",
    )?;
    Ok(())
}

fn apply_session_and_message_migration(connection: &Connection) -> Result<()> {
    ensure_column(connection, "sessions", "title", "TEXT")?;
    ensure_column(connection, "sessions", "task_mode", "TEXT")?;
    ensure_column(connection, "sessions", "cwd", "TEXT")?;
    ensure_column(connection, "messages", "attachments_json", "TEXT")?;
    ensure_column(connection, "messages", "tool_call_id", "TEXT")?;
    ensure_column(connection, "messages", "tool_name", "TEXT")?;
    ensure_column(connection, "messages", "tool_calls_json", "TEXT")?;
    ensure_column(connection, "messages", "provider_payload_json", "TEXT")?;
    ensure_column(connection, "messages", "provider_output_items_json", "TEXT")?;
    Ok(())
}

fn apply_mission_memory_and_skill_migration(connection: &Connection) -> Result<()> {
    ensure_column(connection, "missions", "updated_at", "TEXT")?;
    ensure_column(connection, "missions", "alias", "TEXT")?;
    ensure_column(connection, "missions", "requested_model", "TEXT")?;
    ensure_column(connection, "missions", "session_id", "TEXT")?;
    ensure_column(connection, "missions", "phase_json", "TEXT")?;
    ensure_column(connection, "missions", "handoff_summary", "TEXT")?;
    ensure_column(connection, "missions", "workspace_key", "TEXT")?;
    ensure_column(connection, "missions", "watch_path", "TEXT")?;
    ensure_column(connection, "missions", "watch_recursive", "INTEGER")?;
    ensure_column(connection, "missions", "watch_fingerprint", "TEXT")?;
    ensure_column(connection, "missions", "wake_trigger_json", "TEXT")?;
    ensure_column(connection, "missions", "wake_at", "TEXT")?;
    ensure_column(connection, "missions", "scheduled_for_at", "TEXT")?;
    ensure_column(connection, "missions", "repeat_interval_seconds", "INTEGER")?;
    ensure_column(connection, "missions", "repeat_anchor_at", "TEXT")?;
    ensure_column(connection, "missions", "last_error", "TEXT")?;
    ensure_column(connection, "missions", "retries", "INTEGER")?;
    ensure_column(connection, "missions", "max_retries", "INTEGER")?;
    ensure_column(connection, "missions", "evolve", "INTEGER")?;
    ensure_column(connection, "mission_checkpoints", "phase_json", "TEXT")?;
    ensure_column(connection, "mission_checkpoints", "handoff_summary", "TEXT")?;
    ensure_column(connection, "mission_checkpoints", "next_wake_at", "TEXT")?;
    ensure_column(
        connection,
        "mission_checkpoints",
        "scheduled_for_at",
        "TEXT",
    )?;
    ensure_column(connection, "memory_records", "last_used_at", "TEXT")?;
    ensure_column(connection, "memory_records", "source_session_id", "TEXT")?;
    ensure_column(connection, "memory_records", "source_message_id", "TEXT")?;
    ensure_column(connection, "memory_records", "provider_id", "TEXT")?;
    ensure_column(connection, "memory_records", "workspace_key", "TEXT")?;
    ensure_column(connection, "memory_records", "tags_json", "TEXT")?;
    ensure_column(connection, "memory_records", "tags_text", "TEXT")?;
    ensure_column(connection, "memory_records", "identity_key", "TEXT")?;
    ensure_column(connection, "memory_records", "observation_source", "TEXT")?;
    ensure_column(connection, "memory_records", "superseded_by", "TEXT")?;
    ensure_column(connection, "memory_records", "review_status_json", "TEXT")?;
    ensure_column(connection, "memory_records", "review_note", "TEXT")?;
    ensure_column(connection, "memory_records", "reviewed_at", "TEXT")?;
    ensure_column(connection, "memory_records", "supersedes", "TEXT")?;
    ensure_column(connection, "memory_records", "evidence_refs_json", "TEXT")?;
    ensure_column(connection, "skill_drafts", "trigger_hint", "TEXT")?;
    ensure_column(connection, "skill_drafts", "workspace_key", "TEXT")?;
    ensure_column(connection, "skill_drafts", "provider_id", "TEXT")?;
    ensure_column(connection, "skill_drafts", "source_session_id", "TEXT")?;
    ensure_column(
        connection,
        "skill_drafts",
        "source_message_ids_json",
        "TEXT",
    )?;
    ensure_column(connection, "skill_drafts", "usage_count", "INTEGER")?;
    ensure_column(connection, "skill_drafts", "status_json", "TEXT")?;
    ensure_column(connection, "skill_drafts", "updated_at", "TEXT")?;
    ensure_column(connection, "skill_drafts", "last_used_at", "TEXT")?;
    Ok(())
}

fn apply_connector_approval_migration(connection: &Connection) -> Result<()> {
    ensure_column(connection, "connector_approvals", "source_event_id", "TEXT")?;
    ensure_column(
        connection,
        "connector_approvals",
        "external_chat_id",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "connector_approvals",
        "external_chat_display",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "connector_approvals",
        "external_user_id",
        "TEXT",
    )?;
    ensure_column(
        connection,
        "connector_approvals",
        "external_user_display",
        "TEXT",
    )?;
    ensure_column(connection, "connector_approvals", "message_preview", "TEXT")?;
    ensure_column(
        connection,
        "connector_approvals",
        "queued_mission_id",
        "TEXT",
    )?;
    ensure_column(connection, "connector_approvals", "reviewed_at", "TEXT")?;
    ensure_column(connection, "connector_approvals", "review_note", "TEXT")?;
    connection.execute_batch(
        "
        CREATE UNIQUE INDEX IF NOT EXISTS idx_connector_approvals_source_key
        ON connector_approvals(source_key);
        CREATE INDEX IF NOT EXISTS idx_connector_approvals_status
        ON connector_approvals(status_json, updated_at DESC);
        ",
    )?;
    Ok(())
}

fn apply_search_and_pattern_migration(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS memory_embeddings (
            memory_id TEXT PRIMARY KEY,
            embedding BLOB NOT NULL,
            model TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(message_id, session_id, content)
            VALUES (new.id, new.session_id, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
            DELETE FROM messages_fts WHERE message_id = old.id;
        END;
        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
            DELETE FROM messages_fts WHERE message_id = old.id;
            INSERT INTO messages_fts(message_id, session_id, content)
            VALUES (new.id, new.session_id, new.content);
        END;
        CREATE TRIGGER IF NOT EXISTS memory_records_ai AFTER INSERT ON memory_records BEGIN
            INSERT INTO memory_records_fts(memory_id, subject, content, tags_text)
            VALUES (new.id, new.subject, new.content, COALESCE(new.tags_text, ''));
        END;
        CREATE TRIGGER IF NOT EXISTS memory_records_ad AFTER DELETE ON memory_records BEGIN
            DELETE FROM memory_records_fts WHERE memory_id = old.id;
        END;
        CREATE TRIGGER IF NOT EXISTS memory_records_au AFTER UPDATE ON memory_records BEGIN
            DELETE FROM memory_records_fts WHERE memory_id = old.id;
            INSERT INTO memory_records_fts(memory_id, subject, content, tags_text)
            VALUES (new.id, new.subject, new.content, COALESCE(new.tags_text, ''));
        END;
        CREATE TABLE IF NOT EXISTS usage_patterns (
            id TEXT PRIMARY KEY,
            pattern_type TEXT NOT NULL,
            description TEXT NOT NULL,
            trigger_hint TEXT NOT NULL DEFAULT '',
            frequency INTEGER NOT NULL DEFAULT 1,
            confidence INTEGER NOT NULL DEFAULT 50,
            last_seen_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            workspace_key TEXT,
            provider_id TEXT
        );
        ",
    )?;
    Ok(())
}
