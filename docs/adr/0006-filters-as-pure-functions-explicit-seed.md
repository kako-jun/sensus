# 0006 — Filters are pure functions with an explicit seed

## Status

Accepted.

## Context

`sensus-core` filters are applied not only to single still images but, for
video, **per frame** by the caller (the same per-frame API in a loop). Some
filters need randomness — floaters / vitreous opacities, scatter noise,
flickering stars — which naturally invites internal RNG state.

The crate also has a hard architectural line: `sensus-core` is pure logic with
no filesystem, no subprocesses, no GUI (all host I/O lives in the `sensus` CLI
crate).

## Decision

Every filter is a **pure function over pixel/audio buffers**: it does not
consult the filesystem, does not spawn subprocesses, and carries **no internal
RNG state** that drifts between calls. Filters that need randomness take an
**explicit `seed` parameter**, so successive frames are reproducible and
coherent. `strength` is always normalized to `0.0..=1.0` (`0.0` = identity).

## Alternatives considered

1. **Internal/global RNG state** in each randomized filter (e.g. a thread-local
   or filter-owned PRNG advanced on each call).
2. **Pure functions with an explicit seed argument** (chosen).

## Rationale

- Per-frame video application requires **determinism**: with internal RNG state,
  successive frames would draw different noise and the random elements (floaters,
  stars) would flicker incoherently between frames. An explicit seed lets the
  caller hold the seed constant across a clip so the random pattern stays stable
  (alternative 1 rejected).
- Purity keeps `sensus-core` testable and side-effect-free, matching the
  crate-layout rule that all I/O is isolated in the CLI crate. Pure functions
  are also what the GLSL shader path can mirror exactly (a fragment shader has
  no scan-order state) — e.g. the `dry_eye` per-tile *spatial hash* replaced an
  order-dependent sequential LCG precisely so a parallel shader could reproduce
  it.
- Normalizing `strength` to `0.0..=1.0` with `0.0` = identity gives every filter
  a uniform, predictable contract for callers and tests.

## Consequences / Trade-offs

- **Gain:** deterministic, reproducible output; coherent video; CPU and GLSL
  paths can be held equivalent; trivial unit testing (same input → same output).
- **Gain:** clean dependency story — no RNG crate state to manage, no hidden
  global state.
- **Cost:** callers must thread a `seed` (and other params) through every call;
  the API is slightly more verbose than a "just blur it" one-arg call.
- **Cost:** "more random-looking" effects that would benefit from genuine
  per-frame entropy are not available by default — but the caller can vary the
  seed deliberately if they want that.

## References

- `docs/overview.md` — "I/O contract": *Filters do not consult the filesystem …
  They are pure functions over pixel buffers … must therefore be deterministic …
  accept an explicit seed parameter so successive frames stay coherent*.
- `docs/overview.md` — "Eye Fatigue filters" (`dry_eye` spatial-hash rationale:
  the earlier sequential-LCG noise could not be reproduced by a parallel
  fragment shader).
- `CLAUDE.md` — "主要な設計判断": *フィルタは純粋関数 … 乱数が必要な場合は
  `seed` パラメータを明示的に受け取る*; *`strength` は 0.0..=1.0 に正規化*.
