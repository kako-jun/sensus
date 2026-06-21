//! CLI 引数定義層。
//!
//! clap derive で定義する [`Cli`] struct と、`--filter` / `--hearing` の
//! ValueEnum（[`Filter`] / [`Hearing`]）、および各種 `--flag` の値バリデータ
//! （`parse_*`）を持つ。CLI→core への変換は `filter_mapping` モジュールが担う。

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Filter {
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
    // Phase N: 変視症・コントラスト感度低下・ディテールロス・閃輝暗点 (Issue #55-#59)
    Metamorphopsia,
    ContrastSensitivity,
    DetailLoss,
    Teichopsia,
    FlickeringStars,
    // Phase N: 深度マップ対応距離依存ぼけ (Issue #19)
    MyopiaDepth,
    HyperopiaDepth,
    DepthOfField,
}

/// CLI-facing hearing filter enum (clap derive). Maps to core [`sensus_core::HearingFilter`].
/// `--audio` モードで `--hearing` に指定する（#105）。
//
// `HearingLoss` は疾患名（難聴）であって enum 名の重複ではないため allow する。
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Hearing {
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

/// sensus — simulate sensory perception on images.
#[derive(Debug, Parser)]
#[command(name = "sensus", version, about, long_about = None)]
pub(crate) struct Cli {
    /// Input image path (PNG / JPEG / WebP, etc.). Not required when --mpo is used.
    #[arg(short, long)]
    pub(crate) input: Option<PathBuf>,

    /// 出力ファイルパス（--pipe 時は不要）
    #[arg(short, long, required_unless_present = "pipe")]
    pub(crate) output: Option<PathBuf>,

    /// Filter(s) to apply. Specify multiple times to chain filters.
    #[arg(short, long, value_enum, num_args = 1..)]
    pub(crate) filter: Vec<Filter>,

    /// Filter strength in 0.0..=1.0 (0.0 = original, 1.0 = full effect).
    #[arg(short, long, default_value_t = 1.0, value_parser = parse_strength)]
    pub(crate) strength: f32,

    /// Astigmatism axis in degrees (0.0..=180.0). Only used with
    /// `--filter astigmatism`. Default `90.0` (with-the-rule astigmatism:
    /// vertical sharp, horizontal blurred).
    #[arg(long, default_value_t = 90.0, value_parser = parse_axis)]
    pub(crate) axis: f32,

    /// Random seed for stochastic filters (cataract, floaters). Default: 0.
    #[arg(long, default_value = "0")]
    pub(crate) seed: u64,

    /// Floater density in 0.0..=1.0. Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    pub(crate) density: f32,

    /// Gaze X position in 0.0..=1.0 (0=left, 1=right). Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    pub(crate) gaze_x: f32,

    /// Gaze Y position in 0.0..=1.0 (0=top, 1=bottom). Only used with --filter floaters.
    #[arg(long, default_value = "0.5")]
    pub(crate) gaze_y: f32,

    /// Floater size multiplier (0.1..=5.0). 1.0 = default blob radius / thread width.
    /// Only used with --filter floaters.
    #[arg(long, default_value = "1.0", value_parser = parse_floater_size)]
    pub(crate) size: f32,

    /// Hemianopia side: 0.0 = left field lost, 1.0 = right field lost.
    /// Only used with --filter hemianopia.
    #[arg(long, default_value = "0.0")]
    pub(crate) side: f32,

    /// Depth map image path (PNG / JPEG / etc.). Only used with depth blur filters.
    #[arg(long)]
    pub(crate) depth: Option<PathBuf>,

    /// MPO stereo image path. Automatically generates a depth map and applies depth blur.
    /// Requires a depth blur filter (--filter myopia-depth / hyperopia-depth / depth-of-field).
    #[arg(long)]
    pub(crate) mpo: Option<PathBuf>,

    /// Android portrait-mode JPEG path. Extracts XMP depth map and applies depth blur.
    /// Requires a depth blur filter (--filter myopia-depth / hyperopia-depth / depth-of-field).
    /// --input is not required when --portrait is used.
    #[arg(long, conflicts_with = "mpo")]
    pub(crate) portrait: Option<PathBuf>,

    /// Focus depth in 0.0..=1.0 (bright=near, dark=far). Only used with depth blur filters.
    #[arg(long, default_value = "0.5", value_parser = parse_focus)]
    pub(crate) focus: f32,

    /// Diplopia horizontal offset in min(W,H) ratio (-1.0..=1.0). Default: 0.02
    #[arg(long, default_value = "0.02", value_parser = parse_signed_ratio)]
    pub(crate) offset_x: f32,

    /// Diplopia vertical offset in min(W,H) ratio (-1.0..=1.0). Default: 0.01
    #[arg(long, default_value = "0.01", value_parser = parse_signed_ratio)]
    pub(crate) offset_y: f32,

    /// Diplopia ghost image strength (0.0..=1.0). Default: 0.7
    #[arg(long, default_value = "0.7", value_parser = parse_ratio)]
    pub(crate) ghost_strength: f32,

    /// Nystagmus amplitude in min(W,H) ratio. Default: 0.03
    #[arg(long, default_value = "0.03", value_parser = parse_ratio)]
    pub(crate) amplitude: f32,

    /// Nystagmus direction in degrees (0=horizontal, 90=vertical). Default: 0.0
    #[arg(long, default_value = "0.0", value_parser = parse_direction_deg)]
    pub(crate) direction_deg: f32,

    /// Starbursts number of rays. Default: 6
    #[arg(long, default_value = "6")]
    pub(crate) num_rays: u32,

    /// Starbursts ray length in min(W,H) ratio. Default: 0.1
    #[arg(long, default_value = "0.1", value_parser = parse_ratio)]
    pub(crate) ray_length: f32,

    /// Starbursts brightness threshold (0.0..=1.0). Default: 0.8
    #[arg(long, default_value = "0.8", value_parser = parse_ratio)]
    pub(crate) threshold: f32,

    /// Starbursts chromatic dispersion (0.0..=1.0). 0.0 = white rays, 1.0 = full rainbow.
    /// Only used with --filter starbursts. Default: 0.0
    #[arg(long, default_value = "0.0", value_parser = parse_ratio)]
    pub(crate) dispersion: f32,

    /// Detail-loss tile size in pixels (>= 1; 1 = no effect). Only used with --filter detail-loss.
    /// Default: 8
    #[arg(long, default_value = "8")]
    pub(crate) cell_size: u32,

    /// Metamorphopsia distortion spatial frequency. Only used with --filter metamorphopsia.
    /// Default: 4.0
    #[arg(long, default_value = "4.0", value_parser = parse_meta_freq)]
    pub(crate) meta_freq: f32,

    /// Metamorphopsia distortion-field seed. Only used with --filter metamorphopsia. Default: 0
    #[arg(long, default_value = "0")]
    pub(crate) meta_seed: u64,

    /// Read JPEG frames from stdin and write filtered JPEG frames to stdout (ffmpeg pipe mode).
    /// Cannot be combined with --input.
    #[arg(long, conflicts_with = "input")]
    pub(crate) pipe: bool,

    /// Input audio path (WAV). Routes to hearing-filter mode; requires --hearing and --output.
    /// Cannot be combined with image filters (--filter) or --pipe.
    #[arg(long, conflicts_with_all = ["input", "pipe", "mpo", "portrait"])]
    pub(crate) audio: Option<PathBuf>,

    /// Hearing filter(s) to apply to --audio. Specify multiple times to chain.
    #[arg(long, value_enum, num_args = 1..)]
    pub(crate) hearing: Vec<Hearing>,

    /// Frequency (Hz) for --hearing tinnitus / sudden-hearing-loss / misophonia. Default: 4000.
    #[arg(long, default_value_t = 4000.0, value_parser = parse_freq)]
    pub(crate) freq: f32,

    /// Semitones for --hearing pitch-shift (negative = lower, positive = higher). Default: 0.0
    #[arg(long, default_value_t = 0.0, value_parser = parse_semitones)]
    pub(crate) semitones: f32,
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

/// Parse `--size` (floater size multiplier) in `0.1..=5.0`.
/// core 側も同じ範囲に clamp するが (#110)、誤入力を早期に弾く。
fn parse_floater_size(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || !(0.1..=5.0).contains(&v) {
        return Err(format!("size must be in 0.1..=5.0, got {v}"));
    }
    Ok(v)
}

/// Parse `--meta-freq` (metamorphopsia spatial frequency). 有限かつ正の値。
fn parse_meta_freq(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || v <= 0.0 {
        return Err(format!("meta-freq must be finite and > 0.0, got {v}"));
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
        return Err(format!(
            "freq must be in 0.0 (exclusive)..=20000.0 Hz, got {v}"
        ));
    }
    Ok(v)
}

/// Parse `--semitones` for pitch-shift. 有限値かつ ±48 半音（±4 オクターブ）以内。
fn parse_semitones(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|e: std::num::ParseFloatError| e.to_string())?;
    if !v.is_finite() || !(-48.0..=48.0).contains(&v) {
        return Err(format!(
            "semitones must be finite and in -48.0..=48.0, got {v}"
        ));
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
