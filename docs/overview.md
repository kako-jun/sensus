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
    │       ├── vision.rs    # color vision, refraction, visual field, light, depth-aware blur, diplopia, nystagmus, starbursts
    │       ├── hearing.rs   # hearing loss, pitch shift, balance
    │       ├── shaders.rs   # GLSL ES 3.00 shader sources + uniform structs
    │       ├── shaders/     # *.frag shader source files (included via include_str!)
    │       ├── stereo.rs    # MPO stereo split + SAD disparity → depth map
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
| `vision` | 1–5 | #2, #3, #4, #5, #6, #19, #29, #36, #37 | color vision deficiency, tetrachromacy, refraction, visual field defects, light / transparency, depth-aware blur, diplopia, nystagmus, starbursts, eye_strain, dry_eye |
| `hearing` | 4 | #7, #8, #9 | hearing loss, pitch shift, balance / vertigo |
| `stereo` | 6 | #31, #32 | MPO stereo photography → depth map (`split_mpo`, `stereo_to_depth`); Android XMP Depth extraction (`read_xmp_depth`) |
| `pipeline` | 4 | #10 | filter composition ✅ |
| `shaders` | 5 | #16 | GLSL ES 3.00 shader sources + uniform structs for all visual filters |

## Pipeline (Phase 4, #10)

`Pipeline` chains multiple filters sequentially over a single image.

```rust
use sensus_core::{Filter, pipeline::{Pipeline, FilterStep}};

let result = Pipeline::new()
    .push(FilterStep::new(Filter::Myopia, 1.0))
    .push(FilterStep::new(Filter::Cataract, 0.8))
    .apply(img)?;
```

- **Builder pattern**: `Pipeline::push()` takes ownership and returns `self`, enabling chaining.
- **Order matters**: filters are applied left-to-right. `A → B` and `B → A` generally produce different results.
- **Per-step parameters**: `FilterStep` carries filter-specific params (`axis`, `seed`, `density`, `gaze_x`, `gaze_y`, `side`) with sensible defaults. Set them directly on the struct after construction.
- **Error propagation**: if a step fails, `Error::Pipeline { step, filter, source }` reports which step index and filter name caused the error.
- **CLI**: pass `--filter` multiple times: `sensus -i in.png -o out.png --filter myopia --filter cataract`


## Eye Fatigue filters (Phase 4, #36)

`sensus_core::vision` includes two eye fatigue filters:

- **`eye_strain`**: Simulates visual fatigue through contrast compression
  (`v' = 0.5 + (v - 0.5) × (1 - strength × 0.15)`) in linear sRGB, a light
  disk blur (`radius = strength × 1.5 px`), and a peripheral vignette using
  `smoothstep(0.3, 1.2, d)` where `d = uv·uv` with `uv ∈ [-1, 1]²`. Both CPU
  and GLSL implementations operate in linear sRGB space and apply identical
  vignette math, verified by PSNR ≥ 30 dB equivalence test.
- **`dry_eye`**: Applies random per-tile disk blur (tile = 32×32 px). Each
  tile's blur radius is determined by a fixed-seed (42) LCG. Only the tile
  region (with a blur-radius overlap) is blurred rather than the full image,
  making processing O(tile_area × kernel_height) instead of O(W×H × kernel_height).
  Because the blur radius varies spatially per tile with a fixed seed, this
  filter is not amenable to a GLSL equivalence test.

## Auditory Processing Disorder (APD) (Issue #38)

APD simulation is deferred to a later phase. It would be implemented in
`sensus_core::hearing` alongside existing hearing filters and would simulate
difficulty distinguishing speech in noisy environments.

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

## Tetrachromacy algorithm (Phase 1+, #3)

`tetrachromacy` approximates what a tetrachromat might perceive by
exaggerating color differences that trichromats cannot distinguish.

### Fundamental limitation

RGB cameras and displays capture only 3 channels; the fourth spectral
dimension that a tetrachromat's extra cone type would sense is not
recorded. A physically exact simulation is **impossible from RGB input**.
The filter instead renders a visualization: "if a difference existed here,
it might look like this."

### Algorithm

1. Decode each pixel to **linear sRGB** (gamma removal).
2. Compute **opponent channels**:
   - `rg = R − G` (red–green axis; most relevant to the tetrachromat's
     extra L/M cone overlap near 560 nm)
   - `yb = 0.5×(R+G) − B` (yellow–blue axis)
