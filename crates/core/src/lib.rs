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
    /// 緑内障。`mode` は [`vision::GlaucomaMode`] を参照。デフォルト: Vignette。
    /// `field_loss_mode` は [`vision::FieldLossMode`] を参照。デフォルト: Darken（#171）
    Glaucoma {
        mode: vision::GlaucomaMode,
        field_loss_mode: vision::FieldLossMode,
    },
    /// 黄斑変性。`field_loss_mode` は [`vision::FieldLossMode`] を参照。デフォルト: Darken（#171）
    MacularDegeneration {
        field_loss_mode: vision::FieldLossMode,
    },
    /// 半盲。`side`: 0.0 = 左視野消失, 1.0 = 右視野消失。
    /// `field_loss_mode` は [`vision::FieldLossMode`] を参照。デフォルト: Darken（#171）
    Hemianopia {
        side: f32,
        field_loss_mode: vision::FieldLossMode,
    },
    /// 視野狭窄。`field_loss_mode` は [`vision::FieldLossMode`] を参照。デフォルト: Darken（#171）
    TunnelVision {
        field_loss_mode: vision::FieldLossMode,
    },
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
        Filter::Glaucoma {
            mode,
            field_loss_mode,
        } => vision::glaucoma(img, strength, mode, field_loss_mode),
        Filter::MacularDegeneration { field_loss_mode } => {
            vision::macular_degeneration(img, strength, field_loss_mode)
        }
        Filter::Hemianopia {
            side,
            field_loss_mode,
        } => vision::hemianopia(img, strength, side, field_loss_mode),
        Filter::TunnelVision { field_loss_mode } => {
            vision::tunnel_vision(img, strength, field_loss_mode)
        }
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

