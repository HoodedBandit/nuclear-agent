use std::{
    fs,
    future::Future,
    path::{Path as FsPath, PathBuf},
};

use agent_core::{
    truncate_with_suffix, AuthMode, AutonomyMode, AutonomyState, BatchTaskRequest,
    BatchTaskResponse, ConversationMessage, InputAttachment, MessageRole, ModelAlias,
    PermissionPreset, ProviderConfig, ProviderOutputItem, ProviderReply, RemoteContentArtifact,
    RemoteContentPolicy, RunTaskResponse, RunTaskStreamEvent, SessionMessage, SessionSummary,
    SubAgentResult, TaskMode, ThinkingLevel, ToolCall, ToolExecutionOutcome, ToolExecutionRecord,
};
use agent_policy::permission_summary;
use agent_providers::{load_api_key, load_oauth_token, run_prompt};
use agent_storage::PersistSessionTurnInput;
use anyhow::Result;
use axum::http::StatusCode;
use futures::future::join_all;
use jsonschema::JSONSchema;
use uuid::Uuid;

use crate::{
    append_log, build_memory_context, collect_hosted_plugin_tools,
    delegation::{
        delegation_targets_from_config, resolve_delegation_tasks, summarize_batch_results,
    },
    learn_from_interaction, load_enabled_skill_guidance, load_pattern_guidance,
    sync_system_profile_memories,
    tools::{
        effective_tool_definitions, execute_tool_call, sanitize_tool_arguments, sanitize_tool_call,
        tool_call_has_sensitive_arguments, ToolContext,
    },
    ApiError, AppState, ExecutionCancellation, MAX_TOOL_LOOP_ITERATIONS, REPEATED_TOOL_BATCH_LIMIT,
};

pub(crate) struct TaskRequestInput {
    pub(crate) prompt: String,
    pub(crate) requested_model: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) attachments: Vec<InputAttachment>,
    pub(crate) permission_preset: Option<PermissionPreset>,
    pub(crate) task_mode: Option<TaskMode>,
    pub(crate) output_schema_json: Option<String>,
    pub(crate) remote_content_policy_override: Option<RemoteContentPolicy>,
    pub(crate) persist: bool,
    pub(crate) background: bool,
    pub(crate) delegation_depth: u8,
    pub(crate) cancellation: Option<ExecutionCancellation>,
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
    pub(crate) task_mode: Option<TaskMode>,
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
    task_mode: Option<TaskMode>,
    cwd: &'a PathBuf,
}

pub(crate) async fn execute_task_request(
    state: &AppState,
    alias: &ModelAlias,
    provider: &ProviderConfig,
    input: TaskRequestInput,
) -> Result<RunTaskResponse, ApiError> {
    let mut emit = |_| std::future::ready(true);
    execute_task_request_with_events(state, alias, provider, input, &mut emit).await
}

