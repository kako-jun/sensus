# 0008 — Adopt the Machado 2009 per-severity 11-entry matrix table for protanopia / deuteranopia / tritanopia

## Status

Accepted. Supersedes [ADR-0002](0002-linear-blend-intermediate-severity.md).

## Context

ADR-0002 approximated intermediate `strength` (0.0 < strength < 1.0) for
protanopia / deuteranopia / tritanopia by taking the single published
`severity = 1.0` matrix and linearly blending its result with the original
color in linear sRGB: `n = original + (simulated - original) * strength`.
This needs only one matrix per deficiency and is a documented, commonly used
approximation (DaltonLens and others use it too).

However, Machado, Oliveira & Fernandes (2009) also publish a full family of
matrices, one per severity step (`0.0, 0.1, …, 1.0`, 11 entries), for
anomalous trichromacy. A linear blend toward the severity=1.0 endpoint is only
an approximation of that published family — it assumes the true per-severity
matrix lies on the straight line between identity and the severity=1.0
matrix, cell by cell. It does not.

A concrete example: the tritanomaly table's `row0col0` cell is **not
monotonic** across severity — it decreases from `1.0` (severity 0.0) down to
`~0.896` (severity 0.2), then rises past the severity=1.0 endpoint value
(`1.278864` at severity 0.9, versus `1.255528` at severity 1.0) before
settling back down. A straight-line blend from identity to the severity=1.0
matrix cannot reproduce this — it can only move monotonically between the two
endpoints. The gap is worst for tritanopia because the S-cone deficiency
matrices have this non-monotonic, overshoot-then-settle shape; protanopia and
deuteranomaly are comparatively closer to linear but still measurably off.

