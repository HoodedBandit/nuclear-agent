use super::*;

impl Storage {
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
}
