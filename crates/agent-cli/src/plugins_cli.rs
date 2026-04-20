use agent_core::{
    InstalledPluginConfig, PluginDoctorReport, PluginInstallRequest, PluginPermissions,
    PluginStateUpdateRequest, PluginUpdateRequest,
};
use anyhow::{anyhow, Result};
use clap::{Args, Subcommand};

use super::{storage_plugins, try_daemon, Storage};

#[derive(Subcommand)]
pub(crate) enum PluginCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Install(PluginInstallArgs),
    Update {
        id: String,
        #[arg(long, value_name = "SOURCE")]
        source: Option<String>,
    },
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Trust {
        id: String,
    },
    Untrust {
        id: String,
    },
    Grant(PluginPermissionArgs),
    Revoke(PluginPermissionArgs),
    Pin {
        id: String,
    },
    Unpin {
        id: String,
    },
    Doctor {
        id: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Args)]
pub(crate) struct PluginInstallArgs {
    #[arg(value_name = "SOURCE")]
    source: String,
    #[arg(long, default_value_t = true)]
    enable: bool,
    #[arg(long, default_value_t = false)]
    trust: bool,
    #[arg(long, default_value_t = false)]
    grant_shell: bool,
    #[arg(long, default_value_t = false)]
    grant_network: bool,
    #[arg(long, default_value_t = false)]
    grant_full_disk: bool,
    #[arg(long, default_value_t = false)]
    pinned: bool,
}

#[derive(Args)]
pub(crate) struct PluginPermissionArgs {
    id: String,
    #[arg(long, default_value_t = false)]
    shell: bool,
    #[arg(long, default_value_t = false)]
    network: bool,
    #[arg(long, default_value_t = false)]
    full_disk: bool,
}

pub(crate) async fn plugin_command(storage: &Storage, command: PluginCommands) -> Result<()> {
    match command {
        PluginCommands::List { json } => {
            let plugins = load_plugins(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&plugins)?);
            } else if plugins.is_empty() {
                println!("No plugins installed.");
            } else {
                for plugin in plugins {
                    print_plugin_summary(&plugin);
                }
            }
        }
        PluginCommands::Get { id, json } => {
            let plugin = load_plugin(storage, &id).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&plugin)?);
            } else {
                print_plugin_detail(&plugin);
            }
        }
        PluginCommands::Install(args) => {
            let plugin = install_plugin(storage, args).await?;
            println!(
                "installed plugin={} version={} enabled={} trusted={} pinned={}",
                plugin.id, plugin.manifest.version, plugin.enabled, plugin.trusted, plugin.pinned
            );
        }
        PluginCommands::Update { id, source } => {
            let plugin = update_plugin(storage, &id, source).await?;
            println!(
                "updated plugin={} version={} integrity={} source={}",
                plugin.id,
                plugin.manifest.version,
                plugin.integrity_sha256,
                plugin.source_reference
            );
        }
        PluginCommands::Remove { id } => {
            remove_plugin(storage, &id).await?;
            println!("removed plugin={id}");
        }
        PluginCommands::Enable { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    enabled: Some(true),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("enabled plugin={}", plugin.id);
        }
        PluginCommands::Disable { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    enabled: Some(false),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("disabled plugin={}", plugin.id);
        }
        PluginCommands::Trust { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    trusted: Some(true),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("trusted plugin={}", plugin.id);
        }
        PluginCommands::Untrust { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    trusted: Some(false),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("untrusted plugin={}", plugin.id);
        }
        PluginCommands::Grant(args) => {
            let plugin = load_plugin(storage, &args.id).await?;
            let requested = permission_args_to_permissions(&args)?;
            let granted = plugin.granted_permissions.union(&requested);
            let plugin = update_plugin_state(
                storage,
                &args.id,
                PluginStateUpdateRequest {
                    trusted: Some(true),
                    granted_permissions: Some(granted),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!(
                "updated plugin={} grants={}",
                plugin.id,
                permission_summary(&plugin.granted_permissions)
            );
        }
        PluginCommands::Revoke(args) => {
            let plugin = load_plugin(storage, &args.id).await?;
            let requested = permission_args_to_permissions(&args)?;
            let plugin = update_plugin_state(
                storage,
                &args.id,
                PluginStateUpdateRequest {
                    granted_permissions: Some(PluginPermissions {
                        shell: plugin.granted_permissions.shell && !requested.shell,
                        network: plugin.granted_permissions.network && !requested.network,
                        full_disk: plugin.granted_permissions.full_disk && !requested.full_disk,
                    }),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!(
                "updated plugin={} grants={}",
                plugin.id,
                permission_summary(&plugin.granted_permissions)
            );
        }
        PluginCommands::Pin { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    pinned: Some(true),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("pinned plugin={}", plugin.id);
        }
        PluginCommands::Unpin { id } => {
            let plugin = update_plugin_state(
                storage,
                &id,
                PluginStateUpdateRequest {
                    pinned: Some(false),
                    ..PluginStateUpdateRequest::default()
                },
            )
            .await?;
            println!("unpinned plugin={}", plugin.id);
        }
        PluginCommands::Doctor { id, json } => {
            let mut reports = doctor_plugins(storage, id.as_deref()).await?;
            reports.sort_by(|left, right| left.id.cmp(&right.id));
            if json {
                println!("{}", serde_json::to_string_pretty(&reports)?);
            } else if reports.is_empty() {
                println!("No plugins installed.");
            } else {
                for report in reports {
                    println!(
                        "{} ok={} runtime_ready={} enabled={} trusted={} grants={} declared={} tools={} connectors={} providers={} detail={}",
                        report.id,
                        report.ok,
                        report.runtime_ready,
                        report.enabled,
                        report.trusted,
                        permission_summary(&report.granted_permissions),
                        permission_summary(&report.declared_permissions),
                        report.tools,
                        report.connectors,
                        report.provider_adapters,
                        report.detail
                    );
                }
            }
        }
    }
    Ok(())
}

async fn load_plugins(storage: &Storage) -> Result<Vec<InstalledPluginConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/plugins").await
    } else {
        let mut plugins = storage.load_config()?.plugins;
        plugins.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(plugins)
    }
}

