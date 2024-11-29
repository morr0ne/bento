use clap::{Parser, Subcommand};

mod install;
mod package_json;
mod run;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Install,
    Run { script: Option<String> },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Install => {
            println!("📦 Installing dependencies...");
            install::install().await.expect("Failed to install");
            println!("✨ Done!");
        }
        Commands::Run { script } => {
            run::run(script).expect("Failed to run script");
        }
    }
}