3. Exaggerate each axis scaled by `strength`:
   - `R_out = R + strength × rg × k_rg` (`k_rg = 0.5`)
   - `G_out = G − strength × rg × k_rg`
   - `B_out = B + strength × yb × k_yb` (`k_yb = 0.25`, subtler)
4. Clamp each channel to `0.0..=1.0`.
5. Re-encode to sRGB (gamma application).
6. Alpha is preserved.

**Uniform colours** (R = G = B) produce `rg = yb = 0` and are therefore
unchanged regardless of `strength`. The effect is visible only where
hue differences already exist in the source image.



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
  ⚠️ 即受診 — 急激な視力低下・視野変化は眼科受診推奨。
- **photophobia**: extracts pixels above BT.709 luminance threshold 0.5,
  applies disk blur, and adds the result back as bloom.
- **nyctalopia**: desaturates via BT.709 lerp (×0.8) and darkens (×0.7).
  ⚠️ 早期受診 — 夜盲の急激な悪化はビタミンA欠乏・網膜色素変性の可能性。
- **floaters**: places smoothstep-edged blobs at deterministic positions
  derived from a seed and gaze offset, and multiplies them into the image.

CLI flags: `--seed`, `--density`, `--gaze-x`, `--gaze-y`.

## Motion / visual-optics filters (Phase 5, #29)

`diplopia`, `nystagmus`, and `starbursts` add a third category of visual
simulation beyond spatial field defects and optical blur.

- **diplopia**: copies the source image, translates it by
  `(offset_x × min(W,H), offset_y × min(W,H))` pixels, and alpha-blends
  the ghost at opacity `ghost_strength × strength` in linear sRGB.
  Simulates double vision from strabismus or cranial nerve palsy.
  CLI: `--offset-x`, `--offset-y`, `--ghost-strength`.
  🚨 即救急 — 突然の複視は動眼神経麻痺・脳幹梗塞の可能性。
- **nystagmus**: applies 1D directional blur (`amplitude × strength ×
  min(W,H)` px radius, `direction_deg` in degrees) as a static snapshot of
  the motion blur caused by involuntary oscillatory eye movement.
  `direction_deg = 0°` is horizontal (most common). CLI: `--amplitude`,
  `--direction-deg`.
- **starbursts**: for each pixel whose BT.709 linear luminance exceeds
  `threshold`, emits `num_rays` radial rays of length
  `ray_length_ratio × min(W,H)` pixels. Each ray decays linearly with
  distance and is additively composited onto the image in linear sRGB.
  Simulates the starburst / spike artefact visible after LASIK, IOL
  implantation, or in high uncorrected astigmatism.
  CLI: `--num-rays`, `--ray-length`, `--threshold`.

## Depth-aware blur (Phase 5, #19)

`vision::depth_aware_blur(img, depth_map, focus_depth, max_radius_ratio, kind)`
accepts a greyscale PNG depth map (bright = near, dark = far) alongside the
source image and applies per-pixel disk blur whose radius scales with the
distance from `focus_depth`:

- `DepthBlurKind::Myopia` — pixels with `depth < focus_depth` (far) blur;
  near pixels stay sharp.
- `DepthBlurKind::Hyperopia` — pixels with `depth > focus_depth` (near)
  blur; far pixels stay sharp.
- `DepthBlurKind::DepthOfField` — both sides blur; simulates camera DoF.

If the depth map dimensions differ from the source image, it is
automatically resized with Lanczos3 before processing. This extends the
uniform-blur refraction filters in #4 to spatially-varying defocus for
scenes with a known depth channel (stereo photography, portrait-mode JPEG,
Depth Anything V2 output, etc.).

## Stereo photography depth map generation (Phase 6, #31)

`sensus_core::stereo` converts a stereo image pair into a greyscale depth
map that can be fed directly into `depth_aware_blur`.

```rust
use sensus_core::stereo::{split_mpo, stereo_to_depth};

let mpo_bytes = std::fs::read("photo.mpo")?;
let (left, right) = split_mpo(&mpo_bytes)?;
let depth_map = stereo_to_depth(&left, &right)?;
let result = depth_aware_blur(left, &depth_map, 0.5, 0.023, DepthBlurKind::Myopia)?;
```

**`split_mpo(data: &[u8]) -> Result<(DynamicImage, DynamicImage)>`**

