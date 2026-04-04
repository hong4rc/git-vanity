# Changelog

## 0.1.0 (2026-04-03)

Initial release.

### Features
- 5 pattern types: hex prefix, repeat, structured, pair, regex
- 27 curated hex word presets (cafe, dead, c0ffee, etc.)
- 3 match positions: start (default), contains, end
- `git vanity show` — inspect HEAD vanity status
- `git vanity log` — validate vanity hashes in repo history
- `git vanity undo` — strip nonce and restore original hash
- `-r` / `--random` — pick a random preset
- `-n` / `--dry-run` — preview match, ask to apply
- Progress bar with ETA for long searches
- Color-highlighted pattern in output
- Terminal bell on completion (> 2s)
- Ctrl+C shows stats before exit

### Performance
- ~120M hashes/sec on 8-core Apple Silicon
- Incremental SHA-1 with hardware acceleration (asm)
- Lock-free multi-threaded workers
- Zero-allocation hot loop
- First-nibble pre-filter for contains mode

### Distribution
- `cargo install git-vanity`
- GitHub Releases (macOS, Linux, Windows binaries)
- Homebrew formula
- npm wrapper
