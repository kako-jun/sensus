//! sensus-core — sensory perception simulation core.
//!
//! Pure logic library that applies sensory filters (color blindness, blur,
//! visual field defects, hearing loss, etc.) to media buffers. All public
//! entry points take and return [`image::DynamicImage`] so callers can chain
//! filters without committing to a specific pixel format.
//!
//! This crate intentionally has **no I/O** — file reads, file writes,
//! decoding from arbitrary formats, and any subprocess work belongs in the
//! `sensus` CLI crate or in downstream applications (e.g. universal-experience).

pub mod error;
pub mod hearing;
pub mod pipeline;
pub mod shaders;
pub mod stereo;
pub mod vision;

pub use error::Error;
pub use pipeline::{AudioFilterStep, AudioPipeline};

/// Convenience alias for `Result<T, sensus_core::Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// All sensory filters planned for sensus.
///
/// v0.5.0 以降、フィルタ固有のパラメータはすべて enum payload に持たせる
/// （`FilterStep` 等にスカラを散らさない）。パラメータを持たないバリアントは
/// その効果が strength だけで決まるもの。
///
/// すべてのバリアントは [`apply`] で実装済み（match は網羅的なので、新バリアントを
/// 追加すると未実装はコンパイルエラーになる）。`apply()` と [`pipeline`] は同じ
/// payload を読むため、単体適用と pipeline 適用の挙動は常に一致する。enum は
/// `sensus-core` にあるため、非 CLI consumer（GUI / ライブラリ利用者）は clap を
/// 持ち込まずにフィルタを参照できる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    // vision (Phase 1: color vision deficiency)
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Achromatopsia,
    // vision (Phase 1+: tetrachromacy)
    Tetrachromacy,
    // vision (Phase 2: focus / refraction)
    Myopia,
    Hyperopia,
    /// 乱視。`axis_deg` はシャープ方向の経線角（医学的慣習）。デフォルト: 90°
    Astigmatism {
        axis_deg: f32,
    },
    Presbyopia,
    // vision (Phase 3: visual field)
    /// 緑内障。`mode` は [`vision::GlaucomaMode`] を参照。デフォルト: Vignette
    Glaucoma {
        mode: vision::GlaucomaMode,
    },
    MacularDegeneration,
    /// 半盲。`side`: 0.0 = 左視野消失, 1.0 = 右視野消失
    Hemianopia {
        side: f32,
    },
    TunnelVision,
    // vision (Phase 3: light / transparency)
    /// 白内障。`seed`: 散乱グレア生成用ランダムシード。デフォルト: 0
    Cataract {
        seed: u64,
    },
    /// 飛蚊症。`seed`: ランダムシード, `density`: blob 密度, `size`: blob 半径・糸くず幅の相対倍率,
    /// `gaze_x`/`gaze_y`: 視線位置（飛蚊が追従する中心、0.0..=1.0）
    ///
    /// `size`: 1.0 = 既定。`vision::floaters` の blob 半径・糸くず幅に乗じる（0.1..=5.0 に clamp、#110）。
    /// `gaze_x`/`gaze_y`: 0.5 = 画面中央。
    Floaters {
        seed: u64,
        density: f32,
        size: f32,
        gaze_x: f32,
        gaze_y: f32,
    },
    Photophobia,
    NightBlindness,
    // vision (Phase 4 / #9: balance / vertigo)
    Vertigo,
    BppvRotation,
    VestibularNeuritis,
    // vision (Phase 4 / #29: diplopia / nystagmus / starbursts)
    /// 複視。`offset_x`/`offset_y`: 幽霊像のずれ（min(W,H) 比）, `ghost_strength`: 幽霊像強度
    Diplopia {
        offset_x: f32,
        offset_y: f32,
        ghost_strength: f32,
    },
    /// 眼振。`amplitude`: 振幅（min(W,H) 比）, `direction_deg`: 揺れ方向（0°=水平, 90°=垂直）
    Nystagmus {
        amplitude: f32,
        direction_deg: f32,
    },
    /// 光芒。`num_rays`: 本数, `ray_length_ratio`: 長さ比, `threshold`: 輝度閾値, `dispersion`: 虹色度
    Starbursts {
        num_rays: u32,
        ray_length_ratio: f32,
        threshold: f32,
        dispersion: f32,
    },
    // vision (Phase 4: eye fatigue / #36)
    EyeStrain,
    DryEye,
    // vision (Phase N / #55: metamorphopsia)
    /// 変視症（歪んで見える）。`freq`: 歪みの空間周波数, `seed`: 歪み場生成シード
    Metamorphopsia {
        freq: f32,
        seed: u64,
    },
    // vision (Phase N / #56: contrast sensitivity)
    ContrastSensitivity,
    /// ディテールロス（ピクセル化）。`cell_size`: タイルサイズ (px)
    DetailLoss {
        cell_size: u32,
    },
    // vision (Phase N / #58: teichopsia)
    Teichopsia,
    /// 閃輝暗点・光の星。`seed`: ランダムシード
    FlickeringStars {
        seed: u64,
    },
}

