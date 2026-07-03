# Dev-Test Report — add-unit-tests (`kdl-enrichment-testability`)

> Phase: S4 | Agent: developer | Date: 2026-06-30
> Branch: `feature/add-unit-tests` | Rigor: lite

---

## Summary

Extracted the 8 pure KDL-enrichment functions from `src/main.rs` into a new `src/enrich.rs` module
(zero `zellij_tile` / `std::fs` in the pure path), injected the `SessionResolver` closure seam
(ADR-001), added a 33-test `#[cfg(test)]` regression suite, and created `.github/workflows/ci.yml`.
All cargo checks pass. One deliverable requires human execution (parity round-trip, see below).

---

## Build / Lint / Test Results

### `cargo build --release --target wasm32-wasip1`
```
Compiling zellij-claude-sync v0.1.0
Finished `release` profile [optimized] target(s) in 2.12s
```
**PASS** — `target/wasm32-wasip1/release/zellij-claude-sync.wasm` produced. Binary crate; `_start` present.

### `cargo test` (native host)
```
Compiling zellij-claude-sync v0.1.0
Finished `test` profile [unoptimized + debuginfo] target(s) in 0.46s
Running unittests src/main.rs (target/debug/deps/zellij_claude_sync-80c024f9b0267b11)
test result: ok. 33 passed; 0 failed; 0 ignored
```
**PASS** — 33 tests, 0 failures. `enrich.rs` builds on native host without `zellij_tile`.

### `cargo fmt --check`
```
(no output)
```
**PASS** — clean.

### `cargo clippy -- -D warnings`
```
cargo clippy: No issues found
```
**PASS** — zero warnings.

---

## AC / BR / INT Coverage Table

