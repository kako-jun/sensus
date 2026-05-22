# sensus overview

`sensus` simulates sensory perception — primarily vision, with hearing on
the roadmap — by applying perceptual filters to ordinary media buffers.
The goal is twofold:

1. **Empathy & education** — let sighted / hearing users *experience*
   what a given condition might look or sound like.
2. **Early-warning primer** — pair each filter with concise medical
   guidance ("if your real vision starts looking like this, see a doctor")
   so the simulation doubles as a self-screening reminder.

Filters take and return `image::DynamicImage` (and, for hearing,
PCM-style audio buffers in a later phase). Video is supported by calling
the same per-frame API in a loop.

## Crate layout

The repository is a Cargo workspace with two crates:

- **`sensus-core`** (`crates/core/`) — pure logic. Filter implementations,
  color-space conversions, kernel definitions. No filesystem, no
  subprocesses, no GUI. Anything that depends on the host environment
  (`image::open`, file writes, audio device I/O) lives elsewhere.
- **`sensus`** (`crates/cli/`) — the CLI binary. Owns `image::open`,
  output encoding, argument parsing (clap), and any future I/O glue.
  Depends on `sensus-core` for all filter math.

```
sensus/
├── Cargo.toml              # [workspace] members = ["crates/core", "crates/cli"]
└── crates/
    ├── core/
    │   ├── Cargo.toml      # crate-type = ["rlib"]
    │   └── src/
    │       ├── lib.rs
    │       ├── vision.rs    # color vision, refraction, visual field, light
    │       ├── hearing.rs   # hearing loss, pitch shift, balance
    │       └── pipeline.rs  # filter composition
    └── cli/
        ├── Cargo.toml      # [[bin]] name = "sensus"
        └── src/
            └── main.rs     # clap-based CLI entry point
```

