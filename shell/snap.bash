# zellij-claude-sync shell helpers (bash).
#
# Install: source this file from ~/.bashrc, e.g.
#     echo 'source /path/to/zellij-claude-sync/shell/snap.bash' >> ~/.bashrc
# (install.sh does this for you.)
#
# Override $ZCS_PLUGIN if you deploy the plugin somewhere else.

: "${ZCS_PLUGIN:=$HOME/.config/zellij/plugins/zellij-claude-sync.wasm}"

# Save a Zellij + Claude workspace snapshot.
# Usage: snap [--manual] <name>
#   --manual : restore claude panes suspended (wait for ENTER) instead of
#              auto-launching them. Also enabled by setting $ZCS_NO_AUTO_ENTER.
snap() {
    local manual=""
    if [ "$1" = "--manual" ] || [ "$1" = "-m" ]; then
        manual=1
        shift
    fi
    if [ "$#" -eq 0 ]; then
        echo "Usage: snap [--manual] <name>"
        return 1
    fi
    local name="$1"
    if [ ! -f "$ZCS_PLUGIN" ]; then
        echo "✗ plugin not found at $ZCS_PLUGIN — build it and deploy there (or set \$ZCS_PLUGIN)"
        return 1
    fi

    local pipe_args=()
    if [ -n "$manual" ] || [ -n "${ZCS_NO_AUTO_ENTER:-}" ]; then
        pipe_args=(--args auto_enter=false)
    fi

    # The plugin writes a one-line JSON status here (guest /tmp/claude-sessions maps
    # to host $ZELLIJ_TMP_DIR = /tmp/zellij-<uid>). Clear stale status before saving.
    local tmp_dir="${ZELLIJ_TMP_DIR:-/tmp/zellij-$(id -u)}"
    local status_file="$tmp_dir/claude-sessions/.last-save.json"
    rm -f "$status_file" 2>/dev/null

    # `zellij pipe` stays blocked waiting for plugin output that never arrives,
    # but the dump→enrich→save inside the plugin is synchronous and finishes well
    # within this timeout. We confirm success by the snapshot file, not the exit code.
    timeout 3 zellij pipe --plugin "file:$ZCS_PLUGIN" --name save "${pipe_args[@]}" -- "$name" >/dev/null 2>&1

    if [ -f "$HOME/.config/zellij/layouts/$name.kdl" ]; then
        echo "✓ saved snapshot: $name"
        __snap_report "$status_file"
    else
        echo "✗ snapshot failed: $name (plugin permitted? run inside a Zellij session?)"
        return 1
    fi
}

# Print the enrichment summary from the plugin's status file, if present.
__snap_report() {
    local status_file="$1"
    [ -f "$status_file" ] || return 0
    local json enriched pinned missing
    json="$(cat "$status_file" 2>/dev/null)"
    enriched="$(printf '%s' "$json" | grep -o '"enriched":[0-9]*' | grep -o '[0-9]*')"
    pinned="$(printf '%s' "$json" | grep -o '"already_pinned":[0-9]*' | grep -o '[0-9]*')"
    missing="$(printf '%s' "$json" | grep -o '"missing_marker":[0-9]*' | grep -o '[0-9]*')"
    [ -n "$enriched" ] || return 0
    echo "  $enriched claude pane(s) will resume · $pinned already pinned · $missing without a marker"
    if [ "${missing:-0}" -gt 0 ] 2>/dev/null; then
        echo "  ⚠ $missing claude pane(s) had no session marker — is the SessionStart hook installed?"
    fi
}

# List saved Zellij snapshots.
snap-list() {
    [ -d "$HOME/.config/zellij/layouts" ] || return 0
    local f
    for f in "$HOME"/.config/zellij/layouts/*.kdl; do
        [ -e "$f" ] || continue
        basename "$f" .kdl
    done
}

# Restore a snapshot as a new tab in the current Zellij session.
snap-load() {
    if [ "$#" -eq 0 ]; then
        echo "Usage: snap-load <name>"
        return 1
    fi
    zellij action new-tab --layout "$1"
}
