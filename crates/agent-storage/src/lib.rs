use agent_core::{
    AppConfig, ConnectorApprovalRecord, ConnectorApprovalStatus, ConnectorKind, LogEntry,
    MemoryRecord, MemoryReviewStatus, MemoryScope, Mission, MissionCheckpoint, MissionStatus,
    ModelAlias, PatternType, SessionMessage, SessionSearchHit, SessionSummary, SkillDraft,
    SkillDraftStatus, TaskMode, UsagePattern, APP_NAME, APP_SLUG,
};
use anyhow::{anyhow, Context, Result};
use auto_launch::AutoLaunchBuilder;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, types::Type, Connection, OptionalExtension};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

mod helpers;
mod logs;
mod paths;
pub mod plugins;
mod session_input;
mod session_summary;
use helpers::*;
pub use paths::AppPaths;
pub use session_input::PersistSessionTurnInput;
use session_summary::row_to_session_summary;

#[derive(Debug, Clone)]
pub struct Storage {
    paths: AppPaths,
}

impl Storage {
    fn init_schema(&self) -> Result<()> {
        let connection = self.connection()?;
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
        ensure_column(&connection, "sessions", "title", "TEXT")?;
        ensure_column(&connection, "sessions", "task_mode", "TEXT")?;
        ensure_column(&connection, "sessions", "cwd", "TEXT")?;
        ensure_column(&connection, "messages", "attachments_json", "TEXT")?;
        ensure_column(&connection, "messages", "tool_call_id", "TEXT")?;
        ensure_column(&connection, "messages", "tool_name", "TEXT")?;
        ensure_column(&connection, "messages", "tool_calls_json", "TEXT")?;
        ensure_column(&connection, "messages", "provider_payload_json", "TEXT")?;
        ensure_column(
            &connection,
            "messages",
            "provider_output_items_json",
            "TEXT",
        )?;
        ensure_column(&connection, "missions", "updated_at", "TEXT")?;
        ensure_column(&connection, "missions", "alias", "TEXT")?;
        ensure_column(&connection, "missions", "requested_model", "TEXT")?;
        ensure_column(&connection, "missions", "session_id", "TEXT")?;
        ensure_column(&connection, "missions", "phase_json", "TEXT")?;
        ensure_column(&connection, "missions", "handoff_summary", "TEXT")?;
        ensure_column(&connection, "missions", "workspace_key", "TEXT")?;
        ensure_column(&connection, "missions", "watch_path", "TEXT")?;
        ensure_column(&connection, "missions", "watch_recursive", "INTEGER")?;
        ensure_column(&connection, "missions", "watch_fingerprint", "TEXT")?;
        ensure_column(&connection, "missions", "wake_trigger_json", "TEXT")?;
        ensure_column(&connection, "missions", "wake_at", "TEXT")?;
        ensure_column(&connection, "missions", "scheduled_for_at", "TEXT")?;
        ensure_column(
            &connection,
            "missions",
            "repeat_interval_seconds",
            "INTEGER",
        )?;
        ensure_column(&connection, "missions", "repeat_anchor_at", "TEXT")?;
        ensure_column(&connection, "missions", "last_error", "TEXT")?;
        ensure_column(&connection, "missions", "retries", "INTEGER")?;
        ensure_column(&connection, "missions", "max_retries", "INTEGER")?;
        ensure_column(&connection, "missions", "evolve", "INTEGER")?;
        ensure_column(&connection, "mission_checkpoints", "phase_json", "TEXT")?;
        ensure_column(
            &connection,
            "mission_checkpoints",
            "handoff_summary",
            "TEXT",
        )?;
        ensure_column(&connection, "mission_checkpoints", "next_wake_at", "TEXT")?;
        ensure_column(
            &connection,
            "mission_checkpoints",
            "scheduled_for_at",
            "TEXT",
        )?;
        ensure_column(&connection, "memory_records", "last_used_at", "TEXT")?;
        ensure_column(&connection, "memory_records", "source_session_id", "TEXT")?;
        ensure_column(&connection, "memory_records", "source_message_id", "TEXT")?;
        ensure_column(&connection, "memory_records", "provider_id", "TEXT")?;
        ensure_column(&connection, "memory_records", "workspace_key", "TEXT")?;
        ensure_column(&connection, "memory_records", "tags_json", "TEXT")?;
        ensure_column(&connection, "memory_records", "tags_text", "TEXT")?;
        ensure_column(&connection, "memory_records", "identity_key", "TEXT")?;
        ensure_column(&connection, "memory_records", "observation_source", "TEXT")?;
        ensure_column(&connection, "memory_records", "superseded_by", "TEXT")?;
        ensure_column(&connection, "memory_records", "review_status_json", "TEXT")?;
        ensure_column(&connection, "memory_records", "review_note", "TEXT")?;
        ensure_column(&connection, "memory_records", "reviewed_at", "TEXT")?;
        ensure_column(&connection, "memory_records", "supersedes", "TEXT")?;
        ensure_column(&connection, "memory_records", "evidence_refs_json", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "trigger_hint", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "workspace_key", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "provider_id", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "source_session_id", "TEXT")?;
        ensure_column(
            &connection,
            "skill_drafts",
            "source_message_ids_json",
            "TEXT",
        )?;
        ensure_column(&connection, "skill_drafts", "usage_count", "INTEGER")?;
        ensure_column(&connection, "skill_drafts", "status_json", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "updated_at", "TEXT")?;
        ensure_column(&connection, "skill_drafts", "last_used_at", "TEXT")?;
        ensure_column(
            &connection,
            "connector_approvals",
            "source_event_id",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "external_chat_id",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "external_chat_display",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "external_user_id",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "external_user_display",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "message_preview",
            "TEXT",
        )?;
        ensure_column(
            &connection,
            "connector_approvals",
            "queued_mission_id",
            "TEXT",
        )?;
        ensure_column(&connection, "connector_approvals", "reviewed_at", "TEXT")?;
        ensure_column(&connection, "connector_approvals", "review_note", "TEXT")?;
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
            CREATE UNIQUE INDEX IF NOT EXISTS idx_connector_approvals_source_key
            ON connector_approvals(source_key);
            CREATE INDEX IF NOT EXISTS idx_connector_approvals_status
            ON connector_approvals(status_json, updated_at DESC);
            ",
        )?;
        rebuild_messages_fts(&connection)?;
        rebuild_memory_fts(&connection)?;
        Ok(())
    }

    pub fn upsert_connector_approval(&self, approval: &ConnectorApprovalRecord) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "
            INSERT INTO connector_approvals (
                id, connector_kind_json, connector_id, connector_name, status_json, title, details,
                source_key, source_event_id, external_chat_id, external_chat_display,
                external_user_id, external_user_display, message_preview, queued_mission_id,
                created_at, updated_at, reviewed_at, review_note
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(id) DO UPDATE SET
                connector_kind_json = excluded.connector_kind_json,
                connector_id = excluded.connector_id,
                connector_name = excluded.connector_name,
                status_json = excluded.status_json,
                title = excluded.title,
                details = excluded.details,
                source_key = excluded.source_key,
                source_event_id = excluded.source_event_id,
                external_chat_id = excluded.external_chat_id,
                external_chat_display = excluded.external_chat_display,
                external_user_id = excluded.external_user_id,
                external_user_display = excluded.external_user_display,
                message_preview = excluded.message_preview,
                queued_mission_id = COALESCE(excluded.queued_mission_id, connector_approvals.queued_mission_id),
                updated_at = excluded.updated_at,
                reviewed_at = excluded.reviewed_at,
                review_note = excluded.review_note
            ",
            params![
                approval.id,
                serde_json::to_string(&approval.connector_kind)?,
                approval.connector_id,
                approval.connector_name,
                serde_json::to_string(&approval.status)?,
                approval.title,
                approval.details,
                approval.source_key,
                approval.source_event_id,
                approval.external_chat_id,
                approval.external_chat_display,
                approval.external_user_id,
                approval.external_user_display,
                approval.message_preview,
                approval.queued_mission_id,
                approval.created_at.to_rfc3339(),
                approval.updated_at.to_rfc3339(),
                approval.reviewed_at.map(|value| value.to_rfc3339()),
                approval.review_note,
            ],
        )?;
        Ok(())
    }

    pub fn get_connector_approval(
        &self,
        approval_id: &str,
    ) -> Result<Option<ConnectorApprovalRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, connector_kind_json, connector_id, connector_name, status_json, title, details,
                source_key, source_event_id, external_chat_id, external_chat_display,
                external_user_id, external_user_display, message_preview, queued_mission_id,
                created_at, updated_at, reviewed_at, review_note
            FROM connector_approvals
            WHERE id = ?1
            ",
        )?;
        statement
            .query_row([approval_id], row_to_connector_approval)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_connector_approvals(
        &self,
        connector_kind: Option<ConnectorKind>,
        status: Option<ConnectorApprovalStatus>,
        limit: usize,
    ) -> Result<Vec<ConnectorApprovalRecord>> {
        let connection = self.connection()?;
        let connector_kind_json = connector_kind
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        let status_json = status
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, connector_kind_json, connector_id, connector_name, status_json, title, details,
                source_key, source_event_id, external_chat_id, external_chat_display,
                external_user_id, external_user_display, message_preview, queued_mission_id,
                created_at, updated_at, reviewed_at, review_note
            FROM connector_approvals
            WHERE (?1 IS NULL OR connector_kind_json = ?1)
              AND (?2 IS NULL OR status_json = ?2)
            ORDER BY updated_at DESC
            LIMIT ?3
            ",
        )?;
        let rows = statement.query_map(
            params![connector_kind_json, status_json, limit as i64],
            row_to_connector_approval,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn count_pending_connector_approvals(&self) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM connector_approvals WHERE status_json = ?1",
            [serde_json::to_string(&ConnectorApprovalStatus::Pending)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn update_connector_approval_status(
        &self,
        approval_id: &str,
        status: ConnectorApprovalStatus,
        note: Option<&str>,
        queued_mission_id: Option<&str>,
    ) -> Result<bool> {
        let connection = self.connection()?;
        let reviewed_at = match status {
            ConnectorApprovalStatus::Pending => Option::<String>::None,
            _ => Some(Utc::now().to_rfc3339()),
        };
        let updated = connection.execute(
            "
            UPDATE connector_approvals
            SET status_json = ?2,
                review_note = ?3,
                queued_mission_id = COALESCE(?4, queued_mission_id),
                reviewed_at = ?5,
                updated_at = ?6
            WHERE id = ?1
            ",
            params![
                approval_id,
                serde_json::to_string(&status)?,
                note,
                queued_mission_id,
                reviewed_at,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn ensure_session(
        &self,
        session_id: &str,
        alias: &ModelAlias,
        provider_id: &str,
        model: &str,
        task_mode: Option<TaskMode>,
    ) -> Result<()> {
        self.ensure_session_with_title(session_id, None, alias, provider_id, model, task_mode, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ensure_session_with_title(
        &self,
        session_id: &str,
        title: Option<&str>,
        alias: &ModelAlias,
        provider_id: &str,
        model: &str,
        task_mode: Option<TaskMode>,
        cwd: Option<&Path>,
    ) -> Result<()> {
        let connection = self.connection()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "
            INSERT INTO sessions (id, title, alias, provider_id, model, task_mode, cwd, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(id) DO UPDATE SET
                title = COALESCE(excluded.title, sessions.title),
                alias = excluded.alias,
                provider_id = excluded.provider_id,
                model = excluded.model,
                task_mode = COALESCE(excluded.task_mode, sessions.task_mode),
                cwd = COALESCE(excluded.cwd, sessions.cwd),
                updated_at = excluded.updated_at
            ",
            params![
                session_id,
                title,
                alias.alias,
                provider_id,
                model,
                task_mode.map(TaskMode::as_str),
                cwd.map(|path| path.display().to_string()),
                now
            ],
        )?;
        Ok(())
    }

    pub fn rename_session(&self, session_id: &str, title: &str) -> Result<()> {
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1",
            params![session_id, title, Utc::now().to_rfc3339()],
        )?;
        if updated == 0 {
            return Err(anyhow!("unknown session '{session_id}'"));
        }
        Ok(())
    }

    pub fn append_message(&self, message: &SessionMessage) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "
            INSERT INTO messages (
                id, session_id, role_json, content, created_at, provider_id, model,
                attachments_json, tool_call_id, tool_name, tool_calls_json, provider_payload_json,
                provider_output_items_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ",
            params![
                message.id,
                message.session_id,
                serde_json::to_string(&message.role)?,
                message.content,
                message.created_at.to_rfc3339(),
                message.provider_id,
                message.model,
                serde_json::to_string(&message.attachments)?,
                message.tool_call_id,
                message.tool_name,
                serde_json::to_string(&message.tool_calls)?,
                message.provider_payload_json,
                serde_json::to_string(&message.provider_output_items)?,
            ],
        )?;
        connection.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![message.session_id, message.created_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn persist_session_turn(&self, input: PersistSessionTurnInput<'_>) -> Result<()> {
        let PersistSessionTurnInput {
            session_id,
            title,
            alias,
            provider_id,
            model,
            task_mode,
            cwd,
            messages,
        } = input;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let updated_at = messages
            .last()
            .map(|message| message.created_at)
            .unwrap_or_else(Utc::now)
            .to_rfc3339();
        transaction.execute(
            "
            INSERT INTO sessions (id, title, alias, provider_id, model, task_mode, cwd, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
            ON CONFLICT(id) DO UPDATE SET
                title = COALESCE(excluded.title, sessions.title),
                alias = excluded.alias,
                provider_id = excluded.provider_id,
                model = excluded.model,
                task_mode = COALESCE(excluded.task_mode, sessions.task_mode),
                cwd = COALESCE(excluded.cwd, sessions.cwd),
                updated_at = excluded.updated_at
            ",
            params![
                session_id,
                title,
                alias.alias,
                provider_id,
                model,
                task_mode.map(TaskMode::as_str),
                cwd.map(|path| path.display().to_string()),
                updated_at,
            ],
        )?;

        for message in messages {
            transaction.execute(
                "
                INSERT INTO messages (
                    id, session_id, role_json, content, created_at, provider_id, model,
                    attachments_json, tool_call_id, tool_name, tool_calls_json, provider_payload_json,
                    provider_output_items_json
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                ",
                params![
                    message.id,
                    message.session_id,
                    serde_json::to_string(&message.role)?,
                    message.content,
                    message.created_at.to_rfc3339(),
                    message.provider_id,
                    message.model,
                    serde_json::to_string(&message.attachments)?,
                    message.tool_call_id,
                    message.tool_name,
                    serde_json::to_string(&message.tool_calls)?,
                    message.provider_payload_json,
                    serde_json::to_string(&message.provider_output_items)?,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn list_session_messages(&self, session_id: &str) -> Result<Vec<SessionMessage>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, session_id, role_json, content, created_at, provider_id, model,
                attachments_json, tool_call_id, tool_name, tool_calls_json, provider_payload_json,
                provider_output_items_json
            FROM messages
            WHERE session_id = ?1
            ORDER BY created_at ASC, id ASC
            ",
        )?;
        let rows = statement.query_map([session_id], |row| {
            let created_at: String = row.get(4)?;
            let role_json: String = row.get(2)?;
            let role = serde_json::from_str(&role_json).map_err(json_decode_error)?;
            let attachments_json: Option<String> = row.get(7)?;
            let attachments = attachments_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(json_decode_error)?
                .unwrap_or_default();
            let tool_calls_json: Option<String> = row.get(10)?;
            let tool_calls = tool_calls_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(json_decode_error)?
                .unwrap_or_default();
            let provider_output_items_json: Option<String> = row.get(12)?;
            let provider_output_items = provider_output_items_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(json_decode_error)?
                .unwrap_or_default();
            Ok(SessionMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role,
                content: row.get(3)?,
                created_at: parse_datetime(&created_at)?,
                provider_id: row.get(5)?,
                model: row.get(6)?,
                tool_call_id: row.get(8)?,
                tool_name: row.get(9)?,
                tool_calls,
                provider_payload_json: row.get(11)?,
                attachments,
                provider_output_items,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                sessions.id,
                sessions.title,
                sessions.alias,
                sessions.provider_id,
                sessions.model,
                sessions.task_mode,
                sessions.cwd,
                sessions.created_at,
                sessions.updated_at,
                COUNT(messages.id) AS message_count
            FROM sessions
            LEFT JOIN messages ON messages.session_id = sessions.id
            GROUP BY sessions.id, sessions.title, sessions.alias, sessions.provider_id, sessions.model, sessions.task_mode, sessions.cwd, sessions.created_at, sessions.updated_at
            ORDER BY updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map([limit as i64], row_to_session_summary)?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                sessions.id,
                sessions.title,
                sessions.alias,
                sessions.provider_id,
                sessions.model,
                sessions.task_mode,
                sessions.cwd,
                sessions.created_at,
                sessions.updated_at,
                COUNT(messages.id) AS message_count
            FROM sessions
            LEFT JOIN messages ON messages.session_id = sessions.id
            WHERE sessions.id = ?1
            GROUP BY sessions.id, sessions.title, sessions.alias, sessions.provider_id, sessions.model, sessions.task_mode, sessions.cwd, sessions.created_at, sessions.updated_at
            ",
        )?;
        let session = statement
            .query_row([session_id], row_to_session_summary)
            .optional()?;
        Ok(session)
    }

    pub fn upsert_mission(&self, mission: &Mission) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "
            INSERT INTO missions (
                id, title, details, status_json, created_at, updated_at, alias, requested_model,
                session_id, phase_json, handoff_summary, workspace_key, watch_path, watch_recursive,
                watch_fingerprint, wake_trigger_json, wake_at, scheduled_for_at,
                repeat_interval_seconds, repeat_anchor_at, last_error, retries, max_retries, evolve
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                details = excluded.details,
                status_json = excluded.status_json,
                updated_at = excluded.updated_at,
                alias = excluded.alias,
                requested_model = excluded.requested_model,
                session_id = excluded.session_id,
                phase_json = excluded.phase_json,
                handoff_summary = excluded.handoff_summary,
                workspace_key = excluded.workspace_key,
                watch_path = excluded.watch_path,
                watch_recursive = excluded.watch_recursive,
                watch_fingerprint = excluded.watch_fingerprint,
                wake_trigger_json = excluded.wake_trigger_json,
                wake_at = excluded.wake_at,
                scheduled_for_at = excluded.scheduled_for_at,
                repeat_interval_seconds = excluded.repeat_interval_seconds,
                repeat_anchor_at = excluded.repeat_anchor_at,
                last_error = excluded.last_error,
                retries = excluded.retries,
                max_retries = excluded.max_retries,
                evolve = excluded.evolve
            ",
            params![
                mission.id,
                mission.title,
                mission.details,
                serde_json::to_string(&mission.status)?,
                mission.created_at.to_rfc3339(),
                mission.updated_at.to_rfc3339(),
                mission.alias,
                mission.requested_model,
                mission.session_id,
                mission.phase.as_ref().map(serde_json::to_string).transpose()?,
                mission.handoff_summary,
                mission.workspace_key,
                mission.watch_path.as_ref().map(|path| path.display().to_string()),
                mission.watch_recursive,
                mission.watch_fingerprint,
                mission
                    .wake_trigger
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                mission.wake_at.map(|value| value.to_rfc3339()),
                mission.scheduled_for_at.map(|value| value.to_rfc3339()),
                mission.repeat_interval_seconds,
                mission.repeat_anchor_at.map(|value| value.to_rfc3339()),
                mission.last_error,
                mission.retries,
                mission.max_retries,
                mission.evolve,
            ],
        )?;
        Ok(())
    }

    pub fn insert_mission(&self, mission: &Mission) -> Result<()> {
        self.upsert_mission(mission)
    }

    pub fn get_mission(&self, mission_id: &str) -> Result<Option<Mission>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, title, details, status_json, created_at, updated_at, alias, requested_model,
                session_id, phase_json, handoff_summary, workspace_key, watch_path, watch_recursive,
                watch_fingerprint, wake_trigger_json, wake_at, scheduled_for_at,
                repeat_interval_seconds, repeat_anchor_at, last_error, retries, max_retries, evolve
            FROM missions
            WHERE id = ?1
            ",
        )?;
        statement
            .query_row([mission_id], row_to_mission)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_missions(&self) -> Result<Vec<Mission>> {
        self.list_missions_limited(None)
    }

    pub fn list_missions_limited(&self, limit: Option<usize>) -> Result<Vec<Mission>> {
        let connection = self.connection()?;
        let query = if let Some(limit) = limit {
            format!(
                "
            SELECT
                id, title, details, status_json, created_at, updated_at, alias, requested_model,
                session_id, phase_json, handoff_summary, workspace_key, watch_path, watch_recursive,
                watch_fingerprint, wake_trigger_json, wake_at, scheduled_for_at,
                repeat_interval_seconds, repeat_anchor_at, last_error, retries, max_retries, evolve
            FROM missions
            ORDER BY updated_at DESC, created_at DESC
            LIMIT {}
            ",
                limit.max(1)
            )
        } else {
            "
            SELECT
                id, title, details, status_json, created_at, updated_at, alias, requested_model,
                session_id, phase_json, handoff_summary, workspace_key, watch_path, watch_recursive,
                watch_fingerprint, wake_trigger_json, wake_at, scheduled_for_at,
                repeat_interval_seconds, repeat_anchor_at, last_error, retries, max_retries, evolve
            FROM missions
            ORDER BY updated_at DESC, created_at DESC
            "
            .to_string()
        };
        let mut statement = connection.prepare(&query)?;
        let rows = statement.query_map([], row_to_mission)?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_runnable_missions(&self, now: DateTime<Utc>, limit: usize) -> Result<Vec<Mission>> {
        let mut missions = self
            .list_missions()?
            .into_iter()
            .filter(|mission| match mission.status {
                MissionStatus::Queued | MissionStatus::Running => true,
                MissionStatus::Waiting | MissionStatus::Scheduled => mission
                    .wake_at
                    .map(|wake_at| wake_at <= now)
                    .unwrap_or(false),
                MissionStatus::Blocked
                | MissionStatus::Completed
                | MissionStatus::Failed
                | MissionStatus::Cancelled => false,
            })
            .collect::<Vec<_>>();
        missions.sort_by(|left, right| left.updated_at.cmp(&right.updated_at));
        missions.truncate(limit);
        Ok(missions)
    }

    pub fn count_active_missions(&self) -> Result<usize> {
        let connection = self.connection()?;
        let completed = serde_json::to_string(&MissionStatus::Completed)?;
        let failed = serde_json::to_string(&MissionStatus::Failed)?;
        let cancelled = serde_json::to_string(&MissionStatus::Cancelled)?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM missions WHERE status_json NOT IN (?1, ?2, ?3)",
            params![completed, failed, cancelled],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_missions(&self) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM missions", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn save_mission_checkpoint(&self, checkpoint: &MissionCheckpoint) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "
            INSERT INTO mission_checkpoints (
                id, mission_id, status_json, summary, created_at, session_id,
                phase_json, handoff_summary, response_excerpt, next_wake_at, scheduled_for_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ",
            params![
                checkpoint.id,
                checkpoint.mission_id,
                serde_json::to_string(&checkpoint.status)?,
                checkpoint.summary,
                checkpoint.created_at.to_rfc3339(),
                checkpoint.session_id,
                checkpoint
                    .phase
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                checkpoint.handoff_summary,
                checkpoint.response_excerpt,
                checkpoint.next_wake_at.map(|value| value.to_rfc3339()),
                checkpoint.scheduled_for_at.map(|value| value.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn list_mission_checkpoints(
        &self,
        mission_id: &str,
        limit: usize,
    ) -> Result<Vec<MissionCheckpoint>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, mission_id, status_json, summary, created_at, session_id,
                phase_json, handoff_summary, response_excerpt, next_wake_at, scheduled_for_at
            FROM mission_checkpoints
            WHERE mission_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(params![mission_id, limit as i64], |row| {
            let created_at: String = row.get(4)?;
            let next_wake_at: Option<String> = row.get(9)?;
            let scheduled_for_at: Option<String> = row.get(10)?;
            let status_json: String = row.get(2)?;
            let phase_json: Option<String> = row.get(6)?;
            Ok(MissionCheckpoint {
                id: row.get(0)?,
                mission_id: row.get(1)?,
                status: serde_json::from_str(&status_json).map_err(json_decode_error)?,
                summary: row.get(3)?,
                created_at: parse_datetime(&created_at)?,
                session_id: row.get(5)?,
                phase: phase_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(json_decode_error)?,
                handoff_summary: row.get(7)?,
                response_excerpt: row.get(8)?,
                next_wake_at: next_wake_at.as_deref().map(parse_datetime).transpose()?,
                scheduled_for_at: scheduled_for_at
                    .as_deref()
                    .map(parse_datetime)
                    .transpose()?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn upsert_memory(&self, memory: &MemoryRecord) -> Result<()> {
        let connection = self.connection()?;
        let tags_json = serde_json::to_string(&memory.tags)?;
        let evidence_refs_json = serde_json::to_string(&memory.evidence_refs)?;
        let tags_text = memory.tags.join(" ");
        connection.execute(
            "
            INSERT INTO memory_records (
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, tags_text, identity_key, observation_source, superseded_by,
                review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(id) DO UPDATE SET
                kind_json = excluded.kind_json,
                scope_json = excluded.scope_json,
                subject = excluded.subject,
                content = excluded.content,
                confidence = excluded.confidence,
                updated_at = excluded.updated_at,
                last_used_at = excluded.last_used_at,
                source_session_id = COALESCE(excluded.source_session_id, memory_records.source_session_id),
                source_message_id = COALESCE(excluded.source_message_id, memory_records.source_message_id),
                provider_id = COALESCE(excluded.provider_id, memory_records.provider_id),
                workspace_key = COALESCE(excluded.workspace_key, memory_records.workspace_key),
                tags_json = excluded.tags_json,
                tags_text = excluded.tags_text,
                identity_key = COALESCE(excluded.identity_key, memory_records.identity_key),
                observation_source = COALESCE(excluded.observation_source, memory_records.observation_source),
                superseded_by = excluded.superseded_by,
                review_status_json = excluded.review_status_json,
                review_note = excluded.review_note,
                reviewed_at = excluded.reviewed_at,
                supersedes = excluded.supersedes,
                evidence_refs_json = excluded.evidence_refs_json
            ",
            params![
                memory.id,
                serde_json::to_string(&memory.kind)?,
                serde_json::to_string(&memory.scope)?,
                memory.subject,
                memory.content,
                i64::from(memory.confidence),
                memory.created_at.to_rfc3339(),
                memory.updated_at.to_rfc3339(),
                memory.last_used_at.map(|value| value.to_rfc3339()),
                memory.source_session_id,
                memory.source_message_id,
                memory.provider_id,
                memory.workspace_key,
                tags_json,
                tags_text,
                memory.identity_key,
                memory.observation_source,
                memory.superseded_by,
                serde_json::to_string(&memory.review_status)?,
                memory.review_note,
                memory.reviewed_at.map(|value| value.to_rfc3339()),
                memory.supersedes,
                evidence_refs_json,
            ],
        )?;
        Ok(())
    }

    /// Store an embedding vector for a memory record.
    pub fn upsert_memory_embedding(
        &self,
        memory_id: &str,
        embedding: &[f32],
        model: &str,
    ) -> Result<()> {
        let connection = self.connection()?;
        let blob = embedding_to_blob(embedding);
        connection.execute(
            "
            INSERT INTO memory_embeddings (memory_id, embedding, model, dimensions, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(memory_id) DO UPDATE SET
                embedding = excluded.embedding,
                model = excluded.model,
                dimensions = excluded.dimensions,
                created_at = excluded.created_at
            ",
            params![
                memory_id,
                blob,
                model,
                embedding.len() as i64,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Find memories whose embeddings are most similar to the query embedding.
    /// Returns `(memory_id, similarity_score)` pairs sorted by descending similarity.
    pub fn search_memories_by_embedding(
        &self,
        query_embedding: &[f32],
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;

        // Load all embeddings and compute cosine similarity in Rust.
        // For typical memory counts (hundreds to low thousands) this is fast enough.
        // If the table grows very large, we'd switch to an ANN index.
        let mut statement = connection.prepare(
            "
            SELECT me.memory_id, me.embedding,
                mr.id, mr.kind_json, mr.scope_json, mr.subject, mr.content, mr.confidence,
                mr.created_at, mr.updated_at, mr.last_used_at, mr.source_session_id,
                mr.source_message_id, mr.provider_id, mr.workspace_key, mr.tags_json,
                mr.identity_key, mr.observation_source, mr.superseded_by,
                mr.review_status_json, mr.review_note, mr.reviewed_at, mr.supersedes,
                mr.evidence_refs_json
            FROM memory_embeddings me
            JOIN memory_records mr ON mr.id = me.memory_id
            WHERE mr.superseded_by IS NULL
              AND mr.review_status_json = ?1
            ",
        )?;
        let accepted_json = serde_json::to_string(&MemoryReviewStatus::Accepted)?;
        let rows = statement.query_map(params![accepted_json], |row| {
            let embedding_blob: Vec<u8> = row.get(1)?;
            let memory = row_to_memory_at_offset(row, 2)?;
            Ok((embedding_blob, memory))
        })?;

        let query_norm = vector_norm(query_embedding);
        let mut scored: Vec<(f32, MemoryRecord)> = Vec::new();
        for row in rows {
            let (blob, memory) = row?;
            if exclude_ids.contains(&memory.id) {
                continue;
            }
            if !memory_matches_scope(&memory, workspace_key, provider_id) {
                continue;
            }
            let stored = blob_to_embedding(&blob);
            let similarity = cosine_similarity(query_embedding, &stored, query_norm);
            scored.push((similarity, memory));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(limit)
            .filter(|(score, _)| *score > 0.3) // minimum similarity threshold
            .map(|(_, memory)| memory)
            .collect())
    }

    /// Check whether any embeddings exist.
    pub fn has_memory_embeddings(&self) -> Result<bool> {
        let connection = self.connection()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM memory_embeddings", [], |row| {
                row.get(0)
            })?;
        Ok(count > 0)
    }

    /// Delete the embedding for a memory record.
    pub fn delete_memory_embedding(&self, memory_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM memory_embeddings WHERE memory_id = ?1",
            params![memory_id],
        )?;
        Ok(())
    }

    pub fn get_memory(&self, memory_id: &str) -> Result<Option<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE id = ?1
            ",
        )?;
        statement
            .query_row([memory_id], row_to_memory)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_memories(&self, limit: usize) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?2
            ORDER BY updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map(
            params![
                limit as i64,
                serde_json::to_string(&MemoryReviewStatus::Accepted)?
            ],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_memories_by_review_status(
        &self,
        status: MemoryReviewStatus,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(
            params![serde_json::to_string(&status)?, limit as i64],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn count_memories(&self) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_records WHERE superseded_by IS NULL AND review_status_json = ?1",
            [serde_json::to_string(&MemoryReviewStatus::Accepted)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn count_memories_by_review_status(&self, status: MemoryReviewStatus) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_records WHERE superseded_by IS NULL AND review_status_json = ?1",
            [serde_json::to_string(&status)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn list_memories_by_tag(
        &self,
        tag: &str,
        limit: usize,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?3
              AND tags_text LIKE ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            ",
        )?;
        let rows = statement.query_map(
            params![
                format!("%{tag}%"),
                limit as i64,
                serde_json::to_string(&MemoryReviewStatus::Accepted)?
            ],
            row_to_memory,
        )?;
        let memories = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
            .collect::<Vec<_>>();
        Ok(memories)
    }

    pub fn list_memories_by_source_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE superseded_by IS NULL
              AND review_status_json = ?1
              AND source_session_id = ?2
            ORDER BY updated_at DESC
            LIMIT ?3
            ",
        )?;
        let rows = statement.query_map(
            params![
                serde_json::to_string(&MemoryReviewStatus::Accepted)?,
                session_id,
                limit as i64
            ],
            row_to_memory,
        )?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn forget_memory(&self, memory_id: &str) -> Result<bool> {
        let connection = self.connection()?;
        let deleted =
            connection.execute("DELETE FROM memory_records WHERE id = ?1", [memory_id])?;
        Ok(deleted > 0)
    }

    pub fn touch_memory(&self, memory_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE memory_records SET last_used_at = ?2 WHERE id = ?1",
            params![memory_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn update_memory_review_status(
        &self,
        memory_id: &str,
        status: MemoryReviewStatus,
        note: Option<&str>,
    ) -> Result<bool> {
        let connection = self.connection()?;
        let reviewed_at = match status {
            MemoryReviewStatus::Candidate => Option::<String>::None,
            _ => Some(Utc::now().to_rfc3339()),
        };
        let updated = connection.execute(
            "UPDATE memory_records SET review_status_json = ?2, review_note = ?3, reviewed_at = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                memory_id,
                serde_json::to_string(&status)?,
                note,
                reviewed_at,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn mark_memory_superseded(&self, memory_id: &str, superseded_by: &str) -> Result<bool> {
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE memory_records SET superseded_by = ?2, updated_at = ?3 WHERE id = ?1",
            params![memory_id, superseded_by, Utc::now().to_rfc3339()],
        )?;
        Ok(updated > 0)
    }

    pub fn find_memory_by_subject(
        &self,
        subject: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Option<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE lower(subject) = lower(?1)
              AND superseded_by IS NULL
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([subject], row_to_memory)?;
        for row in rows {
            let memory = row?;
            if memory_matches_scope(&memory, workspace_key, provider_id) {
                return Ok(Some(memory));
            }
        }
        Ok(None)
    }

    pub fn find_memory_by_identity_key(
        &self,
        identity_key: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Option<MemoryRecord>> {
        Ok(self
            .list_active_memories_by_identity_key(identity_key, workspace_key, provider_id)?
            .into_iter()
            .next())
    }

    pub fn list_active_memories_by_identity_key(
        &self,
        identity_key: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, kind_json, scope_json, subject, content, confidence, created_at, updated_at,
                last_used_at, source_session_id, source_message_id, provider_id, workspace_key,
                tags_json, identity_key, observation_source, superseded_by, review_status_json, review_note, reviewed_at, supersedes, evidence_refs_json
            FROM memory_records
            WHERE lower(identity_key) = lower(?1)
              AND superseded_by IS NULL
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([identity_key], row_to_memory)?;
        let mut memories = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
            .collect::<Vec<_>>();
        memories.sort_by(|left, right| {
            memory_review_status_rank(left.review_status.clone())
                .cmp(&memory_review_status_rank(right.review_status.clone()))
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(memories)
    }

    pub fn search_memories(
        &self,
        query: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
        review_statuses: &[MemoryReviewStatus],
        include_superseded: bool,
        limit: usize,
    ) -> Result<(Vec<MemoryRecord>, Vec<SessionSearchHit>)> {
        let connection = self.connection()?;
        let fts_query = normalize_fts_query(query);

        // Try expanded (stemmed) FTS first, fall back to exact FTS.
        let expanded_query = build_expanded_fts_query(query);
        let effective_fts_query = if expanded_query.is_empty() {
            &fts_query
        } else {
            &expanded_query
        };

        let mut memories = if effective_fts_query.is_empty() {
            Vec::new()
        } else {
            let mut statement = connection.prepare(
                "
                SELECT
                    mr.id, mr.kind_json, mr.scope_json, mr.subject, mr.content, mr.confidence,
                    mr.created_at, mr.updated_at, mr.last_used_at, mr.source_session_id,
                    mr.source_message_id, mr.provider_id, mr.workspace_key, mr.tags_json,
                    mr.identity_key, mr.observation_source, mr.superseded_by, mr.review_status_json, mr.review_note, mr.reviewed_at, mr.supersedes, mr.evidence_refs_json
                FROM memory_records_fts
                JOIN memory_records mr ON mr.id = memory_records_fts.memory_id
                WHERE memory_records_fts MATCH ?1
                  AND (?3 = 1 OR mr.superseded_by IS NULL)
                ORDER BY bm25(memory_records_fts), mr.updated_at DESC
                LIMIT ?2
                ",
            )?;
            let rows = statement.query_map(
                params![
                    effective_fts_query,
                    limit as i64,
                    if include_superseded { 1 } else { 0 }
                ],
                row_to_memory,
            )?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|memory| memory_matches_scope(memory, workspace_key, provider_id))
                .filter(|memory| {
                    memory_matches_query_filters(memory, review_statuses, include_superseded)
                })
                .take(limit)
                .collect()
        };

        // Fuzzy LIKE fallback when FTS returns fewer than half the limit.
        if memories.len() < limit / 2 {
            let exclude_ids: Vec<String> = memories.iter().map(|m| m.id.clone()).collect();
            let remaining = limit - memories.len();
            let fuzzy_results = fuzzy_memory_search(
                &connection,
                query,
                workspace_key,
                provider_id,
                review_statuses,
                include_superseded,
                remaining,
                &exclude_ids,
            )?;
            memories.extend(fuzzy_results);
        }

        let transcript_hits = if fts_query.is_empty() {
            Vec::new()
        } else {
            let mut statement = connection.prepare(
                "
                SELECT
                    m.session_id, m.id, m.role_json, m.content, m.created_at, m.provider_id,
                    m.model
                FROM messages_fts
                JOIN messages m ON m.id = messages_fts.message_id
                WHERE messages_fts MATCH ?1
                ORDER BY bm25(messages_fts), m.created_at DESC
                LIMIT ?2
                ",
            )?;
            let rows = statement.query_map(params![fts_query, limit as i64], |row| {
                let role_json: String = row.get(2)?;
                let created_at: String = row.get(4)?;
                let content: String = row.get(3)?;
                Ok(SessionSearchHit {
                    session_id: row.get(0)?,
                    message_id: row.get(1)?,
                    role: serde_json::from_str(&role_json).map_err(json_decode_error)?,
                    preview: summarize_preview(&content, 280),
                    created_at: parse_datetime(&created_at)?,
                    provider_id: row.get(5)?,
                    model: row.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        Ok((memories, transcript_hits))
    }

    pub fn upsert_skill_draft(&self, draft: &SkillDraft) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "
            INSERT INTO skill_drafts (
                id, title, summary, instructions, trigger_hint, workspace_key, provider_id,
                source_session_id, source_message_ids_json, usage_count, status_json, created_at,
                updated_at, last_used_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                summary = excluded.summary,
                instructions = excluded.instructions,
                trigger_hint = excluded.trigger_hint,
                workspace_key = COALESCE(excluded.workspace_key, skill_drafts.workspace_key),
                provider_id = COALESCE(excluded.provider_id, skill_drafts.provider_id),
                source_session_id = COALESCE(excluded.source_session_id, skill_drafts.source_session_id),
                source_message_ids_json = excluded.source_message_ids_json,
                usage_count = excluded.usage_count,
                status_json = excluded.status_json,
                updated_at = excluded.updated_at,
                last_used_at = excluded.last_used_at
            ",
            params![
                draft.id,
                draft.title,
                draft.summary,
                draft.instructions,
                draft.trigger_hint,
                draft.workspace_key,
                draft.provider_id,
                draft.source_session_id,
                serde_json::to_string(&draft.source_message_ids)?,
                i64::from(draft.usage_count),
                serde_json::to_string(&draft.status)?,
                draft.created_at.to_rfc3339(),
                draft.updated_at.to_rfc3339(),
                draft.last_used_at.map(|value| value.to_rfc3339()),
            ],
        )?;
        Ok(())
    }

    pub fn get_skill_draft(&self, draft_id: &str) -> Result<Option<SkillDraft>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, title, summary, instructions, trigger_hint, workspace_key, provider_id,
                source_session_id, source_message_ids_json, usage_count, status_json, created_at,
                updated_at, last_used_at
            FROM skill_drafts
            WHERE id = ?1
            ",
        )?;
        statement
            .query_row([draft_id], row_to_skill_draft)
            .optional()
            .map_err(Into::into)
    }

    pub fn find_skill_draft_by_title(
        &self,
        title: &str,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Option<SkillDraft>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, title, summary, instructions, trigger_hint, workspace_key, provider_id,
                source_session_id, source_message_ids_json, usage_count, status_json, created_at,
                updated_at, last_used_at
            FROM skill_drafts
            WHERE lower(title) = lower(?1)
            ORDER BY updated_at DESC
            ",
        )?;
        let rows = statement.query_map([title], row_to_skill_draft)?;
        for row in rows {
            let draft = row?;
            if skill_draft_matches_scope(&draft, workspace_key, provider_id) {
                return Ok(Some(draft));
            }
        }
        Ok(None)
    }

    pub fn list_skill_drafts(
        &self,
        limit: usize,
        status: Option<SkillDraftStatus>,
        workspace_key: Option<&str>,
        provider_id: Option<&str>,
    ) -> Result<Vec<SkillDraft>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT
                id, title, summary, instructions, trigger_hint, workspace_key, provider_id,
                source_session_id, source_message_ids_json, usage_count, status_json, created_at,
                updated_at, last_used_at
            FROM skill_drafts
            ORDER BY updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = statement.query_map([limit as i64], row_to_skill_draft)?;
        let drafts = rows
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|draft| {
                let status_ok = status
                    .as_ref()
                    .map(|value| draft.status == *value)
                    .unwrap_or(true);
                status_ok && skill_draft_matches_scope(draft, workspace_key, provider_id)
            })
            .collect::<Vec<_>>();
        Ok(drafts)
    }

    pub fn count_skill_drafts(&self) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM skill_drafts", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn count_skill_drafts_by_status(&self, status: SkillDraftStatus) -> Result<usize> {
        let connection = self.connection()?;
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM skill_drafts WHERE status_json = ?1",
            [serde_json::to_string(&status)?],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn touch_skill_draft(&self, draft_id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE skill_drafts SET last_used_at = ?2 WHERE id = ?1",
            params![draft_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    // Usage patterns

    pub fn upsert_pattern(&self, pattern: &UsagePattern) -> Result<()> {
        let connection = self.connection()?;
        let pattern_type_json = serde_json::to_string(&pattern.pattern_type)?;
        connection.execute(
            "INSERT INTO usage_patterns (id, pattern_type, description, trigger_hint, frequency, confidence, last_seen_at, created_at, workspace_key, provider_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                description = excluded.description,
                trigger_hint = excluded.trigger_hint,
                frequency = excluded.frequency,
                confidence = excluded.confidence,
                last_seen_at = excluded.last_seen_at,
                workspace_key = excluded.workspace_key,
                provider_id = excluded.provider_id",
            params![
                pattern.id,
                pattern_type_json,
                pattern.description,
                pattern.trigger_hint,
                pattern.frequency as i64,
                pattern.confidence as i64,
                pattern.last_seen_at.to_rfc3339(),
                pattern.created_at.to_rfc3339(),
                pattern.workspace_key,
                pattern.provider_id,
            ],
        )?;
        Ok(())
    }

    pub fn list_patterns(
        &self,
        limit: usize,
        workspace_key: Option<&str>,
    ) -> Result<Vec<UsagePattern>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, pattern_type, description, trigger_hint, frequency, confidence,
                    last_seen_at, created_at, workspace_key, provider_id
             FROM usage_patterns
             WHERE (?2 IS NULL OR workspace_key = ?2 OR workspace_key IS NULL)
             ORDER BY frequency DESC, last_seen_at DESC
             LIMIT ?1",
        )?;
        let rows = statement
            .query_map(params![limit as i64, workspace_key], row_to_pattern)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn find_pattern_by_description(
        &self,
        description: &str,
        workspace_key: Option<&str>,
    ) -> Result<Option<UsagePattern>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, pattern_type, description, trigger_hint, frequency, confidence,
                    last_seen_at, created_at, workspace_key, provider_id
             FROM usage_patterns
             WHERE description = ?1
               AND (?2 IS NULL OR workspace_key = ?2 OR workspace_key IS NULL)
             LIMIT 1",
        )?;
        statement
            .query_row(params![description, workspace_key], row_to_pattern)
            .optional()
            .map_err(Into::into)
    }

    pub fn increment_pattern_frequency(&self, id: &str) -> Result<bool> {
        let connection = self.connection()?;
        let updated = connection.execute(
            "UPDATE usage_patterns SET frequency = frequency + 1, last_seen_at = ?2 WHERE id = ?1",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(updated > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{
        ConnectorApprovalRecord, ConnectorApprovalStatus, ConnectorKind, MemoryEvidenceRef,
        MemoryKind, MemoryReviewStatus, MemoryScope, MessageRole, Mission, MissionStatus,
        SkillDraftStatus, TaskMode, ToolCall,
    };

    fn temp_storage() -> Storage {
        let root = std::env::temp_dir().join(format!("agent-storage-test-{}", Uuid::new_v4()));
        Storage::open_at(root).unwrap()
    }

    #[test]
    fn persist_session_turn_round_trips_tool_metadata() {
        let storage = temp_storage();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        let tool_call = ToolCall {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            arguments: "{\"path\":\"README.md\"}".to_string(),
        };
        let messages = vec![
            SessionMessage::new(
                "session-1".to_string(),
                MessageRole::User,
                "inspect the file".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ),
            SessionMessage::new(
                "session-1".to_string(),
                MessageRole::Assistant,
                String::new(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            )
            .with_tool_calls(vec![tool_call.clone()])
            .with_provider_payload(Some("{\"id\":\"resp-1\"}".to_string())),
            SessionMessage::new(
                "session-1".to_string(),
                MessageRole::Tool,
                "file contents".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            )
            .with_tool_metadata(Some("call-1".to_string()), Some("read_file".to_string())),
            SessionMessage::new(
                "session-1".to_string(),
                MessageRole::Assistant,
                "done".to_string(),
                Some("openai".to_string()),
                Some("gpt-4.1".to_string()),
            ),
        ];

        storage
            .persist_session_turn(PersistSessionTurnInput {
                session_id: "session-1",
                title: Some("Test Session"),
                alias: &alias,
                provider_id: "openai",
                model: "gpt-4.1",
                task_mode: Some(TaskMode::Build),
                cwd: None,
                messages: &messages,
            })
            .unwrap();

        let persisted = storage.list_session_messages("session-1").unwrap();
        assert_eq!(persisted.len(), 4);
        assert_eq!(persisted[1].tool_calls, vec![tool_call]);
        assert_eq!(
            persisted[1].provider_payload_json.as_deref(),
            Some("{\"id\":\"resp-1\"}")
        );
        assert_eq!(persisted[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(persisted[2].tool_name.as_deref(), Some("read_file"));

        let session = storage.get_session("session-1").unwrap().unwrap();
        assert_eq!(session.title.as_deref(), Some("Test Session"));
        assert_eq!(session.alias, "main");
        assert_eq!(session.task_mode, Some(TaskMode::Build));
    }

    #[test]
    fn persist_session_turn_preserves_existing_task_mode_when_unspecified() {
        let storage = temp_storage();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        storage
            .ensure_session(
                "session-1",
                &alias,
                "openai",
                "gpt-4.1",
                Some(TaskMode::Daily),
            )
            .unwrap();
        let messages = vec![SessionMessage::new(
            "session-1".to_string(),
            MessageRole::User,
            "continue".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )];

        storage
            .persist_session_turn(PersistSessionTurnInput {
                session_id: "session-1",
                title: None,
                alias: &alias,
                provider_id: "openai",
                model: "gpt-4.1",
                task_mode: None,
                cwd: None,
                messages: &messages,
            })
            .unwrap();

        let session = storage.get_session("session-1").unwrap().unwrap();
        assert_eq!(session.task_mode, Some(TaskMode::Daily));
    }

    #[test]
    fn persist_session_turn_updates_existing_task_mode_when_explicit() {
        let storage = temp_storage();
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        storage
            .ensure_session(
                "session-1",
                &alias,
                "openai",
                "gpt-4.1",
                Some(TaskMode::Daily),
            )
            .unwrap();
        let messages = vec![SessionMessage::new(
            "session-1".to_string(),
            MessageRole::Assistant,
            "switched".to_string(),
            Some("openai".to_string()),
            Some("gpt-4.1".to_string()),
        )];

        storage
            .persist_session_turn(PersistSessionTurnInput {
                session_id: "session-1",
                title: None,
                alias: &alias,
                provider_id: "openai",
                model: "gpt-4.1",
                task_mode: Some(TaskMode::Build),
                cwd: None,
                messages: &messages,
            })
            .unwrap();

        let session = storage.get_session("session-1").unwrap().unwrap();
        assert_eq!(session.task_mode, Some(TaskMode::Build));
    }

    #[test]
    fn reset_all_recreates_default_config_and_empty_database() {
        let storage = temp_storage();
        let config = AppConfig {
            onboarding_complete: true,
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();
        storage
            .append_log(&LogEntry {
                id: "log-1".to_string(),
                level: "info".to_string(),
                scope: "test".to_string(),
                message: "hello".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();

        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            description: None,
        };
        storage
            .ensure_session("session-1", &alias, "openai", "gpt-4.1", None)
            .unwrap();

        storage.reset_all().unwrap();

        let reset = storage.load_config().unwrap();
        let expected = AppConfig::default();
        assert_eq!(reset.version, expected.version);
        assert_eq!(reset.daemon.host, expected.daemon.host);
        assert_eq!(reset.daemon.port, expected.daemon.port);
        assert!(!reset.daemon.token.is_empty());
        assert_ne!(reset.daemon.token, config.daemon.token);
        assert_eq!(
            reset.daemon.persistence_mode,
            expected.daemon.persistence_mode
        );
        assert_eq!(reset.daemon.auto_start, expected.daemon.auto_start);
        assert_eq!(reset.main_agent_alias, expected.main_agent_alias);
        assert!(reset.providers.is_empty());
        assert!(reset.aliases.is_empty());
        assert_eq!(reset.thinking_level, expected.thinking_level);
        assert_eq!(reset.permission_preset, expected.permission_preset);
        assert_eq!(reset.trust_policy, expected.trust_policy);
        assert_eq!(reset.autonomy, expected.autonomy);
        assert!(reset.mcp_servers.is_empty());
        assert!(reset.app_connectors.is_empty());
        assert!(reset.enabled_skills.is_empty());
        assert!(!reset.onboarding_complete);
        assert!(storage.list_sessions(10).unwrap().is_empty());
        assert!(storage.list_logs(10).unwrap().is_empty());
        assert!(storage.paths().config_path.exists());
        assert!(storage.paths().db_path.exists());
    }

    #[test]
    fn list_logs_after_returns_chronological_results_from_cursor() {
        let storage = temp_storage();
        let first = LogEntry {
            id: "log-1".to_string(),
            level: "info".to_string(),
            scope: "test".to_string(),
            message: "first".to_string(),
            created_at: Utc::now(),
        };
        let second = LogEntry {
            id: "log-2".to_string(),
            level: "info".to_string(),
            scope: "test".to_string(),
            message: "second".to_string(),
            created_at: first.created_at + chrono::Duration::seconds(1),
        };
        let third = LogEntry {
            id: "log-3".to_string(),
            level: "warn".to_string(),
            scope: "test".to_string(),
            message: "third".to_string(),
            created_at: second.created_at + chrono::Duration::seconds(1),
        };

        storage.append_log(&first).unwrap();
        storage.append_log(&second).unwrap();
        storage.append_log(&third).unwrap();

        let logs = storage.list_logs_after(second.created_at, 10).unwrap();
        let ids = logs.into_iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["log-2".to_string(), "log-3".to_string()]);
    }

    #[test]
    fn list_logs_after_cursor_skips_duplicate_entry_and_keeps_same_timestamp_peers() {
        let storage = temp_storage();
        let created_at = Utc::now();
        let first = LogEntry {
            id: "log-a".to_string(),
            level: "info".to_string(),
            scope: "test".to_string(),
            message: "first".to_string(),
            created_at,
        };
        let second = LogEntry {
            id: "log-b".to_string(),
            level: "info".to_string(),
            scope: "test".to_string(),
            message: "second".to_string(),
            created_at,
        };
        let third = LogEntry {
            id: "log-c".to_string(),
            level: "warn".to_string(),
            scope: "test".to_string(),
            message: "third".to_string(),
            created_at: created_at + chrono::Duration::seconds(1),
        };

        storage.append_log(&first).unwrap();
        storage.append_log(&second).unwrap();
        storage.append_log(&third).unwrap();

        let logs = storage
            .list_logs_after_cursor(created_at, Some("log-a"), 10)
            .unwrap();
        let ids = logs.into_iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["log-b".to_string(), "log-c".to_string()]);
    }

    #[test]
    fn mission_counts_and_limited_listing_round_trip() {
        let storage = temp_storage();

        let mut queued = Mission::new("Queued".to_string(), "Pending".to_string());
        queued.status = MissionStatus::Queued;
        queued.updated_at = Utc::now();
        storage.upsert_mission(&queued).unwrap();

        let mut waiting = Mission::new("Waiting".to_string(), "Sleeping".to_string());
        waiting.status = MissionStatus::Waiting;
        waiting.updated_at = queued.updated_at + chrono::Duration::seconds(1);
        storage.upsert_mission(&waiting).unwrap();

        let mut completed = Mission::new("Completed".to_string(), "Done".to_string());
        completed.status = MissionStatus::Completed;
        completed.updated_at = waiting.updated_at + chrono::Duration::seconds(1);
        storage.upsert_mission(&completed).unwrap();

        assert_eq!(storage.count_missions().unwrap(), 3);
        assert_eq!(storage.count_active_missions().unwrap(), 2);

        let limited = storage.list_missions_limited(Some(2)).unwrap();
        let ids = limited
            .into_iter()
            .map(|mission| mission.id)
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&completed.id));
        assert!(ids.contains(&waiting.id));
        assert!(!ids.contains(&queued.id));
    }

    #[test]
    fn blocked_missions_are_not_runnable() {
        let storage = temp_storage();
        let now = Utc::now();

        let mut blocked = Mission::new("Blocked".to_string(), "Paused".to_string());
        blocked.status = MissionStatus::Blocked;
        blocked.updated_at = now;
        storage.upsert_mission(&blocked).unwrap();

        let mut queued = Mission::new("Queued".to_string(), "Ready".to_string());
        queued.status = MissionStatus::Queued;
        queued.updated_at = now + chrono::Duration::seconds(1);
        storage.upsert_mission(&queued).unwrap();

        let runnable = storage.list_runnable_missions(now, 10).unwrap();
        let ids = runnable
            .into_iter()
            .map(|mission| mission.id)
            .collect::<Vec<_>>();
        assert!(ids.contains(&queued.id));
        assert!(!ids.contains(&blocked.id));
    }

    #[test]
    fn skill_draft_round_trips_and_filters_by_status() {
        let storage = temp_storage();
        let mut draft = SkillDraft::new(
            "Review auth workflow".to_string(),
            "Observed reusable auth workflow.".to_string(),
            "1. Read files\n2. Run tests".to_string(),
        );
        draft.workspace_key = Some("J:/repo".to_string());
        draft.provider_id = Some("openai".to_string());
        draft.status = SkillDraftStatus::Published;
        storage.upsert_skill_draft(&draft).unwrap();

        let stored = storage.get_skill_draft(&draft.id).unwrap().unwrap();
        assert_eq!(stored.title, draft.title);
        assert_eq!(stored.status, SkillDraftStatus::Published);

        let published = storage
            .list_skill_drafts(10, Some(SkillDraftStatus::Published), None, None)
            .unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].id, draft.id);
    }

    #[test]
    fn touch_skill_draft_updates_last_used_without_mutating_updated_at() {
        let storage = temp_storage();
        let draft = SkillDraft::new(
            "Review auth workflow".to_string(),
            "Observed reusable auth workflow.".to_string(),
            "1. Read files\n2. Run tests".to_string(),
        );
        storage.upsert_skill_draft(&draft).unwrap();

        let before = storage.get_skill_draft(&draft.id).unwrap().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        storage.touch_skill_draft(&draft.id).unwrap();
        let after = storage.get_skill_draft(&draft.id).unwrap().unwrap();

        assert_eq!(after.updated_at, before.updated_at);
        assert!(after.last_used_at.is_some());
        assert!(after.last_used_at >= before.last_used_at);
    }

    #[test]
    fn candidate_memories_are_excluded_from_active_retrieval() {
        let storage = temp_storage();

        let accepted = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:theme".to_string(),
            "User prefers concise output.".to_string(),
        );
        storage.upsert_memory(&accepted).unwrap();

        let mut candidate = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Workspace,
            "workspace:stack".to_string(),
            "Project might use Rust and Tauri.".to_string(),
        );
        candidate.review_status = MemoryReviewStatus::Candidate;
        storage.upsert_memory(&candidate).unwrap();

        let active = storage.list_memories(10).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, accepted.id);

        let review_queue = storage
            .list_memories_by_review_status(MemoryReviewStatus::Candidate, 10)
            .unwrap();
        assert_eq!(review_queue.len(), 1);
        assert_eq!(review_queue[0].id, candidate.id);
    }

    #[test]
    fn updating_memory_review_status_sets_review_metadata() {
        let storage = temp_storage();
        let mut memory = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "memory:test".to_string(),
            "Candidate memory".to_string(),
        );
        memory.review_status = MemoryReviewStatus::Candidate;
        storage.upsert_memory(&memory).unwrap();

        let updated = storage
            .update_memory_review_status(
                &memory.id,
                MemoryReviewStatus::Accepted,
                Some("validated"),
            )
            .unwrap();
        assert!(updated);

        let stored = storage.get_memory(&memory.id).unwrap().unwrap();
        assert_eq!(stored.review_status, MemoryReviewStatus::Accepted);
        assert_eq!(stored.review_note.as_deref(), Some("validated"));
        assert!(stored.reviewed_at.is_some());
    }

    #[test]
    fn touch_memory_updates_last_used_without_mutating_updated_at() {
        let storage = temp_storage();
        let memory = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output.".to_string(),
        );
        storage.upsert_memory(&memory).unwrap();

        let before = storage.get_memory(&memory.id).unwrap().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        storage.touch_memory(&memory.id).unwrap();
        let after = storage.get_memory(&memory.id).unwrap().unwrap();

        assert_eq!(after.updated_at, before.updated_at);
        assert!(after.last_used_at.is_some());
        assert!(after.last_used_at >= before.last_used_at);
    }

    #[test]
    fn memory_evidence_refs_round_trip_through_storage() {
        let storage = temp_storage();
        let mut memory = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output.".to_string(),
        );
        memory.source_session_id = Some("session-1".to_string());
        memory.evidence_refs = vec![MemoryEvidenceRef {
            session_id: "session-1".to_string(),
            message_id: Some("message-1".to_string()),
            role: Some(MessageRole::User),
            tool_call_id: None,
            tool_name: None,
            created_at: Utc::now(),
        }];
        storage.upsert_memory(&memory).unwrap();

        let stored = storage.get_memory(&memory.id).unwrap().unwrap();
        assert_eq!(stored.evidence_refs, memory.evidence_refs);

        let from_session = storage
            .list_memories_by_source_session("session-1", 10)
            .unwrap();
        assert_eq!(from_session.len(), 1);
        assert_eq!(from_session[0].evidence_refs, memory.evidence_refs);

        let (searched, transcript_hits) = storage
            .search_memories("concise output", None, None, &[], false, 10)
            .unwrap();
        assert!(transcript_hits.is_empty());
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].evidence_refs, memory.evidence_refs);
    }

    #[test]
    fn list_active_memories_by_identity_key_prefers_accepted_and_skips_superseded() {
        let storage = temp_storage();
        let identity_key = "preference:global:output";

        let mut accepted = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers concise output.".to_string(),
        );
        accepted.identity_key = Some(identity_key.to_string());
        accepted.updated_at = Utc::now();
        storage.upsert_memory(&accepted).unwrap();

        let mut candidate = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers detailed output.".to_string(),
        );
        candidate.identity_key = Some(identity_key.to_string());
        candidate.review_status = MemoryReviewStatus::Candidate;
        candidate.updated_at = accepted.updated_at + chrono::Duration::seconds(1);
        storage.upsert_memory(&candidate).unwrap();

        let mut superseded = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:output".to_string(),
            "User prefers terse output.".to_string(),
        );
        superseded.identity_key = Some(identity_key.to_string());
        superseded.review_status = MemoryReviewStatus::Candidate;
        superseded.superseded_by = Some(candidate.id.clone());
        superseded.updated_at = candidate.updated_at + chrono::Duration::seconds(1);
        storage.upsert_memory(&superseded).unwrap();

        let listed = storage
            .list_active_memories_by_identity_key(identity_key, None, None)
            .unwrap();
        let ids = listed
            .into_iter()
            .map(|memory| memory.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![accepted.id, candidate.id]);
    }

    #[test]
    fn find_memory_by_identity_key_prefers_accepted_memory() {
        let storage = temp_storage();
        let identity_key = "preference:global:verbosity";

        let mut accepted = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers concise output.".to_string(),
        );
        accepted.identity_key = Some(identity_key.to_string());
        accepted.updated_at = Utc::now();
        storage.upsert_memory(&accepted).unwrap();

        let mut newer_candidate = MemoryRecord::new(
            MemoryKind::Preference,
            MemoryScope::Global,
            "preference:verbosity".to_string(),
            "User prefers very detailed output.".to_string(),
        );
        newer_candidate.identity_key = Some(identity_key.to_string());
        newer_candidate.review_status = MemoryReviewStatus::Candidate;
        newer_candidate.updated_at = accepted.updated_at + chrono::Duration::seconds(1);
        storage.upsert_memory(&newer_candidate).unwrap();

        let found = storage
            .find_memory_by_identity_key(identity_key, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(found.id, accepted.id);
        assert_eq!(found.review_status, MemoryReviewStatus::Accepted);
    }

    #[test]
    fn normalize_fts_query_strips_question_mark_safely() {
        assert_eq!(
            normalize_fts_query("can you check the weather in chicago?"),
            "can you check the weather in chicago"
        );
    }

    #[test]
    fn normalize_fts_query_splits_period_delimited_tokens_safely() {
        assert_eq!(
            normalize_fts_query("status for api.openai.com."),
            "status for api openai com"
        );
    }

    #[test]
    fn normalize_fts_query_strips_operator_like_symbols_safely() {
        assert_eq!(
            normalize_fts_query("gpt-5_status -- ??? !!!"),
            "gpt 5 status"
        );
    }

    #[test]
    fn normalize_fts_query_returns_empty_for_symbol_only_input() {
        assert_eq!(normalize_fts_query("!@#$%^&*()_-+=[]{}|;:'\",.<>/?`~"), "");
    }

    #[test]
    fn search_memories_accepts_period_and_question_mark_queries() {
        let storage = temp_storage();
        let memory = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "weather:chicago".to_string(),
            "Check the weather in chicago with api.openai.com before replying.".to_string(),
        );
        storage.upsert_memory(&memory).unwrap();

        let (memories, transcript_hits) = storage
            .search_memories(
                "weather in chicago. api.openai.com?",
                None,
                None,
                &[],
                false,
                10,
            )
            .unwrap();

        assert_eq!(transcript_hits.len(), 0);
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].id, memory.id);
    }

    #[test]
    fn search_memories_accepts_symbol_only_queries_without_error() {
        let storage = temp_storage();
        let memory = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "symbols:test".to_string(),
            "symbol stress note".to_string(),
        );
        storage.upsert_memory(&memory).unwrap();

        let (memories, transcript_hits) = storage
            .search_memories(
                "!@#$%^&*()_-+=[]{}|;:'\",.<>/?`~",
                None,
                None,
                &[],
                false,
                10,
            )
            .unwrap();

        assert!(memories.is_empty());
        assert!(transcript_hits.is_empty());
    }

    #[test]
    fn search_memories_honors_review_status_and_superseded_filters() {
        let storage = temp_storage();

        let accepted = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "memory:accepted".to_string(),
            "alpha accepted memory".to_string(),
        );
        storage.upsert_memory(&accepted).unwrap();

        let mut candidate = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "memory:candidate".to_string(),
            "alpha candidate memory".to_string(),
        );
        candidate.review_status = MemoryReviewStatus::Candidate;
        storage.upsert_memory(&candidate).unwrap();

        let mut superseded = MemoryRecord::new(
            MemoryKind::Note,
            MemoryScope::Global,
            "memory:superseded".to_string(),
            "alpha superseded memory".to_string(),
        );
        superseded.superseded_by = Some(accepted.id.clone());
        storage.upsert_memory(&superseded).unwrap();

        let (default_memories, _) = storage
            .search_memories("alpha", None, None, &[], false, 10)
            .unwrap();
        let default_ids = default_memories
            .into_iter()
            .map(|memory| memory.id)
            .collect::<Vec<_>>();
        assert_eq!(default_ids, vec![accepted.id.clone()]);

        let (candidate_memories, _) = storage
            .search_memories(
                "alpha",
                None,
                None,
                &[MemoryReviewStatus::Candidate],
                false,
                10,
            )
            .unwrap();
        let candidate_ids = candidate_memories
            .into_iter()
            .map(|memory| memory.id)
            .collect::<Vec<_>>();
        assert_eq!(candidate_ids, vec![candidate.id.clone()]);

        let (all_memories, _) = storage
            .search_memories(
                "alpha",
                None,
                None,
                &[MemoryReviewStatus::Accepted, MemoryReviewStatus::Candidate],
                true,
                10,
            )
            .unwrap();
        let all_ids = all_memories
            .into_iter()
            .map(|memory| memory.id)
            .collect::<Vec<_>>();
        assert!(all_ids.contains(&accepted.id));
        assert!(all_ids.contains(&candidate.id));
        assert!(all_ids.contains(&superseded.id));
    }

    #[test]
    fn connector_approvals_round_trip_and_count_pending() {
        let storage = temp_storage();
        let mut approval = ConnectorApprovalRecord::new(
            ConnectorKind::Telegram,
            "ops".to_string(),
            "Ops Bot".to_string(),
            "Ops telegram: hello".to_string(),
            "Telegram connector: Ops Bot".to_string(),
            "telegram:ops:chat:42:user:any".to_string(),
        );
        approval.external_chat_id = Some("42".to_string());
        approval.external_user_id = Some("7".to_string());
        approval.message_preview = Some("hello".to_string());
        storage.upsert_connector_approval(&approval).unwrap();

        assert_eq!(storage.count_pending_connector_approvals().unwrap(), 1);
        let listed = storage
            .list_connector_approvals(
                Some(ConnectorKind::Telegram),
                Some(ConnectorApprovalStatus::Pending),
                10,
            )
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].external_chat_id.as_deref(), Some("42"));

        let updated = storage
            .update_connector_approval_status(
                &approval.id,
                ConnectorApprovalStatus::Approved,
                Some("host approved"),
                Some("telegram:ops:10"),
            )
            .unwrap();
        assert!(updated);

        let stored = storage
            .get_connector_approval(&approval.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, ConnectorApprovalStatus::Approved);
        assert_eq!(stored.review_note.as_deref(), Some("host approved"));
        assert_eq!(stored.queued_mission_id.as_deref(), Some("telegram:ops:10"));
        assert_eq!(storage.count_pending_connector_approvals().unwrap(), 0);
    }
}
