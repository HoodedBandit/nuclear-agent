use super::*;

impl Storage {
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
}
