mod commit;
mod git;
mod hasher;
mod nonce;
mod pattern;
mod preset;
mod worker;

use clap::Parser;
use pattern::MatchPosition;
use std::process;
use std::time::Instant;

/// Generate Git commit hashes matching custom patterns (vanity hashes).
#[derive(Parser, Debug)]
#[command(name = "git-vanity", version, about)]
struct Cli {
    /// Pattern to match (hex prefix, repeat:N, xx, or /regex/)
    pattern: Option<String>,

    /// Use a preset hex word (e.g. cafe, dead, c0ffee). Use --list-presets to see all.
    #[arg(short = 'p', long)]
    preset: Option<String>,

    /// List all available preset hex words
    #[arg(long)]
    list_presets: bool,

    /// Where to match: start (default), contains, end
    #[arg(short = 'm', long, default_value = "start", value_name = "POSITION")]
    r#match: String,

    /// Override commit message
    #[arg(long)]
    message: Option<String>,

    /// Timeout in milliseconds
    #[arg(short = 't', long, default_value = "30000")]
    timeout: u64,

    /// Maximum hash attempts
    #[arg(long)]
    max_attempts: Option<u64>,

    /// Print matching hash without writing
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Show throughput metrics
    #[arg(short = 'd', long)]
    debug: bool,

    /// Disable auto-repeat pattern detection
    #[arg(long)]
    no_repeat: bool,

    /// Number of worker threads
    #[arg(short = 'j', long, default_value_t = num_cpus::get())]
    threads: usize,
}

/// Application error with exit code mapping (railway-oriented error handling).
enum AppError {
    Args(String),
    Timeout(String),
    Git(String),
}

impl AppError {
    fn exit_code(&self) -> i32 {
        match self {
            AppError::Args(_) => 1,
            AppError::Timeout(_) => 2,
            AppError::Git(_) => 3,
        }
    }

    fn message(&self) -> &str {
        match self {
            AppError::Args(m) | AppError::Timeout(m) | AppError::Git(m) => m,
        }
    }
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e.message());
            process::exit(e.exit_code());
        }
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();

    // List presets and exit
    if cli.list_presets {
        println!("Available presets:\n");
        println!("{}", preset::list());
        println!("\nUsage: git vanity -p cafe");
        return Ok(());
    }

    // Resolve pattern: --preset takes priority, then positional arg
    let pattern_str = cli
        .preset
        .as_deref()
        .map(|name| {
            preset::find(name)
                .map(|p| p.hex.to_string())
                .ok_or_else(|| {
                    AppError::Args(format!(
                        "Unknown preset '{}'. Use --list-presets to see available options.",
                        name
                    ))
                })
        })
        .transpose()?
        .or(cli.pattern.clone())
        .ok_or_else(|| {
            AppError::Args("No pattern specified. Use a hex pattern or --preset (-p).".into())
        })?;

    // Validate environment
    git::ensure_repo().map_err(AppError::Git)?;

    // Parse pattern and match position
    let pat = pattern::Pattern::parse(&pattern_str, cli.no_repeat).map_err(AppError::Args)?;
    let position = MatchPosition::parse(&cli.r#match).map_err(AppError::Args)?;

    // Read → parse → optionally override message (pipeline)
    let commit = git::read_head_commit()
        .and_then(|raw| commit::CommitObject::parse(&raw))
        .map(|mut c| {
            if let Some(ref msg) = cli.message {
                c.message = if msg.ends_with('\n') {
                    msg.clone()
                } else {
                    format!("{}\n", msg)
                };
            }
            if c.had_signature {
                eprintln!("Warning: GPG signature stripped (nonce invalidates it)");
            }
            c
        })
        .map_err(AppError::Git)?;

    let old_hash = git::get_head_hash().unwrap_or_default();

    if cli.debug {
        eprintln!("[vanity] threads: {}", cli.threads);
        eprintln!("[vanity] pattern: {} ({})", pat, position);
        eprintln!("[vanity] estimated attempts: {}", pat.estimated_attempts(position));
    }

    // Search
    let start = Instant::now();
    let config = worker::WorkerConfig {
        threads: cli.threads,
        max_attempts: cli.max_attempts,
        timeout_ms: cli.timeout,
        position,
    };

    let result = worker::search(&commit, &pat, &config).map_err(AppError::Timeout)?;
    let elapsed = start.elapsed();

    if cli.debug {
        let speed = result.total_attempts as f64 / elapsed.as_secs_f64();
        eprintln!(
            "[vanity] match: {} | attempts: {} | speed: {:.1}M hash/sec",
            result.hash_hex,
            format_number(result.total_attempts),
            speed / 1_000_000.0
        );
    }

    let hash_preview = format_hash(&result.hash_hex, &pattern_str, position);
    let stats = format!(
        "{} attempts, {:.2}s",
        format_number(result.total_attempts),
        elapsed.as_secs_f64()
    );

    // Dry-run: show result without writing
    if cli.dry_run {
        println!("\u{2713} Found matching hash: {} ({})", hash_preview, stats);

        // If interactive TTY, offer to apply
        if atty::is(atty::Stream::Stdin) {
            eprint!("Apply? [Y/n] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let answer = input.trim().to_ascii_lowercase();
            if answer.is_empty() || answer == "y" || answer == "yes" {
                let new_hash = git::write_commit_object(&result.content)
                    .and_then(|hash| git::update_head(&hash).map(|()| hash))
                    .map_err(AppError::Git)?;
                let new_preview = format_hash(&new_hash, &pattern_str, position);
                println!("\u{2713} {} \u{2192} {} (applied)", &old_hash[..12], new_preview);
            }
        }
        return Ok(());
    }

    // Write commit and update HEAD (pipeline)
    let new_hash = git::write_commit_object(&result.content)
        .and_then(|hash| git::update_head(&hash).map(|()| hash))
        .map_err(AppError::Git)?;

    let new_preview = format_hash(&new_hash, &pattern_str, position);
    println!(
        "\u{2713} {} \u{2192} {} ({})",
        &old_hash[..12],
        new_preview,
        stats
    );

    Ok(())
}

/// Format hash to highlight the pattern position.
/// - start:    `cafeb0ba1234...`
/// - end:      `...9fde699cafe`
/// - contains: `...17cafe995...`
fn format_hash(hash: &str, pattern: &str, position: MatchPosition) -> String {
    let pat_lower = pattern.to_ascii_lowercase();
    match position {
        MatchPosition::Start => {
            let end = std::cmp::min(12.max(pat_lower.len()), hash.len());
            format!("{}...", &hash[..end])
        }
        MatchPosition::End => {
            let start = hash.len().saturating_sub(12.max(pat_lower.len()));
            format!("...{}", &hash[start..])
        }
        MatchPosition::Contains => {
            if let Some(pos) = hash.find(&pat_lower) {
                let ctx = 3; // chars of context around the pattern
                let start = pos.saturating_sub(ctx);
                let end = std::cmp::min(pos + pat_lower.len() + ctx, hash.len());
                let prefix = if start > 0 { "..." } else { "" };
                let suffix = if end < hash.len() { "..." } else { "" };
                format!("{}{}{}", prefix, &hash[start..end], suffix)
            } else {
                hash.to_string()
            }
        }
    }
}

fn format_number(n: u64) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join(",")
}
