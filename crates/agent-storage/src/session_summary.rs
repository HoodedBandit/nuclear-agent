use super::*;

pub(super) fn row_to_session_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
    let created_at: String = row.get(7)?;
    let updated_at: String = row.get(8)?;
    let task_mode: Option<String> = row.get(5)?;
    let cwd: Option<String> = row.get(6)?;
    Ok(SessionSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        alias: row.get(2)?,
        provider_id: row.get(3)?,
        model: row.get(4)?,
        task_mode: parse_task_mode_column(task_mode, 5)?,
        message_count: row.get(9)?,
        cwd: cwd.map(PathBuf::from),
        created_at: parse_datetime(&created_at)?,
        updated_at: parse_datetime(&updated_at)?,
    })
}

pub(super) fn parse_task_mode_column(
    value: Option<String>,
    column_index: usize,
) -> rusqlite::Result<Option<TaskMode>> {
    value
        .as_deref()
        .map(|mode| match mode {
            "build" => Ok(TaskMode::Build),
            "daily" => Ok(TaskMode::Daily),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column_index,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid task mode '{other}'"),
                )),
            )),
        })
        .transpose()
}
