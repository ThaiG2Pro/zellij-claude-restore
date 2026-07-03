# Tech Stack

## Runtime & Language
- **Language**: Rust (edition 2024) for the plugin; Python 3 for the SessionStart hook; POSIX shell (fish / bash / zsh) for helpers.
- **Runtime**: WebAssembly — `wasm32-wasip1` target, run inside Zellij's WASI plugin host (Zellij 0.44.2). The hook runs under the system `python3`.

## Framework
- **Web/App framework**: None (this is a Zellij WASM plugin, not a web app). Plugin framework is the `zellij-tile` SDK — the `ZellijPlugin` trait + `register_plugin!` macro.
- **Key libraries**: `zellij-tile = "=0.44.2"` (pinned to the zellij binary ABI), `kdl = { version = "6", features = ["v1"] }` (parse/serialize Zellij's KDL **v1** dumps via `parse_v1`/`ensure_v1`), `serde` + `serde_json`.

## Data
- **Database**: None. State is plain files — session-marker files (`/tmp/zellij-<uid>/claude-sessions/<encoded-cwd>.session`) and snapshot layouts (`~/.config/zellij/layouts/<name>.kdl`).
- **ORM / data layer**: None. Marker reads via `std::fs::read_to_string`; layout dump/save via the `zellij-tile` host commands `dump_session_layout()` / `save_layout()`.
- **Cache / queue**: None. (Note: Zellij itself caches the compiled `.wasm` per session under `~/.cache/zellij/` — a deployment gotcha, not an app cache.)

## Testing
- **Test framework**: Rust's built-in `#[test]` harness (`cargo test`) for the **pure** enrichment logic, **plus** manual / headless-PTY verification for the host (WASI) flow. The pure KDL-enrichment functions live in `src/enrich.rs` (no `zellij-tile` dep), so they compile and run on the **native host target** — `src/enrich.rs` carries an inline `#[cfg(test)] mod tests` with **43 unit tests** (run `cargo test`). The host-bound `pipe()`/`load()`/`dump_session_layout()`/`save_layout()` path in `src/main.rs` still has **no automated coverage** — it is exercised manually / headless-PTY only.
- **Coverage gate**: R-COV-001 (≥80%) is enforced only on the pure module's `cargo test` suite; the WASI host path cannot be unit-tested (no host outside Zellij). Run host tests: `cargo test`. Manual end-to-end of the host flow: `cargo build --release --target wasm32-wasip1` then `zellij pipe --plugin file:<abs-wasm> --name save -- <snapshot>` and inspect `~/.config/zellij/layouts/<snapshot>.kdl`, then `zellij --layout <snapshot>`. Headless: `script -qfec "zellij -s <name> -n <layout.kdl>" /dev/null &`.
- **Integration test policy**: Host (WASI) integration is manual end-to-end only. Must test in a **freshly started** zellij session (not a reattach) and purge the per-session wasm cache after redeploying: `find ~/.cache/zellij -type d -name 'zellij-claude-sync.wasm' -prune -exec rm -rf {} +`. The mandatory parity check (per the add-unit-tests change) is a headless-PTY round-trip confirming `claude --resume` restores the conversation, run before signing off the byte-identical parity test.

## Build / Tooling
- **Package manager**: Cargo (Rust). No Node/npm/composer.
- **Lint / format**: `cargo fmt` (rustfmt) and `cargo clippy` (Rust defaults; no custom config committed). CI enforces `cargo fmt --check` and `cargo clippy -- -D warnings`.
- **CI**: GitHub Actions — two workflows:
  - `.github/workflows/ci.yml` — on every **push and pull_request**: installs `wasm32-wasip1`, then runs `cargo build --release --target wasm32-wasip1` + `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test`. Each step gates the job.
  - `.github/workflows/release.yml` — on a `v*` tag: installs `wasm32-wasip1`, runs `cargo build --release --target wasm32-wasip1`, and attaches `target/wasm32-wasip1/release/zellij-claude-sync.wasm` to a GitHub Release.

## Build notes (load-bearing)
- Build: `cargo build --release --target wasm32-wasip1` → artifact `target/wasm32-wasip1/release/zellij-claude-sync.wasm` (hyphen; binary crate).
- This is a **binary crate, NOT cdylib** — only a binary emits the WASM `_start` export Zellij's loader needs; a cdylib is rejected with `could not find exported function`.
- `zellij-tile` is pinned `=0.44.2` to match the zellij binary; a caret range can skew the plugin ABI.
- Redeploying a rebuilt `.wasm` does NOT take effect in running sessions — purge the cache and start a fresh zellij session.
- **Pure logic is split out of `main.rs`** into `src/enrich.rs` (module `mod enrich;`) so it builds on the native host target for `cargo test`; `main.rs` keeps the `zellij-tile`-bound host flow and injects the marker resolver into `enrich::enrich_claude_panes` as a borrowed closure (`&|cwd| resolve_session_uuid(cwd)`). The host-bound items in `main.rs` are `#[cfg(not(test))]`-gated so `cargo test` compiles only the pure module.
