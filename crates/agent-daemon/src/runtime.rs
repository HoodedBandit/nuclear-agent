use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

use agent_core::{
    AuthMode, AutonomyMode, AutonomyState, BatchTaskRequest, BatchTaskResponse,
    ConversationMessage, InputAttachment, MessageRole, ModelAlias, PermissionPreset,
    ProviderConfig, ProviderReply, RunTaskResponse, SessionMessage, SubAgentResult, ThinkingLevel,
    ToolExecutionOutcome, ToolExecutionRecord,
};
use agent_policy::{allow_network, allow_shell, permission_summary, tool_allowed_by_preset};
use agent_providers::{load_api_key, load_oauth_token, run_prompt};
use agent_storage::PersistSessionTurnInput;
use anyhow::Result;
use axum::http::StatusCode;
use futures::future::join_all;
use jsonschema::JSONSchema;
use uuid::Uuid;

use crate::{
    append_log, build_memory_context,
    delegation::{
        delegation_targets_from_config, resolve_delegation_tasks, summarize_batch_results,
    },
    learn_from_interaction, load_enabled_skill_guidance, sync_system_profile_memories,
    tools::{execute_tool_call, tool_definitions, ToolContext},
    ApiError, AppState, MAX_TOOL_LOOP_ITERATIONS, REPEATED_TOOL_BATCH_LIMIT,
};

pub(crate) struct TaskRequestInput {
    pub(crate) prompt: String,
    pub(crate) requested_model: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) attachments: Vec<InputAttachment>,
    pub(crate) permission_preset: Option<PermissionPreset>,
    pub(crate) output_schema_json: Option<String>,
    pub(crate) persist: bool,
    pub(crate) background: bool,
    pub(crate) delegation_depth: u8,
}

#[derive(Clone, Copy)]
pub(crate) struct DelegationExecutionOptions {
    pub(crate) background: bool,
    pub(crate) permission_preset: Option<PermissionPreset>,
    pub(crate) delegation_depth: u8,
}

#[derive(Clone)]
pub(crate) struct ResolvedSubAgentTask {
    pub(crate) prompt: String,
    pub(crate) alias: ModelAlias,
    pub(crate) provider: ProviderConfig,
    pub(crate) requested_model: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) output_schema_json: Option<String>,
}

