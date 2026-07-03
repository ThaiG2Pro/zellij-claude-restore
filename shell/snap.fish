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
    # Usage: snap [--manual] <name>
    #   --manual : restore claude panes suspended (wait for ENTER) instead of
    #              auto-launching them. Also enabled by setting $ZCS_NO_AUTO_ENTER.
    argparse m/manual -- $argv
    or return 1
    if test (count $argv) -eq 0
        echo "Usage: snap [--manual] <name>"
        return 1
    end
    set -l name $argv[1]
    if not test -f $ZCS_PLUGIN
        echo "✗ plugin not found at $ZCS_PLUGIN — build it and deploy there (or set \$ZCS_PLUGIN)"
        return 1
    end

    set -l pipe_args
    if set -q _flag_manual; or set -q ZCS_NO_AUTO_ENTER
        set pipe_args --args auto_enter=false
    end

    # The plugin writes a one-line JSON status here (guest /tmp/claude-sessions maps
    # to host $ZELLIJ_TMP_DIR = /tmp/zellij-<uid>). Clear stale status before saving.
    set -l tmp_dir (set -q ZELLIJ_TMP_DIR; and echo $ZELLIJ_TMP_DIR; or echo /tmp/zellij-(id -u))
    set -l status_file $tmp_dir/claude-sessions/.last-save.json
    rm -f $status_file 2>/dev/null

    # `zellij pipe` stays blocked waiting for plugin output that never arrives,
    # but the dump→enrich→save inside the plugin is synchronous and finishes well
    # within this timeout. We confirm success by the snapshot file, not the exit code.
    timeout 3 zellij pipe --plugin file:$ZCS_PLUGIN --name save $pipe_args -- $name >/dev/null 2>&1

    if test -f ~/.config/zellij/layouts/$name.kdl
        echo "✓ saved snapshot: $name"
        __snap_report $status_file
    else
        echo "✗ snapshot failed: $name (plugin permitted? run inside a Zellij session?)"
        return 1
    end
end

# Print the enrichment summary from the plugin's status file, if present.
function __snap_report --argument-names status_file
    test -f $status_file; or return 0
    set -l json (cat $status_file 2>/dev/null)
    set -l enriched (string match -rg '"enriched":([0-9]+)' -- $json)
    set -l pinned (string match -rg '"already_pinned":([0-9]+)' -- $json)
    set -l missing (string match -rg '"missing_marker":([0-9]+)' -- $json)
    test -z "$enriched"; and return 0
    echo "  $enriched claude pane(s) will resume · $pinned already pinned · $missing without a marker"
    if test "$missing" -gt 0 2>/dev/null
        echo "  ⚠ $missing claude pane(s) had no session marker — is the SessionStart hook installed?"
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
