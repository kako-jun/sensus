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
(Tauri / Flutter), most prominently
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

## Non-goals

- **WebAssembly** — sensus is consumed by native apps; a wasm build adds
  maintenance cost without a clear consumer.
- **Real-time camera feeds inside this crate** — capture and display are
  the host application's responsibility. sensus only transforms buffers.
- **Smell / taste / haptics simulation** — these have no general-purpose
  digital output. They may be discussed as informational content but are
  outside the simulation scope.