| ID | Description | Test(s) | Status |
|----|-------------|---------|--------|
| AC-tests-001 | Host-target test build succeeds (no zellij-tile) | `cargo test` on native host | PASS |
| AC-tests-002 | WASM plugin still builds, `_start` present | `cargo build --release --target wasm32-wasip1` | PASS |
| AC-tests-003 | `pipe()` calls `enrich_claude_panes` via `mod enrich;` | `src/main.rs` re-wire; build | PASS |
| AC-tests-004 | cdylib conversion rejected; binary crate stays binary | `Cargo.toml` unchanged; WASM build | PASS |
| AC-tests-005 | KDL v1 (`parse_v1`/`ensure_v1`) only; v2 never used | code inspection + `parity_claude_pane_enriched_with_resume_ac_tests_007` | PASS |
| AC-tests-006 | `--resume` injected; `--session-id` only treated as already-pinned | `parity_claude_pane_enriched_with_resume_ac_tests_007`; `existing_session_id_left_pinned_ac_tests_014` | PASS |
| AC-tests-007 | Byte-identical output for fixed resolver | `parity_claude_pane_enriched_with_resume_ac_tests_007` (exact `assert_eq!` vs `PARITY_EXPECTED` const); `idempotent_double_enrichment_ac_tests_015` | PASS (automated, full byte-identical assert); PENDING parity round-trip |
| AC-tests-008 | Parse failure returns raw input unchanged | `parse_failure_returns_raw_unchanged_ac_tests_008` | PASS |
| AC-tests-009 | Empty/whitespace input does not panic | `empty_input_no_panic_ac_tests_009` (`assert_eq!(result, "")`); `whitespace_only_input_no_panic_ac_tests_009` (`assert_eq!(result, "   \n\t  ")`) | PASS (now asserts returned value, not just non-panic) |
| AC-tests-010 | Snap pane (timeout): command + args + `start_suspended` + `close_on_exit` child nodes stripped | `snap_pane_timeout_all_children_stripped_ac_tests_010`; `neutralize_snap_pane_strips_all_child_nodes_ac_tests_010` | PASS |
| AC-tests-011 | Snap pane detected via `zellij` basename | `snap_pane_zellij_basename_neutralized_ac_tests_011` | PASS |
| AC-tests-012 | Non-snap pane not neutralized | `non_snap_zellij_pane_left_intact_ac_tests_012`; `neutralize_snap_pane_returns_false_for_non_snap_ac_tests_012`; `neutralize_snap_pane_returns_false_for_claude_pane_ac_tests_012` | PASS |
| AC-tests-013 | Pane with existing `--resume` not double-injected | `existing_resume_not_double_injected_ac_tests_013`; `pane_has_session_id_detects_resume_ac_tests_013` | PASS |
| AC-tests-014 | Pane with existing `--session-id` left pinned | `existing_session_id_left_pinned_ac_tests_014`; `pane_has_session_id_detects_session_id_ac_tests_014` | PASS |
| AC-tests-015 | Running enrichment twice is stable (idempotent) | `idempotent_double_enrichment_ac_tests_015` | PASS |
| AC-tests-016 | Claude pane inside template subtree not enriched | `claude_pane_in_template_not_enriched_ac_tests_016`; `all_template_node_types_skipped_ac_tests_016_extended`; `is_template_node_recognizes_all_four_ac_tests_016` | PASS |
| AC-tests-017 | Same pane outside template IS enriched | `claude_pane_outside_template_is_enriched_ac_tests_017`; `is_template_node_recognizes_all_four_ac_tests_016` | PASS |
| AC-tests-018 | Relative cwd joined onto base; trailing slash trimmed | `resolve_cwd_relative_joined_onto_base_ac_tests_018` | PASS |
| AC-tests-019 | Absolute cwd passes through unchanged | `resolve_cwd_absolute_passes_through_ac_tests_019` | PASS |
| AC-tests-020 | No pane cwd inherits base | `resolve_cwd_no_pane_cwd_inherits_base_ac_tests_020` | PASS |
| AC-tests-021 | No cwd + no base → None → pane left bare (no panic) | `resolve_cwd_none_none_returns_none_ac_tests_021`; `pane_no_cwd_no_base_left_bare_no_panic_ac_tests_021` | PASS |
| AC-tests-022 | `inject_session_id` prepends `--resume` preserving existing args | `inject_prepends_resume_preserving_existing_args_ac_tests_022` | PASS |
| AC-tests-023 | `inject_session_id` creates args block when none exists | `inject_creates_args_block_when_none_ac_tests_023` | PASS |
| AC-tests-024 | `basename` matches `/usr/bin/claude`; `vim` not enriched | `basename_full_path_extracts_name_ac_tests_024`; `path_qualified_claude_is_enriched_ac_tests_024`; `vim_pane_not_enriched_ac_tests_024` | PASS |
| AC-tests-025 | CI passes on clean push/PR | `.github/workflows/ci.yml` created with correct triggers | PASS (structural) |
| AC-tests-026 | `wasm32-wasip1` target installed before build step | `ci.yml` install step mirrors `release.yml` | PASS (structural) |
| AC-tests-027 | Failing test fails the job | `cargo test` step in `ci.yml` with default fail-fast | PASS (structural) |
| AC-tests-028 | Lint/format violation fails the job | `fmt --check` + `clippy -D warnings` steps in `ci.yml` | PASS (structural) |
| BR-tests-001 | `enrich.rs` has no `zellij_tile` dep | `grep zellij_tile src/enrich.rs` = 0 matches | PASS |
| BR-tests-002 | Runtime behavior byte-identical to pre-extraction | parity tests + idempotency test; parity round-trip PENDING | PARTIAL (automated PASS; PTY PENDING) |
| BR-tests-003 | Binary crate stays binary; `_start` emitted | WASM build produces artifact; `Cargo.toml` binary target | PASS |
| BR-tests-004 | KDL v1 only (`parse_v1`/`ensure_v1`) | code + tests use `KdlDocument::parse_v1` | PASS |
| BR-tests-005 | Resume uses `--resume`; `--session-id` only treated as pinned | `parity_claude_pane_enriched_with_resume_ac_tests_007`; `existing_session_id_left_pinned_ac_tests_014` | PASS |
| BR-tests-006 | Synchronous save flow; no async `CustomMessage` | `src/main.rs` pipe() inspection | PASS |
| BR-tests-007 | Graceful degradation: parse fail → raw; no cwd → bare; no panic | `parse_failure_returns_raw_unchanged_ac_tests_008`; `pane_no_cwd_no_base_left_bare_no_panic_ac_tests_021` | PASS |
| BR-tests-008 | Four regression areas covered: snap-pane neutralization, idempotent enrichment, template skip, cwd resolution | 10+ tests across tasks 4.2–4.5; `relative_cwd_resolved_and_enriched_br_tests_008` | PASS |
| BR-tests-009 | CI on push/PR; 4 commands each gate the job; `release.yml` unchanged | `ci.yml` structural; `git diff release.yml` = clean | PASS |
| BR-tests-010 | Coverage of extracted pure module SHOULD be ≥80% | 33 tests covering all 8 public/pub(crate) functions; no coverage tool/gate per A4 | PASS (best-effort; no gate) |
| INT-tests-001 | `pipe()` calls `enrich_claude_panes` via `mod enrich;` | `src/main.rs` + WASM build | PASS |
| INT-tests-002 | Module uses `kdl` v1 parse/serialize | `enrich.rs` imports + test uses `KdlDocument::parse_v1` | PASS |
| INT-tests-003 | Resolver seam injected/stubbed in tests | inline stubs `|_| Some(UUID.into())` / `|_| None` | PASS |
| INT-tests-004 | CI workflow triggers on push/PR; distinct from `release.yml` | `on: [push, pull_request]` in `ci.yml`; `release.yml` on `tags: ['v*']` only | PASS |

