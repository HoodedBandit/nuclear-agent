use super::*;
use crate::{
    anthropic::messages_to_anthropic,
    chatgpt_codex::{chatgpt_codex_payload, run_chatgpt_codex},
    chatgpt_codex_catalog::{
        model_descriptor_from_chatgpt_codex_record, resolve_chatgpt_codex_model_descriptor,
        ChatGptCodexModelRecord,
    },
    models::validate_default_model,
    oauth::{parse_oauth_token, refresh_oauth_token},
    ollama::{messages_to_ollama, parse_ollama_tool_calls},
    openai_compatible::messages_to_openai,
    tools::validate_tool_definitions,
};
use agent_core::{
    AttachmentKind, ConversationMessage, InputAttachment, KeyValuePair, MessageRole, OAuthConfig,
    ToolDefinition,
};
use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

#[test]
fn builds_oauth_authorization_url() {
    let provider = ProviderConfig {
        id: "test".to_string(),
        display_name: "Test".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "https://example.com/v1".to_string(),
        auth_mode: AuthMode::OAuth,
        default_model: Some("model".to_string()),
        keychain_account: None,
        oauth: Some(OAuthConfig {
            client_id: "client".to_string(),
            authorization_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec!["profile".to_string(), "offline_access".to_string()],
            extra_authorize_params: vec![KeyValuePair {
                key: "audience".to_string(),
                value: "nuclear".to_string(),
            }],
            extra_token_params: Vec::new(),
        }),
        local: false,
    };

    let url = build_oauth_authorization_url(
        &provider,
        "http://127.0.0.1:1234/callback",
        "state",
        "challenge",
    )
    .unwrap();

    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=client"));
    assert!(url.contains("code_challenge=challenge"));
    assert!(url.contains("audience=nuclear"));
}

#[test]
fn parses_expires_in_from_string() {
    let value = json!({
        "access_token": "abc",
        "expires_in": "90"
    });
    let oauth = OAuthConfig {
        client_id: "client".to_string(),
        authorization_url: "https://auth.example.com/authorize".to_string(),
        token_url: "https://auth.example.com/token".to_string(),
        scopes: Vec::new(),
        extra_authorize_params: Vec::new(),
        extra_token_params: Vec::new(),
    };

    let token = parse_oauth_token(&oauth, &value).unwrap();
    assert_eq!(token.access_token, "abc");
    assert!(token.expires_at.is_some());
}

#[test]
fn local_openai_provider_can_fallback_when_models_endpoint_is_missing() {
    let provider = ProviderConfig {
        id: "local".to_string(),
        display_name: "Local".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://127.0.0.1:5001/v1".to_string(),
        auth_mode: AuthMode::None,
        default_model: Some("kobold".to_string()),
        keychain_account: None,
        oauth: None,
        local: true,
    };

    assert!(supports_local_model_listing_fallback(
        &provider,
        StatusCode::NOT_FOUND
    ));
    assert!(!supports_local_model_listing_fallback(
        &provider,
        StatusCode::UNAUTHORIZED
    ));
}

#[test]
fn provider_endpoint_url_rejects_remote_http_before_authenticated_requests() {
    let provider = ProviderConfig {
        id: "remote-http".to_string(),
        display_name: "Remote HTTP".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://example.invalid/v1".to_string(),
        auth_mode: AuthMode::ApiKey,
        default_model: Some("model".to_string()),
        keychain_account: Some("remote-http".to_string()),
        oauth: None,
        local: false,
    };

    let error = provider_endpoint_url(&provider, "models", "models").unwrap_err();

    assert!(error.to_string().contains("https"));
}

#[test]
fn provider_endpoint_url_preserves_remote_http_for_unauthenticated_providers() {
    let provider = ProviderConfig {
        id: "remote-http".to_string(),
        display_name: "Remote HTTP".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "http://example.invalid/v1".to_string(),
        auth_mode: AuthMode::None,
        default_model: Some("model".to_string()),
        keychain_account: None,
        oauth: None,
        local: false,
    };

    let url = provider_endpoint_url(&provider, "models", "models").unwrap();

    assert_eq!(url.as_str(), "http://example.invalid/v1/models");
}

#[test]
fn provider_endpoint_url_allows_https_and_loopback_http() {
    let mut provider = ProviderConfig {
        id: "secure".to_string(),
        display_name: "Secure".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "https://api.example.invalid/v1".to_string(),
        auth_mode: AuthMode::ApiKey,
        default_model: Some("model".to_string()),
        keychain_account: Some("secure".to_string()),
        oauth: None,
        local: false,
    };

    let https = provider_endpoint_url(&provider, "models", "models").unwrap();
    assert_eq!(https.as_str(), "https://api.example.invalid/v1/models");

    provider.base_url = "http://127.0.0.1:8080/v1".to_string();
    let loopback = provider_endpoint_url(&provider, "models", "models").unwrap();
    assert_eq!(loopback.as_str(), "http://127.0.0.1:8080/v1/models");
}