async fn load_plugin(storage: &Storage, id: &str) -> Result<InstalledPluginConfig> {
    if let Some(client) = try_daemon(storage).await? {
        return client.get(&format!("/v1/plugins/{id}")).await;
    }

    let config = storage.load_config()?;
    config
        .get_plugin(id)
        .cloned()
        .ok_or_else(|| anyhow!("unknown plugin '{id}'"))
}

async fn install_plugin(
    storage: &Storage,
    args: PluginInstallArgs,
) -> Result<InstalledPluginConfig> {
    let request = PluginInstallRequest {
        source: Some(args.source.clone()),
        source_path: None,
        enabled: Some(args.enable),
        trusted: Some(args.trust),
        granted_permissions: Some(PluginPermissions {
            shell: args.grant_shell,
            network: args.grant_network,
            full_disk: args.grant_full_disk,
        }),
        pinned: args.pinned,
    };
    if let Some(client) = try_daemon(storage).await? {
        return client.post("/v1/plugins/install", &request).await;
    }

    let resolved = storage_plugins::resolve_plugin_install_request(storage.paths(), &request)?;
    let mut config = storage.load_config()?;
    let existing = config.get_plugin(&resolved.manifest.id).cloned();
    let installed =
        storage_plugins::install_plugin_package(storage.paths(), &request, existing.as_ref())?;
    config.upsert_plugin(installed.clone());
    storage.save_config(&config)?;
    Ok(installed)
}

async fn remove_plugin(storage: &Storage, id: &str) -> Result<()> {
    if let Some(client) = try_daemon(storage).await? {
        let _: serde_json::Value = client.delete(&format!("/v1/plugins/{id}")).await?;
        return Ok(());
    }

    let mut config = storage.load_config()?;
    let plugin = config
        .get_plugin(id)
        .cloned()
        .ok_or_else(|| anyhow!("unknown plugin '{id}'"))?;
    config.remove_plugin(id);
    storage.save_config(&config)?;
    storage_plugins::uninstall_plugin_package(storage.paths(), &plugin)?;
    Ok(())
}

async fn update_plugin(
    storage: &Storage,
    id: &str,
    source: Option<String>,
) -> Result<InstalledPluginConfig> {
    let request = PluginUpdateRequest {
        source,
        source_path: None,
    };
    if let Some(client) = try_daemon(storage).await? {
        return client
            .post(&format!("/v1/plugins/{id}/update"), &request)
            .await;
    }

    let mut config = storage.load_config()?;
    let existing = config
        .get_plugin(id)
        .cloned()
        .ok_or_else(|| anyhow!("unknown plugin '{id}'"))?;
    let updated = storage_plugins::update_plugin_package(storage.paths(), &existing, &request)?;
    config.upsert_plugin(updated.clone());
    storage.save_config(&config)?;
    Ok(updated)
}

