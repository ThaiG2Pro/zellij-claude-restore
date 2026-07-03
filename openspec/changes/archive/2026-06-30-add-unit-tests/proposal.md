# Proposal — add-unit-tests

> **Type**: feature · **Rigor**: lite · **Change**: `add-unit-tests` · **Branch**: `feature/add-unit-tests` (base `main`)
> **AC/BR/INT id slug**: `tests` — this repo has no ticket numbers (see `context/conventions.md`), so ids read `AC-tests-NNN`.

## Why

The KDL-enrichment logic in `src/main.rs` (`enrich_claude_panes`, `enrich_nodes`, `neutralize_snap_pane`,
`resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`, `is_template_node`) is the heart of
the plugin and has already produced **two production regressions** that were only caught by manual,
freshly-started-session testing (Jun 29): `neutralize_snap_pane` not stripping the `start_suspended`/
`close_on_exit` child nodes (restore failed with `start_suspended can only be set if a command was
specified`), and the `--session-id` → `--resume` correction. There is currently **no automated safety
net** — `context/stack.md` records "Test framework: None", verification is manual/headless-PTY, and CI
(`release.yml`) only runs on `v*` tags. The enrichment logic is **pure** (KDL string in → KDL string out,
no host calls), so it can be unit-tested on the host target the moment it is decoupled from `zellij-tile`.
We do it now to lock in the hard-won regressions before the next change touches this code.

## What Changes

