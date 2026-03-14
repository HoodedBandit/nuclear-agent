use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use agent_core::{
    AppConnectorConfig, AutonomyProfile, BatchTaskRequest, BraveConnectorConfig,
    ConversationMessage, DelegationConfig, DelegationTarget, DiscordConnectorConfig,
    DiscordSendRequest, HomeAssistantConnectorConfig, HomeAssistantServiceCallRequest,
    McpServerConfig, MessageRole, PermissionPreset, SignalConnectorConfig, SignalSendRequest,
    SlackConnectorConfig, SlackSendRequest, SubAgentStrategy, TelegramConnectorConfig,
    TelegramSendRequest, ThinkingLevel, ToolCall, ToolDefinition, ToolExecutionOutcome,
    ToolExecutionRecord, TrustPolicy,
};
use agent_policy::{allow_network, allow_self_edit, allow_shell, path_is_trusted};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::{process::Command, time::timeout};

use crate::{
    append_log, execute_batch_request,
    patch::{apply_hunks_to_text, parse_patch_text, PatchOperation},
    AppState, DelegationExecutionOptions,
};
use agent_providers::store_api_key;

mod admin_helpers;
mod argument_helpers;
mod connector_tools;
mod delegation_tools;
mod dynamic_tools;
mod filesystem_tools;
mod path_helpers;
mod process_tools;

use admin_helpers::ensure_connector_admin_allowed;
use argument_helpers::{parse_arguments, truncate};
use delegation_tools::{spawn_subagents, spawn_subagents_description};
use dynamic_tools::{dynamic_tool_definition, execute_dynamic_tool};
use filesystem_tools::{
    append_file, apply_patch_tool, copy_path, delete_path, find_files, list_dir, make_dir,
    move_path, read_file, replace_in_file, search_files, stat_path, write_file,
};
use process_tools::{
    fetch_url, git_diff, git_log, git_show, git_status, http_request, read_env, run_shell,
};

const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 60;
const MAX_SHELL_TIMEOUT_SECS: u64 = 300;
const DEFAULT_GIT_TIMEOUT_SECS: u64 = 15;
const MAX_FETCH_BYTES: usize = 20_000;
const MAX_HTTP_BODY_BYTES: usize = 20_000;
const MAX_COMMAND_OUTPUT_BYTES: usize = 20_000;
const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_FILE_BYTES: usize = 512_000;
const MAX_DIRECTORY_ENTRIES: usize = 200;
const MAX_FIND_RESULTS: usize = 200;
const MAX_GIT_LOG_ENTRIES: usize = 50;
const SIGNAL_CLI_TIMEOUT_SECS: u64 = 15;
const REDACTED_SECRET_ARGUMENT: &str = "[REDACTED]";

#[derive(Clone)]
pub(crate) struct ToolContext {
    pub state: AppState,
    pub cwd: PathBuf,
    pub trust_policy: TrustPolicy,
    pub autonomy: AutonomyProfile,
    pub permission_preset: PermissionPreset,
    pub http_client: Client,
    pub mcp_servers: Vec<McpServerConfig>,
    pub app_connectors: Vec<AppConnectorConfig>,
    pub brave_connectors: Vec<BraveConnectorConfig>,
    pub current_alias: Option<String>,
    pub default_thinking_level: Option<ThinkingLevel>,
    pub delegation: DelegationConfig,
    pub delegation_targets: Vec<DelegationTarget>,
    pub delegation_depth: u8,
    pub background: bool,
    pub background_shell_allowed: bool,
    pub background_network_allowed: bool,
    pub background_self_edit_allowed: bool,
}

