# Roadmap

Where `zellij-claude-sync` is headed. Items are grouped by how likely they are to
land soon — nothing here is a promise, and PRs that pick any of these up are welcome
(see [`CONTRIBUTING.md`](CONTRIBUTING.md)).

## Shipped

- **Named snapshots** — `snap <name>` dumps the current Zellij layout and enriches every
  `claude` pane with `--resume <uuid>` so `zellij --layout <name>` re-opens the exact chats.
- **Snap-pane neutralization** — the pane that ran `snap` is restored as a plain shell.
- **Idempotent enrichment** — re-running `snap` never double-injects a resume flag.
- **Pure, unit-tested core** — the KDL enrichment lives in `src/enrich.rs` with a full
  regression suite (`cargo test`), CI on every push/PR, and a tagged release pipeline.

## Planned

- **Auto-enter on restore (claude panes only).** Today Zellij restores command panes
  *suspended* — you press ENTER in each pane to launch `claude --resume …`. We want to
  drop `start_suspended` **only** for the panes we enriched, so your conversations resume
  automatically while other command panes keep the safe manual-launch default. The scope
  is deliberately narrow (claude panes we recognize) to avoid auto-running arbitrary
  commands on restore.

- **Trigger snapshots without typing a name.** A keybinding (or a leader-key mapping in
  the Zellij config) that fires `snap` directly. The open question is naming: a keybind
  has no argument, so an unnamed snapshot is hard to restore by name. Likely direction —
  bind to a well-known rolling name (e.g. `snap last`) and/or prompt for a name via a
  floating input pane.

## Considered / longer-term

- **Multiple `claude` panes in one cwd.** Markers are keyed by working directory, so two
  chats in the same directory currently collide on one marker (only one resumes cleanly).
  Disambiguating needs more than the dump exposes (it carries cwd but no pane/pid), so this
  is tracked as a known limitation for now.

- **Clearer save feedback.** `snap` confirms success by the snapshot file appearing, not by
  an exit code. A small status side-channel would let the helper report *what* happened
  (enriched N panes, M markers missing) instead of just "a file exists".
