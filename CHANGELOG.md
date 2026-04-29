# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