pub(crate) fn tool_definitions(context: &ToolContext) -> Vec<ToolDefinition> {
    let mut tools = vec![
        tool(
            "pwd",
            "Return the current working directory for this task.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ),
        tool(
            "list_dir",
            "List files and directories at a path.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "max_entries": {"type": "integer", "minimum": 1, "maximum": MAX_DIRECTORY_ENTRIES}
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "read_file",
            "Read a UTF-8 text file, optionally by line range.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "start_line": {"type": "integer", "minimum": 1},
                    "end_line": {"type": "integer", "minimum": 1}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        tool(
            "apply_patch",
            "Apply a structured patch using the Codex-style patch format with *** Begin Patch / *** End Patch markers.",
            json!({
                "type": "object",
                "properties": {
                    "patch": {"type": "string"}
                },
                "required": ["patch"],
                "additionalProperties": false
            }),
        ),
        tool(
            "write_file",
            "Create or overwrite a text file.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        ),
        tool(
            "append_file",
            "Append text to an existing file or create it if missing.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        ),
        tool(
            "replace_in_file",
            "Replace exact text inside a file.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "old": {"type": "string"},
                    "new": {"type": "string"},
                    "replace_all": {"type": "boolean"}
                },
                "required": ["path", "old", "new"],
                "additionalProperties": false
            }),
        ),
        tool(
            "search_files",
            "Search text recursively under a directory.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "query": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": MAX_SEARCH_RESULTS}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        tool(
            "find_files",
            "Find files or directories recursively by wildcard pattern or substring.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "pattern": {"type": "string"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": MAX_FIND_RESULTS}
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        ),
        tool(
            "make_dir",
            "Create a directory and any missing parents.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        tool(
            "copy_path",
            "Copy a file or directory to a new path.",
            json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "destination": {"type": "string"},
                    "overwrite": {"type": "boolean"}
                },
                "required": ["source", "destination"],
                "additionalProperties": false
            }),
        ),
        tool(
            "move_path",
            "Move or rename a file or directory.",
            json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "destination": {"type": "string"},
                    "overwrite": {"type": "boolean"}
                },
                "required": ["source", "destination"],
                "additionalProperties": false
            }),
        ),
        tool(
            "delete_path",
            "Delete a file or directory.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "recursive": {"type": "boolean"}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        tool(
            "stat_path",
            "Return metadata about a file or directory.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        tool(
            "run_shell",
            "Execute a shell command in the workspace and capture stdout/stderr.",
            json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"},
                    "workdir": {"type": "string"},
                    "timeout_seconds": {"type": "integer", "minimum": 1, "maximum": MAX_SHELL_TIMEOUT_SECS}
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        ),
        tool(
            "read_env",
            "Read a non-secret environment variable from the local process environment.",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"],
                "additionalProperties": false
            }),
        ),
        tool(
            "git_status",
            "Show git status for a repository.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "git_diff",
            "Show git diff output for the current repository.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "staged": {"type": "boolean"},
                    "revision": {"type": "string"}
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "git_log",
            "Show recent git commits for a repository.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": MAX_GIT_LOG_ENTRIES}
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "git_show",
            "Show a git revision or object.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "revision": {"type": "string"}
                },
                "required": ["revision"],
                "additionalProperties": false
            }),
        ),
        tool(
            "fetch_url",
            "Fetch a URL over HTTP GET and return the response body.",
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        ),
        tool(
            "http_request",
            "Make an HTTP request with optional method, headers, and body.",
            json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"},
                    "method": {"type": "string"},
                    "headers": {"type": "object", "additionalProperties": {"type": "string"}},
                    "body": {"type": "string"},
                    "max_bytes": {"type": "integer", "minimum": 1, "maximum": MAX_HTTP_BODY_BYTES}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        ),
        tool(
            "spawn_subagents",
            &spawn_subagents_description(context),
            json!({
                "type": "object",
                "properties": {
                    "tasks": {
                        "type": "array",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "prompt": {"type": "string"},
                                "target": {"type": "string"},
                                "alias": {"type": "string"},
                                "provider_id": {"type": "string"},
                                "requested_model": {"type": "string"},
                                "cwd": {"type": "string"},
                                "thinking_level": {
                                    "type": "string",
                                    "enum": ["none", "minimal", "low", "medium", "high", "xhigh"]
                                },
                                "output_schema_json": {"type": "string"},
                                "strategy": {
                                    "type": "string",
                                    "enum": ["single_best", "parallel_best_effort", "parallel_all"]
                                }
                            },
                            "required": ["prompt"],
                            "additionalProperties": false
                        }
                    },
                    "cwd": {"type": "string"},
                    "thinking_level": {
                        "type": "string",
                        "enum": ["none", "minimal", "low", "medium", "high", "xhigh"]
                    },
                    "strategy": {
                        "type": "string",
                        "enum": ["single_best", "parallel_best_effort", "parallel_all"]
                    }
                },
                "required": ["tasks"],
                "additionalProperties": false
            }),
        ),
    ];
    tools.extend(connector_tools::tool_definitions(context));

    if matches!(context.permission_preset, PermissionPreset::FullAuto)
        && context.background_shell_allowed
        && allow_shell(&context.trust_policy, &context.autonomy)
    {
        for server in &context.mcp_servers {
            if let Some(tool) = dynamic_tool_definition(
                &server.tool_name,
                &server.description,
                &server.input_schema_json,
            ) {
                tools.push(tool);
            }
        }
        for connector in &context.app_connectors {
            if let Some(tool) = dynamic_tool_definition(
                &connector.tool_name,
                &connector.description,
                &connector.input_schema_json,
            ) {
                tools.push(tool);
            }
        }
    }

    tools
}

