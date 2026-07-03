# Implementation Plan: add-unit-tests (`kdl-enrichment-testability`)

## Overview
Extract the eight pure KDL-enrichment functions from `src/main.rs` into a `zellij-tile`-free
`src/enrich.rs` (with an injected marker-resolver closure), add a `#[cfg(test)]` regression suite with
KDL v1 fixtures, and add a push/PR CI workflow (build/fmt/clippy/test, NO coverage gate). Behavior MUST
stay byte-identical (parity, AC-tests-007). Binary crate stays binary (never cdylib). See `design.md`
ADR-001 (resolver seam = closure param), ADR-002 (inline tests), ADR-003 (CI), ADR-004 (parity bootstrap),
ADR-005 (logging at boundary).

Layering order: foundational module extraction → host re-wire → build checkpoint → tests → CI → final
checkpoint (with manual parity round-trip). Tests never precede the code they test.

## Tasks

- [x] 1. Extract the pure enrichment module `src/enrich.rs`
  - [x] 1.1 Create `src/enrich.rs`; move `enrich_claude_panes`, `enrich_nodes`, `neutralize_snap_pane`,
        `resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`, `is_template_node`
        **verbatim** from `src/main.rs:65–263`. Import only `kdl` + `std`; add NO `use zellij_tile`.
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-001, AC-tests-003, BR-tests-001, INT-tests-002_
  - [x] 1.2 Add the resolver seam: `pub type SessionResolver<'a> = dyn Fn(&str)->Option<String> + 'a;`
        and thread `resolve: &SessionResolver<'_>` through `enrich_claude_panes`→`enrich_nodes`→
        `maybe_enrich_pane`, replacing the direct `resolve_session_uuid` call with `resolve(&full_cwd)`.
        Keep `parse_v1`/`ensure_v1`; keep `--resume` injection. (ADR-001)
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-005, AC-tests-006, AC-tests-007, INT-tests-003, BR-tests-002, BR-tests-004, BR-tests-005_
  - [x] 1.3 Move per-pane diagnostics out of the pure walk; keep only the parse-failure `eprintln!` in
        `enrich_claude_panes`. Fix the stale doc-comment on `enrich_claude_panes`
        (`--session-id` → `--resume`, the `src/main.rs:61` text). (ADR-005, BR-tests-007)
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-006, AC-tests-008, BR-tests-007_
  - [x] 1.4 Set module visibility: `enrich_claude_panes` `pub`; the seven helpers `pub(crate)` so the
        inline `#[cfg(test)]` suite can test them directly. (ADR-002)
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-010, AC-tests-013, AC-tests-016, AC-tests-018, AC-tests-022, AC-tests-024_

- [x] 2. Re-wire `src/main.rs` to the extracted module
  - [x] 2.1 Add `mod enrich;`. Remove the eight moved functions from `main.rs`. Keep `resolve_session_uuid`
        (fs read + cwd encoding) and `MARKER_DIR` in `main.rs` as the I/O boundary. (INT-tests-001)
    - File: `src/main.rs`
    - _Requirements: AC-tests-003, INT-tests-001, BR-tests-007_
  - [x] 2.2 In `pipe()`, call `enrich::enrich_claude_panes(&kdl, &|cwd: &str| resolve_session_uuid(cwd))`.
        Keep the synchronous dump→enrich→save flow; do NOT introduce async/`CustomMessage`. Keep
        `register_plugin!(State)` (binary crate, `_start`). (BR-tests-003, BR-tests-006)
    - File: `src/main.rs`
    - _Requirements: AC-tests-003, AC-tests-004, BR-tests-003, BR-tests-006_

- [x] 3. Checkpoint — Extraction compiles on both targets
  - 🔍 HUMAN REVIEW GATE — STOP and wait for user confirmation
  - Verify `cargo build --release --target wasm32-wasip1` produces `target/wasm32-wasip1/release/zellij-claude-sync.wasm`
    (binary crate, `_start` present) — AC-tests-002.
  - Verify `cargo test` BUILDS on the native host (even with zero tests yet) — confirms `enrich.rs` is
    `zellij-tile`-free and fs-free in the pure path — AC-tests-001.
  - Verify `Cargo.toml` is still a binary crate (no `[lib] crate-type=["cdylib"]`) — AC-tests-004.
  - _Requirements: AC-tests-001, AC-tests-002, AC-tests-004, BR-tests-001, BR-tests-003_
  - RESULT: PASS — WASM build OK; `cargo test` 33 passed; Cargo.toml binary crate confirmed.

