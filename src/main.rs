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

    let res = match cli.command {
        Commands::Install => install::install().await,
        Commands::Run { script } => run::run(script),
    };

    res.expect("Failed") // FIXME: handle errors better
}
