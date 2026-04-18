use super::*;

impl Storage {
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
        missions.sort_by_key(|mission| mission.updated_at);
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
}
