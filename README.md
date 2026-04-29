# sensus

**Sensory perception simulation for images** — apply color blindness, low
vision, visual field defects, and (soon) hearing loss filters to ordinary
photos. Built as a Rust crate so it can power accessibility demos,
educational tools, and apps like
[universal-experience](https://github.com/kako-jun/universal-experience).

> *sensus* (Latin) — "sense, perception, feeling".

## Status

Pre-release scaffold (`v0.0.x`). Filters are landing one phase at a time —
see [`docs/roadmap.md`](docs/roadmap.md) for what is implemented today.

## Filters (planned)

### Vision

- **Color vision deficiency** — protanopia / deuteranopia / tritanopia /
  achromatopsia, with an adjustable strength (0.0 – 1.0). Tetrachromacy
  exploration is also on the roadmap.
- **Refraction & focus** — myopia, hyperopia, astigmatism, presbyopia.
- **Visual field defects** — glaucoma (peripheral loss), macular
  degeneration (central loss), hemianopia, tunnel vision.
- **Light & transparency** — cataract, floaters, photophobia, night
  blindness.

### Hearing (later)

- Hearing loss curves, pitch shift, and balance / vertigo simulation
  applied to audio buffers.

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
sensus -i photo.png -o photo-deuteranopia.png --filter deuteranopia --strength 1.0
```

Flags:

| Flag | Description |
|---|---|
| `-i`, `--input`    | Input image path (PNG / JPEG / WebP, etc.). |
| `-o`, `--output`   | Output image path. Format is inferred from the extension. |
| `-f`, `--filter`   | Filter to apply (e.g. `deuteranopia`, `cataract`, `glaucoma`). |
| `-s`, `--strength` | Strength `0.0 – 1.0`. `0.0` keeps the original; `1.0` is full effect. Default `1.0`. |

Run `sensus --help` for the full list of filter names.

## Library usage

`sensus-core` is the pure-logic crate (no I/O). Take a `DynamicImage` in,
get a `DynamicImage` out:

```rust
use image::DynamicImage;
use sensus_core::vision; // Phase 1: vision filters land here.

fn shift(img: DynamicImage) -> DynamicImage {
    // Phase 1 (Issue #2) で公開される API:
    // vision::deuteranopia(img, /* strength */ 1.0)
    img
}
```

The same function signatures can be applied frame-by-frame for video, or
chained together via `sensus_core::pipeline` (Phase 4).

## Crates

| Crate | Path | Role |
|---|---|---|
| `sensus-core` | `crates/core/` | Pure rendering / DSP core. No filesystem, no subprocess. |
| `sensus`      | `crates/cli/`  | CLI binary. Owns `image::open`, file writes, etc. |

## License

MIT (c) 2026 kako-jun. See [`LICENSE`](LICENSE).
