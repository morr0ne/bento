use anyhow::Result;
use clap::{Parser, Subcommand};
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

pub fn install() -> Result<()> {
    Ok(())
}
