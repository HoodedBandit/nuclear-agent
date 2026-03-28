use std::{
    io::{self, Write},
    time::Duration,
};

use agent_core::{AppConfig, AuthMode, ModelAlias, ProviderConfig};
use agent_providers::{list_model_descriptors, load_api_key, load_oauth_token};
use agent_storage::Storage;
use anyhow::{anyhow, bail, Result};
use tokio::time::timeout;

use crate::interactive_commands::InteractiveModelSelection;
use crate::{build_http_client, resolve_active_alias, resolved_requested_model};

pub(crate) fn print_interactive_help() {
    println!("Available commands:");
    println!("/help                     show this help");
    println!("/config                   open the categorized settings menu");
    println!("/dashboard                open the localhost web control room");
    println!("/telegrams                list configured Telegram connectors");
    println!("/discords                 list configured Discord connectors");
    println!("/slacks                   list configured Slack connectors");
    println!("/signals                  list configured Signal connectors");
    println!("/home-assistant           list configured Home Assistant connectors");
    println!("/telegram approvals       list pending Telegram pairing approvals");
    println!("/telegram approve <id>    approve a Telegram pairing request");
    println!("/telegram reject <id>     reject a Telegram pairing request");
    println!("/discord approvals        list pending Discord pairing approvals");
    println!("/discord approve <id>     approve a Discord pairing request");
    println!("/discord reject <id>      reject a Discord pairing request");
    println!("/slack approvals          list pending Slack pairing approvals");
    println!("/slack approve <id>       approve a Slack pairing request");
    println!("/slack reject <id>        reject a Slack pairing request");
    println!("/webhooks                 list configured webhook connectors");
    println!("/inboxes                  list configured inbox connectors");
    println!("/autopilot [on|pause|resume|status] control the background mission runner");
    println!("/missions                 list background missions");
    println!("/events [limit]           show recent daemon events");
    println!("/schedule <seconds> <title> create a scheduled background mission");
    println!("/repeat <seconds> <title> create a recurring background mission");
    println!("/watch <path> <title>     create a filesystem-triggered background mission");
    println!("/profile                  show learned resident profile memory");
    println!("/memory [query]           list recent memory or search memory/transcripts");
    println!("/memory review            show candidate memories awaiting review");
    println!("/memory rebuild [session] rebuild compiled memory from persisted transcript history");
    println!("/memory approve <id>      approve a candidate memory");
    println!("/memory reject <id>       reject a candidate memory");
    println!("/remember <text>          store a manual long-term memory note");
    println!("/forget <memory-id>       delete a stored memory");
    println!("/skills [drafts|published|rejected] list learned skill drafts");
    println!("/skills publish <id>      approve a learned skill draft");
    println!("/skills reject <id>       discard a learned skill draft");
    println!(
        "/model [name]             open the provider switcher or switch the current alias/model"
    );
    println!("/provider [name]          list or switch between logged-in providers");
    println!("/mode [value]             set task mode: default, build, or daily");
    println!("/fast                     set thinking to minimal");
    println!("/thinking [level]         open the thinking picker or set thinking: default, none, minimal, low, medium, high, xhigh");
    println!("/status                   show session, model, daemon, and thinking state");
    println!("/permissions [preset]     open the permissions picker or set permissions: suggest, auto-edit, full-auto");
    println!("/attach <path>            attach an image to the next prompt(s)");
    println!("/attachments              list current image attachments");
    println!("/detach                   clear current image attachments");
    println!("/copy                     copy the latest assistant output to the clipboard");
    println!("/compact                  summarize the current session into a smaller fork");
    println!("/init                     create an AGENTS.md starter file in the current directory");
    println!("/rename [title]           rename the current session");
    println!("/review [instructions]    review current uncommitted changes");
    println!("/diff                     print the current uncommitted git diff");
    println!("/resume [last|session]    resume another recorded session");
    println!("/fork [last|session]      fork the current or selected session");
    println!("/onboard                  wipe saved state and restart fresh setup");
    println!("/new                      start a new chat");
    println!("/clear                    clear the terminal and start a new chat");
    println!("!<command>                run a local shell command in the current directory");
    println!("/exit                     quit the interactive session");
}

pub(crate) fn clear_terminal() {
    print!("\x1B[2J\x1B[H");
    let _ = io::stdout().flush();
}

pub(crate) fn resolve_requested_model_override(
    storage: &Storage,
    alias: Option<&str>,
    actual_model: &str,
) -> Result<Option<String>> {
    let config = storage.load_config()?;
    let active_alias = resolve_active_alias(&config, alias)?;
    Ok((actual_model != active_alias.model).then(|| actual_model.to_string()))
}

pub(crate) fn resolve_session_model_override(
    storage: &Storage,
    session_id: Option<&str>,
    alias: Option<&str>,
) -> Result<Option<String>> {
    let Some(session_id) = session_id else {
        return Ok(None);
    };
    let Some(session) = storage.get_session(session_id)? else {
        return Ok(None);
    };
    resolve_requested_model_override(storage, alias, &session.model)
}

