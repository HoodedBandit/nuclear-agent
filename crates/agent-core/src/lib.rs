use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod app_config;
mod connectors;
mod control;
mod foundation;
mod plugins;
mod runtime;
mod safety;
#[cfg(test)]
mod tests;
mod workspace;

pub use connectors::*;
pub use control::*;
pub use foundation::*;
pub use plugins::*;
pub use runtime::*;
pub use safety::*;
pub use workspace::*;

pub const APP_NAME: &str = "Nuclear";
pub const APP_SLUG: &str = "nuclear";
pub const DISPLAY_APP_NAME: &str = "Nuclear Agent";
pub const PRIMARY_COMMAND_NAME: &str = "nuclear";
pub const CONFIG_VERSION: u32 = 3;
pub const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
pub const DEFAULT_DAEMON_PORT: u16 = 42690;
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_LOCAL_OPENAI_URL: &str = "http://127.0.0.1:5001/v1";
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_CHATGPT_CODEX_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_MOONSHOT_URL: &str = "https://api.moonshot.ai/v1";
pub const DEFAULT_OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_VENICE_URL: &str = "https://api.venice.ai/api/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5";
pub const DEFAULT_CHATGPT_CODEX_MODEL: &str = "gpt-5-codex";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";
pub const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-4.1";
pub const DEFAULT_MOONSHOT_MODEL: &str = "kimi-k2";
pub const DEFAULT_VENICE_MODEL: &str = "venice-large";
pub const KEYCHAIN_SERVICE: &str = "nuclear";
pub const INTERNAL_DAEMON_ARG: &str = "__daemon";
pub const INTERNAL_UPDATE_HELPER_ARG: &str = "__update-helper";

pub fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }

    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub fn truncate_with_suffix(text: &str, max_bytes: usize, suffix: &str) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    format!("{}{}", truncate_utf8(text, max_bytes), suffix)
}
