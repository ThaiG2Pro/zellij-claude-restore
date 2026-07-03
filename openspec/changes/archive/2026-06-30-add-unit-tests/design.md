# S3 — Technical Design: add-unit-tests (`kdl-enrichment-testability`)

> Change: `add-unit-tests` · Type: feature · Rigor: lite (single-pass DESIGN_REVIEW)
> deltaMode: ADDED · Branch: `feature/add-unit-tests` (base `main`)
> **openapi.yaml = N/A** (no HTTP API — Rust WASM plugin + test harness + CI). **DB migrations = N/A** (no database).
> Source of truth: `proposal.md` + `specs/kdl-enrichment-testability/spec.md` (28 ACs / 10 BRs / 4 INTs).

## Sketch — Gap Analysis

### ACs Reviewed
- AC-tests-001 … AC-tests-028 — all 28 reviewed against the current `src/main.rs:1–263`.

### BRs Reviewed
- BR-tests-001 … BR-tests-010 — all 10 reviewed.

### INTs Reviewed
- INT-tests-001 (module ↔ `main.rs` via `mod enrich;`), INT-tests-002 (module ↔ `kdl` v1),
  INT-tests-003 (module ↔ injected marker resolver), INT-tests-004 (CI ↔ GitHub Actions) — all 4 reviewed.

### Key source finding that de-risks the whole change
The eight named functions in `src/main.rs` **already** depend only on `kdl` + `std` — **none of them
import `zellij_tile::*`**. The only host coupling in the enrichment subtree is a single call:
`maybe_enrich_pane` (`:192`) invokes `resolve_session_uuid` (`:252`), which does `std::fs::read_to_string`
on a `/tmp` marker path. `eprintln!` is `std` (compiles natively, no host dep) but is logging that belongs
at the I/O boundary. So the extraction is **a move + one injection point**, not a rewrite. This directly
addresses the 🟡 MEDIUM "decoupling seam" risk: the seam is narrow and already nearly clean.

### Gaps Found
- **No critical gaps found.** All 28 ACs are satisfiable by the design below without a behavior change.
- Two NON-critical notes, handled in-design (no S2 return):
  - GAP-001 (informational): AC-tests-007 parity has **no pre-existing test net** (the 🟠 HIGH parity-bootstrap
    risk). Not a spec gap — it is a *process* risk. Mitigated by ADR-004 (fixtures seeded from current
    documented behavior + a mandatory manual headless-PTY round-trip at S4 before parity sign-off).
  - GAP-002 (informational): `resolve_session_uuid` currently lives inline and couples encoding logic
    (`cwd.replace('/','-')` + path format) with the `std::fs` read. The design splits the **pure encoding**
    (testable, stays in `enrich.rs` or is folded into the resolver-call site) from the **fs read** (stays in
    `main.rs`). No AC requires testing the fs read (Non-Goals); covered by ADR-001.

**Proceeding to full design** — no S2 return recommended.

---

## Context

`src/main.rs` is a single ~263-line file implementing the `ZellijPlugin` trait plus the KDL-enrichment
helpers. The enrichment logic has produced **two production regressions** caught only by manual testing
(Jun 29): the `start_suspended`/`close_on_exit` child-node stripping bug in `neutralize_snap_pane`, and
the `--session-id`→`--resume` correction. There is no automated safety net (`context/stack.md`:
"Test framework: None"; CI runs on `v*` tags only). This change extracts the pure enrichment subset into
`src/enrich.rs`, adds a `#[cfg(test)]` regression suite, and adds a push/PR CI workflow — **with zero
runtime behavior change** (parity is the acceptance bar, AC-tests-007).

### Constraints (locked invariants — MUST NOT break; AC-tests-004/005/006 + BR-tests-003/004/005/006)
1. **Synchronous save flow** in `pipe()` — no async `CustomMessage`. (BR-tests-006)
2. **Binary crate**, never cdylib — `register_plugin!` must keep emitting `_start`. (AC-tests-004 / BR-tests-003)
3. **KDL v1** — `parse_v1`/`ensure_v1`, never default v2 `parse()`. (AC-tests-005 / BR-tests-004)
4. **`--resume`, never `--session-id`** to resume. (AC-tests-006 / BR-tests-005)
5. **Graceful degradation** — parse fail → raw KDL; unresolvable cwd / missing marker → bare pane; never panic. (BR-tests-007)

