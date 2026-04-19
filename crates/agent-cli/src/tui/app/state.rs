use super::*;

#[derive(Clone, Copy)]
pub(crate) enum PickerMode {
    Resume,
    Fork,
    Model,
    Alias,
    Thinking,
    Permissions,
    Config,
    Delegation,
    Autonomy,
    Provider,
    ProviderAction,
    Webhook,
    WebhookAction,
    Inbox,
    InboxAction,
    Telegram,
    TelegramAction,
    Discord,
    DiscordAction,
    Slack,
    SlackAction,
    Signal,
    SignalAction,
    HomeAssistant,
    HomeAssistantAction,
    Persistence,
    SkillDraft,
    SkillDraftAction,
}

#[derive(Clone)]
pub(crate) struct ModelPickerEntry {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) description: Option<String>,
    pub(crate) context_window: Option<i64>,
    pub(crate) effective_context_window_percent: Option<i64>,
}

#[derive(Clone)]
pub(crate) struct GenericPickerEntry {
    pub(crate) label: String,
    pub(crate) detail: Option<String>,
    pub(crate) search_text: String,
    pub(crate) current: bool,
    pub(crate) action: PickerAction,
}

#[derive(Clone)]
pub(crate) enum PickerAction {
    Resume(SessionSummary),
    Fork(SessionSummary),
    SetModel(String),
    SwitchChatAlias(String),
    SetMainAlias(String),
    SetThinking(Option<ThinkingLevel>),
    SetPermission(PermissionPreset),
    OpenConfig,
    OpenSettingsSection(SettingsSection),
    OpenAliasSwitcher,
    OpenCurrentAliasPicker,
    OpenMainAliasPicker,
    OpenModelPicker,
    OpenThinkingPicker,
    OpenPermissionPicker,
    OpenDelegationPicker,
    ToggleTrust(TrustToggle),
    OpenAutonomyPicker,
    OpenEvolvePicker,
    OpenAutopilotPicker,
    SetAutonomy(AutonomyMenuAction),
    SetEvolve(EvolveMenuAction),
    ShowMissionQueue,
    ShowMemoryBrowser,
    ShowResidentProfile,
    OpenSkillDraftPicker(Option<SkillDraftStatus>),
    OpenSkillDraftActions(String),
    ShowSkillDraftDetails(String),
    PublishSkillDraft(String),
    RejectSkillDraft(String),
    ShowDelegationTargets,
    EditApiKey(String),
    OpenProviderSwitchPicker,
    OpenProviderPicker,
    OpenProviderActions(String),
    ShowProviderDetails(String),
    QueueExternal(ExternalAction),
    ClearProviderCredentials(String),
    OpenWebhookPicker,
    OpenWebhookActions(String),
    ShowWebhookDetails(String),
    ToggleWebhookEnabled(String, bool),
    OpenInboxPicker,
    OpenInboxActions(String),
    ShowInboxDetails(String),
    ToggleInboxEnabled(String, bool),
    PollInbox(String),
    OpenTelegramPicker,
    OpenTelegramActions(String),
    ShowTelegramDetails(String),
    ToggleTelegramEnabled(String, bool),
    PollTelegram(String),
    OpenDiscordPicker,
    OpenDiscordActions(String),
    ShowDiscordDetails(String),
    ToggleDiscordEnabled(String, bool),
    PollDiscord(String),
    OpenSlackPicker,
    OpenSlackActions(String),
    ShowSlackDetails(String),
    ToggleSlackEnabled(String, bool),
    PollSlack(String),
    OpenSignalPicker,
    OpenSignalActions(String),
    ShowSignalDetails(String),
    ToggleSignalEnabled(String, bool),
    PollSignal(String),
    OpenHomeAssistantPicker,
    OpenHomeAssistantActions(String),
    ShowHomeAssistantDetails(String),
    ToggleHomeAssistantEnabled(String, bool),
    PollHomeAssistant(String),
    ShowTelegramApprovals,
    SetDelegationDepth(DelegationLimit),
    SetDelegationParallel(DelegationLimit),
    ToggleProviderDelegation(String, bool),
    OpenPersistencePicker,
    SetPersistenceMode(PersistenceMode),
    ToggleAutoStart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrustToggle {
    Shell,
    Network,
    FullDisk,
    SelfEdit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    Providers,
    ModelThinking,
    Permissions,
    Connectors,
    Autonomy,
    MemorySkills,
    Delegation,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutonomyMenuAction {
    EnableFreeThinking,
    EnableEvolve,
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EvolveMenuAction {
    Start,
    StartBudgetFriendly,
    Pause,
    Resume,
    Stop,
}

pub(crate) struct PickerState {
    pub(crate) mode: PickerMode,
    pub(crate) title: String,
    pub(crate) hint: String,
    pub(crate) empty_message: String,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) sessions: Vec<SessionSummary>,
    pub(crate) models: Vec<ModelPickerEntry>,
    pub(crate) items: Vec<GenericPickerEntry>,
}

pub(crate) enum OverlayState {
    Transcript {
        scroll_back: usize,
    },
    Static {
        title: String,
        body: String,
        scroll: usize,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        cursor: usize,
        secret: bool,
        action: InputPromptAction,
    },
}

#[derive(Clone)]
pub(crate) enum InputPromptAction {
    UpdateApiKey { provider_id: String },
}

#[derive(Clone)]
pub(crate) enum ExternalAction {
    AddProvider,
    AddWebhookConnector,
    AddInboxConnector,
    AddTelegramConnector,
    AddDiscordConnector,
    AddSlackConnector,
    AddSignalConnector,
    AddHomeAssistantConnector,
    ProviderBrowserLogin { provider_id: String },
    ProviderOAuthLogin { provider_id: String },
    OnboardReset,
    OpenDashboard,
}

#[derive(Clone)]
pub(super) struct PromptSnapshot {
    pub(super) session_id: Option<String>,
    pub(super) alias: Option<String>,
    pub(super) requested_model: Option<String>,
    pub(super) transcript: Vec<SessionMessage>,
    pub(super) transcript_scroll_back: usize,
}

pub(crate) struct TuiApp<'a> {
    pub(crate) storage: &'a Storage,
    pub(crate) client: DaemonClient,
    pub(crate) alias: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) thinking_level: Option<ThinkingLevel>,
    pub(crate) task_mode: Option<TaskMode>,
    pub(crate) permission_preset: Option<PermissionPreset>,
    pub(crate) attachments: Vec<InputAttachment>,
    pub(crate) cwd: PathBuf,
    pub(crate) transcript: Vec<SessionMessage>,
    pub(crate) input: String,
    pub(crate) input_cursor: usize,
    pub(crate) overlay: Option<OverlayState>,
    pub(crate) picker: Option<PickerState>,
    pub(crate) pending_external_action: Option<ExternalAction>,
    pub(crate) exit_requested: bool,
    pub(crate) busy: bool,
    pub(crate) busy_since: Option<Instant>,
    pub(crate) transcript_scroll_back: usize,
    pub(crate) requested_model: Option<String>,
    pub(crate) active_model: Option<String>,
    pub(crate) active_provider_name: Option<String>,
    pub(crate) context_window_tokens: Option<i64>,
    pub(crate) context_window_percent: Option<i64>,
    pub(crate) recent_events: Vec<LogEntry>,
    pub(crate) last_event_cursor: Option<DateTime<Utc>>,
    pub(crate) pending_tool_calls: Vec<ToolCall>,
    pub(crate) main_target: Option<MainTargetSummary>,
    pub(crate) restart_event_poller: bool,
    pub(super) pending_prompt_snapshot: Option<PromptSnapshot>,
}

