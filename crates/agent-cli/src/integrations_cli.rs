use std::path::PathBuf;

use agent_core::{
    AppConnectorConfig, AppConnectorUpsertRequest, McpServerConfig, McpServerUpsertRequest,
};
use anyhow::{anyhow, bail, Result};
use clap::{Args, Subcommand};

use super::{load_schema_file, try_daemon, Storage};

#[derive(Subcommand)]
pub(crate) enum McpCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(McpAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum AppCommands {
    List {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Get {
        id: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    Add(AppAddArgs),
    Remove {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
}

#[derive(Args)]
pub(crate) struct McpAddArgs {
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) description: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long = "arg")]
    pub(crate) args: Vec<String>,
    #[arg(long = "tool-name")]
    pub(crate) tool_name: String,
    #[arg(long = "schema-file")]
    pub(crate) schema_file: PathBuf,
    #[arg(long)]
    pub(crate) cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    pub(crate) enabled: bool,
}

#[derive(Args)]
pub(crate) struct AppAddArgs {
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) description: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long = "arg")]
    pub(crate) args: Vec<String>,
    #[arg(long = "tool-name")]
    pub(crate) tool_name: String,
    #[arg(long = "schema-file")]
    pub(crate) schema_file: PathBuf,
    #[arg(long)]
    pub(crate) cwd: Option<PathBuf>,
    #[arg(long, default_value_t = true)]
    pub(crate) enabled: bool,
}

pub(crate) async fn mcp_command(storage: &Storage, command: McpCommands) -> Result<()> {
    match command {
        McpCommands::List { json } => {
            let servers = load_mcp_servers(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&servers)?);
            } else {
                for server in servers {
                    println!(
                        "{} [{}] tool={} enabled={} cmd={} {}",
                        server.id,
                        server.name,
                        server.tool_name,
                        server.enabled,
                        server.command,
                        server.args.join(" ")
                    );
                }
            }
        }
        McpCommands::Get { id, json } => {
            let server = load_mcp_servers(storage)
                .await?
                .into_iter()
                .find(|server| server.id == id)
                .ok_or_else(|| anyhow!("unknown MCP server '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&server)?);
            } else {
                println!("id={}", server.id);
                println!("name={}", server.name);
                println!("tool_name={}", server.tool_name);
                println!("enabled={}", server.enabled);
                println!("command={} {}", server.command, server.args.join(" "));
                println!("schema={}", server.input_schema_json);
            }
        }
        McpCommands::Add(args) => {
            let server = McpServerConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                command: args.command,
                args: args.args,
                tool_name: args.tool_name,
                input_schema_json: load_schema_file(&args.schema_file)?,
                enabled: args.enabled,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: McpServerConfig = client
                    .post(
                        "/v1/mcp",
                        &McpServerUpsertRequest {
                            server: server.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_mcp_server(server.clone());
                storage.save_config(&config)?;
            }
            println!("mcp_server='{}' configured", args.id);
        }
        McpCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/mcp/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_mcp_server(&id) {
                    bail!("unknown MCP server '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("mcp_server='{}' removed", id);
        }
        McpCommands::Enable { id } => {
            set_mcp_enabled(storage, &id, true).await?;
            println!("mcp_server='{}' enabled", id);
        }
        McpCommands::Disable { id } => {
            set_mcp_enabled(storage, &id, false).await?;
            println!("mcp_server='{}' disabled", id);
        }
    }
    Ok(())
}

pub(crate) async fn app_command(storage: &Storage, command: AppCommands) -> Result<()> {
    match command {
        AppCommands::List { json } => {
            let connectors = load_app_connectors(storage).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connectors)?);
            } else {
                for connector in connectors {
                    println!(
                        "{} [{}] tool={} enabled={} cmd={} {}",
                        connector.id,
                        connector.name,
                        connector.tool_name,
                        connector.enabled,
                        connector.command,
                        connector.args.join(" ")
                    );
                }
            }
        }
        AppCommands::Get { id, json } => {
            let connector = load_app_connectors(storage)
                .await?
                .into_iter()
                .find(|connector| connector.id == id)
                .ok_or_else(|| anyhow!("unknown app connector '{id}'"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&connector)?);
            } else {
                println!("id={}", connector.id);
                println!("name={}", connector.name);
                println!("tool_name={}", connector.tool_name);
                println!("enabled={}", connector.enabled);
                println!("command={} {}", connector.command, connector.args.join(" "));
                println!("schema={}", connector.input_schema_json);
            }
        }
        AppCommands::Add(args) => {
            let connector = AppConnectorConfig {
                id: args.id.clone(),
                name: args.name,
                description: args.description,
                command: args.command,
                args: args.args,
                tool_name: args.tool_name,
                input_schema_json: load_schema_file(&args.schema_file)?,
                enabled: args.enabled,
                cwd: args.cwd,
            };
            if let Some(client) = try_daemon(storage).await? {
                let _: AppConnectorConfig = client
                    .post(
                        "/v1/apps",
                        &AppConnectorUpsertRequest {
                            connector: connector.clone(),
                        },
                    )
                    .await?;
            } else {
                let mut config = storage.load_config()?;
                config.upsert_app_connector(connector.clone());
                storage.save_config(&config)?;
            }
            println!("app_connector='{}' configured", args.id);
        }
        AppCommands::Remove { id } => {
            if let Some(client) = try_daemon(storage).await? {
                let _: serde_json::Value = client.delete(&format!("/v1/apps/{id}")).await?;
            } else {
                let mut config = storage.load_config()?;
                if !config.remove_app_connector(&id) {
                    bail!("unknown app connector '{id}'");
                }
                storage.save_config(&config)?;
            }
            println!("app_connector='{}' removed", id);
        }
        AppCommands::Enable { id } => {
            set_app_enabled(storage, &id, true).await?;
            println!("app_connector='{}' enabled", id);
        }
        AppCommands::Disable { id } => {
            set_app_enabled(storage, &id, false).await?;
            println!("app_connector='{}' disabled", id);
        }
    }
    Ok(())
}

async fn load_mcp_servers(storage: &Storage) -> Result<Vec<McpServerConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/mcp").await
    } else {
        Ok(storage.load_config()?.mcp_servers)
    }
}

async fn load_app_connectors(storage: &Storage) -> Result<Vec<AppConnectorConfig>> {
    if let Some(client) = try_daemon(storage).await? {
        client.get("/v1/apps").await
    } else {
        Ok(storage.load_config()?.app_connectors)
    }
}

async fn set_mcp_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut servers = load_mcp_servers(storage).await?;
    let server = servers
        .iter_mut()
        .find(|server| server.id == id)
        .ok_or_else(|| anyhow!("unknown MCP server '{id}'"))?;
    server.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: McpServerConfig = client
            .post(
                "/v1/mcp",
                &McpServerUpsertRequest {
                    server: server.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_mcp_server(server.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}

async fn set_app_enabled(storage: &Storage, id: &str, enabled: bool) -> Result<()> {
    let mut connectors = load_app_connectors(storage).await?;
    let connector = connectors
        .iter_mut()
        .find(|connector| connector.id == id)
        .ok_or_else(|| anyhow!("unknown app connector '{id}'"))?;
    connector.enabled = enabled;
    if let Some(client) = try_daemon(storage).await? {
        let _: AppConnectorConfig = client
            .post(
                "/v1/apps",
                &AppConnectorUpsertRequest {
                    connector: connector.clone(),
                },
            )
            .await?;
    } else {
        let mut config = storage.load_config()?;
        config.upsert_app_connector(connector.clone());
        storage.save_config(&config)?;
    }
    Ok(())
}