/// Apply a [`Filter`] to an image at a given strength (`0.0..=1.0`).
///
/// Phase 1 (#2) で色覚特性 4 種、Phase 1+ (#3) で四色型色覚、
/// Phase 2 (#4) で焦点・屈折 4 種、Phase 3 (#5/#6) で視野異常・光透明度を実装済み。
///
/// パラメータ付きバリアントはそのパラメータを直接使用する。
/// パラメータなしバリアントはデフォルト値を使用する。
pub fn apply(
    filter: Filter,
    img: image::DynamicImage,
    strength: f32,
) -> Result<image::DynamicImage> {
    match filter {
        Filter::Protanopia => vision::protanopia(img, strength),
        Filter::Deuteranopia => vision::deuteranopia(img, strength),
        Filter::Tritanopia => vision::tritanopia(img, strength),
        Filter::Achromatopsia => vision::achromatopsia(img, strength),
        Filter::Myopia => vision::myopia(img, strength),
        Filter::Hyperopia => vision::hyperopia(img, strength),
        Filter::Presbyopia => vision::presbyopia(img, strength),
        Filter::Astigmatism { axis_deg } => vision::astigmatism(img, strength, axis_deg),
        Filter::Cataract { seed } => vision::cataract(img, strength, seed),
        Filter::Photophobia => vision::photophobia(img, strength),
        Filter::NightBlindness => vision::nyctalopia(img, strength),
        Filter::Floaters {
            seed,
            density,
            size,
            gaze_x,
            gaze_y,
        } => vision::floaters(img, strength, density, seed, gaze_x, gaze_y, size),
        Filter::Glaucoma { mode } => vision::glaucoma(img, strength, mode),
        Filter::MacularDegeneration => vision::macular_degeneration(img, strength),
        Filter::Hemianopia { side } => vision::hemianopia(img, strength, side),
        Filter::TunnelVision => vision::tunnel_vision(img, strength),
        Filter::Tetrachromacy => vision::tetrachromacy(img, strength),
        // 静止画では時間を持てないため、効果がピークになる代表位相で 1 フレームを描く
        // （アニメーションは GLSL シェーダ側の time uniform が担当する）。
        Filter::Vertigo => vision::vertigo(img, strength, vision::VERTIGO_STILL_TIME_S),
        Filter::BppvRotation => vision::bppv_rotation(img, strength, vision::BPPV_STILL_TIME_S),
        Filter::VestibularNeuritis => vision::vestibular_neuritis(img, strength),
        Filter::Diplopia {
            offset_x,
            offset_y,
            ghost_strength,
        } => vision::diplopia(img, strength, offset_x, offset_y, ghost_strength),
        Filter::Nystagmus {
            amplitude,
            direction_deg,
        } => vision::nystagmus(img, strength, amplitude, direction_deg),
        Filter::Starbursts {
            num_rays,
            ray_length_ratio,
            threshold,
            dispersion,
        } => vision::starbursts(
            img,
            strength,
            num_rays,
            ray_length_ratio,
            threshold,
            dispersion,
        ),
        Filter::EyeStrain => vision::eye_strain(img, strength),
        Filter::DryEye => vision::dry_eye(img, strength),
        Filter::Metamorphopsia { freq, seed } => vision::metamorphopsia(img, strength, freq, seed),
        Filter::ContrastSensitivity => vision::contrast_sensitivity(img, strength),
        Filter::DetailLoss { cell_size } => {
            vision::detail_loss_with_cell_size(img, strength, cell_size)
        }
        Filter::Teichopsia => vision::teichopsia(img, strength),
        Filter::FlickeringStars { seed } => vision::flickering_stars(img, strength, seed),
    }
}

