#!/usr/bin/env bash
# Measures patinote-app in a real window against the PRD targets (step 2.10):
# start until the note is drawn (index already built), memory after 2 s
# idle, and key-to-draw latency on a 5,000-line note in a 10,000-note vault.
#
# Usage: scripts/measure.sh [RUNS] [BACKENDS...]
#   RUNS      measured runs per backend (default 3)
#   BACKENDS  tiny-skia and/or wgpu (default: both)
#
# Needs a desktop session (Wayland or X11). Uses only a generated vault and
# its own XDG folders under target/measure/, never your vault or settings.
# Results: target/measure/results.jsonl, one JSON object per run.
set -euo pipefail

cd "$(dirname "$0")/.."
runs="${1:-3}"
shift || true
backends=("$@")
[ ${#backends[@]} -eq 0 ] && backends=(tiny-skia wgpu)

out="$PWD/target/measure"
vault="$out/vault-10k"

cargo build --release -q -p patinote-app -p vaultgen
app="$PWD/target/release/patinote-app"

rm -rf "$out"
mkdir -p "$out/cache" "$out/state" "$out/config"
"$PWD/target/release/vaultgen" --notes 10000 --long-note-lines 5000 --out "$vault" >/dev/null

# The 5,000-line note, relative to the vault; kept to restore after each run
# (the script types into it and autosave writes it).
note="$(cd "$vault" && find . -name '*.md' -not -path './.*' -exec wc -l {} + |
    grep -v ' total$' | sort -rn | awk 'NR == 1' | sed 's|^ *[0-9]* \./||')"
cp "$vault/$note" "$out/long-note.md"

export XDG_CACHE_HOME="$out/cache" XDG_STATE_HOME="$out/state" XDG_CONFIG_HOME="$out/config"

run() {
    cp "$out/long-note.md" "$vault/$note"
    ICED_BACKEND="$1" "$app" --vault "$vault" --open "$note" --bench 2>/dev/null
}

for backend in "${backends[@]}"; do
    # The first run builds the index; it is not counted.
    run "$backend" >/dev/null
    for _ in $(seq "$runs"); do
        run "$backend" | tee -a "$out/results.jsonl"
    done
done
cp "$out/long-note.md" "$vault/$note"
echo "note: $note ($(wc -l <"$out/long-note.md") lines)" >&2
