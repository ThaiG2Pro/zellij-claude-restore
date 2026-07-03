# Roadmap

Where `zellij-claude-restore` is headed. Items are grouped by how likely they are to
land soon — nothing here is a promise, and PRs that pick any of these up are welcome
(see [`CONTRIBUTING.md`](CONTRIBUTING.md)).

## Shipped

- **Named snapshots** — `snap <name>` dumps the current Zellij layout and enriches every
  `claude` pane with `--resume <uuid>` so `zellij --layout <name>` re-opens the exact chats.
- **Auto-enter on restore** (v0.2) — enriched claude panes drop `start_suspended` and resume
  without a keypress; unrecognized/unpinned panes keep the safe suspended default.
- **Save feedback** (v0.2) — `snap` reports enriched / already-pinned / missing-marker counts.
- **One-key snapshot** (v0.2) — `MessagePlugin` keybind fires a snapshot without typing a name.
- **Snapshot management** (v0.3) — `snap-rm`, `snap-clean`, and a richer `snap-list`
  (date + resumable-pane count).
- **Configurable claude command** (v0.3) — `ZCS_CLAUDE_CMD` / `claude_command` for renamed
  binaries (e.g. `claude-code`).
- **Snap-pane neutralization** — the pane that ran `snap` is restored as a plain shell.
- **Idempotent enrichment** — re-running `snap` never double-injects a resume flag.
- **Pure, unit-tested core** — the KDL enrichment lives in `src/enrich.rs` with a full
  regression suite (`cargo test`), CI on every push/PR, and a tagged release pipeline.

## Planned

- **Multi-assistant support.** Today only a single command basename (default `claude`) is
  recognized. Generalize to a set of AI-CLI assistants — OpenCode, Codex CLI, Gemini CLI,
  etc. — each with its own resume flag and session-id source, the way
  [`tmux-assistant-resurrect`](https://github.com/timvw/tmux-assistant-resurrect) does for
  tmux. `claude_command` (v0.3) is the stepping stone; the work is (1) a per-assistant table
  of `{ basename, resume-flag, marker source }`, (2) matching any of them during the KDL walk,
  and (3) SessionStart-style markers for assistants that expose an id. This is what would make
  `zellij-claude-restore` the general "assistant-resurrect for Zellij" rather than Claude-only.

## Considered / longer-term

- **Multiple `claude` panes in one cwd.** Markers are keyed by working directory, so two
  chats in the same directory currently collide on one marker (only one resumes cleanly).
  Disambiguating needs more than the dump exposes (it carries cwd but no pane/pid), so this
  is tracked as a known limitation for now.

- **Trigger a named snapshot without typing.** The v0.2 keybind uses a fixed rolling name
  (`quicksnap`); a floating input pane that prompts for a name would keep the one-key speed
  while allowing arbitrary names.