/// 聴覚フィルタの種類。
///
/// `apply_hearing()` 経由で使用する。音声バッファに対して純粋関数として適用する。
#[derive(Debug, Clone, PartialEq)]
pub enum HearingFilter {
    /// 難聴: 高音域カット
    HearingLoss,
    /// 突発性難聴: 特定周波数帯の急激な損失
    SuddenHearingLoss { freq_hz: f32 },
    /// 騒音性難聴: 4 kHz 付近の損失
    NoiseInducedHearingLoss,
    /// 耳鳴り: 指定周波数の正弦波を常時ミックス
    Tinnitus { freq_hz: f32 },
    /// 音響過敏: 音量を異常に増幅
    Hyperacusis,
    /// ミソフォニア（聴覚過敏 / 特定音への強い不快）: `freq_hz` 中心のトリガー帯域だけを過剰増幅 + 歪み
    Misophonia { freq_hz: f32 },
    /// 変音: 音を歪んだ・金属的な質感に加工
    Paracusis,
    /// 音楽音痴: 音程の違いを識別しにくくする
    Amusia,
    /// ジスメロディア: 音楽を不快・歪んだ音に変換
    Dysmelodia,
    /// 音程シフト: 半音単位で全体音程をシフト
    PitchShift { semitones: f32 },
    /// ダイプラクシス: 左右耳で異なる音程を知覚
    Diplacusis,
    /// APD（聴覚情報処理障害）: 時間分解能低下 + 雑音付加
    AuditoryProcessingDisorder,
    /// メニエール病の聴覚側: 低音域難聴 + 低い唸る耳鳴り（複合）。
    /// 回転性めまい（視覚）と組で [`Experience::MENIERE`] として正準化される。
    Meniere,
    /// 迷路炎の聴覚側: 高音域感音難聴 + 高音の耳鳴り（複合）。
    /// 回転性めまい（視覚）と組で [`Experience::LABYRINTHITIS`] として正準化される。
    /// 前庭神経炎（聴力温存）との鑑別点となる聴覚症状を表す。
    Labyrinthitis,
}

/// 聴覚フィルタを音声バッファに適用する。
///
/// `strength` は 0.0..=1.0（0.0 = 元音声、1.0 = 最大効果）。
/// `PitchShift` と `Diplacusis` では `strength` の意味が変わる場合があるため、
/// 各フィルタのドキュメントを参照のこと。
pub fn apply_hearing(
    filter: HearingFilter,
    buf: hearing::AudioBuffer,
    strength: f32,
) -> Result<hearing::AudioBuffer> {
    let out = match filter {
        HearingFilter::HearingLoss => hearing::hearing_loss(buf, strength),
        HearingFilter::SuddenHearingLoss { freq_hz } => {
            hearing::sudden_hearing_loss(buf, strength, freq_hz)
        }
        HearingFilter::NoiseInducedHearingLoss => {
            hearing::noise_induced_hearing_loss(buf, strength)
        }
        HearingFilter::Tinnitus { freq_hz } => hearing::tinnitus(buf, strength, freq_hz),
        HearingFilter::Hyperacusis => hearing::hyperacusis(buf, strength),
        HearingFilter::Misophonia { freq_hz } => hearing::misophonia(buf, strength, freq_hz),
        HearingFilter::Paracusis => hearing::paracusis(buf, strength),
        HearingFilter::Amusia => hearing::amusia(buf, strength),
        HearingFilter::Dysmelodia => hearing::dysmelodia(buf, strength),
        HearingFilter::PitchShift { semitones } => hearing::pitch_shift_semitones(buf, semitones),
        HearingFilter::Diplacusis => hearing::diplacusis(buf, strength),
        HearingFilter::AuditoryProcessingDisorder => {
            hearing::auditory_processing_disorder(buf, strength)
        }
        HearingFilter::Meniere => hearing::meniere(buf, strength),
        HearingFilter::Labyrinthitis => hearing::labyrinthitis(buf, strength),
    };
    Ok(out)
}

/// 受診喚起の緊急度分類（仕様リーフの「緊急度分類」に対応）。
///
/// 局所的な i18n 文字列を core に埋め込まず、consumer 側で適切な言語のメッセージを
/// 出し分けられるよう、分類だけを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// 緊急性の注記なし
    None,
    /// ⚠️ 早期受診が望ましい
    EarlyConsultation,
    /// 🚨 即救急（脳卒中等のサインの可能性）
    Emergency,
}

/// 視覚と聴覚にまたがる「複合体験」の正準記述子。
///
/// sensus は pure・別バッファ（画像 / 音声）アーキテクチャのため、メニエール病のような
/// 「回転性めまい（視覚）＋ 難聴・耳鳴り（聴覚）」の複合症状を 1 つのバッファでは表せない。
/// `Experience` は「どの視覚フィルタとどの聴覚フィルタを組にすれば仕様どおりの複合体験になるか」を
/// ライブラリ側で正準化する。consumer（universal-experience の GUI 等）は三徴候の組み合わせを
/// ハードコードせず、`Experience` から視覚・聴覚それぞれのフィルタと緊急度を取得できる。
#[derive(Debug, Clone, PartialEq)]
pub struct Experience {
    /// 安定した識別子（i18n キー等に使う英語 ID）
    pub id: &'static str,
    /// 視覚側フィルタ（視覚要素が無い体験では `None`）
    pub vision: Option<Filter>,
    /// 聴覚側フィルタ（聴覚要素が無い体験では `None`）
    pub hearing: Option<HearingFilter>,
    /// 受診喚起の緊急度
    pub urgency: Urgency,
}