#[test]
fn validate_default_model_accepts_available_model() {
    let provider = ProviderConfig {
        id: "local".to_string(),
        display_name: "Local".to_string(),
        kind: ProviderKind::Ollama,
        base_url: "http://127.0.0.1:11434".to_string(),
        auth_mode: AuthMode::None,
        default_model: Some("qwen".to_string()),
        keychain_account: None,
        oauth: None,
        local: true,
    };

    assert!(validate_default_model(&provider, &["qwen".to_string(), "llama".to_string()]).is_ok());
}

#[test]
fn validate_default_model_rejects_missing_model() {
    let provider = ProviderConfig {
        id: "local".to_string(),
        display_name: "Local".to_string(),
        kind: ProviderKind::Ollama,
        base_url: "http://127.0.0.1:11434".to_string(),
        auth_mode: AuthMode::None,
        default_model: Some("llama3.2".to_string()),
        keychain_account: None,
        oauth: None,
        local: true,
    };

    let error = validate_default_model(
        &provider,
        &["qwen3.5:9b".to_string(), "qwen3.5:4b".to_string()],
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("default model 'llama3.2' not available"));
}

#[test]
fn openai_tool_request_and_response_are_supported() {
    let (base_url, request_rx) = spawn_json_server(json!({
        "choices": [{
            "message": {
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"Cargo.toml\"}"
                    }
                }]
            }
        }]
    }));

    let provider = ProviderConfig {
        id: "openai".to_string(),
        display_name: "OpenAI".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url,
        auth_mode: AuthMode::None,
        default_model: Some("gpt-test".to_string()),
        keychain_account: None,
        oauth: None,
        local: true,
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let reply = runtime
        .block_on(run_prompt(
            &Client::new(),
            &provider,
            &[ConversationMessage {
                role: MessageRole::User,
                content: "Inspect the file".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
            }],
            Some("gpt-test"),
            None,
            None,
            &[ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }),
                backend: ToolBackend::LocalFunction,
                hosted_kind: None,
                strict_schema: true,
            }],
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.contains("\"tools\""));
    assert!(request.contains("\"read_file\""));
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].name, "read_file");
}