## Goals / Non-Goals

**Goals**: (G1) a `zellij-tile`-free `src/enrich.rs` testable via `cargo test` on the native host
(AC-tests-001); (G2) WASM build + `_start` preserved (AC-tests-002); (G3) `pipe()` re-wired to call
`enrich::enrich_claude_panes` via `mod enrich;` (AC-tests-003, INT-tests-001); (G4) regression suite over
the four historically-broken areas + edge cases (BR-tests-008); (G5) push/PR CI running build/fmt/clippy/test
(AC-tests-025…028, BR-tests-009); (G6) byte-identical output (AC-tests-007/parity).

**Non-Goals** (from proposal): testing host code (`dump_session_layout`/`save_layout`/`pipe`/`load`/real
marker fs read); changing runtime behavior/format/parse-dialect/resume-flag; global ≥80% coverage across the
whole crate; integration tests spawning `zellij`; adding a coverage tool/gate (A4); touching `release.yml`,
the Python hook, shell helpers, installer.

## Architecture Overview

### System Components (unchanged process topology)
```
SessionStart hook (python)  →  marker file /tmp/.../<encoded-cwd>.session
                                          │ read (I/O boundary — stays in main.rs)
zellij pipe --name save  →  pipe()  →  dump_session_layout()  ──┐
                            (main.rs)                            │ kdl String
                                                                 ▼
                                          enrich::enrich_claude_panes(kdl, &resolver)   ← PURE (src/enrich.rs)
                                                                 │ enriched kdl String
                                                                 ▼
                                                          save_layout(name, kdl, true)
                                            (main.rs)
```

### Module Structure (after extraction)
```
src/
├── main.rs    — host layer: ZellijPlugin trait (load/pipe/render), register_plugin!(State),
│                 dump_session_layout/save_layout calls, the REAL marker resolver
│                 (fs read + eprintln!), MARKER_DIR const. `mod enrich;`.
└── enrich.rs  — PURE layer: enrich_claude_panes, enrich_nodes, neutralize_snap_pane,
                  resolve_cwd, pane_has_session_id, inject_session_id, basename, is_template_node,
                  + the SessionResolver seam (see ADR-001), + #[cfg(test)] mod tests with KDL v1 fixtures.
                  Imports ONLY `kdl` + `std`. NO `use zellij_tile`.
```

### `src/enrich.rs` public surface (what `main.rs` consumes)
```rust
// The injected lookup seam (ADR-001): a borrowed closure, cwd → Option<session-uuid>.
pub type SessionResolver<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// Pure entry point. Parses KDL v1, walks the tree, enriches claude panes using `resolve`
/// for the marker lookup, re-serializes as v1. On parse failure returns `kdl` unchanged.
pub fn enrich_claude_panes(kdl: &str, resolve: &SessionResolver<'_>) -> String;

// Remaining functions are pub(crate) or pub for direct unit testing of the regression areas:
pub(crate) fn enrich_nodes(nodes: &mut Vec<KdlNode>, base: Option<String>, in_template: bool, resolve: &SessionResolver<'_>);
pub(crate) fn neutralize_snap_pane(node: &mut KdlNode) -> bool;     // AC-tests-010/011/012
pub(crate) fn resolve_cwd(pane_cwd: Option<&str>, base_cwd: Option<&str>) -> Option<String>;  // AC-tests-018..021
pub(crate) fn pane_has_session_id(node: &KdlNode) -> bool;          // AC-tests-013/014
pub(crate) fn inject_session_id(node: &mut KdlNode, uuid: &str);    // AC-tests-022/023
pub(crate) fn basename(path: &str) -> &str;                          // AC-tests-024
pub(crate) fn is_template_node(name: &str) -> bool;                  // AC-tests-016/017
```
> `enrich.rs` carries NO `zellij-tile` dependency and touches NO filesystem in the pure path — the only
> fs/logging lives in `main.rs`'s resolver closure. This satisfies BR-tests-001 and INT-tests-002/003, and
> guarantees `cargo test` builds on the native target (AC-tests-001). (See ADR-001.)

