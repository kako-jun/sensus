# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Phase 2 focus / refraction filters** (#4): `myopia`, `hyperopia`,
  `presbyopia`, `astigmatism`. All implemented as **disk (pillbox) blur**
  in linear sRGB — Gaussian is intentionally rejected because the
  defocused eye images a point source as a *circle of confusion*, not a
  Gaussian. `strength = 1.0` corresponds to the clinical maxima -6 D /
  +4 D / +3 D add / -3 CD respectively, mapped to a `min(W, H)`-relative
  radius assuming a 4 mm mesopic pupil and a 30° image FOV at ~50 cm
  viewing distance (Smith–Helmholtz small-angle approximation,
  `angular_blur ≈ pupil × |D|`). `astigmatism()` accepts an explicit
  `axis_deg` (sharp meridian, medical convention); the elliptical kernel's
  long / blurred axis is at `axis_deg + 90°`. Alpha is preserved.
  Implementation uses precomputed per-row spans + a horizontal prefix
  sum so the cost is `O(W × H × kernel_height)` (≈ 1 s for myopia at
  1024 × 1024, well under the 5 s target).
- CLI gains an `--axis` flag (range `0.0..=180.0`, default `90.0`) for
  astigmatism. Other filters ignore it. `apply(Filter::Astigmatism, …)`
  always uses the default 90° axis; library users who need a custom axis
  call `vision::astigmatism()` directly.
- **Phase 1 color vision deficiency filters** (#2): `protanopia`,
  `deuteranopia`, `tritanopia`, `achromatopsia`. Implemented in linear
  sRGB space. `protanopia` / `deuteranopia` / `tritanopia` use the
  Machado, Oliveira & Fernandes 2009 severity = 1.0 matrices
  (DOI: [10.1109/TVCG.2009.113](https://doi.org/10.1109/TVCG.2009.113))
  and blend towards the original in linear space for intermediate
  `strength` values. `achromatopsia` uses CIE photopic luminance with
  BT.709 primaries (`0.2126 R + 0.7152 G + 0.0722 B`); BT.601 is
  intentionally avoided. Alpha is preserved.
- `sensus_core::apply()` now dispatches the four Phase 1 filters and only
  returns `Error::NotImplemented` for the remaining variants.
- CLI now writes the transformed image to `--output` on success
  (exit code `0`) for any implemented filter.
- Cargo workspace scaffold with two crates: `sensus-core` (pure logic) and
  `sensus` (CLI binary). `sensus-core` is centralized in
  `[workspace.dependencies]`. (#1)
- `sensus_core::Filter` enum (17 variants covering all planned vision
  filters) plus `sensus_core::apply()` facade returning `Result`. CLI
  derives clap-side `Filter` and converts via `to_core()`. (#1)
- `sensus_core::Error` (thiserror-derived) with `NotImplemented(Filter)`
  and `Image(image::ImageError)` variants, and `sensus_core::Result<T>`
  alias. (#1)
- `sensus_core::vision::{protanopia, deuteranopia, tritanopia,
  achromatopsia}` function stubs ready for Phase 1 (#2). (#1)
- CLI argument parsing via clap derive — `--input`, `--output`, `--filter`,
  `--strength` — covering all planned vision filter names. CLI logic is
  split into `main` / `run()` for testability; filters are not yet
  implemented and the binary exits with code `2` and a "not implemented"
  message. (#1)
- GitHub Actions workflows: `ci.yml` (test / fmt / clippy with
  `--all-targets --locked`) and `release.yml` (tag-driven build with
  `-p sensus --locked` for x86_64-linux, x86_64-apple, aarch64-apple,
  x86_64-windows; uploads tarballs / zips to GitHub Releases). (#1)
- Documentation: `README.md` (English, end-user master, with scaffold-era
  install / git-dep instructions), `docs/overview.md` (English, design),
  `docs/roadmap.md` (Japanese, phase tracker), `CLAUDE.md` (Japanese,
  AI-facing internal notes). universal-experience is documented as a
  Flutter app (no Tauri). (#1)
- MIT license. (#1)
