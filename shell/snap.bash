# zellij-claude-sync shell helpers (bash).
#
# Install: source this file from ~/.bashrc, e.g.
#     echo 'source /path/to/zellij-claude-sync/shell/snap.bash' >> ~/.bashrc
# (install.sh does this for you.)
#
# Override $ZCS_PLUGIN if you deploy the plugin somewhere else.

: "${ZCS_PLUGIN:=$HOME/.config/zellij/plugins/zellij-claude-sync.wasm}"

# Save a Zellij + Claude workspace snapshot.
snap() {
    if [ "$#" -eq 0 ]; then
        echo "Usage: snap <name>"
        return 1
    fi
    local name="$1"
    if [ ! -f "$ZCS_PLUGIN" ]; then
        echo "✗ plugin not found at $ZCS_PLUGIN — build it and deploy there (or set \$ZCS_PLUGIN)"
        return 1
    fi
    # `zellij pipe` stays blocked waiting for plugin output that never arrives,
    # but the dump→enrich→save inside the plugin is synchronous and finishes well
    # within this timeout. We confirm success by the snapshot file, not the exit code.
    timeout 3 zellij pipe --plugin "file:$ZCS_PLUGIN" --name save -- "$name" >/dev/null 2>&1
    if [ -f "$HOME/.config/zellij/layouts/$name.kdl" ]; then
        echo "✓ saved snapshot: $name"
    else
        echo "✗ snapshot failed: $name (plugin permitted? run inside a Zellij session?)"
        return 1
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