impl Experience {
    /// メニエール病: 回転性めまい（視覚）＋ 低音域難聴・低い唸る耳鳴り（聴覚）の三徴候。
    /// ⚠️ 早期受診が望ましい。
    pub const MENIERE: Experience = Experience {
        id: "meniere",
        vision: Some(Filter::Vertigo),
        hearing: Some(HearingFilter::Meniere),
        urgency: Urgency::EarlyConsultation,
    };

    /// 良性発作性頭位めまい症（BPPV）: 頭位変化で生じる回転性めまい。
    ///
    /// **聴覚症状は無い**（耳石が三半規管に入り込む純粋な前庭性めまいで、蝸牛＝聴覚は
    /// 障害されない）。したがって `hearing: None` が医学的に正しい。良性で緊急性も低い。
    pub const BPPV: Experience = Experience {
        id: "bppv",
        vision: Some(Filter::BppvRotation),
        hearing: None,
        urgency: Urgency::None,
    };

    /// 前庭神経炎（vestibular neuritis）: 突然の激しい回転性めまい。
    ///
    /// 前庭神経のみの炎症で**聴力は保たれる**（難聴・耳鳴りを伴えばそれは迷路炎＝
    /// [`Experience::LABYRINTHITIS`]）。この聴覚温存が両者の鑑別点なので `hearing: None`。
    /// 突然発症のめまいは脳卒中との鑑別が必要なため緊急。
    pub const VESTIBULAR_NEURITIS: Experience = Experience {
        id: "vestibular_neuritis",
        vision: Some(Filter::VestibularNeuritis),
        hearing: None,
        urgency: Urgency::Emergency,
    };

    /// 迷路炎（labyrinthitis）: 回転性めまい（視覚）＋ 高音域感音難聴・高音の耳鳴り（聴覚）。
    ///
    /// 内耳（蝸牛を含む）の炎症で、前庭神経炎と違い**聴覚症状を伴う**。
    /// 「めまいの聴覚側複合」を医学的に正しく表せる前庭性疾患（メニエール病と並ぶ）。
    /// 突発的な感音難聴は早期治療が予後を左右するため早期受診が望ましい。
    pub const LABYRINTHITIS: Experience = Experience {
        id: "labyrinthitis",
        vision: Some(Filter::Vertigo),
        hearing: Some(HearingFilter::Labyrinthitis),
        urgency: Urgency::EarlyConsultation,
    };

    /// 視覚側フィルタを画像に適用する。視覚要素が無い体験では `Ok(None)`。
    pub fn apply_vision(
        &self,
        img: image::DynamicImage,
        strength: f32,
    ) -> Result<Option<image::DynamicImage>> {
        match self.vision {
            Some(f) => Ok(Some(apply(f, img, strength)?)),
            None => Ok(None),
        }
    }