### How `main.rs` re-wires
```rust
mod enrich;                       // INT-tests-001

// inside pipe(), replacing the current `enrich_claude_panes(&kdl)` call:
let enriched = enrich::enrich_claude_panes(&kdl, &|cwd: &str| resolve_session_uuid(cwd));
// resolve_session_uuid stays in main.rs: fs read + eprintln! (the I/O boundary).
```
The `enrich_claude_panes` body keeps `parse_v1`/`ensure_v1` and the graceful-degradation `eprintln!` on
parse failure — that `eprintln!` is `std`, builds natively, and is acceptable in the pure module (it is not
a host call). Per-pane "enriched"/"no marker"/"no cwd" diagnostics move to the resolver closure / are dropped
from the pure walk so the pure functions stay log-free (keeps test output clean). See ADR-005.

## ADR (Architecture Decision Records)

### ADR-001: Marker-lookup seam = borrowed closure parameter (`&dyn Fn(&str)->Option<String>`)
#### Context
`maybe_enrich_pane` needs a Claude session UUID for a cwd. In production that is a `/tmp` fs read
(`resolve_session_uuid`). For hermetic host tests the lookup must be injectable so `enrich.rs` needs no
filesystem and no `zellij-tile` (BR-tests-001, INT-tests-003, A7). Mechanism deferred to S3.
#### Options
| Option | Pros | Cons |
|--------|------|------|
| **A. Closure param `&dyn Fn(&str)->Option<String>`** (chosen) | Minimal surface; no new trait/type to maintain; tests pass an inline `|cwd| Some("uuid".into())` or `\|_\| None`; threads cleanly through the recursive `enrich_nodes`; zero runtime cost in prod (one indirect call per claude pane); idiomatic Rust DI for one function | Adds a parameter to `enrich_claude_panes`/`enrich_nodes`/`maybe_enrich_pane` signatures |
| B. Trait `SessionResolver` + impls (`FsResolver`, `StubResolver`) | Named, discoverable; easy to add methods later | Heavier: a trait + ≥2 impls for a single one-arg lookup; over-engineered for a 263-line plugin; more code to keep parity-stable |
| C. `#[cfg(test)]` to swap the body of `resolve_session_uuid` | No signature change | Pure module would still *reference* the fs path in non-test builds → keeps fs coupling in `enrich.rs`; `#[cfg(test)]` swaps are easy to drift from prod and CANNOT validate the prod path; violates "hermetic + zellij-tile-free module" intent |
#### Decision
**Option A** — pass the resolver as a borrowed closure. `main.rs` passes `&|cwd| resolve_session_uuid(cwd)`;
tests pass an inline stub. This keeps `enrich.rs` provably free of `std::fs` and `zellij_tile`, is the
smallest change preserving parity, and the closure is the same `Option<String>` contract the current code
already consumes — so output is byte-identical for a fixed resolver (AC-tests-007).
#### Consequences
- Positive: hermetic tests; `enrich.rs` builds on native; idempotency/template/cwd/neutralize functions are
  directly unit-testable; no new types.
- Negative: three signatures gain a `resolve` param (threaded through the recursion). Acceptable — it is the
  literal data dependency made explicit.
- Note: `cwd→encoded` mapping (`replace('/','-')`) is pure; it may stay in `main.rs`'s `resolve_session_uuid`
  (simplest, keeps the whole marker concern at the boundary). Chosen: keep encoding in `main.rs` with the fs read.
#### Status: Accepted

### ADR-002: Test layout = inline `#[cfg(test)] mod tests` in `src/enrich.rs`
#### Context
A3 assumes inline tests; analyst left the choice to S3. Options: inline `#[cfg(test)] mod tests` vs a
separate `tests/` integration directory.
#### Options
| Option | Pros | Cons |
|--------|------|------|
| **A. Inline `#[cfg(test)] mod tests` in `enrich.rs`** (chosen) | Fixtures live next to the code they pin; can test `pub(crate)` helpers (`resolve_cwd`, `basename`, `neutralize_snap_pane`) directly without widening their visibility to `pub`; idiomatic for unit tests; one file to review | Inflates `enrich.rs` length (fixtures are verbose KDL strings) |
| B. Separate `tests/enrich.rs` integration dir | Keeps prod file short | Can only see `pub` items → would force `basename`/`resolve_cwd`/`neutralize_snap_pane` public, widening the API surface for no runtime reason; better suited to whole-binary integration tests, which Non-Goals exclude |
#### Decision
**Option A**. The regression suite tests internal helpers (BR-tests-008 names `neutralize_snap_pane`,
`pane_has_session_id`, `is_template_node`, `resolve_cwd`), all of which are `pub(crate)` — only reachable from
an in-crate `#[cfg(test)]` module. Inline keeps fixtures beside the parity contract.
#### Consequences
- Positive: direct access to crate-internal helpers; fixtures co-located; matches "`#[cfg(test)]` unit-test
  suite" wording.
