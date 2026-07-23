# sensus overview

`sensus` simulates sensory perception — vision and hearing — by applying
perceptual filters to ordinary media buffers (images and audio). The goal
is twofold:

1. **Empathy & education** — let sighted / hearing users *experience*
   what a given condition might look or sound like.
2. **Early-warning primer** — pair each filter with concise medical
   guidance ("if your real vision starts looking like this, see a doctor")
   so the simulation doubles as a self-screening reminder.

Vision filters take and return `image::DynamicImage`; hearing filters take
and return PCM-style audio buffers (`hearing::AudioBuffer`). Video is
supported by calling the same per-frame API in a loop.

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
    │       ├── lib.rs       # Filter / HearingFilter enums, apply(), Experience, Urgency
    │       ├── vision/      # 28 vision filters split by domain (color/refraction/field/light/motion/fatigue/phenomena) + common helpers; mod.rs re-exports all as vision::*
    │       ├── hearing.rs   # 14 hearing filters (loss, tinnitus, pitch, APD, Ménière, labyrinthitis, …)
    │       ├── shaders.rs   # GLSL ES 3.00 shader sources + uniform structs
    │       ├── shaders/     # *.frag shader source files (included via include_str!)
    │       ├── stereo.rs    # MPO stereo split + SAD disparity → depth map
    │       └── pipeline.rs  # filter composition
    └── cli/
        ├── Cargo.toml      # [[bin]] name = "sensus"
        └── src/
            ├── main.rs          # orchestration / I/O: main, run, run_audio, run_pipe, RunError
            ├── arguments.rs     # clap Cli struct + Filter/Hearing ValueEnums + parse_* validators
            ├── filter_mapping.rs # CLI enum → core enum mapping + warn_unused_flags
            ├── depth_resolver.rs # depth blur integration + pipeline apply helpers
            └── audio.rs         # WAV ↔ AudioBuffer I/O (hearing mode)
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
| `vision` | 1–5 | #2, #3, #4, #5, #6, #19, #29, #36, #37, #55, #56, #57, #58, #59 | color vision deficiency, tetrachromacy, refraction, visual field defects, light / transparency, balance / vertigo, eye fatigue / dry eye, depth-aware blur, diplopia, nystagmus, starbursts, metamorphopsia, contrast_sensitivity, detail_loss, teichopsia, flickering_stars |
| `hearing` | 4 | #7, #8, #9, #102, #103, #104 | 14 hearing filters: hearing loss, tinnitus, pitch shift, diplacusis, APD, misophonia, Ménière, labyrinthitis, … (audio buffers) |
| `lib` (`apply`, `Experience`, `Urgency`) | 4 | #103, #104 | dispatch facade + composite vision+hearing experiences with `Urgency` classification |
| `stereo` | 6 | #31, #32 | MPO stereo photography → depth map (`split_mpo`, `stereo_to_depth`); Android XMP Depth extraction (`read_xmp_depth`) |
| `pipeline` | 4 | #10, #105 | vision filter composition (`Pipeline`) + hearing chain (`AudioPipeline`) ✅ |
| `shaders` | 5 | #16, #107, #134 | GLSL ES 3.00 shader sources + uniform structs for all visual filters |

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
  contrast, vignette, and disk-blur math (the GLSL approximates the CPU pillbox
  with a 16-tap Fibonacci lattice), verified by PSNR ≥ 30 dB equivalence test.
