# 0005 — Phased scope: Phase 1 limited to the four dichromacy / achromatopsia types

## Status

Accepted.

## Context

`sensus` ultimately covers a large surface of vision and hearing conditions
(the `roadmap.md` table spans Phases 0–6). Building everything at once risks
shipping a wide but shallow, hard-to-verify codebase.

A starting point had to be chosen. The candidate first cut for the `vision`
module was the color-vision conditions.

## Decision

Scope **Phase 1** (Issue #2) to exactly four color-vision types:

- `protanopia`
- `deuteranopia`
- `tritanopia`
- `achromatopsia`

Later perceptual categories (tetrachromacy, refraction, visual-field defects,
light/transparency, motion, hearing, stereo, shaders) are deferred to their own
later phases, each tracked by its own Issue in `roadmap.md`.

## Alternatives considered

1. **Implement the whole vision module at once** (all field defects, refraction,
   light, motion together).
2. **Start with refraction / blur** instead of color vision.
3. **Start with the four color-vision types** (chosen).

## Rationale

- The four color-vision types rest on a **published, peer-reviewed source with
  citable numeric values** — the Machado 2009 severity-1.0 matrices for the
  three dichromacies and the BT.709 luminance weights for achromatopsia
  (ADR-0001, ADR-0004, [`matrix-provenance.md`](matrix-provenance.md)). That
  makes them the most *verifiable* starting point: the output can be checked
  against an external reference rather than against the project's own opinion.
- That verifiability is concrete: the four types are exactly the ones pinned by
  the source-value known-answer tests (`crates/core/tests/color_kat.rs` covers
  protanopia / deuteranopia / tritanopia / achromatopsia). A first phase whose
  correctness can be asserted against the literature establishes the gamma /
  matrix / blend pipeline that later linear-sRGB filters reuse.
- Tetrachromacy was *not* included in this first cut precisely because it is
  **not** physically derivable from RGB input — it is an illustrative
  visualization, not a source-anchored simulation — so it belongs in a separate
  phase (Phase 1+, #3) with a clearly different fidelity claim.

## Consequences / Trade-offs

- **Gain:** a small, fully source-verifiable first deliverable that lays down
  the linear-sRGB gamma/matrix/blend foundation reused by every later vision
  filter.
- **Gain:** each subsequent category gets its own Issue and phase, so scope and
  fidelity claims stay legible per `roadmap.md`.
- **Cost:** the crate was not "feature complete" for vision at the end of
  Phase 1; consumers had to wait for later phases for blur, field defects, etc.
- **Cost:** the phased boundary is a project-management line, not a perceptual
  one — some users think of "color blindness" and "blurry vision" together, but
  they shipped in different phases.

## References

- `docs/roadmap.md` — phase table: Phase 1 = 色覚特性
  (protanopia / deuteranopia / tritanopia / achromatopsia), #2; tetrachromacy is
  a separate Phase 1+, #3.
- `crates/core/src/vision.rs` — module docstring: *Phase 1 (Issue #2) では色覚
  特性 4 種を実装する*.
- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)" and "Tetrachromacy
  algorithm (Phase 1+, #3)" (the *Fundamental limitation*: a physically exact
  tetrachromat simulation is impossible from RGB input).
- `crates/core/tests/color_kat.rs` — source-value KAT covers exactly these four
  types.
