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
path), and [ADR-0008](0008-machado-per-severity-table.md) (the per-severity
table adopted for intermediate `strength`).

## Scope

| Constant (in `crates/core/src/vision/color.rs`) | Kind | Source |
|---|---|---|
| `PROTANOPIA` | 3×3 CVD matrix, severity = 1.0 (regression anchor; see §1b) | Machado 2009 |
| `DEUTERANOPIA` | 3×3 CVD matrix, severity = 1.0 (regression anchor; see §1b) | Machado 2009 |
| `TRITANOPIA` | 3×3 CVD matrix, severity = 1.0 (regression anchor; see §1b) | Machado 2009 |
| `PROTANOMALY_TABLE` | 11×(3×3) CVD matrix table, severity = 0.0..=1.0 step 0.1 | Machado 2009, cross-checked against VIP-Sim |
| `DEUTERANOMALY_TABLE` | 11×(3×3) CVD matrix table, severity = 0.0..=1.0 step 0.1 | Machado 2009, cross-checked against VIP-Sim |
| `TRITANOMALY_TABLE` | 11×(3×3) CVD matrix table, severity = 0.0..=1.0 step 0.1 | Machado 2009, cross-checked against VIP-Sim |
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
>
> **Since #165 (ADR-0008)**, these three consts are no longer read by
> `apply_machado_matrix` at runtime — the per-severity tables in §1b are. They
> are kept as an independent **regression anchor**: a test
> (`{protanomaly,deuteranomaly,tritanomaly}_table_severity1_matches_legacy_const`
> in `crates/core/src/vision/color.rs`) asserts each table's `[10]` entry
> (severity = 1.0) equals the corresponding const here, element-for-element.
> Because the const and the table's last row were transcribed independently,
> this is a genuine cross-check, not a tautology.

### How extracted