MPO (Multi-Picture Object) is a JPEG superset used by Nintendo 3DS,
PlayStation 3D cameras, and some Android devices. The file embeds left-eye
and right-eye JPEG streams back-to-back; `split_mpo` scans for the
`FFD9 FFD8` (EOI + SOI) boundary and decodes each stream independently.
Returns `Error::InvalidMpo` if no second stream is found.

**`stereo_to_depth(left, right) -> Result<DynamicImage>`**

Computes a disparity map via block-matching SAD (Sum of Absolute
Differences): `BLOCK_SIZE = 7`, `MAX_DISPARITY = 64`. Each pixel's best
horizontal shift (left→right) is mapped to a brightness value — brighter
means closer. Returns `Error::SizeMismatch` if left and right have
different dimensions.

**CLI integration:**

```bash
sensus --filter myopia-depth --mpo photo.mpo --focus 0.5 -o output.png
```

`--mpo <PATH>` auto-generates the depth map from the stereo pair and
applies depth-aware blur to the left-eye image. `--mpo` and `--depth`
are mutually exclusive; only one depth blur filter may be active at a time.

**`read_xmp_depth(data: &[u8]) -> Result<DynamicImage>`** (#32)

Extracts a depth map from an Android portrait-mode JPEG that carries the
Google Depth API XMP extension. The function scans every `APP1` (0xFFE1)
segment in the JPEG byte stream for `GDepth:Data`, decodes the embedded
base64 PNG or JPEG, and returns it as a `DynamicImage`. If no `GDepth:Data`
field is present the function returns `Error::NoDepthMap`.

```rust
use sensus_core::stereo::read_xmp_depth;

let jpeg_bytes = std::fs::read("portrait.jpg")?;
let depth_map = read_xmp_depth(&jpeg_bytes)?;
let result = depth_aware_blur(
    image::load_from_memory(&jpeg_bytes)?,
    &depth_map, 0.5, 0.023, DepthBlurKind::Myopia
)?;
```

**CLI integration:**

```bash
sensus --filter myopia-depth --portrait portrait.jpg --focus 0.5 -o output.png
```

`--portrait <PATH>` reads the JPEG, extracts the XMP depth map, and applies
depth-aware blur. If `--input` is also given the input image is used as the
source; otherwise the portrait file itself is the source. `--portrait` is
mutually exclusive with `--mpo` and `--depth`.

## Hearing filters (Phase 4, #7–#9)

`sensus_core::hearing` is a pure-function audio processing module that
mirrors the `vision` module's design philosophy: every filter takes a buffer
and returns a buffer; no audio device I/O.

- **`AudioBuffer`**: f32 interleaved PCM with explicit sample rate and
  channel count.
- **`BiquadFilter`**: second-order IIR building block (Butterworth
  approximation) used by all hearing filters.
- **10 hearing filters**: `hearing_loss`, `sudden_deafness`,
  `noise_induced_loss` (volume/frequency), `tinnitus`, `diplacusis`,
  `hyperacusis`, `amusia`, `presbycusis`, `recruitment`,
  `temporary_threshold_shift` (quality/pitch), returned as processed
  `AudioBuffer`. All are stateless over frames — callers supply a fresh
  buffer per chunk.
- **3 vestibular–visual filters** added to `vision.rs`: `vertigo` (rotating
  radial warp), `bppv_rotation` (brief rotational jerk), `vestibular_neuritis`
  (sustained horizontal tilt). These are image-space effects; no audio I/O.
  🚨 即救急 — 突然の激しいめまいは脳卒中との鑑別が必要（`vestibular_neuritis`）。

## GLSL ES 3.00 shader source API (Phase 5, #16)

`sensus_core::shaders` provides `*_glsl() -> &'static str` for each visual
filter and matching `*_uniforms()` helpers that compute ready-to-upload
uniform structs. All shaders target GLSL ES 3.00 (`#version 300 es`) for
compatibility with Flutter's `FragmentProgram` API.

The CPU implementation is the normative specification; shaders replicate the
same math. A GPU-free software equivalence test suite (`#17`) asserts that
CPU and shader outputs agree within ≤ 2/255 per channel (matrix filters) or
PSNR ≥ 30 dB (blur / directional filters). — sensus is consumed by native apps; a wasm build adds
  maintenance cost without a clear consumer.
- **Real-time camera feeds inside this crate** — capture and display are
  the host application's responsibility. sensus only transforms buffers.
- **Smell / taste / haptics simulation** — these have no general-purpose
  digital output. They may be discussed as informational content but are
  outside the simulation scope.
