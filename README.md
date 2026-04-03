# git-vanity

Generate Git commit hashes matching custom patterns. Make your commits start with `cafe`, end with `dead`, or contain `c0ffee` anywhere.

```
$ git commit -m "feat: add login"
$ git vanity cafe
✓ a1b2c3d4e5f6 → cafeb0ba1234... (42,567 attempts, 0.01s)

$ git log --oneline -1
cafeb0b feat: add login
```

The commit message, author, and date stay identical. Only an invisible `x-nonce` header is added.

> **Proof it works:** This repo's own commit starts with `000000` — check `git log --oneline` to see it.

## Install

**Homebrew** (macOS/Linux):
```bash
brew install hong4rc/tap/git-vanity
```

**npm** (any platform):
```bash
npx git-vanity cafe
# or install globally
npm install -g git-vanity
```

**Cargo** (from source):
```bash
cargo install git-vanity
```

**GitHub Releases** (prebuilt binaries):

Download from [Releases](https://github.com/hong4rc/git-vanity/releases) for macOS (Intel + ARM), Linux (x64 + arm64), and Windows.

**Build locally:**
```bash
cargo build --release
# Binary at ./target/release/git-vanity
```

## Usage

```bash
git vanity <pattern> [options]
```

### Patterns

| Type | Syntax | Example | Matches |
|------|--------|---------|---------|
| Prefix | `<hex>` | `git vanity cafe` | Hash starts with `cafe` |
| Repeat | `repeat:<n>` | `git vanity repeat:3` | Any run of 3 identical chars (`aaa`, `111`) |
| Structured | `<hex>x{n}` | `git vanity 1997xxx` | `1997` + 3 identical chars |
| Pair | `xx` | `git vanity xx` | Any 2 identical adjacent chars |
| Regex | `/<regex>/` | `git vanity "/^dead/"` | Regex against hex hash |

### Match Position

By default, patterns match at the **start** of the hash. Use `-m` to match elsewhere:

```bash
git vanity cafe                 # cafeb0ba1234...   (start — default)
git vanity cafe -m end          # ...9fde699cafe    (end of hash)
git vanity cafe -m contains     # ...17cafe995...   (anywhere in hash)
```

`contains` is much faster since any position in the 40-char hash can match.

### Presets

Use `-p` for curated hex words:

```bash
git vanity -p cafe              # hash starts with cafe
git vanity -p coffee -m end     # hash ends with c0ffee
git vanity -p dead -m contains  # hash contains dead
git vanity --list-presets       # see all 27 presets
```

Available presets:

```
ace      ace       instant    add      add       instant
bad      bad       instant    babe     babe      < 1s
beef     beef      < 1s       cafe     cafe      < 1s
code     c0de      < 1s       dead     dead      < 1s
face     face      < 1s       food     f00d      < 1s
feed     feed      < 1s       decaf    decaf     ~ 2s
coffee   c0ffee    ~ 5s       decade   decade    ~ 5s
facade   facade    ~ 5s       defaced  defaced   ~ 30s+
```

### Dry Run

Use `-n` to preview the match before writing. In an interactive terminal, it will ask to apply:

```bash
$ git vanity 000000 -n
✓ Found matching hash: 00000075b362... (143,602,767 attempts, 7.96s)
Apply? [Y/n] y
✓ 0000073d64ed → 00000075b362... (applied)
```

This avoids searching twice — preview and apply in one step.

### Show Vanity Info

Check if HEAD has a vanity hash:

```bash
$ git vanity show
Commit: 000000ee199f14cd13dee8803da93a8dfc8757cd
Vanity: yes (x-nonce present)
Prefix:  000000 (6 identical chars)
Message: ux: bell notification on long searches (> 2s)
```

### Vanity Log

See which commits in your repo have vanity hashes:

```bash
$ git vanity log
✓ 000000d chore: remove unused deps (rand, atty), use std::io::IsTerminal
✓ 0000005 ux: add 'git vanity show' to inspect HEAD vanity status
✓ 000000e ux: bell notification on long searches (> 2s)
  ...
17/17 commits have vanity hashes
```

### Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--match <pos>` | `-m` | `start` | Where to match: `start`, `contains`, or `end` |
| `--preset <name>` | `-p` | | Use a preset hex word |
| `--list-presets` | | | List all available presets |
| `--dry-run` | `-n` | | Preview match, then ask to apply |
| `--message <msg>` | | HEAD's message | Override commit message |
| `--timeout <ms>` | `-t` | `30000` | Abort after N milliseconds |
| `--max-attempts <n>` | | unlimited | Abort after N hash attempts |
| `--debug` | `-d` | | Show throughput metrics |
| `--no-repeat` | | | Disable structured pattern detection |
| `--threads <n>` | `-j` | num cpus | Number of worker threads |
| `--quiet` | `-q` | | Hide progress spinner |

Use `git vanity show` to inspect HEAD's vanity status.

A terminal bell plays when searches take longer than 2 seconds.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Invalid arguments / pattern |
| 2 | Timeout or max-attempts reached |
| 3 | Git error (not a repo, no HEAD) |

## How It Works

A Git commit hash is `SHA-1(commit <len>\0<header>\n<message>)`. By appending a hidden `x-nonce` field to the commit header and brute-forcing its value, we search for a hash matching the desired pattern — without altering any user-visible data.

### Performance

| Pattern | Expected Time |
|---------|--------------|
| 4-char prefix (`cafe`) | < 1 second |
| 6-char prefix (`c0ffee`) | < 5 seconds |
| 6-char contains (`c0ffee`) | < 0.1 seconds |
| 8-char prefix | < 60 seconds |

Throughput: ~50M hashes/sec on 8 cores (release build).

### Architecture

- **Incremental SHA-1**: Pre-computes hash state up to the nonce, clones per attempt
- **Lock-free workers**: N threads with `AtomicBool` stop signal, batched checking
- **Nibble matching**: Compares raw bytes for prefix patterns (no hex encoding in hot loop)
- **Zero-allocation hot loop**: Stack-allocated nonces, no heap alloc per attempt

```
src/
  main.rs      — CLI entry, arg parsing (clap)
  pattern.rs   — Pattern parsing & matching (chain-of-responsibility)
  preset.rs    — Curated hex word presets
  commit.rs    — Git commit parsing (fold-based state machine)
  hasher.rs    — Incremental SHA-1 hashing
  worker.rs    — Multi-threaded brute-force coordinator
  nonce.rs     — Nonce generation (0x80-0xFF safe range)
  git.rs       — Git plumbing operations
```

## Auto-Vanity Hook

This repo includes a `post-commit` hook that automatically rewrites every commit to start with `000000`. It activates on first `cargo build` via `build.rs`.

To set it up manually:

```bash
git config core.hooksPath hooks
```

Every commit in this repo proves the tool works — check `git log --oneline`.

## Author

**hong4rc** — [github.com/hong4rc](https://github.com/hong4rc)

## License

Source Available — free for personal and open-source use. Commercial use requires a paid license. See [LICENSE](LICENSE) for details.
