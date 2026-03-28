use std::collections::HashSet;

use anyhow::{anyhow, Result};

use crate::*;

impl AppConfig {
    pub fn validate_dashboard_mutation(&self) -> Result<()> {
        if self.daemon.host.trim().is_empty() {
            return Err(anyhow!("daemon.host must not be empty"));
        }
        if self.daemon.port == 0 {
            return Err(anyhow!("daemon.port must be greater than 0"));
        }
        if self.daemon.token.trim().is_empty() {
            return Err(anyhow!("daemon.token must not be empty"));
        }

        validate_unique_non_empty(
            self.providers.iter().map(|provider| provider.id.as_str()),
            "provider ids",
        )?;
        validate_unique_non_empty(
            self.aliases.iter().map(|alias| alias.alias.as_str()),
            "alias names",
        )?;
        validate_unique_non_empty(
            self.mcp_servers.iter().map(|server| server.id.as_str()),
            "MCP server ids",
        )?;
        validate_unique_non_empty(
            self.app_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "app connector ids",
        )?;
        validate_unique_non_empty(
            self.plugins.iter().map(|plugin| plugin.id.as_str()),
            "plugin ids",
        )?;
        validate_unique_non_empty(
            self.webhook_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "webhook connector ids",
        )?;
        validate_unique_non_empty(
            self.inbox_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "inbox connector ids",
        )?;
        validate_unique_non_empty(
            self.telegram_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "telegram connector ids",
        )?;
        validate_unique_non_empty(
            self.discord_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "discord connector ids",
        )?;
        validate_unique_non_empty(
            self.slack_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "slack connector ids",
        )?;
        validate_unique_non_empty(
            self.home_assistant_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "home assistant connector ids",
        )?;
        validate_unique_non_empty(
            self.signal_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "signal connector ids",
        )?;
        validate_unique_non_empty(
            self.gmail_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "gmail connector ids",
        )?;
        validate_unique_non_empty(
            self.brave_connectors
                .iter()
                .map(|connector| connector.id.as_str()),
            "brave connector ids",
        )?;

        for provider in &self.providers {
            if provider.base_url.trim().is_empty() {
                return Err(anyhow!(
                    "provider '{}' must include a base_url",
                    provider.id
                ));
            }
        }

        for alias in &self.aliases {
            if alias.provider_id.trim().is_empty() {
                return Err(anyhow!(
                    "alias '{}' must include a provider_id",
                    alias.alias
                ));
            }
            if self.resolve_provider(&alias.provider_id).is_none() {
                return Err(anyhow!(
                    "alias '{}' references unknown provider '{}'",
                    alias.alias,
                    alias.provider_id
                ));
            }
            if alias.model.trim().is_empty() {
                return Err(anyhow!("alias '{}' must include a model", alias.alias));
            }
        }

        if let Some(main_alias) = self.main_agent_alias.as_deref() {
            if self.get_alias(main_alias).is_none() {
                return Err(anyhow!(
                    "main_agent_alias '{}' does not exist in aliases",
                    main_alias
                ));
            }
        }

        for connector in &self.webhook_connectors {
            if connector.prompt_template.trim().is_empty() {
                return Err(anyhow!(
                    "webhook connector '{}' must include a prompt_template",
                    connector.id
                ));
            }
        }

        for connector in &self.inbox_connectors {
            if connector.path.as_os_str().is_empty() {
                return Err(anyhow!(
                    "inbox connector '{}' must include a path",
                    connector.id
                ));
            }
        }

        for connector in &self.home_assistant_connectors {
            if connector.base_url.trim().is_empty() {
                return Err(anyhow!(
                    "home assistant connector '{}' must include a base_url",
                    connector.id
                ));
            }
        }

        for connector in &self.signal_connectors {
            if connector.account.trim().is_empty() {
                return Err(anyhow!(
                    "signal connector '{}' must include an account",
                    connector.id
                ));
            }
        }

        if self.embedding.enabled {
            let provider_id = self
                .embedding
                .provider_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("embedding.provider_id must not be empty when enabled"))?;
            if self.resolve_provider(provider_id).is_none() {
                return Err(anyhow!(
                    "embedding.provider_id '{}' does not match a configured provider",
                    provider_id
                ));
            }
            let model = self
                .embedding
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("embedding.model must not be empty when enabled"))?;
            if model.is_empty() {
                return Err(anyhow!("embedding.model must not be empty when enabled"));
            }
        }

        Ok(())
    }

    pub fn get_provider(&self, provider_id: &str) -> Option<&ProviderConfig> {
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
    }

    pub fn projected_plugin_providers(&self) -> Vec<ProviderConfig> {
        projected_plugin_providers(&self.plugins)
    }

    pub fn resolve_provider(&self, provider_id: &str) -> Option<ProviderConfig> {
        self.get_provider(provider_id).cloned().or_else(|| {
            self.projected_plugin_providers()
                .into_iter()
                .find(|provider| provider.id == provider_id)
        })
    }

    pub fn is_projected_plugin_provider(&self, provider_id: &str) -> bool {
        parse_plugin_provider_id(provider_id).is_some()
            && self
                .projected_plugin_providers()
                .into_iter()
                .any(|provider| provider.id == provider_id)
    }

    pub fn all_providers(&self) -> Vec<ProviderConfig> {
        let mut providers = self.providers.clone();
        providers.extend(self.projected_plugin_providers());
        providers
    }

    pub fn get_alias(&self, alias: &str) -> Option<&ModelAlias> {
        self.aliases.iter().find(|entry| entry.alias == alias)
    }

    pub fn provider_delegation_enabled(&self, provider_id: &str) -> bool {
        self.delegation.provider_enabled(provider_id)
    }

    pub fn main_alias(&self) -> Result<&ModelAlias> {
        let alias = self
            .main_agent_alias
            .as_deref()
            .ok_or_else(|| anyhow!("no main agent alias configured"))?;
        self.get_alias(alias)
            .ok_or_else(|| anyhow!("configured main alias '{alias}' is missing"))
    }

    pub fn has_configured_main_alias_provider(&self) -> bool {
        self.main_alias()
            .ok()
            .and_then(|alias| self.resolve_provider(&alias.provider_id))
            .is_some_and(|provider| provider.has_saved_access_reference())
    }

    #[deprecated(
        note = "metadata-only helper; use daemon or CLI runtime readiness checks for actual usability"
    )]
    pub fn has_usable_main_alias(&self) -> bool {
        self.has_configured_main_alias_provider()
    }

    pub fn alias_target_summary(&self, alias_name: &str) -> Option<MainTargetSummary> {
        let alias = self.get_alias(alias_name)?;
        let provider = self.resolve_provider(&alias.provider_id)?;
        Some(MainTargetSummary {
            alias: alias.alias.clone(),
            provider_id: provider.id.clone(),
            provider_display_name: if provider.display_name.trim().is_empty() {
                provider.id.clone()
            } else {
                provider.display_name.clone()
            },
            model: alias.model.clone(),
        })
    }

    pub fn main_target_summary(&self) -> Option<MainTargetSummary> {
        self.main_agent_alias
            .as_deref()
            .and_then(|alias_name| self.alias_target_summary(alias_name))
    }

    pub fn next_available_provider_id(&self, preferred: &str) -> String {
        self.next_available_provider_id_excluding(preferred, None)
    }

    pub fn next_available_provider_id_excluding(
        &self,
        preferred: &str,
        existing_id: Option<&str>,
    ) -> String {
        let preferred = preferred.trim();
        let preferred = if preferred.is_empty() {
            "provider"
        } else {
            preferred
        };
        if !self.provider_id_taken(preferred, existing_id) {
            return preferred.to_string();
        }

        let mut index = 2;
        loop {
            let candidate = format!("{preferred}-{index}");
            if !self.provider_id_taken(&candidate, existing_id) {
                return candidate;
            }
            index += 1;
        }
    }

    pub fn next_available_alias_name(&self, preferred: &str) -> String {
        self.next_available_alias_name_excluding(preferred, None)
    }

    pub fn next_available_alias_name_excluding(
        &self,
        preferred: &str,
        existing_alias: Option<&str>,
    ) -> String {
        let preferred = preferred.trim();
        let preferred = if preferred.is_empty() {
            "alias"
        } else {
            preferred
        };
        if !self.alias_name_taken(preferred, existing_alias) {
            return preferred.to_string();
        }

        let mut index = 2;
        loop {
            let candidate = format!("{preferred}-{index}");
            if !self.alias_name_taken(&candidate, existing_alias) {
                return candidate;
            }
            index += 1;
        }
    }

    pub fn default_alias_name_for(&self, provider_id: &str, model: &str) -> String {
        let preferred = if self.main_agent_alias.is_none() && self.aliases.is_empty() {
            "main".to_string()
        } else {
            let model_slug = model
                .chars()
                .map(|ch| match ch {
                    'a'..='z' | 'A'..='Z' | '0'..='9' => ch.to_ascii_lowercase(),
                    _ => '-',
                })
                .collect::<String>()
                .split('-')
                .filter(|segment| !segment.is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join("-");
            if model_slug.is_empty() {
                provider_id.to_string()
            } else {
                format!("{provider_id}-{model_slug}")
            }
        };
        self.next_available_alias_name(&preferred)
    }

    fn provider_id_taken(&self, candidate: &str, existing_id: Option<&str>) -> bool {
        self.resolve_provider(candidate)
            .is_some_and(|provider| Some(provider.id.as_str()) != existing_id)
    }

    fn alias_name_taken(&self, candidate: &str, existing_alias: Option<&str>) -> bool {
        self.get_alias(candidate)
            .is_some_and(|alias| Some(alias.alias.as_str()) != existing_alias)
    }

    pub fn upsert_provider(&mut self, provider: ProviderConfig) {
        if let Some(existing) = self
            .providers
            .iter_mut()
            .find(|entry| entry.id == provider.id)
        {
            *existing = provider;
        } else {
            self.providers.push(provider);
        }
    }

    pub fn upsert_alias(&mut self, alias: ModelAlias) {
        if let Some(existing) = self
            .aliases
            .iter_mut()
            .find(|entry| entry.alias == alias.alias)
        {
            *existing = alias;
        } else {
            self.aliases.push(alias);
        }
    }

    pub fn remove_provider(&mut self, provider_id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|provider| provider.id != provider_id);
        let removed = before != self.providers.len();
        if removed {
            self.aliases
                .retain(|alias| alias.provider_id != provider_id);
            if self
                .main_agent_alias
                .as_deref()
                .and_then(|alias_name| self.get_alias(alias_name))
                .is_none()
            {
                self.main_agent_alias = None;
            }
            self.delegation
                .disabled_provider_ids
                .retain(|entry| entry != provider_id);
        }
        removed
    }

    pub fn remove_alias(&mut self, alias_name: &str) -> bool {
        let before = self.aliases.len();
        self.aliases.retain(|alias| alias.alias != alias_name);
        let removed = before != self.aliases.len();
        if removed && self.main_agent_alias.as_deref() == Some(alias_name) {
            self.main_agent_alias = None;
        }
        removed
    }

    pub fn upsert_mcp_server(&mut self, server: McpServerConfig) {
        if let Some(existing) = self
            .mcp_servers
            .iter_mut()
            .find(|entry| entry.id == server.id)
        {
            *existing = server;
        } else {
            self.mcp_servers.push(server);
        }
    }

    pub fn remove_mcp_server(&mut self, id: &str) -> bool {
        let before = self.mcp_servers.len();
        self.mcp_servers.retain(|server| server.id != id);
        before != self.mcp_servers.len()
    }

    pub fn upsert_app_connector(&mut self, connector: AppConnectorConfig) {
        if let Some(existing) = self
            .app_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.app_connectors.push(connector);
        }
    }

    pub fn remove_app_connector(&mut self, id: &str) -> bool {
        let before = self.app_connectors.len();
        self.app_connectors.retain(|connector| connector.id != id);
        before != self.app_connectors.len()
    }

    pub fn get_plugin(&self, id: &str) -> Option<&InstalledPluginConfig> {
        self.plugins.iter().find(|plugin| plugin.id == id)
    }

    pub fn upsert_plugin(&mut self, plugin: InstalledPluginConfig) {
        if let Some(existing) = self.plugins.iter_mut().find(|entry| entry.id == plugin.id) {
            *existing = plugin;
        } else {
            self.plugins.push(plugin);
        }
    }

    pub fn remove_plugin(&mut self, id: &str) -> bool {
        let before = self.plugins.len();
        self.plugins.retain(|plugin| plugin.id != id);
        before != self.plugins.len()
    }

    pub fn upsert_webhook_connector(&mut self, connector: WebhookConnectorConfig) {
        if let Some(existing) = self
            .webhook_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.webhook_connectors.push(connector);
        }
    }

    pub fn remove_webhook_connector(&mut self, id: &str) -> bool {
        let before = self.webhook_connectors.len();
        self.webhook_connectors
            .retain(|connector| connector.id != id);
        before != self.webhook_connectors.len()
    }

    pub fn upsert_inbox_connector(&mut self, connector: InboxConnectorConfig) {
        if let Some(existing) = self
            .inbox_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.inbox_connectors.push(connector);
        }
    }

    pub fn remove_inbox_connector(&mut self, id: &str) -> bool {
        let before = self.inbox_connectors.len();
        self.inbox_connectors.retain(|connector| connector.id != id);
        before != self.inbox_connectors.len()
    }

    pub fn upsert_telegram_connector(&mut self, connector: TelegramConnectorConfig) {
        if let Some(existing) = self
            .telegram_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.telegram_connectors.push(connector);
        }
    }

    pub fn remove_telegram_connector(&mut self, id: &str) -> bool {
        let before = self.telegram_connectors.len();
        self.telegram_connectors
            .retain(|connector| connector.id != id);
        before != self.telegram_connectors.len()
    }

    pub fn upsert_discord_connector(&mut self, connector: DiscordConnectorConfig) {
        if let Some(existing) = self
            .discord_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.discord_connectors.push(connector);
        }
    }

    pub fn remove_discord_connector(&mut self, id: &str) -> bool {
        let before = self.discord_connectors.len();
        self.discord_connectors
            .retain(|connector| connector.id != id);
        before != self.discord_connectors.len()
    }

    pub fn upsert_slack_connector(&mut self, connector: SlackConnectorConfig) {
        if let Some(existing) = self
            .slack_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.slack_connectors.push(connector);
        }
    }

    pub fn remove_slack_connector(&mut self, id: &str) -> bool {
        let before = self.slack_connectors.len();
        self.slack_connectors.retain(|connector| connector.id != id);
        before != self.slack_connectors.len()
    }

    pub fn upsert_home_assistant_connector(&mut self, connector: HomeAssistantConnectorConfig) {
        if let Some(existing) = self
            .home_assistant_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.home_assistant_connectors.push(connector);
        }
    }

    pub fn remove_home_assistant_connector(&mut self, id: &str) -> bool {
        let before = self.home_assistant_connectors.len();
        self.home_assistant_connectors
            .retain(|connector| connector.id != id);
        before != self.home_assistant_connectors.len()
    }

    pub fn upsert_signal_connector(&mut self, connector: SignalConnectorConfig) {
        if let Some(existing) = self
            .signal_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.signal_connectors.push(connector);
        }
    }

    pub fn remove_signal_connector(&mut self, id: &str) -> bool {
        let before = self.signal_connectors.len();
        self.signal_connectors
            .retain(|connector| connector.id != id);
        before != self.signal_connectors.len()
    }

    pub fn upsert_gmail_connector(&mut self, connector: GmailConnectorConfig) {
        if let Some(existing) = self
            .gmail_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.gmail_connectors.push(connector);
        }
    }

    pub fn remove_gmail_connector(&mut self, id: &str) -> bool {
        let before = self.gmail_connectors.len();
        self.gmail_connectors.retain(|connector| connector.id != id);
        before != self.gmail_connectors.len()
    }

    pub fn upsert_brave_connector(&mut self, connector: BraveConnectorConfig) {
        if let Some(existing) = self
            .brave_connectors
            .iter_mut()
            .find(|entry| entry.id == connector.id)
        {
            *existing = connector;
        } else {
            self.brave_connectors.push(connector);
        }
    }

    pub fn remove_brave_connector(&mut self, id: &str) -> bool {
        let before = self.brave_connectors.len();
        self.brave_connectors.retain(|connector| connector.id != id);
        before != self.brave_connectors.len()
    }
}

fn validate_unique_non_empty<'a>(
    values: impl IntoIterator<Item = &'a str>,
    label: &str,
) -> Result<()> {
    let mut seen = HashSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("{label} must not contain empty values"));
        }
        if !seen.insert(trimmed.to_string()) {
            return Err(anyhow!("{label} must be unique; duplicate '{}'", trimmed));
        }
    }
    Ok(())
}
