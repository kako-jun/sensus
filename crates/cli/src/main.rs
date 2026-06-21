//! sensus CLI — simulate sensory perception on images.
//!
//! 画像/音声を読み、選択したフィルタを適用して出力する。エラーは stderr に通知し
//! 非ゼロ終了する（成功 0 / 失敗 1）。core の `Filter` は全バリアント実装済みで、
//! 未実装フィルタ経路（旧 exit-2）は持たない。
//!
//! # モジュール構成
//! - `arguments` — clap derive の `Cli` / ValueEnum（`Filter` / `Hearing`）/ `parse_*`（引数定義層）
//! - `filter_mapping` — CLI enum → core enum 変換と `warn_unused_flags`（CLI→core 変換層）
//! - `depth_resolver` — depth blur 統合と Pipeline 適用ヘルパー（depth blur 統合層）
//! - `main.rs`（本ファイル） — `main` / `run` / `run_audio` / `run_pipe` 等のオーケストレーション / I/O 層

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use sensus_core::{
    stereo::{read_xmp_depth, split_mpo, stereo_to_depth},
    vision::depth_aware_blur,
    AudioPipeline, Error as CoreError,
};
use thiserror::Error;

use sensus_core::pipeline::{FilterStep, Pipeline};

mod arguments;
mod audio;
mod depth_resolver;
mod filter_mapping;

use arguments::Cli;
use depth_resolver::{
    apply_filters_to_image, apply_non_depth_filters, depth_kinds, DEPTH_BLUR_MAX_RADIUS_RATIO,
};
use filter_mapping::warn_unused_flags;

/// CLI-internal error type. Keeps the `main` ↔ `run` boundary explicit so
/// integration tests can drive `run` directly without poking `process::exit`.
#[derive(Debug, Error)]
pub(crate) enum RunError {
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
    if let Some(mpo_path) = cli.mpo.clone() {
        // depth フィルタは 1 つだけ必須（#108: 非 depth フィルタは合成可能）
        let kinds = depth_kinds(&cli);
        if kinds.len() != 1 {
            return Err(RunError::MpoError(if kinds.is_empty() {
                "sensus: --mpo requires a depth blur filter (myopia-depth, hyperopia-depth, depth-of-field)".to_string()
            } else {
                "sensus: --mpo accepts at most one depth blur filter".to_string()
            }));
        }
        let kind = kinds[0];
        // --depth との同時指定は不可
        if cli.depth.is_some() {
            return Err(RunError::MpoError(
                "sensus: --mpo and --depth cannot be used together".to_string(),
            ));
        }

        let bytes = std::fs::read(&mpo_path).map_err(|source| RunError::MpoRead {
            path: mpo_path.clone(),
            source,
        })?;
        let (left, right) =
            split_mpo(&bytes).map_err(|e| RunError::MpoError(format!("sensus: {e}")))?;
        let depth_img = stereo_to_depth(&left, &right)
            .map_err(|e| RunError::MpoError(format!("sensus: {e}")))?;
        // #108: depth 以外のフィルタを左目画像に先に適用してから depth blur
        let base = apply_non_depth_filters(left, &cli)?;
        let out = depth_aware_blur(
            base,
            &depth_img,
            cli.focus,
            cli.strength * DEPTH_BLUR_MAX_RADIUS_RATIO,
            kind,
        )
        .map_err(|e| RunError::Pipeline(format!("sensus: {e}")))?;
        let out_path = cli.output.as_ref().unwrap();
        return out.save(out_path).map_err(|source| RunError::OutputSave {
            path: out_path.clone(),
            source,
        });
    }

    // --portrait: Android XMP Depth
    if let Some(portrait_path) = cli.portrait.clone() {
        // depth フィルタは 1 つだけ必須（#108: 非 depth フィルタは合成可能）
        let kinds = depth_kinds(&cli);
        if kinds.len() != 1 {
            return Err(RunError::PortraitError(if kinds.is_empty() {
                "sensus: --portrait requires a depth blur filter (myopia-depth, hyperopia-depth, depth-of-field)".to_string()
            } else {
                "sensus: --portrait accepts at most one depth blur filter".to_string()
            }));
        }
        let kind = kinds[0];
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

        // #108: depth 以外のフィルタを先に適用してから depth blur
        let base = apply_non_depth_filters(source_img, &cli)?;
        let out = depth_aware_blur(
            base,
            &depth_map,
            cli.focus,
            cli.strength * DEPTH_BLUR_MAX_RADIUS_RATIO,
            kind,
        )
        .map_err(|e| RunError::PortraitError(format!("sensus: {e}")))?;
        let out_path = cli.output.as_ref().unwrap();
        return out.save(out_path).map_err(|source| RunError::OutputSave {
            path: out_path.clone(),
            source,
        });
    }

    // --mpo なし・--portrait なし → --input が必須
    let input_path = cli.input.clone().ok_or_else(|| {
        RunError::InputRequired(
            "sensus: --input is required when --mpo and --portrait are not specified".to_string(),
        )
    })?;

    let img = image::open(&input_path).map_err(|source| RunError::InputOpen {
        path: input_path.clone(),
        source,
    })?;

    // depth フィルタが含まれる場合（#108: 非 depth フィルタを先に合成してから depth blur）
    if cli.filter.iter().any(|f| f.is_depth_filter()) {
        let kinds = depth_kinds(&cli);
        if kinds.len() != 1 {
            return Err(RunError::Pipeline(
                "sensus: exactly one depth blur filter is allowed (myopia-depth / hyperopia-depth / depth-of-field)".to_string(),
            ));
        }
        let kind = kinds[0];

        let depth_path = cli.depth.as_ref().ok_or_else(|| {
            RunError::Pipeline(
                "sensus: --depth <PATH> is required for depth blur filters".to_string(),
            )
        })?;
        let depth_img = image::open(depth_path).map_err(|source| RunError::InputOpen {
            path: depth_path.clone(),
            source,
        })?;
        // #108: depth 以外のフィルタを先に適用してから depth blur で合成
        let base = apply_non_depth_filters(img, &cli)?;
        let out = depth_aware_blur(
            base,
            &depth_img,
            cli.focus,
            cli.strength * DEPTH_BLUR_MAX_RADIUS_RATIO,
            kind,
        )
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
        let core_filter = f.to_core(&cli);
        pipeline = pipeline.push(FilterStep::new(core_filter, cli.strength));
    }
    if cli.filter.len() == 1 {
        warn_unused_flags(&cli, cli.filter[0].to_core(&cli));
    }

    let result = pipeline.apply(img);

    match result {
        Ok(out) => {
            let out_path = cli.output.as_ref().unwrap();
            out.save(out_path).map_err(|source| RunError::OutputSave {
                path: out_path.clone(),
                source,
            })?;
            Ok(())
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
            "sensus: --audio (hearing) cannot be combined with --filter (image filters)"
                .to_string(),
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