- Negative: longer file. Mitigate by grouping fixtures into `const` KDL strings at the top of the test module.
#### Status: Accepted

### ADR-003: CI workflow shape — new `.github/workflows/ci.yml`, single job, 4 sequential steps, NO coverage gate
#### Context
Need push/PR CI running build (wasm)/fmt/clippy/test, separate from `release.yml` (untouched, tag-only).
A4: NO coverage gate (orchestrator confirmed at SPEC LOCK). Mirror `release.yml`'s runner + target-install.
#### Options
| Option | Pros | Cons |
|--------|------|------|
| **A. One job `ci`, sequential steps, fail-fast** (chosen) | Simplest; mirrors `release.yml`; one runner provisioning; each step gates the job (AC-tests-027/028); easy to read | Steps run serially (slower than a matrix) — irrelevant for a tiny crate |
| B. Matrix / parallel jobs (build, lint, test split) | Faster on large repos; isolated failures | Over-engineered for one ~300-line crate; duplicates toolchain provisioning 3× |
| C. Add `cargo llvm-cov` coverage gate | Enforces R-COV-001 | A4 says NO gate (no tool configured; `context/stack.md` says ≥80% "cannot be enforced here"); scope addition needing sign-off — explicitly OUT |
#### Decision
**Option A**, no coverage gate. Single job on `ubuntu-latest`; checkout → install Rust + `wasm32-wasip1`
(mirrors `release.yml:20–24`) → `cargo build --release --target wasm32-wasip1` → `cargo fmt --check` →
`cargo clippy -- -D warnings` → `cargo test`. Default fail-fast: any non-zero step reds the job
(AC-tests-027/028, BR-tests-009). Triggers `on: [push, pull_request]` (INT-tests-004).
#### Consequences
- Positive: minimal, matches existing CI conventions, gates merges.
- Negative: serial steps. Acceptable. If a coverage gate is later wanted, it is a separate scoped change (A4).
#### Status: Accepted

