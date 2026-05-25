//! Hearing filters: hearing loss, pitch shift, balance / vertigo, etc.
//!
//! Phase 4 (Issue #7/#8/#9) で実装。
//!
//! # 音声バッファ
//!
//! - [`AudioBuffer`] — `Vec<f32>` の PCM サンプル列（normalized, -1.0..=1.0）
//! - サンプルはインターリーブ: ch0[0], ch1[0], ch0[1], ch1[0], ...
//! - `channels` が 1 のときはモノラル
//!
//! # 周波数フィルタ
//!
//! FFT 不使用。Butterworth 近似の Biquad IIR フィルタを使用する。
//! 依存クレートは追加しない。

use std::f32::consts::PI;

// ---------------------------------------------------------------
// AudioBuffer
// ---------------------------------------------------------------

/// PCM 音声バッファ。
///
/// `samples` はインターリーブ形式: `[ch0_frame0, ch1_frame0, ch0_frame1, ch1_frame1, ...]`
/// モノラル (`channels == 1`) の場合は単純な `[sample0, sample1, ...]`。
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioBuffer {
    /// フレーム数（チャンネルあたりのサンプル数）を返す。
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }
}

// ---------------------------------------------------------------
// Biquad IIR フィルタ
// ---------------------------------------------------------------

/// Biquad (双二次) IIR フィルタ。
///
/// Direct Form II Transposed で実装。係数は外部から設定する。
/// ステート (`z1`, `z2`) はフィルタの内部状態で、連続呼び出し時の継続性を保つ。
///
/// 伝達関数: `H(z) = (b0 + b1*z^-1 + b2*z^-2) / (1 + a1*z^-1 + a2*z^-2)`
#[derive(Debug, Clone)]
pub struct BiquadFilter {
    /// 順方向係数
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    /// フィードバック係数（符号: H(z) の分母の a1, a2 に対応）
    pub a1: f32,
    pub a2: f32,
    /// 内部ステート (Direct Form II Transposed)
    pub z1: f32,
    pub z2: f32,
}

