use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Install => {
            info!("Installing...");
            install().await.expect("Failed to install");
            info!("Installed")
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageJson {
    pub name: String,
    pub version: String,
    pub dependencies: Option<BTreeMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<BTreeMap<String, String>>,
}

pub async fn install() -> Result<()> {
    let mut client = Client::new();

    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    dbg!(&package_json);

    if let Some(dev_dependencies) = package_json.dev_dependencies {
        for (dep, version) in dev_dependencies {
            let metadata: Value = client
                .get(format!("https://registry.npmjs.org/{dep}/{version}"))
                .send()
                .await?
                .json()
                .await?;

            dbg!(metadata);
        }
    }

    Ok(())
}
