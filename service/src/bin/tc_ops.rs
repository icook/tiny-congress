#![deny(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]
// tc-ops uses eprintln! for user-facing output (not tracing), so allow print_stderr.
#![allow(clippy::print_stderr)]

use clap::{Parser, Subcommand};

/// `TinyCongress` operations CLI for prompt iteration and research R&D.
#[derive(Parser)]
#[command(name = "tc-ops", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the research pipeline for a company (no DB required).
    Research(ResearchArgs),
}

#[derive(clap::Args)]
struct ResearchArgs {
    /// Company name to research.
    company: String,

    /// Stock ticker (optional, used in prompts).
    #[arg(long, default_value = "")]
    ticker: String,

    /// LLM model to use (overrides `BOT_DEFAULT_MODEL` env var).
    #[arg(long)]
    model: Option<String>,

    /// Number of Exa results per dimension (low=3, medium=5, high=10).
    #[arg(long, default_value = "high")]
    quality: String,

    /// Path to a file containing a custom synthesis system prompt.
    #[arg(long)]
    prompt_file: Option<std::path::PathBuf>,

    /// Only run Exa searches, skip LLM synthesis. Prints search results as JSON.
    #[arg(long)]
    search_only: bool,

    /// Skip Exa searches, load search results from a JSON file or stdin.
    /// Use with --search-results to provide cached data.
    #[arg(long)]
    no_search: bool,

    /// Path to a JSON file of cached search results (used with --no-search).
    #[arg(long)]
    search_results: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Research(args) => research(args).await,
    }
}

#[allow(clippy::unused_async)]
async fn research(_args: ResearchArgs) -> Result<(), anyhow::Error> {
    eprintln!("tc-ops research: not yet implemented");
    Ok(())
}
