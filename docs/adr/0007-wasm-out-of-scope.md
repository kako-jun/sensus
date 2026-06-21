# 0007 — WebAssembly is out of scope

## Status

Accepted.

## Context

A perceptual-filter library is an obvious candidate for a browser/WebAssembly
build — it would let web frontends run the simulations client-side. Supporting
`wasm32` would, however, pull in web-specific concerns (e.g. a `getrandom`
backend and other target-conditional dependencies) and an additional build/CI
matrix to maintain.

## Decision

**WebAssembly is not a target.** `sensus` does not ship a wasm build and does
not take on `wasm32`-specific dependencies. Web frontends are out of scope.

## Alternatives considered

1. **Add a `wasm32` target** with the necessary web-specific dependencies and a
   wasm CI lane.
2. **No wasm target** (chosen).

## Rationale

- The primary (and prominent) consumer is
  [universal-experience](https://github.com/kako-jun/universal-experience), a
  **native Flutter app** that links `sensus-core` directly. There is no current
  web consumer.
- A wasm build adds maintenance cost — extra dependencies (e.g. a wasm
  `getrandom` backend) and an extra build/test matrix — **without a clear
  consumer** to justify it. The decision keeps the dependency surface small and
  the CI matrix native-only.

## Consequences / Trade-offs

- **Gain:** smaller dependency surface, simpler CI, no target-conditional code
  paths for randomness or I/O.
- **Cost:** a future web frontend cannot reuse `sensus-core` in-browser without
  first reversing this decision (which would be a new ADR superseding this one).
- **Cost:** none for the current native consumer, which is the only one that
  exists.

## References

- `docs/overview.md` — "Crate layout" (*WebAssembly is not a target … Web
  frontends are out of scope*) and "Out of scope / Non-goals" (*a wasm build
  adds maintenance cost without a clear consumer*).
- `CLAUDE.md` — "主要な設計判断": *WASM ターゲットは持たない … wasm32 用の
  getrandom 等の追加依存は避ける*.
