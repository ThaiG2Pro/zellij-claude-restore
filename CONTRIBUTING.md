# Contributing to zellij-claude-sync

Thanks for taking a look! This is a small, focused tool — a Zellij WASM plugin plus a
Claude `SessionStart` hook and shell helpers. Contributions are welcome; the notes below
keep them smooth.

## Ground rules

- **Read the design first for non-trivial changes.** `CLAUDE.md` covers build/runtime
  gotchas; `HANDOFF.md` (Vietnamese) is the authoritative design rationale (decisions
  D1–D8, rejected alternatives). The core approach — explicit named snapshots, synchronous
  save, `--resume` not `--session-id`, binary crate not cdylib — was chosen deliberately.
- **Keep the enrichment core pure and tested.** All KDL logic lives in `src/enrich.rs`
  with no `zellij-tile` dependency, so it runs under `cargo test`. Host I/O enters as an
  injected resolver closure. Don't re-couple it to the host.
- **Every behavior change needs a unit test** in `src/enrich.rs` (`#[cfg(test)] mod tests`).

## Dev setup

```bash
rustup target add wasm32-wasip1
cargo test                                   # 40 host unit tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip1 # artifact: target/…/zellij-claude-sync.wasm
```

CI runs exactly these four on every push and PR — run them locally before opening a PR.

## Manual (host) verification

The WASI host path (`pipe`/`dump_session_layout`/`save_layout`) can't be unit-tested — there
is no host outside Zellij. After a plugin change, verify end-to-end in a **freshly started**
Zellij session (not a reattach), purging the per-session wasm cache first:

```bash
find ~/.cache/zellij -type d -name 'zellij-claude-sync.wasm' -prune -exec rm -rf {} +
cp target/wasm32-wasip1/release/zellij-claude-sync.wasm ~/.config/zellij/plugins/
# then, in a new zellij session with a claude pane running:
snap round-trip && zellij --layout round-trip   # confirm the conversation resumes
```

## Pull requests

- Branch from `main`; keep PRs focused.
- Conventional commit subjects: `<type>(<scope>): <subject>` (e.g. `feat(enrich): …`,
  `fix: …`, `docs: …`).
- Update `CHANGELOG.md` under `## [Unreleased]` and note any user-facing change in `README.md`.
- Don't bump `zellij-tile` without re-testing against the matching `zellij` binary — the
  plugin ABI breaks between pre-1.0 versions.

## Reporting bugs

Open an issue with your Zellij version (`zellij --version`), how you launched `claude`, and
whether the SessionStart hook marker exists
(`ls /tmp/zellij-$(id -u)/claude-sessions/`). Snapshot KDL (`~/.config/zellij/layouts/<name>.kdl`)
is helpful — scrub any session UUIDs you'd rather not share.