A competing implementation (VIP-Sim's `myRecolour.cs`, a Unity shader) ships
the full 11-entry table and interpolates between adjacent table entries
instead of blending toward the endpoint, giving it a fidelity edge at
intermediate severities that `sensus` did not have.

### Measured error of the ADR-0002 approximation

Using an independent f64 reference pipeline (gamma decode → matrix apply →
gamma encode → round to u8), we compared the ADR-0002 linear-blend output
against the per-severity-table output across a dense RGB grid (17 steps per
channel, 5843 colors including all KAT named colors) and severities
`0.01..=0.99` in `0.01` steps:

| Deficiency | Max per-channel diff | Mean per-channel diff |
|---|---|---|
| protanopia | 76 / 255 | 3.86 / 255 |
| deuteranopia | 79 / 255 | 4.29 / 255 |
| tritanopia | 111 / 255 | 5.89 / 255 |
| **overall** | **111 / 255** | **4.68 / 255** |

The error is largest for tritanopia (consistent with the non-monotonic cell
above) and non-trivial even for protanopia / deuteranopia — a max
per-channel drift of 76–79/255 is a clearly visible color shift, not a
sub-pixel rounding difference. This is the quantified version of the
"cost" ADR-0002 already flagged qualitatively ("the error is largest at mid
severities, where the true anomalous matrix is not a linear blend").

## Decision

Adopt the full Machado 2009 per-severity table (11 entries per deficiency,
severity `0.0..=1.0` in `0.1` steps, `table[0]` = identity, `table[10]` =
the existing severity=1.0 matrix) for **protanopia / deuteranopia /
tritanopia only**. Resolve `strength` to a matrix by:

1. `scaled = strength * 10`, `i0 = floor(scaled)`, `i1 = i0 + 1`,
   `frac = scaled - i0`.
2. If `strength` lands exactly on a table grid point (`frac == 0`, i.e.
   `strength` is a multiple of `0.1`), return `table[i0]` **unchanged** — no
   interpolation arithmetic, so grid points are bit-for-bit the table entry
   (this is what lets `strength = 0.5` KAT against `table[5]` with zero
   floating-point slop).
   At `strength = 1.0` this returns `table[10]`, which is **numerically
   (real-number) equivalent** to the pre-#165 output — the old formula
   computed `n = v + (matrix·v - v) * 1.0`, which is `matrix·v` algebraically.
   It is not bit-identical in all cases: `f32` addition/subtraction is not
   associative, so `v + (s - v)` can differ from `s` by 1 ULP for some inputs.
   An exhaustive 256³ (16,777,216 colors) sweep at `strength = 1.0`
   (`crates/core/tests/color_severity1_full_sweep.rs`, `#[ignore]`, run
   manually) measured this drift precisely:

   | Deficiency | Mismatched pixels (of 16,777,216) | Max diff |
   |---|---|---|
   | protanopia | 28 | 1 LSB |
   | deuteranopia | 11 | 1 LSB |
   | tritanopia | 6 | 1 LSB |

   In every one of these cases the difference is exactly ±1 on a single
   channel — never larger, and affecting roughly 1 pixel in 600,000 to
   1,500,000 at most. This is a rounding artifact of eliminating the
   redundant blend arithmetic (the new formula is, if anything, *more*
   numerically direct — one rounding step instead of two), not a fidelity
   regression.
3. Otherwise, linearly interpolate `table[i0]` and `table[i1]` **in matrix
   element space**: `M = table[i0] + (table[i1] - table[i0]) * frac`.

Then apply the resolved matrix `M` directly to the linear sRGB pixel — there
is no additional blend-with-original step, because the table already encodes
the correct behavior at both ends (`table[0]` is exactly identity,
`table[10]` is exactly the severity=1.0 matrix).

This is mathematically equivalent to interpolating the two *matrix-applied
results* rather than the matrix cells (matrix multiplication is linear in the
matrix's coefficients), so this ADR does not introduce a second visually
distinct interpolation scheme — it just resolves the matrix once per call
instead of applying two matrix multiplies and a blend per pixel.

`achromatopsia` is **out of scope** and keeps the ADR-0002 approach
unchanged: its `strength` blends toward a BT.709-luminance grayscale target
rather than through a per-severity matrix family, and no equivalent published
per-severity table exists for it (ADR-0004).

## Alternatives considered

1. **Keep the ADR-0002 linear blend** (status quo) — rejected: quantified
   error above is large enough to be a fidelity gap against a competitor
   (VIP-Sim) that already ships the full table.
2. **Re-fit a smoother analytic curve** (e.g. a cubic through the 11 points)
   — rejected: invents a curve not published by Machado et al.; the 11-entry
   table *is* the published data, so piecewise-linear interpolation between
   its own points is the most literal, most auditable way to use it (no
   extra curve-fitting assumptions to defend).
3. **Adopt the 11-entry table, interpolate in matrix-applied-result space per
   pixel** (apply both `table[i0]` and `table[i1]`, blend the two outputs) —
   mathematically identical to the chosen approach (matrix multiplication is
   linear) but twice the per-pixel matrix work for no fidelity gain. Rejected
   on cost grounds.
4. **Adopt the 11-entry table, interpolate in matrix element space, apply
   once** (chosen) — cheapest correct implementation of the published table.

## Rationale

- The 11-entry table is Machado et al.'s own published data — using it
  directly (instead of approximating with a 2-point blend) removes an
  approximation this project was carrying without a strong reason once the
  fidelity cost was quantified.
- VIP-Sim's `myRecolour.cs` uses the identical published values (its
  `T_Protanomaly` / `T_Deuteranomaly` / `T_Tritanomaly` `float[11,3,3]`
  tables) and the identical index/lerp resolution scheme (`index =
  severity * 10`, floor/ceil, `Mathf.Lerp` per cell) — cross-checking against
  it corroborates both the values (`matrix-provenance.md`) and the
  interpolation method, so this is not a novel invention either.
- Matrix-element-space interpolation is the cheaper of the two mathematically
  equivalent options (one matrix multiply per pixel instead of two), and
  keeping identity/severity=1.0 as exact grid entries (no interpolation
  arithmetic at `frac == 0`) preserves the `strength = 0.0` byte-exact
  contract, and keeps `strength = 1.0` numerically equivalent to the
  pre-#165 output (real-number equal; `f32` non-associativity can move a
  handful of pixels by ±1 LSB — see the sweep numbers above) without any
  special-casing beyond what already existed.

## Consequences / Trade-offs

- **Gain:** intermediate severities now match the published Machado 2009
  family instead of a 2-point linear approximation — closes the fidelity gap
  identified against VIP-Sim, most visible for tritanopia (measured max
  111/255 error under the old approach).
- **Gain:** `strength = 0.0` remains byte-identical to before, and
  `strength = 1.0` remains byte-identical for **all but a handful of inputs**
  (28 / 16,777,216 for protanopia, 11 for deuteranopia, 6 for tritanopia —
  see the Decision section; the rare exceptions are a `±1 LSB` `f32`
  rounding artifact, not a behavior change) — this ADR's intended behavior
  change is strictly *between* the endpoints.
- **Cost:** 10x more constants to source, transcribe, and audit per
  deficiency (11-entry table vs. 1 matrix) — the transcription is guarded by
  a regression test asserting `table[10]` equals the pre-existing
  independently-typed severity=1.0 const (`crates/core/src/vision/color.rs`),
  and by `matrix-provenance.md`.
- **Cost:** the CPU matrix-resolution step (index/floor/lerp) now runs once
  per filter call instead of being folded into the per-pixel blend; this is a
  net perf *win* (one matrix resolve + one matrix multiply per pixel, versus
  one matrix multiply + one blend per pixel before), not a cost.
- **No cost to `achromatopsia`** — unaffected, ADR-0002 continues to govern
  it exactly as before.

## References

- `crates/core/src/vision/color.rs` — `PROTANOMALY_TABLE` /
  `DEUTERANOMALY_TABLE` / `TRITANOMALY_TABLE` consts, `resolve_severity_matrix`,
  `apply_machado_matrix`.
- `crates/core/src/shaders.rs` — `protanopia_uniforms` / `deuteranopia_uniforms`
  / `tritanopia_uniforms` (GLSL parity: the CPU resolves the matrix, the
  `.frag` applies it directly without its own blend step).
- `crates/core/tests/color_kat.rs` — `SRC_PROTANOMALY_SEV_0_5` /
  `SRC_DEUTERANOMALY_SEV_0_5` / `SRC_TRITANOMALY_SEV_0_2` /
  `SRC_TRITANOMALY_SEV_0_3`, `cross_check_protanopia_mid_strength`,
  `cross_check_deuteranopia_mid_strength`,
  `cross_check_tritanopia_quarter_strength_interpolated`.
- `crates/core/tests/color_severity1_full_sweep.rs` — `#[ignore]`d exhaustive
  256³ sweep measuring the `strength = 1.0` old-vs-new `±1 LSB` drift cited
  above (run manually: `cargo test --release --test
  color_severity1_full_sweep -- --ignored --nocapture`).
- `docs/adr/matrix-provenance.md` — table provenance and VIP-Sim
  cross-check.
- [ADR-0002](0002-linear-blend-intermediate-severity.md) (superseded by this
  ADR for protanopia/deuteranopia/tritanopia; still governs achromatopsia),
  [ADR-0001](0001-linear-srgb-machado-matrices.md).
- Issue #165.