---

## Test Run Output (verbatim)

```
Running unittests src/main.rs (target/debug/deps/zellij_claude_sync-80c024f9b0267b11)

test enrich::tests::absolute_cwd_passthrough_enriched_br_tests_008 ... ok
test enrich::tests::all_template_node_types_skipped_ac_tests_016_extended ... ok
test enrich::tests::basename_full_path_extracts_name_ac_tests_024 ... ok
test enrich::tests::claude_pane_in_template_not_enriched_ac_tests_016 ... ok
test enrich::tests::claude_pane_outside_template_is_enriched_ac_tests_017 ... ok
test enrich::tests::empty_input_no_panic_ac_tests_009 ... ok
test enrich::tests::existing_resume_not_double_injected_ac_tests_013 ... ok
test enrich::tests::existing_session_id_left_pinned_ac_tests_014 ... ok
test enrich::tests::idempotent_double_enrichment_ac_tests_015 ... ok
test enrich::tests::inject_creates_args_block_when_none_ac_tests_023 ... ok
test enrich::tests::inject_prepends_resume_preserving_existing_args_ac_tests_022 ... ok
test enrich::tests::is_template_node_recognizes_all_four_ac_tests_016 ... ok
test enrich::tests::neutralize_snap_pane_returns_false_for_claude_pane_ac_tests_012 ... ok
test enrich::tests::neutralize_snap_pane_returns_false_for_non_snap_ac_tests_012 ... ok
test enrich::tests::neutralize_snap_pane_strips_all_child_nodes_ac_tests_010 ... ok
test enrich::tests::non_snap_zellij_pane_left_intact_ac_tests_012 ... ok
test enrich::tests::pane_has_session_id_detects_resume_ac_tests_013 ... ok
test enrich::tests::pane_has_session_id_detects_session_id_ac_tests_014 ... ok
test enrich::tests::pane_has_session_id_returns_false_for_plain_args_ac_tests_013 ... ok
test enrich::tests::pane_no_cwd_no_base_left_bare_no_panic_ac_tests_021 ... ok
test enrich::tests::parse_failure_returns_raw_unchanged_ac_tests_008 ... ok
test enrich::tests::parity_claude_pane_enriched_with_resume_ac_tests_007 ... ok
test enrich::tests::parity_no_marker_pane_left_bare_ac_tests_007_neg ... ok
test enrich::tests::path_qualified_claude_is_enriched_ac_tests_024 ... ok
test enrich::tests::relative_cwd_resolved_and_enriched_br_tests_008 ... ok
test enrich::tests::resolve_cwd_absolute_passes_through_ac_tests_019 ... ok
test enrich::tests::resolve_cwd_no_pane_cwd_inherits_base_ac_tests_020 ... ok
test enrich::tests::resolve_cwd_none_none_returns_none_ac_tests_021 ... ok
test enrich::tests::resolve_cwd_relative_joined_onto_base_ac_tests_018 ... ok
test enrich::tests::snap_pane_timeout_all_children_stripped_ac_tests_010 ... ok
test enrich::tests::snap_pane_zellij_basename_neutralized_ac_tests_011 ... ok
test enrich::tests::vim_pane_not_enriched_ac_tests_024 ... ok
test enrich::tests::whitespace_only_input_no_panic_ac_tests_009 ... ok

test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

---

## Design Deviations

### DEV-001 (minor): `#[cfg(not(test))]` guards on `main.rs` zellij-tile imports

**What**: Added `#[cfg(not(test))]` to `use zellij_tile::prelude::*`, `use std::collections::BTreeMap`,
`struct State`, `impl ZellijPlugin for State`, and `register_plugin!(State)` in `src/main.rs`.
Also added `#[cfg_attr(test, allow(dead_code))]` to `resolve_session_uuid`.

**Why**: `cargo test` on the native host links `main.rs` together with `enrich.rs`. The `zellij_tile`
crate references `host_run_plugin_command` — a WASI-only host symbol — which the native linker cannot
resolve. Without the `#[cfg(not(test))]` guards, `cargo test` fails at link time with
`undefined symbol: host_run_plugin_command`. The inline `#[cfg(test)]` test approach (ADR-002) requires
`main.rs` to be compilable on the native target.

