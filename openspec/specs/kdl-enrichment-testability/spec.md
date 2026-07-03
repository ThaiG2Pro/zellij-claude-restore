# kdl-enrichment-testability Specification

## Purpose
TBD - created by archiving change add-unit-tests. Update Purpose after archive.
## Requirements
### Requirement: Pure KDL-enrichment module extracted without zellij-tile

The system SHALL relocate the KDL-enrichment functions (`enrich_claude_panes`, `enrich_nodes`,
`neutralize_snap_pane`, `resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`,
`is_template_node`) into a dedicated Rust module (`src/enrich.rs`) that has **no dependency on
`zellij-tile`**, so the module compiles and is testable on the host (native) target via `cargo test`.
The crate SHALL remain a **binary crate** (it MUST keep emitting the WASM `_start` export via
`register_plugin!`) and MUST NOT be converted to a `cdylib`. KDL parsing SHALL remain **v1**
(`parse_v1` / `ensure_v1`); the default v2 `parse()` MUST NOT be introduced. The synchronous save flow
in `pipe()` (`dump_session_layout()` → enrichment → `save_layout()`) MUST be preserved — no async
`CustomMessage` pattern. (AC-tests-001 [CONFIRMED], AC-tests-002 [CONFIRMED], AC-tests-003 [CONFIRMED],
AC-tests-004 [CONFIRMED])

#### Scenario: Host-target test build succeeds (AC-tests-001 happy)
- **WHEN** `cargo test` is run on the host (native) target after the extraction
- **THEN** the pure enrichment module compiles and its tests run **without** pulling in `zellij-tile`
  and without requiring the `wasm32-wasip1` target

#### Scenario: WASM plugin still builds and loads (AC-tests-002 happy)
- **WHEN** `cargo build --release --target wasm32-wasip1` is run after the extraction
- **THEN** it produces `target/wasm32-wasip1/release/zellij-claude-sync.wasm` (binary crate, `_start`
  export present) and the plugin still loads in Zellij 0.44.2

#### Scenario: Enrichment entry point relocated and re-wired (AC-tests-003 happy)
- **WHEN** the extraction is complete
- **THEN** `pipe()` calls `enrich_claude_panes` from the new module via `mod enrich;` and the public
  behavior of the save flow is unchanged

#### Scenario: Conversion to cdylib is rejected (AC-tests-004 error)
- **WHEN** a change attempts to satisfy testability by switching `Cargo.toml` to `[lib] crate-type=["cdylib"]`
- **THEN** that approach SHALL be rejected because a cdylib emits no `_start` export and Zellij's loader
  fails with `could not find exported function`; the crate stays a binary crate

#### Scenario: Default v2 KDL parser is rejected (AC-tests-005 error)
- **WHEN** a change attempts to parse the layout with the default `KdlDocument::parse()` (v2)
- **THEN** that approach SHALL be rejected because v2 fails on Zellij's v1 dump; `parse_v1`/`ensure_v1`
  are used (AC-tests-005 [CONFIRMED])

#### Scenario: --session-id used to resume is rejected (AC-tests-006 error)
- **WHEN** a change attempts to inject `--session-id` to resume a session
- **THEN** that approach SHALL be rejected because `--session-id` only assigns an id to a NEW session and
  errors on an existing UUID; injection uses `--resume` (AC-tests-006 [CONFIRMED])

### Requirement: Behavior parity — extracted module produces identical KDL output

The extracted `enrich_claude_panes` SHALL produce **byte-identical** output to the pre-extraction
`src/main.rs` behavior for any given input KDL string, with the session-marker lookup held constant via
an injected/stubbed resolver. The graceful-degradation contract MUST be preserved: on KDL parse failure
the raw input is returned unchanged; a claude pane whose cwd cannot be resolved, or whose marker is
absent, is left bare. (AC-tests-007 [CONFIRMED], AC-tests-008 [CONFIRMED], AC-tests-009 [ASSUMED])

#### Scenario: Identical output on a representative claude-pane layout (AC-tests-007 happy)
- **WHEN** a fixture layout containing an enrichable `claude` pane (resolver returns a known UUID) is run
  through the extracted `enrich_claude_panes`