pub(crate) struct ToolCallExecution {
    pub message: ConversationMessage,
    pub record: ToolExecutionRecord,
}

pub(crate) async fn execute_tool_call(context: &ToolContext, call: &ToolCall) -> ToolCallExecution {
    let (content, outcome) = match execute_tool_call_inner(context, call).await {
        Ok(output) => (output, ToolExecutionOutcome::Success),
        Err(error) => (format!("ERROR: {error:#}"), ToolExecutionOutcome::Error),
    };
    let sanitized_arguments = sanitize_tool_arguments(&call.name, &call.arguments);

    ToolCallExecution {
        message: ConversationMessage {
            role: MessageRole::Tool,
            content: content.clone(),
            tool_call_id: Some(call.id.clone()),
            tool_name: Some(call.name.clone()),
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
        },
        record: ToolExecutionRecord {
            call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: sanitized_arguments,
            outcome,
            output: content,
        },
    }
}

pub(crate) fn sanitize_tool_call(call: &ToolCall) -> ToolCall {
    ToolCall {
        id: call.id.clone(),
        name: call.name.clone(),
        arguments: sanitize_tool_arguments(&call.name, &call.arguments),
    }
}

pub(crate) fn tool_call_has_sensitive_arguments(call: &ToolCall) -> bool {
    !sensitive_tool_argument_fields(&call.name).is_empty()
}

pub(crate) fn sanitize_tool_arguments(tool_name: &str, arguments: &str) -> String {
    let sensitive_fields = sensitive_tool_argument_fields(tool_name);
    if sensitive_fields.is_empty() {
        return arguments.to_string();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(arguments) else {
        return arguments.to_string();
    };
    if !redact_sensitive_argument_fields(&mut value, sensitive_fields) {
        return arguments.to_string();
    }
    serde_json::to_string(&value).unwrap_or_else(|_| arguments.to_string())
}

async fn execute_tool_call_inner(context: &ToolContext, call: &ToolCall) -> Result<String> {
    let args = parse_arguments(&call.arguments)?;
    match call.name.as_str() {
        "pwd" => Ok(context.cwd.display().to_string()),
        "list_dir" => list_dir(context, &args),
        "read_file" => read_file(context, &args),
        "apply_patch" => apply_patch_tool(context, &args),
        "write_file" => write_file(context, &args),
        "append_file" => append_file(context, &args),
        "replace_in_file" => replace_in_file(context, &args),
        "search_files" => search_files(context, &args),
        "find_files" => find_files(context, &args),
        "make_dir" => make_dir(context, &args),
        "copy_path" => copy_path(context, &args),
        "move_path" => move_path(context, &args),
        "delete_path" => delete_path(context, &args),
        "stat_path" => stat_path(context, &args),
        "run_shell" => run_shell(context, &args).await,
        "read_env" => read_env(&args),
        "git_status" => git_status(context, &args).await,
        "git_diff" => git_diff(context, &args).await,
        "git_log" => git_log(context, &args).await,
        "git_show" => git_show(context, &args).await,
        "fetch_url" => fetch_url(context, &args).await,
        "http_request" => http_request(context, &args).await,
        "spawn_subagents" => spawn_subagents(context, &args).await,
        other => {
            if let Some(output) = connector_tools::execute_tool_call(context, other, &args).await? {
                Ok(output)
            } else {
                execute_dynamic_tool(context, other, &args).await
            }
        }
    }
}

fn tool(name: &str, description: &str, input_schema: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
    }
}

