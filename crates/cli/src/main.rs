//! sensus CLI — simulate sensory perception on images.
//!
//! Phase 1 (Issue #2) 以降で各フィルタを実装する。本 scaffold (#1) では
//! 引数を受け取り、未実装フィルタの場合は stderr に通知して exit(2) する。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use sensus_core::{pipeline::{FilterStep, Pipeline}, stereo::{split_mpo, stereo_to_depth, read_xmp_depth}, vision::{depth_aware_blur, DepthBlurKind}, AudioPipeline, Error as CoreError, Filter as CoreFilter, HearingFilter};
use thiserror::Error;

mod audio;

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
    // Phase 4: 眼精疲労・ドライアイ (Issue #36)
    EyeStrain,
    DryEye,
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
            Filter::Astigmatism => CoreFilter::Astigmatism { axis_deg: 90.0 },
            Filter::Presbyopia => CoreFilter::Presbyopia,
            Filter::Glaucoma => CoreFilter::Glaucoma { mode: sensus_core::vision::GlaucomaMode::Vignette },
            Filter::MacularDegeneration => CoreFilter::MacularDegeneration,
            Filter::Hemianopia => CoreFilter::Hemianopia { side: 0.0 },
            Filter::TunnelVision => CoreFilter::TunnelVision,
            Filter::Cataract => CoreFilter::Cataract,
            Filter::Floaters => CoreFilter::Floaters { seed: 0, density: 0.5, size: 1.0 },
            Filter::Photophobia => CoreFilter::Photophobia,
            Filter::NightBlindness => CoreFilter::NightBlindness,
            Filter::Vertigo => CoreFilter::Vertigo,
            Filter::BppvRotation => CoreFilter::BppvRotation,
            Filter::VestibularNeuritis => CoreFilter::VestibularNeuritis,
            Filter::Diplopia => CoreFilter::Diplopia,
            Filter::Nystagmus => CoreFilter::Nystagmus,
            Filter::Starbursts => CoreFilter::Starbursts { num_rays: 6, ray_length_ratio: 0.1, threshold: 0.8, dispersion: 0.0 },
            Filter::EyeStrain => CoreFilter::EyeStrain,
            Filter::DryEye => CoreFilter::DryEye,
            Filter::MyopiaDepth | Filter::HyperopiaDepth | Filter::DepthOfField => {
                // depth フィルタは pipeline を通さないため、ここには来ない
                unreachable!("depth filters must be handled separately")
            }
        }
    }
}

/// CLI-facing hearing filter enum (clap derive). Maps to core [`HearingFilter`].
/// `--audio` モードで `--hearing` に指定する（#105）。
//
// `HearingLoss` は疾患名（難聴）であって enum 名の重複ではないため allow する。
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Hearing {
    /// 難聴（高音域カット）
    HearingLoss,
    /// 突発性難聴（--freq の帯域を削る）
    SuddenHearingLoss,
    /// 騒音性難聴（4 kHz 付近の損失）
    NoiseInducedHearingLoss,
    /// 耳鳴り（--freq の正弦波を常時ミックス）
    Tinnitus,
    /// 音響過敏（全体を異常増幅）
    Hyperacusis,
    /// ミソフォニア（--freq 中心のトリガー帯域だけ過剰増幅 + 歪み）
    Misophonia,
    /// 変音（金属的な歪み）
    Paracusis,
    /// 音楽音痴（音程差を識別しにくくする）
    Amusia,
    /// ジスメロディア（不快・歪んだ音）
    Dysmelodia,
    /// 音程シフト（--semitones 半音）
    PitchShift,
    /// ダイプラクシス（左右耳で異なる音程）
    Diplacusis,
    /// APD（聴覚情報処理障害）
    AuditoryProcessingDisorder,
    /// メニエール病の聴覚側（低音域難聴 + 低い唸る耳鳴り）
    Meniere,
    /// 迷路炎の聴覚側（高音域感音難聴 + 高音の耳鳴り）
    Labyrinthitis,
}

impl Hearing {
    /// Map the CLI-facing enum to the core enum, pulling parameters from `cli`.
    fn to_core(self, cli: &Cli) -> HearingFilter {
        match self {
            Hearing::HearingLoss => HearingFilter::HearingLoss,
            Hearing::SuddenHearingLoss => HearingFilter::SuddenHearingLoss { freq_hz: cli.freq },
            Hearing::NoiseInducedHearingLoss => HearingFilter::NoiseInducedHearingLoss,
            Hearing::Tinnitus => HearingFilter::Tinnitus { freq_hz: cli.freq },
            Hearing::Hyperacusis => HearingFilter::Hyperacusis,
            Hearing::Misophonia => HearingFilter::Misophonia { freq_hz: cli.freq },
            Hearing::Paracusis => HearingFilter::Paracusis,
            Hearing::Amusia => HearingFilter::Amusia,
            Hearing::Dysmelodia => HearingFilter::Dysmelodia,
            Hearing::PitchShift => HearingFilter::PitchShift { semitones: cli.semitones },
            Hearing::Diplacusis => HearingFilter::Diplacusis,
            Hearing::AuditoryProcessingDisorder => HearingFilter::AuditoryProcessingDisorder,
            Hearing::Meniere => HearingFilter::Meniere,
            Hearing::Labyrinthitis => HearingFilter::Labyrinthitis,
        }
    }
}