- **`dry_eye`**: Applies random per-tile disk blur (tile = 32×32 px) in linear
  sRGB space. Each tile's blur radius is `noise × strength × 3 px`, where
  `noise ∈ [0,1]` comes from a fixed-seed (42) 32-bit integer spatial hash of
  the tile coordinates. (#99) The CPU and GLSL implementations now share the
  identical hash and isotropic disk (pillbox) membership (`dx²+dy² ≤ r²`,
  edge clamp), so the filter is verified by a CPU↔GLSL equivalence test
  (PSNR ≥ 30 dB; byte-exact on the test fixtures). The earlier sequential-LCG
  noise (which depended on tile-scan order and could not be reproduced by a
  parallel fragment shader) was replaced by the per-tile spatial hash.

## Contrast Sensitivity filter (#56)

`contrast_sensitivity(img, strength)` compresses luminance contrast toward the
midpoint (0.5) in linear sRGB space.

Formula: `output = 0.5 + (input − 0.5) × (1.0 − strength × 0.5)`

- `strength = 0.0` → identity (same as source image)
- `strength = 1.0` → 50% contrast compression (output luminance variance < input)

## Teichopsia filter (#58)

`teichopsia(img, strength)` simulates the fortification spectra (zigzag luminance
arcs) seen as a migraine aura.

- Ring region (normalized distance 0.2–0.5 from center): additive saw-wave brightness overlay
- Inner scotoma (distance < 0.2): darkened by `strength × 0.7`
- `strength = 0.0` → identity; `strength = 1.0` → full effect

> **Medical note** (⚠️ early consultation): typically a migraine aura lasting
> 20–30 min. On a first-ever episode, see an ophthalmologist / neurologist.

## Flickering Stars filter (#59)

`flickering_stars(img, strength, seed)` simulates photopsia (flashes of light)
by additively blending random white blob points onto the image.

- Point count = `(strength × 200.0) as usize` (200 points at strength=1.0)
- Each point is a 2 px rectangular blob with additive luminance 0.5–1.0
- `strength = 0.0` → identity (zero points); `strength = 1.0` → 200 white blobs

> **Medical note** (🚨 emergency): a sudden surge of flashes with a
> curtain-like field loss can signal retinal detachment — seek care immediately.

## Metamorphopsia filter (#55)

`metamorphopsia(img, strength, freq, seed)` simulates the wavy/warped vision
of macular distortion by displacing each pixel along a smooth pseudo-random
vector field.

- `freq`: spatial frequency of the distortion field (higher = finer ripples).
  `apply(Filter::Metamorphopsia { freq, seed })` and the CLI default to `4.0`.
- `seed`: LCG seed for the distortion field, so successive video frames stay
  coherent (CLI `--meta-seed`, default `0`).
- `strength = 0.0` → identity; `strength = 1.0` → maximum displacement.

> **Medical note** (⚠️ early consultation): new or worsening straight-line
> distortion (an Amsler-grid finding) can indicate macular disease (AMD,
> macular edema) — see an ophthalmologist.

## Auditory Processing Disorder (APD) (Issue #37)

APD simulation is implemented in `sensus_core::hearing` as
`auditory_processing_disorder(buf, strength)`. It approximates the
difficulty of distinguishing speech in noisy environments through three
stages:

1. White-noise injection (LCG seed 42) proportional to `strength`.
2. Weighted FIR smearing over adjacent 3 samples (temporal resolution
   reduction).
3. Gap-filling: silent intervals shorter than ~5 ms are bridged by
   interpolation from surrounding samples.

`strength = 0.0` is sample-exact identity; `strength = 1.0` is the full
clinical effect.

See [`roadmap.md`](roadmap.md) for the per-phase implementation plan.

## Color vision algorithm (Phase 1, #2)

Color vision deficiency simulation uses the
[Machado, Oliveira & Fernandes 2009][machado] physiologically-based model
(IEEE TVCG, DOI: [10.1109/TVCG.2009.113][doi]). The implementation:

- Operates entirely in **linear sRGB** — input pixels are gamma-decoded,
  the simulation is computed, and the result is gamma-encoded back to
  sRGB. Naïve implementations that multiply matrices against gamma-encoded
  sRGB are color-scientifically incorrect.
- For protanopia / deuteranopia / tritanopia, resolves `strength` against the
  full published **per-severity table** (11 entries, severity `0.0..=1.0` in
  `0.1` steps; `table[0]` = identity, `table[10]` = the severity = 1.0
  matrix) instead of a two-point blend: grid-point strengths return the
  matching table entry unchanged, other strengths interpolate the two
  neighboring entries in matrix-element space, then the resolved matrix is
  applied directly (#165, ADR-0008). This matches the full Machado 2009
  family more closely than blending toward the severity = 1.0 endpoint;
  the largest error under the old two-point blend was ~111/255 (tritanopia,
  whose per-severity matrices are not monotonic between identity and
  severity = 1.0).
- Treats `achromatopsia` as a separate path: cone tristimulus values do
  not apply (the cones are dysfunctional), so the filter computes the
  CIE photopic luminance `Y = 0.2126·R + 0.7152·G + 0.0722·B` (BT.709
  primaries, linear) and blends towards `(Y, Y, Y)` with
  `lerp(original, simulated, strength)` in linear space — no published
  per-severity table exists for this path, so it still uses the ADR-0002
  linear blend. BT.601 luma (`0.299/0.587/0.114`, NTSC CRT) is **not**
  used — it is wrong for sRGB content.
- Preserves the alpha channel.

The rationale for these choices (linear sRGB, direct Machado matrices,
per-severity table resolution, achromatopsia's linear blend, BT.709 photopic
luminance) is recorded canonically in [`adr/`](adr/) — see
[ADR-0001](adr/0001-linear-srgb-machado-matrices.md),
[ADR-0008](adr/0008-machado-per-severity-table.md) (supersedes
[ADR-0002](adr/0002-linear-blend-intermediate-severity.md) for the three
dichromacy filters; ADR-0002 still governs achromatopsia), and
[ADR-0004](adr/0004-achromatopsia-bt709-photopic.md). The provenance of the
matrices and luminance coefficients is in
[`adr/matrix-provenance.md`](adr/matrix-provenance.md).

[machado]: https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html
[doi]: https://doi.org/10.1109/TVCG.2009.113

## Tetrachromacy algorithm (Phase 1+, #3)

`tetrachromacy` approximates what a tetrachromat might perceive by detecting
**metameric-pair candidates** — pixels whose red and green channels a
trichromat would tend to confuse — and exaggerating their chroma, plus a
baseline red–green opponent exaggeration applied everywhere else.

### Fundamental limitation

RGB cameras and displays capture only 3 channels; the fourth spectral
dimension that a tetrachromat's extra cone type would sense is not
recorded. A physically exact simulation is **impossible from RGB input**.
The filter instead renders a visualization: "if a difference existed here,
it might look like this." **No colorimetric fidelity is claimed** for any
step of this algorithm, including the LMS-like values in step 2 below — see
the "Heuristic matrices" section of
[`adr/matrix-provenance.md`](adr/matrix-provenance.md).

### Algorithm

1. Decode each pixel to **linear sRGB** (gamma removal).
2. Compute a **pseudo-LMS** `L`/`M` pair: apply the `HPE_LMS_HEURISTIC`
   matrix (the Hunt-Pointer-Estévez XYZ→LMS transform, D65-normalized) directly
   to the linear RGB triple, with **no sRGB→CIE XYZ step** in between. This is
   a fast heuristic proxy, not a colorimetric LMS conversion — the resulting
   `L`/`M` are not true cone tristimulus values, only a repeatable stand-in
   used to locate likely metameric pairs (the matrix's third, `S`, row is
   present but unused).
3. Compute the metameric indicator `delta = M − L`.
4. **Baseline branch** (always computed first *in the CPU reference
   implementation*; the GLSL shader instead takes an equivalent if/else,
   computing only one branch per pixel): red–green opponent exaggeration
   `rg = R − G`, scaled by `strength`:
   - `R_out = R + strength × rg × k_rg` (`k_rg = 0.5`)
   - `G_out = G − strength × rg × k_rg`
   - `B_out = B` (unchanged)
5. **Metameric-pair override**: if `|delta| < 0.05` (a metameric-pair
   candidate region), replace the baseline result with a Cb/Cr chroma
   exaggeration around the pixel's BT.709 luma `Y`:
   - `Cb = B − Y`, `Cr = R − Y`, `scale = strength × 2.0`
   - `R_out = Y + Cr × scale`, `G_out = Y`, `B_out = Y + Cb × scale`
6. Clamp each channel to `0.0..=1.0`.
7. Re-encode to sRGB (gamma application).
8. Alpha is preserved.

**Uniform colours** (R = G = B) produce `rg = Cb = Cr = 0`, so both branches
reduce to the identity — the pixel is unchanged regardless of `strength` or
which branch fires. The effect is visible only where hue differences already
exist in the source image.



`myopia`, `hyperopia`, `presbyopia`, and `astigmatism` simulate refractive
defocus using a **disk (pillbox) blur** in linear sRGB space:

- A point light source falling out of focus on the retina images as a
  **circle of confusion** (CoC), not a Gaussian. The eye's pupil acts as
  the aperture, so the impulse response of a defocused eye is the shape
  of the pupil — a uniform-density disk to first approximation. Gaussian
  blur is a good *de-noising* prior but is **not** what a defocused eye
  produces; sensus uses disk blur for physical correctness. The rationale is
  recorded canonically in
  [ADR-0003](adr/0003-disk-blur-not-gaussian.md).
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
The GLSL uniform side (`shaders::astigmatism_uniforms`) shares this
exact normalization via `vision::refraction::normalize_axis_deg`, so
CPU and GLSL always agree on the effective axis. Before this was
unified (Issue #169), GLSL uniforms applied `axis_deg + 90°`
unconditionally: a `NaN` axis — reachable only via direct library
calls, since the CLI's `parse_axis` rejects it — produced
`cos/sin = NaN` and an all-black GLSL render, while the CPU path fell
back to 90° and produced a normal blurred image.

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

> **Note**: The `Vignette` mode (uniform peripheral darkening) is an
> approximation. Clinical glaucoma typically presents as **arcuate
> (Bjerrum) scotoma** — a bow-shaped blind area that follows the nerve
> fibre bundle from the optic disc (ON head). The `ArcuateSuperior`,
> `ArcuateInferior`, and `Biarcuate` modes implement this more realistic
> pattern, with the ON head offset 15% horizontally from the image centre.

`GlaucomaMode` variants:

| Mode | Description |
|---|---|
| `Vignette` | Legacy uniform peripheral darkening (backward-compatible) |
| `ArcuateSuperior` | Superior Bjerrum scotoma (upper arcuate defect) |
| `ArcuateInferior` | Inferior Bjerrum scotoma (lower arcuate defect) |
| `Biarcuate` | Both superior and inferior arcuate defects (advanced glaucoma) |

> **Note (S-4)**: the arcuate-scotoma modes (`ArcuateSuperior` /
> `ArcuateInferior` / `Biarcuate`) are implemented for a right-eye viewpoint.
> For the left eye the optic-disc offset direction is mirrored.

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

- **cataract**: yellowing via Pokorny et al. (1987) / van Norren & Vos (1974)
  chromatic matrix applied in linear sRGB, plus 32×32 bilinear-interpolated
  scatter noise (LCG-based, spatially correlated).
  ⚠️ Medical note (early consultation): rapid loss of acuity or field change
  warrants an eye exam.
- **photophobia**: extracts pixels above BT.709 luminance threshold 0.5,
  applies disk blur, and adds the result back as bloom.
- **nyctalopia**: desaturates via BT.709 lerp (×0.8) and darkens (×0.7).
  ⚠️ Medical note (early consultation): rapidly worsening night blindness may
  indicate vitamin-A deficiency or retinitis pigmentosa.
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
  🚨 Medical note (emergency): sudden-onset double vision can signal oculomotor
  nerve palsy or brainstem infarction — seek care immediately.
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
- **14 hearing filters**: `hearing_loss`, `sudden_hearing_loss`,
  `noise_induced_hearing_loss`, `tinnitus`, `hyperacusis`, `misophonia`,
  `paracusis`, `amusia`, `dysmelodia`, `pitch_shift_semitones`, `diplacusis`,
  `auditory_processing_disorder`, `meniere`, `labyrinthitis`, returned as
  processed `AudioBuffer`. All are stateless over frames — callers supply a
  fresh buffer per chunk. Dispatched via `apply_hearing` / `HearingFilter`,
  or chained via `AudioPipeline`.
- **3 vestibular–visual filters** added to `vision.rs`: `vertigo` (rotating
  radial warp), `bppv_rotation` (brief rotational jerk), `vestibular_neuritis`
  (sustained horizontal tilt). These are image-space effects; no audio I/O.
  Because a still image has no time axis, `apply()` renders these at a
  representative peak phase (`VERTIGO_STILL_TIME_S` / `BPPV_STILL_TIME_S`);
  animation is the GLSL shader's `time` uniform's job.

## GLSL ES 3.00 shader source API (Phase 5, #16)

`sensus_core::shaders` provides `*_glsl() -> &'static str` for each visual
filter and matching `*_uniforms()` helpers that compute ready-to-upload
uniform structs. All shaders target GLSL ES 3.00 (`#version 300 es`) for
compatibility with Flutter's `FragmentProgram` API.

The CPU implementation is the normative specification; shaders replicate the
same math. A GPU-free software equivalence test suite (`#17`) asserts that
CPU and shader outputs agree within ≤ 2/255 per channel (matrix filters) or
PSNR ≥ 30 dB (blur / directional filters).

That suite fixes *self-consistency* (CPU == shader). A separate known-answer
test suite (`crates/core/tests/color_kat.rs`, `#156`, extended in `#165` to
cover the per-severity table) fixes *source-consistency*: it asserts that the
color-vision output matches values derived independently from the published
[Machado 2009][machado] matrices — both the severity-1.0 endpoint and
intermediate per-severity table entries (severity 0.5 grid point, severity
0.25 interpolated) — the reference matrices and gamma pipeline are re-typed
in the test, never imported from the implementation. This catches a matrix
coefficient that drifts together across the CPU and shader paths (which the
equivalence test alone cannot detect), even when both stay self-consistent.

Because the KAT verifies the **8-bit quantized output**, its sensitivity has a
floor: it catches any drift large enough to move a rounded output channel
(golden anchors use exact equality, so a drift that shifts a single channel by
1/255 fails; including non-saturated mid-tone inputs surfaces coefficient changes
of roughly 0.001–0.004 in at least one color). Sub-u8 floating-point drift that
leaves every rounded channel unchanged is **out of scope by design** — the
intrinsic limit of an 8-bit-output check.

> **Known limitation** — the 1D directional-blur shaders (`nystagmus`,
> `astigmatism`) cap their per-pixel kernel at `RMAX = 15` taps, while the CPU
> blur radius is unbounded. They diverge once the blur radius exceeds ~15 px
> (`nystagmus`: default amplitude on images wider than ~500 px; `astigmatism`:
> only above ~1363 px). The CPU / CLI path is unaffected. A fixed-tap rewrite
> of these two shaders is tracked as a follow-up.

## Medical notes (when to see a doctor)

sensus pairs each filter with a "when to see a doctor" note so the simulation
doubles as an early-warning primer. The urgency vocabulary matches the
[`Urgency`](../crates/core/src/lib.rs) enum used by `Experience`:

- **Emergency** (🚨) — possible stroke / retinal-detachment / acute sign; seek
  care immediately.
- **Early consultation** (⚠️) — see a specialist soon; early treatment changes
  the outcome.
- **None** — typically congenital, refractive, or benign; no urgency by itself.

> These notes are general awareness guidance, **not** medical advice or a
> diagnosis. A simulated effect is not a symptom.

| Filter(s) | Urgency | Note |
|---|---|---|
| `protanopia`, `deuteranopia`, `tritanopia`, `achromatopsia`, `tetrachromacy` | None | Color vision type is usually congenital and stable. |
| `myopia`, `hyperopia`, `presbyopia`, `astigmatism` | None | Refractive — corrected with lenses; routine eye exams. |
| `contrast_sensitivity`, `detail_loss`, `eye_strain` | None | Often lighting/fatigue related; persistent change → eye exam. |
| `dry_eye` | None / ⚠️ | Usually benign; persistent pain or vision change → consult. |
| `starbursts` | ⚠️ early consultation | New night-time halos can accompany cataract or refractive error. |
| `glaucoma`, `tunnel_vision` | ⚠️ early consultation | Painless peripheral / tunnel field loss; early detection (glaucoma, retinitis pigmentosa) preserves the field. |
| `photophobia` | None / ⚠️ | Often benign light sensitivity; sudden severe photophobia with eye pain / headache → evaluate (iritis, migraine). |
| `macular_degeneration`, `metamorphopsia` | ⚠️ early consultation | Central distortion/blur; early treatment slows progression. |
| `cataract` | ⚠️ early consultation | Progressive clouding; rapid change warrants an exam. |
| `night-blindness` (`nyctalopia`) | ⚠️ early consultation | Rapid worsening may mean vitamin-A deficiency / RP. |
| `floaters` | ⚠️ early consultation | A *sudden* surge with flashes → rule out retinal tear. |
| `teichopsia` | ⚠️ early consultation | Usually migraine aura; first-ever episode → evaluate. |
| `nystagmus` | ⚠️ early consultation | New-onset (non-congenital) involuntary motion → evaluate. |
| `vertigo`, `bppv_rotation` | None / ⚠️ | BPPV is benign positional; recurrent/severe → evaluate. |
| `hemianopia` | 🚨 emergency | Sudden half-field loss is a stroke until proven otherwise. |
| `diplopia` | 🚨 emergency | Sudden double vision → nerve palsy / brainstem stroke. |
| `flickering_stars` (photopsia) | 🚨 emergency | Surge of flashes + curtain → retinal detachment. |
| `vestibular_neuritis` | 🚨 emergency | Sudden severe vertigo needs stroke differentiation. |
| `sudden_hearing_loss` | 🚨 emergency | Sudden sensorineural loss is an otologic emergency. |
| `meniere`, `labyrinthitis` | ⚠️ early consultation | Vertigo + hearing change → ENT evaluation. |
| `tinnitus`, `hyperacusis`, `misophonia`, `paracusis`, `amusia`, `dysmelodia`, `pitch_shift`, `diplacusis`, APD, `noise_induced_hearing_loss`, `hearing_loss` | None / ⚠️ | Often chronic/benign; sudden onset or one-sided → consult. |

## Out of scope / Non-goals

- **WebAssembly** — sensus is consumed by native apps; a wasm build adds
  maintenance cost without a clear consumer.
- **Real-time camera feeds inside this crate** — capture and display are
  the host application's responsibility. sensus only transforms buffers.
- **Smell / taste / haptics simulation** — these have no general-purpose
  digital output. They may be discussed as informational content but are
  outside the simulation scope.