async fn update_plugin_state(
    storage: &Storage,
    id: &str,
    payload: PluginStateUpdateRequest,
) -> Result<InstalledPluginConfig> {
    if let Some(client) = try_daemon(storage).await? {
        return client.put(&format!("/v1/plugins/{id}"), &payload).await;
    }

    let mut config = storage.load_config()?;
    let plugin = config
        .plugins
        .iter_mut()
        .find(|plugin| plugin.id == id)
        .ok_or_else(|| anyhow!("unknown plugin '{id}'"))?;
    if let Some(enabled) = payload.enabled {
        plugin.enabled = enabled;
    }
    if let Some(granted_permissions) = payload.granted_permissions.as_ref() {
        if !plugin.trusted && payload.trusted != Some(true) {
            return Err(anyhow!(
                "permission grants require an explicit trust review"
            ));
        }
        plugin.granted_permissions =
            granted_permissions.intersection(&plugin.declared_permissions());
    }
    if let Some(trusted) = payload.trusted {
        plugin.trusted = trusted;
        if trusted {
            plugin.reviewed_integrity_sha256 = plugin.integrity_sha256.clone();
            plugin.reviewed_at = Some(chrono::Utc::now());
            plugin.granted_permissions = plugin
                .granted_permissions
                .intersection(&plugin.declared_permissions());
        } else {
            plugin.granted_permissions = PluginPermissions::default();
            plugin.reviewed_integrity_sha256.clear();
            plugin.reviewed_at = None;
        }
    }
    if let Some(pinned) = payload.pinned {
        plugin.pinned = pinned;
    }
    if payload.granted_permissions.is_some() && plugin.trusted {
        plugin.reviewed_integrity_sha256 = plugin.integrity_sha256.clone();
        plugin.reviewed_at = Some(chrono::Utc::now());
    }
    plugin.updated_at = chrono::Utc::now();
    let plugin = plugin.clone();
    storage.save_config(&config)?;
    Ok(plugin)
}

async fn doctor_plugins(storage: &Storage, id: Option<&str>) -> Result<Vec<PluginDoctorReport>> {
    if let Some(client) = try_daemon(storage).await? {
        return if let Some(id) = id {
            let report: PluginDoctorReport =
                client.get(&format!("/v1/plugins/{id}/doctor")).await?;
            Ok(vec![report])
        } else {
            client.get("/v1/plugins/doctor").await
        };
    }

    let config = storage.load_config()?;
    let plugins = match id {
        Some(id) => vec![config
            .get_plugin(id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown plugin '{id}'"))?],
        None => config.plugins,
    };
    Ok(plugins
        .into_iter()
        .map(|plugin| storage_plugins::doctor_plugin(&plugin))
        .collect())
}

fn print_plugin_summary(plugin: &InstalledPluginConfig) {
    println!(
        "{} {} enabled={} trusted={} reviewed={} pinned={} grants={} {}",
        plugin.id,
        plugin.manifest.version,
        plugin.enabled,
        plugin.trusted,
        plugin.review_current(),
        plugin.pinned,
        permission_summary(&plugin.granted_permissions),
        capability_summary(plugin)
    );
}

fn print_plugin_detail(plugin: &InstalledPluginConfig) {
    println!("id={}", plugin.id);
    println!("name={}", plugin.manifest.name);
    println!("version={}", plugin.manifest.version);
    println!("enabled={}", plugin.enabled);
    println!("trusted={}", plugin.trusted);
    println!("review_current={}", plugin.review_current());
    println!("pinned={}", plugin.pinned);
    println!(
        "declared_permissions={}",
        permission_summary(&plugin.declared_permissions())
    );
    println!(
        "granted_permissions={}",
        permission_summary(&plugin.granted_permissions)
    );
    println!(
        "reviewed_integrity_sha256={}",
        plugin.reviewed_integrity_sha256
    );
    println!(
        "reviewed_at={}",
        plugin
            .reviewed_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("source_reference={}", plugin.source_reference);
    println!("resolved_source_path={}", plugin.source_path.display());
    println!("install_dir={}", plugin.install_dir.display());
    println!("source_kind={:?}", plugin.source_kind);
    println!("integrity_sha256={}", plugin.integrity_sha256);
    println!("capabilities={}", capability_summary(plugin));

    if !plugin.manifest.tools.is_empty() {
        println!("tools:");
        for tool in &plugin.manifest.tools {
            println!("  {} -> {}", tool.name, tool.command);
        }
    }
    if !plugin.manifest.connectors.is_empty() {
        println!("connectors:");
        for connector in &plugin.manifest.connectors {
            println!("  {} ({:?})", connector.id, connector.kind);
        }
    }
    if !plugin.manifest.provider_adapters.is_empty() {
        println!("provider_adapters:");
        for adapter in &plugin.manifest.provider_adapters {
            println!("  {} ({:?})", adapter.id, adapter.provider_kind);
        }
    }
}

fn capability_summary(plugin: &InstalledPluginConfig) -> String {
    format!(
        "tools={} connectors={} providers={}",
        plugin.manifest.tools.len(),
        plugin.manifest.connectors.len(),
        plugin.manifest.provider_adapters.len()
    )
}

fn permission_args_to_permissions(args: &PluginPermissionArgs) -> Result<PluginPermissions> {
    let permissions = PluginPermissions {
        shell: args.shell,
        network: args.network,
        full_disk: args.full_disk,
    };
    if permissions.is_empty() {
        return Err(anyhow!(
            "select at least one permission flag: --shell, --network, or --full-disk"
        ));
    }
    Ok(permissions)
}

fn permission_summary(permissions: &PluginPermissions) -> String {
    let mut parts = Vec::new();
    if permissions.shell {
        parts.push("shell");
    }
    if permissions.network {
        parts.push("network");
    }
    if permissions.full_disk {
        parts.push("full_disk");
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(",")
    }
}
