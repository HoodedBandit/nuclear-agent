use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceInspectRequest {
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceInspectResponse {
    pub requested_path: String,
    pub workspace_root: String,
    #[serde(default)]
    pub git_root: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_commit: Option<String>,
    #[serde(default)]
    pub staged_files: usize,
    #[serde(default)]
    pub dirty_files: usize,
    #[serde(default)]
    pub untracked_files: usize,
    #[serde(default)]
    pub manifests: Vec<String>,
    #[serde(default)]
    pub focus_paths: Vec<WorkspacePathStat>,
    #[serde(default)]
    pub language_breakdown: Vec<WorkspaceLanguageStat>,
    #[serde(default)]
    pub large_source_files: Vec<WorkspaceFileStat>,
    #[serde(default)]
    pub recent_commits: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspacePathStat {
    pub path: String,
    pub source_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceLanguageStat {
    pub label: String,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkspaceFileStat {
    pub path: String,
    pub lines: usize,
}
