# Architect Role Memory — Cross-Spec Lessons

Accumulated across changes. Each section is a reusable lesson for future designs.
Append-only: never delete or overwrite an existing `## ` section.

---

## 2026-06-30 — add-unit-tests: inject the host dependency as a borrowed closure, not a trait, to make a host-coupled module unit-testable

**Context:** Designing how to make a module testable on the native host when its only impurity is a
single host/IO call (here: a `/tmp` marker read via `resolve_session_uuid`, inside a WASM plugin whose
rest is `kdl` + `std`).

**Decision pattern (ADR-001):** Thread the host lookup through as a **borrowed closure parameter**
(`resolve: &dyn Fn(&In)->Option<Out>`) rather than (a) a trait + impls or (b) a `#[cfg(test)]` body-swap.
- Closure: smallest surface; prod passes `&|x| real_lookup(x)`, tests pass an inline stub; keeps the
  module provably free of the host crate + `std::fs`; same `Option<_>` contract as the original code, so
  output stays byte-identical (parity holds).
- Trait: over-engineered for a single one-arg lookup — a trait + ≥2 impls to maintain.
- `#[cfg(test)]` swap: still *references* the host path in non-test builds (keeps the coupling) and
  CANNOT validate the prod path — easy to drift.

**Reusable rule:** before extracting "the pure part", grep the candidate functions for what they actually
import. If they already pull only stdlib + a data crate and the host coupling is one call, the extraction
is a *move + one injection point*, not a rewrite — and a closure param is the lightest seam. Reserve a
trait for when there are several host operations or you need named, swappable implementations.

**Also reusable:** when a module physically cannot be host-tested (WASI/embedded), record the residual
manual verification (here: one headless-PTY round-trip) as an explicit gated checkpoint in `tasks.md`, not
as a hope — and seed test fixtures from *documented current behavior*, never from the post-refactor code's
own output (that bakes drift into the safety net).
