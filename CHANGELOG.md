# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cargo workspace scaffold with two crates: `sensus-core` (pure logic) and
  `sensus` (CLI binary). (#1)
- CLI argument parsing via clap derive — `--input`, `--output`, `--filter`,
  `--strength` — covering all planned vision filter names. Filters are not
  yet implemented; the binary exits with code `2` and a "not implemented"
  message. (#1)
- GitHub Actions workflows: `ci.yml` (test / fmt / clippy) and `release.yml`
  (tag-driven build for x86_64-linux, x86_64-apple, aarch64-apple,
  x86_64-windows; uploads tarballs / zips to GitHub Releases). (#1)
- Documentation: `README.md` (English, end-user master), `docs/overview.md`
  (English, design), `docs/roadmap.md` (Japanese, phase tracker),
  `CLAUDE.md` (Japanese, AI-facing internal notes). (#1)
- MIT license. (#1)