pub(crate) async fn interactive_model_choices_text(
    storage: &Storage,
    current_alias: Option<&str>,
    requested_model: Option<&str>,
) -> Result<String> {
    let config = storage.load_config()?;
    let active_alias = resolve_active_alias(&config, current_alias)?;
    let provider = config
        .resolve_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let selected_model = resolved_requested_model(active_alias, requested_model);
    let mut lines = vec![
        format!("current alias: {}", active_alias.alias),
        format!("provider: {}", provider.display_name),
        format!("selected model: {}", selected_model),
    ];

    match timeout(
        Duration::from_secs(3),
        list_model_descriptors(&build_http_client(), &provider),
    )
    .await
    {
        Ok(Ok(models)) if !models.is_empty() => {
            lines.push(String::new());
            lines.push("provider models:".to_string());
            for model in models {
                let marker = if model.id == selected_model { "*" } else { " " };
                let display_name = model.display_name.as_deref().unwrap_or(model.id.as_str());
                let suffix = match (model.context_window, model.effective_context_window_percent) {
                    (Some(window), Some(percent)) => {
                        format!(" | ctx {} @ {}%", format_tokens_compact(window), percent)
                    }
                    (Some(window), None) => {
                        format!(" | ctx {}", format_tokens_compact(window))
                    }
                    _ => String::new(),
                };

                if display_name == model.id {
                    lines.push(format!("{marker} {}{}", model.id, suffix));
                } else {
                    lines.push(format!(
                        "{marker} {} ({}){}",
                        display_name, model.id, suffix
                    ));
                }
            }
        }
        Ok(Ok(_)) => {
            lines.push(String::new());
            lines.push("provider models: (none returned)".to_string());
        }
        Ok(Err(error)) => {
            lines.push(String::new());
            lines.push(format!("provider models unavailable: {error:#}"));
        }
        Err(_) => {
            lines.push(String::new());
            lines.push("provider models unavailable: request timed out".to_string());
        }
    }

    if !config.aliases.is_empty() {
        lines.push(String::new());
        lines.push("configured aliases:".to_string());
        for alias in &config.aliases {
            let marker = if current_alias == Some(alias.alias.as_str()) && requested_model.is_none()
            {
                "*"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {} -> {} / {}",
                alias.alias, alias.provider_id, alias.model
            ));
        }
    }

    Ok(lines.join("\n"))
}

pub(crate) async fn resolve_interactive_model_selection(
    storage: &Storage,
    current_alias: Option<&str>,
    value: &str,
) -> Result<InteractiveModelSelection> {
    let config = storage.load_config()?;
    if config.get_alias(value).is_some() {
        return Ok(InteractiveModelSelection::Alias(value.to_string()));
    }

    let active_alias = resolve_active_alias(&config, current_alias)?;
    let provider = config
        .resolve_provider(&active_alias.provider_id)
        .ok_or_else(|| anyhow!("unknown provider '{}'", active_alias.provider_id))?;
    let normalized = normalize_model_selection_value(value);

    let resolved_model = match timeout(
        Duration::from_secs(3),
        list_model_descriptors(&build_http_client(), &provider),
    )
    .await
    {
        Ok(Ok(models)) => models
            .into_iter()
            .find(|model| {
                model.id.eq_ignore_ascii_case(value)
                    || normalize_model_selection_value(&model.id) == normalized
                    || model.display_name.as_deref().is_some_and(|name| {
                        name.eq_ignore_ascii_case(value)
                            || normalize_model_selection_value(name) == normalized
                    })
            })
            .map(|model| model.id)
            .unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    };

    Ok(InteractiveModelSelection::Explicit(resolved_model))
}

pub(crate) fn provider_has_saved_access(provider: &ProviderConfig) -> bool {
    match provider.auth_mode {
        AuthMode::None => true,
        AuthMode::ApiKey => provider
            .keychain_account
            .as_deref()
            .is_some_and(|account| load_api_key(account).is_ok()),
        AuthMode::OAuth => provider
            .keychain_account
            .as_deref()
            .is_some_and(|account| load_oauth_token(account).is_ok()),
    }
}

fn provider_display_label(provider: &ProviderConfig) -> String {
    if provider.display_name.trim().is_empty() {
        provider.id.clone()
    } else {
        provider.display_name.clone()
    }
}

