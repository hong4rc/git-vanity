# git-vanity

[![CI](https://github.com/hong4rc/git-vanity/actions/workflows/ci.yml/badge.svg)](https://github.com/hong4rc/git-vanity/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/git-vanity.svg)](https://crates.io/crates/git-vanity)
[![bencher](https://img.shields.io/badge/bencher-perf-blueviolet)](https://bencher.dev/perf/git-vanity?branches=11c5b798-64ee-4f5a-80e0-fc65c97e8383&testbeds=fbe275fb-e5fd-4c6c-9de2-014d035fe7a0&benchmarks=77e59bc7-bc94-49c9-acab-d0ed43ca27b3&measures=5e2f4403-6a1a-4811-8a00-c434b588b850)

Make your Git commits look cool. Generate commit hashes that start with `cafe`, end with `dead`, or contain `c0ffee`.

![demo](demo/01-vanity.svg)

Nothing visible changes — same message, author, date. Just a prettier hash.

> See all 26 demos: [demo.md](demo.md)

> Every commit in this repo starts with `000000`. Run `git log --oneline` to verify.

## Install

```bash
brew install hong4rc/tap/git-vanity     # Homebrew
cargo install git-vanity                 # Cargo
npx git-vanity cafe                      # npm (no install)
```

Or download a binary from [Releases](https://github.com/hong4rc/git-vanity/releases).

## Quick Start

```bash
git vanity cafe                  # starts with cafe
git vanity cafe -m end           # ends with cafe
git vanity cafe -m contains      # cafe anywhere (fastest)
git vanity -p coffee             # preset: c0ffee
git vanity -p dead -m end        # preset + position
git vanity show                  # inspect current commit
git vanity log                   # see vanity stats for repo
```

## Patterns

| Type | Example | What it does |
|------|---------|-------------|
| Hex prefix | `git vanity cafe` | Hash starts with `cafe` |
| Repeat | `git vanity repeat:3` | 3 identical chars anywhere (`aaa`) |
| Structured | `git vanity 1997xxx` | `1997` + 3 identical chars |
| Pair | `git vanity xx` | Any adjacent pair (`aa`, `ff`) |
| Regex | `git vanity "/^dead/"` | Full regex on hex hash |

## Presets

27 curated hex words. Run `git vanity --list-presets` to see all.

```
ace  add  bad  bed  cab  dad  fab  fed     instant
babe bead beef cafe code dead deaf face    < 1s
fade feed food decaf                       ~ 2s
coffee decade deface facade                ~ 5s
defaced effaced                            ~ 30s+
```

## Options

| Flag | Description |
|------|------------|
| `-m start\|contains\|end` | Where to match (default: `start`) |
| `-p <name>` | Use a preset hex word |
| `-n` | Dry run — preview, then ask to apply |
| `-t <ms>` | Timeout (default: 30s) |
| `-j <n>` | Threads (default: all cores) |
| `-d` | Debug — show speed and attempt count |
| `-q` | Quiet — hide progress spinner |
| `--list-presets` | Show all presets |
| `--message <msg>` | Override commit message |
| `--max-attempts <n>` | Stop after N attempts |

Exit codes: `0` success, `1` bad args, `2` timeout, `3` git error.

## How It Works

Git hashes are `SHA-1(commit object)`. We append an invisible `x-nonce` header and brute-force its value until the hash matches. The commit message and all visible metadata stay identical.

## Performance

~120M hashes/sec on 8 cores (Apple Silicon, release build).

| Pattern | Time |
|---------|------|
| 4-char (`cafe`) | instant |
| 6-char (`c0ffee`) | ~0.2s |
| 6-char contains | ~0.01s |
| 7-char (`0000000`) | ~3s |
| 8-char | ~60s |

## Design

Written in Rust using functional programming patterns:

- **Chain of responsibility** — pattern parsing via `find_map` over parser functions
- **Fold-based state machine** — commit header parsing without mutable flags
- **Iterator pipelines** — `from_fn` + `find_map` for the worker hot loop
- **Railway-oriented errors** — `Result` chains with typed `AppError` for clean control flow
- **Higher-order functions** — `run_git()` eliminates git command boilerplate
- **Incremental SHA-1** — pre-compute hash state, clone per attempt (immutable, FP-clean)
- **Lock-free concurrency** — `AtomicBool` + `AtomicU64` with `Relaxed` ordering

```
src/
  main.rs      CLI + railway-oriented error handling
  pattern.rs   Chain-of-responsibility parser + nibble matching
  preset.rs    Curated hex word dictionary
  commit.rs    Fold-based commit parser
  hasher.rs    Incremental SHA-1 (clone, not mutate)
  worker.rs    Lock-free multi-threaded search (from_fn + find_map)
  nonce.rs     Safe byte-range nonce generation
  git.rs       Higher-order git command helpers
```

## Auto-Vanity Hook

This repo auto-vanities every commit to `000000`. Activates on `cargo build` via `build.rs`.

```bash
git config core.hooksPath hooks   # manual setup
```

## License

[Business Source License 1.1](LICENSE) — free for non-commercial use. Converts to MIT on 2030-04-03.