/// フィルタ種別ごとの静的メタデータ（kako-jun/sensus#182）。
///
/// 消費側（universal-experience）がフィルタ単位で「受診喚起・典型的な強度・出典・
/// このシミュレーションで表現できないこと」を取得できるようにする API。値は
/// payload（`axis_deg` 等）ではなく **バリアント種別だけ** で決まる（同じ
/// `Filter::Astigmatism { axis_deg }` なら軸角度によらず同じメタデータを返す）。
///
/// > **医療監修は入っていない。** 以下の値は公開されている医学文献・ガイドライン
/// > から読み取れる一般的傾向、またはそれが無い場合は sensus の設計上の既定値
/// > （典型例として妥当な範囲に収める、という工学的判断）であり、個々の患者の
/// > 診断・重症度を表すものではない。根拠は各 match アームのコメントに残す。
impl Filter {
    /// 受診喚起の緊急度。
    ///
    /// [`Experience`] が持つ `Urgency` と**同じ尺度・同じ根拠**を返す。[`Experience`] が
    /// 特定のフィルタを使う体験（[`Experience::BPPV`] の `Filter::BppvRotation` など）を
    /// 定義している場合は、その `Urgency` と矛盾しない値をここでも返す（
    /// `filter_urgency_matches_experience_urgency` テストで保証する）。
    ///
    /// 根拠は `docs/overview.md` の "Medical notes (when to see a doctor)" 表を正本とする。
    /// 同表が `None / ⚠️` のように両論併記しているフィルタ（`photophobia` / `dry_eye`）は、
    /// 「典型的には良性」という表の記述に合わせて `Urgency::None` を返し、急変時の受診目安
    /// は [`limitations`](Self::limitations) 側の注記に譲る。
    pub fn urgency(&self) -> Urgency {
        use Urgency::{EarlyConsultation, Emergency, None as NoUrgency};
        match self {
            // 色覚特性: 先天性・安定 — overview.md 表 "Color vision type ... congenital and stable"
            Filter::Protanopia
            | Filter::Deuteranopia
            | Filter::Tritanopia
            | Filter::Achromatopsia
            | Filter::Tetrachromacy => NoUrgency,
            // 屈折異常: 眼鏡等で矯正可能 — overview.md 表 "Refractive — corrected with lenses"
            Filter::Myopia
            | Filter::Hyperopia
            | Filter::Presbyopia
            | Filter::Astigmatism { .. } => NoUrgency,
            // 視野欠損: 無痛性に進行するため早期発見が視野温存に直結する
            Filter::Glaucoma { .. } | Filter::TunnelVision { .. } => EarlyConsultation,
            Filter::MacularDegeneration { .. } => EarlyConsultation,
            // 突然の半盲は脳卒中を疑う所見（overview.md "Sudden half-field loss is a stroke
            // until proven otherwise"）
            Filter::Hemianopia { .. } => Emergency,
            Filter::Cataract { .. } => EarlyConsultation,
            Filter::Floaters { .. } => EarlyConsultation,
            // 羞明は多くが良性 — overview.md "Often benign light sensitivity"。急変時の受診
            // 目安は limitations() に記す
            Filter::Photophobia => NoUrgency,
            Filter::NightBlindness => EarlyConsultation,
            // Experience::BPPV.urgency と同じ根拠（良性・聴力温存の前庭疾患）
            Filter::BppvRotation => NoUrgency,
            // Experience::MENIERE / Experience::LABYRINTHITIS がどちらも Filter::Vertigo を
            // EarlyConsultation で使うため、フィルタ単体でも同じ値を返す
            Filter::Vertigo => EarlyConsultation,
            // Experience::VESTIBULAR_NEURITIS.urgency と同じ根拠（突然発症の激しいめまいは
            // 脳卒中との鑑別が必要）
            Filter::VestibularNeuritis => Emergency,
            // 突然の複視は脳神経麻痺・脳幹卒中のサイン
            Filter::Diplopia { .. } => Emergency,
            Filter::Nystagmus { .. } => EarlyConsultation,
            Filter::Starbursts { .. } => EarlyConsultation,
            // 疲れ目・ドライアイ由来が大半 — overview.md "Often lighting/fatigue related"
            Filter::EyeStrain => NoUrgency,
            // ドライアイも多くが良性。持続する痛み・視力変化があれば受診（limitations() 参照）
            Filter::DryEye => NoUrgency,
            Filter::Metamorphopsia { .. } => EarlyConsultation,
            Filter::ContrastSensitivity => NoUrgency,
            Filter::DetailLoss { .. } => NoUrgency,
            Filter::Teichopsia => EarlyConsultation,
            // 光視症の急増は網膜剥離のサイン — overview.md "Surge of flashes + curtain →
            // retinal detachment"
            Filter::FlickeringStars { .. } => Emergency,
        }
    }

