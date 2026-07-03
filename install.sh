#!/usr/bin/env bash
# zellij-claude-restore installer (hybrid: build from source if possible, else download a release).
#
# Usage:
#   ./install.sh                 # auto: build if cargo+wasm target present, else download
#   ZCS_MODE=build ./install.sh  # force building from source
#   ZCS_MODE=download ./install.sh
#   ZCS_REPO=user/repo ./install.sh   # where to fetch prebuilt releases from
#   ZCS_VERSION=v0.1.0 ./install.sh   # release tag to download (default: latest)
#
# What it does:
#   1. Puts zellij-claude-restore.wasm in ~/.config/zellij/plugins/
#   2. Pre-grants the plugin's ReadApplicationState/ChangeApplicationState permission
#   3. Sources the snap helpers for your shell (fish/bash/zsh)
#   4. Copies the SessionStart hook into ~/.claude/hooks/
#   5. PRINTS the ~/.claude/settings.json snippet you must add by hand
#      (we never edit your Claude settings for you)

set -euo pipefail

ZELLIJ_TILE_PIN="0.44.x"           # the zellij version this plugin's ABI targets
REPO="${ZCS_REPO:-}"               # for download mode; auto-detected from git remote, else default below
VERSION="${ZCS_VERSION:-latest}"
MODE="${ZCS_MODE:-auto}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_DIR="$HOME/.config/zellij/plugins"
PLUGIN_PATH="$PLUGIN_DIR/zellij-claude-restore.wasm"
PERMS="$HOME/.cache/zellij/permissions.kdl"
HOOK_SRC="$SCRIPT_DIR/hooks/session-marker.py"
HOOK_DST="$HOME/.claude/hooks/zellij-claude-session-marker.py"

say()  { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!  \033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m✗  \033[0m %s\n' "$*" >&2; exit 1; }

# Auto-detect REPO from a git remote when running inside the cloned repo.
if [ -z "$REPO" ] && command -v git >/dev/null 2>&1; then
    url="$(git -C "$SCRIPT_DIR" config --get remote.origin.url 2>/dev/null || true)"
    case "$url" in
        *github.com[:/]*) REPO="$(echo "$url" | sed -E 's#.*github.com[:/]([^/]+/[^/.]+).*#\1#')" ;;
    esac
fi
REPO="${REPO:-ThaiG2Pro/zellij-claude-restore}"   # fallback for download mode

have_cargo_wasm() {
    command -v cargo >/dev/null 2>&1 || return 1
    rustup target list --installed 2>/dev/null | grep -qx wasm32-wasip1 && return 0
    # cargo without rustup-managed target: try anyway, build will tell us
    command -v rustc >/dev/null 2>&1
}

build_from_source() {
    [ -f "$SCRIPT_DIR/Cargo.toml" ] || die "no Cargo.toml next to install.sh — clone the repo to build, or use download mode"
    command -v cargo >/dev/null 2>&1 || die "cargo not found — install Rust (https://rustup.rs) or use ZCS_MODE=download"
    if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-wasip1; then
        say "Adding rust target wasm32-wasip1"
        rustup target add wasm32-wasip1
    fi
    say "Building plugin from source (cargo build --release --target wasm32-wasip1)"
    ( cd "$SCRIPT_DIR" && cargo build --release --target wasm32-wasip1 )
    install -D -m 0644 "$SCRIPT_DIR/target/wasm32-wasip1/release/zellij-claude-restore.wasm" "$PLUGIN_PATH"
}

download_release() {
    [ -n "$REPO" ] || die "set ZCS_REPO=user/repo to download a prebuilt .wasm (or run from the cloned repo with cargo to build)"
    command -v curl >/dev/null 2>&1 || die "curl not found"
    local url
    if [ "$VERSION" = latest ]; then
        url="https://github.com/$REPO/releases/latest/download/zellij-claude-restore.wasm"
    else
        url="https://github.com/$REPO/releases/download/$VERSION/zellij-claude-restore.wasm"
    fi
    say "Downloading prebuilt plugin: $url"
    warn "Prebuilt .wasm targets zellij $ZELLIJ_TILE_PIN — if your zellij differs, build from source instead."
    mkdir -p "$PLUGIN_DIR"
    curl -fSL "$url" -o "$PLUGIN_PATH" || die "download failed — check ZCS_REPO/ZCS_VERSION, or build from source"
}

# --- 1. obtain the wasm ---------------------------------------------------
mkdir -p "$PLUGIN_DIR"
case "$MODE" in
    build)    build_from_source ;;
    download) download_release ;;
    auto)
        if [ -f "$SCRIPT_DIR/Cargo.toml" ] && have_cargo_wasm; then
            build_from_source
        else
            download_release
        fi ;;
    *) die "unknown ZCS_MODE='$MODE' (build|download|auto)" ;;
