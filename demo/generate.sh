#!/bin/bash
set -e
DIR="$(cd "$(dirname "$0")" && pwd)"

record() {
    local name="$1"
    local height="${2:-10}"
    echo -n "  $name "
    asciinema rec --command "bash $DIR/$name.sh" --overwrite "$DIR/$name.cast" 2>/dev/null
    python3 -c "
import json
lines = open('$DIR/$name.cast').readlines()
h = json.loads(lines[0])
v2 = {'version':2,'width':h['term']['cols'],'height':h['term']['rows']}
with open('/tmp/$name.cast','w') as f:
    f.write(json.dumps(v2)+'\n')
    for l in lines[1:]: f.write(l)
"
    svg-term --in "/tmp/$name.cast" --out "$DIR/$name.svg" --window --width 72 --height "$height" --padding 8
    rm -f "/tmp/$name.cast" "$DIR/$name.cast"
    echo "→ $(du -h "$DIR/$name.svg" | cut -f1)"
}

echo "Generating all demos..."

echo "=== Patterns ==="
record "01-prefix"
record "02-preset"
record "03-random"
record "04-repeat"
record "05-pair"
record "06-structured"
record "07-regex"

echo "=== Positions ==="
record "08-match-end"
record "09-match-contains"

echo "=== Options ==="
record "10-dry-run"
record "11-debug" 12
record "12-quiet-vs-normal" 14
record "13-threads" 12
record "14-timeout"
record "15-max-attempts"
record "16-message" 10
record "17-no-repeat"
record "18-list-presets" 34

echo "=== Subcommands ==="
record "19-show" 12
record "20-log" 14
record "21-undo" 14

echo "=== Error Cases ==="
record "22-invalid-pattern"
record "23-invalid-preset"
record "24-invalid-position"
record "25-no-pattern"
record "26-not-a-repo"

echo "Done. $(ls "$DIR"/*.svg | wc -l) SVGs generated."
