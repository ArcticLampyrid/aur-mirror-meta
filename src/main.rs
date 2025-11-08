use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info};

mod app_state;
mod aur_fetcher;
mod config;
mod database;
mod rpc_server;
mod srcinfo_parse;
mod syncer;
mod types;

use app_state::AppState;
use config::Config;
use rpc_server::RpcServer;
use syncer::Syncer;

#[derive(Parser)]
#[command(name = "aur-mirror-meta")]
#[command(about = "AUR Mirror Meta Tool")]
struct Cli {
    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to GitHub
    Login {
        #[arg(long)]
        token: String,
    },
    /// Sync metadata from AUR GitHub Mirror
    Sync,
    /// Start HTTP RPC server
    Serve {
        /// Address to bind to
        #[arg(long, default_values_t = vec!["[::]:3000".to_string()])]
        bind: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let config = Config::new(cli.config);
    if let Some(config_path) = config.config_path() {
        info!("Config file: {}", config_path.display());
    }

    let db_path = config
        .db_path()
        .ok_or(anyhow!("Database path is not configured."))?;
    info!("Database file: {}", db_path);

    let github_token = config.github_token().or_else(|| {
        debug!("GitHub token is not set. Try `gh auth token`.");
        Command::new("gh")
            .args(["auth", "token"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !token.is_empty() {
                        info!("GitHub token obtained from `gh` CLI.");
                        return Some(token);
                    }
                }
                None
            })
    });

    let app_state = AppState::new(&db_path, github_token).await?;

    match cli.command {
        Commands::Login { token } => {
            config.modify_file(|model| {
                model.github_token = Some(token);
            })?;
            info!("GitHub token saved to config file.");
        }
        Commands::Sync => {
            let syncer = Syncer::new(app_state);
            syncer.sync().await?;
        }
        Commands::Serve { bind } => {
            let server = RpcServer::new(app_state);
            server.run(bind.iter()).await?;
        }
    }

    Ok(())
}
