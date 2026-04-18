use super::*;
use agent_core::TaskMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InteractiveCommand {
    Exit,
    Help,
    Status,
    UpdateStatus,
    UpdateRun,
    ConfigShow,
    DashboardOpen,
    TelegramsShow,
    DiscordsShow,
    SlacksShow,
    SignalsShow,
    HomeAssistantsShow,
    TelegramApprovalsShow,
    TelegramApprove { id: String, note: Option<String> },
    TelegramReject { id: String, note: Option<String> },
    DiscordApprovalsShow,
    DiscordApprove { id: String, note: Option<String> },
    DiscordReject { id: String, note: Option<String> },
    SlackApprovalsShow,
    SlackApprove { id: String, note: Option<String> },
    SlackReject { id: String, note: Option<String> },
    WebhooksShow,
    InboxesShow,
    AutopilotShow,
    AutopilotEnable,
    AutopilotPause,
    AutopilotResume,
    MissionsShow,
    EventsShow(usize),
    Schedule { after_seconds: u64, title: String },
    Repeat { every_seconds: u64, title: String },
    Watch { path: PathBuf, title: String },
    ProfileShow,
    MemoryShow(Option<String>),
    MemoryReviewShow,
    MemoryRebuild { session_id: Option<String> },
    MemoryApprove { id: String, note: Option<String> },
    MemoryReject { id: String, note: Option<String> },
    Remember(String),
    Forget(String),
    Skills(InteractiveSkillCommand),
    PermissionsShow,
    PermissionsSet(Option<PermissionPreset>),
    Attach(PathBuf),
    AttachmentsShow,
    AttachmentsClear,
    New,
    Clear,
    Diff,
    Copy,
    Compact,
    Init,
    Onboard,
    ModelShow,
    ModelSet(String),
    ProviderShow,
    ProviderSet(String),
    ModeShow,
    ModeSet(Option<TaskMode>),
    ThinkingShow,
    ThinkingSet(Option<ThinkingLevel>),
    Fast,
    Rename(Option<String>),
    Review(Option<String>),
    Resume(Option<String>),
    Fork(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InteractiveSkillCommand {
    Show(Option<SkillDraftStatus>),
    Publish(String),
    Reject(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InteractiveModelSelection {
    Alias(String),
    Explicit(String),
}

pub(crate) fn parse_interactive_command(line: &str) -> Result<Option<InteractiveCommand>> {
    if !line.starts_with('/') {
        return Ok(None);
    }

    let body = &line[1..];
    let mut parts = body.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let args = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let parsed = match command {
        "exit" | "quit" => InteractiveCommand::Exit,
        "help" => InteractiveCommand::Help,
        "status" => InteractiveCommand::Status,
        "update" => match args.map(|value| value.to_ascii_lowercase()) {
            None => InteractiveCommand::UpdateRun,
            Some(value) if value == "run" || value == "apply" => InteractiveCommand::UpdateRun,
            Some(value) if value == "status" || value == "check" => {
                InteractiveCommand::UpdateStatus
            }
            Some(_) => bail!("usage: /update [status]"),
        },
        "config" | "settings" => InteractiveCommand::ConfigShow,
        "dashboard" | "ui" => InteractiveCommand::DashboardOpen,
        "telegram" | "telegrams" => parse_telegram_interactive_command(args)?,
        "discord" | "discords" => parse_discord_interactive_command(args)?,
        "slack" | "slacks" => parse_slack_interactive_command(args)?,
        "signal" | "signals" => parse_signal_interactive_command(args)?,
        "home-assistant" | "home-assistants" | "homeassistant" | "homeassistants" | "ha" => {
            parse_home_assistant_interactive_command(args)?
        }
        "webhooks" => InteractiveCommand::WebhooksShow,
        "inboxes" => InteractiveCommand::InboxesShow,
        "autopilot" => match args.map(|value| value.to_ascii_lowercase()) {
            Some(value) if value == "on" || value == "enable" => {
                InteractiveCommand::AutopilotEnable
            }
            Some(value) if value == "pause" => InteractiveCommand::AutopilotPause,
            Some(value) if value == "resume" => InteractiveCommand::AutopilotResume,
            Some(value) if value == "status" => InteractiveCommand::AutopilotShow,
            Some(_) | None => InteractiveCommand::AutopilotShow,
        },
        "missions" => InteractiveCommand::MissionsShow,
        "events" => InteractiveCommand::EventsShow(parse_optional_limit(args, 10)?),
        "schedule" => {
            let (after_seconds, title) = parse_schedule_command_args(args)?;
            InteractiveCommand::Schedule {
                after_seconds,
                title,
            }
        }
        "repeat" => {
            let (every_seconds, title) = parse_repeat_command_args(args)?;
            InteractiveCommand::Repeat {
                every_seconds,
                title,
            }
        }
        "watch" => {
            let (path, title) = parse_watch_command_args(args)?;
            InteractiveCommand::Watch { path, title }
        }
        "profile" => InteractiveCommand::ProfileShow,
        "memory" => parse_memory_interactive_command(args)?,
        "remember" => InteractiveCommand::Remember(
            args.ok_or_else(|| anyhow!("usage: /remember <text>"))?
                .to_string(),
        ),
        "forget" => InteractiveCommand::Forget(
            args.ok_or_else(|| anyhow!("usage: /forget <memory-id>"))?
                .to_string(),
        ),
        "skills" => InteractiveCommand::Skills(parse_interactive_skill_command(args)?),
        "permissions" | "approvals" => match args {
            Some(value) => {
                InteractiveCommand::PermissionsSet(Some(parse_permission_preset(value)?))
            }
            None => InteractiveCommand::PermissionsShow,
        },
        "attach" => InteractiveCommand::Attach(PathBuf::from(
            args.ok_or_else(|| anyhow!("usage: /attach <path>"))?,
        )),
        "attachments" => InteractiveCommand::AttachmentsShow,
        "detach" | "attachments-clear" => InteractiveCommand::AttachmentsClear,
        "new" => InteractiveCommand::New,
        "clear" => InteractiveCommand::Clear,
        "diff" => InteractiveCommand::Diff,
        "copy" => InteractiveCommand::Copy,
        "compact" => InteractiveCommand::Compact,
        "init" => InteractiveCommand::Init,
        "onboard" => InteractiveCommand::Onboard,
        "alias" | "model" => match args {
            Some(value) => InteractiveCommand::ModelSet(value.to_string()),
            None => InteractiveCommand::ModelShow,
        },
        "provider" | "providers" => match args {
            Some(value) => InteractiveCommand::ProviderSet(value.to_string()),
            None => InteractiveCommand::ProviderShow,
        },
        "mode" => match args {
            Some(value) => InteractiveCommand::ModeSet(parse_task_mode_setting(value)?),
            None => InteractiveCommand::ModeShow,
        },
        "thinking" => match args {
            Some(value) => InteractiveCommand::ThinkingSet(parse_thinking_setting(value)?),
            None => InteractiveCommand::ThinkingShow,
        },
        "fast" => InteractiveCommand::Fast,
        "rename" => InteractiveCommand::Rename(args.map(ToOwned::to_owned)),
        "review" => InteractiveCommand::Review(args.map(ToOwned::to_owned)),
        "resume" => InteractiveCommand::Resume(args.map(ToOwned::to_owned)),
        "fork" => InteractiveCommand::Fork(args.map(ToOwned::to_owned)),
        other => bail!("unknown slash command '/{other}'. Use /help to list commands."),
    };

    Ok(Some(parsed))
}

fn parse_telegram_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::TelegramsShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::TelegramsShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::TelegramApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /telegram approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::TelegramApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /telegram reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::TelegramReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::TelegramsShow),
    }
}

fn parse_discord_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::DiscordsShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::DiscordsShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::DiscordApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /discord approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::DiscordApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /discord reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::DiscordReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::DiscordsShow),
    }
}

