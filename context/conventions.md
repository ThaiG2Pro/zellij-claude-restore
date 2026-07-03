# API & Code Conventions

> This project has **no HTTP/REST API** — it is a Zellij WASM plugin plus a Python hook and shell
> helpers. The "API" here is the Zellij pipe interface, the marker-file contract, and the KDL
> snapshot format. The standard web-API rules below are recorded as N/A so agents do not invent them.

## API Response Format
No HTTP API → no JSON success/error envelope. The relevant contracts are:
- **Pipe trigger**: `zellij pipe --plugin file:<abs-wasm> --name save -- <name>`. The plugin acts on
  `pipe_message.name == "save"`; the payload is the snapshot name (trimmed; empty → `"unnamed"`).
- **Marker file**: a single line containing the session UUID, written atomically by the hook at
  `/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session`. Plugin reads it (guest path
  `/tmp/claude-sessions/<encoded-cwd>.session`), trims, and treats empty as absent.
- **Outcome / error reporting**: the plugin does NOT return a structured result — the `zellij pipe`
  CLI stays blocked and is killed by `timeout`. Success is confirmed by the **presence of the
  snapshot file** `~/.config/zellij/layouts/<name>.kdl`, not by an exit code or response body.
  Diagnostics go to stderr via `eprintln!("[zellij-claude-restore] …")`.

## HTTP Status Policy
N/A — no HTTP layer, so no 2xx/4xx/5xx status codes are emitted or consumed. Failure signalling is
out-of-band: stderr log lines plus the snapshot-file existence check in the shell helper.

## URL / Resource Naming
No URLs. Naming contracts that DO matter:
- **Snapshot files**: `~/.config/zellij/layouts/<name>.kdl` (name = first positional arg to `snap`).
- **Encoded cwd**: absolute cwd with every `/` replaced by `-` — matching Claude's own
  `~/.claude/projects/<encoded-cwd>/` scheme. Used in both the hook and `resolve_session_uuid`.
- **WASM artifact basename** follows the package name with a **hyphen**: `zellij-claude-restore.wasm`
  (the old cdylib underscore name `zellij_claude_restore.wasm` is stale — do not reference it).

## Naming Conventions
- **Rust**: `snake_case` functions/locals, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts
  (e.g. `MARKER_DIR`). Standard rustfmt formatting.
- **Shell helpers**: kebab-style user commands (`snap`, `snap-list`, `snap-load`); env override
  `ZCS_PLUGIN`; installer env vars prefixed `ZCS_` (`ZCS_MODE`, `ZCS_REPO`, `ZCS_VERSION`).
- **Diagnostics**: every log line prefixed `[zellij-claude-restore]`.
- **Commits**: `<type>(<scope>): <subject>` (R-GIT-001; this repo has no ticket numbers, so the
  ticket-id segment is omitted — e.g. `fix: resume claude via --resume`).

## Validation
- **Plugin**: parse defensively. `KdlDocument::parse_v1` failure → save the raw KDL unchanged (never
  crash, never lose the snapshot). Panes already carrying `--resume`/`--session-id` are left
  untouched. Template subtrees (`new_tab_template`, `tab_template`, `swap_tiled_layout`,
  `swap_floating_layout`) are skipped. A claude pane whose cwd cannot be resolved is left bare.
- **Hook**: reads JSON from stdin (`session_id`, `cwd` — never trust `$CLAUDE_SESSION_ID`, empty in
  tool-call shells); empty `session_id`/`cwd` → no-op; always exits 0; writes atomically via
  `os.replace` so a half-written marker is never read.

## Documentation
No OpenAPI/Swagger (no HTTP API). Authoritative docs, in priority order:
1. **`HANDOFF.md`** (Vietnamese) — the authoritative design document (research, rejected
   alternatives, decisions D1–D8, open questions Q1–Q6).
2. **`CLAUDE.md`** — build / architecture / runtime gotchas for agents.
3. **`README.md`** — user-facing install + usage.
If a future change introduces an HTTP API, it must add an OpenAPI 3.0.x spec per R-API-003.
