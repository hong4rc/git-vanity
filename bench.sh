#!/bin/bash
# Benchmark git-vanity with multiple iterations per pattern.
# Shows min/median/max speed across runs for reliable results.
#
# Usage: bash bench.sh [iterations] [binary]
# Example: bash bench.sh 5 ./target/release/git-vanity

set -e

ITERATIONS="${1:-5}"
BINARY="${2:-git-vanity}"

# Setup temp repos — different commit messages for varied contexts
REPOS=()
MESSAGES=(
    "feat: add user authentication"
    "fix: resolve null pointer in parser"
    "chore: update dependencies to latest"
    "docs: improve API reference guide"
    "refactor: extract validation logic"
)

for i in $(seq 0 $((ITERATIONS - 1))); do
    repo=$(mktemp -d)
    git init "$repo" > /dev/null 2>&1
    msg_idx=$((i % ${#MESSAGES[@]}))
    git -C "$repo" commit --allow-empty -m "${MESSAGES[$msg_idx]}" > /dev/null 2>&1
    REPOS+=("$repo")
done

echo "╔═══════════════════════════════════════════════════════════════════════════╗"
echo "║                       git-vanity benchmark ($ITERATIONS iterations)                    ║"
echo "╠═══════════════════════════════════════════════════════════════════════════╣"
printf "║ %-12s │ %-10s │ %8s │ %8s │ %8s │ %8s ║\n" "Pattern" "Position" "Min" "Median" "Max" "Med.Speed"
echo "╠═══════════════════════════════════════════════════════════════════════════╣"

# Sort numbers, return median
median() {
    echo "$@" | tr ' ' '\n' | sort -n | awk -v n="$#" '{a[NR]=$1} END{print a[int((n+1)/2)]}'
}

min_of() {
    echo "$@" | tr ' ' '\n' | sort -n | head -1
}

max_of() {
    echo "$@" | tr ' ' '\n' | sort -n | tail -1
}

run_bench() {
    local label="$1"
    local pattern="$2"
    local position="${3:-start}"
    local timeout="${4:-30000}"

    local speeds=()
    local times=()

    for i in $(seq 0 $((ITERATIONS - 1))); do
        local repo="${REPOS[$i]}"
        local output
        output=$("$BINARY" "$pattern" -m "$position" -t "$timeout" -q --dry-run --debug 2>&1 -C "$repo" || true)
        # Run from repo dir
        output=$(cd "$repo" && "$BINARY" "$pattern" -m "$position" -t "$timeout" -q --dry-run --debug 2>&1) || true

        local speed=$(echo "$output" | grep -o 'speed: [0-9.]*M' | grep -o '[0-9.]*')
        local elapsed=$(echo "$output" | grep -oE '[0-9]+\.[0-9]+s\)' | tail -1 | tr -d 's)')

        [ -n "$speed" ] && speeds+=("$speed")
        [ -n "$elapsed" ] && times+=("$elapsed")
    done

    if [ ${#speeds[@]} -eq 0 ]; then
        printf "║ %-12s │ %-10s │ %8s │ %8s │ %8s │ %8s ║\n" \
            "$label" "$position" "-" "-" "-" "timeout"
        return
    fi

    # For instant matches (no time captured), use speed-based display
    if [ ${#times[@]} -eq 0 ]; then
        local med_speed=$(median "${speeds[@]}")
        local min_speed=$(min_of "${speeds[@]}")
        local max_speed=$(max_of "${speeds[@]}")
        printf "║ %-12s │ %-10s │ %7ss │ %7ss │ %7ss │ %5sM/s ║\n" \
            "$label" "$position" "<0.01" "<0.01" "<0.01" "$med_speed"
        return
    fi

    local min_t=$(min_of "${times[@]}")
    local med_t=$(median "${times[@]}")
    local max_t=$(max_of "${times[@]}")
    local med_speed=$(median "${speeds[@]}")

    printf "║ %-12s │ %-10s │ %6ss │ %6ss │ %6ss │ %5sM/s ║\n" \
        "$label" "$position" "$min_t" "$med_t" "$max_t" "$med_speed"
}

# === Prefix patterns — increasing difficulty ===
run_bench "cafe"      "cafe"      "start"
run_bench "cafeb"     "cafeb"     "start"
run_bench "cafeba"    "cafeba"    "start"
run_bench "000000"    "000000"    "start"

echo "╠───────────────────────────────────────────────────────────────────────────╣"

# === Position comparison — same pattern, different match modes ===
run_bench "cafe"      "cafe"      "start"
run_bench "cafe"      "cafe"      "end"
run_bench "cafe"      "cafe"      "contains"
run_bench "c0ffee"    "c0ffee"    "start"
run_bench "c0ffee"    "c0ffee"    "contains"

echo "╠───────────────────────────────────────────────────────────────────────────╣"

# === Pattern types ===
run_bench "repeat:3"  "repeat:3"  "start"
run_bench "repeat:4"  "repeat:4"  "start"
run_bench "xx"        "xx"        "start"
run_bench "aaxxx"     "aaxxx"     "start"
run_bench "/^dead/"   "/^dead/"   "start"

echo "╠───────────────────────────────────────────────────────────────────────────╣"

# === Presets ===
run_bench "dead"      "dead"      "start"
run_bench "beef"      "beef"      "start"
run_bench "decade"    "decade"    "start"

echo "╚═══════════════════════════════════════════════════════════════════════════╝"
echo ""
echo "Iterations: $ITERATIONS"
echo "Threads: $(nproc 2>/dev/null || sysctl -n hw.ncpu)"
echo "Platform: $(uname -ms)"
echo "Binary: $BINARY"

# Cleanup
for repo in "${REPOS[@]}"; do
    rm -rf "$repo"
done