    /// 典型的な症状の程度として推奨される `strength`（`apply()` に渡す値、`(0.0, 1.0]`）。
    ///
    /// `strength = 1.0` は「そのフィルタが表現できる最大の効果」であって「典型例」では
    /// ない。たとえば `tunnel_vision` の `strength = 1.0` はほぼ視野が閉じた末期像に近く、
    /// 多くの利用者が体験する程度の視野狭窄ではない。ここではそうした極端な値を消費側の
    /// 既定値にしないため、各フィルタの「よくある／代表的な」程度を返す。
    ///
    /// 医学文献に典型値の記載がある場合はそれを、無い場合は各モジュールの
    /// `strength = 1.0` の定義（何 D 相当か等）から見て「よくある軽〜中等度」に収まる
    /// sensus の設計上の既定値をコメント付きで返す。
    pub fn recommended_strength(&self) -> f32 {
        match self {
            // Machado 2009 の severity テーブルは 0.0(正常)〜1.0(完全2色覚) の連続量。人口の
            // 多くを占める色覚異常は完全な2色覚(dichromacy)ではなく軽度の異常3色覚(-omaly)
            // なので、テーブル中央値 0.5（moderate anomalous trichromacy）を典型値とする。
            // 1.0（完全2色覚）は color.rs の regression anchor であって典型例ではない。
            Filter::Protanopia | Filter::Deuteranopia | Filter::Tritanopia => 0.5,
            // 全色盲は"程度"を持たない全か無かの錐体機能不全なので、フル効果がそのまま典型像
            Filter::Achromatopsia => 1.0,
            // 四色型色覚は疾患ではなく「4種類目の錐体による弁別」の可視化なので、部分効果に
            // 臨床的な意味はない。フル効果が意図した可視化を表す（sensus 設計上の既定値）
            Filter::Tetrachromacy => 1.0,
            // MYOPIA_MAX_RADIUS_RATIO は -6D（強度近視の入口）相当。実際に多いのは軽度〜中等度
            // (おおよそ -2D 台) の近視なので、その比率にあたる 0.4 を典型値とする（sensus 設計
            // 上の既定値。個々の屈折度分布の統計調査は行っていない）
            Filter::Myopia => 0.4,
            // HYPEROPIA_MAX_RADIUS_RATIO は +4D 相当。軽度〜中等度(+1.5D 前後)を典型とする
            Filter::Hyperopia => 0.4,
            // PRESBYOPIA_MAX_RADIUS_RATIO は +3.00D add（老視の臨床的上限に近い）相当。
            // 一般的な老視の加入度数はもっと低い(+1.5〜+2.0D 程度)ことが多いため 0.6 を典型値とする
            Filter::Presbyopia => 0.6,
            // ASTIGMATISM_MAX_RADIUS_RATIO は -3CD（乱視としては強い部類）相当。軽度〜中等度の
            // 乱視の方が多いため 0.4 を典型値とする
            Filter::Astigmatism { .. } => 0.4,
            // 緑内障は早期〜中期で発見されることが多く、末期の広範な暗点化は典型例ではない
            Filter::Glaucoma { .. } => 0.4,
            Filter::MacularDegeneration { .. } => 0.4,
            // 同名半盲は「視野の半分が完全に見えない」という all-or-nothing に近い所見なので、
            // 部分的な弱いフィルタは典型像を薄める。フル効果を典型値とする
            Filter::Hemianopia { .. } => 1.0,
            // strength=1.0 はほぼ中心視野のみが残る末期像（kako-jun/sensus#182 が問題視した
            // ケース）。多くの網膜色素変性症・緑内障末期以前の視野狭窄はもっと緩やかなので、
            // 中間値の 0.5 を典型値とする
            Filter::TunnelVision { .. } => 0.5,
            // 加齢性白内障は進行がゆっくりで、軽度〜中等度の混濁期間が長い
            Filter::Cataract { .. } => 0.5,
            Filter::Floaters { .. } => 0.4,
            Filter::Photophobia => 0.4,
            Filter::NightBlindness => 0.5,
            // BPPV は発作時のみ強い回転性めまいが出るが、発作自体はこの強度が典型
            Filter::BppvRotation => 0.6,
            Filter::Vertigo => 0.6,
            // 前庭神経炎は「突然の激しい」めまいが臨床的な特徴そのものなので、他の前庭系
            // フィルタより強めの値を典型とする
            Filter::VestibularNeuritis => 0.7,
            Filter::Diplopia { .. } => 0.5,
            Filter::Nystagmus { .. } => 0.5,
            Filter::Starbursts { .. } => 0.5,
            Filter::EyeStrain => 0.5,
            Filter::DryEye => 0.4,
            Filter::Metamorphopsia { .. } => 0.5,
            Filter::ContrastSensitivity => 0.4,
            Filter::DetailLoss { .. } => 0.4,
            Filter::Teichopsia => 0.5,
            Filter::FlickeringStars { .. } => 0.5,
        }
    }

