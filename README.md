# zellij-claude-restore

[![CI](https://github.com/ThaiG2Pro/zellij-claude-restore/actions/workflows/ci.yml/badge.svg)](https://github.com/ThaiG2Pro/zellij-claude-restore/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
![Zellij 0.44.2](https://img.shields.io/badge/Zellij-0.44.2-blue)

Save a named **Zellij workspace snapshot** that captures the layout **plus the Claude Code
chat session running in each pane** — so `zellij --layout <name>` after a reboot brings back
the exact panes *and* resumes the exact Claude conversations.

```console
$ snap my-workspace
✓ saved snapshot: my-workspace
  2 claude pane(s) will resume · 0 already pinned · 0 without a marker

# ... reboot ...

$ zellij --layout my-workspace     # panes restored; both claude chats resume automatically
```

> A short recorded demo can be produced with [`docs/record-demo.sh`](docs/record-demo.sh).

## The problem it solves

Zellij's resurrection replays each pane's command from `/proc/<pid>/cmdline`. A bare `claude`
(no session flag in its argv) therefore starts a **brand-new chat** on restore — your previous
conversation is lost. argv is immutable after `execve()`, so the running session ID can't be
recovered from the process afterward.

`zellij-claude-restore` fixes this by enriching the saved layout: for every pane running `claude`
it injects `args "--resume" "<session-uuid>"`, looked up from a per-cwd marker that a Claude
`SessionStart` hook writes. On restore each pane runs `claude --resume <uuid>` and re-opens the
right conversation.

## How it works

```
 Claude SessionStart hook            zellij-claude-restore.wasm (on `snap`)
 ──────────────────────              ───────────────────────────────────
 writes /tmp/zellij-<uid>/           dump layout KDL ─► for each `claude` pane,
 claude-sessions/<cwd>.session  ──►  look up its cwd's marker ─► inject
   = the live session UUID           args "--resume" "<uuid>" ─► save snapshot
```

- **`snap <name>`** triggers the plugin over `zellij pipe`, which dumps the current layout,
  enriches every `claude` pane, and writes `~/.config/zellij/layouts/<name>.kdl`.
- **`zellij --layout <name>`** restores it; each claude pane resumes its conversation.

## Requirements

- **Zellij `0.44.x`** — plugin ABI is version-pinned (`zellij-tile = "=0.44.2"`). A prebuilt
  `.wasm` only works on a matching zellij; otherwise build from source against your version.
- **Claude Code** with `SessionStart` hooks.
- `python3` (for the hook), and `rustup` + `wasm32-wasip1` target *if* building from source.

## Install

```bash
git clone https://github.com/ThaiG2Pro/zellij-claude-restore.git zellij-claude-restore
cd zellij-claude-restore
./install.sh
```

`install.sh` is **hybrid**: it builds from source when `cargo` + the `wasm32-wasip1` target are
available, otherwise downloads a prebuilt release. Force either path:

```bash
ZCS_MODE=build ./install.sh                  # always compile locally
ZCS_REPO=user/zellij-claude-restore ZCS_MODE=download ./install.sh   # fetch a release .wasm
```

It will:
1. Install the plugin to `~/.config/zellij/plugins/zellij-claude-restore.wasm`
2. Pre-grant the plugin's permissions in `~/.cache/zellij/permissions.kdl`
3. Source the `snap` helpers for fish / bash / zsh
4. Copy the SessionStart hook to `~/.claude/hooks/`

### Two manual steps (by design)

**1. Register the hook in `~/.claude/settings.json`** — the installer never edits your Claude
settings for you. Merge this into `.hooks.SessionStart` (keep any existing hooks):

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command",
          "command": "python3 /home/<you>/.claude/hooks/zellij-claude-session-marker.py" } ] }
    ]
  }
}
```

Verify: start a fresh `claude` somewhere, then check
`ls /tmp/zellij-$(id -u)/claude-sessions/` for a marker file.

**2. Open a new shell** (or re-`source` your rc) so `snap` is on your PATH.

## Usage

```bash
snap my-workspace              # save the current Zellij + Claude layout
snap --manual my-workspace     # ...but restore panes suspended (wait for ENTER)
snap-list                      # list snapshots — name · date · resumable panes
snap-rm my-workspace           # delete a snapshot (accepts multiple names)
snap-clean                     # delete ALL snapshots (prompts; -f to skip)
zellij --layout my-workspace   # restore — claude panes resume via --resume
```

`snap` prints what it captured, e.g.:

```
✓ saved snapshot: my-workspace
  2 claude pane(s) will resume · 0 already pinned · 0 without a marker
```

**Auto-enter (default).** On restore, panes running `claude` that `snap` could pin
resume **automatically** — no keypress. Only recognized-and-pinned claude panes
auto-launch; every other command pane keeps Zellij's safe suspended default
(`Press ENTER to run: …`), so restore never auto-runs an arbitrary command. Prefer
the old behavior? Use `snap --manual <name>` (or set `ZCS_NO_AUTO_ENTER=1`) and each
claude pane waits for ENTER too.

**One-key snapshot.** Bind a key to snapshot without typing a name — see
[`layouts/keybind.kdl.example`](layouts/keybind.kdl.example) (uses Zellij's
`MessagePlugin` keybind action; restore with `zellij --layout quicksnap`).

**Renamed / symlinked claude binary?** By default only a command whose basename is
`claude` is enriched. If yours is e.g. `claude-code`, set `ZCS_CLAUDE_CMD=claude-code`
(or, for the resident-plugin layout, `claude_command "claude-code"` in the plugin
block — see [`layouts/default.kdl.example`](layouts/default.kdl.example)). A wrapper
that runs claude as an *argument* (`npx claude`) is not detected.

## Notes & limitations

- **One claude pane per cwd.** Markers are keyed by working directory, so two `claude` panes in
  the *same* cwd share one UUID — only one resumes cleanly. Use distinct cwds.
- **`--resume`, not `--session-id`.** `claude --session-id <uuid>` only *assigns* an ID to a new
  session and errors if the UUID already exists; `--resume` is the flag that re-opens it.
- **Redeploying the plugin?** Zellij caches the compiled `.wasm` per session and won't pick up a
  rebuild until you purge the cache and start a fresh session — see `CLAUDE.md`.
- `snap` from a *separate* pane than your claude panes; the snap pane is auto-neutralized to a
  plain shell on restore.

## Building manually

```bash
cargo build --release --target wasm32-wasip1
# artifact: target/wasm32-wasip1/release/zellij-claude-restore.wasm
cp target/wasm32-wasip1/release/zellij-claude-restore.wasm ~/.config/zellij/plugins/
```

This is a **binary** crate (not a cdylib) — Zellij's loader needs the WASM `_start` export that
only a binary target emits. See `CLAUDE.md` for the full build/architecture notes and `HANDOFF.md`
(Vietnamese) for the design rationale.
