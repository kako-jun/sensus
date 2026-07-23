# Matrix provenance specification

This document records the **provenance** of every numeric color-vision constant
baked into `sensus-core`: which published source it came from, *which cells* were
taken, *how* they were extracted, the tie-break rule when sources disagree, and
*how the values are pinned by tests*. It exists so that any future proposal to
change a matrix can be audited — and so a downstream consumer can trace a number
back to its origin.

Read this alongside [ADR-0001](0001-linear-srgb-machado-matrices.md) (why the
matrices are applied directly in linear sRGB),
[ADR-0004](0004-achromatopsia-bt709-photopic.md) (the achromatopsia luminance
path), and [ADR-0005](0005-phased-scope-phase1-four-types.md) (why
tetrachromacy sits outside the colorimetrically-verifiable Phase 1 scope —
relevant background for §3 below).

## Scope

| Constant (in `crates/core/src/vision/color.rs`) | Kind | Source |
|---|---|---|
| `PROTANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `DEUTERANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `TRITANOPIA` | 3×3 CVD matrix, severity = 1.0 | Machado 2009 |
| `LUMA_R` / `LUMA_G` / `LUMA_B` | photopic luminance weights | ITU-R BT.709 / sRGB (CIE Y) |
| `HPE_LMS_HEURISTIC` | 3×3 heuristic matrix, linear RGB → pseudo-LMS (only rows 0–1 used) | Hunt-Pointer-Estévez XYZ→LMS (D65) — **not** Machado 2009; see §3 |

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
> §4). Row order is `[R_out; G_out; B_out]`, column order is `[R_in, G_in,
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

## 3. Heuristic matrices (visualization-only, not colorimetric)

Not every numeric matrix in the crate is a citable, source-anchored
scientific instrument the way §1/§2 are. This section documents such
matrices honestly: what the numbers actually are, where they came from, and
that **no colorimetric fidelity is claimed** for them.

### `HPE_LMS_HEURISTIC` (tetrachromacy, `crates/core/src/vision/color.rs`)

**What it is.** The numeric values

```
[ 0.4002,  0.7076, -0.0808 ]
[-0.2263,  1.1653,  0.0457 ]
[ 0.0000,  0.0000,  0.9182 ]
```

are the **Hunt-Pointer-Estévez (HPE) CIE XYZ→LMS transform, D65
white-point-normalized**. This matrix does **not** appear in Machado,
Oliveira & Fernandes (2009) Table 1 — that table publishes only the three
severity-1.0 dichromacy matrices listed in §1. An earlier revision of this
codebase's comments incorrectly attributed this matrix to "Machado 2009,
Equation 1 / Table 1"; that attribution was wrong and has been corrected
(#170).

**Why it's a heuristic, not a colorimetric LMS conversion.** `tetrachromacy`
applies this matrix directly to **linear RGB**, with no sRGB→CIE XYZ step in
between. A colorimetrically correct LMS conversion requires
`linear RGB → CIE XYZ → LMS`; skipping the XYZ step means the resulting "L"
and "M" values are not true cone tristimulus values — they are a fast,
plausible-looking proxy used only to locate pixels where the red and green
channels are hard to tell apart (a stand-in for a metameric-pair detector).
Only rows 0–1 (L, M) are used by the filter; row 2 (S) is present in the
matrix literal but unused.

**What it's for.** `tetrachromacy` is a **visualization** of "what a
four-cone observer might perceive differently," not a source-anchored
simulation — see [ADR-0005](0005-phased-scope-phase1-four-types.md), which
explicitly scopes tetrachromacy out of the first (colorimetrically
verifiable) phase for exactly this reason. This spec does not claim, and the
filter does not need, colorimetric fidelity from this matrix; it only needs
a repeatable split between "similar red/green" and "distinguishable
red/green" pixels.

**Where it's duplicated.** The same six non-zero coefficients are hardcoded
directly (not read from a named constant, since GLSL has no cross-file
const import) in `crates/core/src/shaders/tetrachromacy.frag`, for the GPU
path. Both copies carry the corrected provenance comment as of #170; the
numeric values themselves are unchanged.

**Verification coverage.** Unlike §1/§2, this matrix is **not** pinned by the
independent-literal known-answer test described in §4 — `color_kat.rs` covers
only `PROTANOPIA` / `DEUTERANOPIA` / `TRITANOPIA` / `LUMA_R/G/B`.
`HPE_LMS_HEURISTIC` is instead covered by the CPU↔GLSL cross-check
(`shader_equiv_tetrachromacy_*` in `crates/core/tests/shader_equivalence.rs`),
which pins CPU/GPU agreement but not agreement against an external source.

## 4. How these values are verified (pinning)

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

## 5. Procedure for changing a matrix (auditable improvement path)

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
  `LUMA_R/G/B` / `HPE_LMS_HEURISTIC` consts and their derivation comments.
- `crates/core/src/shaders/tetrachromacy.frag` — GPU-side hardcoded copy of the
  `HPE_LMS_HEURISTIC` coefficients (§3).
- `crates/core/tests/color_kat.rs` — `SRC_*`, `BT709`, reference pipeline,
  `golden_*`, and `cross_check_*` tests.
- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)" and "GLSL ES 3.00
  shader source API" (the source-consistency vs self-consistency distinction).
- [ADR-0001](0001-linear-srgb-machado-matrices.md),
  [ADR-0004](0004-achromatopsia-bt709-photopic.md),
  [ADR-0005](0005-phased-scope-phase1-four-types.md).
