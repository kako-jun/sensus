//! sensus CLI — simulate sensory perception on images.
//!
//! Phase 1 (Issue #2) 以降で各フィルタを実装する。本 scaffold (#1) では
//! 引数を受け取り、未実装フィルタの場合は stderr に通知して exit(2) する。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use sensus_core::{Error as CoreError, Filter as CoreFilter};
use thiserror::Error;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Filter {
    // Phase 1: 色覚特性 (Issue #2)
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Achromatopsia,
    // Phase 1+: 四色型色覚 (Issue #3)
    Tetrachromacy,
    // Phase 2: 焦点・屈折 (Issue #4)
    Myopia,
    Hyperopia,
    Astigmatism,
    Presbyopia,
    // Phase 3: 視野異常 (Issue #5)
    Glaucoma,
    MacularDegeneration,
    Hemianopia,
    TunnelVision,
    // Phase 3: 光・透明度 (Issue #6)
    Cataract,
    Floaters,
    Photophobia,
    NightBlindness,
}

impl Filter {
    /// Map the CLI-facing enum (clap derive) to the core enum.
    fn to_core(self) -> CoreFilter {
        match self {
            Filter::Protanopia => CoreFilter::Protanopia,
            Filter::Deuteranopia => CoreFilter::Deuteranopia,
            Filter::Tritanopia => CoreFilter::Tritanopia,
            Filter::Achromatopsia => CoreFilter::Achromatopsia,
            Filter::Tetrachromacy => CoreFilter::Tetrachromacy,
            Filter::Myopia => CoreFilter::Myopia,
            Filter::Hyperopia => CoreFilter::Hyperopia,
            Filter::Astigmatism => CoreFilter::Astigmatism,
            Filter::Presbyopia => CoreFilter::Presbyopia,
            Filter::Glaucoma => CoreFilter::Glaucoma,
            Filter::MacularDegeneration => CoreFilter::MacularDegeneration,
            Filter::Hemianopia => CoreFilter::Hemianopia,
            Filter::TunnelVision => CoreFilter::TunnelVision,
            Filter::Cataract => CoreFilter::Cataract,
            Filter::Floaters => CoreFilter::Floaters,
            Filter::Photophobia => CoreFilter::Photophobia,
            Filter::NightBlindness => CoreFilter::NightBlindness,
        }
    }
}

/// sensus — simulate sensory perception on images.
#[derive(Debug, Parser)]
#[command(name = "sensus", version, about, long_about = None)]
struct Cli {
    /// Input image path (PNG / JPEG / WebP, etc.).
    #[arg(short, long)]
    input: PathBuf,

    /// Output image path. Format is inferred from the extension.
    #[arg(short, long)]
    output: PathBuf,

    /// Filter to apply.
    #[arg(short, long, value_enum)]
    filter: Filter,

    /// Filter strength in 0.0..=1.0 (0.0 = original, 1.0 = full effect).
    #[arg(short, long, default_value_t = 1.0, value_parser = parse_strength)]
    strength: f32,

    /// Astigmatism axis in degrees (0.0..=180.0). Only used with
    /// `--filter astigmatism`. Default `90.0` (with-the-rule astigmatism:
    /// vertical sharp, horizontal blurred).
    #[arg(long, default_value_t = 90.0, value_parser = parse_axis)]
    axis: f32,
}

/// Parse the `--strength` argument and reject values outside `0.0..=1.0`
/// or NaN early, before any I/O. core 側の clamp は防御的に残してある。
fn parse_strength(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(0.0..=1.0).contains(&v) {
        return Err(format!("strength must be in 0.0..=1.0, got {v}"));
    }
    Ok(v)
}

/// Parse the `--axis` argument (astigmatism cylinder axis in degrees) and
/// reject values outside `0.0..=180.0` or NaN early. 軸の周期は 180° なので
/// それより広い範囲は意味的に冗長 (誤入力の可能性が高い) として弾く。
fn parse_axis(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(0.0..=180.0).contains(&v) {
        return Err(format!("axis must be in 0.0..=180.0 degrees, got {v}"));
    }
    Ok(v)
}

/// CLI-internal error type. Keeps the `main` ↔ `run` boundary explicit so
/// integration tests can drive `run` directly without poking `process::exit`.
#[derive(Debug, Error)]
enum RunError {
    #[error("sensus: failed to open input {path:?}: {source}")]
    InputOpen {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    #[error("sensus: failed to save output {path:?}: {source}")]
    OutputSave {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },

    /// A filter was selected but not yet implemented in core.
    #[error("{0}")]
    NotImplemented(String),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::NotImplemented(msg)) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), RunError> {
    let img = image::open(&cli.input).map_err(|source| RunError::InputOpen {
        path: cli.input.clone(),
        source,
    })?;

    let (width, height) = (img.width(), img.height());

    // astigmatism のみ CLI から軸を渡せるよう特別扱いする。他フィルタは
    // 既定軸の apply() ファサード経由 (同じ動作・他フィルタでは axis は無視)。
    let core_filter = cli.filter.to_core();
    let result = match core_filter {
        CoreFilter::Astigmatism => sensus_core::vision::astigmatism(img, cli.strength, cli.axis),
        f => sensus_core::apply(f, img, cli.strength),
    };

    match result {
        Ok(out) => {
            out.save(&cli.output)
                .map_err(|source| RunError::OutputSave {
                    path: cli.output.clone(),
                    source,
                })?;
            Ok(())
        }
        Err(CoreError::NotImplemented(filter)) => {
            let msg = format!(
                "sensus: filter {:?} (strength {:.2}) is not implemented yet.\n\
                 sensus: input {}x{} {:?} -> output {:?}\n\
                 sensus: see https://github.com/kako-jun/sensus for roadmap.",
                filter, cli.strength, width, height, cli.input, cli.output
            );
            Err(RunError::NotImplemented(msg))
        }
        Err(CoreError::Image(err)) => Err(RunError::InputOpen {
            path: cli.input.clone(),
            source: err,
        }),
    }
}
