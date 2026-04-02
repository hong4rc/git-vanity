# git vanity — Product Requirements Document

## 1. Objective

Build a high-performance CLI tool that generates Git commit hashes matching
user-defined patterns (vanity hashes). The tool mutates only invisible metadata
(a binary nonce in the commit header) so the commit message, tree, parent, author,
and committer remain byte-for-byte identical.

## 2. Background

A Git commit hash is `SHA-1(commit <len>\0<header>\n<message>)`. Any byte change
in the commit object produces a completely different hash. By appending a hidden
`x-nonce` field to the commit header and brute-forcing its value, we can search
for a hash that matches a desired pattern without altering any user-visible data.

Git resolves `git vanity` to the binary `git-vanity` on `$PATH`, making this a
seamless Git subcommand.

## 3. Goals

### Primary

- Generate valid Git commits whose hash matches a user-specified pattern
- Preserve commit message integrity (byte-for-byte)
- Produce no visible difference in `git log` output
- Achieve high throughput via multi-threaded brute-force (Rust)

### Secondary

- Cross-platform: macOS (Intel + Apple Silicon), Linux, Windows
- Simple CLI: `git vanity <pattern>`
- Optional npm wrapper for easy distribution

## 4. Non-Goals

- Modifying commits beyond HEAD (no automated rebase)
- Supporting non-Git VCS
- Guaranteeing cryptographic uniqueness
- SHA-256 support in v1 (abstracted for future)

## 5. CLI Specification

```
git vanity <pattern> [options]
```

### 5.1 Pattern Types

| Type | Syntax | Example | Match Rule |
|------|--------|---------|------------|
| **Prefix** | `<hex>` | `git vanity cafe` | `hash.startsWith("cafe")` |
| **Repeat** | `repeat:<n>` | `git vanity repeat:3` | Any run of `n` identical hex chars (e.g. `aaa`, `111`) |
| **Structured** | `<prefix>x{n}` | `git vanity 1997xxx` | Prefix `1997` followed by 3 identical chars |
| **Pair** | `xx` | `git vanity xx` | Any position has 2 identical adjacent chars |
| **Regex** | `/<regex>/` | `git vanity "/1997(.)\1/"` | Full regex against hex hash string |

Pattern validation rules:
- Prefix patterns must be valid hex (`[0-9a-f]`)
- `x` in structured patterns is a wildcard for "any identical char"
- Regex patterns are delimited by `/`
- Invalid patterns produce a clear error and exit code 1

### 5.2 Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--amend` | | `true` | Rewrite HEAD commit (default behavior) |
| `--message <msg>` | `-m` | HEAD's message | Override commit message |
| `--timeout <ms>` | `-t` | `30000` | Abort after N milliseconds |
| `--max-attempts <n>` | | `∞` | Abort after N hash attempts |
| `--dry-run` | `-n` | `false` | Print matching hash without writing |
| `--debug` | `-d` | `false` | Show throughput metrics |
| `--no-repeat` | | `false` | Disable auto-repeat pattern detection |
| `--threads <n>` | `-j` | `num_cpus` | Number of worker threads |

### 5.3 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success — matching commit written (or printed with `--dry-run`) |
| 1 | Invalid arguments / pattern |
| 2 | Timeout or max-attempts reached without match |
| 3 | Git error (not a repo, no HEAD, write failure) |

## 6. Functional Requirements

### 6.1 Read HEAD Commit

```
git cat-file commit HEAD
```

Parse into:
- **Header fields**: `tree`, `parent` (0..n), `author`, `committer`, `gpgsig` (optional), other extra headers
- **Message**: everything after the first blank line

### 6.2 Nonce Injection

Append to the header (after `committer`, before message):

```
x-nonce <binary-payload>
```

Nonce requirements:
- 8–16 bytes of random binary data per attempt
- Non-printable bytes (outside ASCII 0x20–0x7E) to avoid `git log` display
- Monotonically incrementing counter per thread + random base (avoids collisions across threads)
- Must not contain `\n` (0x0A) as that terminates the header line — use byte range `0x80–0xFF`

### 6.3 Hash Computation

Reconstruct the full commit object:

```
commit <content-length>\0<tree ...>\n<parent ...>\n<author ...>\n<committer ...>\nx-nonce <binary>\n\n<message>
```

Compute: `SHA-1(object)`

### 6.4 Pattern Matching

Evaluate the hex-encoded hash against the active pattern engine.

Optimization: for prefix patterns, compare raw bytes (not hex strings) to avoid
hex-encoding overhead on every attempt.

### 6.5 Commit Write (on match)

```bash
# Write the object to Git's object store
echo -n "$content" | git hash-object -t commit -w --stdin

# Update HEAD to point to the new commit
git update-ref HEAD <new-hash>
```

**Important**: Use `git update-ref` instead of `git reset --hard` to avoid
touching the working tree or index unnecessarily. The tree hash is identical,
so the working directory is already correct.

### 6.6 GPG Signature Handling

