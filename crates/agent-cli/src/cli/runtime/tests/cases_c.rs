    #[test]
    fn cli_parses_resume_last() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "resume", "--last"]);
        match cli.command {
            Some(Commands::Resume(args)) => {
                assert!(args.last);
                assert!(args.session_id.is_none());
            }
            _ => panic!("expected resume command"),
        }
    }

    #[test]
    fn cli_parses_browser_auth_for_login() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "login",
            "--id",
            "openrouter",
            "--name",
            "OpenRouter",
            "--kind",
            "openrouter",
            "--auth",
            "browser",
            "--model",
            "openai/gpt-4.1",
        ]);
        match cli.command {
            Some(Commands::Login(args)) => {
                assert_eq!(args.kind, Some(HostedKindArg::Openrouter));
                assert_eq!(args.auth, Some(AuthMethodArg::Browser));
            }
            _ => panic!("expected login command"),
        }
    }

    #[test]
    fn cli_parses_daemon_config_bool_value() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "daemon",
            "config",
            "--mode",
            "always-on",
            "--auto-start",
            "true",
        ]);
        match cli.command {
            Some(Commands::Daemon {
                command: DaemonCommands::Config(args),
            }) => {
                assert_eq!(args.mode, Some(PersistenceModeArg::AlwaysOn));
                assert_eq!(args.auto_start, Some(true));
            }
            _ => panic!("expected daemon config command"),
        }
    }

    #[test]
    fn cli_parses_trust_bool_values() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "trust",
            "--allow-shell",
            "true",
            "--allow-network",
            "false",
        ]);
        match cli.command {
            Some(Commands::Trust(args)) => {
                assert_eq!(args.allow_shell, Some(true));
                assert_eq!(args.allow_network, Some(false));
            }
            _ => panic!("expected trust command"),
        }
    }

    #[test]
    fn parse_interactive_command_supports_model_mode_and_thinking() {
        assert_eq!(
            parse_interactive_command("/model claude").unwrap(),
            Some(InteractiveCommand::ModelSet("claude".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/provider anthropic").unwrap(),
            Some(InteractiveCommand::ProviderSet("anthropic".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/provider").unwrap(),
            Some(InteractiveCommand::ProviderShow)
        );
        assert_eq!(
            parse_interactive_command("/onboard").unwrap(),
            Some(InteractiveCommand::Onboard)
        );
        assert_eq!(
            parse_interactive_command("/thinking high").unwrap(),
            Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
        );
        assert_eq!(
            parse_interactive_command("/thinking default").unwrap(),
            Some(InteractiveCommand::ThinkingSet(None))
        );
        assert_eq!(
            parse_interactive_command("/mode daily").unwrap(),
            Some(InteractiveCommand::ModeSet(Some(TaskMode::Daily)))
        );
        assert_eq!(
            parse_interactive_command("/mode default").unwrap(),
            Some(InteractiveCommand::ModeSet(None))
        );
    }

    #[test]
    fn resolve_interactive_provider_selection_prefers_logged_in_provider_aliases() {
        let storage = temp_storage();
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::OpenAiCompatible,
                    base_url: DEFAULT_OPENAI_URL.to_string(),
                    provider_profile: None,
                    auth_mode: AuthMode::None,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: true,
                },
                ProviderConfig {
                    id: "anthropic".to_string(),
                    display_name: "Claude".to_string(),
                    kind: ProviderKind::Anthropic,
                    base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                    provider_profile: None,
                    auth_mode: AuthMode::None,
                    default_model: Some("claude-sonnet".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: true,
                },
            ],
            aliases: vec![
                ModelAlias {
                    alias: "main".to_string(),
                    provider_id: "openai".to_string(),
                    model: "gpt-5".to_string(),
                    description: None,
                },
                ModelAlias {
                    alias: "claude".to_string(),
                    provider_id: "anthropic".to_string(),
                    model: "claude-sonnet".to_string(),
                    description: None,
                },
            ],
            main_agent_alias: Some("main".to_string()),
            ..AppConfig::default()
        };
        storage.save_config(&config).unwrap();

        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "anthropic").unwrap(),
            "claude"
        );
        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "Claude").unwrap(),
            "claude"
        );
        assert_eq!(
            resolve_interactive_provider_selection(&storage, Some("main"), "claude").unwrap(),
            "claude"
        );
    }

    #[test]
    fn normalize_model_selection_value_ignores_punctuation() {
        assert_eq!(normalize_model_selection_value("gpt-5.4"), "gpt54");
        assert_eq!(normalize_model_selection_value("GPT 5_4"), "gpt54");
    }

    #[test]
    fn resolved_requested_model_prefers_explicit_override() {
        let alias = ModelAlias {
            alias: "main".to_string(),
            provider_id: "openai".to_string(),
            model: "gpt-5.2".to_string(),
            description: None,
        };

        assert_eq!(resolved_requested_model(&alias, Some("gpt-5.4")), "gpt-5.4");
        assert_eq!(resolved_requested_model(&alias, None), "gpt-5.2");
    }

    #[test]
    fn parse_interactive_command_supports_review_and_status() {
        assert_eq!(
            parse_interactive_command("/review focus on tests").unwrap(),
            Some(InteractiveCommand::Review(Some(
                "focus on tests".to_string()
            )))
        );
        assert_eq!(
            parse_interactive_command("/status").unwrap(),
            Some(InteractiveCommand::Status)
        );
        assert_eq!(
            parse_interactive_command("/config").unwrap(),
            Some(InteractiveCommand::ConfigShow)
        );
        assert_eq!(
            parse_interactive_command("/dashboard").unwrap(),
            Some(InteractiveCommand::DashboardOpen)
        );
        assert_eq!(
            parse_interactive_command("/telegrams").unwrap(),
            Some(InteractiveCommand::TelegramsShow)
        );
        assert_eq!(
            parse_interactive_command("/telegram approvals").unwrap(),
            Some(InteractiveCommand::TelegramApprovalsShow)
        );
        assert_eq!(
            parse_interactive_command("/discords").unwrap(),
            Some(InteractiveCommand::DiscordsShow)
        );
        assert_eq!(
            parse_interactive_command("/home-assistant").unwrap(),
            Some(InteractiveCommand::HomeAssistantsShow)
        );
        assert_eq!(
            parse_interactive_command("/telegram approve req-1 looks good").unwrap(),
            Some(InteractiveCommand::TelegramApprove {
                id: "req-1".to_string(),
                note: Some("looks good".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/webhooks").unwrap(),
            Some(InteractiveCommand::WebhooksShow)
        );
        assert_eq!(
            parse_interactive_command("/inboxes").unwrap(),
            Some(InteractiveCommand::InboxesShow)
        );
    }

    #[test]
    fn parse_interactive_command_supports_events_and_schedule() {
        assert_eq!(
            parse_interactive_command("/events 25").unwrap(),
            Some(InteractiveCommand::EventsShow(25))
        );
        assert_eq!(
            parse_interactive_command("/schedule 300 review auth flow").unwrap(),
            Some(InteractiveCommand::Schedule {
                after_seconds: 300,
                title: "review auth flow".to_string(),
            })
        );
        assert_eq!(
            parse_interactive_command("/repeat 600 weekly cleanup").unwrap(),
            Some(InteractiveCommand::Repeat {
                every_seconds: 600,
                title: "weekly cleanup".to_string(),
            })
        );
        assert_eq!(
            parse_interactive_command("/watch src watch auth changes").unwrap(),
            Some(InteractiveCommand::Watch {
                path: PathBuf::from("src"),
                title: "watch auth changes".to_string(),
            })
        );
    }

    #[test]
    fn parse_interactive_command_supports_profile_and_skills() {
        assert_eq!(
            parse_interactive_command("/profile").unwrap(),
            Some(InteractiveCommand::ProfileShow)
        );
        assert_eq!(
            parse_interactive_command("/skills published").unwrap(),
            Some(InteractiveCommand::Skills(InteractiveSkillCommand::Show(
                Some(SkillDraftStatus::Published)
            )))
        );
        assert_eq!(
            parse_interactive_command("/skills publish draft-1").unwrap(),
            Some(InteractiveCommand::Skills(
                InteractiveSkillCommand::Publish("draft-1".to_string())
            ))
        );
    }

    #[test]
    fn parse_interactive_command_supports_memory_review_actions() {
        assert_eq!(
            parse_interactive_command("/memory review").unwrap(),
            Some(InteractiveCommand::MemoryReviewShow)
        );
        assert_eq!(
            parse_interactive_command("/memory rebuild session-1").unwrap(),
            Some(InteractiveCommand::MemoryRebuild {
                session_id: Some("session-1".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/memory approve mem-1 looks good").unwrap(),
            Some(InteractiveCommand::MemoryApprove {
                id: "mem-1".to_string(),
                note: Some("looks good".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/memory reject mem-2 duplicate").unwrap(),
            Some(InteractiveCommand::MemoryReject {
                id: "mem-2".to_string(),
                note: Some("duplicate".to_string()),
            })
        );
    }

    #[test]
    fn parse_interactive_command_supports_discord_review_actions() {
        assert_eq!(
            parse_interactive_command("/discord approvals").unwrap(),
            Some(InteractiveCommand::DiscordApprovalsShow)
        );
        assert_eq!(
            parse_interactive_command("/discord approve appr-1 trusted").unwrap(),
            Some(InteractiveCommand::DiscordApprove {
                id: "appr-1".to_string(),
                note: Some("trusted".to_string()),
            })
        );
        assert_eq!(
            parse_interactive_command("/discord reject appr-2 spam").unwrap(),
            Some(InteractiveCommand::DiscordReject {
                id: "appr-2".to_string(),
                note: Some("spam".to_string()),
            })
        );
    }

    #[test]
    fn cli_parses_mission_schedule_flags() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "mission",
            "add",
            "Follow up",
            "--after-seconds",
            "120",
        ]);
        match cli.command {
            Some(Commands::Mission {
                command:
                    MissionCommands::Add {
                        title,
                        after_seconds,
                        at,
                        ..
                    },
            }) => {
                assert_eq!(title, "Follow up");
                assert_eq!(after_seconds, Some(120));
                assert_eq!(at, None);
            }
            _ => panic!("expected scheduled mission add command"),
        }
    }

    #[test]
    fn cli_parses_mission_watch_flags() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "mission",
            "add",
            "Watch repo",
            "--watch",
            "src",
            "--watch-nonrecursive",
        ]);
        match cli.command {
            Some(Commands::Mission {
                command:
                    MissionCommands::Add {
                        watch,
                        watch_nonrecursive,
                        ..
                    },
            }) => {
                assert_eq!(watch, Some(PathBuf::from("src")));
                assert!(watch_nonrecursive);
            }
            _ => panic!("expected watched mission add command"),
        }
    }

    #[test]
    fn cli_parses_run_and_chat_modes() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "run",
            "--mode",
            "daily",
            "plan my week",
        ]);
        match cli.command {
            Some(Commands::Run(args)) => {
                assert_eq!(args.mode, Some(TaskModeArg::Daily));
                assert_eq!(args.prompt.as_deref(), Some("plan my week"));
            }
            _ => panic!("expected run command"),
        }

        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "chat", "--mode", "build"]);
        match cli.command {
            Some(Commands::Chat(args)) => {
                assert_eq!(args.mode, Some(TaskModeArg::Build));
            }
            _ => panic!("expected chat command"),
        }
    }

    #[test]
    fn cli_parses_skill_draft_listing() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "skills",
            "drafts",
            "--status",
            "published",
            "--limit",
            "5",
        ]);
        match cli.command {
            Some(Commands::Skills {
                command: SkillCommands::Drafts { limit, status },
            }) => {
                assert_eq!(limit, 5);
                assert_eq!(status, Some(SkillDraftStatusArg::Published));
            }
            _ => panic!("expected skill drafts command"),
        }
    }

    #[test]
    fn cli_parses_memory_profile_command() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "memory", "profile", "--limit", "7"]);
        match cli.command {
            Some(Commands::Memory {
                command: MemoryCommands::Profile { limit },
            }) => {
                assert_eq!(limit, 7);
            }
            _ => panic!("expected memory profile command"),
        }
    }

    #[test]
    fn cli_parses_memory_rebuild_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "memory",
            "rebuild",
            "--session-id",
            "session-1",
            "--recompute-embeddings",
        ]);
        match cli.command {
            Some(Commands::Memory {
                command:
                    MemoryCommands::Rebuild {
                        session_id,
                        recompute_embeddings,
                    },
            }) => {
                assert_eq!(session_id.as_deref(), Some("session-1"));
                assert!(recompute_embeddings);
            }
            _ => panic!("expected memory rebuild command"),
        }
    }

    #[test]
    fn cli_parses_session_resume_packet_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "session",
            "resume-packet",
            "session-1",
        ]);
        match cli.command {
            Some(Commands::Session {
                command: SessionCommands::ResumePacket { id },
            }) => {
                assert_eq!(id, "session-1");
            }
            _ => panic!("expected session resume-packet command"),
        }
    }

    #[test]
    fn cli_parses_telegram_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "telegram",
            "add",
            "--id",
            "ops-bot",
            "--name",
            "Ops Bot",
            "--description",
            "telegram resident connector",
            "--bot-token",
            "123:abc",
            "--chat-id",
            "42",
            "--user-id",
            "7",
        ]);
        match cli.command {
            Some(Commands::Telegram {
                command: TelegramCommands::Add(args),
            }) => {
                assert_eq!(args.id, "ops-bot");
                assert_eq!(args.chat_ids, vec![42]);
                assert_eq!(args.user_ids, vec![7]);
                assert!(args.require_pairing_approval);
            }
            _ => panic!("expected telegram add command"),
        }
    }

    #[test]
    fn cli_parses_webhook_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "webhook",
            "add",
            "--id",
            "alerts",
            "--name",
            "Alerts",
            "--description",
            "system alerts",
            "--prompt-template",
            "Check {payload_json}",
        ]);
        match cli.command {
            Some(Commands::Webhook {
                command: WebhookCommands::Add(args),
            }) => {
                assert_eq!(args.id, "alerts");
                assert_eq!(args.name, "Alerts");
            }
            _ => panic!("expected webhook add command"),
        }
    }

    #[test]
    fn cli_parses_discord_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "discord",
            "add",
            "--id",
            "ops-discord",
            "--name",
            "Ops Discord",
            "--description",
            "discord resident connector",
            "--bot-token",
            "discord-secret",
            "--monitored-channel-id",
            "123",
            "--allowed-channel-id",
            "456",
            "--user-id",
            "789",
        ]);
        match cli.command {
            Some(Commands::Discord {
                command: DiscordCommands::Add(args),
            }) => {
                assert_eq!(args.id, "ops-discord");
                assert_eq!(args.monitored_channel_ids, vec!["123".to_string()]);
                assert_eq!(args.allowed_channel_ids, vec!["456".to_string()]);
                assert_eq!(args.user_ids, vec!["789".to_string()]);
                assert!(args.require_pairing_approval);
            }
            _ => panic!("expected discord add command"),
        }
    }

    #[test]
    fn cli_parses_inbox_add_command() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "inbox",
            "add",
            "--id",
            "dropbox",
            "--name",
            "Drop Box",
            "--description",
            "local inbox",
            "--path",
            "tasks",
        ]);
        match cli.command {
            Some(Commands::Inbox {
                command: InboxCommands::Add(args),
            }) => {
                assert_eq!(args.id, "dropbox");
                assert_eq!(args.name, "Drop Box");
                assert_eq!(args.path, PathBuf::from("tasks"));
            }
            _ => panic!("expected inbox add command"),
        }
    }

    #[test]
    fn interactive_thinking_command_parses_default_and_levels() {
        assert_eq!(
            parse_interactive_command("/thinking").unwrap(),
            Some(InteractiveCommand::ThinkingShow)
        );
        assert_eq!(
            parse_interactive_command("/thinking high").unwrap(),
            Some(InteractiveCommand::ThinkingSet(Some(ThinkingLevel::High)))
        );
        assert_eq!(
            parse_interactive_command("/thinking default").unwrap(),
            Some(InteractiveCommand::ThinkingSet(None))
        );
        assert_eq!(
            parse_interactive_command("/mode").unwrap(),
            Some(InteractiveCommand::ModeShow)
        );
    }

    #[test]
    fn interactive_model_and_review_commands_parse() {
        assert_eq!(
            parse_interactive_command("/model claude").unwrap(),
            Some(InteractiveCommand::ModelSet("claude".to_string()))
        );
        assert_eq!(
            parse_interactive_command("/settings").unwrap(),
            Some(InteractiveCommand::ConfigShow)
        );
        assert_eq!(
            parse_interactive_command("/review focus on auth").unwrap(),
            Some(InteractiveCommand::Review(Some(
                "focus on auth".to_string()
            )))
        );
    }

    #[test]
    fn interactive_copy_compact_init_and_rename_commands_parse() {
        assert_eq!(
            parse_interactive_command("/copy").unwrap(),
            Some(InteractiveCommand::Copy)
        );
        assert_eq!(
            parse_interactive_command("/compact").unwrap(),
            Some(InteractiveCommand::Compact)
        );
        assert_eq!(
            parse_interactive_command("/init").unwrap(),
            Some(InteractiveCommand::Init)
        );
        assert_eq!(
            parse_interactive_command("/rename auth fixes").unwrap(),
            Some(InteractiveCommand::Rename(Some("auth fixes".to_string())))
        );
    }

    #[test]
    fn build_compact_prompt_includes_transcript_content() {
        let transcript = SessionTranscript {
            session: agent_core::SessionSummary {
                id: "session-1".to_string(),
                title: Some("Test".to_string()),
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                task_mode: None,
                message_count: 0,
                cwd: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            messages: vec![
                agent_core::SessionMessage::new(
                    "session-1".to_string(),
                    MessageRole::User,
                    "Fix auth".to_string(),
                    Some("openai".to_string()),
                    Some("gpt-4.1".to_string()),
                ),
                agent_core::SessionMessage::new(
                    "session-1".to_string(),
                    MessageRole::Assistant,
                    "Working on it".to_string(),
                    Some("openai".to_string()),
                    Some("gpt-4.1".to_string()),
                ),
            ],
        };

        let prompt = build_compact_prompt(&transcript).unwrap();
        assert!(prompt.contains("Fix auth"));
        assert!(prompt.contains("Working on it"));
    }

    #[test]
    fn truncate_for_prompt_preserves_utf8_boundaries() {
        let text = format!("{}😀tail", "a".repeat(19_997));
        let truncated = truncate_for_prompt(text, 20_000);
        assert!(truncated.ends_with("\n\n[truncated]"));
        assert!(!truncated.contains("😀tail"));
    }

    #[test]
    fn determine_logout_targets_uses_main_alias_provider() {
        let config = AppConfig {
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "anthropic".to_string(),
                model: "claude-opus".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "anthropic".to_string(),
                display_name: "Anthropic".to_string(),
                kind: ProviderKind::Anthropic,
                base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                provider_profile: None,
                auth_mode: AuthMode::ApiKey,
                default_model: Some("claude-opus".to_string()),
                keychain_account: None,
                oauth: None,
                local: false,
            }],
            ..AppConfig::default()
        };

        let targets = determine_logout_targets(
            &config,
            &LogoutArgs {
                provider: None,
                all: false,
            },
        )
        .unwrap();

        assert_eq!(targets, vec!["anthropic".to_string()]);
    }

    #[test]
    fn configured_keychain_accounts_deduplicates_and_skips_missing() {
        let config = AppConfig {
            providers: vec![
                ProviderConfig {
                    id: "openai".to_string(),
                    display_name: "OpenAI".to_string(),
                    kind: ProviderKind::ChatGptCodex,
                    base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                    provider_profile: None,
                    auth_mode: AuthMode::OAuth,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: Some("shared-account".to_string()),
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "openai-2".to_string(),
                    display_name: "OpenAI 2".to_string(),
                    kind: ProviderKind::ChatGptCodex,
                    base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                    provider_profile: None,
                    auth_mode: AuthMode::OAuth,
                    default_model: Some("gpt-5".to_string()),
                    keychain_account: Some("shared-account".to_string()),
                    oauth: None,
                    local: false,
                },
                ProviderConfig {
                    id: "anthropic".to_string(),
                    display_name: "Anthropic".to_string(),
                    kind: ProviderKind::Anthropic,
                    base_url: DEFAULT_ANTHROPIC_URL.to_string(),
                    provider_profile: None,
                    auth_mode: AuthMode::ApiKey,
                    default_model: Some("claude-opus".to_string()),
                    keychain_account: None,
                    oauth: None,
                    local: false,
                },
            ],
            telegram_connectors: vec![TelegramConnectorConfig {
                id: "ops-bot".to_string(),
                name: "Ops Bot".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("telegram-account".to_string()),
                require_pairing_approval: true,
                allowed_chat_ids: Vec::new(),
                allowed_user_ids: Vec::new(),
                last_update_id: None,
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            discord_connectors: vec![DiscordConnectorConfig {
                id: "ops-discord".to_string(),
                name: "Ops Discord".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("discord-account".to_string()),
                require_pairing_approval: true,
                monitored_channel_ids: vec!["123".to_string()],
                allowed_channel_ids: vec!["456".to_string()],
                allowed_user_ids: vec!["789".to_string()],
                channel_cursors: vec![DiscordChannelCursor {
                    channel_id: "123".to_string(),
                    last_message_id: Some("999".to_string()),
                }],
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            slack_connectors: vec![SlackConnectorConfig {
                id: "ops-slack".to_string(),
                name: "Ops Slack".to_string(),
                description: String::new(),
                enabled: true,
                bot_token_keychain_account: Some("slack-account".to_string()),
                require_pairing_approval: true,
                monitored_channel_ids: vec!["C123".to_string()],
                allowed_channel_ids: Vec::new(),
                allowed_user_ids: Vec::new(),
                channel_cursors: Vec::new(),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            home_assistant_connectors: vec![HomeAssistantConnectorConfig {
                id: "ops-home".to_string(),
                name: "Ops Home".to_string(),
                description: String::new(),
                enabled: true,
                base_url: "http://ha.local".to_string(),
                access_token_keychain_account: Some("home-account".to_string()),
                monitored_entity_ids: vec!["light.office".to_string()],
                allowed_service_domains: Vec::new(),
                allowed_service_entity_ids: Vec::new(),
                entity_cursors: Vec::new(),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            brave_connectors: vec![BraveConnectorConfig {
                id: "brave-search".to_string(),
                name: "Brave Search".to_string(),
                description: String::new(),
                enabled: true,
                api_key_keychain_account: Some("brave-account".to_string()),
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            gmail_connectors: vec![GmailConnectorConfig {
                id: "gmail-ops".to_string(),
                name: "Ops Gmail".to_string(),
                description: String::new(),
                enabled: true,
                oauth_keychain_account: Some("gmail-account".to_string()),
                require_pairing_approval: true,
                allowed_sender_addresses: Vec::new(),
                label_filter: Some("INBOX".to_string()),
                last_history_id: None,
                alias: None,
                requested_model: None,
                cwd: None,
            }],
            ..AppConfig::default()
        };

        let accounts = configured_keychain_accounts(&config);

        assert_eq!(accounts.len(), 7);
        assert!(accounts.contains("shared-account"));
        assert!(accounts.contains("telegram-account"));
        assert!(accounts.contains("discord-account"));
        assert!(accounts.contains("slack-account"));
        assert!(accounts.contains("home-account"));
        assert!(accounts.contains("brave-account"));
        assert!(accounts.contains("gmail-account"));
    }

    #[test]
    fn cli_parses_reset_yes() {
        let cli = Cli::parse_from([PRIMARY_COMMAND_NAME, "reset", "--yes"]);

        match cli.command {
            Some(Commands::Reset(args)) => assert!(args.yes),
            _ => panic!("expected reset command"),
        }
    }

    #[test]
    fn cli_parses_openrouter_provider_add() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "openrouter",
            "--name",
            "OpenRouter",
            "--kind",
            "openrouter",
            "--model",
            "openai/gpt-4.1",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::Openrouter);
                assert_eq!(args.model, "openai/gpt-4.1");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn cli_parses_openai_provider_add_alias() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "openai",
            "--name",
            "OpenAI",
            "--kind",
            "openai",
            "--model",
            "gpt-4.1",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::OpenaiCompatible);
                assert_eq!(args.model, "gpt-4.1");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn cli_parses_venice_provider_add() {
        let cli = Cli::parse_from([
            PRIMARY_COMMAND_NAME,
            "provider",
            "add",
            "--id",
            "venice",
            "--name",
            "Venice",
            "--kind",
            "venice",
            "--model",
            "venice-large",
            "--api-key",
            "secret",
        ]);

        match cli.command {
            Some(Commands::Provider {
                command: ProviderCommands::Add(args),
            }) => {
                assert_eq!(args.kind, HostedKindArg::Venice);
                assert_eq!(args.model, "venice-large");
            }
            _ => panic!("expected provider add command"),
        }
    }

    #[test]
    fn needs_onboarding_for_default_config() {
        assert!(needs_onboarding(&AppConfig::default()));
    }

    #[test]
    fn onboarding_not_needed_when_main_alias_resolves() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::Ollama,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                provider_profile: None,
                auth_mode: AuthMode::None,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            }],
            ..AppConfig::default()
        };

        assert!(!needs_onboarding(&config));
    }

    #[test]
    fn onboarding_not_needed_when_projected_plugin_provider_resolves() {
        let config = projected_plugin_config("main", "echo-provider", "echo-1");

        assert!(has_usable_main_alias(&config));
        assert!(!needs_onboarding(&config));
    }

    #[test]
    fn usable_main_alias_does_not_depend_on_onboarding_complete() {
        let config = AppConfig {
            onboarding_complete: false,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::Ollama,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                provider_profile: None,
                auth_mode: AuthMode::None,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: None,
                local: true,
            }],
            ..AppConfig::default()
        };

        assert!(has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_provider_is_missing() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "missing".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_provider_credentials_are_missing() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::ChatGptCodex,
                base_url: DEFAULT_CHATGPT_CODEX_URL.to_string(),
                provider_profile: None,
                auth_mode: AuthMode::OAuth,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: None,
                oauth: Some(openai_browser_oauth_config()),
                local: false,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }

    #[test]
    fn main_alias_is_not_usable_when_saved_credentials_are_unreadable() {
        let config = AppConfig {
            onboarding_complete: true,
            main_agent_alias: Some("main".to_string()),
            aliases: vec![ModelAlias {
                alias: "main".to_string(),
                provider_id: "openai".to_string(),
                model: "gpt-4.1".to_string(),
                description: None,
            }],
            providers: vec![ProviderConfig {
                id: "openai".to_string(),
                display_name: "OpenAI".to_string(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: DEFAULT_OPENAI_URL.to_string(),
                provider_profile: None,
                auth_mode: AuthMode::ApiKey,
                default_model: Some("gpt-4.1".to_string()),
                keychain_account: Some("missing-provider-account".to_string()),
                oauth: None,
                local: false,
            }],
            ..AppConfig::default()
        };

        assert!(!has_usable_main_alias(&config));
        assert!(needs_onboarding(&config));
    }