struct TaskExecution {
    reply: ProviderReply,
    transcript_messages: Vec<ConversationMessage>,
    tool_events: Vec<ToolExecutionRecord>,
    structured_output_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolBatchExecution {
    pub(crate) name: String,
    pub(crate) arguments: String,
    pub(crate) outcome: &'static str,
    pub(crate) output: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToolLoopResolution {
    Success(String),
    Error(String),
}

struct PersistExecutionInput<'a> {
    session_id: &'a str,
    alias: &'a ModelAlias,
    provider: &'a ProviderConfig,
    prompt: &'a str,
    attachments: &'a [InputAttachment],
    reply: &'a ProviderReply,
    transcript_messages: &'a [ConversationMessage],
    is_new_session: bool,
    cwd: &'a PathBuf,
}

pub(crate) async fn execute_task_request(
    state: &AppState,
    alias: &ModelAlias,
    provider: &ProviderConfig,
    input: TaskRequestInput,
) -> Result<RunTaskResponse, ApiError> {
    verify_runtime_provider_credentials(provider)?;
    let TaskRequestInput {
        prompt,
        requested_model,
        session_id,
        cwd,
        thinking_level,
        attachments,
        permission_preset,
        output_schema_json,
        persist,
        background,
        delegation_depth,
    } = input;
    let is_new_session = session_id.is_none();
    let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let cwd = resolve_request_cwd(cwd)?;
    let mut messages =
        load_history_messages(state, if persist { Some(&session_id) } else { None })?;
    let permission_preset = resolve_permission_preset(state, permission_preset).await;
    sync_system_profile_memories(state, Some(&cwd), Some(&provider.id))?;
    let skill_guidance =
        load_enabled_skill_guidance(state, &prompt, Some(&cwd), Some(&provider.id)).await;
    let memory_context = build_memory_context(
        state,
        &prompt,
        Some(&session_id),
        Some(&cwd),
        Some(&provider.id),
    )?;
    let context = tool_context(
        state,
        alias,
        cwd.clone(),
        permission_preset,
        thinking_level,
        background,
        delegation_depth,
    )
    .await;
    let delegation_hint = delegation_guidance(&context);
    messages.insert(
        0,
        system_message(
            &cwd,
            thinking_level,
            permission_preset,
            output_schema_json.as_deref(),
            &skill_guidance,
            &delegation_hint,
        ),
    );
    if let Some(memory_context) = memory_context {
        messages.insert(
            1,
            ConversationMessage {
                role: MessageRole::System,
                content: memory_context,
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
            },
        );
    }
    messages.push(ConversationMessage {
        role: MessageRole::User,
        content: prompt.clone(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: attachments.clone(),
    });
    let execution = drive_tool_loop(
        state,
        provider,
        requested_model.as_deref().unwrap_or(&alias.model),
        &session_id,
        messages,
        thinking_level,
        output_schema_json.as_deref(),
        &context,
    )
    .await?;

    if persist {
        persist_execution(
            state,
            PersistExecutionInput {
                session_id: &session_id,
                alias,
                provider,
                prompt: &prompt,
                attachments: &attachments,
                reply: &execution.reply,
                transcript_messages: &execution.transcript_messages,
                is_new_session,
                cwd: &cwd,
            },
        )?;
        append_log(
            state,
            "info",
            "run",
            format!("session '{}' used alias '{}'", session_id, alias.alias),
        )?;
        learn_from_interaction(
            state,
            &prompt,
            &execution.reply.content,
            &execution.transcript_messages,
            &execution.tool_events,
            permission_preset,
            &session_id,
            Some(&provider.id),
            Some(&cwd),
            background,
        )?;
    }

    Ok(RunTaskResponse {
        session_id,
        alias: alias.alias.clone(),
        provider_id: provider.id.clone(),
        model: execution.reply.model,
        response: execution.reply.content,
        tool_events: execution.tool_events,
        structured_output_json: execution.structured_output_json,
    })
}

pub(crate) async fn execute_batch_request(
    state: &AppState,
    payload: BatchTaskRequest,
    options: DelegationExecutionOptions,
) -> Result<BatchTaskResponse, ApiError> {
    let config = state.config.read().await.clone();
    let child_depth = options.delegation_depth.saturating_add(1);
    if let Some(limit) = config.delegation.max_depth.as_option() {
        if usize::from(child_depth) > limit {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "delegation depth limit exceeded: requested depth {} but max depth is {}",
                    child_depth, limit
                ),
            ));
        }
    }
    let tasks = resolve_delegation_tasks(&config, &payload)?;
    if let Some(limit) = config.delegation.max_parallel_subagents.as_option() {
        if tasks.len() > limit {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                format!(
                    "parallel subagent limit exceeded: resolved {} run(s) but max parallel subagents is {}",
                    tasks.len(),
                    limit
                ),
            ));
        }
    }
    let futures = tasks.into_iter().map(|task| {
        let state = state.clone();
        async move {
            let fallback_model = task
                .requested_model
                .clone()
                .unwrap_or_else(|| task.alias.model.clone());
            let alias = task.alias.clone();
            let provider = task.provider.clone();
            let response = execute_task_request(
                &state,
                &task.alias,
                &task.provider,
                TaskRequestInput {
                    prompt: task.prompt,
                    requested_model: task.requested_model,
                    session_id: None,
                    cwd: task.cwd,
                    thinking_level: task.thinking_level,
                    attachments: Vec::new(),
                    permission_preset: options.permission_preset,
                    output_schema_json: task.output_schema_json,
                    persist: false,
                    background: options.background,
                    delegation_depth: child_depth,
                },
            )
            .await;

            match response {
                Ok(response) => SubAgentResult {
                    alias: alias.alias,
                    provider_id: provider.id,
                    model: response.model,
                    success: true,
                    response: response.response,
                    error: None,
                    structured_output_json: response.structured_output_json,
                },
                Err(error) => SubAgentResult {
                    alias: alias.alias,
                    provider_id: provider.id,
                    model: fallback_model,
                    success: false,
                    response: String::new(),
                    error: Some(error.message),
                    structured_output_json: None,
                },
            }
        }
    });

    let results = join_all(futures).await;
    let all_succeeded = results.iter().all(|result| result.success);
    Ok(BatchTaskResponse {
        summary: summarize_batch_results(&results),
        results,
        all_succeeded,
    })
}

