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

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::time::Instant;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tc_llm::{build_synthesis_messages, CompanyEvidence, SearchResponse, DIMENSIONS};

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

struct ResearchConfig {
    llm_api_key: String,
    llm_base_url: String,
    exa_api_key: String,
    exa_base_url: String,
    model: String,
    num_results: usize,
    system_prompt: Option<String>,
}

impl ResearchConfig {
    fn from_env_and_args(args: &ResearchArgs) -> Result<Self, anyhow::Error> {
        let llm_api_key =
            std::env::var("BOT_LLM_API_KEY").context("BOT_LLM_API_KEY env var is required")?;
        let llm_base_url =
            std::env::var("BOT_LLM_BASE_URL").context("BOT_LLM_BASE_URL env var is required")?;
        let exa_api_key =
            std::env::var("BOT_EXA_API_KEY").context("BOT_EXA_API_KEY env var is required")?;
        let exa_base_url =
            std::env::var("BOT_EXA_BASE_URL").context("BOT_EXA_BASE_URL env var is required")?;

        let model = args
            .model
            .clone()
            .or_else(|| std::env::var("BOT_DEFAULT_MODEL").ok())
            .unwrap_or_else(|| "anthropic/claude-sonnet-4-20250514".to_string());

        let num_results = match args.quality.as_str() {
            "low" => 3,
            "medium" => 5,
            _ => 10,
        };

        let system_prompt = if let Some(path) = &args.prompt_file {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("reading prompt file: {}", path.display()))?;
            Some(content)
        } else {
            None
        };

