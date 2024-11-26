use clap::{Parser, Subcommand};

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
    let cli = Cli::parse();

    match cli.command {
        Commands::Install => {
            println!("Installing...")
        }
    }
}
