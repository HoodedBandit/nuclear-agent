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
mod memory;
mod migrations;
mod missions;
mod paths;
mod patterns;
pub mod plugins;
mod session_input;
mod session_summary;
mod skills;
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
        migrations::apply_schema_migrations(&connection)
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
}

#[cfg(test)]
mod tests;