WebAssembly is **not** a target — sensus is consumed by native apps
(Flutter), most prominently
[universal-experience](https://github.com/kako-jun/universal-experience).
Web frontends are out of scope.

## I/O contract

Every filter exposed by `sensus-core` follows the same shape:

```rust
fn filter(img: DynamicImage, /* filter-specific params */, strength: f32) -> DynamicImage;
```

- `strength` is always normalized to `0.0..=1.0`. `0.0` returns an image
  perceptually identical to the input; `1.0` is the full clinical effect.
- Filters do **not** consult the filesystem and do **not** spawn
  subprocesses. They are pure functions over pixel buffers.
- For video, callers apply the filter per-frame. Filter implementations
  must therefore be deterministic (no internal RNG state that drifts
  between frames).
- Filters that need randomness (e.g. floaters / vitreous opacities)
  accept an explicit seed parameter so successive frames stay coherent.

## Modules

| Module | Phase | Issues | Filters |
|---|---|---|---|
| `vision` | 1, 2, 3 | #2, #3, #4, #5, #6 | color vision deficiency, tetrachromacy, refraction, visual field defects, light / transparency |
| `hearing` | 4 | #7, #8, #9 | hearing loss, pitch shift, balance / vertigo |
| `pipeline` | 4 | #10 | filter composition |

See [`roadmap.md`](roadmap.md) for the per-phase implementation plan.

## Color vision algorithm (Phase 1, #2)

Color vision deficiency simulation uses the
[Machado, Oliveira & Fernandes 2009][machado] physiologically-based model
(IEEE TVCG, DOI: [10.1109/TVCG.2009.113][doi]). The implementation:

- Operates entirely in **linear sRGB** — input pixels are gamma-decoded,
  the simulation is computed, and the result is gamma-encoded back to
  sRGB. Naïve implementations that multiply matrices against gamma-encoded
  sRGB are color-scientifically incorrect.
- Applies the published **severity = 1.0** matrix and uses
  `lerp(original, simulated, strength)` in linear space for intermediate
  `strength` values. This is the linearised approximation of anomalous
  trichromacy that Machado suggests and that DaltonLens et al. adopt.
- Treats `achromatopsia` as a separate path: cone tristimulus values do
  not apply (the cones are dysfunctional), so the filter computes the
  CIE photopic luminance `Y = 0.2126·R + 0.7152·G + 0.0722·B` (BT.709
  primaries, linear) and blends towards `(Y, Y, Y)`. BT.601 luma
  (`0.299/0.587/0.114`, NTSC CRT) is **not** used — it is wrong for
  sRGB content.
- Preserves the alpha channel.

[machado]: https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html
[doi]: https://doi.org/10.1109/TVCG.2009.113

## Focus / refraction algorithm (Phase 2, #4)

`myopia`, `hyperopia`, `presbyopia`, and `astigmatism` simulate refractive
defocus using a **disk (pillbox) blur** in linear sRGB space:

- A point light source falling out of focus on the retina images as a
  **circle of confusion** (CoC), not a Gaussian. The eye's pupil acts as
  the aperture, so the impulse response of a defocused eye is the shape
  of the pupil — a uniform-density disk to first approximation. Gaussian
  blur is a good *de-noising* prior but is **not** what a defocused eye
  produces; sensus uses disk blur for physical correctness.
- All four filters operate in **linear sRGB** (decode → blur → re-encode).
  Convolving gamma-encoded sRGB darkens midtones and is wrong.
- Alpha is preserved (the filter affects color only).
- For each output pixel, the kernel is averaged over the input region with
  **edge replication** at image borders. The implementation precomputes
  per-row spans of the disk / ellipse and a horizontal prefix sum so the
  total cost is `O(W × H × kernel_height)` rather than the naive
  `O(W × H × R²)` — roughly 1 second for `myopia` (`R ≈ 51 px`) on a
  1024 × 1024 image.

### Diopter → pixel-radius mapping

The angular **diameter** of the circle of confusion produced by `D`
diopters of defocus is `pupil_diameter(m) × |D|` radians (small-angle /
Smith–Helmholtz approximation — note this is *diameter*, not radius).
The disk **radius** used for convolution is therefore half of that:

```
radius_rad = 0.5 × pupil_diameter(m) × |D|_max
radius_ratio = radius_rad / image_fov_rad
```

With a 4 mm mesopic pupil (`pupil = 0.004 m`) and an assumed image FOV
of 30° ≈ 0.5236 rad (viewing a ~25 cm print at ~50 cm), `strength = 1.0`
corresponds to:

| Filter | Clinical maximum | θ_diameter | radius (rad) | `min(W, H)` ratio |
|---|---|---|---|---|
| `myopia` | -6 D | 0.024 rad | 0.012 | 0.023 (2.3%) |
| `hyperopia` | +4 D | 0.016 rad | 0.008 | 0.015 (1.5%) |
| `presbyopia` | +3 D add | 0.012 rad | 0.006 | 0.011 (1.1%) |
| `astigmatism` | -3 CD (cylinder) | 0.012 rad | 0.006 | 0.011 (long axis only) |

Intermediate `strength` values scale the radius linearly. Below ~0.5 px
the filter is identity (sub-pixel blur is not perceptible). The
"clinical maximum" column is the upper bound the slider represents — the
real distribution of refractive error is wider, but sensus prioritises
optical fidelity (radius derived from physical optics) over visual
exaggeration.

### Two-dimensional limitation

Real refractive defocus depends on *distance to the object*: with myopia,
distant objects are blurred while near objects stay sharp; with
presbyopia, near objects blur while distant ones stay sharp. Because
sensus operates on flat 2D images with no depth channel, the filter
applies a uniform blur to the whole frame. Calling `myopia(img, 1.0)`,
`hyperopia(img, 1.0)`, and `presbyopia(img, 1.0)` therefore differ only
in radius (not in spatial selectivity). A future extension could accept
a depth map and produce depth-aware defocus.

### Astigmatism: pure cylindrical lens (1D directional blur)

A pure cylindrical refractive error focuses light to a *line* on the
retina (Sturm's conoid: one meridian focuses, the orthogonal one does
not). The optically correct point-spread is therefore a **line spread
function**, i.e. **1D directional blur** in the meridian perpendicular
to the cylinder axis — *not* an elliptical disk.

In sensus the kernel is built as an ellipse where the short axis is
clamped to the sub-pixel floor (`MIN_BLUR_RADIUS_PX = 0.5 px`); this
makes the kernel degenerate into a 1-row directional box filter, which
is the discrete approximation of the line spread function.

`vision::astigmatism(img, strength, axis_deg)` follows the medical
convention where `axis_deg` denotes the **sharp meridian** (the
orientation of the cylinder lens that corrects the astigmatism). The
blurred direction is therefore at `axis_deg + 90°`. Default
`axis = 90°` corresponds to with-the-rule astigmatism (vertical lines
sharp, horizontal lines blurred). `axis_deg` is normalised
modulo 180° (`rem_euclid`); only `NaN` falls back to the 90° default.

Real clinical astigmatism is almost always *compound* (cylinder + a
spherical refractive error), so both meridians are blurred to differing
degrees. sensus models the **pure cylinder** in isolation; compound
astigmatism is expressed by chaining `Astigmatism + Myopia` (or
`+ Hyperopia`) through the upcoming pipeline (Issue #10).

`apply(Filter::Astigmatism, ...)` always uses the default 90° axis;
callers that need a different axis should call `vision::astigmatism()`
directly.

## Visual field defect algorithm (Phase 3a, #5)

`glaucoma`, `macular_degeneration`, `hemianopia`, and `tunnel_vision`
simulate spatial loss of the visual field using **distance-based vignette
masks** computed in linear sRGB space.

### glaucoma

Peripheral field loss radiating inward from the corners. A normalised
radial distance `r` (0 = image centre, 1 = farthest corner) determines
the darkening coefficient:

- `r ≤ inner_r = 1.0 - strength × 0.7`: preserved (multiplier = 1.0)
- `r ≥ outer_r = inner_r + 0.2`: fully darkened (multiplier = 0.0)
- Between the two: smoothstep transition

The pixel multiplier is `1.0 - strength × fade`.

### macular_degeneration

Central scotoma (blind spot). The same radial scheme is inverted — the
*centre* is darkened toward a dark grey (`lum × (1 - strength × 0.95)`)
and the periphery is unchanged:

- `r ≤ inner_r = strength × 0.25`: full scotoma
- `r ≥ outer_r = strength × 0.4`: unchanged
- Between the two: smoothstep

Uses `lerp(original_channel, lum × (1 - strength × 0.95), t)` to blend
desaturation and darkening together.

### hemianopia

Left or right half-field loss. A vertical split at `x = width / 2` with
a `2%` wide smoothstep border darkens the specified side. The `side`
parameter (0.0 = left field lost, 1.0 = right field lost) is linearly
interpolated so intermediate values shade both sides partially.

Pixel multiplier: `1.0 - fade × strength`, where `fade` is derived from
the horizontal smoothstep.

CLI flag: `--side` (default `0.0`).

### tunnel_vision

Severe peripheral constriction (end-stage glaucoma, retinitis pigmentosa).
Identical to `glaucoma` in structure but with a dramatically narrower
preserved centre and sharper transition:

- `inner_r = (1.0 - strength) × 0.5`
- `outer_r = inner_r + 0.05` (cf. glaucoma's 0.2)

At `strength = 1.0` only the single central pixel escapes darkening.

### Medical urgency notes

- 🚨 **hemianopia** (sudden onset): possible stroke — call emergency services immediately.
- ⚠️ **glaucoma**: often asymptomatic until advanced; early treatment is critical.
- ⚠️ **macular_degeneration**: early detection can slow progression.

## Light / transparency algorithm (Phase 3b, #6)

`cataract`, `photophobia`, `nyctalopia`, and `floaters` simulate
aberrations of the eye's optical medium, all in linear sRGB space.

- **cataract**: per-channel attenuation (`R×0.7, G×0.7, B×0.4`) for
  yellowing, plus 8×8 block-hash noise for scatter haze.
- **photophobia**: extracts pixels above BT.709 luminance threshold 0.5,
  applies disk blur, and adds the result back as bloom.
- **nyctalopia**: desaturates via BT.709 lerp (×0.8) and darkens (×0.7).
- **floaters**: places smoothstep-edged blobs at deterministic positions
  derived from a seed and gaze offset, and multiplies them into the image.

CLI flags: `--seed`, `--density`, `--gaze-x`, `--gaze-y`.



- **WebAssembly** — sensus is consumed by native apps; a wasm build adds
  maintenance cost without a clear consumer.
- **Real-time camera feeds inside this crate** — capture and display are
  the host application's responsibility. sensus only transforms buffers.
- **Smell / taste / haptics simulation** — these have no general-purpose
  digital output. They may be discussed as informational content but are
  outside the simulation scope.
