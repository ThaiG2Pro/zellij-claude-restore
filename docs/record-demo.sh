#!/usr/bin/env bash
# Record a short terminal demo (GIF) of a snap → restore round-trip.
#
# Needs: asciinema (record) + agg (render to GIF). Both are optional dev tools:
#   cargo install --git https://github.com/asciinema/agg   # or: brew install agg
#   pipx install asciinema                                 # or your package manager
#
# Usage: docs/record-demo.sh            → writes docs/demo.cast and docs/demo.gif
#
# The recording is meant to be driven by hand: once asciinema starts, run through
#   1) a couple of `claude` panes doing something recognizable
#   2) `snap demo`   (show the "N claude pane(s) will resume" summary)
#   3) exit zellij, then `zellij --layout demo`  (chats resume automatically)
# then press Ctrl-D to stop.
set -euo pipefail

cd "$(dirname "$0")/.."
CAST="docs/demo.cast"
GIF="docs/demo.gif"

command -v asciinema >/dev/null || { echo "install asciinema first"; exit 1; }

echo "▶ recording to $CAST — reproduce a snap → restore round-trip, then Ctrl-D"
asciinema rec --overwrite "$CAST"

if command -v agg >/dev/null; then
    echo "▶ rendering $GIF"
    agg "$CAST" "$GIF"
    echo "✓ $GIF — add it to README.md's Demo section"
else
    echo "ℹ agg not found — keep $CAST or render it later with: agg $CAST $GIF"
fi
