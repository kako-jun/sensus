//! WAV 音声の入出力（#105）。
//!
//! sensus-core は I/O を持たない pure ライブラリなので、CLI 側で WAV ↔ [`AudioBuffer`]
//! の変換を担う。対象は WAV のみ（mp3/flac/ogg 等の広域デコードは非対象）。
//!
//! サンプルは内部的に正規化 f32（-1.0..=1.0）で扱い、出力時に入力の bit 深度・
//! サンプル形式（整数 / 浮動小数）へ戻す。チャンネル数・サンプルレートは
//! フィルタ適用後のバッファ（diplacusis 等で mono→stereo になりうる）に追従させる。

use std::path::Path;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use sensus_core::hearing::AudioBuffer;

/// WAV ファイルを読み、正規化 f32 の [`AudioBuffer`] と元の [`WavSpec`] を返す。
///
/// 元 spec は出力時に bit 深度・サンプル形式を保つために返す。
pub fn read_wav(path: &Path) -> Result<(AudioBuffer, WavSpec), String> {
    let reader =
        WavReader::open(path).map_err(|e| format!("failed to open WAV {path:?}: {e}"))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| format!("failed to read float WAV samples: {e}"))?,
        SampleFormat::Int => {
            // i{bits} → f32 正規化。フルスケールは 2^(bits-1)。
            let scale = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|e| format!("failed to read integer WAV samples: {e}"))?
        }
    };

    Ok((
        AudioBuffer {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        },
        spec,
    ))
}

/// [`AudioBuffer`] を WAV ファイルに書く。
///
/// bit 深度・サンプル形式は `src_spec`（入力 WAV の spec）を踏襲するが、
/// チャンネル数・サンプルレートは `buf` 側に追従させる。
pub fn write_wav(path: &Path, buf: &AudioBuffer, src_spec: WavSpec) -> Result<(), String> {
    let out_spec = WavSpec {
        channels: buf.channels,
        sample_rate: buf.sample_rate,
        bits_per_sample: src_spec.bits_per_sample,
        sample_format: src_spec.sample_format,
    };
    let mut writer =
        WavWriter::create(path, out_spec).map_err(|e| format!("failed to create WAV {path:?}: {e}"))?;

    match src_spec.sample_format {
        SampleFormat::Float => {
            for &s in &buf.samples {
                writer
                    .write_sample(s)
                    .map_err(|e| format!("failed to write float WAV sample: {e}"))?;
            }
        }
        SampleFormat::Int => {
            // フルスケール - 1 にスケール（+1.0 が範囲外にならないようにする）。
            let max = (1i64 << (src_spec.bits_per_sample.saturating_sub(1))) as f32 - 1.0;
            for &s in &buf.samples {
                let v = (s.clamp(-1.0, 1.0) * max).round() as i32;
                writer
                    .write_sample(v)
                    .map_err(|e| format!("failed to write integer WAV sample: {e}"))?;
            }
        }
    }

    writer
        .finalize()
        .map_err(|e| format!("failed to finalize WAV {path:?}: {e}"))
}
