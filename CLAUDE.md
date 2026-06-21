# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Zellij plugin (Rust → WASM) that lets a developer **explicitly save a named workspace snapshot** capturing the Zellij layout **plus the Claude Code chat session ID running in each pane**, so that `zellij --layout <name>` after a reboot resumes the exact layout *and* the exact Claude sessions.

The core problem: Zellij resurrection replays each pane's `command` from `/proc/<pid>/cmdline`, but `claude` launched bare (no `--session-id` in argv) spawns a *new* chat. argv is immutable after `execve()`, so the running session ID can't be recovered from the process. The fix enriches the saved layout KDL with `args "--session-id" "<uuid>"` per claude pane.

**`HANDOFF.md` is the authoritative design document** (research findings, rejected alternatives, decisions D1–D8, open questions Q1–Q6, phased plan). It is in Vietnamese. Read it before making non-trivial architecture changes — the chosen approach (explicit named snapshots via the `SaveLayout` API, not auto-tick race-writes) was deliberately picked over four rejected alternatives, and that reasoning should not be re-litigated by accident.

## Build

```bash
cargo build --release --target wasm32-wasip1
# artifact: target/wasm32-wasip1/release/zellij-claude-sync.wasm   (hyphen — it's a BINARY crate)
```

- **This is a binary crate, NOT a cdylib.** zellij's plugin loader requires the WASM `_start` export (`plugin_loader.rs:176`), which only a binary target emits. `register_plugin!` generates `fn main()` for exactly this reason. A `[lib] crate-type=["cdylib"]` builds a reactor module with no `_start` and zellij rejects it with `could not find exported function`. (Build target was `wasm32-wasip1` on stable Rust 1.95 — toolchain version does NOT matter; crate type does.)
- The artifact basename follows the package name: `zellij-claude-sync.wasm` (hyphen). The old cdylib build produced `zellij_claude_sync.wasm` (underscore) — that name is stale, don't reference it.
- `zellij-tile` is pinned `=0.44.2` to match the zellij binary exactly (mise-managed `0.44.2`); a caret range resolves to a newer SDK and can skew the plugin ABI.
- The `wasm32-wasip1` target must be installed (`rustup target add wasm32-wasip1`).
- There are no automated tests. Manual verification: build → `zellij pipe --plugin file:<abs-wasm> --name save -- <snapshot>` → inspect `~/.config/zellij/layouts/<snapshot>.kdl` → `zellij --layout <snapshot>`. Headless verification works via a `script`-provided PTY: `script -qfec "zellij -s <name> -n <layout.kdl>" /dev/null &` (use `-n`/`--new-session-with-layout`; plain `-l` may route to attach and fail with "There is no active session"). To exercise the plugin without an interactive permission prompt, pre-grant in `~/.cache/zellij/permissions.kdl` keyed by the plugin's **bare absolute path** (no `file:` prefix — `RunPluginLocation::File` Display is just the path):
  ```kdl
  "/abs/path/to/zellij-claude-sync.wasm" {
      ReadApplicationState
      ChangeApplicationState
  }
  ```

## Runtime / install (how the plugin is exercised)

The plugin runs as a hidden background pane inside a Zellij session and is triggered over the pipe interface, not a keybinding:

```bash
zellij pipe --plugin file:~/.config/zellij/plugins/zellij-claude-sync.wasm --name save -- <snapshot-name>
```

Pinned to **Zellij 0.44.2** (`zellij-tile = "=0.44.2"`). Zellij is pre-1.0 and the plugin API breaks between versions — do not bump `zellij-tile` without re-testing against the matching `zellij` binary.

### Shell helpers (`shell/snap.fish`)