fn verify_runtime_provider_credentials(provider: &ProviderConfig) -> Result<(), ApiError> {
    match provider.auth_mode {
        AuthMode::None => Ok(()),
        AuthMode::ApiKey => {
            let account = provider.keychain_account.as_deref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("provider '{}' is missing keychain metadata", provider.id),
                )
            })?;
            load_api_key(account).map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "provider '{}' has unreadable saved API key: {error}",
                        provider.id
                    ),
                )
            })?;
            Ok(())
        }
        AuthMode::OAuth => {
            let account = provider.keychain_account.as_deref().ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!("provider '{}' is missing keychain metadata", provider.id),
                )
            })?;
            load_oauth_token(account).map_err(|error| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "provider '{}' has unreadable saved OAuth credentials: {error}",
                        provider.id
                    ),
                )
            })?;
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn drive_tool_loop(
    state: &AppState,
    provider: &ProviderConfig,
    model: &str,
    session_id: &str,
    mut messages: Vec<ConversationMessage>,
    thinking_level: Option<ThinkingLevel>,
    output_schema_json: Option<&str>,
    context: &ToolContext,
) -> Result<TaskExecution, ApiError> {
    let mut tools = tool_definitions(context);
    tools.retain(|tool| tool_allowed_by_preset(&tool.name, context.permission_preset));
    if !context.background_shell_allowed || !allow_shell(&context.trust_policy, &context.autonomy) {
        tools.retain(|tool| {
            !matches!(
                tool.name.as_str(),
                "run_shell" | "git_status" | "git_diff" | "git_log" | "git_show"
            )
        });
    }
    if !context.background_network_allowed
        || !allow_network(&context.trust_policy, &context.autonomy)
    {
        tools.retain(|tool| tool.name != "fetch_url" && tool.name != "http_request");
    }
    let mut transcript_messages = Vec::new();
    let mut tool_events = Vec::new();
    let mut last_tool_batch: Option<Vec<ToolBatchExecution>> = None;
    let mut repeated_tool_batch_count = 0usize;

    for _ in 0..MAX_TOOL_LOOP_ITERATIONS {
        state.rate_limiter.acquire(&provider.id).await;
        let reply = run_prompt(
            &state.http_client,
            provider,
            &messages,
            Some(model),
            Some(session_id),
            thinking_level,
            &tools,
        )
        .await?;

        if reply.tool_calls.is_empty() {
            return Ok(TaskExecution {
                structured_output_json: maybe_validate_structured_output(
                    &reply.content,
                    output_schema_json,
                )?,
                reply,
                transcript_messages,
                tool_events,
            });
        }

        let assistant_message = ConversationMessage {
            role: MessageRole::Assistant,
            content: reply.content.clone(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: reply.tool_calls.clone(),
            provider_payload_json: reply.provider_payload_json.clone(),
            attachments: Vec::new(),
        };
        transcript_messages.push(assistant_message.clone());
        messages.push(assistant_message);

        let mut current_tool_batch = Vec::new();
        for tool_call in &reply.tool_calls {
            let tool_execution = execute_tool_call(context, tool_call).await;
            append_log(
                state,
                if tool_execution.message.content.starts_with("ERROR:") {
                    "warn"
                } else {
                    "info"
                },
                "tool",
                format!(
                    "{} -> {}",
                    tool_call.name,
                    summarize_tool_output(&tool_execution.message.content)
                ),
            )?;
            tool_events.push(tool_execution.record.clone());
            current_tool_batch.push(ToolBatchExecution {
                name: tool_call.name.clone(),
                arguments: tool_call.arguments.clone(),
                outcome: match tool_execution.record.outcome {
                    ToolExecutionOutcome::Success => "success",
                    ToolExecutionOutcome::Error => "error",
                },
                output: tool_execution.record.output.clone(),
            });
            transcript_messages.push(tool_execution.message.clone());
            messages.push(tool_execution.message);
        }

        if last_tool_batch.as_ref() == Some(&current_tool_batch) {
            repeated_tool_batch_count += 1;
        } else {
            repeated_tool_batch_count = 1;
            last_tool_batch = Some(current_tool_batch.clone());
        }

        if repeated_tool_batch_count >= REPEATED_TOOL_BATCH_LIMIT {
            match repeated_tool_loop_resolution(&current_tool_batch, output_schema_json) {
                ToolLoopResolution::Success(content) => {
                    let reply = ProviderReply {
                        provider_id: reply.provider_id.clone(),
                        model: reply.model.clone(),
                        content,
                        tool_calls: Vec::new(),
                        provider_payload_json: reply.provider_payload_json.clone(),
                    };
                    return Ok(TaskExecution {
                        structured_output_json: None,
                        reply,
                        transcript_messages,
                        tool_events,
                    });
                }
                ToolLoopResolution::Error(message) => {
                    return Err(ApiError::new(StatusCode::BAD_GATEWAY, message));
                }
            }
        }
    }

    let message = match last_tool_batch.as_ref() {
        Some(batch) => format!(
            "tool loop exceeded maximum iterations; the model kept requesting tool calls without finishing.\nLast tool batch:\n{}",
            repeated_tool_batch_summary(batch)
        ),
        None => "tool loop exceeded maximum iterations; the model kept requesting tool calls without finishing".to_string(),
    };
    Err(ApiError::new(StatusCode::BAD_GATEWAY, message))
}

pub(crate) fn repeated_tool_loop_resolution(
    batch: &[ToolBatchExecution],
    output_schema_json: Option<&str>,
) -> ToolLoopResolution {
    let summary = repeated_tool_batch_summary(batch);
    if output_schema_json.is_none() && repeated_tool_batch_is_safe_completion(batch) {
        return ToolLoopResolution::Success(format!(
            "Completed the requested filesystem change. The daemon stopped a repeated identical tool batch instead of looping.\n\nLast successful tool result:\n{}",
            summary
        ));
    }

    ToolLoopResolution::Error(format!(
        "tool loop repeated the same tool batch without making progress. Last repeated batch:\n{}",
        summary
    ))
}

fn repeated_tool_batch_summary(batch: &[ToolBatchExecution]) -> String {
    batch
        .iter()
        .map(|tool| {
            format!(
                "{}({}) [{}] -> {}",
                tool.name,
                tool.arguments,
                tool.outcome,
                summarize_tool_output(&tool.output)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn repeated_tool_batch_is_safe_completion(batch: &[ToolBatchExecution]) -> bool {
    if batch.is_empty() || batch.iter().any(|tool| tool.outcome != "success") {
        return false;
    }

    batch.iter().all(|tool| {
        matches!(
            tool.name.as_str(),
            "write_file"
                | "append_file"
                | "replace_in_file"
                | "apply_patch"
                | "make_dir"
                | "copy_path"
                | "move_path"
                | "delete_path"
        )
    })
}

fn persist_execution(state: &AppState, input: PersistExecutionInput<'_>) -> Result<()> {
    let PersistExecutionInput {
        session_id,
        alias,
        provider,
        prompt,
        attachments,
        reply,
        transcript_messages,
        is_new_session,
        cwd,
    } = input;
    let initial_title = is_new_session.then(|| derive_session_title(prompt));
    let mut persisted_messages = vec![SessionMessage::new(
        session_id.to_string(),
        MessageRole::User,
        prompt.to_string(),
        Some(provider.id.clone()),
        Some(reply.model.clone()),
    )
    .with_attachments(attachments.to_vec())];
    persisted_messages.extend(transcript_messages.iter().map(|message| {
        SessionMessage::new(
            session_id.to_string(),
            message.role.clone(),
            message.content.clone(),
            Some(provider.id.clone()),
            Some(reply.model.clone()),
        )
        .with_tool_metadata(message.tool_call_id.clone(), message.tool_name.clone())
        .with_tool_calls(message.tool_calls.clone())
        .with_provider_payload(message.provider_payload_json.clone())
        .with_attachments(message.attachments.clone())
    }));
    persisted_messages.push(
        SessionMessage::new(
            session_id.to_string(),
            MessageRole::Assistant,
            reply.content.clone(),
            Some(provider.id.clone()),
            Some(reply.model.clone()),
        )
        .with_tool_calls(reply.tool_calls.clone())
        .with_provider_payload(reply.provider_payload_json.clone()),
    );
    state
        .storage
        .persist_session_turn(PersistSessionTurnInput {
            session_id,
            title: initial_title.as_deref(),
            alias,
            provider_id: &provider.id,
            model: &reply.model,
            cwd: Some(cwd.as_path()),
            messages: &persisted_messages,
        })?;
    Ok(())
}

fn load_history_messages(
    state: &AppState,
    session_id: Option<&str>,
) -> Result<Vec<ConversationMessage>, ApiError> {
    let Some(session_id) = session_id else {
        return Ok(Vec::new());
    };

    Ok(state
        .storage
        .list_session_messages(session_id)?
        .into_iter()
        .map(|message| ConversationMessage {
            role: message.role,
            content: message.content,
            tool_call_id: message.tool_call_id,
            tool_name: message.tool_name,
            tool_calls: message.tool_calls,
            provider_payload_json: message.provider_payload_json,
            attachments: message.attachments,
        })
        .collect())
}

async fn tool_context(
    state: &AppState,
    alias: &ModelAlias,
    cwd: PathBuf,
    permission_preset: PermissionPreset,
    thinking_level: Option<ThinkingLevel>,
    background: bool,
    delegation_depth: u8,
) -> ToolContext {
    let config = state.config.read().await;
    let background_shell_allowed = !background || config.autopilot.allow_background_shell;
    let background_network_allowed = !background || config.autopilot.allow_background_network;
    let background_self_edit_allowed = !background || config.autopilot.allow_background_self_edit;
    let delegation_targets = delegation_targets_from_config(&config, Some(&alias.alias));
    let delegation = if matches!(
        (config.autonomy.state.clone(), config.autonomy.mode.clone()),
        (AutonomyState::Enabled, AutonomyMode::Evolve)
    ) {
        agent_core::DelegationConfig {
            max_depth: agent_core::DelegationLimit::Unlimited,
            max_parallel_subagents: agent_core::DelegationLimit::Unlimited,
            disabled_provider_ids: config.delegation.disabled_provider_ids.clone(),
        }
    } else {
        config.delegation.clone()
    };
    ToolContext {
        state: state.clone(),
        cwd,
        trust_policy: config.trust_policy.clone(),
        autonomy: config.autonomy.clone(),
        permission_preset,
        http_client: state.http_client.clone(),
        mcp_servers: config.mcp_servers.clone(),
        app_connectors: config.app_connectors.clone(),
        current_alias: Some(alias.alias.clone()),
        default_thinking_level: thinking_level,
        delegation,
        delegation_targets,
        delegation_depth,
        background,
        background_shell_allowed,
        background_network_allowed,
        background_self_edit_allowed,
    }
}

async fn resolve_permission_preset(
    state: &AppState,
    requested: Option<PermissionPreset>,
) -> PermissionPreset {
    if let Some(preset) = requested {
        return preset;
    }
    let config = state.config.read().await;
    if matches!(config.autonomy.state, AutonomyState::Enabled)
        && matches!(
            config.autonomy.mode,
            AutonomyMode::FreeThinking | AutonomyMode::Evolve
        )
    {
        return PermissionPreset::FullAuto;
    }
    config.permission_preset
}

pub(crate) fn maybe_validate_structured_output(
    reply_content: &str,
    output_schema_json: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(schema_json) = output_schema_json else {
        return Ok(None);
    };
    let schema = serde_json::from_str::<serde_json::Value>(schema_json).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("invalid output schema JSON: {error}"),
        )
    })?;
    let parsed = serde_json::from_str::<serde_json::Value>(reply_content).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("agent response was not valid JSON for structured output: {error}"),
        )
    })?;
    let compiled = JSONSchema::compile(&schema).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("invalid output schema definition: {error}"),
        )
    })?;
    if let Err(errors) = compiled.validate(&parsed) {
        let details = errors
            .take(5)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            format!("agent response did not match structured output schema: {details}"),
        ));
    }
    Ok(Some(parsed.to_string()))
}

