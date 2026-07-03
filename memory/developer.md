# Developer Role Memory — Cross-Spec Lessons

Accumulated across changes. Each section is a reusable lesson for future builds.
Append-only: never delete or overwrite an existing `## ` section.

---

## 2026-06-30 — add-unit-tests: WASM binary crate + `cargo test` on native host requires `#[cfg(not(test))]` guards

**Context:** Adding inline `#[cfg(test)]` tests to a Rust binary crate that depends on `zellij_tile`
(or any WASM/WASI-only library with unresolvable host symbols).

**Trap:** `cargo test` compiles `main.rs` alongside the test module for the native host target. If
`main.rs` imports a crate that references platform-specific symbols (e.g. `host_run_plugin_command` in
`zellij_tile`), the native linker fails with `undefined symbol: host_run_plugin_command` even though the
`#[cfg(test)]` module itself is pure. The error looks like a test-setup failure but is a linker issue.

**Fix:** Wrap all imports and items that reference WASM-only host symbols with `#[cfg(not(test))]`:
```rust
#[cfg(not(test))]
use zellij_tile::prelude::*;

#[cfg(not(test))]
struct State {}

#[cfg(not(test))]
impl ZellijPlugin for State { … }

#[cfg(not(test))]
register_plugin!(State);
```
The `cfg(test)` flag is never set when compiling for `wasm32-wasip1`, so the WASM build is 100%
unaffected. Any functions in `main.rs` that are only called from the cfg-gated block (e.g.
`resolve_session_uuid`) should get `#[cfg_attr(test, allow(dead_code))]` to suppress the warning.

**Applicable to:** Any Rust binary crate targeting WASM/WASI that wants `cargo test` on the native host
while keeping `zellij_tile` (or similar WASI-specific crate) in the binary crate's `main.rs`. Works for
ADR-002-style inline tests without changing crate type to cdylib (which would break `_start`).