`snap <name>` / `snap-list` / `snap-load <name>` (fish, install into `~/.config/fish/conf.d/`). `snap` launches the plugin on demand via `zellij pipe --plugin file:$ZCS_PLUGIN …`, so the plugin need not be auto-started. **The `zellij pipe` call hangs** (the plugin can't release the blocked CLI — `unblock_cli_pipe_input` only frees the input side, not the CLI's wait for output), so `snap` wraps it in `timeout 3` and confirms success by the presence of the snapshot file, not the exit code. `layouts/default.kdl.example` shows the optional resident-plugin layout (zellij has no true hidden pane; it's a 1-row borderless strip).

## Architecture of the plugin (`src/main.rs`)

Single file implementing the `ZellijPlugin` trait. The save flow is **synchronous**, entirely inside `pipe()`:

1. `load()` — requests `ReadApplicationState` (for dump) + `ChangeApplicationState` (for save). No event subscription is needed.
2. `pipe()` — on `name == "save"`: calls `dump_session_layout()`, which **returns the layout KDL synchronously** as `Result<(String, Option<LayoutMetadata>), String>` (it blocks on `host_run_plugin_command`), runs it through `enrich_claude_panes()`, then calls `save_layout(name, enriched, overwrite=true)` (also synchronous, `Result<(), String>`). The snapshot file lands at `~/.config/zellij/layouts/<name>.kdl`.

> **Do not reintroduce the async/`CustomMessage` pattern.** An earlier version stored a `pending_snapshot_name` and waited for an `Event::CustomMessage("session_layout", …)` that never fires — in zellij-tile 0.44 the dump result is the function's return value, not a later event. That version compiled (the dropped `Result`s were the two "unused must-use" warnings) but silently never saved anything.

### KDL enrichment — implemented and verified

`enrich_claude_panes()` is the core feature and is **working end-to-end** (headless test: two bare `claude` panes in distinct cwds → `pipe save` → restore spawns each with its correct `claude --session-id <uuid>`). The flow inside `pipe()` is dump → `enrich_claude_panes(&kdl)` → `save_layout`.

How enrichment works (all in `src/main.rs`, deterministic and self-contained):
- **Parse/serialize as KDL v1.** zellij dumps KDL **v1** syntax (it uses the kdl v4 crate). We depend on `kdl = { version = "6", features = ["v1"] }` and use `KdlDocument::parse_v1()` + `ensure_v1()`. The default `parse()` uses the v2 parser and **fails** on zellij's dump ("Failed to parse KDL document") — do not switch to it. On any parse failure the raw KDL is saved unchanged (graceful degradation).
- **Match** `pane` nodes whose `command` **basename** is `claude` (so `/usr/bin/claude` matches too).
- **Skip template subtrees** — `new_tab_template`, `tab_template`, `swap_tiled_layout`, `swap_floating_layout`. Those describe what to spawn for a *brand-new* tab; pinning them to an old session id would be wrong. Only the live `tab` panes get enriched.
- **Resolve cwd.** A dumped pane's `cwd` is often **relative** (`cwd="api"`) against a layout-level `cwd "/home/user"` base node; `resolve_cwd()` joins them to an absolute path.
- **Inject** `args "--session-id" "<uuid>"` (prepended if an `args` block already exists; panes already carrying `--session-id` are left untouched).

### The SessionStart hook (`hooks/session-marker.py`)

The markers are produced by `hooks/session-marker.py`, a Claude Code `SessionStart` hook. It reads the hook's stdin JSON (`session_id`, `cwd` — **not** `$CLAUDE_SESSION_ID`, which is empty in tool-call shells), and writes `/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session` atomically. It exits 0 on any malformed input so it can never disrupt Claude.

Install (run as the human — modifying the global `~/.claude/settings.json` is intentionally **not** done automatically by Claude):
```bash
cp hooks/session-marker.py ~/.claude/hooks/zellij-claude-session-marker.py
# then add to ~/.claude/settings.json under .hooks.SessionStart, preserving existing hooks:
#   { "hooks": [ { "type": "command", "command": "python3 /home/<you>/.claude/hooks/zellij-claude-session-marker.py" } ] }
```
Verify after install: start a fresh `claude` somewhere and check `/tmp/zellij-$(id -u)/claude-sessions/` for the marker.

### Session-UUID resolution — cwd-keyed markers (deviates from HANDOFF §6.4)

`resolve_session_uuid(cwd)` reads `/tmp/claude-sessions/<encoded-cwd>.session` (cwd with `/`→`-`, matching Claude's own project-dir encoding). This **intentionally differs** from HANDOFF D4/D5 (PID-keyed marker + `~/.claude/projects` scan) because of WASI sandbox realities discovered during implementation:
- The plugin only gets `/host`, `/data`, `/cache`, `/tmp` preopened. **`~/.claude/projects/` is unreachable**, so the "newest-jsonl" heuristic fallback cannot run inside WASM.
- Guest `/tmp` maps to the host's `ZELLIJ_TMP_DIR` = **`/tmp/zellij-<uid>`**, *not* real `/tmp`. So the `SessionStart` hook must write markers to `/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session`.
- The dumped KDL carries **cwd but no pane_id/pid**, so PID-keyed markers can't be matched from the dump in the synchronous flow. Keying on cwd is what the dump actually provides.
- `/tmp` is preopened **without** `FullHdAccess`, so no extra permission is needed (contrary to the HANDOFF's expectation).
- Known limitation (HANDOFF Risk 3): two claude panes sharing one cwd collide on the same marker.

Claude session storage for reference: `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl`, append-only event log. Resume via `claude -r <uuid>` / `claude --session-id <uuid>`.

## Verification status (HANDOFF §9 open questions)

Resolved during Phase 0:
- ✅ **Q2 — `SaveLayout` round-trips `command + args`.** A hand-written layout with `command="…" { args "--session-id" "<uuid>" }` spawns the command with those exact args on `zellij --layout`; the plugin's dump→save preserves the `args` block. Approach is feasible.
- ✅ **Q4 — `pipe()` reaches the plugin.** `zellij pipe --name save` triggers `pipe()` and the synchronous save runs. Note: the `zellij pipe` CLI call itself blocks until killed because the plugin never closes the CLI pipe — harmless for the save, but use a `timeout` when scripting.

Still genuinely open — do not write code that silently depends on these:
- **Q1 — Whether Claude's `SessionStart` hook fires on the `/resume` UI picker** (decides UUID-marker accuracy). Needs an interactive Claude session to confirm.
- **Q5 — `CLAUDE_SESSION_ID` availability.** It is **empty** in the shell environment of Claude Code tool calls; confirm it is populated specifically inside the `SessionStart` hook's process before relying on the marker-file design.