#[test]
fn openai_compatible_requests_include_reasoning_effort() {
    let (base_url, request_rx) = spawn_json_server(json!({
        "choices": [{
            "message": {
                "content": "done"
            }
        }]
    }));

    let provider = ProviderConfig {
        id: "openai".to_string(),
        display_name: "OpenAI".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url,
        auth_mode: AuthMode::None,
        default_model: Some("gpt-test".to_string()),
        keychain_account: None,
        oauth: None,
        local: true,
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_prompt(
            &Client::new(),
            &provider,
            &[ConversationMessage {
                role: MessageRole::User,
                content: "Think carefully".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
            }],
            Some("gpt-test"),
            None,
            Some(ThinkingLevel::High),
            &[],
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.contains("\"reasoning_effort\":\"high\""));
}

#[test]
fn openrouter_requests_use_reasoning_object() {
    let (base_url, request_rx) = spawn_json_server(json!({
        "choices": [{
            "message": {
                "content": "done"
            }
        }]
    }));

    let provider = ProviderConfig {
        id: "openrouter".to_string(),
        display_name: "OpenRouter".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: base_url.replace("/token", "/api/v1"),
        auth_mode: AuthMode::None,
        default_model: Some("openai/gpt-4.1".to_string()),
        keychain_account: None,
        oauth: None,
        local: false,
    };

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(run_prompt(
            &Client::new(),
            &provider,
            &[ConversationMessage {
                role: MessageRole::User,
                content: "Think carefully".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
            }],
            Some("openai/gpt-4.1"),
            None,
            Some(ThinkingLevel::Medium),
            &[],
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.contains("\"reasoning\":{\"effort\":\"medium\"}"));
}

#[test]
fn chatgpt_codex_lists_models_with_browser_session_headers() {
    let (base_url, request_rx) = spawn_response_server_at(
        "/backend-api/codex",
        "200 OK",
        "application/json",
        &json!({
            "models": [{
                "slug": "gpt-5",
                "display_name": "GPT-5",
                "description": "desc",
                "default_reasoning_level": "medium",
                "supported_reasoning_levels": [
                    {"effort": "low", "description": "low"},
                    {"effort": "medium", "description": "medium"},
                    {"effort": "high", "description": "high"}
                ],
                "shell_type": "shell_command",
                "visibility": "list",
                "supported_in_api": true,
                "priority": 1,
                "availability_nux": null,
                "upgrade": null,
                "base_instructions": "base instructions",
                "model_messages": null,
                "supports_reasoning_summaries": false,
                "default_reasoning_summary": "auto",
                "support_verbosity": false,
                "default_verbosity": null,
                "apply_patch_tool_type": null,
                "web_search_tool_type": "web_search_preview",
                "truncation_policy": {"mode": "bytes", "limit": 10000},
                "supports_parallel_tool_calls": true,
                "supports_image_detail_original": false,
                "context_window": 272000,
                "auto_compact_token_limit": null,
                "effective_context_window_percent": 90,
                "experimental_supported_tools": [],
                "input_modalities": ["text"],
                "prefer_websockets": false
            }]
        })
        .to_string(),
    );

    let provider = ProviderConfig {
        id: "openai-browser".to_string(),
        display_name: "OpenAI Browser Session".to_string(),
        kind: ProviderKind::ChatGptCodex,
        base_url,
        auth_mode: AuthMode::OAuth,
        default_model: None,
        keychain_account: None,
        oauth: Some(openai_browser_test_oauth_config()),
        local: false,
    };
    let token = OAuthToken {
        access_token: "session-token".to_string(),
        refresh_token: None,
        expires_at: None,
        token_type: Some("Bearer".to_string()),
        scopes: Vec::new(),
        id_token: None,
        account_id: Some("acct-123".to_string()),
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    };

    let models = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(list_models_with_overrides(
            &Client::new(),
            &provider,
            None,
            Some(&token),
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    let request_lower = request.to_ascii_lowercase();
    assert!(request.starts_with("GET /backend-api/codex/models?client_version="));
    assert!(request_lower.contains("authorization: bearer session-token"));
    assert!(request_lower.contains("chatgpt-account-id: acct-123"));
    assert!(models.iter().any(|model| model == "gpt-5"));
}

#[test]
fn chatgpt_codex_model_descriptors_include_context_window_metadata() {
    let (base_url, _request_rx) = spawn_response_server_at(
        "/backend-api/codex",
        "200 OK",
        "application/json",
        &json!({
            "models": [{
                "slug": "gpt-5",
                "display_name": "GPT-5",
                "context_window": 272000,
                "effective_context_window_percent": 90
            }]
        })
        .to_string(),
    );

    let provider = ProviderConfig {
        id: "openai-browser".to_string(),
        display_name: "OpenAI Browser Session".to_string(),
        kind: ProviderKind::ChatGptCodex,
        base_url,
        auth_mode: AuthMode::OAuth,
        default_model: None,
        keychain_account: None,
        oauth: Some(openai_browser_test_oauth_config()),
        local: false,
    };
    let token = OAuthToken {
        access_token: "session-token".to_string(),
        refresh_token: None,
        expires_at: None,
        token_type: Some("Bearer".to_string()),
        scopes: Vec::new(),
        id_token: None,
        account_id: Some("acct-123".to_string()),
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    };

    let models = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(list_model_descriptors_with_overrides(
            &Client::new(),
            &provider,
            None,
            Some(&token),
        ))
        .unwrap();

    let model = models
        .iter()
        .find(|model| model.id == "gpt-5")
        .expect("merged model list should include gpt-5");
    assert_eq!(model.display_name.as_deref(), Some("GPT-5"));
    assert_eq!(model.context_window, Some(272000));
    assert_eq!(model.effective_context_window_percent, Some(90));
}

#[test]
fn chatgpt_codex_run_prompt_supports_tool_calls() {
    let body = build_sse_body(&[
        json!({
            "type": "response.output_item.done",
            "item": {
                "type": "function_call",
                "name": "read_file",
                "arguments": "{\"path\":\"Cargo.toml\"}",
                "call_id": "call_1"
            }
        }),
        json!({
            "type": "response.completed",
            "response": { "id": "resp_1" }
        }),
    ]);
    let (base_url, request_rx) =
        spawn_response_server_at("/backend-api/codex", "200 OK", "text/event-stream", &body);

    let provider = ProviderConfig {
        id: "openai-browser".to_string(),
        display_name: "OpenAI Browser Session".to_string(),
        kind: ProviderKind::ChatGptCodex,
        base_url,
        auth_mode: AuthMode::OAuth,
        default_model: Some("gpt-5".to_string()),
        keychain_account: None,
        oauth: Some(openai_browser_test_oauth_config()),
        local: false,
    };
    let token = OAuthToken {
        access_token: "session-token".to_string(),
        refresh_token: None,
        expires_at: None,
        token_type: Some("Bearer".to_string()),
        scopes: Vec::new(),
        id_token: None,
        account_id: Some("acct-123".to_string()),
        user_id: None,
        org_id: None,
        project_id: None,
        display_email: None,
        subscription_type: None,
    };

    let reply = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(run_chatgpt_codex(
            &Client::new(),
            &provider,
            "gpt-5",
            &[ConversationMessage {
                role: MessageRole::User,
                content: "Inspect Cargo.toml".to_string(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: Vec::new(),
                provider_payload_json: None,
                attachments: Vec::new(),
                provider_output_items: Vec::new(),
            }],
            Some("session-123"),
            None,
            &[ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }),
                backend: ToolBackend::LocalFunction,
                hosted_kind: None,
                strict_schema: true,
            }],
            Some(&token),
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.starts_with("POST /backend-api/codex/responses "));
    assert!(request
        .to_ascii_lowercase()
        .contains("session_id: session-123"));
    assert!(request
        .to_ascii_lowercase()
        .contains("user-agent: codex_cli_rs/"));
    assert!(request
        .to_ascii_lowercase()
        .contains("chatgpt-account-id: acct-123"));
    assert!(request.contains("\"type\":\"message\""));
    assert!(request.contains("\"role\":\"user\""));
    assert!(request.contains("\"type\":\"input_text\""));
    assert!(request.contains("\"tool_choice\":\"auto\""));
    assert!(request.contains("\"parallel_tool_calls\":true"));
    assert!(request.contains("\"read_file\""));
    assert_eq!(reply.content, "");
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].id, "call_1");
    assert_eq!(reply.tool_calls[0].name, "read_file");
    let payload: Value =
        serde_json::from_str(reply.provider_payload_json.as_deref().unwrap()).unwrap();
    assert_eq!(payload[0]["type"], "function_call");
    assert_eq!(payload[0]["call_id"], "call_1");
}

#[test]
fn chatgpt_codex_payload_includes_responses_defaults_without_tools() {
    let payload = chatgpt_codex_payload(
        "gpt-5",
        &[ConversationMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        }],
        None,
        &[],
        None,
    )
    .unwrap();

    assert_eq!(payload["tools"], Value::Array(Vec::new()));
    assert_eq!(payload["tool_choice"], Value::String("auto".to_string()));
    assert_eq!(payload["parallel_tool_calls"], Value::Bool(false));
    assert_eq!(payload["include"], Value::Array(Vec::new()));
}

