# zellij-claude-sync shell helpers (fish).
#
# Install: symlink or copy this file into ~/.config/fish/conf.d/ so fish
# auto-sources it at startup:
#     ln -s (pwd)/shell/snap.fish ~/.config/fish/conf.d/zellij-claude-sync.fish
#
# Assumes the built plugin lives at the path in $ZCS_PLUGIN (override in your
# environment if you deploy it elsewhere).

set -q ZCS_PLUGIN; or set -g ZCS_PLUGIN ~/.config/zellij/plugins/zellij-claude-sync.wasm

function snap --description 'Save a Zellij + Claude workspace snapshot'
    if test (count $argv) -eq 0
        echo "Usage: snap <name>"
        return 1
    end
    set -l name $argv[1]
    if not test -f $ZCS_PLUGIN
        echo "✗ plugin not found at $ZCS_PLUGIN — build it and deploy there (or set \$ZCS_PLUGIN)"
        return 1
    end
    # `zellij pipe` stays blocked waiting for plugin output that never arrives,
    # but the dump→enrich→save inside the plugin is synchronous and finishes well
    # within this timeout. We confirm success by the snapshot file, not the exit code.
    timeout 3 zellij pipe --plugin file:$ZCS_PLUGIN --name save -- $name >/dev/null 2>&1
    if test -f ~/.config/zellij/layouts/$name.kdl
        echo "✓ saved snapshot: $name"
    else
        echo "✗ snapshot failed: $name (plugin permitted? run inside a Zellij session?)"
        return 1
    end
end

function snap-list --description 'List saved Zellij snapshots'
    test -d ~/.config/zellij/layouts/; or return 0
    for f in ~/.config/zellij/layouts/*.kdl
        basename $f .kdl
    end
end

function snap-load --description 'Restore a snapshot as a new tab in the current Zellij session'
    if test (count $argv) -eq 0
        echo "Usage: snap-load <name>"
        return 1
    end
    zellij action new-tab --layout $argv[1]
end
