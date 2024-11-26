use std::{
    collections::HashMap,
    env::current_dir,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use futures_util::{future::BoxFuture, FutureExt};
use indicatif::ProgressBar;
use reqwest::Client;
use semver::{Version as SemverVersion, VersionReq};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tar::Archive;
use tokio_stream::StreamExt;
use tracing::debug;

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
            println!("📦 Installing dependencies...");
            install().await.expect("Failed to install");
            println!("✨ Done!");
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
    pub bin: Option<Bin>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Bin {
    Single(String),
    Multiple(HashMap<String, String>),
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
    let mut client = Client::new();

    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    if let Some(dev_dependencies) = package_json.dependencies {
        for (package, req) in dev_dependencies {
            install_package(&mut client, &package, &req).await?;
        }
    }

    if let Some(dev_dependencies) = package_json.dev_dependencies {
        for (package, req) in dev_dependencies {
            install_package(&mut client, &package, &req).await?;
        }
    }

    Ok(())
}

fn install_package<'f>(
    client: &'f mut Client,
    package: &'f str,
    req: &'f str,
) -> BoxFuture<'f, Result<()>> {
    async move {
        let req = VersionReq::parse(&req).context("Invalid semver requirement")?;

        debug!("Fetching metadata");

        let metadata: Metadata = client
            .get(format!("https://registry.npmjs.org/{package}"))
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

        debug!("Downloading {package}@{version}");

        let version = metadata
            .versions
            .get(&version.to_string())
            .expect("internal error");

        let path = PathBuf::from("temp").join(&version.name);
        fs::create_dir_all(path.parent().unwrap())?;

        download(&version.dist.tarball, &path, client).await?;

        debug!("Downloaded");

        let file = fs::read(&path)?;
        let hash = Sha1::digest(&file);
        let hex = base16ct::lower::encode_string(&hash);

        if hex != version.dist.shasum {
            bail!("Integrity failed")
        }

        // FIXME: mhhh yes reading the file a 3rd time is def not stupid
        let mut archive = Archive::new(GzDecoder::new(std::fs::File::open(path)?));
        archive.set_preserve_permissions(true);
        archive.set_unpack_xattrs(true);

        let output = current_dir()?.join(format!("bento_modules/{}", version.name));

        unpack(&mut archive, &output)?;

        let package_json = fs::read(output.join("package.json")).context("Missing package.json")?;
        let package_json: PackageJson =
            serde_json::from_slice(&package_json).context("Invalid package.json")?;

        if let Some(dev_dependencies) = package_json.dependencies {
            for (package, req) in dev_dependencies {
                install_package(client, &package, &req).await?;
            }
        }

        Ok(())
    }
    .boxed()
}

fn unpack<R: Read, P: AsRef<Path>>(archive: &mut Archive<R>, output: P) -> Result<()> {
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Convert path components to string representation
        let components: Vec<_> = path.components().collect();

        // Skip if there are no components after the root
        if components.len() <= 1 {
            continue;
        }

        // Create the new path without the root directory
        let path: PathBuf = components[1..].iter().collect();

        // Sanitize the path to prevent directory traversal
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            bail!(
                "Invalid path with parent directory references: {}",
                path.display()
            );
        }

        let output_path = output.as_ref().join(&path);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        entry.unpack(output_path)?;
    }

    Ok(())
}

async fn download(url: &str, file_path: impl AsRef<Path>, client: &Client) -> Result<()> {
    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        bail!("Failed to download: HTTP {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);

    let mut file = File::create(file_path)?;
    let mut downloaded = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;

        downloaded += chunk.len();

        pb.set_position(downloaded as u64);
    }

    file.flush()?;

    Ok(())
}