### ADR-004: Parity-bootstrap mitigation — seed fixtures from CURRENT behavior + mandatory headless-PTY round-trip at S4
#### Context
🟠 HIGH risk: the refactor that *enables* tests has no prior net; a drift introduced while moving the
functions could be silently baked into the fixtures meant to catch regressions (AC-tests-007).
#### Options
| Option | Pros | Cons |
|--------|------|------|
| **A. Move-then-pin: extract with byte-for-byte function bodies, seed fixture EXPECTATIONS from current `src/main.rs` + spec scenarios, then one manual headless-PTY round-trip before parity sign-off** (chosen) | Catches both a refactor drift (round-trip would fail to restore) and locks current behavior into fixtures; cheap (one manual run already in the project's verification playbook) | Requires a human manual step at S4 (cannot be automated here — needs the WASI host) |
| B. Rewrite/clean up functions during extraction | Tidier code | Any behavior change during a no-net refactor is exactly the risk; rejected — extraction MUST be a pure move |
| C. Trust fixtures alone, no PTY round-trip | Fully automated | Cannot detect a drift baked into both code and fixtures; defeats the parity bar |
#### Decision
**Option A**. Extraction is a **verbatim move** of the eight function bodies (only signature additions for
the resolver param + the stale doc-comment fix). Fixture *expected outputs* are authored from the documented
current behavior (this spec's scenarios + the existing `src/main.rs` source), NOT from running the
post-refactor code. Before parity (AC-tests-007) is signed off at S4, the developer performs ONE manual
headless-PTY round-trip per the `context/stack.md` playbook (`script -qfec "zellij -s … -n <enriched.kdl>"`
… → confirm `claude --resume` restores). This is captured as the final-checkpoint gate in `tasks.md`.
#### Consequences
- Positive: defends parity from both code drift and fixture drift.
- Negative: a non-automatable manual step remains (inherent — host code is untestable here, per Non-Goals).
#### Status: Accepted

### ADR-005: Logging stays at the I/O boundary; pure walk is log-free except the parse-failure diagnostic
#### Context
The current code sprinkles `eprintln!` through `maybe_enrich_pane` (enriched / no-marker / no-cwd). These are
diagnostics, not behavior. For clean, deterministic test output the pure functions should not log per-pane.
#### Options
| Option | Pros | Cons |
|--------|------|------|
| **A. Drop per-pane `eprintln!` from the pure walk; keep only the parse-failure `eprintln!` in `enrich_claude_panes`; resolver closure in `main.rs` may log on its own** (chosen) | Pure functions deterministic + side-effect-light; test stdout/stderr clean; parse-failure diagnostic (operationally useful, std-only) retained | Loses per-pane stderr breadcrumbs — minor; the save still succeeds and the snapshot file is the success signal |
| B. Keep all `eprintln!` in the pure functions | Zero diagnostic loss | `eprintln!` is std (compiles), but noisy/non-deterministic in tests; per-pane logs are a host concern |
| C. Thread a logger callback too | Fully configurable logging | Over-engineered; second injected dependency for cosmetic logs |
#### Decision
**Option A**. The parse-failure `eprintln!` stays (it is `std`, not a host call, and is the documented
graceful-degradation breadcrumb — BR-tests-007). Per-pane enriched/skip diagnostics are removed from the
pure path; if desired, the `main.rs` resolver closure can log when it returns `Some`/`None`. This is a
**diagnostic-only** change and does NOT alter the returned KDL string — parity (AC-tests-007) holds.
#### Consequences
- Positive: deterministic tests; pure module stays clean.
- Negative: fewer per-pane stderr lines in production (acceptable; not a behavior contract).
#### Status: Accepted

## API Design

**N/A — no HTTP API.** This change adds no network surface. The relevant "interfaces" are:
- The `enrich.rs` public Rust surface (above) — the in-crate contract `main.rs` consumes.
- The unchanged Zellij pipe trigger (`zellij pipe --name save`) and marker-file / snapshot-file contracts.
- The GitHub Actions CI workflow trigger (`on: [push, pull_request]`).

`openapi.yaml` is intentionally **not produced** for this change (R-API-003 applies only to HTTP endpoints;
there are none — see `context/conventions.md`).

## DB Schema

**N/A — no database.** State is plain files (marker files + snapshot KDL). No tables, no migrations
(R-DB-001 N/A). No schema changes.

## Error Mapping

No HTTP status codes. Failure signalling (matches spec `## Error States`):

| Surface | Failure | Signal | AC ref |
|---------|---------|--------|--------|
| `enrich_claude_panes` | `parse_v1` fails | returns raw input unchanged; `eprintln!("[zellij-claude-sync] KDL parse failed…")` | AC-tests-008, BR-tests-007 |
| `enrich_claude_panes` | empty/whitespace input | no panic; raw/empty returned | AC-tests-009 |
| `maybe_enrich_pane` | cwd unresolvable | pane left bare (no panic) | AC-tests-021, BR-tests-007 |
| resolver | marker absent (`None`) | pane left bare | AC-tests-007(neg), BR-tests-007 |
| `cargo build --target wasm32-wasip1` | won't compile / wrong crate type | non-zero exit → CI red | AC-tests-002, AC-tests-026 |
| `cargo test` | regression reappears | non-zero exit → CI red | AC-tests-027 |
| `cargo fmt --check` / `cargo clippy -- -D warnings` | not clean / any warning | non-zero exit → CI red | AC-tests-028 |

## Sequence Flows

### Production save flow (unchanged behavior, re-wired call)
```
snap <name> → zellij pipe --name save → pipe() [main.rs]
  → dump_session_layout()                         (host, blocking, returns KDL String)
  → enrich::enrich_claude_panes(&kdl, &resolver)  [enrich.rs, PURE]
        resolver = |cwd| resolve_session_uuid(cwd)  [main.rs, fs read at boundary]
        → parse_v1 → enrich_nodes(walk) → ensure_v1 → to_string
  → save_layout(name, enriched, true)             (host, blocking)
  → eprintln! "saved snapshot"                    (success = snapshot file present)
```

### Test flow (hermetic, native target)
```
#[test] → enrich::enrich_claude_panes(FIXTURE_KDL, &|_cwd| Some("UUID".into()))
  → assert_eq!(output, EXPECTED_ENRICHED_KDL)   // parity / regression
no filesystem, no zellij-tile, no wasm target.
```

### Enrichment decision (per pane, unchanged logic)
```
pane → in template subtree? ─yes→ skip                              (AC-tests-016/017)
     └no→ is snap pane? ─yes→ neutralize (strip command/args/start_suspended/close_on_exit)  (AC-tests-010/011/012)
          └no→ command basename == "claude"? ─no→ leave             (AC-tests-024)
               └yes→ already has --resume/--session-id? ─yes→ leave (AC-tests-013/014)
                    └no→ resolve_cwd → None? ─yes→ leave bare       (AC-tests-021)
                         └Some(cwd)→ resolve(cwd) → None? ─yes→ leave bare
                              └Some(uuid)→ inject_session_id (prepend --resume <uuid>) (AC-tests-007/022/023)
```

## Edge Cases (→ test fixtures; BR-tests-008 + proposal §Edge Cases)
1. Empty / whitespace-only KDL → no panic, parity (AC-tests-009).
2. Unparseable KDL → raw returned unchanged (AC-tests-008).
3. v1 dump round-trips via `parse_v1`/`ensure_v1`; v2 `parse()` never used (AC-tests-005).
4. Snap pane strips `command`+`args`+`start_suspended`+`close_on_exit` child nodes (AC-tests-010) — the Jun 29 regression.
5. Pane already `--resume <uuid>` → not double-injected (AC-tests-013).
6. Pane already `--session-id <uuid>` → left pinned (AC-tests-014).
7. Template subtree skipped; identical pane outside template enriched (AC-tests-016/017).
8. Relative `cwd="api"` + base `/home/u` → `/home/u/api`; one trailing slash trimmed (AC-tests-018).
9. Absolute `cwd="/srv/x"` passes through (AC-tests-019).
10. No cwd + no base → `None` → bare, no panic (AC-tests-021); no cwd + base → inherits base (AC-tests-020).
11. Resolver `Some(uuid)` → inject; `None` → bare (AC-tests-007/BR-tests-007).
12. `basename` on `/usr/bin/claude` matches; `vim` never enriched (AC-tests-024).
13. Claude pane nested in non-template tab enriched (interaction of skip + inject).
14. `inject_session_id` into existing `args "prompt"` → `args "--resume" "<uuid>" "prompt"` (AC-tests-022); no args block → created (AC-tests-023).
15. Running `enrich_claude_panes` on its own output → unchanged / idempotent (AC-tests-015).

## Performance
Negligible. The enrichment is a one-shot recursive tree walk over a small KDL document on user-triggered
`snap`. Adding the resolver closure is one indirect call per claude pane (typically 1–3). CI adds ~minutes
of build/test on push/PR. No hot path, no concurrency (single-threaded pure functions — proposal §N/A
concurrency).

## Security
**STRIDE: N/A** (A8 — no auth/PII/payment/upload/admin/network surface; dev-tooling refactor). Relevant
checks for this change:
- R-SEC-001 (no hardcoded secrets): fixtures use placeholder UUIDs (e.g. `00000000-0000-0000-0000-000000000000`),
  NOT real session IDs. **Do not commit a real Claude session UUID** in a fixture.
- R-SEC-002 (no PII/tokens in logs): the retained parse-failure `eprintln!` logs no UUID; per-pane UUID logs
  are dropped from the pure path (ADR-005). A session UUID is not a secret credential but should not be logged
  gratuitously.
- No new dependencies (`kdl` already present; tests reuse it; no `[dev-dependencies]` strictly required —
  `kdl` is a normal dep available to `#[cfg(test)]`).

## Risk Assessment

| Risk | Sev | Mitigation |
|------|-----|------------|
| Parity drift baked into fixtures (no prior net) | 🟠 HIGH | ADR-004: verbatim move; fixtures seeded from current documented behavior; mandatory manual headless-PTY round-trip at S4 before parity sign-off (final checkpoint in tasks.md) |
| `zellij-tile` decoupling fails → `cargo test` won't build native | 🟡 MED | Source-verified: the 8 functions already import only `kdl`+`std`; only seam is the resolver (ADR-001). Task 4 checkpoint explicitly runs `cargo test` on native to confirm |
| Accidentally widening crate type to cdylib to "expose for tests" | 🟡 MED (AC-tests-004) | NOT needed — inline `#[cfg(test)]` tests in the binary crate run via `cargo test` without any crate-type change. Cargo.toml stays binary; ADR-002 |
| `cargo fmt`/`clippy -D warnings` failing the new CI on pre-existing code | 🟢 LOW | Task includes a `fmt`+`clippy` pass on the extracted code before CI lands (AC-tests-028) |
| Stale doc-comment shipped (`--session-id` at `:61`) | 🟢 LOW | Fixed during extraction (tasks.md task 2.x) — cosmetic, not an AC |

## Migration Plan
No deploy/runtime migration (no DB, no released artifact change in behavior). Rollout = merge the PR; CI
runs on the PR itself. Rollback = revert the commit (pure code move + new workflow file; `release.yml`
untouched). The `.wasm` produced post-refactor is byte-behavior-identical (parity), so no consumer impact.

## Open Questions
- None blocking. (The only manual dependency — the S4 headless-PTY parity round-trip — is a known,
  in-playbook verification step, not an open design question.)

## Implementation Guide

### Recommended Order (mirrors tasks.md)
1. **Extract** the 8 functions verbatim into `src/enrich.rs` (`mod enrich;` in `main.rs`); add the
   `SessionResolver` closure param threaded through `enrich_claude_panes`→`enrich_nodes`→`maybe_enrich_pane`;
   fix the stale `--session-id`→`--resume` doc-comment. Keep `parse_v1`/`ensure_v1`. (ADR-001, ADR-004, ADR-005)
2. **Re-wire** `main.rs`: `pipe()` calls `enrich::enrich_claude_panes(&kdl, &|cwd| resolve_session_uuid(cwd))`;
   `resolve_session_uuid` (fs read + encoding) stays in `main.rs`. (INT-tests-001)
3. **Checkpoint (mid-build)**: `cargo build --release --target wasm32-wasip1` (WASM + `_start` intact, AC-tests-002)
   AND `cargo test` builds on native (AC-tests-001) — even before tests exist, confirm the module compiles host-side.
4. **Tests**: add `#[cfg(test)] mod tests` to `enrich.rs` with KDL v1 fixtures covering the 15 edge cases /
   four regression areas (BR-tests-008). Seed expected outputs from documented current behavior (ADR-004).
5. **CI**: add `.github/workflows/ci.yml` (ADR-003). Do NOT touch `release.yml`.
6. **Final checkpoint**: full local `build (wasm) + fmt --check + clippy -D warnings + test` green; perform the
   manual headless-PTY round-trip to sign off parity (ADR-004) before handing to S5.

### Patterns to follow (with file paths)
- KDL parse/serialize: `src/enrich.rs` — `KdlDocument::parse_v1` + `ensure_v1` (NEVER `parse()`); `context/architecture.md` anti-pattern.
- Recursive walk with inherited base cwd: `enrich_nodes` (move verbatim from `src/main.rs:93–116`).
- Idempotency guard: `pane_has_session_id` treats `--resume` OR `--session-id` as pinned (`src/main.rs:218–229`).
- CI mirror: copy runner + target-install from `.github/workflows/release.yml:16–24` into the new `ci.yml`.
- Diagnostics prefix `[zellij-claude-sync]` (`context/conventions.md`).

### Gotchas
- **Do NOT switch to cdylib** to "make a lib testable" — inline `#[cfg(test)]` tests in the binary crate run
  fine with `cargo test`; cdylib breaks `_start` (AC-tests-004).
- **Extraction is a MOVE, not a refactor** — keep function bodies byte-identical except the resolver param and
  the doc-comment fix; behavior drift is the HIGH risk (ADR-004).
- **`enrich.rs` must not `use zellij_tile`** and must not call `std::fs` in the pure path — verify with a host
  `cargo test` build (AC-tests-001).
- **Fixtures use placeholder UUIDs**, never a real session id (R-SEC-001).
- **`start_suspended`/`close_on_exit` are CHILD nodes** on dumped command panes — the neutralize fixture must
  include them as children and assert they are stripped (AC-tests-010); they STAY on real command panes.