#[test]
fn chatgpt_codex_payload_uses_responses_api_tool_shape() {
    let payload = chatgpt_codex_payload(
        "gpt-5",
        &[ConversationMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        }],
        None,
        &[ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            backend: ToolBackend::LocalFunction,
            hosted_kind: None,
            strict_schema: true,
        }],
        None,
    )
    .unwrap();

    assert_eq!(payload["tools"][0]["type"], "function");
    assert_eq!(payload["tools"][0]["name"], "read_file");
    assert_eq!(payload["tools"][0]["description"], "Read a file");
    assert_eq!(payload["tools"][0]["strict"], true);
    assert_eq!(payload["tools"][0]["parameters"]["type"], "object");
    assert!(payload["tools"][0].get("function").is_none());
}

#[test]
fn chatgpt_codex_payload_uses_provider_builtin_web_search_tool_shape() {
    let payload = chatgpt_codex_payload(
        "gpt-5",
        &[ConversationMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        }],
        None,
        &[ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            backend: ToolBackend::ProviderBuiltin,
            hosted_kind: Some(HostedToolKind::WebSearch),
            strict_schema: false,
        }],
        None,
    )
    .unwrap();

    assert_eq!(payload["tools"][0]["type"], "web_search");
    assert!(payload["tools"][0].get("name").is_none());
    assert!(payload["tools"][0].get("parameters").is_none());
}

#[test]
fn chatgpt_codex_payload_uses_bundled_model_metadata_for_newer_models() {
    let descriptor = resolve_chatgpt_codex_model_descriptor("gpt-5.4")
        .expect("bundled model catalog should include gpt-5.4");
    let payload = chatgpt_codex_payload(
        "gpt-5.4",
        &[ConversationMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        }],
        None,
        &[],
        Some(&descriptor),
    )
    .unwrap();

    assert_eq!(
        payload["reasoning"]["effort"],
        descriptor.default_reasoning_effort.as_deref().unwrap()
    );
    assert!(payload["reasoning"].get("summary").is_none());
    assert_eq!(
        payload["include"],
        Value::Array(vec![Value::String(
            "reasoning.encrypted_content".to_string()
        )])
    );
    assert_eq!(
        payload["text"]["verbosity"],
        descriptor.default_verbosity.as_deref().unwrap()
    );
}

