# 0004 — Achromatopsia via BT.709 photopic luminance (not BT.601, not LMS)

## Status

Accepted.

## Context

`achromatopsia` (total color blindness) simulates a viewer with non-functional
cones, who perceives only luminance — the image collapses to grayscale.

Two sub-questions arise:

1. Can the Machado matrix path (ADR-0001) be reused? Achromatopsia is a *cone
   dysfunction*, so cone tristimulus values do not apply — the LMS / cone-matrix
   framing breaks down.
2. Which luminance coefficients produce the grayscale value? The two common
   choices are BT.709 (`0.2126 / 0.7152 / 0.0722`) and BT.601 NTSC luma
   (`0.299 / 0.587 / 0.114`).

## Decision

Treat `achromatopsia` as a **separate path** from the dichromacy matrices.
Compute the **CIE photopic luminance using BT.709 coefficients in linear sRGB**:

```
Y = 0.2126·R + 0.7152·G + 0.0722·B      (R, G, B linear)
```

and blend each channel toward `(Y, Y, Y)` with `strength` in linear space
(ADR-0002). Do **not** use BT.601 luma, and do **not** route through an LMS /
cone matrix.

## Alternatives considered

1. **Reuse a Machado-style cone matrix** for achromatopsia.
2. **BT.601 luma** (`0.299 / 0.587 / 0.114`) for the grayscale value.
3. **BT.709 photopic luminance** in linear sRGB (chosen).

## Rationale

- The cones are dysfunctional, so the tristimulus (LMS / cone-matrix) premise
  does not hold; there is no meaningful cone transform to apply. A dedicated
  luminance-collapse path is the honest model (alternative 1 rejected).
- `0.2126 / 0.7152 / 0.0722` are the **BT.709 / sRGB primaries' CIE Y weights** —
  the correct luminance for sRGB content. BT.601's `0.299 / 0.587 / 0.114` are
  *NTSC CRT* luma weights and are wrong for sRGB / linear content (alternative 2
  rejected).
- Computing `Y` in **linear** sRGB (not gamma space) is consistent with every
  other filter (ADR-0001) and is required for a photometrically correct
  luminance.

## Consequences / Trade-offs

- **Gain:** a correct grayscale luminance for sRGB content; `R == G == B` at
  `strength = 1.0`; white→white and black→black hold exactly.
- **Gain:** the same BT.709 coefficients are reused as `LUMA_R/G/B` across other
  luminance-based filters (e.g. `photophobia`, `starbursts`, `nyctalopia`'s
  photopic term), so there is one luminance convention in the crate.
- **Cost:** achromatopsia does not share the matrix code path, so it is a
  separate function and a separate KAT (`golden_achromatopsia_strength1`,
  `cross_check_achromatopsia`).
- **Cost:** photopic luminance models cone-mediated daylight vision; true
  achromats rely on rod (scotopic) vision, which has a different spectral
  weighting. We deliberately model the *appearance* (a luminance-only image),
  not the rod spectral sensitivity. (Scotopic weighting is used separately by
  `nyctalopia`; see `crates/core/tests/color_kat.rs`.)

## References

- `docs/overview.md` — "Color vision algorithm": *the filter computes the CIE
  photopic luminance Y = 0.2126·R + 0.7152·G + 0.0722·B (BT.709 primaries,
  linear) … BT.601 luma … is not used*.
- `crates/core/src/vision.rs` — module docstring (`## achromatopsia`,
  `BT.601 … は使わない`); `LUMA_R` / `LUMA_G` / `LUMA_B` consts; `achromatopsia`.
- `CLAUDE.md` — "主要な設計判断": *achromatopsia だけは LMS 経路を捨てて
  BT.709 photopic luminance（NTSC 用 BT.601 ではない）でグレースケール化する*.
- [`matrix-provenance.md`](matrix-provenance.md) — provenance of the BT.709
  coefficients.
