# Glossary — add-unit-tests

Shared, append-only domain/technical glossary. Every role appends here; keep **Phase** as the LAST column.

| Term | Definition | Phase |
|------|------------|-------|
| Pure KDL module | The zellij-tile-free Rust module (proposed `src/enrich.rs`) holding the KDL enrichment logic so `cargo test` builds on the host target. | S1 |
| Behavior parity | The acceptance bar for the extraction: for identical input KDL (and a fixed marker resolver), the extracted `enrich_claude_panes` produces byte-identical output to the pre-extraction `src/main.rs`. | S2 |
| Injected resolver | The session-marker lookup (`resolve_session_uuid`) supplied to enrichment as a parameter/trait in tests, so the pure module needs no filesystem/host access. Exact mechanism is an S3 decision. | S2 |
| I/O seam | The boundary separating the pure tree-walk/serialization from host I/O (`eprintln!` logging + `std::fs` marker read in `maybe_enrich_pane`/`resolve_session_uuid`). Drawing it cleanly is what lets `enrich.rs` drop the `zellij-tile` dep. | S2 |
| Regression fixture | A KDL v1 test fixture encoding a historically-broken behavior (snap-pane neutralization, idempotent enrichment, template skip, cwd resolution) so it can never silently regress. | S2 |
| CI workflow (push/PR) | New GitHub Actions workflow (separate from `release.yml`) running build + `fmt --check` + `clippy -D warnings` + `cargo test` on push/PR; each command gates the job. | S2 |
| Parity bootstrap risk | The 🟠 HIGH risk that the refactor enabling tests is itself untested, so a drift introduced during extraction could be baked into the very fixtures meant to catch regressions. | S2 |
| Pure subset | The eight string-in/string-out functions targeted for host tests (`enrich_claude_panes`, `enrich_nodes`, `neutralize_snap_pane`, `resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`, `is_template_node`); everything needing the WASI host (pipe/load/dump/save) stays manual/headless-PTY. | S2 |
| Idempotent enrichment | The property that re-running enrichment on a pane already carrying `--resume`/`--session-id` does not double-inject; `pane_has_session_id` treats either flag as already pinned. | S2 |
| Snap-pane neutralization | Stripping `command`/`args` plus the `start_suspended`/`close_on_exit` child nodes from the pane that ran `snap`, so restore brings it back as a plain shell instead of re-running the save and hanging. | S2 |
| SessionResolver seam | The S3-chosen mechanism for the injected marker lookup: a borrowed closure `&dyn Fn(&str)->Option<String>` threaded through `enrich_claude_panes`→`enrich_nodes`→`maybe_enrich_pane`. Prod passes `&\|cwd\| resolve_session_uuid(cwd)`; tests pass an inline stub. Chosen over a trait or `#[cfg]` swap (ADR-001). | S3 |
| Verbatim move | The extraction discipline (ADR-004): the eight function bodies are relocated byte-for-byte (only the resolver param + doc-comment fix added), so no behavior drift can creep in during the no-net refactor. | S3 |
| Headless-PTY round-trip | The mandatory manual S4 parity check (`script -qfec "zellij -s … -n <enriched.kdl>"` → confirm `claude --resume` restores) run before AC-tests-007 parity is signed off; defends against drift baked into fixtures. | S3 |