- **Extract a pure KDL module** `src/enrich.rs` out of `src/main.rs`, holding `enrich_claude_panes`,
  `enrich_nodes`, `neutralize_snap_pane`, `resolve_cwd`, `pane_has_session_id`, `inject_session_id`,
  `basename`, `is_template_node`. The module **MUST NOT** depend on `zellij-tile` so `cargo test` builds
  on the host (native) target. Runtime behavior **MUST stay byte-identical** to the pre-extraction code.
  - **Refactor note (handed to S3/S4, not part of this spec's behavior contract)**: `maybe_enrich_pane`
    and `resolve_session_uuid` call host-side `eprintln!`/`std::fs` and the session-marker lookup; the
    extraction must split the *pure* tree-walk/serialization from the *I/O* (marker read, logging) so the
    pure half carries no `zellij-tile` dependency. How exactly to draw that seam is an S3 design decision.
- **Add a `#[cfg(test)]` unit-test suite with KDL v1 fixtures** covering the historically-broken
  regressions: snap-pane neutralization (incl. `start_suspended`/`close_on_exit` child-node stripping),
  idempotent enrichment (no double-inject when `--resume`/`--session-id` already present), template-subtree
  skipping, and relative-vs-absolute cwd resolution.
- **Add a new CI workflow** (push/PR triggered, separate from `release.yml` which stays release-on-tag)
  running `cargo build --release --target wasm32-wasip1` + `cargo fmt --check` + `cargo clippy -- -D warnings`
  + `cargo test`.
- Stale-comment fix surfaced during intake (developer note, not a spec AC): the doc-comment on
  `enrich_claude_panes` still says "inject `args "--session-id" "<uuid>"`" while the code injects `--resume`
  (`src/main.rs:61` vs `:240`). Should be corrected during extraction.

## Capabilities

### New Capabilities
- `kdl-enrichment-testability`: the extracted pure KDL module, its behavior-parity contract with the
  pre-extraction code, the regression unit-test suite, and the push/PR CI workflow that runs build + fmt +
  clippy + test.

### Modified Capabilities
- _(none)_ — `openspec/specs/` is empty; this is the first spec. No existing requirement changes.

## Impact

- **Code**: `src/main.rs` (functions move out + `mod enrich;` wiring), new `src/enrich.rs`. Binary crate
  stays a binary crate (`register_plugin!` must keep emitting `_start`). `Cargo.toml` may gain
  `[dev-dependencies]` and/or restructure to expose the module to tests — **without** flipping to cdylib.
- **CI/Tooling**: new `.github/workflows/ci.yml` (or similarly named). `release.yml` is untouched.
- **Dependencies**: `kdl` already present; tests reuse it. No new runtime deps. `zellij-tile` stays
  `=0.44.2` and is NOT pulled into the pure module or the host test target.
- **Docs**: `context/stack.md` "Test framework: None" / "Coverage gate: N/A" become stale once this lands
  (sync at S6). `CLAUDE.md` "There are no automated tests" line likewise.

## Non-Goals

- **NOT** testing host-interaction code: `dump_session_layout()`, `save_layout()`, the `pipe()`/`load()`
  trait methods, or `resolve_session_uuid`'s actual filesystem read — these need the Zellij WASI host and
  stay manual/headless-PTY verification.
- **NOT** changing any runtime behavior, output format, parsing dialect (stays KDL v1), or the
  `--resume` injection. This is a refactor-for-testability + harness, not a behavior change.
- **NOT** reaching the R-COV-001 ≥80% global coverage bar for the whole crate — coverage applies to the
  *extracted pure module*; the host/trait code remains untestable here. (See Assumptions.)
- **NOT** adding integration tests that spawn `zellij`, nor automating the existing manual PTY workflow.
- **NOT** touching `release.yml`, the Python hook, the shell helpers, or the installer.
- **NOT** adding a coverage-reporting tool (tarpaulin/llvm-cov) to CI unless the orchestrator confirms it
  (see Assumptions A4).

## Assumptions

- A1 [CONFIRMED] The eight named functions are pure (KDL str → KDL str, plus the recursive walk) **except**
  `maybe_enrich_pane`/`resolve_session_uuid`, which call `eprintln!` + `std::fs`. Source-verified at
  `src/main.rs:65–263`. Only the pure subset is the test target; the I/O seam is an S3 decision.
- A2 [CONFIRMED] `enrich_claude_panes` is pure string-in/string-out and is the natural top-level entry
  point for parity tests (`src/main.rs:65`).
- A3 [ASSUMED] The unit tests live in the extracted module via `#[cfg(test)] mod tests` (Rust-idiomatic,
  matches the user's "`#[cfg(test)]` unit-test suite" wording) rather than a separate `tests/` integration
  dir. Either satisfies `cargo test`; `#[cfg(test)]` keeps fixtures next to the code.
- A4 [ASSUMED] CI runs the four commands the user named (build/fmt/clippy/test) and does **not** add a
  coverage gate, because R-COV-001 (≥80%) "cannot be enforced here" per `context/stack.md` and no coverage
  tool is configured. If the orchestrator wants a coverage gate, that is a scope addition.
- A5 [ASSUMED] The new CI workflow runs on Ubuntu GitHub-hosted runners (matches `release.yml`) and
  installs the `wasm32-wasip1` target for the build step; `cargo test`/`fmt`/`clippy` run on the host
  (native) target. Concrete YAML is an S3/S4 artifact.
- A6 [CONFIRMED] "Byte-identical runtime behavior" is testable as: for the same input KDL string,
  the extracted `enrich_claude_panes` (with the marker lookup stubbed/injected) produces the same output
  string as the documented current behavior. Fixtures encode the current behavior.
- A7 [ASSUMED] `resolve_session_uuid`'s real `/tmp/...` read is replaced in tests by an injectable
  resolver (e.g. a function parameter or trait) so enrichment can be exercised without touching the
  filesystem — the cleanest way to test `maybe_enrich_pane`'s "marker found → inject / missing → leave
  bare" branch. The exact mechanism is an S3 design choice.
- A8 [CONFIRMED] No HTTP API, no DB, no auth, no PII, no payment, no file upload, no admin surface →
  STRIDE/threat modeling is **N/A** for this change (dev-tooling refactor). Recorded, not invented.

## Edge Cases

(Behavioral edge cases the test suite must encode — input boundary, state transition, data integrity,
integration; concurrency/permission/UI categories are largely N/A for a pure-function harness and are
noted as such.)

1. **(input boundary)** Empty / whitespace-only input KDL string → must not panic; parity preserved.
2. **(data integrity)** Malformed / unparseable KDL → `parse_v1` fails → raw input returned unchanged
   (graceful-degradation contract must hold post-extraction).
3. **(data integrity)** KDL that parses as **v2 but not v1**, and vice-versa → must use `parse_v1`, never
   default `parse()`; a v1 dump must round-trip via `ensure_v1`.
4. **(state transition)** `neutralize_snap_pane` strips `command`, `args`, **and** the `start_suspended`
   + `close_on_exit` CHILD nodes (the Jun 29 regression) — else restore errors.
5. **(state transition / idempotency)** Pane already carrying `--resume <uuid>` → NOT double-injected.
6. **(state transition / idempotency)** Pane already carrying `--session-id <uuid>` → treated as pinned,
   NOT injected with `--resume`.
7. **(data integrity)** Template subtrees (`new_tab_template`, `tab_template`, `swap_tiled_layout`,
   `swap_floating_layout`) and their nested panes are skipped — never enriched.
8. **(input boundary)** Relative pane `cwd="api"` joined onto layout base `cwd "/home/u"` →
   `/home/u/api`; trailing slash on base is trimmed once.
9. **(input boundary)** Absolute pane `cwd="/srv/x"` passes through unchanged regardless of base.
10. **(input boundary)** Pane with **no** `cwd` inherits the base cwd directly; pane with no cwd AND no
    base → `resolve_cwd` returns `None` → claude pane left bare (no panic).
11. **(integration / marker)** Marker resolver returns `Some(uuid)` → `args "--resume" "<uuid>"` prepended
    (any pre-existing positional args preserved, e.g. a prompt); resolver returns `None` → pane left bare.
12. **(data integrity)** `basename` on `/usr/bin/claude`, bare `claude`, and a path with no `/` →
    correct command-name match; a non-claude command (e.g. `vim`) is never enriched.
13. **(state transition)** A claude pane nested inside a non-template tab is enriched; the same pane
    pattern inside a template subtree is not (interaction of #7 and #11).
14. **(input boundary)** `inject_session_id` into a pane that has an existing `args` block prepends
    `--resume <uuid>` before existing args; into a pane with no `args` block creates one.

> **N/A categories**: *concurrency* — the pure functions are single-threaded, no shared state (the real
> two-panes-one-cwd marker race is a runtime/HANDOFF Risk-3 limitation, out of scope here). *permission* —
> no auth surface. *UI/UX* — no UI.

## Early Risk Flags

See the spec file `### Early Risk Flags` section for the full QA-early-review list. Headline risks:
- 🟠 **Parity-during-extraction with no prior safety net.** The refactor that *enables* tests has no test
  protecting it — a behavior drift introduced while moving the functions could be baked into the very
  fixtures meant to catch it. Mitigation: author fixtures from the *current* documented behavior + a manual
  headless-PTY round-trip before SPEC LOCK assumptions are locked at S4.
- 🟡 **`zellij-tile` decoupling may be non-trivial** if any of the eight functions transitively touch host
  types; A1/A7's I/O seam is the riskiest part of the extraction.
- 🟢 STRIDE: **N/A** (no auth/PII/payment/upload/admin) — A8.

## Figma

Figma: N/A (no UI — Zellij WASM plugin + CI/test tooling).
