//! Hearing filters: hearing loss, pitch shift, balance / vertigo, etc.
//!
//! Phase 4 (Issue #7/#8/#9) で実装。
//!
//! # 音声バッファ
//!
//! - [`AudioBuffer`] — `Vec<f32>` の PCM サンプル列（normalized, -1.0..=1.0）
//! - サンプルはインターリーブ: `ch0[0], ch1[0], ch0[1], ch1[1], ...`
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
    // sample_rate=0 の場合はフォールバックとして 44100 Hz を使用する。
    let fs = if sample_rate == 0 {
        44100.0
    } else {
        sample_rate as f32
    };
    // バイリニア変換による Butterworth 2 次 LP
    let f0 = freq_hz.clamp(1.0, fs * 0.4999);
    let w0 = 2.0 * PI * f0 / fs;
    let q = std::f32::consts::FRAC_1_SQRT_2; // Butterworth Q = 1/sqrt(2)
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
    let fs = if sample_rate == 0 {
        44100.0
    } else {
        sample_rate as f32
    };
    let f0 = freq_hz.clamp(1.0, fs * 0.4999);
    let w0 = 2.0 * PI * f0 / fs;
    let q = std::f32::consts::FRAC_1_SQRT_2;
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
    let fs = if sample_rate == 0 {
        44100.0
    } else {
        sample_rate as f32
    };
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
fn apply_biquad_all_channels(
    buf: &AudioBuffer,
    make_filter: impl Fn() -> BiquadFilter,
) -> Vec<f32> {
    let ch = buf.channels as usize;
    if ch == 0 {
        return buf.samples.clone();
    }
    // チャンネルごとにフィルタインスタンスを生成（ステートを独立させる）
    let mut filters: Vec<BiquadFilter> = (0..ch).map(|_| make_filter()).collect();
    let mut out = buf.samples.clone();
    let frames = buf.frames();
    for frame in 0..frames {
        #[allow(clippy::needless_range_loop)] // idx = frame * ch + c のインターリーブ計算が必要
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
    // 中心 4 kHz。帯域幅は strength 比例で、sudden_hearing_loss と同じ `50 + s*950` Hz
    // （最大 1000 Hz ≒ ±500 Hz）。
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
    let samples = buf
        .samples
        .iter()
        .map(|&x| (x * gain).clamp(-1.0, 1.0))
        .collect();
    AudioBuffer {
        samples,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// 聴覚過敏・ミソフォニア（misophonia）シミュレーション。
///
/// ハイパーアクーシス（[`hyperacusis`]）が音量全体への耐性低下なのに対し、
/// ミソフォニアは**特定のトリガー音への強い不快感**であり、全体ではなく
/// 特定周波数帯だけが過剰に大きく・耳障りに知覚される。
///
/// そこで `freq_hz` を中心とするトリガー帯域だけを抜き出して
/// 強調（最大 6 倍ブースト）＋ tanh 倍音歪みで耳障り化し、帯域外はそのまま残す。
/// この帯域選択性が hyperacusis（全帯域一様増幅）との違い。
///
/// `freq_hz`: 不快に感じるトリガー音の中心周波数（咀嚼音・打鍵音などは概ね 1–4 kHz）。
/// `strength = 0.0` は元音声と同一。
pub fn misophonia(buf: AudioBuffer, strength: f32, freq_hz: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    let sr = buf.sample_rate;
    // トリガー帯域幅。中心周波数の半分（最低 500 Hz）を採り、低中音の咀嚼音も拾えるようにする。
    let bandwidth = (freq_hz.abs() * 0.5).clamp(500.0, 2000.0);
    // band-reject でトリガー帯域を除いた成分（帯域外）を得る。
    let rejected = apply_biquad_all_channels(&buf, || band_reject_biquad(freq_hz, bandwidth, sr));

    // トリガー帯域成分 = 元信号 − 帯域外成分。これをブーストし tanh で耳障りに歪ませる。
    let boost = 1.0 + s * 5.0; // 1.0 → 6.0
    let drive = 1.0 + s * 3.0;
    let reconstructed: Vec<f32> = buf
        .samples
        .iter()
        .zip(rejected.iter())
        .map(|(&orig, &rest)| {
            let band = orig - rest; // トリガー帯域成分
            let harsh = (band * drive).tanh(); // 倍音歪みで耳障り化
            let aversive = harsh * boost; // 過剰増幅
            (rest + aversive).clamp(-1.0, 1.0)
        })
        .collect();

    AudioBuffer {
        samples: blend(&buf.samples, &reconstructed, s),
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
    if semitones == 0.0 || !semitones.is_finite() {
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

/// APD（聴覚情報処理障害）シミュレーション。
///
/// 時間分解能の低下 + 雑音付加を近似する:
/// 1. LCG（seed=42 固定）によるホワイトノイズ混入
/// 2. 隣接 3 サンプルの加重平均（FIR スミア）
/// 3. 無音区間の gap 埋め（< 5 ms の無音を隣接値で補間）
///
/// `strength = 0.0` は元音声と同一。
pub fn auditory_processing_disorder(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }

    let ch = buf.channels as usize;
    if ch == 0 || buf.samples.is_empty() {
        return buf;
    }

    let n = buf.samples.len();

    // Step 1: ホワイトノイズ混入（LCG, seed=42）
    const LCG_A: u64 = 1664525;
    const LCG_C: u64 = 1013904223;
    let mut state: u64 = 42;
    let noise_amp = s * 0.05; // 最大 5% のノイズ
    let mut noisy: Vec<f32> = buf
        .samples
        .iter()
        .map(|&x| {
            state = state.wrapping_mul(LCG_A).wrapping_add(LCG_C);
            // -1.0..=1.0 の符号付きノイズ
            let noise = (state >> 32) as f32 / (u32::MAX as f32 / 2.0) - 1.0;
            (x + noise * noise_amp).clamp(-1.0, 1.0)
        })
        .collect();

    // Step 2: FIR スミア（隣接 3 サンプルの加重平均: 0.25, 0.5, 0.25）
    // strength に応じて元とブレンド
    let w_center = 1.0 - s * 0.5; // strength=1.0 で center=0.5
    let w_side = s * 0.25; // strength=1.0 で w_center=0.5, w_side=0.25 → 合計 0.5 + 0.25×2 = 1.0
    let mut smeared = noisy.clone();
    for i in 0..n {
        let prev = if i >= ch { noisy[i - ch] } else { noisy[i] };
        let next = if i + ch < n { noisy[i + ch] } else { noisy[i] };
        smeared[i] = (prev * w_side + noisy[i] * w_center + next * w_side).clamp(-1.0, 1.0);
    }
    noisy = smeared;

    // Step 3: gap 埋め（< 5 ms の無音区間を前後の値で補間）
    // sample_rate=0 は 44100 Hz として扱う
    let sr = if buf.sample_rate == 0 {
        44100
    } else {
        buf.sample_rate
    };
    let gap_frames = ((sr as f32 * 0.005) as usize).max(1); // 5 ms
    let silence_threshold = 0.01_f32;

    let frames = n / ch;
    let mut result = noisy.clone();
    let mut gap_start: Option<usize> = None;

    for f in 0..frames {
        // フレームが無音かどうか（全チャンネル）
        let is_silent = (0..ch).all(|c| noisy[f * ch + c].abs() < silence_threshold);
        if is_silent {
            if gap_start.is_none() {
                gap_start = Some(f);
            }
        } else if let Some(gs) = gap_start {
            let gap_len = f - gs;
            if gap_len < gap_frames {
                // gap を前後の値で線形補間して埋める
                let before_f = if gs > 0 { gs - 1 } else { gs };
                for gf in gs..f {
                    let t = (gf - gs + 1) as f32 / (gap_len + 1) as f32;
                    for c in 0..ch {
                        let before_val = result[before_f * ch + c];
                        let after_val = noisy[f * ch + c];
                        result[gf * ch + c] =
                            (before_val + (after_val - before_val) * t).clamp(-1.0, 1.0);
                    }
                }
            }
            gap_start = None;
        }
    }

    AudioBuffer {
        samples: result,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    }
}

/// メニエール病（Ménière's disease）の聴覚側シミュレーション。
///
/// メニエール病の三徴候は「変動性の低音域感音難聴 + 低い唸るような耳鳴り + 回転性めまい」。
/// めまい（視覚側）は [`crate::Filter::Vertigo`] が担当し、[`crate::Experience::MENIERE`]
/// で視覚と組として正準化される。本関数は聴覚側の二要素を合成する:
///
/// 1. 低音域感音難聴: ハイパスで低域を部分的に減衰させる。メニエールは高音ではなく
///    **低音**が落ちるのが特徴で、加齢性難聴（[`hearing_loss`] = 高音カット）とは逆。
/// 2. 低い唸る耳鳴り: ~200 Hz の正弦波をミックス（高音の `tinnitus` とは音色が異なる）。
///
/// `strength = 0.0` は元音声と同一。
pub fn meniere(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    let sr = buf.sample_rate;
    // 1) 低音域感音難聴: 100→800 Hz のハイパスを部分ブレンド（完全除去はしない）。
    let hp_cut = 100.0 + s * 700.0;
    let hp = apply_biquad_all_channels(&buf, || high_pass_biquad(hp_cut, sr));
    let low_loss = blend(&buf.samples, &hp, s * 0.7);
    let stage1 = AudioBuffer {
        samples: low_loss,
        sample_rate: buf.sample_rate,
        channels: buf.channels,
    };
    // 2) 低音の唸る耳鳴り（~200 Hz）。
    tinnitus(stage1, s, 200.0)
}

/// 迷路炎（labyrinthitis）の聴覚側シミュレーション。
///
/// 前庭神経炎（[`crate::Experience::VESTIBULAR_NEURITIS`]）が前庭神経のみの炎症で
/// **聴力は保たれる**のに対し、迷路炎は内耳（蝸牛を含む）の炎症で、回転性めまいに
/// **感音難聴と耳鳴りを伴う**。この聴覚症状の有無が両者の臨床的鑑別点であり、
/// 「めまいの聴覚側複合」を医学的に正しく表せるのは迷路炎（やメニエール病）の側。
///
/// 1. 感音難聴: 高音域カット（[`hearing_loss`] と同系の広めの感音難聴を近似）
/// 2. 高音の耳鳴り: ~4 kHz（メニエールの低い唸りとは音色が異なる）
///
/// `strength = 0.0` は元音声と同一。
pub fn labyrinthitis(buf: AudioBuffer, strength: f32) -> AudioBuffer {
    let s = strength.clamp(0.0, 1.0);
    if s == 0.0 {
        return buf;
    }
    let stage1 = hearing_loss(buf, s);
    tinnitus(stage1, s, 4000.0)
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
        let rms: f32 =
            (out.samples.iter().map(|&x| x * x).sum::<f32>() / out.samples.len() as f32).sqrt();
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

    // ---------------------------------------------------------------
    // TC-H-03: sample_rate=0 は panic しない
    // ---------------------------------------------------------------

    #[test]
    fn hearing_loss_sample_rate_zero_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.2, 0.3],
            sample_rate: 0,
            channels: 1,
        };
        let _ = hearing_loss(buf, 1.0);
    }

    // ---------------------------------------------------------------
    // TC-H-04〜07: strength=0.0 は identity
    // ---------------------------------------------------------------

    #[test]
    fn amusia_strength_zero_is_identity() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.2, 0.3],
            sample_rate: 44100,
            channels: 1,
        };
        let orig = buf.samples.clone();
        let out = amusia(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn noise_induced_hearing_loss_strength_zero_is_identity() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.2, 0.3],
            sample_rate: 44100,
            channels: 1,
        };
        let orig = buf.samples.clone();
        let out = noise_induced_hearing_loss(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn paracusis_strength_zero_is_identity() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.2, 0.3],
            sample_rate: 44100,
            channels: 1,
        };
        let orig = buf.samples.clone();
        let out = paracusis(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    #[test]
    fn dysmelodia_strength_zero_is_identity() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.2, 0.3],
            sample_rate: 44100,
            channels: 1,
        };
        let orig = buf.samples.clone();
        let out = dysmelodia(buf, 0.0);
        assert_eq!(out.samples, orig);
    }

    // ---------------------------------------------------------------
    // TC-H-11〜13: 空バッファは panic しない
    // ---------------------------------------------------------------

    #[test]
    fn hearing_loss_empty_buffer_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = hearing_loss(buf, 1.0);
        assert!(out.samples.is_empty());
    }

    #[test]
    fn tinnitus_empty_buffer_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = tinnitus(buf, 1.0, 4000.0);
        assert!(out.samples.is_empty());
    }

    #[test]
    fn pitch_shift_empty_buffer_returns_empty() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = pitch_shift_semitones(buf, 2.0);
        assert!(out.samples.is_empty());
    }

    // ---------------------------------------------------------------
    // TC-H-16: ステレオ入力でチャンネル数保持
    // ---------------------------------------------------------------

    #[test]
    fn hearing_loss_stereo_preserves_channel_count() {
        let buf = AudioBuffer {
            samples: vec![0.1, 0.2, -0.1, -0.2, 0.3, 0.4],
            sample_rate: 44100,
            channels: 2,
        };
        let out = hearing_loss(buf, 0.5);
        assert_eq!(out.channels, 2);
    }

    // ---------------------------------------------------------------
    // TC-H-24: NaN semitones は panic しない
    // ---------------------------------------------------------------

    #[test]
    fn pitch_shift_nan_semitones_does_not_panic() {
        let buf = sine_wave(440.0, 100, 44100);
        let orig = buf.samples.clone();
        let out = pitch_shift_semitones(buf, f32::NAN);
        // NaN は identity として扱う契約
        assert_eq!(out.samples, orig);
    }

    // ---------------------------------------------------------------
    // TC-H-25〜26: diplacusis
    // ---------------------------------------------------------------

    #[test]
    fn diplacusis_mono_input_produces_stereo_output() {
        let buf = sine_wave(440.0, 100, 44100);
        assert_eq!(buf.channels, 1);
        let out = diplacusis(buf, 1.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 200);
    }

    #[test]
    fn diplacusis_stereo_input_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![0.1, 0.2, -0.1, -0.2, 0.3, 0.4],
            sample_rate: 44100,
            channels: 2,
        };
        let out = diplacusis(buf, 1.0);
        assert_eq!(out.channels, 2);
    }

    #[test]
    fn diplacusis_stereo_output() {
        let buf = sine_wave(440.0, 1000, 44100);
        let out = diplacusis(buf, 1.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 2000);
    }

    #[test]
    fn apd_strength_zero_is_identity() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = auditory_processing_disorder(buf, 0.0);
        assert_eq!(
            out.samples, orig,
            "APD strength=0 should be byte-exact identity"
        );
    }

    #[test]
    fn apd_adds_noise() {
        // 無音バッファに strength=1 で APD を適用すると RMS > 0 になる
        let buf = silence(44100, 44100, 1);
        let out = auditory_processing_disorder(buf, 1.0);
        let rms: f32 =
            (out.samples.iter().map(|&x| x * x).sum::<f32>() / out.samples.len() as f32).sqrt();
        assert!(rms > 0.0, "APD should add noise to silence, rms={rms}");
    }

    // ---------------------------------------------------------------
    // Issue #61: paracusis / dysmelodia / amusia の strength=1 効果確認
    // ---------------------------------------------------------------

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|&x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn paracusis_strength_one_differs_from_input() {
        // 440 Hz サイン波に paracusis strength=1 を適用すると歪みが加わる
        let buf = sine_wave(440.0, 44100, 44100);
        let orig_rms = rms(&buf.samples);
        let out = paracusis(buf, 1.0);
        let out_rms = rms(&out.samples);
        // tanh クリッピングで RMS が変化することを確認（完全一致しないこと）
        let rel_diff = (out_rms - orig_rms).abs() / orig_rms.max(1e-6);
        assert!(
            rel_diff > 0.001,
            "paracusis strength=1 must alter signal RMS (rel_diff={rel_diff})"
        );
    }

    #[test]
    fn dysmelodia_strength_one_differs_from_input() {
        let buf = sine_wave(440.0, 44100, 44100);
        let orig_rms = rms(&buf.samples);
        let out = dysmelodia(buf, 1.0);
        let out_rms = rms(&out.samples);
        let rel_diff = (out_rms - orig_rms).abs() / orig_rms.max(1e-6);
        assert!(
            rel_diff > 0.001,
            "dysmelodia strength=1 must alter signal RMS (rel_diff={rel_diff})"
        );
    }

    #[test]
    fn amusia_strength_one_differs_from_input() {
        // 4000 Hz サイン波は 200 Hz カットオフで大きく減衰する
        let buf = sine_wave(4000.0, 44100, 44100);
        let orig_rms = rms(&buf.samples);
        let out = amusia(buf, 1.0);
        let out_rms = rms(&out.samples);
        let rel_diff = (out_rms - orig_rms).abs() / orig_rms.max(1e-6);
        assert!(
            rel_diff > 0.1,
            "amusia strength=1 must significantly attenuate 4 kHz signal (rel_diff={rel_diff})"
        );
    }

    // ---------------------------------------------------------------
    // Issue #102: misophonia
    // ---------------------------------------------------------------

    #[test]
    fn misophonia_strength_zero_is_identity() {
        let buf = sine_wave(2000.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = misophonia(buf, 0.0, 2000.0);
        assert_eq!(
            out.samples, orig,
            "misophonia strength=0 should be byte-exact identity"
        );
    }

    #[test]
    fn misophonia_empty_buffer_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = misophonia(buf, 1.0, 2000.0);
        assert!(out.samples.is_empty());
    }

    #[test]
    fn misophonia_stereo_preserves_channel_count() {
        let buf = AudioBuffer {
            samples: vec![0.1; 2000],
            sample_rate: 44100,
            channels: 2,
        };
        let out = misophonia(buf, 1.0, 2000.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 2000);
    }

    #[test]
    fn misophonia_strength_one_differs_from_input() {
        // トリガー周波数のサイン波は強調・歪みで RMS が変化する
        let buf = sine_wave(2000.0, 44100, 44100);
        let orig_rms = rms(&buf.samples);
        let out = misophonia(buf, 1.0, 2000.0);
        let out_rms = rms(&out.samples);
        let rel_diff = (out_rms - orig_rms).abs() / orig_rms.max(1e-6);
        assert!(
            rel_diff > 0.05,
            "misophonia strength=1 must alter the trigger-band signal (rel_diff={rel_diff})"
        );
    }

    // ---------------------------------------------------------------
    // Issue #104: labyrinthitis（前庭性めまいの聴覚側複合）
    // ---------------------------------------------------------------

    #[test]
    fn labyrinthitis_strength_zero_is_identity() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = labyrinthitis(buf, 0.0);
        assert_eq!(
            out.samples, orig,
            "labyrinthitis strength=0 should be byte-exact identity"
        );
    }

    #[test]
    fn labyrinthitis_empty_buffer_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = labyrinthitis(buf, 1.0);
        assert!(out.samples.is_empty());
    }

    #[test]
    fn labyrinthitis_stereo_preserves_channel_count() {
        let buf = AudioBuffer {
            samples: vec![0.1; 2000],
            sample_rate: 44100,
            channels: 2,
        };
        let out = labyrinthitis(buf, 1.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 2000);
    }

    #[test]
    fn labyrinthitis_adds_tinnitus_to_silence() {
        let buf = silence(44100, 44100, 1);
        let out = labyrinthitis(buf, 1.0);
        assert!(
            rms(&out.samples) > 0.0,
            "labyrinthitis should add tinnitus to silence"
        );
    }

    #[test]
    fn labyrinthitis_attenuates_high_more_than_low() {
        // 感音難聴は高音域カット: 高音(8 kHz)は低音(200 Hz)より強く減衰する。
        // メニエール（低音側が落ちる）とは逆向きであることを検証する。
        let high = sine_wave(8000.0, 44100, 44100);
        let high_ratio =
            rms(&labyrinthitis(high.clone(), 1.0).samples) / rms(&high.samples).max(1e-6);

        let low = sine_wave(200.0, 44100, 44100);
        let low_ratio = rms(&labyrinthitis(low.clone(), 1.0).samples) / rms(&low.samples).max(1e-6);

        assert!(
            high_ratio < low_ratio,
            "labyrinthitis must attenuate high freq more than low: high_ratio={high_ratio}, low_ratio={low_ratio}"
        );
    }

    // ---------------------------------------------------------------
    // Issue #103: meniere
    // ---------------------------------------------------------------

    #[test]
    fn meniere_strength_zero_is_identity() {
        let buf = sine_wave(440.0, 1000, 44100);
        let orig = buf.samples.clone();
        let out = meniere(buf, 0.0);
        assert_eq!(
            out.samples, orig,
            "meniere strength=0 should be byte-exact identity"
        );
    }

    #[test]
    fn meniere_empty_buffer_does_not_panic() {
        let buf = AudioBuffer {
            samples: vec![],
            sample_rate: 44100,
            channels: 1,
        };
        let out = meniere(buf, 1.0);
        assert!(out.samples.is_empty());
    }

    #[test]
    fn meniere_stereo_preserves_channel_count() {
        let buf = AudioBuffer {
            samples: vec![0.1; 2000],
            sample_rate: 44100,
            channels: 2,
        };
        let out = meniere(buf, 1.0);
        assert_eq!(out.channels, 2);
        assert_eq!(out.samples.len(), 2000);
    }

    #[test]
    fn meniere_adds_low_tinnitus_to_silence() {
        // 無音に低音の唸る耳鳴りが加わるので RMS > 0
        let buf = silence(44100, 44100, 1);
        let out = meniere(buf, 1.0);
        assert!(
            rms(&out.samples) > 0.0,
            "meniere should add roaring tinnitus to silence"
        );
    }

    #[test]
    fn meniere_attenuates_low_more_than_high() {
        // 低音域感音難聴: 低音(60 Hz)は減衰するが高音(2 kHz)はハイパスを通過する。
        // tinnitus が一律に低音を加えても、低音入力の方が RMS 保持率が低くなることで検証する。
        let low = sine_wave(60.0, 44100, 44100);
        let low_ratio = rms(&meniere(low.clone(), 1.0).samples) / rms(&low.samples).max(1e-6);

        let high = sine_wave(2000.0, 44100, 44100);
        let high_ratio = rms(&meniere(high.clone(), 1.0).samples) / rms(&high.samples).max(1e-6);

        assert!(
            low_ratio < high_ratio,
            "meniere must attenuate low freq more than high: low_ratio={low_ratio}, high_ratio={high_ratio}"
        );
    }

    #[test]
    fn misophonia_is_band_selective() {
        // トリガー帯域内（2000 Hz）の方が帯域外（200 Hz）より強く影響を受ける
        // ＝ hyperacusis（全帯域一様）との違いを検証する
        let in_band = sine_wave(2000.0, 44100, 44100);
        let in_orig = rms(&in_band.samples);
        let in_out = rms(&misophonia(in_band, 1.0, 2000.0).samples);
        let delta_in = (in_out - in_orig).abs() / in_orig.max(1e-6);

        let out_band = sine_wave(200.0, 44100, 44100);
        let out_orig = rms(&out_band.samples);
        let out_out = rms(&misophonia(out_band, 1.0, 2000.0).samples);
        let delta_out = (out_out - out_orig).abs() / out_orig.max(1e-6);

        assert!(
            delta_in > delta_out,
            "trigger-band (2 kHz) must be affected more than out-of-band (200 Hz): delta_in={delta_in}, delta_out={delta_out}"
        );
    }

    // ---------------------------------------------------------------
    // Issue #114: hearing 効果アサート網羅
    // ---------------------------------------------------------------

    /// インターリーブ stereo を L / R チャンネルに分離する。
    fn deinterleave_lr(buf: &AudioBuffer) -> (Vec<f32>, Vec<f32>) {
        assert_eq!(buf.channels, 2);
        let l = buf.samples.iter().step_by(2).copied().collect();
        let r = buf.samples.iter().skip(1).step_by(2).copied().collect();
        (l, r)
    }

    #[test]
    fn diplacusis_left_and_right_differ() {
        // ダイプラクシスは左右耳で別音程に知覚させる → L と R が一致してはならない。
        let buf = sine_wave(440.0, 4000, 44100);
        let out = diplacusis(buf, 1.0);
        let (l, r) = deinterleave_lr(&out);
        assert_ne!(
            l, r,
            "diplacusis must produce different left/right channels"
        );
        // 単なる定数オフセットではなく、リサンプリングによる音程差であることを確認
        let diff_rms = rms(&l
            .iter()
            .zip(r.iter())
            .map(|(&a, &b)| a - b)
            .collect::<Vec<_>>());
        assert!(diff_rms > 0.0, "L/R difference must be non-trivial");
    }

    #[test]
    fn sudden_hearing_loss_attenuates_notch_band() {
        // 4 kHz を中心にノッチを掛けると 4 kHz サイン波の RMS が顕著に下がる。
        let buf = sine_wave(4000.0, 44100, 44100);
        let orig = rms(&buf.samples);
        let out = rms(&sudden_hearing_loss(buf, 1.0, 4000.0).samples);
        assert!(
            out < orig * 0.8,
            "sudden_hearing_loss must attenuate the notch band (orig={orig}, out={out})"
        );
    }

    #[test]
    fn noise_induced_hearing_loss_attenuates_4khz_more_than_low() {
        // 騒音性難聴は 4 kHz 付近を削る: 4 kHz は低音(500 Hz)より強く減衰する。
        let high = sine_wave(4000.0, 44100, 44100);
        let high_ratio = rms(&noise_induced_hearing_loss(high.clone(), 1.0).samples)
            / rms(&high.samples).max(1e-6);
        let low = sine_wave(500.0, 44100, 44100);
        let low_ratio = rms(&noise_induced_hearing_loss(low.clone(), 1.0).samples)
            / rms(&low.samples).max(1e-6);
        assert!(
            high_ratio < low_ratio,
            "noise-induced must attenuate 4 kHz more than 500 Hz: high_ratio={high_ratio}, low_ratio={low_ratio}"
        );
    }

    #[test]
    fn pitch_shift_changes_signal_but_keeps_length() {
        // 1 オクターブ上げると波形が変わる（恒等でない）が、フレーム長は保たれる。
        let buf = sine_wave(440.0, 8000, 44100);
        let orig = buf.samples.clone();
        let out = pitch_shift_semitones(buf, 12.0);
        assert_eq!(
            out.samples.len(),
            orig.len(),
            "pitch shift must preserve length"
        );
        assert_ne!(out.samples, orig, "pitch shift must alter the waveform");
    }

    #[test]
    fn tinnitus_adds_tone_on_nonsilent_signal() {
        // 無音だけでなく、信号入り音声にも耳鳴りトーンが重畳される（差分に energy がある）。
        let buf = sine_wave(440.0, 44100, 44100);
        let orig = buf.samples.clone();
        let out = tinnitus(buf, 1.0, 4000.0);
        let diff: Vec<f32> = out
            .samples
            .iter()
            .zip(orig.iter())
            .map(|(&a, &b)| a - b)
            .collect();
        assert!(
            rms(&diff) > 0.01,
            "tinnitus must add a tone even on a non-silent signal (diff_rms={})",
            rms(&diff)
        );
    }
}