    /// モデル名と出典（DOI など）。出典が無ければ `None`（出典のでっち上げはしない）。
    ///
    /// ここでの「出典」は、このフィルタのアルゴリズム定義に使われている一次資料を指す。
    /// `docs/adr/matrix-provenance.md` に記載がある色覚特性はそれを、各モジュールの doc
    /// comment に出典が明記されているものはそれを返す。出典が明記されていないフィルタ
    /// （sensus 独自のヒューリスティック実装）は `None` を返す。
    pub fn citation(&self) -> Option<&'static str> {
        match self {
            // docs/adr/matrix-provenance.md §1 が正本
            Filter::Protanopia | Filter::Deuteranopia | Filter::Tritanopia => Some(
                "Machado, Oliveira & Fernandes (2009), \"A Physiologically-based Model for \
                 Simulation of Color Vision Deficiency\", IEEE TVCG. DOI: 10.1109/TVCG.2009.113",
            ),
            // ADR-0004 が正本。ITU-R BT.709 の photopic luminance (CIE Y) 係数を使う
            Filter::Achromatopsia => {
                Some("ITU-R BT.709 photopic luminance (CIE Y) weights; see ADR-0004")
            }
            // docs/adr/matrix-provenance.md §3: Hunt-Pointer-Estévez 由来の係数だが
            // colorimetric な忠実性は主張しないビジュアライゼーション用ヒューリスティック
            // なので、出典として引用しない（でっち上げ防止）
            Filter::Tetrachromacy => None,
            // ADR-0003 が正本。瞳孔径×屈折度から角直径を求める Smith–Helmholtz 近似で
            // disk/cylinder blur 半径を導出する（module doc 冒頭のディオプター換算式）
            Filter::Myopia
            | Filter::Hyperopia
            | Filter::Presbyopia
            | Filter::Astigmatism { .. } => Some(
                "Smith–Helmholtz relation (pupil diameter × diopter → angular \
                     circle-of-confusion diameter approximation); see ADR-0003",
            ),
            Filter::Glaucoma { .. } => None,
            Filter::MacularDegeneration { .. } => None,
            Filter::Hemianopia { .. } => None,
            Filter::TunnelVision { .. } => None,
            // light.rs の doc comment が正本。水晶体黄変マトリクスの係数の出典
            Filter::Cataract { .. } => Some(
                "Pokorny et al. (1987), \"Aging of the human lens\", Applied Optics 26(8): \
                 1437-1440; van Norren & Vos (1974), \"Spectral transmission of the human \
                 ocular media\", Vision Research 14(11): 1237-1244 (lens-yellowing \
                 coefficients, reinterpreted — see light.rs doc comment)",
            ),
            Filter::Floaters { .. } => None,
            Filter::Photophobia => None,
            // light.rs の doc comment が正本。scotopic luminance 係数（Purkinje shift）の出典
            Filter::NightBlindness => Some(
                "Vos (1978), \"Colorimetric and photometric properties of a 2° fundamental \
                 observer\", Color Research & Application 3(3): 125-128 (scotopic luminance \
                 weights)",
            ),
            Filter::BppvRotation => None,
            Filter::Vertigo => None,
            Filter::VestibularNeuritis => None,
            Filter::Diplopia { .. } => None,
            Filter::Nystagmus { .. } => None,
            Filter::Starbursts { .. } => None,
            Filter::EyeStrain => None,
            Filter::DryEye => None,
            Filter::Metamorphopsia { .. } => None,
            Filter::ContrastSensitivity => None,
            Filter::DetailLoss { .. } => None,
            Filter::Teichopsia => None,
            Filter::FlickeringStars { .. } => None,
        }
    }

    /// このシミュレーションで表現できないことの簡潔な説明（英語 1〜2 文）。
    ///
    /// i18n は消費側の責務なので、キーではなく英文そのものを返す（Issue #182 提案の
    /// 「キーまたは英文」のうち英文を採用: 消費側は必要なら自前で翻訳・要約する）。
    pub fn limitations(&self) -> &'static str {
        match self {
            Filter::Protanopia | Filter::Deuteranopia | Filter::Tritanopia => {
                "Represents one point on the Machado severity table (population-typical \
                 anomalous trichromacy), not an anomaloscope-calibrated measurement of any \
                 individual's color vision."
            }
            Filter::Achromatopsia => {
                "Collapses color to a single achromatic axis; does not model the photophobia, \
                 nystagmus, and reduced visual acuity that usually accompany real achromatopsia."
            }
            Filter::Tetrachromacy => {
                "A heuristic visualization of 'a fourth cone type might separate these colors,' \
                 not a colorimetrically accurate simulation of tetrachromatic perception (no \
                 validated model of a functional fourth cone response exists)."
            }
            Filter::Myopia | Filter::Hyperopia | Filter::Presbyopia => {
                "Applies a single uniform blur radius to the whole 2D image; real refractive \
                 blur varies with each object's distance, which a flat image has no depth data \
                 to represent."
            }
            Filter::Astigmatism { .. } => {
                "Simulates an isolated cylinder error only; commonly co-occurring spherical \
                 error (myopia/hyperopia) is not included and must be composed separately."
            }
            Filter::Glaucoma { .. } => {
                "Uses an idealized static field-defect pattern (vignette or arcuate scotoma); \
                 real glaucomatous field loss is irregular and progresses gradually over years, \
                 which a single still-image strength cannot convey."
            }
            Filter::MacularDegeneration { .. } => {
                "Models the central scotoma as a smooth radial gradient; real AMD scotomas are \
                 often patchy and irregular, and this filter does not include the distortion \
                 covered separately by the metamorphopsia filter."
            }
            Filter::Hemianopia { .. } => {
                "Draws a clean, softly-blurred vertical boundary; real hemianopic field cuts can \
                 be irregular, sometimes macular-sparing, and appear instantly rather than as a \
                 gradual effect."
            }
            Filter::TunnelVision { .. } => {
                "Represents a static snapshot of a condition that usually develops gradually \
                 over years and is often worse in low light, neither of which a single strength \
                 value on a still image can show."
            }
            Filter::Cataract { .. } => {
                "Combines lens-yellowing and scatter glare into one strength axis; does not \
                 distinguish nuclear, cortical, and posterior subcapsular cataract subtypes, \
                 which look visually different."
            }
            Filter::Floaters { .. } => {
                "Draws synthetic circular/thread-like shapes from a fixed statistical \
                 distribution; does not reproduce any individual's actual floater shapes, nor \
                 the sudden flash/posterior-vitreous-detachment context that would be a warning \
                 sign."
            }
            Filter::Photophobia => {
                "Only brightens and blooms highlights; does not model the eye pain or headache \
                 that often accompanies clinically significant photophobia."
            }
            Filter::NightBlindness => {
                "Approximates the photopic-to-scotopic luminance shift and desaturation; does \
                 not model the loss of visual acuity or the longer dark-adaptation time that \
                 real night blindness involves."
            }
            Filter::BppvRotation => {
                "Renders one representative phase of a rotational still image; real BPPV \
                 attacks are brief, triggered by specific head movements, and accompanied by \
                 nystagmus that a static image cannot fully convey."
            }
            Filter::Vertigo => {
                "Renders one representative phase of a continuous rotational sensation on a \
                 still image; the felt motion and any accompanying nausea are not represented."
            }
            Filter::VestibularNeuritis => {
                "Approximates the visual sway with a horizontal shift and motion blur; does not \
                 model the accompanying nausea, vomiting, or gait imbalance."
            }
            Filter::Diplopia { .. } => {
                "Draws a fixed ghost offset for the whole frame; real diplopia varies with gaze \
                 direction and distance, and this filter does not distinguish monocular from \
                 binocular double vision."
            }
            Filter::Nystagmus { .. } => {
                "Approximates involuntary eye motion with a directional blur on a still image; \
                 does not reproduce the actual jerk/pendular waveform or the associated \
                 oscillopsia timing."
            }
            Filter::Starbursts { .. } => {
                "Draws stylized rays from bright highlights; the ray count/length/dispersion are \
                 design parameters, not a fit to any individual's optical aberration."
            }
            Filter::EyeStrain => {
                "Approximates asthenopia as mild contrast loss, vignette, and blur; does not \
                 model the accompanying eye ache, headache, or difficulty focusing over time."
            }
            Filter::DryEye => {
                "Approximates dry eye as a visual effect only; does not model the physical \
                 grittiness, stinging, or fluctuating focus that are the primary symptoms."
            }
            Filter::Metamorphopsia { .. } => {
                "Uses a deterministic grid-noise displacement field; real metamorphopsia \
                 distortion patterns (as seen on an Amsler grid) are specific to each patient's \
                 retinal pathology and are not fit to any individual case."
            }
            Filter::ContrastSensitivity => {
                "Compresses contrast around a fixed mathematical midpoint in linear space, not a \
                 perceptual midpoint; does not model spatial-frequency-dependent contrast loss."
            }
            Filter::DetailLoss { .. } => {
                "Pixelates uniformly across the whole image; does not model acuity loss that \
                 varies with eccentricity (worse in the periphery, for example) as in some real \
                 conditions."
            }
            Filter::Teichopsia => {
                "Draws a stylized expanding zigzag arc; real fortification-spectrum patterns \
                 vary in shape between individuals and evolve over 20-30 minutes, which a single \
                 strength value on a still image cannot show."
            }
            Filter::FlickeringStars { .. } => {
                "Draws randomly distributed static points of light; does not reproduce the \
                 flickering/scintillating timing of real photopsia, nor the visual-field \
                 location the flashes would actually occupy."
            }
        }
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

    /// kako-jun/sensus#182: 全 `Filter` バリアントを網羅する固定リスト。
    ///
    /// payload は代表値（デフォルト相当）を使う。メタデータ関数はバリアント種別だけで
    /// 決まる契約なので、payload の値自体はテストの本質ではない。
    fn all_filter_variants() -> Vec<Filter> {
        vec![
            Filter::Protanopia,
            Filter::Deuteranopia,
            Filter::Tritanopia,
            Filter::Achromatopsia,
            Filter::Tetrachromacy,
            Filter::Myopia,
            Filter::Hyperopia,
            Filter::Astigmatism { axis_deg: 90.0 },
            Filter::Presbyopia,
            Filter::Glaucoma {
                mode: vision::GlaucomaMode::Vignette,
                field_loss_mode: vision::FieldLossMode::Darken,
            },
            Filter::MacularDegeneration {
                field_loss_mode: vision::FieldLossMode::Darken,
            },
            Filter::Hemianopia {
                side: 0.0,
                field_loss_mode: vision::FieldLossMode::Darken,
            },
            Filter::TunnelVision {
                field_loss_mode: vision::FieldLossMode::Darken,
            },
            Filter::Cataract { seed: 0 },
            Filter::Floaters {
                seed: 0,
                density: 1.0,
                size: 1.0,
                gaze_x: 0.5,
                gaze_y: 0.5,
            },
            Filter::Photophobia,
            Filter::NightBlindness,
            Filter::Vertigo,
            Filter::BppvRotation,
            Filter::VestibularNeuritis,
            Filter::Diplopia {
                offset_x: 0.02,
                offset_y: 0.0,
                ghost_strength: 0.5,
            },
            Filter::Nystagmus {
                amplitude: 0.02,
                direction_deg: 0.0,
            },
            Filter::Starbursts {
                num_rays: 8,
                ray_length_ratio: 0.2,
                threshold: 0.8,
                dispersion: 0.3,
            },
            Filter::EyeStrain,
            Filter::DryEye,
            Filter::Metamorphopsia { freq: 8.0, seed: 0 },
            Filter::ContrastSensitivity,
            Filter::DetailLoss { cell_size: 8 },
            Filter::Teichopsia,
            Filter::FlickeringStars { seed: 0 },
        ]
    }

    #[test]
    fn all_filter_variants_covers_every_variant() {
        // enum の全 30 バリアントを数え上げていること自体の回帰テスト。新しいバリアントが
        // 追加されたのにこのリストが追従していないと、以降のメタデータ網羅テストが
        // 新バリアントを検査しないまま静かに通ってしまうので、個数を固定で pin する。
        assert_eq!(all_filter_variants().len(), 30);
    }

    #[test]
    fn every_filter_variant_has_metadata() {
        for filter in all_filter_variants() {
            // urgency() / citation() / limitations() は全バリアントで呼べる(パニックしない)。
            let _ = filter.urgency();
            let _ = filter.citation();
            assert!(
                !filter.limitations().is_empty(),
                "{filter:?}: limitations() must not be empty"
            );
        }
    }

    #[test]
    fn recommended_strength_is_in_valid_range() {
        for filter in all_filter_variants() {
            let s = filter.recommended_strength();
            assert!(
                s > 0.0 && s <= 1.0,
                "{filter:?}: recommended_strength() = {s} must be in (0.0, 1.0]"
            );
        }
    }

    /// tunnel_vision の recommended_strength は「ほぼ視野が閉じた末期像」にならないこと
    /// （kako-jun/sensus#182 が問題にした、消費側が strength=1.0 から始めてしまう不整合の
    /// 直接の回帰テスト）。strength=1.0 に近いと `inner_r`(視野半径) が 0 に近づき画像の
    /// ほとんどが暗転するため、0.7 未満であることを固定する。
    #[test]
    fn tunnel_vision_recommended_strength_is_not_near_black() {
        let s = Filter::TunnelVision {
            field_loss_mode: vision::FieldLossMode::Darken,
        }
        .recommended_strength();
        assert!(
            s < 0.7,
            "tunnel_vision recommended_strength = {s} is too close to full closure"
        );
    }

    #[test]
    fn filter_urgency_matches_experience_urgency() {
        // BPPV: 良性・聴力温存の前庭疾患。Experience と Filter が矛盾した結論を出さないこと
        // （このズレが #182 の背景そのもの）。
        assert_eq!(Filter::BppvRotation.urgency(), Experience::BPPV.urgency);

        // 前庭神経炎: 突然発症の激しいめまいは緊急。
        assert_eq!(
            Filter::VestibularNeuritis.urgency(),
            Experience::VESTIBULAR_NEURITIS.urgency
        );

        // Vertigo は MENIERE / LABYRINTHITIS どちらの Experience からも参照される。
        // フィルタ単体の urgency はどちらの Experience の urgency とも一致する必要がある。
        assert_eq!(Filter::Vertigo.urgency(), Experience::MENIERE.urgency);
        assert_eq!(Filter::Vertigo.urgency(), Experience::LABYRINTHITIS.urgency);
    }

    #[test]
    fn citation_is_only_present_when_documented() {
        // 出典を主張してよいのは、docs/adr や各モジュールの doc comment に実際に一次資料が
        // 書かれているフィルタだけ（でっち上げ禁止）。
        for filter in [
            Filter::Protanopia,
            Filter::Deuteranopia,
            Filter::Tritanopia,
            Filter::Achromatopsia,
            Filter::Myopia,
            Filter::Hyperopia,
            Filter::Presbyopia,
            Filter::Astigmatism { axis_deg: 90.0 },
            Filter::Cataract { seed: 0 },
            Filter::NightBlindness,
        ] {
            assert!(
                filter.citation().is_some(),
                "{filter:?} has a documented source and should return Some(_)"
            );
        }

        // 四色型色覚のヒューリスティック行列は colorimetric な出典として主張しない
        // (docs/adr/matrix-provenance.md §3)。
        assert_eq!(Filter::Tetrachromacy.citation(), None);
        // sensus 独自実装で外部の一次資料が無いフィルタは None。
        assert_eq!(
            Filter::Glaucoma {
                mode: vision::GlaucomaMode::Vignette,
                field_loss_mode: vision::FieldLossMode::Darken,
            }
            .citation(),
            None
        );
        assert_eq!(Filter::EyeStrain.citation(), None);
    }
}