fn parse_slack_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::SlacksShow);
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::SlacksShow);
    }
    match action.to_ascii_lowercase().as_str() {
        "approvals" | "approval" => Ok(InteractiveCommand::SlackApprovalsShow),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /slack approve <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::SlackApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /slack reject <approval-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::SlackReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::SlacksShow),
    }
}

fn parse_home_assistant_interactive_command(_args: Option<&str>) -> Result<InteractiveCommand> {
    Ok(InteractiveCommand::HomeAssistantsShow)
}

fn parse_signal_interactive_command(_args: Option<&str>) -> Result<InteractiveCommand> {
    Ok(InteractiveCommand::SignalsShow)
}

fn parse_memory_interactive_command(args: Option<&str>) -> Result<InteractiveCommand> {
    let Some(args) = args else {
        return Ok(InteractiveCommand::MemoryShow(None));
    };
    let mut parts = args.splitn(3, char::is_whitespace);
    let action = parts.next().unwrap_or_default().trim();
    if action.is_empty() {
        return Ok(InteractiveCommand::MemoryShow(None));
    }
    match action.to_ascii_lowercase().as_str() {
        "review" => Ok(InteractiveCommand::MemoryReviewShow),
        "rebuild" => Ok(InteractiveCommand::MemoryRebuild {
            session_id: parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned),
        }),
        "approve" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /memory approve <memory-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::MemoryApprove {
                id: id.to_string(),
                note,
            })
        }
        "reject" => {
            let id = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("usage: /memory reject <memory-id> [note]"))?;
            let note = parts
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(InteractiveCommand::MemoryReject {
                id: id.to_string(),
                note,
            })
        }
        _ => Ok(InteractiveCommand::MemoryShow(Some(args.to_string()))),
    }
}