- **THEN** the output KDL contains `command="claude"` with `args "--resume" "<uuid>"` and is byte-identical
  to the documented pre-extraction output for that input

#### Scenario: Parse failure returns raw input unchanged (AC-tests-008 error)
- **WHEN** an input string that `parse_v1` cannot parse is passed to `enrich_claude_panes`
- **THEN** the function returns the original input string unchanged and does not panic

#### Scenario: Empty / whitespace-only input does not panic (AC-tests-009 error)
- **WHEN** an empty or whitespace-only string is passed to `enrich_claude_panes`
- **THEN** the function returns without panicking, preserving parity with current behavior

### Requirement: Snap-pane neutralization strips command, args, and lifecycle child nodes

`neutralize_snap_pane` SHALL detect the pane that ran the `snap`/`zellij pipe … --name save` command
(command basename `zellij` or `timeout`, with `args` containing `save` plus `pipe` or
`zellij-claude-sync`) and strip its `command` entry, its `args` child node, and — critically — the
`start_suspended` and `close_on_exit` CHILD nodes, so the pane restores as a plain shell. A pane that
is not the snap pane SHALL be left untouched. (AC-tests-010 [CONFIRMED], AC-tests-011 [CONFIRMED],
AC-tests-012 [CONFIRMED])

#### Scenario: Snap pane has start_suspended/close_on_exit children stripped (AC-tests-010 happy)
- **WHEN** a fixture pane with `command="timeout"`, an `args` block containing `save`+`pipe`, and
  `start_suspended true` / `close_on_exit` child nodes is neutralized
- **THEN** the resulting pane has no `command`, no `args`, no `start_suspended`, and no `close_on_exit`
  node — so `zellij --layout` does NOT error with `start_suspended can only be set if a command was specified`

#### Scenario: Snap pane detected via zellij basename (AC-tests-011 happy)
- **WHEN** the snap pane's command basename is `zellij` (not `timeout`) with the matching save args
- **THEN** the pane is neutralized identically

#### Scenario: A real command pane is not neutralized (AC-tests-012 error)
- **WHEN** a non-snap command pane (e.g. `command="claude"`, or `command="zellij"` without `save` args)
  is checked
- **THEN** `neutralize_snap_pane` returns false and leaves the pane's `command`/`args`/`start_suspended`
  intact

### Requirement: Idempotent enrichment — no double-injection

`pane_has_session_id` SHALL treat a pane already carrying an `args` entry of `--resume` OR `--session-id`
as "already pinned", and `maybe_enrich_pane` SHALL skip it so re-running `snap` never double-injects.
(AC-tests-013 [CONFIRMED], AC-tests-014 [CONFIRMED], AC-tests-015 [CONFIRMED])

#### Scenario: Pane with existing --resume is not double-injected (AC-tests-013 happy)
- **WHEN** a claude pane already has `args "--resume" "<uuid>"`
- **THEN** enrichment leaves it unchanged (no second `--resume` prepended)

#### Scenario: Pane with existing --session-id is left pinned (AC-tests-014 happy)
- **WHEN** a claude pane already has `args "--session-id" "<uuid>"`
- **THEN** enrichment treats it as pinned and injects nothing

#### Scenario: Running enrichment twice is stable (AC-tests-015 error/idempotency)
- **WHEN** `enrich_claude_panes` is applied to its own output a second time
- **THEN** the output is unchanged from the first pass (idempotent)

### Requirement: Template subtrees are skipped during enrichment

`enrich_nodes` SHALL NOT enrich panes inside template subtrees — `new_tab_template`, `tab_template`,
`swap_tiled_layout`, `swap_floating_layout` (via `is_template_node`) — because those describe what to
spawn for a brand-new tab and pinning them to an old session would be wrong. (AC-tests-016 [CONFIRMED],
AC-tests-017 [CONFIRMED])

#### Scenario: Claude pane inside a template is not enriched (AC-tests-016 happy)
- **WHEN** a `claude` pane sits inside a `new_tab_template` (or any of the four template nodes)
- **THEN** it is NOT given `args "--resume" …`

#### Scenario: Same pane outside a template is enriched (AC-tests-017 error/contrast)
- **WHEN** the identical `claude` pane sits inside a normal `tab` (resolver returns a UUID)
- **THEN** it IS enriched — confirming the template skip, not a global no-op