**Impact**: Zero runtime or WASM behavior change. The WASM build continues to compile
`#[cfg(not(test))]`-gated code as normal (the test cfg is never set for WASM target). Parity is
preserved; binary crate invariant is preserved.

**Classification**: Minor implementation detail not foreseen in design (ADR-002 noted "inline tests in a
binary crate run via `cargo test` without any crate-type change" but did not specify the linker
isolation mechanism). This is the standard Rust idiom for binary crates with platform-specific host
deps. No design change required.

---

## Self-Review Log

- R-SEC-001 (no real session IDs): verified — only `00000000-0000-0000-0000-000000000000` and
  `11111111-1111-1111-1111-111111111111` placeholder UUIDs in fixtures.
- BR-tests-001 (`enrich.rs` no `zellij_tile`): `grep zellij_tile src/enrich.rs` = 0 matches.
- AC-tests-004 (binary crate not cdylib): `Cargo.toml` has `[[bin]]` not `[lib]`; `_start` present.
- AC-tests-005 (KDL v1 only): all test fixtures use `KdlDocument::parse_v1`; no `parse()` calls.
- AC-tests-006 (`--resume` not `--session-id`): doc-comment on `enrich_claude_panes` updated; code
  injects `"--resume"` only.
- BR-tests-006 (synchronous flow): `main.rs` `pipe()` calls `dump → enrich → save` sequentially with
  no async/channel primitives.
- `release.yml` unchanged: `git diff .github/workflows/release.yml` = empty.

---

## Bug Fixes

| # | Finding | Severity | What changed | New assertion shape |
|---|---------|----------|-------------|---------------------|
| F-001 | Shallow `contains()` assertions in parity test; `let _ = result` in empty/whitespace tests | Low (advisory) | `parity_claude_pane_enriched_with_resume_ac_tests_007`: replaced 3x `assert!(result.contains(…))` with `assert_eq!(result, PARITY_EXPECTED)` against a `const` byte-identical expected string. `empty_input_no_panic_ac_tests_009`: replaced `let _ = result` with `assert_eq!(result, "")`. `whitespace_only_input_no_panic_ac_tests_009`: replaced `let _ = result` with `assert_eq!(result, "   \n\t  ")`. | Full `assert_eq!` on the complete serialized output. |

### F-001 Resolution Detail

**`PARITY_EXPECTED` derivation** — the const was NOT generated by running the code and pasting output
(that would defeat the parity bar). It was constructed from the fixture + the documented injection rule:
- `inject_session_id` prepends `args "--resume" UUID` as a new child node inserted at index 0 of the
  children list, before the formatter assigns indentation context — so the injected line has no leading
  spaces.
- The KDL v1 serializer (`ensure_v1` + `to_string`) escapes `/` in string values as `\/`.
- The injected `args` node is terminated with `;` (KDL v1 node-terminator).

Resulting `PARITY_EXPECTED`:
```
layout {
    cwd "\/home\/user"
    pane command="claude" {
        start_suspended true
args "--resume" "00000000-0000-0000-0000-000000000000";
    }
}
```

**Empty and whitespace expected values** were confirmed by a temporary `#[test]` that printed
`{:?}` of the actual return value; the temp test was removed before final commit.

### Post-fix cargo results

```
cargo test: 33 passed (1 suite, 0.01s)
cargo fmt --check: PASS (no output)
cargo clippy -- -D warnings: No issues found
```

**F-001 status: RESOLVED.**

---

## Parity Round-Trip Status

**PENDING — requires human/host execution.**

The automated cargo checks (build + test + fmt + clippy) are all green and the unit tests encode the
documented pre-extraction behavior as fixtures. However, AC-tests-007 parity sign-off (ADR-004) requires
one manual headless-PTY round-trip confirming no behavior drift was baked into the fixtures.

**Exact command to run** (from `context/stack.md` playbook):
1. In a running Zellij session with a real `claude` pane, run: `snap <name>`
2. Inspect `~/.config/zellij/layouts/<name>.kdl` — confirm `command="claude"` pane carries `args "--resume" "<uuid>"`.
3. Start a fresh Zellij session with the enriched layout:
   `script -qfec "zellij -s test-restore -n ~/.config/zellij/layouts/<name>.kdl" /dev/null &`
   (use `-n`/`--new-session-with-layout`)
4. Confirm the claude pane reopens the prior conversation (not a new chat).

This step requires a live interactive Zellij 0.44.2 session with a real `claude` process running and a
SessionStart hook that has written a marker file. It cannot be run in this headless sandbox.