esac
say "Plugin installed at $PLUGIN_PATH"

# --- 2. pre-grant permission ---------------------------------------------
# Keyed by the plugin's BARE absolute path (RunPluginLocation::File Display).
# The node name contains '/', so it MUST be double-quoted or KDL parsing fails.
mkdir -p "$(dirname "$PERMS")"
touch "$PERMS"
if ! grep -qF "\"$PLUGIN_PATH\"" "$PERMS"; then
    {
        printf '"%s" {\n' "$PLUGIN_PATH"
        printf '    ReadApplicationState\n'
        printf '    ChangeApplicationState\n'
        printf '}\n'
    } >> "$PERMS"
    say "Granted plugin permissions in $PERMS"
else
    say "Plugin permissions already present in $PERMS"
fi

# --- 3. shell helpers -----------------------------------------------------
add_source_line() {  # $1 = rc file, $2 = helper file
    local rc="$1" helper="$2" line="source \"$helper\""
    [ -f "$rc" ] || return 1
    if ! grep -qF "$helper" "$rc"; then
        printf '\n# zellij-claude-restore\n%s\n' "$line" >> "$rc"
        say "Added snap helpers to $rc"
    else
        say "snap helpers already sourced in $rc"
    fi
}

# fish: drop into conf.d (auto-sourced)
if [ -d "$HOME/.config/fish" ] || command -v fish >/dev/null 2>&1; then
    mkdir -p "$HOME/.config/fish/conf.d"
    ln -sf "$SCRIPT_DIR/shell/snap.fish" "$HOME/.config/fish/conf.d/zellij-claude-restore.fish"
    say "Linked fish helpers into ~/.config/fish/conf.d/"
fi
add_source_line "$HOME/.bashrc" "$SCRIPT_DIR/shell/snap.bash" || true
add_source_line "$HOME/.zshrc"  "$SCRIPT_DIR/shell/snap.zsh"  || true

# --- 4. SessionStart hook (file copy only — settings.json stays manual) ---
if [ -f "$HOOK_SRC" ]; then
    install -D -m 0755 "$HOOK_SRC" "$HOOK_DST"
    say "Installed SessionStart hook at $HOOK_DST"
else
    warn "hook source not found at $HOOK_SRC — skipping hook copy"
fi

# --- 5. manual steps ------------------------------------------------------
cat <<EOF

────────────────────────────────────────────────────────────────────────
✓ Install done. TWO manual steps remain:

1) Register the SessionStart hook in ~/.claude/settings.json (merge into any
   existing "hooks", do NOT clobber). Add under .hooks.SessionStart:

   {
     "hooks": {
       "SessionStart": [
         { "hooks": [ { "type": "command",
             "command": "python3 $HOOK_DST" } ] }
       ]
     }
   }

   Then start a fresh \`claude\` somewhere and confirm a marker appears:
     ls /tmp/zellij-\$(id -u)/claude-sessions/

2) Open a NEW shell (or \`source\` your rc) so \`snap\` is available.

Usage:
   snap <name>        # save current Zellij+Claude layout as <name>
   snap-list          # list snapshots
   zellij --layout <name>   # restore (claude panes resume via --resume)
────────────────────────────────────────────────────────────────────────
EOF