impl PickerState {
    pub(crate) fn filtered_sessions(&self) -> Vec<SessionSummary> {
        let query = self.query.trim().to_ascii_lowercase();
        let mut sessions = self
            .sessions
            .iter()
            .filter(|session| {
                if query.is_empty() {
                    return true;
                }
                let title = session.title.as_deref().unwrap_or("(untitled)");
                let cwd = session
                    .cwd
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                format!(
                    "{} {} {} {} {} {} {}",
                    session.id,
                    title,
                    session.alias,
                    session.provider_id,
                    session.model,
                    task_mode_label(session.task_mode),
                    cwd
                )
                .to_ascii_lowercase()
                .contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        sessions
    }

    pub(crate) fn filtered_models(&self) -> Vec<ModelPickerEntry> {
        let query = self.query.trim().to_ascii_lowercase();
        self.models
            .iter()
            .filter(|model| {
                if query.is_empty() {
                    return true;
                }
                format!(
                    "{} {} {}",
                    model.id,
                    model.display_name,
                    model.description.as_deref().unwrap_or_default()
                )
                .to_ascii_lowercase()
                .contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn filtered_items(&self) -> Vec<GenericPickerEntry> {
        let query = self.query.trim().to_ascii_lowercase();
        self.items
            .iter()
            .filter(|item| {
                query.is_empty() || item.search_text.to_ascii_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn filtered_len(&self) -> usize {
        match self.mode {
            PickerMode::Resume | PickerMode::Fork => self.filtered_sessions().len(),
            PickerMode::Model => self.filtered_models().len(),
            PickerMode::Alias
            | PickerMode::Thinking
            | PickerMode::Permissions
            | PickerMode::Config
            | PickerMode::Delegation
            | PickerMode::Autonomy
            | PickerMode::Provider
            | PickerMode::ProviderAction
            | PickerMode::Webhook
            | PickerMode::WebhookAction
            | PickerMode::Inbox
            | PickerMode::InboxAction
            | PickerMode::Telegram
            | PickerMode::TelegramAction
            | PickerMode::Discord
            | PickerMode::DiscordAction
            | PickerMode::Slack
            | PickerMode::SlackAction
            | PickerMode::Signal
            | PickerMode::SignalAction
            | PickerMode::HomeAssistant
            | PickerMode::HomeAssistantAction
            | PickerMode::Persistence
            | PickerMode::SkillDraft
            | PickerMode::SkillDraftAction => self.filtered_items().len(),
        }
    }

    pub(crate) fn clamp_selected(&mut self) {
        let len = self.filtered_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }
}

pub(super) fn build_thinking_picker_entries(
    descriptor: Option<&ModelDescriptor>,
    current: Option<ThinkingLevel>,
) -> Vec<GenericPickerEntry> {
    let mut items = vec![
        thinking_picker_entry(
            "default",
            Some("Use the model's default reasoning level.".to_string()),
            current.is_none(),
            PickerAction::SetThinking(None),
        ),
        thinking_picker_entry(
            "none",
            Some("Disable additional reasoning effort.".to_string()),
            current == Some(ThinkingLevel::None),
            PickerAction::SetThinking(Some(ThinkingLevel::None)),
        ),
    ];

    let mut supported_levels = Vec::new();
    let level_description = |level: ThinkingLevel| {
        descriptor
            .and_then(|descriptor| {
                descriptor
                    .supported_reasoning_levels
                    .iter()
                    .find(|entry| thinking_level_from_effort(&entry.effort) == Some(level))
                    .and_then(|entry| entry.description.clone())
            })
            .unwrap_or_else(|| default_thinking_description(level).to_string())
    };

    if let Some(descriptor) = descriptor {
        for level in descriptor
            .supported_reasoning_levels
            .iter()
            .filter_map(|entry| thinking_level_from_effort(&entry.effort))
        {
            if !supported_levels.contains(&level) {
                supported_levels.push(level);
            }
        }
    }

    if supported_levels.is_empty() {
        supported_levels.extend([
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::XHigh,
        ]);
    } else if supported_levels.contains(&ThinkingLevel::Low)
        && !supported_levels.contains(&ThinkingLevel::Minimal)
    {
        supported_levels.insert(0, ThinkingLevel::Minimal);
    }

    for level in [
        ThinkingLevel::Minimal,
        ThinkingLevel::Low,
        ThinkingLevel::Medium,
        ThinkingLevel::High,
        ThinkingLevel::XHigh,
    ] {
        if !supported_levels.contains(&level) {
            continue;
        }
        let detail = if level == ThinkingLevel::Minimal {
            "Fastest option; maps to low effort when supported.".to_string()
        } else {
            level_description(level)
        };
        items.push(thinking_picker_entry(
            level.as_str(),
            Some(detail),
            current == Some(level),
            PickerAction::SetThinking(Some(level)),
        ));
    }

    items
}

fn thinking_picker_entry(
    label: &str,
    detail: Option<String>,
    current: bool,
    action: PickerAction,
) -> GenericPickerEntry {
    GenericPickerEntry {
        label: label.to_string(),
        search_text: format!("thinking {label} {}", detail.as_deref().unwrap_or_default()),
        detail,
        current,
        action,
    }
}

fn thinking_level_from_effort(effort: &str) -> Option<ThinkingLevel> {
    match effort.trim().to_ascii_lowercase().as_str() {
        "none" => Some(ThinkingLevel::None),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" | "x-high" | "extra-high" | "extra_high" => Some(ThinkingLevel::XHigh),
        _ => None,
    }
}

fn default_thinking_description(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::None => "Disable additional reasoning effort.",
        ThinkingLevel::Minimal => "Fastest option; maps to low effort when supported.",
        ThinkingLevel::Low => "Fast responses with lighter reasoning.",
        ThinkingLevel::Medium => "Balanced speed and reasoning depth.",
        ThinkingLevel::High => "More deliberate reasoning for harder tasks.",
        ThinkingLevel::XHigh => "Maximum reasoning depth for complex work.",
    }
}

pub(super) fn hosted_kind_for_provider(provider: &ProviderConfig) -> Option<HostedKindArg> {
    match provider.kind {
        ProviderKind::ChatGptCodex => Some(HostedKindArg::OpenaiCompatible),
        ProviderKind::Anthropic if !provider.local => Some(HostedKindArg::Anthropic),
        ProviderKind::OpenAiCompatible if !provider.local => {
            let normalized = provider.base_url.trim_end_matches('/');
            if normalized == DEFAULT_OPENAI_URL.trim_end_matches('/')
                || normalized == DEFAULT_CHATGPT_CODEX_URL.trim_end_matches('/')
            {
                Some(HostedKindArg::OpenaiCompatible)
            } else if normalized == DEFAULT_OPENROUTER_URL.trim_end_matches('/') {
                Some(HostedKindArg::Openrouter)
            } else if normalized == DEFAULT_MOONSHOT_URL.trim_end_matches('/') {
                Some(HostedKindArg::Moonshot)
            } else if normalized == DEFAULT_VENICE_URL.trim_end_matches('/') {
                Some(HostedKindArg::Venice)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn provider_kind_label(provider: &ProviderConfig) -> &'static str {
    match provider.kind {
        ProviderKind::ChatGptCodex => "chatgpt/codex",
        ProviderKind::OpenAiCompatible => {
            if provider.local {
                "openai-compatible (local)"
            } else {
                "openai-compatible"
            }
        }
        ProviderKind::Anthropic => {
            if provider.local {
                "anthropic (local)"
            } else {
                "anthropic"
            }
        }
        ProviderKind::Ollama => "ollama",
    }
}

pub(super) fn provider_auth_label(provider: &ProviderConfig) -> &'static str {
    match provider.auth_mode {
        AuthMode::None => "none",
        AuthMode::ApiKey => "api-key",
        AuthMode::OAuth => "oauth",
    }
}

pub(super) fn browser_action_label(provider: &ProviderConfig) -> &'static str {
    match hosted_kind_for_provider(provider) {
        Some(HostedKindArg::Moonshot | HostedKindArg::Venice) => "Browser portal",
        Some(_) => "Browser sign-in",
        None => "Browser auth",
    }
}