- [x] 4. Add the `#[cfg(test)]` regression suite with KDL v1 fixtures
  - [x] 4.1 Add `#[cfg(test)] mod tests` to `enrich.rs` with `const` KDL v1 fixture strings. Use placeholder
        UUIDs only (no real session ids — R-SEC-001). Stub the resolver inline (`|_| Some("00000000-…".into())`
        / `|_| None`). (ADR-002, ADR-004)
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-007, INT-tests-002, INT-tests-003, BR-tests-008_
  - [x] 4.2 Parity + graceful-degradation tests: representative claude-pane layout → byte-identical enriched
        output; unparseable KDL → raw returned; empty/whitespace → no panic; idempotent second pass. Seed
        expected outputs from documented CURRENT behavior, not post-refactor output. (ADR-004)
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-007, AC-tests-008, AC-tests-009, AC-tests-015, BR-tests-002, BR-tests-007_
  - [x] 4.3 Snap-pane neutralization tests: `timeout`/`zellij` + `save`+`pipe` args with `start_suspended`
        + `close_on_exit` CHILD nodes → all stripped (the Jun 29 regression); a real command pane untouched.
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-010, AC-tests-011, AC-tests-012_
  - [x] 4.4 Idempotent-enrichment tests: existing `--resume` → not double-injected; existing `--session-id`
        → left pinned. Template-skip tests: claude pane inside `new_tab_template`/etc not enriched; same pane
        outside template IS enriched.
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-013, AC-tests-014, AC-tests-016, AC-tests-017_
  - [x] 4.5 cwd-resolution + injection + basename tests: relative joined onto base (trailing slash trimmed);
        absolute passthrough; no-cwd inherits base; no-cwd-no-base → None → bare; `inject_session_id` prepends
        `--resume <uuid>` preserving existing args / creates block; `basename` on `/usr/bin/claude` matches,
        `vim` not enriched.
    - File: `src/enrich.rs`
    - _Requirements: AC-tests-018, AC-tests-019, AC-tests-020, AC-tests-021, AC-tests-022, AC-tests-023, AC-tests-024, BR-tests-010_

- [x] 5. Add the push/PR CI workflow
  - [x] 5.1 Create `.github/workflows/ci.yml`: `on: [push, pull_request]`, single job on `ubuntu-latest`;
        steps = checkout → install Rust + `wasm32-wasip1` (mirror `release.yml:20–24`) →
        `cargo build --release --target wasm32-wasip1` → `cargo fmt --check` →
        `cargo clippy -- -D warnings` → `cargo test`. Fail-fast: any non-zero step reds the job. Do NOT add
        a coverage gate. (ADR-003)
    - File: `.github/workflows/ci.yml`
    - _Requirements: AC-tests-025, AC-tests-026, AC-tests-027, AC-tests-028, INT-tests-004, BR-tests-009_
  - [x] 5.2 Verify `.github/workflows/release.yml` is UNCHANGED (tag-only release stays as-is).
    - File: `.github/workflows/release.yml`
    - _Requirements: AC-tests-025, BR-tests-009_

- [x] 6. Checkpoint — Final: green pipeline + manual parity round-trip
  - 🔍 HUMAN REVIEW GATE — STOP and wait for user confirmation
  - Run locally: `cargo build --release --target wasm32-wasip1` + `cargo fmt --check` +
    `cargo clippy -- -D warnings` + `cargo test` — all green (AC-tests-025/027/028).
  - **Parity sign-off (ADR-004, 🟠 HIGH risk)**: perform ONE manual headless-PTY round-trip per the
    `context/stack.md` playbook — `snap` a layout with a real claude pane, inspect the enriched
    `~/.config/zellij/layouts/<name>.kdl`, then `script -qfec "zellij -s … -n <name>.kdl" /dev/null` and
    confirm `claude --resume` restores the conversation. Confirms no behavior drift was baked into fixtures.
  - Confirm no real session UUID committed in fixtures (R-SEC-001); no `zellij_tile` import in `enrich.rs`.
  - _Requirements: AC-tests-007, AC-tests-025, AC-tests-026, AC-tests-027, AC-tests-028, BR-tests-002, BR-tests-008, BR-tests-009_
  - RESULT (automated): PASS — WASM build OK; fmt --check PASS; clippy -D warnings PASS; 33 tests PASS;
    no zellij_tile in enrich.rs; no real UUIDs in fixtures.
  - RESULT (parity round-trip): PENDING — requires human execution of the headless-PTY playbook
    (needs a live Zellij/WASI host). Command: `snap <name>` on a layout with a real claude pane, then
    `script -qfec "zellij -s test-restore -n ~/.config/zellij/layouts/<name>.kdl" /dev/null` and confirm
    `claude --resume <uuid>` re-opens the conversation.
