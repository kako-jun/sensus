# sensus

**Sensory perception simulation for images** — apply color blindness, low
vision, visual field defects, and (soon) hearing loss filters to ordinary
photos. Built as a Rust crate so it can power accessibility demos,
educational tools, and apps like
[universal-experience](https://github.com/kako-jun/universal-experience).

> *sensus* (Latin) — "sense, perception, feeling".

## Status

**v0.1.0** — stable release on [crates.io](https://crates.io/crates/sensus).
All Phase 1–3 vision filters are implemented. See
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
| `nyctalopia`            | ✅ implemented | 3 (#6) | Darkening and desaturation (night-blindness) |

### Hearing (Phase 4)

- Hearing loss curves, pitch shift, balance / vertigo simulation applied
  to audio buffers (#7, #8, #9).

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
```

Flags:

| Flag | Description |
|---|---|
| `-i`, `--input`    | Input image path (PNG / JPEG / WebP, etc.). |
| `-o`, `--output`   | Output image path. Format is inferred from the extension. |
| `-f`, `--filter`   | Filter to apply (e.g. `deuteranopia`, `cataract`, `glaucoma`). |
| `-s`, `--strength` | Strength `0.0 – 1.0`. `0.0` keeps the original; `1.0` is full effect. Default `1.0`. |
| `--axis`           | Astigmatism cylinder axis in degrees `0.0 – 180.0`. Only used with `--filter astigmatism`. Default `90.0` (with-the-rule: vertical lines sharp, horizontal blurred). |

Run `sensus --help` for the full list of filter names.

## Library usage

`sensus-core` is the pure-logic crate (no I/O). Take a `DynamicImage` in,
get a `DynamicImage` out.

```toml
# Cargo.toml
[dependencies]
sensus-core = "0.1"
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
    // `apply()` always uses the default 90° axis for astigmatism — call
    // `vision::astigmatism()` directly if you need a custom axis.
    let _shifted = apply(Filter::Protanopia, img, 1.0)?;
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
chained together via `sensus_core::pipeline` (Phase 4).

## Crates

| Crate | Path | Role |
|---|---|---|
| `sensus-core` | `crates/core/` | Pure rendering / DSP core. No filesystem, no subprocess. |
| `sensus`      | `crates/cli/`  | CLI binary. Owns `image::open`, file writes, etc. |

## License

MIT (c) 2026 kako-jun. See [`LICENSE`](LICENSE).