pub(crate) fn resolve_request_cwd(cwd: Option<PathBuf>) -> Result<PathBuf, ApiError> {
    let cwd = cwd
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(path)
            }
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    Ok(cwd)
}

fn system_message(
    cwd: &FsPath,
    thinking_level: Option<ThinkingLevel>,
    permission_preset: PermissionPreset,
    output_schema_json: Option<&str>,
    skill_guidance: &str,
    delegation_hint: &str,
) -> ConversationMessage {
    let agents_guidance = load_agents_guidance(cwd);
    let thinking_hint = thinking_level.map(|thinking_level| {
        format!(
            " Reasoning effort preference for this session: {}.",
            thinking_level.as_str()
        )
    });
    let permission_hint = format!(
        " Current permission preset: {}.",
        permission_summary(permission_preset)
    );
    let structured_output_hint = output_schema_json.map(|schema| {
        format!(
            " When you produce the final answer, emit valid JSON only and ensure it matches this schema: {}",
            schema
        )
    });
    let agents_hint = if agents_guidance.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nApply the following AGENTS.md guidance. Later files are more specific than earlier ones.\n{}",
            agents_guidance
        )
    };
    let skills_hint = if skill_guidance.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nThe following enabled skill instructions are available for this turn.\n{}",
            skill_guidance
        )
    };
    let delegation_hint = if delegation_hint.is_empty() {
        String::new()
    } else {
        format!("\n\n{delegation_hint}")
    };
    ConversationMessage {
        role: MessageRole::System,
        content: format!(
            "You are a local coding agent running in {}. Use the available tools when you need filesystem, git, environment, shell, or network access. Prefer apply_patch for precise edits and write_file only for full rewrites or new files. Prefer accurate tool use over guessing. Do not repeat an identical successful tool call batch; after a successful change, summarize completion instead of calling the same tool again.{}{}{}{}{}{}",
            cwd.display(),
            thinking_hint.unwrap_or_default(),
            permission_hint,
            structured_output_hint.unwrap_or_default(),
            agents_hint,
            skills_hint,
            delegation_hint,
        ),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: Vec::new(),
    }
}

