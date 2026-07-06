# Architecture

## Style
Small two-module event-driven WASM plugin. No layered/DDD architecture — `src/main.rs` implements the
`zellij-tile` `ZellijPlugin` trait and owns the host (WASI) I/O, while the pure KDL-enrichment logic
lives in a sibling `src/enrich.rs` module (`mod enrich;`) with **no `zellij-tile` dependency** so it
compiles and unit-tests on the native host target. They are supported by an out-of-process Python hook
(the data producer) and shell helpers (the trigger). The design is deliberately small and
self-contained; complexity lives in the KDL tree walk, not in layering.

## Layers & Boundaries
Process boundaries, not code layers:
- **Producer** — `hooks/session-marker.py` runs in Claude Code's `SessionStart` host process; writes
  cwd-keyed marker files. Knows nothing about Zellij internals.
- **Consumer** — the WASM plugin runs inside Zellij's WASI sandbox; reads markers, dumps/enriches/
  saves layout KDL. Knows nothing about how markers got there.
- **Trigger** — `shell/snap.*` runs in the user's shell; fires `zellij pipe` and verifies by file
  presence.
Boundary contract = the marker file path + format and the snapshot file path. The plugin's save flow
inside `pipe()` is strictly synchronous: `dump_session_layout()` returns the KDL as a value (it blocks
on `host_run_plugin_command`), then `enrich_claude_panes()`, then `save_layout(name, kdl, true)`.

In-process **purity seam** (added by the add-unit-tests change): the pure tree-walk/serialization in
`src/enrich.rs` is decoupled from host I/O via an injected **marker resolver** — a borrowed closure
`&dyn Fn(&str) -> Option<String>` (`enrich::SessionResolver`) threaded through
`enrich_claude_panes` → `enrich_nodes` → `maybe_enrich_pane`. Production (`main.rs`) passes
`&|cwd| resolve_session_uuid(cwd)` (the `std::fs` marker read); tests pass an inline stub. This is what
lets `enrich.rs` drop the `zellij-tile` dep and run under `cargo test`. The host-bound items in
`main.rs` are `#[cfg(not(test))]`-gated.

## Key Patterns
- **Synchronous request/response to the host** — dump and save are blocking calls whose return value
  IS the result. (Anti-pattern to avoid below.)
- **Recursive KDL tree walk** — `enrich_nodes()` descends the layout, carrying an inherited base
  `cwd` so relative pane cwds resolve to absolute (`resolve_cwd`).
- **Injected resolver (purity seam)** — host I/O (marker lookup) enters the pure module as a borrowed
  closure parameter, so `src/enrich.rs` is `zellij-tile`-free and host-testable.
- **Idempotent enrichment** — `pane_has_session_id()` skips panes already pinned with
  `--resume`/`--session-id`, so re-running `snap` doesn't double-inject.
- **Graceful degradation** — parse failure saves raw KDL; missing marker leaves the pane bare.
- **Self-neutralization** — `neutralize_snap_pane()` detects the pane that ran the `snap` command and
  strips its `command`/`args` (plus the `start_suspended`/`close_on_exit` child nodes) so restore
  doesn't re-run `snap` and hang.
- **Stray stdio-pane neutralization** — `neutralize_stdio_pane()` catches a different case: Zellij
  derives a dumped pane's `command` from its *current foreground process*, so a pane whose real job is
  an interactive shell can get captured running a transient MCP/LSP subprocess an agent/editor spawned
  in it (e.g. `npm exec figma-developer-mcp --stdio`). Detected by the literal `--stdio` arg and
  stripped via the same shared `strip_command_pane()` helper, since such processes can never be
  meaningfully resumed from a bare terminal replay.

## Transaction & Consistency
No DB transactions. Consistency is filesystem-level:
- Marker writes are **atomic** (`os.replace` of a temp file) so the plugin never reads a half-written
  UUID.
- `save_layout(..., overwrite=true)` overwrites the snapshot in one host call.
- Known race (HANDOFF Risk 3): two claude panes sharing one cwd collide on a single marker — only one
  resumes cleanly. Documented limitation, not yet solved.

## Directory Map
- `src/main.rs` — host (WASI) layer: `ZellijPlugin` trait impl (`load`/`pipe`/`render`), the
  `register_plugin!` entry, and `resolve_session_uuid` (marker file read). All `#[cfg(not(test))]`-gated
  except the resolver; delegates the KDL work to `enrich`.
- `src/enrich.rs` — pure KDL-enrichment module (no `zellij-tile` dep): `enrich_claude_panes`,
  `enrich_nodes`, `neutralize_snap_pane`, `neutralize_stdio_pane`, `strip_command_pane`,
  `maybe_enrich_pane`, `resolve_cwd`, `pane_has_session_id`,
  `inject_session_id`, `basename`, `is_template_node`, the `SessionResolver` type alias, and an inline
  `#[cfg(test)] mod tests` (47 unit tests run via `cargo test`).
- `hooks/session-marker.py` — Claude Code `SessionStart` hook (marker writer).
- `shell/snap.fish` · `snap.bash` · `snap.zsh` — user command helpers (`snap`/`snap-list`/`snap-load`).
- `layouts/default.kdl.example` — optional resident-plugin layout (1-row borderless strip).
- `install.sh` — hybrid build-or-download installer.
- `.github/workflows/ci.yml` — build + `fmt --check` + `clippy -D warnings` + `cargo test` on push/PR.
- `.github/workflows/release.yml` — build + attach `.wasm` to a GitHub Release on `v*` tags.
- `Cargo.toml` / `Cargo.lock` — binary crate manifest (NOT cdylib).
- `HANDOFF.md` (authoritative design, Vietnamese), `CLAUDE.md` (agent build/runtime notes), `README.md`.

## Anti-patterns (do NOT do)
- ❌ **Async `CustomMessage` save flow** — an earlier version stored `pending_snapshot_name` and waited
  for an `Event::CustomMessage("session_layout", …)` that never fires in zellij-tile 0.44; it compiled
  (dropped `Result`s = the two unused-must-use warnings) but silently saved nothing. Keep it synchronous.
- ❌ **`[lib] crate-type=["cdylib"]`** — produces a reactor module with no `_start`; Zellij rejects it.
- ❌ **Default `KdlDocument::parse()` (v2)** — fails on Zellij's v1 dump. Use `parse_v1` + `ensure_v1`.
- ❌ **`claude --session-id <uuid>` to resume** — only assigns an ID to a new session and errors on an
  existing UUID. Use `--resume`.
- ❌ **Enriching template subtrees** — never pin `*_template` / `swap_*_layout` panes to an old session.
- ❌ **Stripping `start_suspended` from real command panes** — it's Zellij's default for dumped command
  panes; only the neutralized snap pane drops it.
- ❌ **Bumping `zellij-tile` without re-testing** against the matching zellij binary (ABI breaks pre-1.0).
- ❌ **Re-coupling `enrich.rs` to `zellij-tile` / host I/O** — keep the marker lookup an injected
  closure so the pure module stays host-testable under `cargo test`.