fn preferred_provider_alias<'a>(
    config: &'a AppConfig,
    current_alias: Option<&str>,
    provider_id: &str,
) -> Option<&'a ModelAlias> {
    if let Some(alias) = current_alias
        .and_then(|name| config.get_alias(name))
        .filter(|alias| alias.provider_id == provider_id)
    {
        return Some(alias);
    }

    if let Some(alias) = config
        .main_agent_alias
        .as_deref()
        .and_then(|name| config.get_alias(name))
        .filter(|alias| alias.provider_id == provider_id)
    {
        return Some(alias);
    }

    if let Some(alias) = config
        .get_alias(provider_id)
        .filter(|alias| alias.provider_id == provider_id)
    {
        return Some(alias);
    }

    config
        .aliases
        .iter()
        .filter(|alias| alias.provider_id == provider_id)
        .min_by(|left, right| left.alias.cmp(&right.alias))
}

pub(crate) fn interactive_provider_choices_text(
    storage: &Storage,
    current_alias: Option<&str>,
) -> Result<String> {
    let config = storage.load_config()?;
    let active_provider = resolve_active_alias(&config, current_alias)
        .ok()
        .map(|alias| alias.provider_id.clone());
    let mut lines = Vec::new();

    if let Some(provider_id) = &active_provider {
        if let Some(provider) = config.resolve_provider(provider_id) {
            lines.push(format!(
                "current provider: {}",
                provider_display_label(&provider)
            ));
        }
    }

    let mut entries = config
        .all_providers()
        .into_iter()
        .filter(|provider| provider_has_saved_access(provider))
        .filter_map(|provider| {
            let alias = preferred_provider_alias(&config, current_alias, &provider.id)?;
            Some((provider, alias))
        })
        .collect::<Vec<_>>();
    entries.sort_by(
        |(left_provider, left_alias), (right_provider, right_alias)| {
            provider_display_label(left_provider)
                .cmp(&provider_display_label(right_provider))
                .then_with(|| left_alias.alias.cmp(&right_alias.alias))
        },
    );

    if !entries.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("usable providers:".to_string());
        for (provider, alias) in entries {
            let marker = if active_provider.as_deref() == Some(provider.id.as_str()) {
                "*"
            } else {
                " "
            };
            lines.push(format!(
                "{marker} {} ({}) -> {} / {}",
                provider_display_label(&provider),
                provider.id,
                alias.alias,
                alias.model
            ));
        }
    } else {
        lines.push("No usable providers with aliases are configured.".to_string());
    }

    Ok(lines.join("\n"))
}

pub(crate) fn resolve_interactive_provider_selection(
    storage: &Storage,
    current_alias: Option<&str>,
    value: &str,
) -> Result<String> {
    let config = storage.load_config()?;
    if let Some(alias) = config.get_alias(value).filter(|alias| {
        config
            .resolve_provider(&alias.provider_id)
            .is_some_and(|provider| provider_has_saved_access(&provider))
    }) {
        return Ok(alias.alias.clone());
    }

    let mut exact_matches = Vec::new();
    let mut normalized_matches = Vec::new();
    let normalized = normalize_model_selection_value(value);
    for provider in config
        .all_providers()
        .into_iter()
        .filter(|provider| provider_has_saved_access(provider))
    {
        let Some(alias) = preferred_provider_alias(&config, current_alias, &provider.id) else {
            continue;
        };
        let display = provider_display_label(&provider);
        if provider.id.eq_ignore_ascii_case(value) || display.eq_ignore_ascii_case(value) {
            exact_matches.push(alias.alias.clone());
            continue;
        }
        if normalize_model_selection_value(&provider.id) == normalized
            || normalize_model_selection_value(&display) == normalized
        {
            normalized_matches.push(alias.alias.clone());
        }
    }

    let matches = if exact_matches.is_empty() {
        normalized_matches
    } else {
        exact_matches
    };
    let mut matches = matches;
    matches.sort();
    matches.dedup();

    match matches.as_slice() {
        [alias] => Ok(alias.clone()),
        [] => bail!("unknown usable provider '{value}'"),
        _ => bail!("provider selection '{value}' is ambiguous"),
    }
}

pub(crate) fn normalize_model_selection_value(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn format_tokens_compact(value: i64) -> String {
    let value = value.max(0);
    if value == 0 {
        return "0".to_string();
    }
    if value < 1_000 {
        return value.to_string();
    }

    let value_f64 = value as f64;
    let (scaled, suffix) = if value >= 1_000_000_000_000 {
        (value_f64 / 1_000_000_000_000.0, "T")
    } else if value >= 1_000_000_000 {
        (value_f64 / 1_000_000_000.0, "B")
    } else if value >= 1_000_000 {
        (value_f64 / 1_000_000.0, "M")
    } else {
        (value_f64 / 1_000.0, "K")
    };

    let decimals = if scaled < 10.0 {
        2
    } else if scaled < 100.0 {
        1
    } else {
        0
    };

    let mut formatted = format!("{scaled:.decimals$}");
    if formatted.contains('.') {
        while formatted.ends_with('0') {
            formatted.pop();
        }
        if formatted.ends_with('.') {
            formatted.pop();
        }
    }
    format!("{formatted}{suffix}")
}
