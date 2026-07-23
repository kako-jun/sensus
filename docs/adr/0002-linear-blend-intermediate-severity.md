# 0002 — Linear blend for intermediate severity

## Status

Superseded by [0008](0008-machado-per-severity-table.md) for protanopia /
deuteranopia / tritanopia. Still governs `achromatopsia`, which has no
published per-severity matrix table and continues to use the linear blend
described below unchanged.

## Context

The published Machado 2009 matrices used by `sensus` (see ADR-0001) are the
`severity = 1.0` matrices: full dichromacy. But the `strength` parameter is
normalized to `0.0..=1.0`, where `0.0` must return the original image and
intermediate values must represent partial / anomalous trichromacy (mild color
vision deficiency).

Machado et al. publish a *family* of matrices indexed by severity, so one
option is to interpolate between published matrices. The question is how to map
an arbitrary `strength` to a partial effect.

## Decision

Compute the full `severity = 1.0` simulated color, then blend it with the
original in **linear sRGB space**:

```
n = original + (simulated - original) * strength
```

i.e. `lerp(original, simulated, strength)` per channel, in linear space, applied
identically for protanopia / deuteranopia / tritanopia (and the achromatopsia
grayscale target — see ADR-0004).

## Alternatives considered

1. **Interpolate between Machado's per-severity matrices** — carry the full
   table of matrices (severity 0.0…1.0 in 0.1 steps) and pick / interpolate by
   `strength`.
2. **Blend in gamma-encoded sRGB** — lerp after gamma encoding.
3. **Linear-space lerp between original and the severity-1.0 result** (chosen).

## Rationale

- A linear-space lerp toward the full-deficiency result is the linearised
  approximation of anomalous trichromacy that Machado himself suggests and that
  DaltonLens et al. adopt — it is an accepted, documented practice, not an
  invention of this project.
- It needs only the single `severity = 1.0` matrix per deficiency, keeping the
  constant set small and the provenance story simple (one cited matrix per type,
  per [`matrix-provenance.md`](matrix-provenance.md)) instead of an
  eleven-entry table to source and pin.
- Blending must happen in **linear** space for the same reason the matrix is
  applied in linear space (ADR-0001): a gamma-space lerp is photometrically
  wrong (alternative 2 rejected).

## Consequences / Trade-offs

- **Gain:** a single source matrix per deficiency; trivially exact identity at
  `strength = 0.0`; a continuous, monotone slider for partial CVD.
- **Gain:** the blend is one shared code path across all four color-vision
  filters, so the KAT can verify it once and trust it everywhere
  (`crates/core/tests/color_kat.rs`, `cross_check_*_mid_strength`).
- **Cost:** intermediate `strength` is an *approximation* of anomalous
  trichromacy, not the exact per-severity Machado matrix. The error is largest
  at mid severities, where the true anomalous matrix is not a linear blend of
  identity and the dichromat matrix. We accept this because it is the
  conventional approximation and keeps the matrix set minimal.

## References

- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)": *applies the
  published severity = 1.0 matrix and uses `lerp(original, simulated, strength)`
  in linear space*.
- `crates/core/src/vision/mod.rs` — module docstring (`中間 strength は Machado 自身
  が示唆する通り … DaltonLens 等で広く採用されている方式`); `crates/core/src/vision/color.rs` —
  blend step in `apply_machado_matrix` and `achromatopsia`.
- `CLAUDE.md` — "主要な設計判断": *中間 strength は linear 空間で補間する*.
- `crates/core/tests/color_kat.rs` — `cross_check_protanopia_mid_strength`,
  `cross_check_deuteranopia_mid_strength` (pins the blend at `strength = 0.5`).
