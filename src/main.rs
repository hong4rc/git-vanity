mod commit;
mod git;
mod hasher;
mod nonce;
mod pattern;
mod preset;
mod worker;

use clap::Parser;
use pattern::MatchPosition;
use std::io::IsTerminal;
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Generate Git commit hashes matching custom patterns (vanity hashes).
///
/// Examples:
///   git vanity cafe                Hash starts with cafe
///   git vanity -p coffee           Use preset hex word (c0ffee)
///   git vanity dead -m end         Hash ends with dead
///   git vanity beef -m contains    Hash contains beef anywhere
///   git vanity cafe -n             Preview match, then ask to apply
///   git vanity --list-presets      Show all preset hex words
#[derive(Parser, Debug)]
#[command(
    name = "git-vanity",
    version,
    about,
    after_help = "See --list-presets for curated hex words like cafe, dead, c0ffee."
)]
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

    /// Hide progress spinner
    #[arg(short = 'q', long)]
    quiet: bool,

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

    // Subcommands: show / log
    match cli.pattern.as_deref() {
        Some("show") => return show_vanity(),
        Some("log") => return vanity_log(),
        _ => {}
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
        eprintln!(
            "[vanity] estimated attempts: {}",
            pat.estimated_attempts(position)
        );
    }

    // Show estimated time and warn for hard patterns
    let est = pat.estimated_attempts(position);
    let est_secs = est as f64 / 100_000_000.0;
    if !cli.quiet {
        if est_secs >= 60.0 {
            eprintln!(
                "Warning: this pattern is hard (~{}). Consider a shorter pattern or -m contains.",
                format_duration(est_secs)
            );
        } else if est_secs >= 0.5 {
            eprintln!("Estimated time: ~{}", format_duration(est_secs));
        }
    }

    // Search with progress reporting
    let start = Instant::now();
    let progress_counter = Arc::new(AtomicU64::new(0));

    let config = worker::WorkerConfig {
        threads: cli.threads,
        max_attempts: cli.max_attempts,
        timeout_ms: cli.timeout,
        position,
    };

    // Progress bar with ETA on TTY unless --quiet
    let show_progress = !cli.quiet && !cli.debug && std::io::stderr().is_terminal();
    let spinner_counter = Arc::clone(&progress_counter);
    let est_attempts = est;
    let spinner_handle = show_progress.then(|| {
        std::thread::spawn(move || {
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0usize;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let attempts = spinner_counter.load(Ordering::Relaxed);
                if attempts == u64::MAX {
                    break; // signal to stop
                }
                let elapsed = start.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    attempts as f64 / elapsed
                } else {
                    0.0
                };

                // Progress bar with ETA when we have an estimate
                let progress_str = if est_attempts > 0 && speed > 0.0 {
                    let pct = (attempts as f64 / est_attempts as f64 * 100.0).min(999.0);
                    let remaining = (est_attempts as f64 - attempts as f64).max(0.0) / speed;
                    let bar_width = 20;
                    let filled = ((pct / 100.0) * bar_width as f64).min(bar_width as f64) as usize;
                    let bar: String = "\u{2588}".repeat(filled)
                        + &"\u{2591}".repeat(bar_width - filled);
                    format!(
                        "\r{} {} {:>5.1}% | {:.0}M/s | ~{}  ",
                        frames[i % frames.len()],
                        bar,
                        pct,
                        speed / 1_000_000.0,
                        format_duration_short(remaining)
                    )
                } else {
                    format!(
                        "\r{} Searching... {} | {:.0}M/s  ",
                        frames[i % frames.len()],
                        format_number(attempts),
                        speed / 1_000_000.0
                    )
                };
                eprint!("{}", progress_str);
                i += 1;
            }
            eprint!("\r\x1b[K"); // clear spinner line
        })
    });

    let result = worker::search(&commit, &pat, &config, Some(Arc::clone(&progress_counter)))
        .map_err(|e| {
            // Stop spinner before printing error
            progress_counter.store(u64::MAX, Ordering::Relaxed);
            if let Some(h) = spinner_handle.as_ref() {
                // Wait briefly for spinner to clear
                h.thread().unpark();
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            AppError::Timeout(e)
        })?;

    // Stop spinner
    progress_counter.store(u64::MAX, Ordering::Relaxed);
    if let Some(h) = spinner_handle {
        let _ = h.join();
    }

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

    // Bell on long searches (> 2s) to notify user
    if elapsed.as_secs_f64() > 2.0 && std::io::stderr().is_terminal() {
        eprint!("\x07"); // BEL character
    }

    let hash_preview = format_hash(&result.hash_hex, &pattern_str, position);
    let stats = if elapsed.as_secs_f64() < 0.1 {
        String::new() // skip stats for instant matches
    } else {
        format!(
            " ({} attempts, {:.2}s)",
            format_number(result.total_attempts),
            elapsed.as_secs_f64()
        )
    };

    // Dry-run: show result without writing
    if cli.dry_run {
        println!("\u{2713} Found matching hash: {}{}", hash_preview, stats);

        if std::io::stdin().is_terminal() {
            eprint!("Apply? [Y/n] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let answer = input.trim().to_ascii_lowercase();
            if answer.is_empty() || answer == "y" || answer == "yes" {
                let new_hash = git::write_commit_object(&result.content)
                    .and_then(|hash| git::update_head(&hash).map(|()| hash))
                    .map_err(AppError::Git)?;
                let new_preview = format_hash(&new_hash, &pattern_str, position);
                println!(
                    "\u{2713} {} \u{2192} {} (applied)",
                    &old_hash[..12],
                    new_preview
                );
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
        "\u{2713} {} \u{2192} {}{}",
        &old_hash[..12],
        new_preview,
        stats
    );

    Ok(())
}

/// Detect color support: stdout is TTY + NO_COLOR env not set.
fn supports_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

/// Wrap text in bold green ANSI codes if color is supported.
fn bold_green(text: &str, color: bool) -> String {
    if color {
        format!("\x1b[1;32m{}\x1b[0m", text)
    } else {
        text.to_string()
    }
}

/// Format hash with pattern highlighted in color.
/// Pure function: computes (before, matched, after, prefix_dots, suffix_dots)
/// from position, then assembles with color applied to matched portion.
fn format_hash(hash: &str, pattern: &str, position: MatchPosition) -> String {
    let pat = pattern.to_ascii_lowercase();
    let color = supports_color();

    // Compute the visible window: (show_start, match_start, match_end, show_end)
    let (ss, ms, me, se) = match position {
        MatchPosition::Start => {
            let show = 12.max(pat.len()).min(hash.len());
            (0, 0, pat.len().min(hash.len()), show)
        }
        MatchPosition::End => {
            let show = 12.max(pat.len()).min(hash.len());
            (
                hash.len() - show,
                hash.len() - pat.len(),
                hash.len(),
                hash.len(),
            )
        }
        MatchPosition::Contains => hash.find(&pat).map_or((0, 0, 0, hash.len()), |pos| {
            let ctx = 3;
            (
                pos.saturating_sub(ctx),
                pos,
                pos + pat.len(),
                (pos + pat.len() + ctx).min(hash.len()),
            )
        }),
    };

    // Assemble: dots + before + highlighted(matched) + after + dots
    [
        if ss > 0 { "..." } else { "" },
        &hash[ss..ms],
        &bold_green(&hash[ms..me], color),
        &hash[me..se],
        if se < hash.len() { "..." } else { "" },
    ]
    .concat()
}

/// Show vanity info for HEAD commit.
/// Checks if HEAD has an x-nonce header and displays hash details.
fn show_vanity() -> Result<(), AppError> {
    git::ensure_repo().map_err(AppError::Git)?;

    let hash = git::get_head_hash().map_err(AppError::Git)?;
    let raw = git::read_head_commit().map_err(AppError::Git)?;
    let has_nonce = raw.lines().any(|l| l.starts_with("x-nonce "));
    let color = supports_color();

    println!(
        "Commit: {}",
        if color {
            bold_green(&hash, true)
        } else {
            hash.clone()
        }
    );

    if has_nonce {
        // Find matching presets
        let matching_presets: Vec<_> = preset::PRESETS
            .iter()
            .filter(|p| hash.starts_with(p.hex) || hash.ends_with(p.hex) || hash.contains(p.hex))
            .collect();

        // Find longest prefix run of repeated chars
        let prefix_len = hash
            .chars()
            .zip(hash.chars().skip(1))
            .take_while(|(a, b)| a == b)
            .count();

        println!("Vanity: yes (x-nonce present)");

        if !matching_presets.is_empty() {
            let names: Vec<_> = matching_presets.iter().map(|p| p.name).collect();
            println!("Presets: {}", names.join(", "));
        }

        if prefix_len >= 2 {
            println!(
                "Prefix:  {} ({} identical chars)",
                &hash[..prefix_len + 1],
                prefix_len + 1
            );
        }
    } else {
        println!("Vanity: no (no x-nonce header)");
    }

    // Show commit message (first line)
    let msg = raw
        .find("\n\n")
        .map(|pos| &raw[pos + 2..])
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("");
    println!("Message: {}", msg);

    Ok(())
}

/// Check if a hash has a recognizable vanity pattern:
/// - 4+ leading identical chars (e.g. 000000, aaaabc)
/// - Matches any preset (start or end)
fn is_vanity_hash(hash: &str) -> bool {
    // Check leading repeat (4+ identical chars)
    let leading = hash
        .chars()
        .take_while(|&c| c == hash.chars().next().unwrap_or(' '))
        .count();
    if leading >= 4 {
        return true;
    }

    // Check preset match (start or end)
    preset::PRESETS
        .iter()
        .any(|p| p.hex.len() >= 4 && (hash.starts_with(p.hex) || hash.ends_with(p.hex)))
}

/// Show vanity stats for recent commits.
/// Validates both: x-nonce present AND hash shows a real pattern.
fn vanity_log() -> Result<(), AppError> {
    git::ensure_repo().map_err(AppError::Git)?;

    let entries = git::log_with_nonce_info(50).map_err(AppError::Git)?;
    let color = supports_color();

    let vanity_count = entries
        .iter()
        .filter(|(hash, has_nonce, _)| *has_nonce && is_vanity_hash(hash))
        .count();

    entries.iter().for_each(|(hash, has_nonce, subject)| {
        let short = &hash[..7];
        let valid = *has_nonce && is_vanity_hash(hash);
        let marker = if valid { "\u{2713}" } else { " " };
        let colored_hash = if valid && color {
            bold_green(short, true)
        } else {
            short.to_string()
        };
        println!("{} {} {}", marker, colored_hash, subject);
    });

    println!(
        "\n{}/{} commits have vanity hashes",
        vanity_count,
        entries.len()
    );

    Ok(())
}

fn format_duration_short(secs: f64) -> String {
    match secs {
        s if s < 1.0 => format!("{:.0}s", (s * 10.0).ceil() / 10.0_f64.max(1.0)),
        s if s < 60.0 => format!("{:.0}s", s.ceil()),
        s if s < 3600.0 => format!("{:.0}m", (s / 60.0).ceil()),
        s => format!("{:.0}h", (s / 3600.0).ceil()),
    }
}

fn format_duration(secs: f64) -> String {
    match secs {
        s if s < 1.0 => format!("{:.1}s", s),
        s if s < 60.0 => format!("{:.0}s", s),
        s if s < 3600.0 => format!("{:.0}m {:.0}s", s / 60.0, s % 60.0),
        s => format!("{:.0}h {:.0}m", s / 3600.0, (s % 3600.0) / 60.0),
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
