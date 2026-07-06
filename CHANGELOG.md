# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.2] — 2026-07-06

### Fixed
- **Stray `unnamed.kdl` on every snapshot.** A single `zellij pipe … --name save --
  <name>` delivers `pipe()` twice — once with the payload, once empty — so alongside the
  real snapshot the plugin also saved an `unnamed.kdl` (and the status file reported
  `"name":"unnamed"`). An empty/whitespace payload is now a no-op. (Found via an
  end-to-end round-trip; the pure unit tests can't see the host pipe path.)

## [0.4.1] — 2026-07-06

### Fixed
- **`install.sh` aborted partway through** (`helper: unbound variable`) on any run with
  `set -u`: a single `local rc=$1 helper=$2 line="…$helper…"` expands all RHS before
  assigning, so `$helper` was still unbound when `line` referenced it. Split the
  assignment. Fresh installs now complete the hook copy + final instructions.

## [0.4.0] — 2026-07-03

### Changed
- **Renamed the project `zellij-claude-sync` → `zellij-claude-restore`** to match the
  repository. This changes the built artifact and default install path:
  - artifact: `zellij-claude-sync.wasm` → **`zellij-claude-restore.wasm`**
  - default plugin path / `$ZCS_PLUGIN`: `~/.config/zellij/plugins/zellij-claude-restore.wasm`
  - diagnostics prefix: `[zellij-claude-restore]`

  **Upgrading from ≤0.3.0:** re-run `install.sh` (or copy the new `.wasm`), update any
  resident-plugin layout's `plugin location=…` path, re-`source` the shell helpers, and
  remove the old `~/.config/zellij/plugins/zellij-claude-sync.wasm`. Environment variables
  are unchanged (still `ZCS_*`). Snapshot files and the marker/hook contract are unchanged.

## [0.3.0] — 2026-07-03

### Added
- **Snapshot management:** `snap-rm <name>...` (delete named snapshots) and
  `snap-clean` (delete all; prompts, `-f` to skip) in all three shells.
- **Richer `snap-list`:** now shows each snapshot's date and resumable-pane count.
- **Configurable claude command:** enrich a renamed/symlinked binary via
  `ZCS_CLAUDE_CMD=claude-code` (or `claude_command "…"` in the resident-plugin
  layout). Default stays `claude`. Matched by basename; arg-wrappers (`npx claude`)
  are not detected.
- **Configurable auto-enter default** via the plugin `load()` config
  (`auto_enter "false"`); per-snapshot `--args` still overrides.
- Pure API generalized to `enrich_layout(kdl, resolve, &EnrichConfig)`
  (`EnrichConfig { auto_enter, claude_command }`); +3 unit tests (43 total).

## [0.2.0] — 2026-07-03

### Added
- **Auto-enter on restore (default).** Panes running `claude` that a snapshot could
  pin now resume automatically — no per-pane ENTER. Only recognized-and-pinned claude
  panes auto-launch; every other command pane keeps Zellij's suspended default, so
  restore never auto-runs an unrecognized command.
- **Save feedback.** `snap` reports what it captured — how many claude panes will
  resume, how many were already pinned, and how many had no session marker (with a hint
  to check the SessionStart hook). Backed by a one-line JSON status file the plugin writes.
- **`snap --manual` / `$ZCS_NO_AUTO_ENTER`** to opt out of auto-enter per snapshot.
- **One-key snapshot** via Zellij's `MessagePlugin` keybind action
  (`layouts/keybind.kdl.example`).
- New pure API `enrich_layout(kdl, resolve, auto_enter) -> (String, EnrichStats)`;
  `enrich_claude_panes` retained as a back-compatible wrapper.
- 7 new unit tests for auto-enter + stats (40 total).

### Changed
- Repo hygiene for public release: `main` now carries the full tested core
  (`src/enrich.rs` + CI on push/PR), a `ROADMAP.md`, and gitignored transient kit backups.

## [0.1.0] — 2026-06-29

### Added
- Named Zellij snapshots that enrich every `claude` pane with `--resume <uuid>`, looked
  up from a per-cwd marker written by a Claude `SessionStart` hook.
- Snap-pane neutralization, idempotent enrichment, template-subtree skipping, relative-cwd
  resolution, graceful degradation on KDL parse failure.
- `snap` / `snap-list` / `snap-load` shell helpers (fish/bash/zsh), hybrid installer, and a
  tagged-release CI workflow.
- Pure KDL-enrichment module (`src/enrich.rs`) with a 33-test regression suite.

[Unreleased]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ThaiG2Pro/zellij-claude-restore/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ThaiG2Pro/zellij-claude-restore/releases/tag/v0.1.0
