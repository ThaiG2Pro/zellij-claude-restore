# Cross-Spec Context

Knowledge bridge agents read when starting a NEW change. Append-only — one block per change at S3 done.

## add-unit-tests — kdl-enrichment-testability (S3 done: 2026-06-30)
### Dependencies (from other changes)
- None (first change in this repo's OpenSpec workspace).
### Shared Decisions
- ADR-001: KDL-enrichment marker lookup is injected as a borrowed closure `&dyn Fn(&str)->Option<String>` (resolver seam), NOT a trait — keeps the pure module `zellij-tile`-free and fs-free.
- ADR-002: unit tests live inline as `#[cfg(test)] mod tests` in the source module (binary crate, never cdylib for testability).
- ADR-004: refactors of enrichment logic are verbatim moves; parity = byte-identical KDL output + one manual headless-PTY round-trip before sign-off (no auto host test possible).
### Exports (other changes may depend on these)
- `src/enrich.rs` — pure KDL-enrichment module: `pub fn enrich_claude_panes(kdl: &str, resolve: &SessionResolver) -> String` + `pub type SessionResolver<'a> = dyn Fn(&str)->Option<String> + 'a`; `pub(crate)` helpers (`neutralize_snap_pane`, `resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`, `is_template_node`, `enrich_nodes`).
- `.github/workflows/ci.yml` — push/PR CI (build wasm32-wasip1 + fmt --check + clippy -D warnings + cargo test); `release.yml` stays tag-only.
### Constraints Set (apply to subsequent changes)
- Locked invariants: synchronous save flow (no async CustomMessage); binary crate (emits `_start`, never cdylib); KDL v1 (`parse_v1`/`ensure_v1`, never v2 `parse()`); `--resume` not `--session-id`.
- New enrichment logic MUST be added to `src/enrich.rs` (host-testable), not back into `main.rs`; cover it with `#[cfg(test)]` fixtures.
- No CI coverage gate currently (A4) — adding one is a separately-scoped change.
---
