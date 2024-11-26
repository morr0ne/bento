use std::{collections::HashMap, fs};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use semver::{Version as SemverVersion, VersionReq};
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
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Metadata {
    pub name: String,
    pub modified: String,
    #[serde(rename = "dist-tags")]
    pub dist_tags: HashMap<String, String>,
    pub versions: HashMap<String, Version>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Version {
    pub name: String,
    pub version: String,
    pub dist: Dist,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Dist {
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
}

pub async fn install() -> Result<()> {
    let client = Client::new();

    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    dbg!(&package_json);

    if let Some(dev_dependencies) = package_json.dev_dependencies {
        for (dep, req) in dev_dependencies {
            let req = VersionReq::parse(&req).context("Invalid semver requirement")?;

            debug!("Fetching metadata");

            let metadata: Metadata = client
                .get(format!("https://registry.npmjs.org/{dep}"))
                .header(
                    "Accept",
                    "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*",
                )
                .send()
                .await?
                .json()
                .await?;

            debug!("Searching for version {req}");

            // FIXME: keep the original version instead of doing an encoding roundtrip
            let version = metadata
                .versions
                .keys()
                .filter_map(|v| SemverVersion::parse(v.as_str()).ok())
                .filter(|v| req.matches(v))
                .max()
                .expect("Failed to find a suitable version");

            let version = metadata
                .versions
                .get(&version.to_string())
                .expect("internal error");

            dbg!(version);
        }
    }

    Ok(())
}
