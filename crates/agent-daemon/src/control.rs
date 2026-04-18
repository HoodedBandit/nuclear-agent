use std::collections::BTreeSet;

use agent_core::{AppConfig, ProviderConfig, INTERNAL_DAEMON_ARG};
use axum::http::StatusCode;

use crate::{ApiError, AppState};

mod bootstrap;
mod operations;
mod providers;
mod system;
mod update;

#[cfg(test)]
pub(crate) use bootstrap::OnboardingResetRequest;
pub(crate) use bootstrap::{
    build_daemon_status, build_dashboard_bootstrap_response, dashboard_bootstrap, export_config,
    import_config, reset_onboarding, shutdown, status,
};
pub(crate) use operations::{
    autonomy_status, autopilot_status, doctor, enable_autonomy, format_event_cursor, list_events,
    list_logs, load_events, next_event_cursor, parse_event_cursor, pause_autonomy,
    provider_capability_summaries, resume_autonomy, update_autopilot, EventCursor,
};
pub(crate) use providers::{
    clear_provider_credentials, delete_alias, delete_provider, list_aliases,
    list_provider_model_descriptors, list_provider_models, list_providers,
    suggest_provider_defaults, update_main_alias, upsert_alias, upsert_provider,
};
pub(crate) use system::{
    delegation_status, delete_mcp_server, get_permission_preset, get_trust,
    list_delegation_targets, list_enabled_skills, list_mcp_servers, update_daemon_config,
    update_delegation_config, update_enabled_skills, update_permission_preset, update_trust,
    upsert_mcp_server,
};
pub(crate) use update::{run_update, update_status};

fn redact_provider_secret_metadata(mut provider: ProviderConfig) -> ProviderConfig {
    provider.keychain_account = None;
    provider
}

fn sync_daemon_autostart_setting(state: &AppState, auto_start: bool) -> Result<(), ApiError> {
    let daemon_path = std::env::current_exe()
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    state
        .storage
        .sync_autostart(&daemon_path, &[INTERNAL_DAEMON_ARG], auto_start)
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

fn configured_keychain_accounts(config: &AppConfig) -> BTreeSet<String> {
    let mut accounts = config
        .providers
        .iter()
        .filter_map(|provider| provider.keychain_account.clone())
        .collect::<BTreeSet<_>>();
    accounts.extend(
        config
            .telegram_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .discord_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .slack_connectors
            .iter()
            .filter_map(|connector| connector.bot_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .home_assistant_connectors
            .iter()
            .filter_map(|connector| connector.access_token_keychain_account.clone()),
    );
    accounts.extend(
        config
            .brave_connectors
            .iter()
            .filter_map(|connector| connector.api_key_keychain_account.clone()),
    );
    accounts.extend(
        config
            .gmail_connectors
            .iter()
            .filter_map(|connector| connector.oauth_keychain_account.clone()),
    );
    accounts
}