pub(crate) async fn execute_task_request_with_events<F, Fut>(
    state: &AppState,
    alias: &ModelAlias,
    provider: &ProviderConfig,
    input: TaskRequestInput,
    emit: &mut F,
) -> Result<RunTaskResponse, ApiError>
where
    F: FnMut(RunTaskStreamEvent) -> Fut,
    Fut: Future<Output = bool>,
{
    verify_runtime_provider_credentials(provider)?;
    let TaskRequestInput {
        prompt,
        requested_model,
        session_id,
        cwd,
        thinking_level,
        attachments,
        permission_preset,
        task_mode,
        output_schema_json,
        remote_content_policy_override,
        persist,
        background,
        delegation_depth,
        cancellation,
    } = input;
    ensure_execution_active(cancellation.as_ref())?;
    let is_new_session = session_id.is_none();
    let session_id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let model = requested_model.as_deref().unwrap_or(&alias.model);
    let existing_session = if persist {
        state.storage.get_session(&session_id)?
    } else {
        None
    };
    let task_mode = effective_task_mode(task_mode, existing_session.as_ref());
    let cwd = resolve_request_cwd(effective_session_cwd(cwd, existing_session.as_ref()))?;
    let mut messages =
        load_history_messages(state, if persist { Some(&session_id) } else { None })?;
    let allowed_direct_urls =
        crate::tools::remote_content::extract_user_allowed_urls(&messages, &prompt);
    let permission_preset = resolve_permission_preset(state, permission_preset).await;
    let remote_content_policy =
        resolve_remote_content_policy(state, remote_content_policy_override).await;
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
        provider,
        model,
        cwd.clone(),
        permission_preset,
        thinking_level,
        task_mode,
        background,
        delegation_depth,
        remote_content_policy,
        allowed_direct_urls,
    )
    .await;
    let delegation_hint = delegation_guidance(&context);
    messages.insert(
        0,
        system_message(
            &cwd,
            thinking_level,
            permission_preset,
            task_mode,
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
                provider_output_items: Vec::new(),
            },
        );
    }
    // Inject learned usage-pattern guidance when available.
    let pattern_guidance = load_pattern_guidance(
        state,
        Some(&cwd.display().to_string()),
        Some(&provider.id),
        5,
    )
    .unwrap_or_default();
    if !pattern_guidance.is_empty() {
        messages.insert(
            if messages.len() > 1 { 2 } else { 1 },
            ConversationMessage {
                role: MessageRole::System,
                content: pattern_guidance,
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
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
        provider_output_items: Vec::new(),
    });
    emit_stream_event(
        emit,
        RunTaskStreamEvent::SessionStarted {
            session_id: session_id.clone(),
            alias: alias.alias.clone(),
            provider_id: provider.id.clone(),
            model: model.to_string(),
        },
    )
    .await?;
    let execution = drive_tool_loop(
        state,
        provider,
        model,
        &session_id,
        messages,
        thinking_level,
        output_schema_json.as_deref(),
        &context,
        cancellation.as_ref(),
        emit,
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
                task_mode,
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

    let response = RunTaskResponse {
        session_id,
        alias: alias.alias.clone(),
        provider_id: provider.id.clone(),
        model: execution.reply.model,
        response: execution.reply.content,
        tool_events: execution.tool_events,
        structured_output_json: execution.structured_output_json,
    };
    emit_stream_event(
        emit,
        RunTaskStreamEvent::Completed {
            response: response.clone(),
        },
    )
    .await?;
    Ok(response)
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
                    task_mode: task.task_mode,
                    output_schema_json: task.output_schema_json,
                    remote_content_policy_override: None,
                    persist: false,
                    background: options.background,
                    delegation_depth: child_depth,
                    cancellation: None,
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

pub(crate) fn provider_has_runnable_access(provider: &ProviderConfig) -> bool {
    verify_runtime_provider_credentials(provider).is_ok()
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
async fn drive_tool_loop<F, Fut>(
    state: &AppState,
    provider: &ProviderConfig,
    model: &str,
    session_id: &str,
    mut messages: Vec<ConversationMessage>,
    thinking_level: Option<ThinkingLevel>,
    output_schema_json: Option<&str>,
    context: &ToolContext,
    cancellation: Option<&ExecutionCancellation>,
    emit: &mut F,
) -> Result<TaskExecution, ApiError>
where
    F: FnMut(RunTaskStreamEvent) -> Fut,
    Fut: Future<Output = bool>,
{
    let tools = effective_tool_definitions(context).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("failed to assemble tool registry: {error}"),
        )
    })?;
    let allowed_tools = tools
        .iter()
        .map(|tool| (tool.name.clone(), tool.input_schema.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut transcript_messages = Vec::new();
    let mut tool_events = Vec::new();
    let mut last_tool_batch: Option<Vec<ToolBatchExecution>> = None;
    let mut repeated_tool_batch_count = 0usize;

    for _ in 0..MAX_TOOL_LOOP_ITERATIONS {
        wait_for_rate_limit(state, &provider.id, cancellation).await?;
        ensure_execution_active(cancellation)?;
        let mut reply = run_prompt_with_cancellation(
            state,
            provider,
            &messages,
            Some(model),
            Some(session_id),
            thinking_level,
            &tools,
            cancellation,
        )
        .await?;
        if reply.remote_content.is_empty() {
            reply.remote_content =
                crate::tools::remote_content::provider_reply_remote_artifacts(&reply);
        }
        remember_remote_artifacts(context, &reply.remote_content).await?;

        if reply.tool_calls.is_empty() {
            emit_stream_event(
                emit,
                RunTaskStreamEvent::Message {
                    message: stream_session_message_from_reply(
                        session_id,
                        provider,
                        &reply,
                        MessageRole::Assistant,
                    ),
                },
            )
            .await?;
            emit_remote_content_events(emit, &reply.remote_content).await?;
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
            provider_output_items: reply.output_items.clone(),
        };
        emit_stream_event(
            emit,
            RunTaskStreamEvent::Message {
                message: stream_session_message_from_conversation(
                    session_id,
                    &provider.id,
                    &reply.model,
                    &assistant_message,
                ),
            },
        )
        .await?;
        emit_remote_content_events(emit, &reply.remote_content).await?;
        transcript_messages.push(assistant_message.clone());
        messages.push(assistant_message);

        let mut current_tool_batch = Vec::with_capacity(reply.tool_calls.len());
        for tool_call in &reply.tool_calls {
            ensure_execution_active(cancellation)?;
            let tool_execution = execute_tool_call_with_cancellation(
                context,
                tool_call,
                &allowed_tools,
                cancellation,
            )
            .await?;
            emit_stream_event(
                emit,
                RunTaskStreamEvent::Message {
                    message: stream_session_message_from_conversation(
                        session_id,
                        &provider.id,
                        &reply.model,
                        &tool_execution.message,
                    ),
                },
            )
            .await?;
            emit_remote_content_events(emit, &tool_execution.remote_content).await?;
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
                arguments: sanitize_tool_arguments(&tool_call.name, &tool_call.arguments),
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
        }
        last_tool_batch = Some(current_tool_batch);

        if repeated_tool_batch_count >= REPEATED_TOOL_BATCH_LIMIT {
            match repeated_tool_loop_resolution(
                last_tool_batch.as_deref().unwrap_or_default(),
                output_schema_json,
            ) {
                ToolLoopResolution::Success(content) => {
                    let reply = ProviderReply {
                        provider_id: reply.provider_id.clone(),
                        model: reply.model.clone(),
                        content,
                        tool_calls: Vec::new(),
                        provider_payload_json: reply.provider_payload_json.clone(),
                        output_items: Vec::new(),
                        artifacts: Vec::new(),
                        remote_content: Vec::new(),
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

fn execution_cancelled_error() -> ApiError {
    ApiError::new(StatusCode::CONFLICT, "execution cancelled by operator")
}

fn ensure_execution_active(cancellation: Option<&ExecutionCancellation>) -> Result<(), ApiError> {
    if cancellation.is_some_and(ExecutionCancellation::is_cancelled) {
        return Err(execution_cancelled_error());
    }
    Ok(())
}

async fn wait_for_rate_limit(
    state: &AppState,
    provider_id: &str,
    cancellation: Option<&ExecutionCancellation>,
) -> Result<(), ApiError> {
    if let Some(cancellation) = cancellation.cloned() {
        tokio::select! {
            _ = cancellation.cancelled() => Err(execution_cancelled_error()),
            _ = state.rate_limiter.acquire(provider_id) => Ok(()),
        }
    } else {
        state.rate_limiter.acquire(provider_id).await;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_prompt_with_cancellation(
    state: &AppState,
    provider: &ProviderConfig,
    messages: &[ConversationMessage],
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[agent_core::ToolDefinition],
    cancellation: Option<&ExecutionCancellation>,
) -> Result<ProviderReply, ApiError> {
    if let Some(cancellation) = cancellation.cloned() {
        tokio::select! {
            _ = cancellation.cancelled() => Err(execution_cancelled_error()),
            reply = execute_provider_prompt(state, provider, messages, requested_model, session_id, thinking_level, tools) => reply,
        }
    } else {
        execute_provider_prompt(
            state,
            provider,
            messages,
            requested_model,
            session_id,
            thinking_level,
            tools,
        )
        .await
    }
}

async fn execute_provider_prompt(
    state: &AppState,
    provider: &ProviderConfig,
    messages: &[ConversationMessage],
    requested_model: Option<&str>,
    session_id: Option<&str>,
    thinking_level: Option<ThinkingLevel>,
    tools: &[agent_core::ToolDefinition],
) -> Result<ProviderReply, ApiError> {
    let config = state.config.read().await.clone();
    if let Some(result) = crate::plugins::plugin_provider_prompt(
        &config,
        &provider.id,
        messages,
        requested_model,
        session_id,
        thinking_level,
        tools,
    )
    .await
    {
        return result;
    }

    run_prompt(
        &state.http_client,
        provider,
        messages,
        requested_model,
        session_id,
        thinking_level,
        tools,
    )
    .await
    .map_err(ApiError::from)
}

async fn execute_tool_call_with_cancellation(
    context: &ToolContext,
    tool_call: &ToolCall,
    allowed_tools: &std::collections::HashMap<String, serde_json::Value>,
    cancellation: Option<&ExecutionCancellation>,
) -> Result<crate::tools::ToolCallExecution, ApiError> {
    if let Some(cancellation) = cancellation.cloned() {
        tokio::select! {
            _ = cancellation.cancelled() => Err(execution_cancelled_error()),
            execution = execute_tool_call(context, tool_call, allowed_tools) => Ok(execution),
        }
    } else {
        Ok(execute_tool_call(context, tool_call, allowed_tools).await)
    }
}

async fn emit_stream_event<F, Fut>(emit: &mut F, event: RunTaskStreamEvent) -> Result<(), ApiError>
where
    F: FnMut(RunTaskStreamEvent) -> Fut,
    Fut: Future<Output = bool>,
{
    if emit(event).await {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::REQUEST_TIMEOUT,
            "stream client disconnected",
        ))
    }
}

fn stream_session_message_from_reply(
    session_id: &str,
    provider: &ProviderConfig,
    reply: &ProviderReply,
    role: MessageRole,
) -> SessionMessage {
    SessionMessage::new(
        session_id.to_string(),
        role,
        reply.content.clone(),
        Some(provider.id.clone()),
        Some(reply.model.clone()),
    )
    .with_tool_calls(sanitized_tool_calls(&reply.tool_calls))
    .with_provider_payload(reply.provider_payload_json.clone())
    .with_provider_output_items(reply_provider_output_items(reply))
}

fn stream_session_message_from_conversation(
    session_id: &str,
    provider_id: &str,
    model: &str,
    message: &ConversationMessage,
) -> SessionMessage {
    SessionMessage::new(
        session_id.to_string(),
        message.role.clone(),
        message.content.clone(),
        Some(provider_id.to_string()),
        Some(model.to_string()),
    )
    .with_tool_metadata(message.tool_call_id.clone(), message.tool_name.clone())
    .with_tool_calls(sanitized_tool_calls(&message.tool_calls))
    .with_attachments(message.attachments.clone())
    .with_provider_payload(message.provider_payload_json.clone())
    .with_provider_output_items(message.provider_output_items.clone())
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
        task_mode,
        cwd,
    } = input;
    let initial_title = is_new_session.then(|| derive_session_title(prompt));
    let mut persisted_messages = Vec::with_capacity(transcript_messages.len() + 2);
    persisted_messages.push(
        SessionMessage::new(
            session_id.to_string(),
            MessageRole::User,
            prompt.to_string(),
            Some(provider.id.clone()),
            Some(reply.model.clone()),
        )
        .with_attachments(attachments.to_vec()),
    );
    persisted_messages.extend(transcript_messages.iter().map(|message| {
        SessionMessage::new(
            session_id.to_string(),
            message.role.clone(),
            message.content.clone(),
            Some(provider.id.clone()),
            Some(reply.model.clone()),
        )
        .with_tool_metadata(message.tool_call_id.clone(), message.tool_name.clone())
        .with_tool_calls(sanitized_tool_calls(&message.tool_calls))
        .with_provider_payload(sanitized_provider_payload(
            &message.tool_calls,
            message.provider_payload_json.clone(),
        ))
        .with_attachments(message.attachments.clone())
        .with_provider_output_items(message.provider_output_items.clone())
    }));
    persisted_messages.push(
        SessionMessage::new(
            session_id.to_string(),
            MessageRole::Assistant,
            reply.content.clone(),
            Some(provider.id.clone()),
            Some(reply.model.clone()),
        )
        .with_tool_calls(sanitized_tool_calls(&reply.tool_calls))
        .with_provider_payload(sanitized_provider_payload(
            &reply.tool_calls,
            reply.provider_payload_json.clone(),
        ))
        .with_provider_output_items(reply_provider_output_items(reply)),
    );
    state
        .storage
        .persist_session_turn(PersistSessionTurnInput {
            session_id,
            title: initial_title.as_deref(),
            alias,
            provider_id: &provider.id,
            model: &reply.model,
            task_mode,
            cwd: Some(cwd.as_path()),
            messages: &persisted_messages,
        })?;
    Ok(())
}

fn sanitized_tool_calls(tool_calls: &[ToolCall]) -> Vec<ToolCall> {
    tool_calls.iter().map(sanitize_tool_call).collect()
}

fn sanitized_provider_payload(
    tool_calls: &[ToolCall],
    provider_payload_json: Option<String>,
) -> Option<String> {
    if tool_calls.iter().any(tool_call_has_sensitive_arguments) {
        None
    } else {
        provider_payload_json
    }
}

fn reply_provider_output_items(reply: &ProviderReply) -> Vec<ProviderOutputItem> {
    let mut items = reply.output_items.clone();
    items.extend(
        reply
            .remote_content
            .iter()
            .cloned()
            .map(|artifact| ProviderOutputItem::RemoteContent { artifact }),
    );
    items
}

async fn remember_remote_artifacts(
    context: &ToolContext,
    artifacts: &[RemoteContentArtifact],
) -> Result<(), ApiError> {
    for artifact in artifacts {
        crate::tools::remote_content::remember_remote_artifact(context, artifact).await?;
    }
    Ok(())
}

async fn emit_remote_content_events<F, Fut>(
    emit: &mut F,
    artifacts: &[RemoteContentArtifact],
) -> Result<(), ApiError>
where
    F: FnMut(RunTaskStreamEvent) -> Fut,
    Fut: Future<Output = bool>,
{
    for artifact in artifacts {
        emit_stream_event(
            emit,
            RunTaskStreamEvent::RemoteContent {
                artifact: artifact.clone(),
            },
        )
        .await?;
    }
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
            provider_output_items: message.provider_output_items,
        })
        .collect())
}

fn effective_task_mode(
    requested: Option<TaskMode>,
    session: Option<&SessionSummary>,
) -> Option<TaskMode> {
    requested.or_else(|| session.and_then(|summary| summary.task_mode))
}

fn effective_session_cwd(
    requested: Option<PathBuf>,
    session: Option<&SessionSummary>,
) -> Option<PathBuf> {
    requested.or_else(|| session.and_then(|summary| summary.cwd.clone()))
}

#[allow(clippy::too_many_arguments)]
async fn tool_context(
    state: &AppState,
    alias: &ModelAlias,
    provider: &ProviderConfig,
    model: &str,
    cwd: PathBuf,
    permission_preset: PermissionPreset,
    thinking_level: Option<ThinkingLevel>,
    task_mode: Option<TaskMode>,
    background: bool,
    delegation_depth: u8,
    remote_content_policy: RemoteContentPolicy,
    allowed_direct_urls: std::collections::HashSet<String>,
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
    let plugin_tools = collect_hosted_plugin_tools(&config);
    let model_capabilities = agent_providers::describe_model(provider, model).capabilities;
    ToolContext {
        state: state.clone(),
        cwd,
        trust_policy: config.trust_policy.clone(),
        autonomy: config.autonomy.clone(),
        permission_preset,
        http_client: state.http_client.clone(),
        mcp_servers: config.mcp_servers.clone(),
        app_connectors: config.app_connectors.clone(),
        plugin_tools,
        brave_connectors: config.brave_connectors.clone(),
        current_alias: Some(alias.alias.clone()),
        default_thinking_level: thinking_level,
        task_mode,
        delegation,
        delegation_targets,
        delegation_depth,
        background,
        background_shell_allowed,
        background_network_allowed,
        background_self_edit_allowed,
        model_capabilities,
        remote_content_policy,
        remote_content_state: std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::tools::remote_content::RemoteContentRuntimeState::default(),
        )),
        allowed_direct_urls: std::sync::Arc::new(allowed_direct_urls),
    }
}

async fn resolve_remote_content_policy(
    state: &AppState,
    requested: Option<RemoteContentPolicy>,
) -> RemoteContentPolicy {
    if let Some(policy) = requested {
        return policy;
    }
    let config = state.config.read().await;
    config.remote_content_policy
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
    task_mode: Option<TaskMode>,
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
    let task_mode_hint = task_mode.map(|task_mode| {
        format!(
            " Current task mode: {}. {}",
            task_mode.as_str(),
            match task_mode {
                TaskMode::Build => {
                    "Prioritize repo inspection, precise edits, verification, and concise engineering summaries."
                }
                TaskMode::Daily => {
                    "Prioritize research, planning, writing, follow-through, and practical next steps."
                }
            }
        )
    });
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
    let remote_content_hint = format!(
        " {}",
        crate::tools::remote_content::remote_content_system_guidance()
    );
    ConversationMessage {
        role: MessageRole::System,
        content: format!(
            "You are a local work agent running in {}. Handle coding and repo tasks with rigorous engineering discipline, and handle everyday tasks such as research, planning, writing, and operational follow-up with concise, practical help. Use the available tools when you need filesystem, git, environment, shell, or network access. For code changes, prefer apply_patch for precise edits and write_file only for full rewrites or new files. Prefer accurate tool use over guessing. Do not repeat an identical successful tool call batch; after a successful change, summarize completion instead of calling the same tool again.{}{}{}{}{}{}{}{}",
            cwd.display(),
            thinking_hint.unwrap_or_default(),
            permission_hint,
            task_mode_hint.unwrap_or_default(),
            structured_output_hint.unwrap_or_default(),
            agents_hint,
            skills_hint,
            delegation_hint,
            remote_content_hint,
        ),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: Vec::new(),
        provider_output_items: Vec::new(),
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

        let Ok(content) = fs::read_to_string(&file) else {
            continue;
        };
        let content = truncate_with_suffix(&content, MAX_FILE_BYTES, "\n\n[truncated]");

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

#[cfg(test)]
mod tests {
    use super::{
        effective_session_cwd, effective_task_mode, reply_provider_output_items,
        sanitized_provider_payload, sanitized_tool_calls, system_message,
    };
    use agent_core::{
        MessageRole, PermissionPreset, ProviderOutputItem, ProviderReply, RemoteContentArtifact,
        RemoteContentAssessment, RemoteContentRisk, RemoteContentSource, RemoteContentSourceKind,
        SessionSummary, TaskMode, ToolCall,
    };
    use chrono::Utc;
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    fn session_summary(task_mode: Option<TaskMode>, cwd: Option<&str>) -> SessionSummary {
        SessionSummary {
            id: "session-1".to_string(),
            title: Some("Test".to_string()),
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            task_mode,
            message_count: 0,
            cwd: cwd.map(PathBuf::from),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn sanitized_tool_calls_redacts_secret_fields() {
        let tool_calls = vec![ToolCall {
            id: "call-1".to_string(),
            name: "configure_telegram_connector".to_string(),
            arguments: r#"{"bot_token":"123:abc","id":"telegram-main"}"#.to_string(),
        }];

        let sanitized = sanitized_tool_calls(&tool_calls);

        let parsed: Value = serde_json::from_str(&sanitized[0].arguments).unwrap();
        assert_eq!(parsed["bot_token"], Value::String("[REDACTED]".to_string()));
        assert_eq!(parsed["id"], Value::String("telegram-main".to_string()));
    }

    #[test]
    fn sanitized_provider_payload_drops_raw_payload_for_sensitive_tool_calls() {
        let tool_calls = vec![ToolCall {
            id: "call-1".to_string(),
            name: "configure_home_assistant_connector".to_string(),
            arguments: r#"{"access_token":"secret-token"}"#.to_string(),
        }];

        assert_eq!(
            sanitized_provider_payload(&tool_calls, Some("{\"raw\":true}".to_string())),
            None
        );
    }

    #[test]
    fn sanitized_provider_payload_keeps_non_sensitive_payloads() {
        let tool_calls = vec![ToolCall {
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"path":"README.md"}"#.to_string(),
        }];

        assert_eq!(
            sanitized_provider_payload(&tool_calls, Some("{\"raw\":true}".to_string())),
            Some("{\"raw\":true}".to_string())
        );
    }

    #[test]
    fn reply_provider_output_items_includes_remote_content_artifacts() {
        let reply = ProviderReply {
            provider_id: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            content: "done".to_string(),
            tool_calls: Vec::new(),
            provider_payload_json: None,
            output_items: vec![ProviderOutputItem::Message {
                role: MessageRole::Assistant,
                content: "hello".to_string(),
            }],
            artifacts: Vec::new(),
            remote_content: vec![RemoteContentArtifact {
                id: "artifact-1".to_string(),
                source: RemoteContentSource {
                    kind: RemoteContentSourceKind::WebPage,
                    label: Some("Example".to_string()),
                    url: Some("https://example.com".to_string()),
                    host: Some("example.com".to_string()),
                },
                title: Some("Example".to_string()),
                mime_type: Some("text/plain".to_string()),
                excerpt: Some("content".to_string()),
                content_sha256: Some("abc".to_string()),
                assessment: RemoteContentAssessment {
                    risk: RemoteContentRisk::Medium,
                    blocked: false,
                    reasons: vec!["reason".to_string()],
                    warnings: vec!["warning".to_string()],
                },
            }],
        };

        let items = reply_provider_output_items(&reply);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            items.last(),
            Some(ProviderOutputItem::RemoteContent { .. })
        ));
    }

    #[test]
    fn system_message_frames_everyday_tasks_as_first_class() {
        let message = system_message(
            Path::new("."),
            None,
            PermissionPreset::AutoEdit,
            None,
            None,
            "",
            "",
        );

        assert!(message.content.contains("local work agent"));
        assert!(message.content.contains("coding and repo tasks"));
        assert!(message
            .content
            .contains("research, planning, writing, and operational follow-up"));
    }

    #[test]
    fn system_message_includes_build_mode_hint() {
        let message = system_message(
            Path::new("."),
            None,
            PermissionPreset::AutoEdit,
            Some(TaskMode::Build),
            None,
            "",
            "",
        );

        assert!(message.content.contains("Current task mode: build."));
        assert!(message
            .content
            .contains("Prioritize repo inspection, precise edits, verification"));
    }

    #[test]
    fn system_message_includes_daily_mode_hint() {
        let message = system_message(
            Path::new("."),
            None,
            PermissionPreset::AutoEdit,
            Some(TaskMode::Daily),
            None,
            "",
            "",
        );

        assert!(message.content.contains("Current task mode: daily."));
        assert!(message
            .content
            .contains("Prioritize research, planning, writing, follow-through"));
    }

    #[test]
    fn effective_task_mode_prefers_explicit_request_over_session() {
        let session = session_summary(Some(TaskMode::Daily), Some("J:/repo"));
        assert_eq!(
            effective_task_mode(Some(TaskMode::Build), Some(&session)),
            Some(TaskMode::Build)
        );
    }

    #[test]
    fn effective_task_mode_falls_back_to_session_setting() {
        let session = session_summary(Some(TaskMode::Daily), Some("J:/repo"));
        assert_eq!(
            effective_task_mode(None, Some(&session)),
            Some(TaskMode::Daily)
        );
    }

    #[test]
    fn effective_session_cwd_falls_back_to_session_cwd() {
        let session = session_summary(Some(TaskMode::Daily), Some("J:/repo"));
        assert_eq!(
            effective_session_cwd(None, Some(&session)),
            Some(PathBuf::from("J:/repo"))
        );
    }
}
