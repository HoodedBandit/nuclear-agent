use std::path::Path;

use crate::{ModelAlias, SessionMessage, TaskMode};

pub struct PersistSessionTurnInput<'a> {
    pub session_id: &'a str,
    pub title: Option<&'a str>,
    pub alias: &'a ModelAlias,
    pub provider_id: &'a str,
    pub model: &'a str,
    pub task_mode: Option<TaskMode>,
    pub cwd: Option<&'a Path>,
    pub messages: &'a [SessionMessage],
}