        Ok(Self {
            llm_api_key,
            llm_base_url,
            exa_api_key,
            exa_base_url,
            model,
            num_results,
            system_prompt,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Research(args) => research(args).await,
    }
}

#[allow(clippy::too_many_lines)]
async fn research(args: ResearchArgs) -> Result<(), anyhow::Error> {
    let config = ResearchConfig::from_env_and_args(&args)?;

    eprintln!(
        "tc-ops research: company={} model={} quality={}",
        args.company, config.model, args.quality
    );
    if !args.ticker.is_empty() {
        eprintln!("  ticker={}", args.ticker);
    }

    // Step 1: Get search results (from cache/file or by running Exa searches)
    let dim_results: HashMap<String, SearchResponse> = if args.no_search {
        // Load from file or stdin
        let raw = if let Some(path) = &args.search_results {
            std::fs::read_to_string(path)
                .with_context(|| format!("reading search results file: {}", path.display()))?
        } else {
            eprintln!("  reading search results from stdin...");
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading search results from stdin")?;
            buf
        };
        serde_json::from_str(&raw).context("parsing search results JSON")?
    } else {
        // Run parallel Exa searches
        run_exa_searches(&args.company, &config).await?
    };

    // Step 2: If search-only, dump results as JSON to stdout and exit
    if args.search_only {
        let json = serde_json::to_string_pretty(&dim_results)
            .context("serializing search results to JSON")?;
        eprintln!();
        eprintln!("Search results (JSON). To reuse without re-searching:");
        eprintln!(
            "  tc-ops research \"{}\" --no-search --search-results <file>",
            args.company
        );
        std::io::Write::write_all(&mut std::io::stdout(), json.as_bytes())
            .context("writing search results to stdout")?;
        std::io::Write::write_all(&mut std::io::stdout(), b"\n")
            .context("writing newline to stdout")?;
        return Ok(());
    }

    // Step 3: Build search context string
    let search_context = build_search_context(&dim_results);

    if search_context.is_empty() {
        anyhow::bail!("no search results available — cannot synthesize evidence");
    }

    // Step 4: Run LLM synthesis
    eprintln!();
    eprintln!("Running LLM synthesis with model: {}", config.model);

    let http = reqwest::Client::new();
    let system_prompt_ref = config.system_prompt.as_deref();
    let messages = build_synthesis_messages(
        &args.company,
        &args.ticker,
        &search_context,
        system_prompt_ref,
    );

    let synthesis_start = Instant::now();
    let completion = tc_llm::chat_completion(
        &http,
        &config.llm_api_key,
        &config.llm_base_url,
        &config.model,
        messages,
        true,
        Some(0.3),
    )
    .await
    .context("LLM synthesis call failed")?;
    let synthesis_elapsed = synthesis_start.elapsed();

    // Step 5: Print synthesis stats to stderr
    let cache_layers: Vec<&str> = {
        let mut layers = Vec::new();
        if completion.cache.litellm_hit {
            layers.push("litellm");
        }
        if completion.cache.nginx_hit {
            layers.push("nginx");
        }
        if completion.cache.openrouter_cached_tokens.is_some() {
            layers.push("openrouter");
        }
        layers
    };
    let cache_display = if cache_layers.is_empty() {
        "none".to_string()
    } else {
        cache_layers.join(", ")
    };

    let cost_display = completion
        .usage
        .cost
        .map_or_else(|| "unknown".to_string(), |c| format!("${c:.6}"));

    eprintln!(
        "  synthesis: model={} tokens={}/{} cost={} latency={:.1}s cache={}",
        completion.model,
        completion.usage.prompt_tokens,
        completion.usage.completion_tokens,
        cost_display,
        synthesis_elapsed.as_secs_f64(),
        cache_display,
    );

    // Step 6: Parse evidence JSON
    let json_str = tc_llm::extract_json(&completion.content);
    let evidence: CompanyEvidence = serde_json::from_str(json_str).with_context(|| {
        format!(
            "failed to parse synthesis JSON\nraw: {}",
            completion.content
        )
    })?;

    // Step 7: Write pretty-printed evidence JSON to stdout
    let evidence_json =
        serde_json::to_string_pretty(&evidence).context("serializing evidence to JSON")?;
    std::io::Write::write_all(&mut std::io::stdout(), evidence_json.as_bytes())
        .context("writing evidence JSON to stdout")?;
    std::io::Write::write_all(&mut std::io::stdout(), b"\n")
        .context("writing newline to stdout")?;

    // Step 8: Print dimension summary to stderr
    eprintln!();
    eprintln!("Evidence summary:");
    for (dim_name, _, _) in DIMENSIONS {
        let (pro_count, con_count) = evidence
            .dimensions
            .get(*dim_name)
            .map_or((0, 0), |d| (d.pro.len(), d.con.len()));
        eprintln!("  {dim_name}: {pro_count} pro, {con_count} con");
    }

    Ok(())
}

/// Run parallel Exa searches for all 5 dimensions and return a map of
/// dimension name → `SearchResponse`.
async fn run_exa_searches(
    company_name: &str,
    config: &ResearchConfig,
) -> anyhow::Result<HashMap<String, SearchResponse>> {
    let http = reqwest::Client::new();
    let mut join_set = tokio::task::JoinSet::new();

    for (dim_name, _, _) in DIMENSIONS {
        let http = http.clone();
        let api_key = config.exa_api_key.clone();
        let base_url = config.exa_base_url.clone();
        let company = company_name.to_string();
        let dim = (*dim_name).to_string();
        let num_results = config.num_results;

        join_set.spawn(async move {
            let query = format!("{company} {dim}");
            let start = Instant::now();
            let result = tc_llm::exa_search(&http, &api_key, &base_url, &query, num_results).await;
            let elapsed = start.elapsed();
            (dim, query, result, elapsed)
        });
    }

    let mut dim_results: HashMap<String, SearchResponse> = HashMap::new();

    while let Some(join_result) = join_set.join_next().await {
        let (dim_name, query, search_result, elapsed) =
            join_result.context("Exa search task panicked")?;

        match search_result {
            Ok(response) => {
                // Build cache label
                let mut cache_labels = Vec::new();
                if response.cache.nginx_hit {
                    cache_labels.push("CACHE HIT");
                }
                let cache_tag = if cache_labels.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", cache_labels.join(", "))
                };

                eprintln!(
                    "  {} {} results in {:.1}s{} — \"{}\"",
                    dim_name,
                    response.results.len(),
                    elapsed.as_secs_f64(),
                    cache_tag,
                    query,
                );

                dim_results.insert(dim_name, response);
            }
            Err(e) => {
                eprintln!("  {dim_name} FAILED: {e}");
            }
        }
    }

    Ok(dim_results)
}

/// Build the markdown search context string from dimension results.
fn build_search_context(dim_results: &HashMap<String, SearchResponse>) -> String {
    let mut search_context = String::new();

    for (dim_name, _, _) in DIMENSIONS {
        if let Some(response) = dim_results.get(*dim_name) {
            write!(search_context, "\n## {dim_name}\n\n").ok();
            for r in &response.results {
                let title = if r.title.is_empty() {
                    "(no title)"
                } else {
                    &r.title
                };
                write!(
                    search_context,
                    "### [{title}]({url})\n{text}\n\n",
                    url = r.url,
                    text = r.text,
                )
                .ok();
            }
        }
    }

    search_context
}