/// sensus — simulate sensory perception on images.
#[derive(Debug, Parser)]
#[command(name = "sensus", version, about, long_about = None)]
struct Cli {
    /// Input image path (PNG / JPEG / WebP, etc.). Not required when --mpo is used.
    #[arg(short, long)]
    input: Option<PathBuf>,

    /// 出力ファイルパス（--pipe 時は不要）
    #[arg(short, long, required_unless_present = "pipe")]
    output: Option<PathBuf>,

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

    /// MPO stereo image path. Automatically generates a depth map and applies depth blur.
    /// Requires a depth blur filter (--filter myopia-depth / hyperopia-depth / depth-of-field).
    #[arg(long)]
    mpo: Option<PathBuf>,

    /// Android portrait-mode JPEG path. Extracts XMP depth map and applies depth blur.
    /// Requires a depth blur filter (--filter myopia-depth / hyperopia-depth / depth-of-field).
    /// --input is not required when --portrait is used.
    #[arg(long, conflicts_with = "mpo")]
    portrait: Option<PathBuf>,

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

    /// Read JPEG frames from stdin and write filtered JPEG frames to stdout (ffmpeg pipe mode).
    /// Cannot be combined with --input.
    #[arg(long, conflicts_with = "input")]
    pipe: bool,

    /// Input audio path (WAV). Routes to hearing-filter mode; requires --hearing and --output.
    /// Cannot be combined with image filters (--filter) or --pipe.
    #[arg(long, conflicts_with_all = ["input", "pipe", "mpo", "portrait"])]
    audio: Option<PathBuf>,

    /// Hearing filter(s) to apply to --audio. Specify multiple times to chain.
    #[arg(long, value_enum, num_args = 1..)]
    hearing: Vec<Hearing>,

    /// Frequency (Hz) for --hearing tinnitus / sudden-hearing-loss / misophonia. Default: 4000.
    #[arg(long, default_value_t = 4000.0, value_parser = parse_freq)]
    freq: f32,