#[test]
fn chatgpt_codex_model_descriptor_normalizes_summary_and_verbosity_defaults() {
    let descriptor = model_descriptor_from_chatgpt_codex_record(ChatGptCodexModelRecord {
        slug: "gpt-test".to_string(),
        supports_reasoning_summaries: Some(true),
        default_reasoning_summary: Some("none".to_string()),
        support_verbosity: Some(true),
        default_verbosity: Some("loud".to_string()),
        ..Default::default()
    });

    assert_eq!(descriptor.default_reasoning_summary, None);
    assert_eq!(descriptor.default_verbosity, None);
}

#[test]
fn chatgpt_codex_payload_omits_invalid_reasoning_and_text_defaults() {
    let descriptor = ModelDescriptor {
        id: "gpt-test".to_string(),
        display_name: None,
        description: None,
        context_window: None,
        effective_context_window_percent: None,
        show_in_picker: true,
        default_reasoning_effort: None,
        supported_reasoning_levels: Vec::new(),
        supports_reasoning_summaries: true,
        default_reasoning_summary: Some("none".to_string()),
        support_verbosity: true,
        default_verbosity: Some("unsupported".to_string()),
        supports_parallel_tool_calls: true,
        priority: None,
        capabilities: ModelToolCapabilities::default(),
    };
    let payload = chatgpt_codex_payload(
        "gpt-test",
        &[ConversationMessage {
            role: MessageRole::User,
            content: "hello".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        }],
        None,
        &[],
        Some(&descriptor),
    )
    .unwrap();

    assert!(payload.get("reasoning").is_none());
    assert!(payload.get("text").is_none());
}

#[test]
fn validate_tool_definitions_rejects_missing_name() {
    let error = validate_tool_definitions(
        &[ToolDefinition {
            name: "   ".to_string(),
            description: "broken".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            backend: ToolBackend::LocalFunction,
            hosted_kind: None,
            strict_schema: true,
        }],
        "ChatGPT/Codex",
    )
    .unwrap_err();

    assert!(error.to_string().contains("missing a name"));
}

#[test]
fn validate_tool_definitions_rejects_non_object_schema() {
    let error = validate_tool_definitions(
        &[ToolDefinition {
            name: "read_file".to_string(),
            description: "broken".to_string(),
            input_schema: json!(["not", "an", "object"]),
            backend: ToolBackend::LocalFunction,
            hosted_kind: None,
            strict_schema: true,
        }],
        "ChatGPT/Codex",
    )
    .unwrap_err();

    assert!(error.to_string().contains("object JSON schema"));
}

