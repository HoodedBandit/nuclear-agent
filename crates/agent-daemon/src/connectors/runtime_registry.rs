use super::{discord, gmail, home_assistant, inbox, signal, slack, telegram, ApiError, AppState};
use agent_core::ConnectorKind;
use std::{future::Future, pin::Pin};

use crate::plugins::poll_hosted_plugin_connectors;

type ConnectorPollFuture<'a> = Pin<Box<dyn Future<Output = Result<usize, ApiError>> + Send + 'a>>;
type ConnectorPoller = for<'a> fn(&'a AppState) -> ConnectorPollFuture<'a>;

#[derive(Clone, Copy)]
struct ConnectorRuntime {
    kind: ConnectorKind,
    display_name: &'static str,
    log_category: &'static str,
    poll: Option<ConnectorPoller>,
}

const CONNECTOR_RUNTIMES: [ConnectorRuntime; 10] = [
    ConnectorRuntime {
        kind: ConnectorKind::App,
        display_name: "app",
        log_category: "apps",
        poll: None,
    },
    ConnectorRuntime {
        kind: ConnectorKind::Webhook,
        display_name: "webhook",
        log_category: "webhooks",
        poll: None,
    },
    ConnectorRuntime {
        kind: ConnectorKind::Inbox,
        display_name: "inbox",
        log_category: "inboxes",
        poll: Some(poll_inbox_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Telegram,
        display_name: "telegram",
        log_category: "telegram",
        poll: Some(poll_telegram_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Discord,
        display_name: "discord",
        log_category: "discord",
        poll: Some(poll_discord_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Slack,
        display_name: "slack",
        log_category: "slack",
        poll: Some(poll_slack_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::HomeAssistant,
        display_name: "home assistant",
        log_category: "home_assistant",
        poll: Some(poll_home_assistant_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Signal,
        display_name: "signal",
        log_category: "signal",
        poll: Some(poll_signal_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Gmail,
        display_name: "gmail",
        log_category: "gmail",
        poll: Some(poll_gmail_runtime),
    },
    ConnectorRuntime {
        kind: ConnectorKind::Brave,
        display_name: "brave",
        log_category: "brave",
        poll: None,
    },
];

pub(super) async fn poll_enabled_connectors(state: &AppState) -> Result<usize, ApiError> {
    let mut queued = 0usize;
    for runtime in CONNECTOR_RUNTIMES {
        if let Some(poll) = runtime.poll {
            queued += poll(state).await?;
        }
    }
    queued += poll_hosted_plugin_connectors(state).await?;
    Ok(queued)
}

pub(super) fn connector_display_name(kind: ConnectorKind) -> &'static str {
    connector_runtime(kind).display_name
}

pub(super) fn connector_log_category(kind: ConnectorKind) -> &'static str {
    connector_runtime(kind).log_category
}

fn connector_runtime(kind: ConnectorKind) -> ConnectorRuntime {
    CONNECTOR_RUNTIMES
        .iter()
        .copied()
        .find(|runtime| runtime.kind == kind)
        .expect("connector runtime metadata must exist for every connector kind")
}

fn poll_inbox_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move {
        let connectors = {
            let config = state.config.read().await;
            config
                .inbox_connectors
                .iter()
                .filter(|connector| connector.enabled)
                .cloned()
                .collect::<Vec<_>>()
        };
        let mut queued = 0usize;
        for connector in connectors {
            let (_, queued_missions) = inbox::process_inbox_connector(state, &connector)?;
            queued += queued_missions;
        }
        Ok(queued)
    })
}

fn poll_telegram_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { telegram::poll_telegram_connectors(state).await })
}

fn poll_discord_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { discord::poll_discord_connectors(state).await })
}

fn poll_slack_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { slack::poll_slack_connectors(state).await })
}

fn poll_home_assistant_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { home_assistant::poll_home_assistant_connectors(state).await })
}

fn poll_signal_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { signal::poll_signal_connectors(state).await })
}

fn poll_gmail_runtime(state: &AppState) -> ConnectorPollFuture<'_> {
    Box::pin(async move { gmail::poll_gmail_connectors(state).await })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_registry_covers_every_connector_kind() {
        let kinds = [
            ConnectorKind::App,
            ConnectorKind::Webhook,
            ConnectorKind::Inbox,
            ConnectorKind::Telegram,
            ConnectorKind::Discord,
            ConnectorKind::Slack,
            ConnectorKind::HomeAssistant,
            ConnectorKind::Signal,
            ConnectorKind::Gmail,
            ConnectorKind::Brave,
        ];

        for kind in kinds {
            let runtime = connector_runtime(kind);
            assert_eq!(runtime.kind, kind);
            assert!(!runtime.display_name.is_empty());
            assert!(!runtime.log_category.is_empty());
        }
    }

    #[test]
    fn pollable_connector_runtimes_match_background_connector_support() {
        let pollable_kinds = CONNECTOR_RUNTIMES
            .iter()
            .filter(|runtime| runtime.poll.is_some())
            .map(|runtime| runtime.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            pollable_kinds,
            vec![
                ConnectorKind::Inbox,
                ConnectorKind::Telegram,
                ConnectorKind::Discord,
                ConnectorKind::Slack,
                ConnectorKind::HomeAssistant,
                ConnectorKind::Signal,
                ConnectorKind::Gmail,
            ]
        );
    }
}