    /// 聴覚側フィルタを音声バッファに適用する。聴覚要素が無い体験では `Ok(None)`。
    pub fn apply_audio(
        &self,
        buf: hearing::AudioBuffer,
        strength: f32,
    ) -> Result<Option<hearing::AudioBuffer>> {
        match self.hearing.clone() {
            Some(f) => Ok(Some(apply_hearing(f, buf, strength)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearing::AudioBuffer;
    use image::{DynamicImage, RgbImage};

    fn test_image() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(16, 16, |x, y| {
            image::Rgb([(x * 8) as u8, (y * 8) as u8, 128])
        }))
    }

    fn test_audio() -> AudioBuffer {
        AudioBuffer {
            samples: vec![0.1; 2000],
            sample_rate: 44100,
            channels: 1,
        }
    }

    /// 1×N の十分大きな画像で 2 つの画像が byte 同一かどうか。
    fn images_equal(a: &DynamicImage, b: &DynamicImage) -> bool {
        a.to_rgba8().into_raw() == b.to_rgba8().into_raw()
    }

    fn gradient_image(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        }))
    }

    /// 回帰: BppvRotation / Vertigo は静止画でも代表位相で実際に効果が出る
    /// （`apply()` が time_t=0 を渡して恒等変換になっていた B1 の再発防止）。
    #[test]
    fn bppv_and_vertigo_are_not_identity_through_apply() {
        let img = gradient_image(64, 64);

        let bppv = apply(Filter::BppvRotation, img.clone(), 1.0).unwrap();
        assert!(
            !images_equal(&img, &bppv),
            "BppvRotation must rotate the still image (regression: time_t=0 made it identity)"
        );

        let vertigo = apply(Filter::Vertigo, img.clone(), 1.0).unwrap();
        assert!(
            !images_equal(&img, &vertigo),
            "Vertigo must visibly tilt/blur the still image"
        );
    }

    /// 回帰: Floaters の gaze_x/gaze_y が apply() 経由で実際に反映される
    /// （視線位置が違えば出力も変わる）。
    #[test]
    fn floaters_gaze_position_affects_output() {
        let img = gradient_image(64, 64);
        let left = apply(
            Filter::Floaters {
                seed: 7,
                density: 1.0,
                size: 2.0,
                gaze_x: 0.1,
                gaze_y: 0.1,
            },
            img.clone(),
            1.0,
        )
        .unwrap();
        let right = apply(
            Filter::Floaters {
                seed: 7,
                density: 1.0,
                size: 2.0,
                gaze_x: 0.9,
                gaze_y: 0.9,
            },
            img,
            1.0,
        )
        .unwrap();
        assert!(
            !images_equal(&left, &right),
            "different gaze positions must yield different floaters"
        );
    }

    #[test]
    fn experience_meniere_canonical_pairing() {
        let e = Experience::MENIERE;
        assert_eq!(e.id, "meniere");
        assert_eq!(e.vision, Some(Filter::Vertigo));
        assert_eq!(e.hearing, Some(HearingFilter::Meniere));
        assert_eq!(e.urgency, Urgency::EarlyConsultation);
    }

    #[test]
    fn experience_vestibular_pairings_are_medically_correct() {
        // BPPV と前庭神経炎は聴力温存 = hearing None。迷路炎のみ聴覚を伴う。
        assert_eq!(Experience::BPPV.vision, Some(Filter::BppvRotation));
        assert_eq!(Experience::BPPV.hearing, None);
        assert_eq!(Experience::BPPV.urgency, Urgency::None);

        assert_eq!(
            Experience::VESTIBULAR_NEURITIS.vision,
            Some(Filter::VestibularNeuritis)
        );
        assert_eq!(Experience::VESTIBULAR_NEURITIS.hearing, None);
        assert_eq!(Experience::VESTIBULAR_NEURITIS.urgency, Urgency::Emergency);

        assert_eq!(
            Experience::LABYRINTHITIS.hearing,
            Some(HearingFilter::Labyrinthitis)
        );
        assert!(Experience::LABYRINTHITIS.vision.is_some());
    }

    #[test]
    fn experience_vision_only_returns_none_audio() {
        // BPPV は視覚のみ → 音声適用は Ok(None)
        let bppv = Experience::BPPV;
        assert!(bppv.apply_audio(test_audio(), 1.0).unwrap().is_none());
        assert!(bppv.apply_vision(test_image(), 1.0).unwrap().is_some());
    }

    #[test]
    fn experience_meniere_applies_both_modalities() {
        let e = Experience::MENIERE;
        let img = e.apply_vision(test_image(), 0.8).unwrap();
        assert!(img.is_some(), "MENIERE has a vision component");
        let audio = e.apply_audio(test_audio(), 0.8).unwrap();
        assert!(audio.is_some(), "MENIERE has a hearing component");
    }

    #[test]
    fn experience_missing_modality_returns_none() {
        // 聴覚のみ / 視覚のみの体験では欠けている側が None を返すことを確認する。
        let vision_only = Experience {
            id: "vision_only",
            vision: Some(Filter::Cataract { seed: 0 }),
            hearing: None,
            urgency: Urgency::None,
        };
        assert!(vision_only
            .apply_audio(test_audio(), 1.0)
            .unwrap()
            .is_none());
        assert!(vision_only
            .apply_vision(test_image(), 1.0)
            .unwrap()
            .is_some());

        let hearing_only = Experience {
            id: "hearing_only",
            vision: None,
            hearing: Some(HearingFilter::Tinnitus { freq_hz: 4000.0 }),
            urgency: Urgency::None,
        };
        assert!(hearing_only
            .apply_vision(test_image(), 1.0)
            .unwrap()
            .is_none());
        assert!(hearing_only
            .apply_audio(test_audio(), 1.0)
            .unwrap()
            .is_some());
    }
}
