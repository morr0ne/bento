use std::fs;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install,
}

fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Install => {
            info!("Installing...");
            install().expect("Failed to install");
            info!("Installed")
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageJson {}

pub fn install() -> Result<()> {
    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    dbg!(package_json);

    Ok(())
}
