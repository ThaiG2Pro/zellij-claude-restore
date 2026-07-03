# Domain Glossary

| Term | Definition | Notes / aliases |
|------|------------|-----------------|
| Snapshot | A named saved Zellij layout (`~/.config/zellij/layouts/<name>.kdl`) enriched so each claude pane resumes its conversation on restore. | Created by `snap <name>`; restored by `zellij --layout <name>`. |
| Enrichment | Injecting `args "--resume" "<uuid>"` into every restorable `claude` pane of a dumped layout KDL. | Done by `enrich_claude_panes()` / `enrich_nodes()`. |
| Marker file | A one-line file holding a Claude session UUID, keyed by encoded cwd, written by the SessionStart hook and read by the plugin. | Path `…/claude-sessions/<encoded-cwd>.session`. |
| Encoded cwd | An absolute working directory with every `/` replaced by `-`, used as the marker filename and matching Claude's `~/.claude/projects/<encoded-cwd>/` scheme. | e.g. `/home/u/api` → `-home-u-api`. |
| Session UUID | The Claude Code chat session identifier; the value resumed via `claude --resume <uuid>`. | Stored at `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`. |
| Snap pane | The pane that ran the `snap`/`zellij pipe … --name save` command; captured by the dump and neutralized so restore doesn't re-run it. | `neutralize_snap_pane()`; detected by command basename `zellij`/`timeout` + args. |
| Neutralize | Strip a pane's `command`/`args` (and `start_suspended`/`close_on_exit` children) so it restores as a plain shell. | Applied only to the snap pane. |
| Template subtree | A KDL node (`new_tab_template`, `tab_template`, `swap_tiled_layout`, `swap_floating_layout`) describing what to spawn for a new tab — skipped during enrichment. | Pinning these to an old session would be wrong. |
| `start_suspended` | A child node on dumped command panes making the pane wait for ENTER before running its command on restore. | Zellij default; kept on real command panes, dropped only when neutralizing. |
| KDL v1 | The KDL dialect Zellij dumps (kdl v4 crate). Parsed with `parse_v1`, re-serialized with `ensure_v1`. | The default v2 `parse()` fails on Zellij dumps. |
| Resurrection | Zellij's restore mechanism that replays each pane's command from `/proc/<pid>/cmdline` — the behavior this plugin works around for `claude`. | The core problem: bare `claude` → new chat. |
| `--resume` vs `--session-id` | `--resume <uuid>` re-opens an existing Claude session; `--session-id <uuid>` only assigns an ID to a NEW session and errors if the UUID exists. | The plugin injects `--resume`; treats either as "already pinned". |
| Pure KDL module | The `zellij-tile`-free Rust module `src/enrich.rs` holding the KDL enrichment logic, split out of `src/main.rs` so it compiles on the native host target and is unit-testable via `cargo test`. | Functions: `enrich_claude_panes`, `enrich_nodes`, `neutralize_snap_pane`, `resolve_cwd`, `pane_has_session_id`, `inject_session_id`, `basename`, `is_template_node`. |
| SessionResolver seam | The injected marker-lookup mechanism that keeps `enrich.rs` pure: a borrowed closure `&dyn Fn(&str) -> Option<String>` (`enrich::SessionResolver`) threaded through `enrich_claude_panes`→`enrich_nodes`→`maybe_enrich_pane`. | Prod passes `&\|cwd\| resolve_session_uuid(cwd)`; tests pass an inline stub. Chosen over a trait or `#[cfg]` swap (ADR-001). |
| Regression fixture | A KDL v1 test input encoding a historically-broken behavior (snap-pane neutralization, idempotent enrichment, template skip, cwd resolution) so it can never silently regress. | Lives in the `#[cfg(test)] mod tests` of `src/enrich.rs` (43 unit tests). |
