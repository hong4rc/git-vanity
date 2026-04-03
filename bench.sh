#!/bin/bash
# Benchmark git-vanity across pattern types and lengths.
# Run from a git repo with at least one commit.

set -e

BINARY="${1:-git-vanity}"
REPO=$(mktemp -d)

# Setup temp repo
git init "$REPO" > /dev/null 2>&1
git -C "$REPO" commit --allow-empty -m "bench commit" > /dev/null 2>&1

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                  git-vanity benchmark                       ║"
echo "╠══════════════════════════════════════════════════════════════╣"
printf "║ %-12s │ %-10s │ %8s │ %10s │ %8s ║\n" "Pattern" "Position" "Time" "Attempts" "Speed"
echo "╠══════════════════════════════════════════════════════════════╣"

run_bench() {
    local label="$1"
    local pattern="$2"
    local position="${3:-start}"
    local timeout="${4:-30000}"

    local output
    output=$("$BINARY" "$pattern" -m "$position" -t "$timeout" -q --dry-run --debug 2>&1) || true

    local speed=$(echo "$output" | grep -o 'speed: [0-9.]*M' | grep -o '[0-9.]*')
    local attempts=$(echo "$output" | grep -o 'attempts: [0-9,]*' | head -1 | grep -o '[0-9,]*')
    # Extract elapsed from debug line: "match: ... | attempts: N | speed: XM hash/sec"
    # Or from output line: "(N attempts, Xs)"
    local time=$(echo "$output" | grep -oE '[0-9]+\.[0-9]+s\)' | tail -1 | tr -d 's)')

    if [ -z "$speed" ]; then
        speed="-"
        attempts="timeout"
        time=">$(echo "$timeout/1000" | bc)"
    fi

    [ -z "$time" ] && time="<0.01"

    printf "║ %-12s │ %-10s │ %7ss │ %10s │ %6sM/s ║\n" \
        "$label" "$position" "$time" "$attempts" "$speed"
}

cd "$REPO"

# Prefix patterns — increasing difficulty
run_bench "cafe"      "cafe"      "start"
run_bench "cafeb"     "cafeb"     "start"
run_bench "cafeba"    "cafeba"    "start"
run_bench "000000"    "000000"    "start"
run_bench "0000000"   "0000000"   "start"

echo "╠──────────────────────────────────────────────────────────────╣"

# Position comparison
run_bench "cafe"      "cafe"      "start"
run_bench "cafe"      "cafe"      "end"
run_bench "cafe"      "cafe"      "contains"
run_bench "c0ffee"    "c0ffee"    "start"
run_bench "c0ffee"    "c0ffee"    "contains"

echo "╠──────────────────────────────────────────────────────────────╣"

# Other pattern types
run_bench "repeat:3"  "repeat:3"  "start"
run_bench "repeat:4"  "repeat:4"  "start"
run_bench "xx"        "xx"        "start"
run_bench "aaxxx"     "aaxxx"     "start"
run_bench "/^dead/"   "/^dead/"   "start"

echo "╠──────────────────────────────────────────────────────────────╣"

# Presets
run_bench "dead"      "dead"      "start"
run_bench "beef"      "beef"      "start"
run_bench "decade"    "decade"    "start"

echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Threads: $(nproc 2>/dev/null || sysctl -n hw.ncpu)"
echo "Platform: $(uname -ms)"

# Cleanup
rm -rf "$REPO"
