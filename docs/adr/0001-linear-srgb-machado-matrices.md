# 0001 — Operate color-vision simulation in linear sRGB, applying Machado 2009 matrices directly (no explicit LMS pipeline)

## Status

Accepted.

## Context

Color vision deficiency (CVD) simulation conceptually lives in cone (LMS)
space: the deficiency is a loss or shift of one cone type. A textbook pipeline
would therefore be `sRGB → linear sRGB → LMS → (deficiency transform) → linear
sRGB → sRGB`, carrying explicit LMS conversion matrices and a cone-response
model.

`sensus` needs CVD simulation for protanopia, deuteranopia, and tritanopia. It
must produce color-scientifically defensible output and be cheap enough to run
per-frame for video.

## Decision

Operate the simulation **entirely in linear sRGB** and apply the
**Machado, Oliveira & Fernandes (2009)** pre-computed `severity = 1.0` matrices
**directly** as `linear sRGB → simulated linear sRGB`. Do **not** build an
explicit LMS pipeline.

Pixels are gamma-decoded (`srgb_to_linear`), the 3×3 matrix is multiplied in
linear space, and the result is gamma-encoded back to sRGB
(`linear_to_srgb`). Alpha is preserved.

## Alternatives considered

1. **Explicit LMS pipeline** — convert to LMS, apply a cone-loss transform,
   convert back. This is the "from first principles" route.
2. **Multiply the matrix against gamma-encoded sRGB** (skip the linearization).
   This is the common naïve shortcut seen in many web demos.
3. **Use Machado's pre-computed linear-sRGB matrices directly** (chosen).

## Rationale

- Machado et al. already pre-multiplied the cone-space physiology into a single
  linear-sRGB→linear-sRGB matrix per deficiency and severity. Re-deriving an
  LMS pipeline would reproduce the same published result with more code, more
  constants to get wrong, and no fidelity gain — the physiology is *already
  baked into* the published matrix.
- The matrices are a published, peer-reviewed, citable artefact (IEEE TVCG,
  DOI: 10.1109/TVCG.2009.113), with the author's supplementary page and the
  DaltonLens project carrying identical values. Using them directly keeps the
  output auditable against an external reference (see
  [`matrix-provenance.md`](matrix-provenance.md)).
- Doing the math in **linear** sRGB is mandatory for correctness: multiplying a
  color matrix against gamma-encoded sRGB darkens midtones and is
  color-scientifically wrong (alternative 2 is rejected on these grounds, per
  `docs/overview.md` "Color vision algorithm").

## Consequences / Trade-offs

- **Gain:** minimal, auditable code; output that can be cross-checked against
  the published source values; a single matrix multiply per pixel (cheap enough
  for per-frame video).
- **Gain:** no separate LMS conversion matrices to maintain or get wrong.
- **Cost:** the simulation is only as good as the Machado matrices; we cannot
  independently tune cone responses without replacing the source matrices (a
  change that would have to update both the const/table and the provenance
  spec — see ADR-0008 (per-severity table; ADR-0002 for achromatopsia) and
  `matrix-provenance.md`).
- **Cost:** gamma decode/encode runs per pixel; this is the price of doing the
  math in linear space (and is shared by every other linear-sRGB filter).

## References

- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)".
- `crates/core/src/vision/mod.rs` — module docstring (`## protanopia / deuteranopia
  / tritanopia`, `# 色空間`); `crates/core/src/vision/color.rs` — `PROTANOPIA` /
  `DEUTERANOPIA` / `TRITANOPIA` const definitions and `apply_machado_matrix`.
- `CLAUDE.md` — "主要な設計判断": *色覚特性は linear sRGB + Machado 2009
  severity=1.0 行列*.
- [`matrix-provenance.md`](matrix-provenance.md).
