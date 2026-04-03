use super::*;

impl Storage {
    pub fn append_log(&self, entry: &LogEntry) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO logs (id, level, scope, message, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.id,
                entry.level,
                entry.scope,
                entry.message,
                entry.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_logs(&self, limit: usize) -> Result<Vec<LogEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, level, scope, message, created_at
             FROM logs
             ORDER BY created_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            let created_at: String = row.get(4)?;
            Ok(LogEntry {
                id: row.get(0)?,
                level: row.get(1)?,
                scope: row.get(2)?,
                message: row.get(3)?,
                created_at: parse_datetime(&created_at)?,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_logs_after_cursor(
        &self,
        after: DateTime<Utc>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "
            SELECT id, level, scope, message, created_at
            FROM logs
            WHERE created_at > ?1
               OR (?2 IS NULL AND created_at >= ?1)
               OR (?2 IS NOT NULL AND created_at = ?1 AND id > ?2)
            ORDER BY created_at ASC, id ASC
            LIMIT ?3
            ",
        )?;
        let rows =
            statement.query_map(params![after.to_rfc3339(), after_id, limit as i64], |row| {
                let created_at: String = row.get(4)?;
                Ok(LogEntry {
                    id: row.get(0)?,
                    level: row.get(1)?,
                    scope: row.get(2)?,
                    message: row.get(3)?,
                    created_at: parse_datetime(&created_at)?,
                })
            })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_logs_after(&self, after: DateTime<Utc>, limit: usize) -> Result<Vec<LogEntry>> {
        self.list_logs_after_cursor(after, None, limit)
    }
}