If the original commit has a `gpgsig` header:
- **Default**: Strip the signature and warn the user (the nonce invalidates it)
- **`--keep-sig`** (future): Attempt to re-sign if GPG key is available

## 7. Non-Functional Requirements

### 7.1 Performance Targets

| Pattern | Target (release build) |
|---------|----------------------|
| 4-char prefix (16^4 = 65K space) | < 1 second |
| 6-char prefix (16^6 = 16M space) | < 5 seconds |
| 8-char prefix (16^8 = 4B space) | < 60 seconds |

Expected throughput: **50–150M hashes/sec** on modern hardware (8+ cores).

### 7.2 Reliability

- Must never corrupt the Git repository
- Must produce valid Git objects (`git fsck` clean)
- Must handle concurrent Git operations gracefully (lock file awareness)
- Atomic: either writes the new commit or leaves HEAD unchanged

### 7.3 Portability

- macOS: Intel + Apple Silicon
- Linux: x86_64 + aarch64
- Windows: x86_64

## 8. Architecture

### 8.1 Core (Rust)

```
src/
  main.rs          — CLI entry, arg parsing (clap)
  pattern.rs       — Pattern parsing and matching engine
  commit.rs        — Git commit parsing and reconstruction
  hasher.rs        — SHA-1 hashing with nonce injection
  worker.rs        — Multi-threaded brute-force coordinator
  nonce.rs         — Nonce generation strategy
  git.rs           — Git plumbing operations (read/write objects)
```

### 8.2 Concurrency Model

- Spawn `N` worker threads (default: `num_cpus`)
- Each thread gets a unique nonce range (base + thread_id offset)
- Threads increment their nonce counter independently
- First match signals all threads to stop via `AtomicBool`
- Main thread collects result and writes commit

### 8.3 Optional npm Wrapper

```json
{
  "name": "git-vanity",
  "bin": { "git-vanity": "bin/index.js" },
  "optionalDependencies": {
    "git-vanity-darwin-arm64": "*",
    "git-vanity-darwin-x64": "*",
    "git-vanity-linux-x64": "*",
    "git-vanity-win32-x64": "*"
  }
}
```

`bin/index.js` selects the correct binary for the current platform and executes it.

## 9. Observability

### 9.1 Standard Output

```
✓ Found matching hash: cafebabe1234... (1,234,567 attempts, 0.82s)
```

### 9.2 Debug Output (`--debug` or `VANITY_DEBUG=1`)

```
[vanity] threads: 8
[vanity] pattern: prefix("cafe")
[vanity] attempt: 1,000,000 | speed: 120M hash/sec | elapsed: 0.42s
[vanity] match: cafebabe1234... | nonce: 0x8a9b3c... | attempts: 1,234,567
```

### 9.3 Progress (stderr)

When attached to a TTY, show a live progress line on stderr:

```
⠋ Searching... 1.2M attempts | 98M hash/sec
```

## 10. Edge Cases

| Case | Behavior |
|------|----------|
| No HEAD (empty repo) | Error with exit code 3 and helpful message |
| Detached HEAD | Works normally — updates HEAD ref |
| Merge commit (multiple parents) | Preserves all parent refs |
| GPG-signed commit | Strip signature + warn (nonce invalidates it) |
| Large commit message | No impact (message is unchanged, only header grows) |
| `.git/index.lock` exists | Error with message to retry |
| SHA-256 repo (`extensions.objectFormat = sha256`) | v1: error. Future: abstract hash function |

## 11. Distribution

| Channel | Artifact |
|---------|----------|
| GitHub Releases | `git-vanity-{os}-{arch}` binaries |
| Homebrew | `brew install git-vanity` (future) |
| npm | `npx git-vanity` (optional wrapper) |
| Cargo | `cargo install git-vanity` |

## 12. Risks & Mitigation

| Risk | Mitigation |
|------|-----------|
| Slow brute-force for long patterns | Multi-threading + raw byte comparison |
| History confusion after rewrite | Clear output showing old → new hash |
| Invalid commit format | Strict parser + `git fsck` validation in tests |
| Git version incompatibility | Use only stable plumbing commands |
| GPG signature invalidation | Warn user, strip by default |

## 13. Success Criteria

- [ ] `git vanity cafe` produces a commit starting with `cafe`
- [ ] `git log` shows identical message, author, date
- [ ] `git fsck` reports no errors
- [ ] 4-char prefix completes in < 1 second
- [ ] Works on macOS, Linux, Windows
- [ ] `--dry-run` prints hash without modifying repo

## 14. Future Enhancements

- Regex pattern engine (full PCRE support)
- GPU acceleration (SHA-1 on compute shaders)
- Pattern presets (`--preset cool` → curated patterns)
- Distributed brute-force across machines
- `git vanity log` — show vanity stats for repo
- Commit signature re-signing after nonce injection
- Branch name vanity (`git vanity-branch`)

## 15. One-Line Summary

**git vanity** = brute-force commit nonce → match hash pattern → write invisible
mutation → beautiful commit hash.