### Requirement: cwd resolution joins relative pane cwd onto the layout base

`resolve_cwd` SHALL return absolute pane `cwd` values unchanged, join relative pane `cwd` values onto the
inherited base cwd (trimming a single trailing slash on the base), inherit the base directly when the
pane has no `cwd`, and return `None` when neither a pane cwd nor a base is available. (AC-tests-018
[CONFIRMED], AC-tests-019 [CONFIRMED], AC-tests-020 [CONFIRMED], AC-tests-021 [CONFIRMED])

#### Scenario: Relative pane cwd joined onto base (AC-tests-018 happy)
- **WHEN** pane `cwd="api"` and base `cwd "/home/u"`
- **THEN** `resolve_cwd` returns `/home/u/api`

#### Scenario: Absolute pane cwd passes through (AC-tests-019 happy)
- **WHEN** pane `cwd="/srv/x"` with any base
- **THEN** `resolve_cwd` returns `/srv/x` unchanged

#### Scenario: No pane cwd inherits base (AC-tests-020 happy)
- **WHEN** the pane has no `cwd` and base is `/home/u`
- **THEN** `resolve_cwd` returns `/home/u`

#### Scenario: No cwd and no base yields None → pane left bare (AC-tests-021 error)
- **WHEN** the pane has no `cwd` and there is no inherited base
- **THEN** `resolve_cwd` returns `None` and the claude pane is left bare (no panic)

### Requirement: Session-id injection prepends --resume preserving existing args

`inject_session_id` SHALL prepend `--resume <uuid>` to an existing `args` block (preserving any trailing
positional args such as a prompt) and SHALL create an `args` block when none exists. `basename` SHALL
match the command name from a full path (`/usr/bin/claude` → `claude`) so path-qualified commands enrich.
(AC-tests-022 [CONFIRMED], AC-tests-023 [CONFIRMED], AC-tests-024 [CONFIRMED])

#### Scenario: Inject into pane with existing args preserves them (AC-tests-022 happy)
- **WHEN** a claude pane has `args "my-prompt"` and a UUID is injected
- **THEN** the result is `args "--resume" "<uuid>" "my-prompt"`

#### Scenario: Inject into pane with no args creates the block (AC-tests-023 happy)
- **WHEN** a claude pane has no `args` block and a UUID is injected
- **THEN** an `args "--resume" "<uuid>"` block is created

#### Scenario: Path-qualified and non-claude commands (AC-tests-024 error)
- **WHEN** a pane has `command="/usr/bin/claude"` (matches) vs `command="vim"` (does not)
- **THEN** the first is enriched and the second is left untouched

### Requirement: CI workflow runs build, fmt, clippy, and test on push and PR

The system SHALL add a CI workflow (separate from `release.yml`, which stays release-on-tag) triggered on
push and pull_request that runs, and fails the job on any failure: `cargo build --release --target
wasm32-wasip1`, `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`. `release.yml` SHALL
remain unchanged. (AC-tests-025 [CONFIRMED], AC-tests-026 [CONFIRMED], AC-tests-027 [ASSUMED],
AC-tests-028 [ASSUMED])

#### Scenario: CI passes on a clean push/PR (AC-tests-025 happy)
- **WHEN** a push or pull_request triggers the workflow on healthy code
- **THEN** all four steps (build, fmt --check, clippy -D warnings, test) succeed and the job is green

#### Scenario: wasm32-wasip1 target installed for the build step (AC-tests-026 happy)
- **WHEN** the workflow runs the release build step
- **THEN** the `wasm32-wasip1` target is installed first so the build step succeeds on a fresh runner

#### Scenario: Failing test fails the job (AC-tests-027 error)
- **WHEN** a unit test fails (e.g. a regression reappears)
- **THEN** `cargo test` exits non-zero and the CI job fails (red), blocking merge

#### Scenario: Lint/format violation fails the job (AC-tests-028 error)
- **WHEN** code is not rustfmt-clean or clippy emits any warning
- **THEN** `cargo fmt --check` / `cargo clippy -- -D warnings` exits non-zero and the CI job fails

