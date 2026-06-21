# 0003 — Disk (pillbox) blur for defocus / refraction, not Gaussian

## Status

Accepted.

## Context

The refraction filters (`myopia`, `hyperopia`, `presbyopia`, and the cylinder
component of `astigmatism`) simulate optical defocus. Defocus needs a blur
kernel. The default reflex in image processing is a Gaussian blur, because it
is separable, cheap, and a good de-noising prior.

The question is which point-spread function (PSF) correctly models a defocused
human eye.

## Decision

Use a **disk (pillbox) blur** — a uniform-density circular kernel — for
isotropic defocus (`myopia` / `hyperopia` / `presbyopia`), computed in linear
sRGB. Do **not** use a Gaussian. For `astigmatism`, use the degenerate
1-dimensional case of the same construction (a directional box filter — the line
spread function of a pure cylinder lens; see also `docs/overview.md`
"Astigmatism").

## Alternatives considered

1. **Gaussian blur** — separable, fast, conventional.
2. **Disk / pillbox blur** (chosen) — uniform circular kernel matching the
   pupil shape.

## Rationale

- A point light source out of focus images on the retina as a **circle of
  confusion**, not a Gaussian falloff. The eye's pupil acts as the aperture, so
  the impulse response (PSF) of a defocused eye is the *shape of the pupil* — a
  uniform-density disk to first approximation. Disk blur is therefore the
  optically correct PSF; Gaussian is not what a defocused eye produces.
- The disk radius is derived from physical optics (Smith–Helmholtz
  approximation; pupil diameter × diopters → angular CoC diameter → radius). A
  Gaussian has no single radius that corresponds to a given diopter of defocus,
  so it could not carry the same physically-grounded `diopter → pixel-radius`
  mapping (see `docs/overview.md` "Diopter → pixel-radius mapping").
- The implementation keeps disk blur affordable: per-row spans of the disk plus
  a horizontal prefix sum give `O(W·H·kernel_height)` instead of the naïve
  `O(W·H·R²)`, so the optical-fidelity choice does not cost an impractical
  runtime.

## Consequences / Trade-offs

- **Gain:** optically faithful defocus — the simulation matches the physics of
  the eye, which is the project's stated priority (fidelity over visual
  exaggeration).
- **Gain:** a single physical model (pupil-as-aperture) drives both the kernel
  shape *and* the radius, so the parameters stay internally consistent.
- **Cost:** disk blur is not separable like a Gaussian, so it needs the prefix-sum
  trick to stay fast; the naïve form is `O(R²)` per pixel.
- **Cost:** a uniform disk has a hard edge and visible ringing on point
  highlights compared with the soft Gaussian — but that ring *is* the real
  circle-of-confusion appearance, so it is a feature, not a defect.

## References

- `docs/overview.md` — refraction section (`circle of confusion (CoC)` … *uses
  disk blur for physical correctness*) and "Diopter → pixel-radius mapping".
- `crates/core/src/vision/mod.rs` — module docstring (`Gaussian は実際の defocus
  blur ではないため採用しない …`); `crates/core/src/vision/refraction.rs` —
  `MYOPIA_MAX_RADIUS_RATIO` and sibling derivation comments;
  `crates/core/src/vision/common.rs` — `MIN_BLUR_RADIUS_PX`.
- `CLAUDE.md` — "主要な設計判断": *焦点・屈折 (Phase 2 / #4) は disk blur
  (pillbox) を linear sRGB で適用 — Gaussian は採用しない*.
