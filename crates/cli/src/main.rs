//! sensus CLI — simulate sensory perception on images.
//!
//! Phase 1 (Issue #2) 以降で各フィルタを実装する。本 scaffold (#1) では
//! 引数を受け取り、未実装フィルタの場合は stderr に通知して exit(2) する。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use sensus_core::{pipeline::{FilterStep, Pipeline}, vision::{depth_aware_blur, DepthBlurKind}, Error as CoreError, Filter as CoreFilter};
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
    // Phase 4: 平衡・めまい視覚 (Issue #9)
    Vertigo,
    BppvRotation,
    VestibularNeuritis,
    // Phase 4: 眼振・複視・スターバースト (Issue #29)
    Diplopia,
    Nystagmus,
    Starbursts,
    // Phase N: 深度マップ対応距離依存ぼけ (Issue #19)
    MyopiaDepth,
    HyperopiaDepth,
    DepthOfField,
}

impl Filter {
    fn is_depth_filter(self) -> bool {
        matches!(self, Filter::MyopiaDepth | Filter::HyperopiaDepth | Filter::DepthOfField)
    }

    fn depth_kind(self) -> Option<DepthBlurKind> {
        match self {
            Filter::MyopiaDepth => Some(DepthBlurKind::Myopia),
            Filter::HyperopiaDepth => Some(DepthBlurKind::Hyperopia),
            Filter::DepthOfField => Some(DepthBlurKind::DepthOfField),
            _ => None,
        }
    }

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
            Filter::Vertigo => CoreFilter::Vertigo,
            Filter::BppvRotation => CoreFilter::BppvRotation,
            Filter::VestibularNeuritis => CoreFilter::VestibularNeuritis,
            Filter::Diplopia => CoreFilter::Diplopia,
            Filter::Nystagmus => CoreFilter::Nystagmus,
            Filter::Starbursts => CoreFilter::Starbursts,
            Filter::MyopiaDepth | Filter::HyperopiaDepth | Filter::DepthOfField => {
                // depth フィルタは pipeline を通さないため、ここには来ない
                unreachable!("depth filters must be handled separately")
            }
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

    /// Filter(s) to apply. Specify multiple times to chain filters.
    #[arg(short, long, value_enum, num_args = 1..)]
    filter: Vec<Filter>,

    /// Filter strength in 0.0..=1.0 (0.0 = original, 1.0 = full effect).
    #[arg(short, long, default_value_t = 1.0, value_parser = parse_strength)]
    strength: f32,

    /// Astigmatism axis in degrees (0.0..=180.0). Only used with
    /// `--filter astigmatism`. Default `90.0` (with-the-rule astigmatism:
    /// vertical sharp, horizontal blurred).
    #[arg(long, default_value_t = 90.0, value_parser = parse_axis)]
    axis: f32,

    /// Random seed for stochastic filters (cataract, floaters). Default: 0.
    #[arg(long, default_value = "0")]
    seed: u64,

    /// Floater density in 0.0..=1.0. Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    density: f32,

    /// Gaze X position in 0.0..=1.0 (0=left, 1=right). Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    gaze_x: f32,

    /// Gaze Y position in 0.0..=1.0 (0=top, 1=bottom). Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    gaze_y: f32,

    /// Hemianopia side: 0.0 = left field lost, 1.0 = right field lost.
    /// Only used with --filter hemianopia.
    #[arg(long, default_value = "0.0")]
    side: f32,

    /// Depth map image path (PNG / JPEG / etc.). Only used with depth blur filters.
    #[arg(long)]
    depth: Option<PathBuf>,

    /// Focus depth in 0.0..=1.0 (bright=near, dark=far). Only used with depth blur filters.
    #[arg(long, default_value = "0.5", value_parser = parse_focus)]
    focus: f32,

    /// Diplopia horizontal offset in min(W,H) ratio (-1.0..=1.0). Default: 0.02
    #[arg(long, default_value = "0.02", value_parser = parse_signed_ratio)]
    offset_x: f32,

    /// Diplopia vertical offset in min(W,H) ratio (-1.0..=1.0). Default: 0.01
    #[arg(long, default_value = "0.01", value_parser = parse_signed_ratio)]
    offset_y: f32,

    /// Diplopia ghost image strength (0.0..=1.0). Default: 0.7
    #[arg(long, default_value = "0.7", value_parser = parse_ratio)]
    ghost_strength: f32,

    /// Nystagmus amplitude in min(W,H) ratio. Default: 0.03
    #[arg(long, default_value = "0.03", value_parser = parse_ratio)]
    amplitude: f32,

    /// Nystagmus direction in degrees (0=horizontal, 90=vertical). Default: 0.0
    #[arg(long, default_value = "0.0", value_parser = parse_direction_deg)]
    direction_deg: f32,

    /// Starbursts number of rays. Default: 6
    #[arg(long, default_value = "6")]
    num_rays: u32,