    /// Semitones for --hearing pitch-shift (negative = lower, positive = higher). Default: 0.0
    #[arg(long, default_value_t = 0.0, value_parser = parse_semitones)]
    semitones: f32,
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

/// Parse `--freq` (Hz) for hearing filters. 正の有限値 0 < f <= 20000 のみ許容する。
fn parse_freq(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || v <= 0.0 || v > 20000.0 {
        return Err(format!("freq must be in 0.0 (exclusive)..=20000.0 Hz, got {v}"));
    }
    Ok(v)
}

/// Parse `--semitones` for pitch-shift. 有限値かつ ±48 半音（±4 オクターブ）以内。
fn parse_semitones(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || !(-48.0..=48.0).contains(&v) {
        return Err(format!("semitones must be finite and in -48.0..=48.0, got {v}"));
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

    #[error("sensus: failed to read MPO file {path:?}: {source}")]
    MpoRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    MpoError(String),

    #[error("sensus: failed to read portrait file {path:?}: {source}")]
    PortraitRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    PortraitError(String),

    /// --input が未指定で --mpo / --portrait も指定されていない
    #[error("{0}")]
    InputRequired(String),

    /// --audio モードのバリデーション失敗 / WAV 読み書き失敗
    #[error("{0}")]
    AudioError(String),

    #[error("sensus: I/O error: {0}")]
    Io(#[from] std::io::Error),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Pipeline(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
        Err(RunError::MpoError(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
        Err(RunError::PortraitError(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
        Err(err @ RunError::MpoRead { .. }) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
        Err(err @ RunError::PortraitRead { .. }) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
        Err(RunError::InputRequired(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
        Err(RunError::AudioError(msg)) => {
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
    // --audio モード: WAV を読み、聴覚フィルタチェーンを適用して WAV に書き出す
    if let Some(audio_path) = cli.audio.clone() {
        return run_audio(&cli, &audio_path);
    }

    // --pipe モード: stdin から JPEG フレームを読み stdout に出力する
    if cli.pipe {
        return run_pipe(&cli);
    }

    // --mpo が指定されている場合のバリデーションと処理
    if let Some(mpo_path) = cli.mpo {
        // 複数フィルタとの組み合わせは不可
        if cli.filter.len() > 1 {
            return Err(RunError::MpoError(
                "sensus: --mpo cannot be combined with multiple filters".to_string(),
            ));
        }
        // depth フィルタ以外は不可
        if !cli.filter.iter().any(|f| f.is_depth_filter()) {
            return Err(RunError::MpoError(
                "sensus: --mpo requires a depth blur filter (myopia-depth, hyperopia-depth, depth-of-field)".to_string(),
            ));
        }
        // --depth との同時指定は不可
        if cli.depth.is_some() {
            return Err(RunError::MpoError(
                "sensus: --mpo and --depth cannot be used together".to_string(),
            ));
        }

        let kind = cli.filter[0].depth_kind().unwrap();
        let bytes = std::fs::read(&mpo_path).map_err(|source| RunError::MpoRead {
            path: mpo_path.clone(),
            source,
        })?;
        let (left, right) = split_mpo(&bytes)
            .map_err(|e| RunError::MpoError(format!("sensus: {e}")))?;
        let depth_img = stereo_to_depth(&left, &right)
            .map_err(|e| RunError::MpoError(format!("sensus: {e}")))?;
        let out = depth_aware_blur(left, &depth_img, cli.focus, cli.strength * 0.023, kind)
            .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))?;
        let out_path = cli.output.as_ref().unwrap();
        return out.save(out_path).map_err(|source| RunError::OutputSave {
            path: out_path.clone(),
            source,
        });
    }

    // --portrait: Android XMP Depth
    if let Some(portrait_path) = cli.portrait {
        if cli.filter.len() > 1 {
            return Err(RunError::PortraitError(
                "sensus: --portrait cannot be combined with multiple filters".to_string(),
            ));
        }
        if !cli.filter.iter().any(|f| f.is_depth_filter()) {
            return Err(RunError::PortraitError(
                "sensus: --portrait requires a depth blur filter (myopia-depth, hyperopia-depth, depth-of-field)".to_string(),
            ));
        }
        if cli.depth.is_some() {
            return Err(RunError::PortraitError(
                "sensus: --portrait and --depth cannot be used together".to_string(),
            ));
        }

        let portrait_bytes =
            std::fs::read(&portrait_path).map_err(|source| RunError::PortraitRead {
                path: portrait_path.clone(),
                source,
            })?;
        let depth_map = read_xmp_depth(&portrait_bytes)
            .map_err(|e| RunError::PortraitError(format!("sensus: {e}")))?;

        let source_img = if let Some(ref inp) = cli.input {
            image::open(inp).map_err(|source| RunError::InputOpen {
                path: inp.clone(),
                source,
            })?
        } else {
            image::load_from_memory(&portrait_bytes).map_err(|source| RunError::InputOpen {
                path: portrait_path.clone(),
                source,
            })?
        };

        let kind = cli.filter[0].depth_kind().unwrap();
        let out = depth_aware_blur(source_img, &depth_map, cli.focus, cli.strength * 0.023, kind)
            .map_err(|e| RunError::PortraitError(format!("sensus: {e}")))?;
        let out_path = cli.output.as_ref().unwrap();
        return out.save(out_path).map_err(|source| RunError::OutputSave {
            path: out_path.clone(),
            source,
        });
    }

    // --mpo なし・--portrait なし → --input が必須
    let input_path = cli.input.ok_or_else(|| {
        RunError::InputRequired(
            "sensus: --input is required when --mpo and --portrait are not specified".to_string(),
        )
    })?;

    let img = image::open(&input_path).map_err(|source| RunError::InputOpen {
        path: input_path.clone(),
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
        let out_path = cli.output.as_ref().unwrap();
        return out.save(out_path).map_err(|source| RunError::OutputSave {
            path: out_path.clone(),
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
    if cli.filter.len() == 1 {
        let core_filter = cli.filter[0].to_core();
        if !matches!(core_filter, CoreFilter::Astigmatism { .. }) && cli.axis != 90.0 {
            eprintln!(
                "sensus: warning: --axis is only used with --filter astigmatism (ignored for {core_filter:?})"
            );
        }
        let uses_seed = matches!(core_filter, CoreFilter::Cataract | CoreFilter::Floaters { .. });
        let uses_floater_params = matches!(core_filter, CoreFilter::Floaters { .. });
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
        let uses_side = matches!(core_filter, CoreFilter::Hemianopia { .. });
        if !uses_side && cli.side != 0.0 {
            eprintln!(
                "sensus: warning: --side is only used with --filter hemianopia (ignored for {core_filter:?})"
            );
        }
    }

    let result = pipeline.apply(img);

    match result {
        Ok(out) => {
            let out_path = cli.output.as_ref().unwrap();
            out.save(out_path)
                .map_err(|source| RunError::OutputSave {
                    path: out_path.clone(),
                    source,
                })?;
            Ok(())
        }
        Err(CoreError::NotImplemented(filter)) => {
            let msg = format!(
                "sensus: filter {:?} (strength {:.2}) is not implemented yet.\n\
                 sensus: input {}x{} {:?} -> output {:?}\n\
                 sensus: see https://github.com/kako-jun/sensus for roadmap.",
                filter, cli.strength, width, height, input_path, cli.output
            );
            Err(RunError::NotImplemented(msg))
        }
        Err(CoreError::Image(err)) => Err(RunError::InputOpen {
            path: input_path.clone(),
            source: err,
        }),
        Err(e) => Err(RunError::Pipeline(format!("sensus: {e}"))),
    }
}

/// --audio モード: WAV を読み、`--hearing` の聴覚フィルタを [`AudioPipeline`] で
/// 順番に適用し、WAV に書き出す。
fn run_audio(cli: &Cli, audio_path: &Path) -> Result<(), RunError> {
    if cli.hearing.is_empty() {
        return Err(RunError::AudioError(
            "sensus: --audio requires at least one --hearing filter".to_string(),
        ));
    }
    if !cli.filter.is_empty() {
        return Err(RunError::AudioError(
            "sensus: --audio (hearing) cannot be combined with --filter (image filters)".to_string(),
        ));
    }
    let out_path = cli.output.as_ref().ok_or_else(|| {
        RunError::AudioError("sensus: --output <PATH> is required with --audio".to_string())
    })?;

    let (buf, spec) = audio::read_wav(audio_path).map_err(RunError::AudioError)?;

    let mut pipeline = AudioPipeline::new();
    for h in &cli.hearing {
        pipeline = pipeline.push(h.to_core(cli), cli.strength);
    }
    let out = pipeline
        .apply(&buf)
        .map_err(|e| RunError::AudioError(format!("sensus: {e}")))?;

    audio::write_wav(out_path, &out, spec).map_err(RunError::AudioError)
}

/// 画像にフィルタパイプラインを適用する（--pipe モードと通常モードの共通処理）。
///
/// # 通常モードとの差分
/// 通常モードの `run()` では pipeline 構築後に warning 出力（--axis / --seed 等の
/// 使われていないフラグに対する注意喚起）を行うが、--pipe モードでは省略している。
/// これはフレームごとに同じ warning が大量に出力されることを防ぐためである。
fn apply_filters_to_image(
    img: image::DynamicImage,
    cli: &Cli,
) -> Result<image::DynamicImage, RunError> {
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
    pipeline
        .apply(img)
        .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))
}

fn run_pipe(args: &Cli) -> Result<(), RunError> {
    use std::io::{Read, Write};
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;

    // FFD8 から FFD9 までを 1 フレームとして切り出す
    let frames = split_jpeg_frames(&buf);
    for frame_data in frames {
        let img = image::load_from_memory(frame_data).map_err(|source| RunError::InputOpen {
            path: std::path::PathBuf::from("<stdin>"),
            source,
        })?;
        let out = apply_filters_to_image(img, args)?;
        // JPEG エンコードして stdout に書く
        let mut jpeg_buf = Vec::new();
        out.write_to(
            &mut std::io::Cursor::new(&mut jpeg_buf),
            image::ImageFormat::Jpeg,
        )
        .map_err(|source| RunError::OutputSave {
            path: std::path::PathBuf::from("<stdout>"),
            source,
        })?;
        writer.write_all(&jpeg_buf)?;
    }
    Ok(())
}

fn split_jpeg_frames(data: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        // フレーム開始: SOI (FFD8)
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let frame_start = i;
        i += 2; // SOI を消費
        // マーカーを走査して EOI (FFD9) を探す
        'frame: loop {
            // 0xFF を探す
            while i < data.len() && data[i] != 0xFF {
                i += 1;
            }
            if i + 1 >= data.len() {
                break 'frame;
            }
            let marker = data[i + 1];
            match marker {
                0xD9 => {
                    // EOI: フレーム終端
                    frames.push(&data[frame_start..=i + 1]);
                    i += 2;
                    break 'frame;
                }
                0xD8 => {
                    // 別の SOI は不正（スキップ）
                    i += 2;
                }
                0x00 | 0xFF => {
                    // スタッフドバイト or padding、スキップ
                    i += 1;
                }
                0xD0..=0xD7 | 0x01 => {
                    // RST0-7, TEM: length フィールドなし
                    i += 2;
                }
                _ => {
                    // 通常マーカー: 2バイト length フィールドあり
                    if i + 3 >= data.len() {
                        break 'frame;
                    }
                    let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                    i += 2 + len; // マーカー2バイト + length (length 自身の2バイトを含む)
                }
            }
        }
    }
    frames
}
