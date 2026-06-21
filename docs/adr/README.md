# Architecture Decision Records (ADR)

This directory is the **canonical record** of the significant design decisions
behind `sensus`. An ADR captures *why* a choice was made — the problem, the
alternatives that were weighed, the reasoning, and the trade-offs we accepted —
so that downstream consumers
([universal-experience](https://github.com/kako-jun/universal-experience)) and
future contributors can audit the rationale rather than reverse-engineering it
from the code.

`sensus` is the upstream source of truth for the perceptual simulation math.
Before this directory existed, the design rationale was scattered across
`CLAUDE.md` notes, `docs/overview.md` prose, and derivation comments in
`crates/core/src/vision.rs`. The ADRs here promote that scattered reasoning into
formal, dated, individually-citable records. The scattered prose remains as a
quick summary; **the ADRs are authoritative when they disagree.**

## What an ADR is

Each ADR is a short Markdown file describing **one** decision. We follow a
[MADR](https://adr.github.io/madr/)-style template, kept deliberately lean.
Every ADR has these sections:

- **Status** — `Accepted`, `Superseded by NNNN`, or `Proposed`.
- **Context** — the problem and the forces at play (what made a decision
  necessary).
- **Decision** — the choice that was made, stated plainly.
- **Alternatives considered** — the options that were on the table.
- **Rationale** — why the chosen option won over the alternatives.
- **Consequences / Trade-offs** — what we gain and what we give up by accepting
  this decision.

## Numbering convention

- ADRs are numbered sequentially with a zero-padded four-digit prefix:
  `0001-`, `0002-`, … followed by a short kebab-case slug.
- Numbers are **never reused**. A decision that overturns an earlier one gets a
  new number and marks the old ADR `Superseded by NNNN`; the old file is kept
  for the historical record.
- New ADRs append to the end of the table below.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-linear-srgb-machado-matrices.md) | Operate color-vision simulation in linear sRGB, applying Machado 2009 matrices directly (no explicit LMS pipeline) | Accepted |
| [0002](0002-linear-blend-intermediate-severity.md) | Linear blend for intermediate severity | Accepted |
| [0003](0003-disk-blur-not-gaussian.md) | Disk (pillbox) blur for defocus / refraction, not Gaussian | Accepted |
| [0004](0004-achromatopsia-bt709-photopic.md) | Achromatopsia via BT.709 photopic luminance (not BT.601, not LMS) | Accepted |
| [0005](0005-phased-scope-phase1-four-types.md) | Phased scope: Phase 1 limited to the four dichromacy / achromatopsia types | Accepted |
| [0006](0006-filters-as-pure-functions-explicit-seed.md) | Filters are pure functions with an explicit seed | Accepted |
| [0007](0007-wasm-out-of-scope.md) | WebAssembly is out of scope | Accepted |

## Related specifications

- [`matrix-provenance.md`](matrix-provenance.md) — provenance of the color-vision
  matrices and luminance coefficients: which source, which cells, how extracted,
  and how the values are pinned by tests. Read alongside ADR-0001 and ADR-0004.
