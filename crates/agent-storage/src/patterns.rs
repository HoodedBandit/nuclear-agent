use super::*;

impl Storage {
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