fn sensitive_tool_argument_fields(tool_name: &str) -> &'static [&'static str] {
    match tool_name {
        "configure_telegram_connector"
        | "configure_discord_connector"
        | "configure_slack_connector" => &["bot_token"],
        "configure_home_assistant_connector" => &["access_token"],
        _ => &[],
    }
}

fn redact_sensitive_argument_fields(value: &mut Value, sensitive_fields: &[&str]) -> bool {
    match value {
        Value::Object(map) => {
            let mut changed = false;
            for (key, nested) in map {
                if sensitive_fields.contains(&key.as_str()) {
                    redact_secret_value(nested);
                    changed = true;
                } else {
                    changed |= redact_sensitive_argument_fields(nested, sensitive_fields);
                }
            }
            changed
        }
        Value::Array(values) => {
            let mut changed = false;
            for nested in values {
                changed |= redact_sensitive_argument_fields(nested, sensitive_fields);
            }
            changed
        }
        _ => false,
    }
}

fn redact_secret_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for nested in map.values_mut() {
                redact_secret_value(nested);
            }
        }
        Value::Array(values) => {
            for nested in values {
                redact_secret_value(nested);
            }
        }
        _ => {
            *value = Value::String(REDACTED_SECRET_ARGUMENT.to_string());
        }
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoLogo", "-NoProfile", "-Command", command]);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", command]);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_context(root: &Path) -> ToolContext {
        ToolContext {
            state: test_state(),
            cwd: root.to_path_buf(),
            trust_policy: TrustPolicy {
                trusted_paths: vec![root.to_path_buf()],
                allow_shell: true,
                allow_network: true,
                allow_full_disk: false,
                allow_self_edit: false,
            },
            autonomy: AutonomyProfile::default(),
            permission_preset: PermissionPreset::FullAuto,
            http_client: Client::new(),
            mcp_servers: Vec::new(),
            app_connectors: Vec::new(),
            brave_connectors: Vec::new(),
            current_alias: Some("main".to_string()),
            default_thinking_level: None,
            delegation: agent_core::DelegationConfig::default(),
            delegation_targets: Vec::new(),
            delegation_depth: 0,
            background: false,
            background_shell_allowed: true,
            background_network_allowed: true,
            background_self_edit_allowed: true,
        }
    }

    fn test_state() -> AppState {
        let storage = agent_storage::Storage::open_at(
            std::env::temp_dir().join(format!("agent-tools-test-{}", uuid::Uuid::new_v4())),
        )
        .unwrap();
        AppState {
            storage,
            config: std::sync::Arc::new(tokio::sync::RwLock::new(agent_core::AppConfig::default())),
            http_client: Client::new(),
            browser_auth_sessions: crate::new_browser_auth_store(),
            dashboard_sessions: crate::new_dashboard_session_store(),
            started_at: Utc::now(),
            shutdown: tokio::sync::mpsc::unbounded_channel().0,
            autopilot_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            log_wake: std::sync::Arc::new(tokio::sync::Notify::new()),
            restart_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            rate_limiter: crate::ProviderRateLimiter::new(),
        }
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("agent-tools-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn truncate_keeps_valid_utf8_boundaries() {
        let input = "hello🙂world";
        let output = truncate(input, 7);
        assert!(output.starts_with("hello"));
    }

    #[test]
    fn path_join_uses_cwd_for_relative_paths() {
        let joined = path_helpers::join_to_cwd(Path::new("C:\\tmp"), "file.txt");
        assert!(joined.ends_with("file.txt"));
    }

    #[test]
    fn read_and_replace_file_tools_work() {
        let root = temp_root();
        let context = test_context(&root);
        let file = root.join("notes.txt");
        fs::write(&file, "alpha\nbeta\n").unwrap();

        let read = read_file(
            &context,
            &json!({
                "path": "notes.txt",
                "start_line": 2
            }),
        )
        .unwrap();
        assert!(read.contains("2: beta"));

        let replaced = replace_in_file(
            &context,
            &json!({
                "path": "notes.txt",
                "old": "beta",
                "new": "gamma"
            }),
        )
        .unwrap();
        assert!(replaced.contains("replaced 1 occurrence"));
        assert!(fs::read_to_string(file).unwrap().contains("gamma"));
    }

    #[test]
    fn apply_patch_tool_updates_and_adds_files() {
        let root = temp_root();
        let context = test_context(&root);
        fs::write(root.join("notes.txt"), "alpha\nbeta\n").unwrap();

        let result = apply_patch_tool(
            &context,
            &json!({
                "patch": "*** Begin Patch\n*** Update File: notes.txt\n@@\n alpha\n-beta\n+gamma\n*** Add File: hello.txt\n+hello\n*** End Patch"
            }),
        )
        .unwrap();

        assert!(result.contains("updated"));
        assert!(result.contains("added"));
        assert_eq!(
            fs::read_to_string(root.join("notes.txt")).unwrap(),
            "alpha\ngamma\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("hello.txt")).unwrap(),
            "hello\n"
        );
    }

    #[test]
    fn apply_patch_tool_rolls_back_on_failure() {
        let root = temp_root();
        let context = test_context(&root);
        fs::write(root.join("notes.txt"), "alpha\nbeta\n").unwrap();

        let _error = apply_patch_tool(
            &context,
            &json!({
                "patch": "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** Update File: notes.txt\n@@\n alpha\n-missing\n+gamma\n*** End Patch"
            }),
        )
        .unwrap_err();
        assert_eq!(
            fs::read_to_string(root.join("notes.txt")).unwrap(),
            "alpha\nbeta\n"
        );
        assert!(!root.join("hello.txt").exists());
    }

    #[test]
    fn search_files_finds_recursive_matches() {
        let root = temp_root();
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("main.rs"), "fn main() {}\nprintln!(\"hi\");\n").unwrap();
        let context = test_context(&root);

        let result = search_files(
            &context,
            &json!({
                "path": ".",
                "query": "main"
            }),
        )
        .unwrap();

        assert!(result.contains("main.rs:1"));
    }

    #[test]
    fn find_files_supports_wildcards() {
        let root = temp_root();
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(nested.join("lib.rs"), "pub fn helper() {}\n").unwrap();
        let context = test_context(&root);

        let result = find_files(
            &context,
            &json!({
                "path": ".",
                "pattern": "src/*.rs"
            }),
        )
        .unwrap();

        assert!(result.contains("src/main.rs"));
        assert!(result.contains("src/lib.rs"));
    }

    #[test]
    fn copy_move_and_delete_tools_work() {
        let root = temp_root();
        let context = test_context(&root);
        fs::write(root.join("alpha.txt"), "alpha").unwrap();

        let copied = copy_path(
            &context,
            &json!({
                "source": "alpha.txt",
                "destination": "beta.txt"
            }),
        )
        .unwrap();
        assert!(copied.contains("copied"));
        assert_eq!(fs::read_to_string(root.join("beta.txt")).unwrap(), "alpha");

        let moved = move_path(
            &context,
            &json!({
                "source": "beta.txt",
                "destination": "gamma.txt"
            }),
        )
        .unwrap();
        assert!(moved.contains("moved"));
        assert!(!root.join("beta.txt").exists());
        assert_eq!(fs::read_to_string(root.join("gamma.txt")).unwrap(), "alpha");

        let deleted = delete_path(
            &context,
            &json!({
                "path": "gamma.txt"
            }),
        )
        .unwrap();
        assert!(deleted.contains("deleted"));
        assert!(!root.join("gamma.txt").exists());
    }

    #[test]
    fn parse_subagent_task_supports_provider_pool_fields() {
        let task = delegation_tools::parse_subagent_task(&json!({
            "prompt": "Compare these implementations",
            "target": "claude",
            "provider_id": "anthropic",
            "requested_model": "claude-sonnet",
            "thinking_level": "high",
            "strategy": "parallel_best_effort"
        }))
        .unwrap();

        assert_eq!(task.prompt, "Compare these implementations");
        assert_eq!(task.target.as_deref(), Some("claude"));
        assert_eq!(task.provider_id.as_deref(), Some("anthropic"));
        assert_eq!(task.requested_model.as_deref(), Some("claude-sonnet"));
        assert_eq!(task.thinking_level, Some(ThinkingLevel::High));
        assert_eq!(task.strategy, Some(SubAgentStrategy::ParallelBestEffort));
    }

    #[test]
    fn parse_subagent_task_rejects_unknown_strategy() {
        let error = delegation_tools::parse_subagent_task(&json!({
            "prompt": "Compare these implementations",
            "strategy": "fastest"
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unsupported strategy"));
    }

    #[test]
    fn read_env_blocks_sensitive_names() {
        let error = read_env(&json!({ "name": "OPENAI_API_KEY" })).unwrap_err();
        assert!(error
            .to_string()
            .contains("sensitive environment variables"));
    }

    #[test]
    fn sanitize_tool_arguments_redacts_secret_fields() {
        let sanitized = sanitize_tool_arguments(
            "configure_telegram_connector",
            r#"{"bot_token":"123:abc","id":"telegram-main"}"#,
        );
        let parsed: Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(parsed["bot_token"], Value::String("[REDACTED]".to_string()));
        assert_eq!(parsed["id"], Value::String("telegram-main".to_string()));
    }

    #[test]
    fn sanitize_tool_arguments_redacts_nested_secret_values() {
        let sanitized = sanitize_tool_arguments(
            "configure_home_assistant_connector",
            r#"{"base_url":"http://ha.local","access_token":{"value":"secret-token","refresh":"refresh-token"}}"#,
        );
        let parsed: Value = serde_json::from_str(&sanitized).unwrap();
        assert_eq!(
            parsed["access_token"]["value"],
            Value::String("[REDACTED]".to_string())
        );
        assert_eq!(
            parsed["access_token"]["refresh"],
            Value::String("[REDACTED]".to_string())
        );
        assert_eq!(
            parsed["base_url"],
            Value::String("http://ha.local".to_string())
        );
    }

    #[test]
    fn sanitize_tool_arguments_leaves_non_sensitive_tools_unchanged() {
        let arguments = r#"{"path":"README.md"}"#;
        assert_eq!(sanitize_tool_arguments("read_file", arguments), arguments);
    }

    #[test]
    fn git_target_uses_parent_directory_for_files() {
        let root = temp_root();
        let file = root.join("repo").join("tracked.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "tracked").unwrap();
        let context = test_context(&root);

        let (workdir, filter) = admin_helpers::git_target(
            &context,
            &json!({
                "path": file.display().to_string()
            }),
        )
        .unwrap();

        assert_eq!(
            workdir.file_name().and_then(|name| name.to_str()),
            Some("repo")
        );
        assert_eq!(filter.as_deref(), Some("tracked.txt"));
    }

    #[tokio::test]
    async fn run_shell_executes_in_workspace() {
        let root = temp_root();
        let context = test_context(&root);
        let command = if cfg!(target_os = "windows") {
            "Write-Output smoke-shell"
        } else {
            "printf smoke-shell"
        };

        let output = run_shell(
            &context,
            &json!({
                "command": command,
                "timeout_seconds": 5
            }),
        )
        .await
        .unwrap();

        assert!(output.contains("smoke-shell"));
    }
}
