# Matrix provenance specification

This document records the **provenance** of every numeric color-vision constant
baked into `sensus-core`: which published source it came from, *which cells* were
taken, *how* they were extracted, the tie-break rule when sources disagree, and
*how the values are pinned by tests*. It exists so that any future proposal to
change a matrix can be audited — and so a downstream consumer can trace a number
back to its origin.

Read this alongside [ADR-0001](0001-linear-srgb-machado-matrices.md) (why the
matrices are applied directly in linear sRGB) and
[ADR-0004](0004-achromatopsia-bt709-photopic.md) (the achromatopsia luminance
path).

## Scope

| Constant (in `crates/core/src/vision/color.rs`) | Kind | Source |
|---|---|---|
| `PROTANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `DEUTERANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `TRITANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `LUMA_R` / `LUMA_G` / `LUMA_B` | photopic luminance weights | ITU-R BT.709 / sRGB (CIE Y) |

## 1. The dichromacy matrices (Machado 2009)

### Source

Machado, Oliveira & Fernandes (2009), **"A Physiologically-based Model for
Simulation of Color Vision Deficiency"**, *IEEE Transactions on Visualization
and Computer Graphics (TVCG)*.
DOI: [10.1109/TVCG.2009.113](https://doi.org/10.1109/TVCG.2009.113).

Corroborating references:

- Author's supplementary page (carries the same severity-1.0 matrices verbatim):
  <https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html>
- DaltonLens project (independent analysis / reimplementation of the
  Machado-family model; corroborates the values): <https://daltonlens.org/>

### Which cells

The **severity = 1.0** (full dichromacy) pre-computed matrices, one per
deficiency. Each is the `3×3` operator that maps **linear sRGB → simulated
linear sRGB** (the cone-space physiology is already pre-multiplied into this
matrix — see ADR-0001; there is no separate LMS step).

These are the exact values currently in `crates/core/src/vision/color.rs`:

**`PROTANOPIA`** (type-1, L-cone deficiency)

```
[ 0.152286,  1.052583, -0.204868 ]
[ 0.114503,  0.786281,  0.099216 ]
[-0.003882, -0.048116,  1.051998 ]
```

**`DEUTERANOPIA`** (type-2, M-cone deficiency)

```
[ 0.367322,  0.860646, -0.227968 ]
[ 0.280085,  0.672501,  0.047413 ]
[-0.011820,  0.042940,  0.968881 ]
```

**`TRITANOPIA`** (type-3, S-cone deficiency)

```
[ 1.255528, -0.076749, -0.178779 ]
[-0.078411,  0.930809,  0.147602 ]
[ 0.004733,  0.691367,  0.303900 ]
```

> The crate stores these as `f32`; the literals above are reproduced exactly as
> written in the source const (and re-typed independently in the test — see
> §3). Row order is `[R_out; G_out; B_out]`, column order is `[R_in, G_in,
> B_in]`, i.e. `out = M · in` per pixel in linear sRGB.

### How extracted

The published severity-1.0 matrices were taken **verbatim** — no re-derivation,
no re-fit, no re-scaling. Because Machado et al. already pre-multiplied the
cone physiology into a linear-sRGB→linear-sRGB matrix, the extraction is simply
"copy the published severity-1.0 table". The application pipeline around them is
documented in ADR-0001 and ADR-0002.

### Tie-break rule when sources disagree

The published paper and the author's supplementary page **carry identical
values**, and the DaltonLens analysis corroborates them, so no tie-break was
required for the current matrices. The standing rule, should a future source disagree:

1. The **IEEE TVCG paper / author's supplementary page is the primary source.**
   If a redistributor's value differs, the original takes precedence.
2. Any discrepancy must be resolved *before* changing a const, and the chosen
   value and the reason must be recorded in this spec (and, if it changes the
   decision, an ADR).

## 2. The luminance coefficients (BT.709 / sRGB)

### Source

ITU-R Recommendation **BT.709** luma / sRGB primaries' CIE Y weights:

```
LUMA_R = 0.2126
LUMA_G = 0.7152
LUMA_B = 0.0722
```

### Which cells / why these

These are the photopic luminance (CIE Y) weights for the BT.709 / sRGB
primaries. They are used by `achromatopsia` to collapse a pixel to grayscale
(ADR-0004) and are shared by other luminance-based filters (`photophobia`,
`starbursts`, the photopic term of `nyctalopia`).

**BT.601** luma (`0.299 / 0.587 / 0.114`, NTSC CRT) is deliberately **not**
used: those weights are wrong for sRGB / linear content. This rejection is part
of ADR-0004.

### How extracted

Standard published constants, copied verbatim from the BT.709 / sRGB
specification.

## 3. How these values are verified (pinning)

The constants above are pinned against the source by a known-answer test (KAT),
introduced in **#156**: `crates/core/tests/color_kat.rs`.

The KAT is deliberately built to avoid tautology — it does **not** import the
`vision/color.rs` consts:

- The source matrices are **re-typed as independent literals** in the test
  (`SRC_PROTANOPIA` / `SRC_DEUTERANOPIA` / `SRC_TRITANOPIA`) with the DOI in a
  comment. These are a physically separate copy of the published values, so if
  the `vision/color.rs` const drifts, the test's expectation and the implementation's
  output diverge and the KAT fails.
- The BT.709 weights are likewise re-typed independently (`BT709` in the test).
- The gamma round-trip, matrix multiply, blend, and 8-bit packing are
  **re-implemented inside the test** (a reference pipeline), so the test does
  not call any `vision/` private function.
- A handful of cases are additionally pinned with **offline-computed golden u8
  literals** (`golden_*` tests, exact equality), catching pipeline regressions
  as well as matrix drift.

**Sensitivity, stated honestly:** the KAT verifies the **8-bit quantized
output**. It catches any matrix drift large enough to move a rounded output
channel by `1/255` (golden anchors use exact equality; non-saturated mid-tone
inputs surface coefficient changes of roughly `0.001–0.004`). Sub-`u8`
floating-point drift that leaves every rounded channel unchanged is **out of
scope by design** — the intrinsic limit of an 8-bit-output check.

## 4. Procedure for changing a matrix (auditable improvement path)

A future proposal to change any value in this spec **must**:

1. Identify the new source and cite it (DOI / URL), and apply the tie-break rule
   in §1 if it disagrees with the existing source.
2. Update the const in `crates/core/src/vision/color.rs` **and** its derivation
   comment.
3. Update this spec (the value table, the source, and the tie-break note).
4. Update the **independent** literal copy in `crates/core/tests/color_kat.rs`
   (`SRC_*` / `BT709`) and recompute the affected `golden_*` u8 anchors offline.
5. If the change alters a *decision* (not just a value), add or supersede an ADR.

Because the implementation const and the test literal are independent copies,
step 2 without step 4 will make the KAT fail — which is exactly the guardrail
that keeps this spec, the code, and the tests in agreement.

## References

- `crates/core/src/vision/color.rs` — `PROTANOPIA` / `DEUTERANOPIA` / `TRITANOPIA` /
  `LUMA_R/G/B` consts and their derivation comments.
- `crates/core/tests/color_kat.rs` — `SRC_*`, `BT709`, reference pipeline,
  `golden_*`, and `cross_check_*` tests.
- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)" and "GLSL ES 3.00
  shader source API" (the source-consistency vs self-consistency distinction).
- [ADR-0001](0001-linear-srgb-machado-matrices.md),
  [ADR-0004](0004-achromatopsia-bt709-photopic.md).
