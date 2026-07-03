## 2026-06-30 — add-unit-tests: parity-spec AC tests should use exact-output assertions, not contains()

When a spec AC requires "byte-identical output" (parity contract), the corresponding test must use
`assert_eq!(result, EXPECTED_OUTPUT)` — not `result.contains("key-substring")`. The substring form is
a `[SHALLOW-TC]` pattern: it passes even if the serializer adds extra whitespace, re-orders nodes, or
inserts unexpected structure.

Compensating controls (idempotency `assert_eq!(first, second)` + a human PTY round-trip) made this a
Low finding rather than a blocker, but the gap should be caught earlier.

**Smoke-checklist addition for Rust/WASM QA:** For any spec with a "byte-identical" or "round-trip
fidelity" AC, grep the test suite for `contains()` assertions on the full output — flag them as
potentially shallow if the spec requires exact equality. Prefer `assert_eq!` with a pinned expected
string literal or at minimum the idempotency pattern.

**Secondary finding:** No-panic tests with `let _ = result;` (no assertion at all) do prove non-panic
but provide zero regression protection for return-value changes. At minimum assert `!result.is_empty()`
or the expected empty-document value.
