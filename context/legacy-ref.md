# Legacy / Reference System

## Status
N/A — greenfield. This project does not port or mirror an existing system; there is no parity
contract to preserve. Agents should ignore parity concerns (R-API-001 greenfield scope, etc.).

> Caveat (not a legacy parity obligation): the plugin must conform to **external, version-pinned
> contracts** it does not own — Zellij 0.44.2's layout-KDL v1 format and plugin ABI, and Claude
> Code's `--resume <uuid>` / `SessionStart`-hook JSON / `~/.claude/projects/<encoded-cwd>/` storage
> scheme. These are upstream interfaces, treated as fixed inputs, not a legacy system being ported.
> The authoritative internal design rationale is `HANDOFF.md` (Vietnamese).

## Reference Source Location
- **Path / repo**: N/A

## Source-of-Truth Priority
1. `HANDOFF.md` — authoritative design (decisions D1–D8, rejected alternatives, open questions).
2. Approved specs (`openspec/specs/`) and `CLAUDE.md` build/runtime notes.
3. Live behavior of the pinned upstream tools (Zellij 0.44.2, Claude Code) where docs are silent.

## Parity Rules
N/A — no legacy byte-for-byte parity. The only hard external invariants are the upstream contracts
noted above (KDL v1 syntax, `zellij-tile =0.44.2` ABI, `claude --resume`).