fn parse_interactive_skill_command(args: Option<&str>) -> Result<InteractiveSkillCommand> {
    let Some(args) = args else {
        return Ok(InteractiveSkillCommand::Show(None));
    };
    let mut parts = args.splitn(2, char::is_whitespace);
    let action = parts.next().unwrap_or_default().to_ascii_lowercase();
    let remainder = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match action.as_str() {
        "drafts" | "list" => Ok(InteractiveSkillCommand::Show(None)),
        "published" => Ok(InteractiveSkillCommand::Show(Some(
            SkillDraftStatus::Published,
        ))),
        "rejected" => Ok(InteractiveSkillCommand::Show(Some(
            SkillDraftStatus::Rejected,
        ))),
        "publish" => Ok(InteractiveSkillCommand::Publish(
            remainder
                .ok_or_else(|| anyhow!("usage: /skills publish <draft-id>"))?
                .to_string(),
        )),
        "reject" => Ok(InteractiveSkillCommand::Reject(
            remainder
                .ok_or_else(|| anyhow!("usage: /skills reject <draft-id>"))?
                .to_string(),
        )),
        _ => Ok(InteractiveSkillCommand::Show(None)),
    }
}

fn parse_thinking_setting(value: &str) -> Result<Option<ThinkingLevel>> {
    if value.eq_ignore_ascii_case("default") {
        return Ok(None);
    }

    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "none" => Ok(Some(ThinkingLevel::None)),
        "minimal" => Ok(Some(ThinkingLevel::Minimal)),
        "low" => Ok(Some(ThinkingLevel::Low)),
        "medium" => Ok(Some(ThinkingLevel::Medium)),
        "high" => Ok(Some(ThinkingLevel::High)),
        "xhigh" | "x-high" | "extra-high" => Ok(Some(ThinkingLevel::XHigh)),
        _ => bail!("unknown thinking level '{value}'"),
    }
}

fn parse_task_mode_setting(value: &str) -> Result<Option<TaskMode>> {
    if value.eq_ignore_ascii_case("default") {
        return Ok(None);
    }

    match value.trim().to_ascii_lowercase().as_str() {
        "build" => Ok(Some(TaskMode::Build)),
        "daily" => Ok(Some(TaskMode::Daily)),
        _ => bail!("unknown task mode '{value}'"),
    }
}

fn parse_optional_limit(value: Option<&str>, default: usize) -> Result<usize> {
    match value {
        Some(value) => value
            .parse::<usize>()
            .with_context(|| format!("invalid limit '{value}'")),
        None => Ok(default),
    }
}

fn parse_schedule_command_args(args: Option<&str>) -> Result<(u64, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let delay = parts
        .next()
        .ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /schedule <after-seconds> <title>"))?;
    let after_seconds = delay
        .parse::<u64>()
        .with_context(|| format!("invalid schedule delay '{delay}'"))?;
    Ok((after_seconds, title.to_string()))
}

fn parse_repeat_command_args(args: Option<&str>) -> Result<(u64, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let interval = parts
        .next()
        .ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /repeat <every-seconds> <title>"))?;
    let every_seconds = interval
        .parse::<u64>()
        .with_context(|| format!("invalid repeat interval '{interval}'"))?;
    Ok((every_seconds, title.to_string()))
}

fn parse_watch_command_args(args: Option<&str>) -> Result<(PathBuf, String)> {
    let value = args.ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    let mut parts = value.splitn(2, char::is_whitespace);
    let path = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("usage: /watch <path> <title>"))?;
    Ok((PathBuf::from(path), title.to_string()))
}
