<!-- Thanks for contributing! Keep PRs focused. See CONTRIBUTING.md. -->

## What & why

<!-- What does this change and what problem does it solve? -->

## Checklist

- [ ] `cargo test` passes (add a test for any behavior change)
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo build --release --target wasm32-wasip1` succeeds
- [ ] Manually verified the host round-trip in a fresh Zellij session (if the plugin/host path changed)
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`
- [ ] `README.md` updated for user-facing changes