    /// Starbursts ray length in min(W,H) ratio. Default: 0.1
    #[arg(long, default_value = "0.1", value_parser = parse_ratio)]
    ray_length: f32,

    /// Starbursts brightness threshold (0.0..=1.0). Default: 0.8
    #[arg(long, default_value = "0.8", value_parser = parse_ratio)]
    threshold: f32,
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

/// Parse a ratio argument in `0.0..=1.0` (ghost-strength, amplitude, ray-length, threshold 等).
fn parse_ratio(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(0.0..=1.0).contains(&v) {
        return Err(format!("value must be in 0.0..=1.0, got {v}"));
    }
    Ok(v)
}

/// Parse a signed offset ratio in `-1.0..=1.0` (offset-x, offset-y).
fn parse_signed_ratio(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(-1.0..=1.0).contains(&v) {
        return Err(format!("value must be in -1.0..=1.0, got {v}"));
    }
    Ok(v)
}

/// Parse the `--focus` argument and reject values outside `0.0..=1.0` or NaN.
fn parse_focus(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(0.0..=1.0).contains(&v) {
        return Err(format!("focus must be in 0.0..=1.0, got {v}"));
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

/// Parse `--direction-deg` (nystagmus) in 0.0..=360.0.
fn parse_direction_deg(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if v.is_nan() || !(0.0..=360.0).contains(&v) {
        return Err(format!("direction-deg must be in 0.0..=360.0, got {v}"));
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

    /// A pipeline step failed at runtime.
    #[error("{0}")]
    Pipeline(String),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Pipeline(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
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

    // depth フィルタが含まれる場合は単独処理（Pipeline を通さない）
    // TODO(#19): Pipeline 統合時にここを削除し Pipeline 経由で処理する
    if cli.filter.iter().any(|f| f.is_depth_filter()) {
        if cli.filter.len() > 1 {
            return Err(RunError::Pipeline(
                "sensus: depth blur filters cannot be combined with other filters".to_string(),
            ));
        }
        let kind = cli.filter[0].depth_kind().unwrap();
        let depth_path = cli.depth.as_ref().ok_or_else(|| {
            RunError::Pipeline(
                "sensus: --depth <PATH> is required for depth blur filters".to_string(),
            )
        })?;
        let depth_img = image::open(depth_path).map_err(|source| RunError::InputOpen {
            path: depth_path.clone(),
            source,
        })?;
        let out = depth_aware_blur(img, &depth_img, cli.focus, cli.strength * 0.023, kind)
            .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))?;
        return out.save(&cli.output).map_err(|source| RunError::OutputSave {
            path: cli.output.clone(),
            source,
        });
    }

    // Build pipeline from --filter list (must not be empty; clap enforces num_args=1..)
    let mut pipeline = Pipeline::new();
    for f in &cli.filter {
        let core_filter = f.to_core();
        let mut step = FilterStep::new(core_filter, cli.strength);
        step.axis = cli.axis;
        step.seed = cli.seed;
        step.density = cli.density;
        step.gaze_x = cli.gaze_x;
        step.gaze_y = cli.gaze_y;
        step.side = cli.side;
        step.offset_x = cli.offset_x;
        step.offset_y = cli.offset_y;
        step.ghost_strength = cli.ghost_strength;
        step.amplitude = cli.amplitude;
        step.direction_deg = cli.direction_deg;
        step.num_rays = cli.num_rays;
        step.ray_length_ratio = cli.ray_length;
        step.threshold = cli.threshold;
        pipeline = pipeline.push(step);
    }

    // For single-filter case, replicate legacy warnings.
    if cli.filter.len() == 1 {
        let core_filter = cli.filter[0].to_core();
        if !matches!(core_filter, CoreFilter::Astigmatism) && cli.axis != 90.0 {
            eprintln!(
                "sensus: warning: --axis is only used with --filter astigmatism (ignored for {core_filter:?})"
            );
        }
        let uses_seed = matches!(core_filter, CoreFilter::Cataract | CoreFilter::Floaters);
        let uses_floater_params = matches!(core_filter, CoreFilter::Floaters);
        if !uses_seed && cli.seed != 0 {
            eprintln!(
                "sensus: warning: --seed is only used with --filter cataract or floaters (ignored for {core_filter:?})"
            );
        }
        if !uses_floater_params && (cli.density != 0.5 || cli.gaze_x != 0.5 || cli.gaze_y != 0.5) {
            eprintln!(
                "sensus: warning: --density/--gaze-x/--gaze-y are only used with --filter floaters (ignored for {core_filter:?})"
            );
        }
        let uses_side = matches!(core_filter, CoreFilter::Hemianopia);
        if !uses_side && cli.side != 0.0 {
            eprintln!(
                "sensus: warning: --side is only used with --filter hemianopia (ignored for {core_filter:?})"
            );
        }
    }

    let result = pipeline.apply(img);

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
        Err(e) => Err(RunError::Pipeline(format!("sensus: {e}"))),
    }
}
