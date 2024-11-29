use std::{env::current_dir, fs, process::Command};

use anyhow::{bail, Context, Result};
use owo_colors::OwoColorize;
use textwrap::wrap;
use tracing::debug;

use crate::package_json::PackageJson;

pub fn run(script: Option<String>) -> Result<()> {
    debug!("Reading package.json");
    let package_json =
        fs::read(std::env::current_dir()?.join("package.json")).context("Missing package.json")?;
    let package_json: PackageJson =
        serde_json::from_slice(&package_json).context("Invalid package.json")?;

    if let Some(script) = script {
        if let Some(script) = package_json.scripts.get(&script) {
            let current_path = std::env::var("PATH").unwrap_or_default();
            let bin_dir = current_dir()?.join("node_modules/.bin");

            let new_path = format!("{}:{}", current_path, bin_dir.display());

            Command::new("/bin/sh")
                .env("PATH", new_path)
                .arg("-c")
                .arg(script)
                .spawn()?
                .wait()?;

            return Ok(());
        }

        bail!("Script not found")
    }

    let scripts = package_json.scripts;

    if scripts.is_empty() {
        println!("{}", "No script available".italic());
        return Ok(());
    }

    println!("{}:", "Available Scripts".bold());

    let max_length = scripts.keys().map(|k| k.len()).max().unwrap_or(0);

    for script_name in scripts.keys() {
        let script_content = &scripts[script_name];

        // Format the script command line
        println!(
            "  {script_name}{}",
            " ".repeat(max_length.saturating_sub(script_name.len())),
        );

        let wrapped_content = wrap(script_content, 80); // Wrap at 80 characters
        for line in wrapped_content {
            println!("    {}", line.bright_black());
        }
    }

    Ok(())
}
