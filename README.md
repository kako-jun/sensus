# sensus

**Sensory perception simulation for images and audio** — apply color
blindness, low vision, visual field defects, and hearing loss filters to
ordinary photos and audio buffers. Built as a Rust crate so it can power
accessibility demos, educational tools, and apps like
[universal-experience](https://github.com/kako-jun/universal-experience).

> *sensus* (Latin) — "sense, perception, feeling".

## Status

**v0.5.0** — published on [crates.io](https://crates.io/crates/sensus).
All vision filters through Phase 4 (color vision, refraction, visual field,
light/transparency, balance/vertigo, eye fatigue, and more) are implemented,
plus 14 hearing filters (usable from the CLI via `--audio`). See
[`docs/roadmap.md`](docs/roadmap.md) for the full phase tracker.

## Filters

### Vision

| Filter | Status | Phase | Notes |
|---|---|---|---|
| `protanopia`            | ✅ implemented | 1 (#2) | L-cone loss (red-blind), Machado 2009 |
| `deuteranopia`          | ✅ implemented | 1 (#2) | M-cone loss (green-blind), Machado 2009 |
| `tritanopia`            | ✅ implemented | 1 (#2) | S-cone loss (blue-blind), Machado 2009 |
| `achromatopsia`         | ✅ implemented | 1 (#2) | Total color blindness, BT.709 luma |
| `tetrachromacy`         | ✅ implemented | 1+ (#3) | Four-cone exploration, gamut expansion in LMS space |
| `myopia`                | ✅ implemented | 2 (#4) | Disk blur, -6 D max |
| `hyperopia`             | ✅ implemented | 2 (#4) | Disk blur, +4 D max |
| `presbyopia`            | ✅ implemented | 2 (#4) | Disk blur, +3 D add max |
| `astigmatism`           | ✅ implemented | 2 (#4) | 1D directional blur (pure cylindrical lens), configurable axis |
| `glaucoma`              | ✅ implemented | 3 (#5) | Radial vignette / peripheral field loss |
| `macular_degeneration`  | ✅ implemented | 3 (#5) | Foveal blur and dimming |
| `hemianopia`            | ✅ implemented | 3 (#5) | Half-field blanking |
| `tunnel_vision`         | ✅ implemented | 3 (#5) | Severe radial vignette |
| `cataract`              | ✅ implemented | 3 (#6) | Haze overlay |
| `floaters`              | ✅ implemented | 3 (#6) | Translucent blob compositing |
| `photophobia`           | ✅ implemented | 3 (#6) | Brightness boost and highlight halation |
| `night-blindness`       | ✅ implemented | 3 (#6) | Darkening and desaturation (nyctalopia) |
| `vertigo`               | ✅ implemented | 4 (#9) | Rotational displacement / dizziness |
| `bppv-rotation`         | ✅ implemented | 4 (#9) | Positional vertigo rotation |
| `vestibular-neuritis`   | ✅ implemented | 4 (#9) | Sustained spinning + blur |
| `diplopia`              | ✅ implemented | 4 (#29) | Double vision (ghost image offset) |
| `nystagmus`             | ✅ implemented | 4 (#29) | Involuntary directional motion blur |
| `starbursts`            | ✅ implemented | 4 (#29) | Light starbursts / glare from highlights |
| `eye-strain`            | ✅ implemented | 4 (#36) | Contrast loss, vignette, slight blur |
| `dry-eye`               | ✅ implemented | 4 (#36) | Tear-film tile distortion |

Additional vision filters (library `Filter` enum **and** CLI `--filter`):
`metamorphopsia` (#55), `contrast-sensitivity` (#56), `detail-loss` (#57),
`teichopsia` (#58), and `flickering-stars` (#59). The library function names
use underscores (`contrast_sensitivity`, `detail_loss`, `flickering_stars`);
the CLI flag names and the `night-blindness` / `nyctalopia` pair follow the
kebab-case ↔ snake-case mapping clap derives.

Depth-aware refraction is also available **CLI-side only** via
`--filter myopia-depth` / `hyperopia-depth` / `depth-of-field` combined with
`--depth`, `--mpo`, or `--portrait` (#19).

### Hearing (library + CLI)

14 hearing filters operate on audio buffers (#7–#9, #102–#104): `hearing_loss`,
`sudden_hearing_loss`, `noise_induced_hearing_loss`, `tinnitus`,
`hyperacusis`, `misophonia`, `paracusis`, `amusia`, `dysmelodia`,
`pitch_shift_semitones`, `diplacusis`, `auditory_processing_disorder` (APD),
`meniere`, and `labyrinthitis`. Each takes a
`sensus_core::hearing::AudioBuffer` (f32 interleaved PCM) and returns one.

From the CLI, use `--audio <in.wav> --hearing <filter> -o <out.wav>` (WAV only).
See [CLI usage](#cli-usage). From Rust, use `sensus_core::apply_hearing` /
`HearingFilter`, or chain via `AudioPipeline`.

### Composite experiences (`Experience`)

Some conditions span both vision and hearing (e.g. Ménière's disease =
spinning vertigo + low-frequency hearing loss + low-pitched tinnitus). Because
sensus is a pure, separate-buffer (image vs audio) library, `Experience`
canonicalises which vision filter pairs with which hearing filter, so consumers
(e.g. the universal-experience GUI) don't hard-code the combination:
`Experience::MENIERE`, `BPPV`, `VESTIBULAR_NEURITIS`, `LABYRINTHITIS`. Each
carries an `Urgency` classification for "see a doctor" prompts.

Each filter ships with a short note on **when to see a doctor** — sensus is
designed both as an empathy / education tool and as an early-warning primer
for symptoms worth taking seriously.

## Install

```sh
cargo install sensus
```

This installs the `sensus` CLI binary. Pre-built binaries for Linux,
macOS (Intel & Apple Silicon), and Windows are also attached to each
[GitHub Release](https://github.com/kako-jun/sensus/releases).

## CLI usage

```sh
# Phase 1: color vision deficiency simulation
sensus -i photo.png -o photo-deuteranopia.png --filter deuteranopia --strength 1.0
sensus -i photo.png -o photo-grayscale.png    --filter achromatopsia --strength 1.0
sensus -i photo.png -o photo-mild-protan.png  --filter protanopia    --strength 0.5

# Phase 2: focus / refraction (disk blur in linear sRGB)
sensus -i photo.png -o photo-myopia.png       --filter myopia        --strength 1.0
sensus -i photo.png -o photo-presbyopia.png   --filter presbyopia    --strength 0.7
# astigmatism with explicit cylinder axis (degrees, 0..=180; default 90)
sensus -i photo.png -o photo-astig.png        --filter astigmatism   --strength 1.0 --axis 90

# Phase 3: visual field defects and light conditions
sensus -i photo.png -o photo-glaucoma.png     --filter glaucoma      --strength 1.0
sensus -i photo.png -o photo-cataract.png     --filter cataract      --strength 0.8

# Pipeline: chain multiple filters in one pass
sensus -i photo.png -o out.png --filter deuteranopia --filter myopia --strength 1.0
sensus -i photo.png -o out.png --filter glaucoma --filter cataract --strength 0.7

# Pipe mode: read JPEG frames from stdin, write filtered frames to stdout (ffmpeg integration)
# --output is not required when --pipe is used
ffmpeg -i video.mp4 -f image2pipe -vcodec mjpeg - | \
    sensus --filter deuteranopia --pipe | \
    ffmpeg -f mjpeg -i - -c:v libx264 out.mp4

# Hearing: apply hearing filters to a WAV file (#105). WAV in / WAV out only.
sensus --audio voice.wav -o voice-hearing-loss.wav --hearing hearing-loss -s 1.0
sensus --audio voice.wav -o voice-tinnitus.wav     --hearing tinnitus     --freq 6000 -s 0.7
# Chain hearing filters and use the composite Ménière hearing side
sensus --audio voice.wav -o voice-meniere.wav      --hearing meniere      -s 0.8
sensus --audio voice.wav -o voice-chain.wav        --hearing hearing-loss tinnitus --freq 4000
```

Flags:

| Flag | Description |
|---|---|
| `-i`, `--input`    | Input image path (PNG / JPEG / WebP, etc.). |
| `-o`, `--output`   | Output path (image, or WAV with `--audio`). Format is inferred from the extension. |
| `-f`, `--filter`   | Vision filter to apply (e.g. `deuteranopia`, `cataract`, `glaucoma`). |
| `--audio`          | Input audio path (WAV). Routes to hearing-filter mode; requires `--hearing` and `-o`. Cannot be combined with `--filter` / `--pipe`. |
| `--hearing`        | Hearing filter(s) for `--audio` (e.g. `hearing-loss`, `tinnitus`, `meniere`, `labyrinthitis`). Repeatable to chain. |
| `--freq`           | Frequency (Hz) for `--hearing tinnitus` / `sudden-hearing-loss` / `misophonia`. Default `4000`. |
| `--semitones`      | Semitones for `--hearing pitch-shift` (negative = lower). Default `0.0`. |
| `-s`, `--strength` | Strength `0.0 – 1.0`. `0.0` keeps the original; `1.0` is full effect. Default `1.0`. |
| `--axis`           | Astigmatism cylinder axis in degrees `0.0 – 180.0` (astigmatism only). Default `90.0` (with-the-rule: vertical lines sharp, horizontal blurred). |
| `--seed`           | Random seed for stochastic filters (`cataract`, `floaters`, `flickering-stars`). Default `0`. |
| `--density`, `--gaze-x`, `--gaze-y`, `--size` | `floaters`: blob density `0–1`, gaze position `0–1`, and size multiplier `0.1–5.0`. |
| `--side`           | `hemianopia` side: `0.0` = left field lost, `1.0` = right field lost. |
| `--offset-x`, `--offset-y`, `--ghost-strength` | `diplopia` ghost-image offsets (`min(W,H)` ratio) and ghost strength. |
| `--amplitude`, `--direction-deg` | `nystagmus` motion amplitude (`min(W,H)` ratio) and direction in degrees. |
| `--num-rays`, `--ray-length`, `--threshold`, `--dispersion` | `starbursts`: ray count, ray length (`min(W,H)` ratio), brightness threshold, and chromatic dispersion (`0` = white, `1` = rainbow). |
| `--cell-size`      | `detail-loss` pixelation tile size in pixels (`>= 1`; `1` = no effect). Default `8`. |
| `--meta-freq`, `--meta-seed` | `metamorphopsia` distortion spatial frequency and field seed. |
| `--depth`, `--mpo`, `--portrait`, `--focus` | Depth-map sources and focus depth for the `*-depth` / `depth-of-field` filters. |

Per-filter parameter flags only affect their own filter; passing one to an
unrelated single filter prints a warning and is otherwise ignored.
Run `sensus --help` for the full list of filter names and flags.

## Library usage

`sensus-core` is the pure-logic crate (no I/O). Take a `DynamicImage` in,
get a `DynamicImage` out.

```toml
# Cargo.toml
[dependencies]
sensus-core = "0.5"
```

```rust
use image::DynamicImage;
use sensus_core::{apply, vision, Filter};

fn examples(img: DynamicImage) -> sensus_core::Result<()> {
    // Direct call (most ergonomic for a known filter):
    let _full   = vision::deuteranopia(img.clone(), 1.0)?;
    let _mild   = vision::deuteranopia(img.clone(), 0.5)?;
    let _gray   = vision::achromatopsia(img.clone(), 1.0)?;
    let _near   = vision::myopia(img.clone(), 1.0)?;
    let _astig  = vision::astigmatism(img.clone(), 0.5, 45.0)?; // axis in degrees

    // Or via the dispatching facade (handy when the filter is dynamic).
    // Filter-specific parameters travel in the enum variant, so `apply()`
    // honours them (e.g. the astigmatism axis below):
    let _shifted = apply(Filter::Protanopia, img.clone(), 1.0)?;
    let _astig2  = apply(Filter::Astigmatism { axis_deg: 45.0 }, img, 1.0)?;
    Ok(())
}
```

`strength` is clamped to `[0.0, 1.0]`. Filters in Phase 1 simulate color
vision deficiency in linear sRGB space using the
[Machado 2009](https://doi.org/10.1109/TVCG.2009.113) model;
`achromatopsia` uses the BT.709 photopic luminance.
(BT.601 luma coefficients are designed for NTSC CRTs and are colorimetrically
wrong for sRGB images.)
Phase 2 (refraction) applies an optically correct **disk blur** in linear
sRGB — radius derived from `0.5 × pupil_diameter × |D|` (Smith–Helmholtz
gives angular *diameter*, so the radius is half), `min(W, H)`-relative,
edge-replicated borders. `astigmatism` is **1D directional blur** (pure
cylindrical lens / line spread function), not an elliptical disk — see
`docs/overview.md` for the full derivation.

The same function signatures can be applied frame-by-frame for video, or
chained together via `sensus_core::pipeline`.

### Hearing filters

Hearing filters live in `sensus_core::hearing` and are dispatched via
`apply_hearing`. They operate on audio buffers, not images. From the CLI they
are reachable via `--audio <in.wav> --hearing <filter> -o <out.wav>` (WAV only);
from Rust use the API below or chain them via `AudioPipeline`.

```rust
use sensus_core::{apply_hearing, hearing::AudioBuffer, HearingFilter};

fn deafen(buf: AudioBuffer) -> sensus_core::Result<AudioBuffer> {
    let muffled = apply_hearing(HearingFilter::HearingLoss, buf, 1.0)?;
    // Tinnitus and pitch shift carry their own parameters in the variant:
    let ringing = apply_hearing(HearingFilter::Tinnitus { freq_hz: 4000.0 }, muffled, 0.5)?;
    Ok(ringing)
}
```

### GLSL shaders (host integration)

`sensus_core::shaders` ships the same vision effects as GLSL fragment
shaders for hosts that render on the GPU (Flutter `FragmentProgram`, WebGL,
etc.). Some shaders need **resolution-dependent uniforms** — e.g.
`photophobia`, `eye_strain`, `dry_eye`, `teichopsia`, `glaucoma`,
`macular_degeneration`, `tunnel_vision`, and the disk-blur refraction
filters consume values such as `uRadiusPx` and `uTexelSize`
(`vec2(1.0/width, 1.0/height)`). When using these shaders in an external
host you must set those uniforms from the matching `*_uniforms()` helper
(e.g. `shaders::photophobia_uniforms(strength, width, height)`); passing only
`strength` will produce incorrect (scale-dependent) results. This is the
contract universal-experience relies on.

## Crates

| Crate | Path | Role |
|---|---|---|
| `sensus-core` | `crates/core/` | Pure rendering / DSP core. No filesystem, no subprocess. |
| `sensus`      | `crates/cli/`  | CLI binary. Owns `image::open`, file writes, etc. |

## License

MIT (c) 2026 kako-jun. See [`LICENSE`](LICENSE).
