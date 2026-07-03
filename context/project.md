# Project Context

## Identity
- **Name**: zellij-claude-restore
- **Slug**: zellij-claude-restore
- **One-liner**: A Zellij plugin (Rust → WASM) that saves a named workspace snapshot capturing the layout plus the Claude Code chat session running in each pane, so `zellij --layout <name>` resumes the exact layout and re-opens the exact Claude conversations after a reboot.

## Domain
Developer-tooling / terminal-multiplexer automation. The plugin solves a specific resurrection gap:
Zellij replays each pane's command from `/proc/<pid>/cmdline`, so a bare `claude` (no session flag in
argv) starts a brand-new chat on restore — the previous conversation is lost, and because argv is
immutable after `execve()` the running session ID can't be recovered from the process. The fix
enriches the saved layout KDL: for every `claude` pane it injects `args "--resume" "<uuid>"`, looked
up from a per-cwd marker that a Claude Code `SessionStart` hook writes. On restore each pane runs
`claude --resume <uuid>` and re-opens the right conversation. Users are developers who run Claude Code
inside Zellij and want layouts to survive reboots.

## Modules / Bounded Contexts
| Module | Responsibility |
|--------|----------------|
| WASM plugin (`src/main.rs`) | Single-file `ZellijPlugin`. Synchronous save flow in `pipe()`: dump layout KDL → `enrich_claude_panes()` → `save_layout()`. Owns KDL parse/enrich, snap-pane neutralization, cwd resolution, session-UUID lookup. |
| SessionStart hook (`hooks/session-marker.py`) | Claude Code hook that writes the cwd-keyed session marker the plugin reads. Produces the data; the plugin consumes it. |
| Shell helpers (`shell/snap.{fish,bash,zsh}`) | `snap` / `snap-list` / `snap-load` user commands that trigger the plugin over `zellij pipe` and confirm by snapshot-file presence. |
| Installer / packaging (`install.sh`, `.github/workflows/release.yml`, `layouts/default.kdl.example`) | Hybrid build-or-download install; CI release of the prebuilt `.wasm`. |

## Primary Interfaces / Endpoints
- No network API. The plugin's only entry point is the Zellij pipe interface:
  `zellij pipe --plugin file:<abs-wasm> --name save -- <snapshot-name>` → triggers `pipe()`.
- User CLI surface (shell helpers): `snap <name>`, `snap-list`, `snap-load <name>`; restore with
  `zellij --layout <name>`.
- Filesystem contract: reads markers at `/tmp/claude-sessions/<encoded-cwd>.session` (guest path;
  host `/tmp/zellij-<uid>/claude-sessions/…`); writes snapshots to `~/.config/zellij/layouts/<name>.kdl`.

## External Dependencies
- **Zellij 0.44.2** — host multiplexer; plugin ABI is version-pinned (`zellij-tile = "=0.44.2"`).
- **Claude Code** — provides the `SessionStart` hook and the `--resume <uuid>` resume flag; owns
  session storage at `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`.
- **Rust toolchain + `wasm32-wasip1` target** — to build from source.
- **`python3`** — runs the SessionStart hook.
- Rust crates: `zellij-tile` (pinned), `kdl` v6 (v1 feature), `serde`, `serde_json`.

## Principles / Non-negotiables
- **Explicit named snapshots only.** The save is user-triggered (`snap <name>`), never auto-tick /
  race-write — chosen deliberately over four rejected alternatives (see HANDOFF.md).
- **Binary crate, not cdylib.** Only a binary target emits the WASM `_start` export Zellij's loader
  requires; `register_plugin!` generates `fn main()` for this.
- **Synchronous save flow.** Do NOT reintroduce the async `CustomMessage` pattern — in zellij-tile
  0.44 the dump result is the function return value, not a later event.
- **`--resume`, NOT `--session-id`.** `--session-id` only assigns an ID to a new session and errors on
  an existing UUID; `--resume` re-opens it.
- **Graceful degradation.** On KDL parse failure the raw layout is saved unchanged. The hook always
  exits 0 so it can never disrupt Claude.
- **Never edit the user's `~/.claude/settings.json` automatically** — the installer prints the snippet;
  the human registers the hook.
- **HANDOFF.md (Vietnamese) is the authoritative design document.** Read it before non-trivial
  architecture changes; its decisions (D1–D8) should not be re-litigated by accident.