#[test]
fn anthropic_message_encoding_supports_tool_use_and_results() {
    let messages = messages_to_anthropic(&[
        ConversationMessage {
            role: MessageRole::User,
            content: "Find the file".to_string(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        },
        ConversationMessage {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: vec![ToolCall {
                id: "toolu_1".to_string(),
                name: "read_file".to_string(),
                arguments: "{\"path\":\"src/main.rs\"}".to_string(),
            }],
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        },
        ConversationMessage {
            role: MessageRole::Tool,
            content: "1: fn main() {}".to_string(),
            tool_call_id: Some("toolu_1".to_string()),
            tool_name: Some("read_file".to_string()),
            tool_calls: Vec::new(),
            provider_payload_json: None,
            attachments: Vec::new(),
            provider_output_items: Vec::new(),
        },
    ])
    .unwrap();

    assert_eq!(messages[1]["content"][0]["type"], "tool_use");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(messages[2]["content"][0]["tool_use_id"], "toolu_1");
}

#[test]
fn openai_message_encoding_supports_image_attachments() {
    let image = TestImageFile::new("png", &[1, 2, 3]);
    let messages = messages_to_openai(&[ConversationMessage {
        role: MessageRole::User,
        content: "Describe this".to_string(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: vec![image.attachment()],
        provider_output_items: Vec::new(),
    }])
    .unwrap();

    assert_eq!(messages[0]["content"][0]["type"], "text");
    assert_eq!(messages[0]["content"][1]["type"], "image_url");
    assert_eq!(
        messages[0]["content"][1]["image_url"]["url"],
        "data:image/png;base64,AQID"
    );
}

#[test]
fn anthropic_message_encoding_supports_image_attachments() {
    let image = TestImageFile::new("jpg", &[1, 2, 3]);
    let messages = messages_to_anthropic(&[ConversationMessage {
        role: MessageRole::User,
        content: "Describe this".to_string(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: vec![image.attachment()],
        provider_output_items: Vec::new(),
    }])
    .unwrap();

    assert_eq!(messages[0]["content"][0]["type"], "text");
    assert_eq!(messages[0]["content"][1]["type"], "image");
    assert_eq!(
        messages[0]["content"][1]["source"]["media_type"],
        "image/jpeg"
    );
    assert_eq!(messages[0]["content"][1]["source"]["data"], "AQID");
}

#[test]
fn ollama_message_encoding_supports_image_attachments() {
    let image = TestImageFile::new("webp", &[1, 2, 3]);
    let messages = messages_to_ollama(&[ConversationMessage {
        role: MessageRole::User,
        content: "Describe this".to_string(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: Vec::new(),
        provider_payload_json: None,
        attachments: vec![image.attachment()],
        provider_output_items: Vec::new(),
    }])
    .unwrap();

    assert_eq!(messages[0]["images"][0], "AQID");
}

#[test]
fn ollama_tool_calls_get_generated_ids_when_missing() {
    let tool_calls = parse_ollama_tool_calls(&json!({
        "tool_calls": [{
            "function": {
                "name": "search_files",
                "arguments": {"query": "main"}
            }
        }]
    }))
    .unwrap();

    assert_eq!(tool_calls[0].id, "ollama-tool-1");
    assert_eq!(tool_calls[0].name, "search_files");
}

#[test]
fn exchanges_oauth_code_against_token_endpoint() {
    let (token_url, request_rx) = spawn_json_server(json!({
        "access_token": "access-123",
        "refresh_token": "refresh-123",
        "expires_in": 120,
        "token_type": "Bearer",
        "scope": "profile offline_access"
    }));
    let provider = oauth_provider(token_url);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let token = runtime
        .block_on(exchange_oauth_code(
            &Client::new(),
            &provider,
            "code-123",
            "verifier-123",
            "http://127.0.0.1:8080/callback",
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.contains("grant_type=authorization_code"));
    assert!(request.contains("code=code-123"));
    assert!(request.contains("code_verifier=verifier-123"));
    assert!(request.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8080%2Fcallback"));
    assert_eq!(token.access_token, "access-123");
    assert_eq!(token.refresh_token.as_deref(), Some("refresh-123"));
    assert_eq!(token.token_type.as_deref(), Some("Bearer"));
    assert_eq!(token.scopes, vec!["profile", "offline_access"]);
}

#[test]
fn oauth_token_exchange_surfaces_error_description() {
    let (token_url, _request_rx) = spawn_response_server(
        "400 Bad Request",
        "application/json",
        r#"{"error":"access_denied","error_description":"unknown authentication error"}"#,
    );
    let provider = oauth_provider(token_url);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let error = runtime
        .block_on(exchange_oauth_code(
            &Client::new(),
            &provider,
            "code-123",
            "verifier-123",
            "http://127.0.0.1:8080/callback",
        ))
        .unwrap_err();

    assert!(error.to_string().contains("unknown authentication error"));
}

#[test]
fn oauth_token_exchange_surfaces_plain_text_errors() {
    let (token_url, _request_rx) =
        spawn_response_server("502 Bad Gateway", "text/plain", "service unavailable");
    let provider = oauth_provider(token_url);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let error = runtime
        .block_on(exchange_oauth_code(
            &Client::new(),
            &provider,
            "code-123",
            "verifier-123",
            "http://127.0.0.1:8080/callback",
        ))
        .unwrap_err();

    assert!(error.to_string().contains("service unavailable"));
}

#[test]
fn oauth_authorization_url_rejects_remote_http_endpoint() {
    let provider = oauth_provider_with_endpoints(
        "http://example.com/authorize".to_string(),
        "https://auth.example.com/token".to_string(),
    );

    let error = build_oauth_authorization_url(
        &provider,
        "http://127.0.0.1:8080/callback",
        "state",
        "challenge",
    )
    .unwrap_err();

    assert!(error.to_string().contains("authorization_url"));
    assert!(error.to_string().contains("must use https"));
}

#[test]
fn oauth_token_exchange_rejects_remote_http_token_endpoint() {
    let provider = oauth_provider_with_endpoints(
        "https://auth.example.com/authorize".to_string(),
        "http://example.com/token".to_string(),
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let error = runtime
        .block_on(exchange_oauth_code(
            &Client::new(),
            &provider,
            "code-123",
            "verifier-123",
            "http://127.0.0.1:8080/callback",
        ))
        .unwrap_err();

    assert!(error.to_string().contains("token_url"));
    assert!(error.to_string().contains("must use https"));
}

#[test]
fn oauth_authorization_url_allows_loopback_http_endpoints() {
    let provider = oauth_provider_with_endpoints(
        "http://127.0.0.1:45454/authorize".to_string(),
        "http://127.0.0.1:45454/token".to_string(),
    );

    let url = build_oauth_authorization_url(
        &provider,
        "http://127.0.0.1:8080/callback",
        "state",
        "challenge",
    )
    .unwrap();

    assert!(url.starts_with("http://127.0.0.1:45454/authorize?"));
}

#[test]
fn oauth_token_exchange_redacts_plain_text_tokens() {
    let (token_url, _request_rx) = spawn_response_server(
        "502 Bad Gateway",
        "text/plain",
        "Bearer sk-live-123456 refresh_token=refresh-secret",
    );
    let provider = oauth_provider(token_url);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let error = runtime
        .block_on(exchange_oauth_code(
            &Client::new(),
            &provider,
            "code-123",
            "verifier-123",
            "http://127.0.0.1:8080/callback",
        ))
        .unwrap_err();

    let message = error.to_string();
    assert!(!message.contains("sk-live-123456"));
    assert!(!message.contains("refresh-secret"));
    assert!(message.contains("[REDACTED]"));
}

#[test]
fn extract_error_redacts_nested_secret_fields() {
    let message = extract_error(&json!({
        "error": {
            "message": "request failed for access_token=access-secret refresh_token=refresh-secret"
        }
    }));

    assert!(!message.contains("access-secret"));
    assert!(!message.contains("refresh-secret"));
    assert!(message.contains("[REDACTED]"));
}

#[test]
fn provider_error_for_display_does_not_echo_secret_response_text() {
    let message = provider_error_for_status(StatusCode::UNAUTHORIZED);

    assert!(!message.contains("sk-live-123456"));
    assert!(!message.contains("refresh-secret"));
    assert_eq!(message, "authentication error");
}

#[test]
fn refresh_keeps_existing_refresh_token_when_provider_omits_it() {
    let (token_url, request_rx) = spawn_json_server(json!({
        "access_token": "access-456",
        "expires_in": 45,
        "token_type": "Bearer"
    }));
    let provider = oauth_provider(token_url);
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let refreshed = runtime
        .block_on(refresh_oauth_token(
            &Client::new(),
            &provider,
            &OAuthToken {
                access_token: "stale".to_string(),
                refresh_token: Some("refresh-keep".to_string()),
                expires_at: None,
                token_type: Some("Bearer".to_string()),
                scopes: vec!["profile".to_string()],
                id_token: None,
                account_id: None,
                user_id: None,
                org_id: None,
                project_id: None,
                display_email: None,
                subscription_type: None,
            },
        ))
        .unwrap();

    let request = request_rx.recv_timeout(StdDuration::from_secs(2)).unwrap();
    assert!(request.contains("grant_type=refresh_token"));
    assert!(request.contains("refresh_token=refresh-keep"));
    assert_eq!(refreshed.access_token, "access-456");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-keep"));
}

#[test]
fn oversized_oauth_tokens_use_segmented_keychain_storage() {
    let token = OAuthToken {
        access_token: "a".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 350),
        refresh_token: Some("r".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 275)),
        expires_at: Some(Utc::now()),
        token_type: Some("Bearer".to_string()),
        scopes: vec!["profile".to_string(), "offline_access".to_string()],
        id_token: Some("i".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 125)),
        account_id: Some("account-123".to_string()),
        user_id: Some("user-123".to_string()),
        org_id: Some("org-123".to_string()),
        project_id: Some("project-123".to_string()),
        display_email: Some("user@example.com".to_string()),
        subscription_type: Some("pro".to_string()),
    };

    let serialized = serialize_oauth_token_secret("provider:test", &token).unwrap();
    let secret = match serialized {
        SerializedOAuthTokenSecret::Inline(_) => {
            panic!("expected oversized token to use segmented storage")
        }
        SerializedOAuthTokenSecret::Segmented(secret) => secret,
    };

    assert!(secret_storage_units(&secret.metadata_raw) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
    assert!(secret
        .segments
        .iter()
        .all(|(_, value)| secret_storage_units(value) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS));

    let stored_segments = secret
        .segments
        .iter()
        .cloned()
        .collect::<HashMap<String, String>>();
    let restored =
        deserialize_oauth_token_secret("provider:test", &secret.metadata_raw, |segment_account| {
            stored_segments
                .get(segment_account)
                .cloned()
                .ok_or_else(|| anyhow!("missing segment {segment_account}"))
        })
        .unwrap();

    assert_eq!(restored, token);
}

#[test]
fn oversized_plain_secrets_use_segmented_keychain_storage() {
    let secret_value = "k".repeat(KEYCHAIN_SECRET_SAFE_UTF16_UNITS + 512);

    let serialized = serialize_secret_storage("provider:test", &secret_value).unwrap();
    let secret = match serialized {
        SerializedSecret::Inline(_) => {
            panic!("expected oversized secret to use segmented storage")
        }
        SerializedSecret::Segmented(secret) => secret,
    };

    assert!(secret_storage_units(&secret.metadata_raw) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
    assert!(secret
        .segments
        .iter()
        .all(|(_, value)| secret_storage_units(value) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS));

    let stored_segments = secret
        .segments
        .iter()
        .cloned()
        .collect::<HashMap<String, String>>();
    let restored =
        deserialize_secret_storage("provider:test", &secret.metadata_raw, |segment_account| {
            stored_segments
                .get(segment_account)
                .cloned()
                .ok_or_else(|| anyhow!("missing segment {segment_account}"))
        })
        .unwrap();

    assert_eq!(restored, secret_value);
}

#[test]
fn split_secret_chunks_respects_utf16_boundaries() {
    let secret = format!("A{}\u{1F600}BC{}\u{1F680}", "D".repeat(16), "E".repeat(16));

    let chunks = split_secret_chunks(&secret, 8);

    assert!(chunks.iter().all(|chunk| secret_storage_units(chunk) <= 8));
    assert_eq!(chunks.concat(), secret);
}

fn oauth_provider(token_url: String) -> ProviderConfig {
    oauth_provider_with_endpoints("https://auth.example.com/authorize".to_string(), token_url)
}

fn oauth_provider_with_endpoints(authorization_url: String, token_url: String) -> ProviderConfig {
    ProviderConfig {
        id: "oauth".to_string(),
        display_name: "OAuth".to_string(),
        kind: ProviderKind::OpenAiCompatible,
        base_url: "https://example.com/v1".to_string(),
        auth_mode: AuthMode::OAuth,
        default_model: Some("model".to_string()),
        keychain_account: None,
        oauth: Some(OAuthConfig {
            client_id: "client".to_string(),
            authorization_url,
            token_url,
            scopes: vec!["profile".to_string(), "offline_access".to_string()],
            extra_authorize_params: Vec::new(),
            extra_token_params: vec![KeyValuePair {
                key: "audience".to_string(),
                value: "nuclear".to_string(),
            }],
        }),
        local: false,
    }
}

fn openai_browser_test_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: "browser-client".to_string(),
        authorization_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/authorize"),
        token_url: format!("{OPENAI_BROWSER_AUTH_ISSUER}/oauth/token"),
        scopes: vec!["openid".to_string(), "offline_access".to_string()],
        extra_authorize_params: Vec::new(),
        extra_token_params: Vec::new(),
    }
}

fn build_sse_body(events: &[Value]) -> String {
    let mut body = String::new();
    for event in events {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .expect("SSE fixture event missing type");
        if event.as_object().is_some_and(|event| event.len() == 1) {
            body.push_str(&format!("event: {kind}\n\n"));
        } else {
            body.push_str(&format!("event: {kind}\ndata: {event}\n\n"));
        }
    }
    body
}

fn spawn_json_server(body: Value) -> (String, Receiver<String>) {
    spawn_response_server("200 OK", "application/json", &body.to_string())
}

fn spawn_response_server_at(
    base_path: &str,
    status: &str,
    content_type: &str,
    body: &str,
) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = mpsc::channel();
    let status = status.to_string();
    let content_type = content_type.to_string();
    let body = body.to_string();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0_u8; 65536];
        let bytes_read = stream.read(&mut buffer).unwrap();
        request_tx
            .send(String::from_utf8_lossy(&buffer[..bytes_read]).to_string())
            .unwrap();

        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });

    (format!("http://{address}{base_path}"), request_rx)
}

fn spawn_response_server(
    status: &str,
    content_type: &str,
    body: &str,
) -> (String, Receiver<String>) {
    spawn_response_server_at("/token", status, content_type, body)
}

struct TestImageFile {
    path: PathBuf,
}

impl TestImageFile {
    fn new(extension: &str, bytes: &[u8]) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "agent-providers-test-{}-{unique}.{extension}",
            std::process::id()
        ));
        fs::write(&path, bytes).unwrap();
        Self { path }
    }

    fn attachment(&self) -> InputAttachment {
        InputAttachment {
            kind: AttachmentKind::Image,
            path: self.path.clone(),
        }
    }
}

impl Drop for TestImageFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