fn delegation_guidance(context: &ToolContext) -> String {
    if context.delegation_targets.is_empty() {
        return "Cross-provider delegation is unavailable because no logged-in delegation targets are currently usable.".to_string();
    }
    let targets = context
        .delegation_targets
        .iter()
        .take(8)
        .map(|target| {
            let mut detail = format!(
                "{} -> {} / {}",
                target.alias, target.provider_id, target.model
            );
            if target.primary {
                detail.push_str(" [primary]");
            }
            detail
        })
        .collect::<Vec<_>>()
        .join("; ");
    let overflow = if context.delegation_targets.len() > 8 {
        format!(
            " and {} more target(s)",
            context.delegation_targets.len() - 8
        )
    } else {
        String::new()
    };
    format!(
        "Cross-provider delegation is available via spawn_subagents. Available targets: {targets}{overflow}. Current delegation depth: {}. Max depth: {}. Max parallel subagents: {}.",
        context.delegation_depth,
        context.delegation.max_depth,
        context.delegation.max_parallel_subagents
    )
}

pub(crate) fn summarize_tool_output(output: &str) -> String {
    const MAX_LEN: usize = 120;
    if output.len() <= MAX_LEN {
        return output.to_string();
    }
    let mut end = MAX_LEN;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &output[..end])
}

