//! Integration tests for --audio (hearing filter) mode (Issue #105).

use std::path::PathBuf;
use std::process::Command;

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

fn sensus_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sensus"))
}

/// 16-bit PCM の 440 Hz サイン波 WAV を生成して path に書く。
fn write_sine_wav(path: &std::path::Path, freq: f32, frames: usize, sample_rate: u32, channels: u16) {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        let t = i as f32 / sample_rate as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin();
        let v = (s * 30000.0) as i16;
        for _ in 0..channels {
            writer.write_sample(v).unwrap();
        }
    }
    writer.finalize().unwrap();
}

fn read_wav_samples(path: &std::path::Path) -> (Vec<f32>, WavSpec) {
    let reader = WavReader::open(path).unwrap();
    let spec = reader.spec();
    let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Float => reader.into_samples::<f32>().map(|s| s.unwrap()).collect(),
        SampleFormat::Int => reader
            .into_samples::<i32>()
            .map(|s| s.unwrap() as f32 / scale)
            .collect(),
    };
    (samples, spec)
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|&x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

#[test]
fn audio_hearing_loss_produces_wav() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("in.wav");
    let out_path = dir.path().join("out.wav");
    // 8 kHz の高音サイン波 → 難聴(高音カット)で大きく減衰するはず
    write_sine_wav(&in_path, 8000.0, 44100, 44100, 1);

    let status = Command::new(sensus_bin())
        .args([
            "--audio",
            in_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--hearing",
            "hearing-loss",
            "-s",
            "1.0",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "sensus --audio should succeed");
    assert!(out_path.exists(), "output WAV should be written");

    let (in_samples, _) = read_wav_samples(&in_path);
    let (out_samples, out_spec) = read_wav_samples(&out_path);
    assert_eq!(out_spec.sample_rate, 44100);
    assert_eq!(out_spec.channels, 1);
    // 高音域カットで RMS が顕著に下がる
    assert!(
        rms(&out_samples) < rms(&in_samples) * 0.8,
        "hearing-loss should attenuate the 8 kHz tone (in_rms={}, out_rms={})",
        rms(&in_samples),
        rms(&out_samples)
    );
}

#[test]
fn audio_chains_multiple_hearing_filters() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("in.wav");
    let out_path = dir.path().join("out.wav");
    write_sine_wav(&in_path, 440.0, 22050, 44100, 1);

    let status = Command::new(sensus_bin())
        .args([
            "--audio",
            in_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--hearing",
            "hearing-loss",
            "tinnitus",
            "--freq",
            "4000",
            "-s",
            "0.8",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "chained hearing filters should succeed");
    let (out_samples, _) = read_wav_samples(&out_path);
    assert!(rms(&out_samples) > 0.0);
}

#[test]
fn audio_without_hearing_filter_fails() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("in.wav");
    let out_path = dir.path().join("out.wav");
    write_sine_wav(&in_path, 440.0, 1000, 44100, 1);

    let output = Command::new(sensus_bin())
        .args([
            "--audio",
            in_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "--audio without --hearing must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--hearing"), "error should mention --hearing: {stderr}");
}

#[test]
fn audio_diplacusis_mono_to_stereo() {
    // diplacusis は mono → stereo に変換する。出力 WAV が 2ch になることを確認。
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("in.wav");
    let out_path = dir.path().join("out.wav");
    write_sine_wav(&in_path, 440.0, 4000, 44100, 1);

    let status = Command::new(sensus_bin())
        .args([
            "--audio",
            in_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--hearing",
            "diplacusis",
            "-s",
            "1.0",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let (_, out_spec) = read_wav_samples(&out_path);
    assert_eq!(out_spec.channels, 2, "diplacusis output should be stereo");
}

#[test]
fn audio_conflicts_with_image_filter() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("in.wav");
    let out_path = dir.path().join("out.wav");
    write_sine_wav(&in_path, 440.0, 1000, 44100, 1);

    // --audio と --filter(画像) は併用不可（clap の conflicts でも弾かれるが念のため）
    let output = Command::new(sensus_bin())
        .args([
            "--audio",
            in_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--hearing",
            "hearing-loss",
            "--filter",
            "protanopia",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "--audio + --filter must fail");
}
