use std::{
    collections::HashMap,
    env::current_dir,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use indicatif::ProgressBar;
use reqwest::Client;
use rustls::crypto::aws_lc_rs;
use rustls_platform_verifier::BuilderVerifierExt;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tar::Archive;
use tokio_stream::StreamExt;
use tracing::debug;

use crate::package_json::{Bin, PackageJson};

#[derive(Debug, Deserialize, Serialize)]
pub struct Metadata {
    pub name: String,
    pub modified: String,
    #[serde(rename = "dist-tags")]
    pub dist_tags: HashMap<String, String>,
    pub versions: HashMap<String, Version>,
}

impl Metadata {
    pub fn _find_best_version(&self, requirement: &str) -> Option<String> {
        let req = deno_semver::npm::parse_npm_version_req(requirement).ok()?;

        self.versions
            .keys()
            .filter_map(|v| {
                deno_semver::npm::parse_npm_version(v)
                    .ok()
                    .filter(|version| req.matches(version))
                    .map(|_| v.clone())
            })
            .max()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Version {
    pub name: String,
    pub version: String,
    pub dist: Dist,
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "devDependencies")]
    pub dev_dependencies: Option<HashMap<String, String>>,
    #[serde(rename = "optionalDependencies")]
    pub optional_dependencies: Option<HashMap<String, String>>,
    pub bin: Option<Bin>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Dist {
    pub tarball: String,
    pub shasum: String,
    pub integrity: Option<String>,
}

pub async fn install() -> Result<()> {
    println!("📦 Installing dependencies...");

    let mut client = Client::builder()
        .use_preconfigured_tls(
            rustls::ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_platform_verifier()
                .with_no_client_auth(),
        )
        .build()?;

    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    if let Some(dependencies) = package_json.dependencies {
        for (package, req) in dependencies {
            install_package(&mut client, &package, &req).await?;
        }
    }

    if let Some(dependencies) = package_json.dev_dependencies {
        for (package, req) in dependencies {
            install_package(&mut client, &package, &req).await?;
        }
    }

    if let Some(dependencies) = package_json.optional_dependencies {
        for (package, req) in dependencies {
            install_package(&mut client, &package, &req).await?;
        }
    }

    println!("✨ Done!");

    Ok(())
}

async fn install_package<'f>(client: &'f mut Client, package: &'f str, req: &'f str) -> Result<()> {
    let req =
        deno_semver::npm::parse_npm_version_req(&req).context("Invalid semver requirement")?;

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
        .filter_map(|v| deno_semver::npm::parse_npm_version(v.as_str()).ok())
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

    let output = current_dir()?.join(format!("node_modules/{}", version.name));

    unpack(&mut archive, &output)?;

    let package_json = fs::read(output.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    if let Some(dependencies) = package_json.dependencies {
        for (package, req) in dependencies {
            Box::pin(install_package(client, &package, &req)).await?;
        }
    }

    if let Some(dependencies) = package_json.optional_dependencies {
        for (package, req) in dependencies {
            Box::pin(install_package(client, &package, &req)).await?;
        }
    }

    if let Some(bin) = package_json.bin {
        let bin_folder = current_dir()?.join(format!("node_modules/.bin"));

        fs::create_dir_all(&bin_folder)?;

        match bin {
            Bin::Single(path) => {
                let link = bin_folder.join(package_json.name);

                if link.exists() {
                    fs::remove_file(&link)?;
                }

                symlink(output.join(path), link)?
            }
            Bin::Multiple(bins) => {
                for (bin, path) in bins {
                    let link = bin_folder.join(bin);

                    if link.exists() {
                        fs::remove_file(&link)?;
                    }

                    symlink(output.join(path), link)?
                }
            }
        }
    }

    Ok(())
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
