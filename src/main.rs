#![deny(unsafe_code)]

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

    /// Pick a random preset
    #[arg(short = 'r', long)]
    random: bool,
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

    // Subcommands: show / log / undo
    match cli.pattern.as_deref() {
        Some("show") => return show_vanity(),
        Some("log") => return vanity_log(),
        Some("undo") => return undo_vanity(),
        _ => {}
    }

    // Resolve pattern: --random > --preset > positional arg
    let pattern_str = if cli.random {
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as usize)
            % preset::PRESETS.len();
        let p = &preset::PRESETS[idx];
        eprintln!("Random preset: {} ({})", p.name, p.hex);
        p.hex.to_string()
    } else {
        cli.preset
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
                AppError::Args(
                    "No pattern specified. Use a hex pattern, --preset (-p), or --random (-r)."
                        .into(),
                )
            })?
    };

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

    // Ctrl+C handler: print stats before exiting
    let ctrlc_counter = Arc::clone(&progress_counter);
    let ctrlc_start = start;
    ctrlc::set_handler(move || {
        let attempts = ctrlc_counter.load(Ordering::Relaxed);
        let elapsed = ctrlc_start.elapsed().as_secs_f64();
        eprint!("\r\x1b[K"); // clear progress bar
        eprintln!(
            "Interrupted: {} attempts in {:.2}s ({:.0}M hash/sec)",
            format_number(attempts),
            elapsed,
            if elapsed > 0.0 {
                attempts as f64 / elapsed / 1_000_000.0
            } else {
                0.0
            }
        );
        std::process::exit(2);
    })
    .ok();

    let config = worker::WorkerConfig {
        threads: cli.threads,
        max_attempts: cli.max_attempts,
        timeout_ms: cli.timeout,
        position,
    };

    // Progress bar only for patterns expected to take > 0.5s
    let show_progress =
        !cli.quiet && !cli.debug && std::io::stderr().is_terminal() && est_secs >= 0.5;
    let spinner_counter = Arc::clone(&progress_counter);
    let est_attempts = est;
    let spinner_handle = show_progress.then(|| {
        std::thread::spawn(move || {
            let frames = [
                '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
                '\u{2827}', '\u{2807}', '\u{280F}',
            ];

            // from_fn produces ticks lazily until search signals done
            std::iter::from_fn(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let attempts = spinner_counter.load(Ordering::Relaxed);
                (attempts != u64::MAX).then_some((attempts, start.elapsed().as_secs_f64()))
            })
            .enumerate()
            // Only render when we have meaningful data (> 300ms, attempts > 0)
            .filter(|(_, (attempts, elapsed))| *attempts > 0 && *elapsed > 0.3)
            // Format and display each tick
            .for_each(|(i, (attempts, elapsed))| {
                let line =
                    format_progress(frames[i % frames.len()], attempts, elapsed, est_attempts);
                eprint!("\r{}", line);
            });

            eprint!("\r\x1b[K"); // clear line
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

/// Strip x-nonce from HEAD and restore the original hash.
fn undo_vanity() -> Result<(), AppError> {
    git::ensure_repo().map_err(AppError::Git)?;

    let old_hash = git::get_head_hash().map_err(AppError::Git)?;
    let raw = git::read_head_commit().map_err(AppError::Git)?;

    if !raw.lines().any(|l| l.starts_with("x-nonce ")) {
        println!("No vanity nonce found in HEAD — nothing to undo.");
        return Ok(());
    }

    let commit = commit::CommitObject::parse(&raw).map_err(AppError::Git)?;

    // Rebuild content without nonce (CommitObject already strips x-nonce)
    let content: Vec<u8> = commit
        .header_lines
        .iter()
        .flat_map(|line| line.as_bytes().iter().chain(std::iter::once(&b'\n')))
        .chain(b"\n".iter())
        .chain(commit.message.as_bytes().iter())
        .copied()
        .collect();

    let new_hash = git::write_commit_object(&content)
        .and_then(|hash| git::update_head(&hash).map(|()| hash))
        .map_err(AppError::Git)?;

    let color = supports_color();
    println!(
        "\u{2713} {} \u{2192} {} (nonce removed)",
        if color {
            bold_green(&old_hash[..12], true)
        } else {
            old_hash[..12].to_string()
        },
        &new_hash[..12]
    );

    Ok(())
}

/// Detect vanity pattern in a hash. Returns (matched_portion, position).
/// Checks: leading repeat (4+), preset at start, preset at end, preset contains.
fn detect_vanity(hash: &str) -> Option<(String, MatchPosition)> {
    // Check leading repeat (4+ identical chars)
    let leading = hash
        .chars()
        .take_while(|&c| c == hash.chars().next().unwrap_or(' '))
        .count();
    if leading >= 4 {
        return Some((hash[..leading].to_string(), MatchPosition::Start));
    }

    // Check preset match — prioritize start > end > contains,
    // and longer patterns first within each position
    let presets: Vec<_> = preset::PRESETS
        .iter()
        .filter(|p| p.hex.len() >= 4)
        .collect();

    // Start matches (longest first)
    presets
        .iter()
        .filter(|p| hash.starts_with(p.hex))
        .max_by_key(|p| p.hex.len())
        .map(|p| (p.hex.to_string(), MatchPosition::Start))
        // End matches
        .or_else(|| {
            presets
                .iter()
                .filter(|p| hash.ends_with(p.hex))
                .max_by_key(|p| p.hex.len())
                .map(|p| (p.hex.to_string(), MatchPosition::End))
        })
        // Contains matches
        .or_else(|| {
            presets
                .iter()
                .filter(|p| hash.contains(p.hex))
                .max_by_key(|p| p.hex.len())
                .map(|p| (p.hex.to_string(), MatchPosition::Contains))
        })
}

/// Format a short hash (7 chars) with the vanity pattern highlighted.
fn format_log_hash(hash: &str, vanity: &Option<(String, MatchPosition)>, color: bool) -> String {
    let short = &hash[..7];
    vanity
        .as_ref()
        .and_then(|(pattern, position)| {
            match position {
                MatchPosition::Start => {
                    let end = pattern.len().min(7);
                    Some(format!(
                        "{}{}",
                        bold_green(&short[..end], color),
                        &short[end..]
                    ))
                }
                MatchPosition::End => {
                    // Show last 7 chars with pattern highlighted
                    let tail = &hash[hash.len() - 7..];
                    let pat_start = tail.find(pattern.as_str())?;
                    Some(format!(
                        "..{}{}{}",
                        &tail[..pat_start],
                        bold_green(&tail[pat_start..pat_start + pattern.len()], color),
                        &tail[pat_start + pattern.len()..]
                    ))
                }
                MatchPosition::Contains => {
                    // Find pattern in hash, show context around it
                    let pos = hash.find(pattern.as_str())?;
                    let start = pos.saturating_sub(2);
                    let end = (pos + pattern.len() + 2).min(hash.len());
                    let ctx = &hash[start..end];
                    let rel = pos - start;
                    Some(format!(
                        "..{}{}{}",
                        &ctx[..rel],
                        bold_green(&ctx[rel..rel + pattern.len()], color),
                        &ctx[rel + pattern.len()..]
                    ))
                }
            }
        })
        .unwrap_or_else(|| short.to_string())
}

/// Show vanity stats for recent commits.
/// Validates both: x-nonce present AND hash shows a real pattern.
fn vanity_log() -> Result<(), AppError> {
    git::ensure_repo().map_err(AppError::Git)?;

    let entries = git::log_with_nonce_info(50).map_err(AppError::Git)?;
    let color = supports_color();

    // Detect vanity for each commit: (hash, has_nonce, subject) → + vanity info
    let annotated: Vec<_> = entries
        .iter()
        .map(|(hash, has_nonce, subject)| {
            let vanity = if *has_nonce {
                detect_vanity(hash)
            } else {
                None
            };
            (hash, has_nonce, subject, vanity)
        })
        .collect();

    let vanity_count = annotated.iter().filter(|(_, _, _, v)| v.is_some()).count();

    annotated
        .iter()
        .for_each(|(hash, _has_nonce, subject, vanity)| {
            let marker = if vanity.is_some() { "\u{2713}" } else { " " };
            let display_hash = format_log_hash(hash, vanity, color);
            println!("{} {} {}", marker, display_hash, subject);
        });

    println!(
        "\n{}/{} commits have vanity hashes",
        vanity_count,
        annotated.len()
    );

    Ok(())
}

/// Pure function: format progress bar from current state.
/// No side effects — just data in, string out.
fn format_progress(frame: char, attempts: u64, elapsed: f64, est_attempts: u64) -> String {
    let speed = attempts as f64 / elapsed;
    let pct = (attempts as f64 / est_attempts as f64 * 100.0).min(100.0);
    let remaining = (est_attempts as f64 - attempts as f64).max(0.0) / speed;
    let bar_width = 20;
    let filled = ((pct / 100.0) * bar_width as f64).min(bar_width as f64) as usize;
    let bar: String = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(bar_width - filled);
    format!(
        "{} {} {:>5.1}% | {:.0}M/s | ~{}  ",
        frame,
        bar,
        pct,
        speed / 1_000_000.0,
        format_duration_short(remaining)
    )
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
        s if s < 3600.0 => format!("{}m {}s", s as u64 / 60, s as u64 % 60),
        s => format!("{}h {}m", s as u64 / 3600, (s as u64 % 3600) / 60),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_progress_zero_percent() {
        let s = format_progress('⠋', 0, 1.0, 1_000_000);
        assert!(s.contains("0.0%"));
        assert!(s.contains("0M/s"));
    }

    #[test]
    fn test_format_progress_50_percent() {
        let s = format_progress('⠙', 50_000_000, 0.5, 100_000_000);
        assert!(s.contains("50.0%"));
        assert!(s.contains("100M/s"));
    }

    #[test]
    fn test_format_progress_caps_at_100() {
        // Attempts exceed estimate — should cap at 100%
        let s = format_progress('⠹', 200_000_000, 2.0, 100_000_000);
        assert!(s.contains("100.0%"));
        assert!(!s.contains("200"));
    }

    #[test]
    fn test_format_progress_shows_eta() {
        let s = format_progress('⠸', 50_000_000, 0.5, 1_000_000_000);
        // 950M remaining / 100M/s = ~10s
        assert!(s.contains("~"));
    }

    #[test]
    fn test_format_duration_short_subsecond() {
        assert_eq!(format_duration_short(0.5), "0s");
    }

    #[test]
    fn test_format_duration_short_seconds() {
        assert_eq!(format_duration_short(3.2), "4s");
    }

    #[test]
    fn test_format_duration_short_minutes() {
        assert_eq!(format_duration_short(90.0), "2m");
    }

    #[test]
    fn test_format_duration_short_hours() {
        assert_eq!(format_duration_short(7200.0), "2h");
    }

    #[test]
    fn test_format_duration_full() {
        assert_eq!(format_duration(0.5), "0.5s");
        assert_eq!(format_duration(5.0), "5s");
        assert_eq!(format_duration(90.0), "1m 30s");
        assert_eq!(format_duration(3700.0), "1h 1m");
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(42), "42");
        assert_eq!(format_number(999), "999");
    }

    #[test]
    fn test_format_number_thousands() {
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(1_234_567), "1,234,567");
    }

    #[test]
    fn test_format_hash_start() {
        let h = "cafebabe12345678901234567890abcdef123456";
        let result = format_hash(h, "cafe", MatchPosition::Start);
        assert!(result.contains("cafe"));
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_format_hash_end() {
        let h = "1234567890abcdef1234567890abcdef1234cafe";
        let result = format_hash(h, "cafe", MatchPosition::End);
        assert!(result.contains("cafe"));
        assert!(result.starts_with("..."));
    }

    #[test]
    fn test_format_hash_contains() {
        let h = "1234567890cafe567890abcdef1234567890abcd";
        let result = format_hash(h, "cafe", MatchPosition::Contains);
        assert!(result.contains("cafe"));
    }

    #[test]
    fn test_detect_vanity_leading_zeros() {
        let v = detect_vanity("00001234567890abcdef1234567890abcdef1234");
        assert!(v.is_some());
        assert_eq!(v.unwrap(), ("0000".to_string(), MatchPosition::Start));

        let v = detect_vanity("aaaa1234567890abcdef1234567890abcdef1234");
        assert!(v.is_some());
        assert_eq!(v.unwrap(), ("aaaa".to_string(), MatchPosition::Start));

        assert!(detect_vanity("0123456789abcdef0123456789abcdef01234567").is_none());
    }

    #[test]
    fn test_detect_vanity_preset_start() {
        let v = detect_vanity("cafebabe12345678901234567890abcdef123456");
        assert!(v.is_some());
        assert_eq!(v.unwrap().1, MatchPosition::Start);
    }

    #[test]
    fn test_detect_vanity_preset_end() {
        let v = detect_vanity("1234567890abcdef1234567890abcdef1234cafe");
        assert!(v.is_some());
        assert_eq!(v.unwrap().1, MatchPosition::End);
    }

    #[test]
    fn test_detect_vanity_preset_contains() {
        let v = detect_vanity("1234567890abcafe1234567890abcdef12345678");
        assert!(v.is_some());
        assert_eq!(v.unwrap().1, MatchPosition::Contains);
    }

    #[test]
    fn test_format_log_hash_start() {
        let vanity = Some(("0000".to_string(), MatchPosition::Start));
        let result = format_log_hash("00001234567890abcdef1234567890abcdef1234", &vanity, false);
        assert_eq!(result, "0000123");
    }

    #[test]
    fn test_format_log_hash_end() {
        let vanity = Some(("cafe".to_string(), MatchPosition::End));
        let result = format_log_hash("1234567890abcdef1234567890abcdef1234cafe", &vanity, false);
        assert!(result.contains("cafe"));
        assert!(result.starts_with(".."));
    }

    #[test]
    fn test_format_log_hash_no_vanity() {
        let result = format_log_hash("0123456789abcdef0123456789abcdef01234567", &None, false);
        assert_eq!(result, "0123456");
    }

    #[test]
    fn test_supports_color_respects_no_color() {
        // NO_COLOR env var should disable color
        std::env::set_var("NO_COLOR", "1");
        assert!(!supports_color());
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn test_bold_green_no_color() {
        assert_eq!(bold_green("test", false), "test");
    }

    #[test]
    fn test_bold_green_with_color() {
        let result = bold_green("test", true);
        assert!(result.contains("\x1b[1;32m"));
        assert!(result.contains("\x1b[0m"));
        assert!(result.contains("test"));
    }
}