impl BiquadFilter {
    /// 1 サンプルを処理して出力を返す。ステートは更新される。
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// ステートをリセットする（無音状態に戻す）。
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

// ---------------------------------------------------------------
// フィルタ係数計算
// ---------------------------------------------------------------

/// Butterworth 近似の 2 次ローパスフィルタ係数を返す。
///
/// `freq_hz`: カットオフ周波数 (Hz)
/// `sample_rate`: サンプルレート (Hz)
pub fn low_pass_biquad(freq_hz: f32, sample_rate: u32) -> BiquadFilter {
    let fs = sample_rate as f32;
    // バイリニア変換による Butterworth 2 次 LP
    let f0 = freq_hz.clamp(1.0, fs * 0.4999);
    let w0 = 2.0 * PI * f0 / fs;
    let q = 0.7071_f32; // Butterworth Q = 1/sqrt(2)
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();

    let b0 = (1.0 - cos_w0) / 2.0;
    let b1 = 1.0 - cos_w0;
    let b2 = (1.0 - cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    BiquadFilter {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Butterworth 近似の 2 次ハイパスフィルタ係数を返す。
///
/// `freq_hz`: カットオフ周波数 (Hz)
/// `sample_rate`: サンプルレート (Hz)
pub fn high_pass_biquad(freq_hz: f32, sample_rate: u32) -> BiquadFilter {
    let fs = sample_rate as f32;
    let f0 = freq_hz.clamp(1.0, fs * 0.4999);
    let w0 = 2.0 * PI * f0 / fs;
    let q = 0.7071_f32;
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();

    let b0 = (1.0 + cos_w0) / 2.0;
    let b1 = -(1.0 + cos_w0);
    let b2 = (1.0 + cos_w0) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    BiquadFilter {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// ノッチ（バンドリジェクト）フィルタ係数を返す。
///
/// `center_hz`: 中心周波数 (Hz)
/// `bandwidth_hz`: 帯域幅 (Hz)
/// `sample_rate`: サンプルレート (Hz)
pub fn band_reject_biquad(center_hz: f32, bandwidth_hz: f32, sample_rate: u32) -> BiquadFilter {
    let fs = sample_rate as f32;
    let f0 = center_hz.clamp(1.0, fs * 0.4999);
    let w0 = 2.0 * PI * f0 / fs;
    let bw = bandwidth_hz.max(1.0);
    // Q = f0 / BW
    let q = (f0 / bw).max(0.1);
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();

    let b0 = 1.0;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;

    BiquadFilter {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

// ---------------------------------------------------------------
// フィルタを全チャンネルに適用するヘルパー
// ---------------------------------------------------------------

/// 全チャンネルに同一 Biquad フィルタを適用する（チャンネルごとに独立したステート）。
fn apply_biquad_all_channels(buf: &AudioBuffer, make_filter: impl Fn() -> BiquadFilter) -> Vec<f32> {
    let ch = buf.channels as usize;
    if ch == 0 {
        return buf.samples.clone();
    }
    // チャンネルごとにフィルタインスタンスを生成（ステートを独立させる）
    let mut filters: Vec<BiquadFilter> = (0..ch).map(|_| make_filter()).collect();
    let mut out = buf.samples.clone();
    let frames = buf.frames();
    for frame in 0..frames {
        for c in 0..ch {
            let idx = frame * ch + c;
            out[idx] = filters[c].process(buf.samples[idx]);
        }
    }
    out
}

/// strength で元のサンプルとフィルタ済みサンプルをブレンドする。
fn blend(original: &[f32], filtered: &[f32], strength: f32) -> Vec<f32> {
    let s = strength.clamp(0.0, 1.0);
    original
        .iter()
        .zip(filtered.iter())
        .map(|(&o, &f)| o + (f - o) * s)
        .collect()
}

// ---------------------------------------------------------------
// 公開フィルタ関数
// ---------------------------------------------------------------

/// 難聴（hearing loss）シミュレーション。
///
/// 高音域カットのローパスフィルタ。
/// `strength = 0.0` でカットオフ 20000 Hz（変化なし）、
/// `strength = 1.0` でカットオフ 500 Hz（高音がほぼ聞こえない）。
pub fn hearing_loss(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    // strength=0 → 20000 Hz, strength=1 → 500 Hz（対数補間）
    let cutoff = 20000.0_f32 * (500.0_f32 / 20000.0_f32).powf(s);
    let sr = buf.sample_rate;
    let filtered = apply_biquad_all_channels(&buf, || low_pass_biquad(cutoff, sr));
    AudioBuffer {
        samples: blend(&buf.samples, &filtered, s),
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 突発性難聴（sudden hearing loss）シミュレーション。
///
/// 特定の周波数帯域をノッチフィルタで削る。
/// `freq_hz`: 損失する中心周波数。帯域幅は `strength` に応じて広がる。
pub fn sudden_hearing_loss(buf: AudioBuffer, strength: f32, freq_hz: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    // bandwidth は strength に応じて 50 Hz から 1000 Hz に拡大
    let bandwidth = 50.0 + s * 950.0;
    let sr = buf.sample_rate;
    let filtered = apply_biquad_all_channels(&buf, || band_reject_biquad(freq_hz, bandwidth, sr));
    AudioBuffer {
        samples: blend(&buf.samples, &filtered, s),
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 騒音性難聴（noise induced hearing loss）シミュレーション。
///
/// 4 kHz 付近のバンドリジェクトフィルタ。強い騒音への曝露で生じる典型的難聴パターン。
pub fn noise_induced_hearing_loss(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    // 4 kHz ± 1 kHz（中心 4000 Hz, 帯域幅 2000 Hz）
    sudden_hearing_loss(buf, strength, 4000.0)
}

/// 耳鳴り（tinnitus）シミュレーション。
///
/// 指定周波数の正弦波を音声にミックスする。
/// `freq_hz`: 耳鳴りの周波数（典型的に 4000-8000 Hz）。
pub fn tinnitus(buf: AudioBuffer, strength: f32, freq_hz: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    let sr = buf.sample_rate as f32;
    let ch = buf.channels as usize;
    let frames = buf.frames();
    let mut out = buf.samples.clone();

    // 正弦波を全チャンネルにミックス
    for frame in 0..frames {
        let t = frame as f32 / sr;
        let sine = (2.0 * PI * freq_hz * t).sin() * s * 0.3; // 最大振幅 0.3
        for c in 0..ch {
            let idx = frame * ch + c;
            out[idx] = (out[idx] + sine).clamp(-1.0, 1.0);
        }
    }

    AudioBuffer {
        samples: out,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 音響過敏（hyperacusis）シミュレーション。
///
/// 音量を異常に増幅しハードクリッピングを加える。
/// `strength = 1.0` で 4 倍増幅 + クリッピング。
pub fn hyperacusis(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    let gain = 1.0 + s * 3.0; // 1.0 → 4.0
    let samples = buf.samples.iter().map(|&x| (x * gain).clamp(-1.0, 1.0)).collect();
    AudioBuffer {
        samples,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 変音（paracusis）シミュレーション。
///
/// ソフトクリッピング（3次倍音歪み）で金属的・歪んだ質感を付加する。
pub fn paracusis(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    // tanh ソフトクリッピング: より自然な歪み感
    let drive = 1.0 + s * 4.0; // 歪み量
    let samples = buf
        .samples
        .iter()
        .map(|&x| {
            let driven = x * drive;
            let distorted = driven.tanh(); // ソフトクリップ
            // 元とブレンド
            x + (distorted - x) * s
        })
        .collect();
    AudioBuffer {
        samples,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 音楽音痴（amusia）シミュレーション。
///
/// 強いローパスフィルタで音程情報（倍音構造）を潰す。
/// 基音は残るが、音程の差が識別しにくくなる。
pub fn amusia(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    // strength=0 → 8000 Hz, strength=1 → 200 Hz
    let cutoff = 8000.0_f32 * (200.0_f32 / 8000.0_f32).powf(s);
    let sr = buf.sample_rate;
    let filtered = apply_biquad_all_channels(&buf, || low_pass_biquad(cutoff, sr));
    AudioBuffer {
        samples: blend(&buf.samples, &filtered, s),
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// ジスメロディア（dysmelodia）シミュレーション。
///
/// 音楽を不快・歪んだ音に変換する。
/// ハイパスフィルタで低音を除去しつつ、高調波歪みを加える。
pub fn dysmelodia(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }

    // 1) 高周波強調のためにハイパスフィルタをかける
    let sr = buf.sample_rate;
    let hp_cutoff = 500.0 + s * 2000.0;
    let hp_samples = apply_biquad_all_channels(&buf, || high_pass_biquad(hp_cutoff, sr));

    // 2) ハイパス成分を歪ませる（tanh）
    let drive = 1.0 + s * 2.0;
    let distorted: Vec<f32> = hp_samples.iter().map(|&x| (x * drive).tanh()).collect();

    // 3) 元信号と歪み信号をブレンド
    let out: Vec<f32> = buf
        .samples
        .iter()
        .zip(distorted.iter())
        .map(|(&orig, &dist)| (orig + dist * s * 0.5).clamp(-1.0, 1.0))
        .collect();

    AudioBuffer {
        samples: out,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 音程シフト（pitch shift）。
///
/// Linear resampling による音程シフト。
/// `semitones`: 半音単位のシフト量（正で高く、負で低く）。
/// 比率 `ratio = 2^(semitones/12)` でリサンプリングする。
///
/// # 注意
/// Linear resampling は簡易実装のため、大きなシフト量では音質劣化がある。
pub fn pitch_shift_semitones(buf: AudioBuffer, semitones: f32) -> AudioBuffer {
    if semitones == 0.0 {
        return buf;
    }
    let ratio = 2.0_f32.powf(semitones / 12.0);
    resample_linear(&buf, ratio)
}

/// linear resampling でバッファを再サンプリングする（内部ヘルパー）。
///
/// `ratio > 1.0` で音程が上がり（速い再生）、`ratio < 1.0` で音程が下がる（遅い再生）。
/// 出力は入力と同じフレーム数になるようにゼロパディングまたはトリミングする。
fn resample_linear(buf: &AudioBuffer, ratio: f32) -> AudioBuffer {
    let ch = buf.channels as usize;
    if ch == 0 || buf.samples.is_empty() {
        return buf.clone();
    }
    let frames_in = buf.frames();
    let frames_out = frames_in; // 出力フレーム数は同じ（時間長を保つ）
    let mut out = vec![0.0_f32; frames_out * ch];

    for frame_out in 0..frames_out {
        // ratio > 1 のとき、より速くソースを読む（音程が上がる）
        let src_pos = frame_out as f32 * ratio;
        let src_lo = src_pos.floor() as usize;
        let src_hi = src_lo + 1;
        let frac = src_pos - src_lo as f32;

        for c in 0..ch {
            let s0 = if src_lo < frames_in {
                buf.samples[src_lo * ch + c]
            } else {
                0.0
            };
            let s1 = if src_hi < frames_in {
                buf.samples[src_hi * ch + c]
            } else {
                0.0
            };
            out[frame_out * ch + c] = s0 + (s1 - s0) * frac;
        }
    }

    AudioBuffer {
        samples: out,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// ダイプラクシス（diplacusis）シミュレーション。
///
/// 左右の耳で同じ音を異なる音程で知覚する。
/// 左チャンネルを `+strength * 0.5` 半音、右チャンネルを `-strength * 0.5` 半音シフト。
/// モノラル入力の場合は stereo に変換してから処理する。
pub fn diplacusis(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }

    let shift_semitones = s * 0.5;

    // ステレオに変換（必要に応じて）
    let stereo = if buf.channels == 1 {
        // モノラル → ステレオ（L/R を複製）
        let frames = buf.frames();
        let mut stereo_samples = Vec::with_capacity(frames * 2);
        for &sample in &buf.samples {
            stereo_samples.push(sample);
            stereo_samples.push(sample);
        }
        AudioBuffer {
            samples: stereo_samples,
            sample_rate: buf.sample_rate,
            channels: 2,
        }
    } else {
        buf.clone()
    };

    // 左チャンネルのみを +shift_semitones シフト
    let left_shifted = {
        let ch = stereo.channels as usize;
        let frames = stereo.frames();
        let ratio = 2.0_f32.powf(shift_semitones / 12.0);
        let mut left_mono = AudioBuffer {
            samples: (0..frames).map(|f| stereo.samples[f * ch]).collect(),
            sample_rate: stereo.sample_rate,
            channels: 1,
        };
        left_mono = resample_linear(&left_mono, ratio);
        left_mono
    };

    // 右チャンネルのみを -shift_semitones シフト
    let right_shifted = {
        let ch = stereo.channels as usize;
        let frames = stereo.frames();
        let ratio = 2.0_f32.powf(-shift_semitones / 12.0);
        let mut right_mono = AudioBuffer {
            samples: (0..frames).map(|f| stereo.samples[f * ch + 1]).collect(),
            sample_rate: stereo.sample_rate,
            channels: 1,
        };
        right_mono = resample_linear(&right_mono, ratio);
        right_mono
    };

    // L/R を再インターリーブ
    let frames = stereo.frames();
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        out.push(left_shifted.samples[f]);
        out.push(right_shifted.samples[f]);
    }

    AudioBuffer {
        samples: out,
        sample_rate: stereo.sample_rate,
        channels: 2,
    }
}

// ---------------------------------------------------------------
// テスト
// ---------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn silence(frames: usize, sr: u32, ch: u16) -> AudioBuffer {
        AudioBuffer {
            samples: vec![0.0; frames * ch as usize],
            sample_rate: sr,
            channels: ch,
        }
    }

    fn sine_wave(freq: f32, frames: usize, sr: u32) -> AudioBuffer {
        let samples: Vec<f32> = (0..frames)
            .map(|i| (2.0 * PI * freq * i as f32 / sr as f32).sin())
            .collect();
        AudioBuffer {
            samples,
            sample_rate: sr,
            channels: 1,
        }
    }

    #[test]
    fn hearing_loss_strength_zero_is_identity() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = hearing_loss(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn sudden_hearing_loss_strength_zero_is_identity() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = sudden_hearing_loss(buf, 0.0, 4000.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn tinnitus_adds_noise_above_silence() {
        let buf = silence(44100, 44100, 1);
        let out = tinnitus(buf, 1.0, 4000.0);
        // 無音に耳鳴りが加わるので RMS > 0
        let rms: f32 = (out.samples.iter().map(|&x| x * x).sum::<f32>() / out.samples.len() as f32).sqrt();
        assert!(rms > 0.0, "tinnitus should add signal to silence");
    }

    #[test]
    fn hyperacusis_clips_loud_signal() {
        let buf = AudioBuffer {
            samples: vec![0.8; 100],
            sample_rate: 44100,
            channels: 1,
        };
        let out = hyperacusis(buf, 1.0);
        // 0.8 * 4.0 = 3.2 → クリップして 1.0
        assert!(out.samples.iter().all(|&x| x <= 1.0));
        assert!(out.samples[0] > 0.8, "hyperacusis should amplify");
    }

    #[test]
    fn pitch_shift_identity_at_zero() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = pitch_shift_semitones(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn audio_buffer_frames() {
        let buf = AudioBuffer {
            samples: vec![0.0; 200],
            sample_rate: 44100,
            channels: 2,
        };
        assert_eq!(buf.frames(), 100);
    }

    #[test]
    fn diplacusis_stereo_output() {
        let buf = sine_wave(440.0, 1000, 44100);
        let out = diplacusis(buf, 1.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 2000);
    }
}
