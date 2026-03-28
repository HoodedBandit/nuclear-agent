use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use dialoguer::{theme::ColorfulTheme, Input, Password};

use super::SettingsSection;

pub(super) fn previous_char_boundary(input: &str, cursor: usize) -> usize {
    input[..cursor]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub(super) fn next_char_boundary(input: &str, cursor: usize) -> usize {
    if cursor >= input.len() {
        input.len()
    } else {
        input[cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| cursor + index)
            .unwrap_or(input.len())
    }
}

pub(super) fn line_start_offset(input: &str, cursor: usize) -> usize {
    input[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0)
}

pub(super) fn line_end_offset(input: &str, cursor: usize) -> usize {
    input[cursor..]
        .find('\n')
        .map(|idx| cursor + idx)
        .unwrap_or(input.len())
}

pub(super) fn cursor_line_and_column(input: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut column = 0usize;

    for ch in input[..cursor].chars() {
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    (line, column)
}

pub(super) fn line_column_to_offset(input: &str, line: usize, column: usize) -> usize {
    let mut current_line = 0usize;
    let mut line_start = 0usize;

    for (index, ch) in input.char_indices() {
        if current_line == line {
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start = index + 1;
        }
    }

    if current_line < line {
        return input.len();
    }

    let line_end = input[line_start..]
        .find('\n')
        .map(|index| line_start + index)
        .unwrap_or(input.len());

    let mut offset = line_start;
    let mut remaining = column;
    while offset < line_end && remaining > 0 {
        offset = next_char_boundary(input, offset);
        remaining -= 1;
    }
    offset
}

pub(super) fn input_line_count(input: &str) -> usize {
    input.chars().filter(|ch| *ch == '\n').count() + 1
}

pub(super) fn boolean_status(enabled: bool) -> String {
    if enabled {
        "enabled".to_string()
    } else {
        "disabled".to_string()
    }
}

pub(super) fn slugify_identifier(input: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, ' ' | '_' | '-' | '.' | '/' | '\\') {
            Some('-')
        } else {
            None
        };
        if let Some(value) = normalized {
            if value == '-' {
                if !slug.is_empty() && !last_dash {
                    slug.push(value);
                    last_dash = true;
                }
            } else {
                slug.push(value);
                last_dash = false;
            }
        }
    }
    slug.trim_matches('-').to_string()
}

pub(super) fn prompt_required(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<String> {
    let mut input = Input::<String>::with_theme(theme);
    input = input.with_prompt(prompt);
    if let Some(initial) = initial {
        input = input.with_initial_text(initial.to_string());
    }
    let value = input.interact_text()?.trim().to_string();
    if value.is_empty() {
        Err(anyhow!("{prompt} cannot be empty"))
    } else {
        Ok(value)
    }
}

pub(super) fn prompt_optional(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<Option<String>> {
    let mut input = Input::<String>::with_theme(theme);
    input = input.with_prompt(prompt).allow_empty(true);
    if let Some(initial) = initial {
        input = input.with_initial_text(initial.to_string());
    }
    let value = input.interact_text()?.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub(super) fn prompt_secret(theme: &ColorfulTheme, prompt: &str) -> Result<String> {
    let value = Password::with_theme(theme)
        .with_prompt(prompt)
        .with_confirmation("Confirm", "Values did not match")
        .interact()?;
    if value.trim().is_empty() {
        Err(anyhow!("{prompt} cannot be empty"))
    } else {
        Ok(value)
    }
}

pub(super) fn prompt_required_path(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<PathBuf> {
    Ok(PathBuf::from(prompt_required(theme, prompt, initial)?))
}

pub(super) fn prompt_optional_path(
    theme: &ColorfulTheme,
    prompt: &str,
    initial: Option<&str>,
) -> Result<Option<PathBuf>> {
    Ok(prompt_optional(theme, prompt, initial)?.map(PathBuf::from))
}

pub(super) fn prompt_csv_strings(theme: &ColorfulTheme, prompt: &str) -> Result<Vec<String>> {
    let value = prompt_optional(theme, prompt, None)?.unwrap_or_default();
    Ok(value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub(super) fn prompt_csv_i64(theme: &ColorfulTheme, prompt: &str) -> Result<Vec<i64>> {
    let value = prompt_optional(theme, prompt, None)?.unwrap_or_default();
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("failed to parse integer value '{value}'"))
        })
        .collect()
}

pub(super) fn default_webhook_prompt_template() -> String {
    "Connector: {connector_name}\nSummary: {summary}\nPrompt: {prompt}\nDetails: {details}\nPayload:\n{payload_json}".to_string()
}

pub(super) fn settings_section_title(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Providers => "Settings: Providers & Login",
        SettingsSection::ModelThinking => "Settings: Model & Thinking",
        SettingsSection::Permissions => "Settings: Permissions",
        SettingsSection::Connectors => "Settings: Connectors",
        SettingsSection::Autonomy => "Settings: Autonomy",
        SettingsSection::MemorySkills => "Settings: Memory & Skills",
        SettingsSection::Delegation => "Settings: Delegation",
        SettingsSection::System => "Settings: System",
    }
}
