use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "bella")]
#[command(about = "Command-line client for Bella.")]
struct Cli {
    #[arg(
        long,
        env = "BELLA_API_BASE_URL",
        default_value = "http://127.0.0.1:3000"
    )]
    api_base_url: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the configured API base URL.
    Config,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Config => println!("api_base_url={}", cli.api_base_url),
    }

    Ok(())
}