fn derive_session_title(prompt: &str) -> String {
    let trimmed = prompt.trim();
    let first_line = trimmed.lines().next().unwrap_or("New session").trim();
    let mut title = if first_line.is_empty() {
        "New session".to_string()
    } else {
        first_line.to_string()
    };
    const MAX_TITLE_CHARS: usize = 80;
    if title.chars().count() > MAX_TITLE_CHARS {
        title = title.chars().take(MAX_TITLE_CHARS).collect::<String>();
        title.push_str("...");
    }
    title
}

fn load_agents_guidance(cwd: &FsPath) -> String {
    const MAX_FILE_BYTES: usize = 16_000;
    const MAX_TOTAL_BYTES: usize = 48_000;

    let mut files = Vec::new();
    if let Some(home) = home_dir() {
        files.push(home.join(".codex").join("AGENTS.md"));
    }

    let mut ancestors = Vec::new();
    let mut current = Some(cwd);
    while let Some(path) = current {
        ancestors.push(path.to_path_buf());
        current = path.parent();
    }
    ancestors.reverse();
    for directory in ancestors {
        files.push(directory.join("AGENTS.md"));
    }

    let mut output = String::new();
    let mut seen = std::collections::HashSet::new();
    for file in files {
        if !seen.insert(file.clone()) || !file.is_file() {
            continue;
        }

        let Ok(mut content) = fs::read_to_string(&file) else {
            continue;
        };
        if content.len() > MAX_FILE_BYTES {
            content.truncate(MAX_FILE_BYTES);
            content.push_str("\n\n[truncated]");
        }

        let block = format!("--- {}\n{}\n", file.display(), content.trim_end());
        if output.len() + block.len() > MAX_TOTAL_BYTES {
            output.push_str("\n--- [additional AGENTS.md content truncated]");
            break;
        }
        output.push_str(&block);
    }

    output
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}
