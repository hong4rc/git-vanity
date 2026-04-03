use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use crate::commit::CommitObject;
use crate::hasher::IncrementalHasher;
use crate::nonce::{generate_nonce, nonce_len};
use crate::pattern::{MatchPosition, Pattern};

/// Result from a successful search.
pub struct SearchResult {
    /// The raw commit content (without "commit <len>\0" prefix)
    pub content: Vec<u8>,
    /// The matching hash as hex string
    pub hash_hex: String,
    /// Total attempts across all threads
    pub total_attempts: u64,
}

/// Configuration for the worker pool.
pub struct WorkerConfig {
    pub threads: usize,
    pub max_attempts: Option<u64>,
    pub timeout_ms: u64,
    pub position: MatchPosition,
}

/// Build prefix and suffix byte vectors from a commit using iterator chains.
fn build_commit_parts(commit: &CommitObject) -> (Vec<u8>, Vec<u8>) {
    let prefix: Vec<u8> = commit
        .header_lines
        .iter()
        .flat_map(|line| line.as_bytes().iter().chain(std::iter::once(&b'\n')))
        .chain(b"x-nonce ".iter())
        .copied()
        .collect();

    let suffix: Vec<u8> = b"\n\n"
        .iter()
        .chain(commit.message.as_bytes().iter())
        .copied()
        .collect();

    (prefix, suffix)
}

/// Reconstruct full commit content from parts + nonce.
fn assemble_content(prefix: &[u8], nonce: &[u8], suffix: &[u8]) -> Vec<u8> {
    prefix
        .iter()
        .chain(nonce.iter())
        .chain(suffix.iter())
        .copied()
        .collect()
}

/// Check if the search should stop (found or max attempts reached).
#[inline]
fn should_stop(found: &AtomicBool, total: &AtomicU64, max: Option<u64>) -> bool {
    found.load(Ordering::Relaxed)
        || max.map_or(false, |m| total.load(Ordering::Relaxed) >= m)
}

/// Multi-threaded brute-force coordinator.
///
/// Uses IncrementalHasher (immutable, clone-based) for FP-clean iterator chains,
/// shared AtomicBool for fast cross-thread cancellation,
/// and AtomicU64 for lock-free attempt counting.
///
/// The hot loop uses `from_fn` to produce batch ranges lazily,
/// then `find_map` to scan each batch for a match.
pub fn search(
    commit: &CommitObject,
    pattern: &Pattern,
    config: &WorkerConfig,
    progress: Option<Arc<AtomicU64>>,
) -> Result<SearchResult, String> {
    let (prefix_bytes, suffix_bytes) = build_commit_parts(commit);
    let incremental = IncrementalHasher::new(&prefix_bytes, &suffix_bytes, nonce_len());

    // Shared state — use caller's counter if provided (for progress reporting)
    let found = Arc::new(AtomicBool::new(false));
    let total_attempts = progress.unwrap_or_else(|| Arc::new(AtomicU64::new(0)));

    const BATCH_SIZE: u64 = 65536;

    let pattern = pattern.clone();
    let max_attempts = config.max_attempts;
    let position = config.position;

    // Spawn workers: map thread IDs → JoinHandles
    let handles: Vec<_> = (0..config.threads)
        .map(|tid| {
            let found = Arc::clone(&found);
            let total_attempts = Arc::clone(&total_attempts);
            let pattern = pattern.clone();
            let incremental = incremental.clone();
            let prefix_bytes = prefix_bytes.clone();
            let suffix_bytes = suffix_bytes.clone();

            thread::spawn(move || {
                let mut batch_start: u64 = 0;

                // from_fn produces batch ranges lazily until stop signal
                std::iter::from_fn(|| {
                    if should_stop(&found, &total_attempts, max_attempts) {
                        return None;
                    }
                    let range = batch_start..batch_start + BATCH_SIZE;
                    batch_start += BATCH_SIZE;
                    Some(range)
                })
                .find_map(|mut batch| {
                    let batch_base = batch.start;
                    let result = batch.find_map(|counter| {
                        let nonce = generate_nonce(counter, tid as u16);
                        let hash = incremental.hash_with_nonce(&nonce);
                        pattern.matches_raw(&hash, position).then(|| {
                            found.store(true, Ordering::Relaxed);
                            total_attempts.fetch_add(counter - batch_base + 1, Ordering::Relaxed);
                            (assemble_content(&prefix_bytes, &nonce, &suffix_bytes), hash)
                        })
                    });

                    if result.is_none() {
                        total_attempts.fetch_add(BATCH_SIZE, Ordering::Relaxed);
                    }

                    result
                })
            })
        })
        .collect();

    // Timeout thread
    let found_for_timeout = Arc::clone(&found);
    let timeout_ms = config.timeout_ms;
    let _timeout = thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(timeout_ms));
        found_for_timeout.store(true, Ordering::Relaxed);
    });

    // Collect: find the first Some result
    let attempts = &total_attempts;
    let result = handles
        .into_iter()
        .filter_map(|h| h.join().map_err(|_| "Worker thread panicked").transpose())
        .find_map(|r| r.ok());

    let total = attempts.load(Ordering::Relaxed);

    result
        .map(|(content, hash)| SearchResult {
            content,
            hash_hex: hex::encode(hash),
            total_attempts: total,
        })
        .ok_or_else(|| {
            format!(
                "No match found after {} attempts (timeout or max-attempts reached)",
                total
            )
        })
}