The published severity-1.0 matrices were taken **verbatim** — no re-derivation,
no re-fit, no re-scaling. Because Machado et al. already pre-multiplied the
cone physiology into a linear-sRGB→linear-sRGB matrix, the extraction is simply
"copy the published severity-1.0 table". The application pipeline around them is
documented in ADR-0001 and ADR-0008 (intermediate severity resolution) /
ADR-0002 (achromatopsia only, unaffected by #165).

### Tie-break rule when sources disagree

The published paper and the author's supplementary page **carry identical
values**, and the DaltonLens analysis corroborates them, so no tie-break was
required for the current matrices. The standing rule, should a future source disagree:

1. The **IEEE TVCG paper / author's supplementary page is the primary source.**
   If a redistributor's value differs, the original takes precedence.
2. Any discrepancy must be resolved *before* changing a const, and the chosen
   value and the reason must be recorded in this spec (and, if it changes the
   decision, an ADR).

## 1b. The per-severity matrix tables (#165, ADR-0008)

### Source

Same primary source as §1 (Machado, Oliveira & Fernandes 2009, DOI:
10.1109/TVCG.2009.113) — the full **11-entry** severity family (`0.0, 0.1, …,
1.0`) rather than just the `severity = 1.0` endpoint.

Corroborating reference used for this transcription:

- VIP-Sim, `myRecolour.cs` (`T_Protanomaly` / `T_Deuteranomaly` /
  `T_Tritanomaly`, each a `float[11, 3, 3]`) — an independent, publicly
  available Unity implementation of the same Machado 2009 family. Used here to
  **cross-check the transcription**, not as the primary source: the values in
  VIP-Sim's table and the values in Machado's paper / author's supplementary
  page (§1) agree at the `severity = 1.0` endpoint (verified by the regression
  anchor test in §1), which corroborates that VIP-Sim carries the same
  published family rather than a re-fit or approximation.

### Which cells

All 11 entries per deficiency (`PROTANOMALY_TABLE`, `DEUTERANOMALY_TABLE`,
`TRITANOMALY_TABLE` in `crates/core/src/vision/color.rs`), each a `3×3`
`linear sRGB → simulated linear sRGB` matrix at severity `index / 10`.
`table[0]` is the identity matrix (severity 0.0 — Machado's family starts at
no deficiency, so this is definitionally identity rather than a
paper-published cell); `table[10]` equals the corresponding §1 const exactly
(severity 1.0).

### How extracted

Transcribed verbatim from VIP-Sim's `myRecolour.cs` `T_Protanomaly` /
`T_Deuteranomaly` / `T_Tritanomaly` tables (a faithful copy of the Machado
2009 published family), with the `table[10]` entries verified byte-for-byte
against the independently-sourced §1 consts (the pinning test described in
§1's callout). No re-derivation, re-fit, or re-scaling — the same "copy the
published table" extraction as §1, extended to all 11 severities.

### How `strength` resolves to a matrix

See [ADR-0008](0008-machado-per-severity-table.md) "Decision" for the full
resolution algorithm (`resolve_severity_matrix` in `color.rs`): grid points
(`strength` a multiple of `0.1`) return the table entry unchanged; other
values interpolate the two neighboring entries in matrix-element space.

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
- **Since #165 (ADR-0008):** the per-severity table entries used at
  intermediate strength are likewise re-typed as independent literals
  (`SRC_PROTANOMALY_SEV_0_5` / `SRC_DEUTERANOMALY_SEV_0_5` /
  `SRC_TRITANOMALY_SEV_0_2` / `SRC_TRITANOMALY_SEV_0_3`), and a
  matrix-element-space lerp (`lerp_matrix_f64`) is re-implemented inside the
  test (independent of `resolve_severity_matrix`). `strength = 0.5`
  (`cross_check_protanopia_mid_strength` /
  `cross_check_deuteranopia_mid_strength`) pins an exact table grid point (no
  interpolation, per the resolution algorithm); `strength = 0.25`
  (`cross_check_tritanopia_quarter_strength_interpolated`) pins a non-grid
  point, exercising the interpolation path.

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
2. Update the const(s) in `crates/core/src/vision/color.rs` **and** their
   derivation comments — for a per-severity change (§1b) that means updating
   the relevant `*_TABLE` entry/entries; if the change touches `severity =
   1.0`, update **both** the table's `[10]` entry and the corresponding §1
   const (`PROTANOPIA` / `DEUTERANOPIA` / `TRITANOPIA`) so the regression
   anchor test (§1's callout) keeps agreeing with itself.
3. Update this spec (the value table, the source, and the tie-break note).
4. Update the **independent** literal copy in `crates/core/tests/color_kat.rs`
   (`SRC_*` / `BT709`, and for §1b changes the `SRC_*_SEV_*` per-severity
   literals) and recompute the affected `golden_*` u8 anchors offline.
5. If the change alters a *decision* (not just a value), add or supersede an ADR.

Because the implementation const and the test literal are independent copies,
step 2 without step 4 will make the KAT fail — which is exactly the guardrail
that keeps this spec, the code, and the tests in agreement.

## References

- `crates/core/src/vision/color.rs` — `PROTANOPIA` / `DEUTERANOPIA` / `TRITANOPIA` /
  `PROTANOMALY_TABLE` / `DEUTERANOMALY_TABLE` / `TRITANOMALY_TABLE` /
  `LUMA_R/G/B` consts, `resolve_severity_matrix`, and their derivation comments.
- `crates/core/tests/color_kat.rs` — `SRC_*`, `SRC_*_SEV_*`, `BT709`,
  reference pipeline, `golden_*`, and `cross_check_*` tests.
- `docs/overview.md` — "Color vision algorithm (Phase 1, #2)" and "GLSL ES 3.00
  shader source API" (the source-consistency vs self-consistency distinction).
- [ADR-0001](0001-linear-srgb-machado-matrices.md),
  [ADR-0008](0008-machado-per-severity-table.md),
  [ADR-0004](0004-achromatopsia-bt709-photopic.md).
