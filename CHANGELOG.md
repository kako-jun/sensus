# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **vision: Flickering Stars フィルタ追加** (#59):
  `flickering_stars(img, strength, seed)` を `vision.rs` に追加。
  LCG でランダムな光点を生成して additive blend する。光点数 = `(strength × 200.0) as usize`。
  各光点は半径 2 px の矩形ブロブ。`Filter::FlickeringStars` を `lib.rs` に追加。
  `flickering_stars.frag` GLSL シェーダ追加。
  `docs/overview.md` に医学的注記追加:「急激な光点の増加・カーテン状の視野欠損を伴う場合は網膜剥離の前兆。即受診。」
  テスト: strength=0 → identity、strength=1 → 出力の最大輝度が入力より高いこと。

- **vision: Teichopsia フィルタ追加** (#58):
  `teichopsia(img, strength)` を `vision.rs` に追加。視野周辺にジグザグ縞の光（要塞スペクトル）を重畳し、
  内側（scotoma）を暗化する。リング領域（正規化距離 0.2〜0.5）で saw wave 輝度加算、
  内側（< 0.2）を `strength × 0.7` で暗化。`Filter::Teichopsia` を `lib.rs` に追加。
  `teichopsia.frag` GLSL シェーダ追加。
  `docs/overview.md` に医学的注記追加:「偏頭痛の前兆として 20〜30 分続く。初めて経験する場合は眼科・神経内科を受診。」
  テスト: strength=0 → PSNR ≥ 60 dB、strength=1 → 画像中心が暗化。

- **vision: Detail Loss フィルタ追加** (#57):
  `detail_loss(img, strength)` を `vision.rs` に追加。矩形タイルごとに平均色に置き換える（pixelation）。
  タイルサイズ = `(strength × 20.0).max(1.0) as u32` px（strength=1 で 20px タイル）。
  `strength=0` で identity、`strength=1` で標準偏差が入力より低くなること。
  `Filter::DetailLoss` を `lib.rs` に追加。
  `detail_loss.frag` GLSL シェーダ追加。
  テスト: strength=0 → identity、strength=1 → 輝度標準偏差が入力より低いこと。

- **vision: Contrast Sensitivity フィルタ追加** (#56):
  `contrast_sensitivity(img, strength)` を `vision.rs` に追加。
  輝度コントラストを linear sRGB 空間で midpoint (0.5) に引き寄せる。
  式: `output = 0.5 + (input − 0.5) × (1.0 − strength × 0.5)`。
  `strength=0` で identity、`strength=1` で 50% コントラスト圧縮。
  `Filter::ContrastSensitivity` を `lib.rs` に追加。
  `contrast_sensitivity.frag` GLSL シェーダ追加。
  テスト: strength=0 → PSNR ≥ 60 dB、strength=1 → 輝度分散が入力より小さいこと。

### Fixed

- **vision: cataract の散乱ノイズを LCG ベース Simplex-like ノイズに改善** (#50):
  旧実装の 8×8 矩形ブロックノイズ（空間非連続）を格子頂点 LCG + smoothstep bilinear 補間による
  空間相関ノイズ（CELL_SIZE=32）に置き換え。白濁パターンがより自然な滲みになった。
  黄変マトリクス係数のコメントに医学的出典を追記:
  Pokorny et al. (1987) *Applied Optics* 26(8) および van Norren & Vos (1974) *Vision Research* 14(11)。
  `cataract.frag` も同じ格子補間ノイズ + 出典コメントに更新（`uSeed` / `uResolution` uniform 追加）。

- **vision: nyctalopia に Purkinje shift を追加** (#51):
  暗所では桿体が支配的になり分光感度が青寄り（507 nm）にシフトする Purkinje 現象を実装。
  scotopic luminance 計算（`0.0610 R + 0.3751 G + 0.6038 B`、Vos 1978）を導入し、
  strength に応じて photopic / scotopic blend、青チャネル微増（`×(1 + s×0.1)`）、
  赤チャネル微減（`×(1 − s×0.2)`）を適用。`nyctalopia.frag` も同様に更新。
  テスト追加: strength=1 で青チャネル合計が赤チャネル合計を上回ること（Purkinje shift 確認）。

### Added

- **vision: glaucoma に弧状暗点モードを追加** (#52):
  `GlaucomaMode` enum を導入し、臨床的により正確な Bjerrum 弧状暗点パターンを実装。
  ON head を中央から水平 15% オフセットした位置に設定し、極座標リング + `sin(θ)` フェードで
  弧状マスクを生成。モード一覧:
  - `Vignette` — 従来の均等周辺暗化（後方互換）
  - `ArcuateSuperior` — 上方弧状暗点（上半球 Bjerrum scotoma）
  - `ArcuateInferior` — 下方弧状暗点（下半球 Bjerrum scotoma）
  - `Biarcuate` — 上下両方（進行期緑内障）
  `lib.rs` のデフォルト呼び出しは `Vignette` を維持し後方互換性を保つ。
  テスト追加: 各モードで strength=0 → 元画像一致、strength=1 → 暗化確認（7 ケース）。
  `docs/overview.md` にモード説明と「均等暗化は近似」注記を追記。

- **shader: tetrachromacy/vertigo/bppv_rotation/vestibular_neuritis/floaters の GLSL シェーダ追加** (#48):
  - `tetrachromacy.frag` — LMS 変換 + Cb/Cr 誇張（uStrength）
  - `vestibular_neuritis.frag` — 水平シフト + 1D blur（uStrength, uRadiusPx, uShiftTexel）
  - `vertigo.frag` — 回転変位（uStrength, uTime）
  - `bppv_rotation.frag` — nystagmus パターン回転（uStrength, uTime）
  - `floaters.frag` — hash ベース floater パターン（uStrength, uSeed）
  - `shaders.rs` に各 `*_glsl()` / `*_uniforms()` + uniform 構造体を追加。

- **test: tetrachromacy/vestibular_neuritis の PSNR ≥ 30 dB テストを追加** (#48):
  vertigo / bppv_rotation / floaters は `include_str!` コンパイルテストのみ。

### Changed

- **perf: floaters の距離判定で sqrt を省略** (#62):
  ループ内の `dist = (dx*dx + dy*dy).sqrt()` を除去し、`dist_sq < half_w_sq` による二乗比較に変更。
  マスク値の計算には依然 `dist_sq.sqrt()` を使用するが、大多数のピクセルはガード条件で早期 skip されるため全体のコストを削減。

### Added

- **docs: 医学的緊急注記を diplopia/vestibular_neuritis/cataract/nyctalopia に追加** (#64):
  - `diplopia`: 🚨 即救急 — 突然の複視は動眼神経麻痺・脳幹梗塞の可能性
  - `vestibular_neuritis`: 🚨 即救急 — 突然の激しいめまいは脳卒中との鑑別が必要
  - `cataract`: ⚠️ 即受診 — 急激な視力低下・視野変化は眼科受診推奨
  - `nyctalopia`: ⚠️ 早期受診 — 夜盲の急激な悪化はビタミンA欠乏・網膜色素変性の可能性

- **test: vertigo / bppv_rotation / vestibular_neuritis の strength=1 効果確認テストを追加** (#60):
  `vertigo_strength_one_differs_from_input`、`bppv_rotation_strength_one_differs_from_input`、
  `vestibular_neuritis_strength_one_differs_from_input` の 3 テストを追加。
  グラデーション画像に strength=1 を適用し、少なくとも 1 ピクセルが変化することを確認する。

- **test: hearing フィルタの strength=1 効果確認テストを追加** (#61):
  `paracusis_strength_one_differs_from_input`、`dysmelodia_strength_one_differs_from_input`、
  `amusia_strength_one_differs_from_input` の 3 テストを追加。
  サイン波入力に strength=1 を適用し、出力 RMS が入力 RMS と有意に異なることを確認する。

- **vision: Metamorphopsia（歪視）フィルタを追加** (#55):
  LCG ベースの 2D グリッドノイズ変位マップで各ピクセルをリマップする `metamorphopsia()` 関数を実装。
  `strength=0` は byte-exact identity、`strength=1` で最大 8px の変位。双線形補間 + edge clamp。
  `Filter::Metamorphopsia` を enum に追加し、`apply()` / Pipeline の `apply()` に対応。
  GLSL シェーダー `metamorphopsia.frag`（hash2D ベースの smooth noise）と `metamorphopsia_glsl()` / `MetamorphopsiaUniforms` / `metamorphopsia_uniforms()` を追加。

- **shader: 視野欠損 4 種の GLSL ES 3.00 シェーダを追加** (#46):
  `glaucoma.frag`（周辺ビネット、smoothstep マスク）、`macular_degeneration.frag`（中心暗化、foveal smoothstep マスク）、`hemianopia.frag`（左右半側マスク）、`tunnel_vision.frag`（急峻なトンネルビネット）の 4 シェーダを `crates/core/src/shaders/` に追加。
  `shaders.rs` に `glaucoma_glsl()` / `macular_degeneration_glsl()` / `hemianopia_glsl()` / `tunnel_vision_glsl()` および対応する uniform 構造体・計算関数を追加。
  uniform 構造体: `FieldOfVisionUniforms { strength }` / `HemianopiaUniforms { strength, side }`（side: 1.0=右側欠損, -1.0=左側欠損）。

### Changed

- **docs: overview.md の APD・hearing フィルタ名の乖離を修正** (#63):
  APD セクションを「後回し」から「実装済み」に更新し、実装詳細（ノイズ注入・FIR スミア・gap 埋め）を記載。
  hearing フィルタ一覧を実際の関数名（`hearing_loss`, `sudden_hearing_loss`, `noise_induced_hearing_loss`,
  `tinnitus`, `hyperacusis`, `paracusis`, `amusia`, `dysmelodia`, `pitch_shift_semitones`,
  `diplacusis`, `auditory_processing_disorder`）に修正し、フィルタ数を 10 → 11 に訂正。

- **vision: depth_aware_blur をビン線形補間に変更** (#54):
  深度値を 8 段階ビンに量子化して不連続に切り替える方式を廃止し、隣接 2 ビンの blur 結果を線形補間する方式に変更。
  `t = frac(d * 7.0)` を補間係数として `out = blur[bin_floor] * (1-t) + blur[bin_ceil] * t` を適用し、ビン境界でのバンディングアーティファクトを除去。
  メモリ使用量を 8 枚同時保持から 2 枚逐次処理に削減。

### Fixed

- **BREAKING: vision: diplopia を加算合成から alpha blend に修正** (#53):
  `out = orig + ghost * alpha`（加算）を `out = orig * (1 - alpha) + ghost * alpha`（alpha blend）に変更。
  加算合成では輝度が加算されて白飛びが生じていたが、alpha blend により合計輝度が保存される。
  `strength=1` かつ `ghost_strength=1` の場合、従来は加算で白飛びしていたが、
  修正後は幽霊像がそのまま重なった状態（ghost が完全に前面）になる。
  GLSL シェーダ `diplopia.frag` も同様に修正。

## [0.3.0] - 2026-05-25

### Added

- **vision: eye_strain / dry_eye フィルタ** (#36): 眼精疲労（コントラスト圧縮 + 微小 disk blur + vignette）とドライアイ（LCG ノイズマスクによる空間的不均一ぼかし）を追加。GLSL シェーダ `eye_strain.frag` / `dry_eye.frag` 付き。

- **hearing: APD（聴覚情報処理障害）** (#37): LCG ノイズ混入 + FIR スミア + 短い無音 gap 埋めの 3 段処理で時間分解能低下を模倣。`HearingFilter::AuditoryProcessingDisorder` として追加。

- **vision: floaters 形状改善** (#38): 円形ブロブ 30% + LCG ランダムウォーク糸くず形状 70% の混合。seed パラメータを実際に使用するよう修正。描画後に box blur でエッジをソフト化。

- **vision: tetrachromacy アルゴリズム刷新** (#39): gamut 拡張ヒューリスティックから Machado 2009 LMS 変換ベースのメタメリズム強調に刷新。`|M - L| < 0.05` のメタメリックペア候補領域で Cb/Cr を誇張。

- **vision: cataract 黄変・青感度低下** (#40): 白濁 haze overlay に加えて黄変マトリクス（B チャネル 0.85 倍 + RG クロストーク）とコントラスト圧縮を追加。実際の白内障に近い色温度シフトを再現。

- **bench: criterion ベースのフィルタベンチマーク** (#41): `crates/core/benches/filters.rs` を新設。9 フィルタ × 512×512 を `cargo bench` で計測可能に。

- **CLI: `--pipe` による動画フレーム連続処理** (#42): stdin から JPEG フレームを連続読み込み（FFD8/FFD9 境界で切り出し）、フィルタ適用後に stdout に書き出す。ffmpeg との pipe 連携で動画処理が可能に。

## [0.2.0] - 2026-05-25

### Added

- **stereo: Android XMP Depth extraction** (#32):
  `sensus_core::stereo::read_xmp_depth(data: &[u8])` extracts the depth map
  embedded in Android portrait-mode JPEGs (Google Depth API). Scans all
  `APP1` segments for `GDepth:Data`, decodes the base64-encoded PNG/JPEG
  payload without external dependencies, and returns a `DynamicImage`.
  Returns `Error::NoDepthMap` when no depth data is present.
  CLI gains `--portrait <PATH>`: auto-extracts the XMP depth map and applies
  depth-aware blur in one command. `--portrait` is mutually exclusive with
  `--mpo` and `--depth`; `--input` is optional when `--portrait` is used.

- **stereo: MPO stereo photography depth map generation** (#31):
  `sensus_core::stereo` module with `split_mpo(data: &[u8])` and
  `stereo_to_depth(left, right)`. `split_mpo` splits an MPO file into
  left- and right-eye `DynamicImage` by scanning for the `FFD9 FFD8`
  (JPEG EOI + SOI) boundary. `stereo_to_depth` computes a greyscale depth
  map via block-matching SAD (`BLOCK_SIZE = 7`, `MAX_DISPARITY = 64`);
  brighter pixels are closer. The depth map can be passed directly to
  `depth_aware_blur`. CLI gains `--mpo <PATH>` for fully automated
  MPO → depth → blur in a single command.

- **vision: diplopia / nystagmus / starbursts** (#29): three new motion /
  visual-optics filters. `diplopia` alpha-blends a pixel-shifted ghost image
  (linear sRGB) to simulate double vision from strabismus or nerve palsy.
  `nystagmus` applies 1D directional motion blur (`amplitude`, `direction_deg`)
  to represent the involuntary oscillatory eye movement visible as a static
  snapshot. `starbursts` performs radial ray-marching from supra-threshold
  bright pixels (`threshold`, `num_rays`, `ray_length_ratio`) to simulate
  the starburst artefact seen after LASIK / cataract surgery or in high
  astigmatism. All three include GLSL ES 3.00 fragment shaders
  (`diplopia.frag`, `nystagmus.frag`, `starbursts.frag`).
  CLI gains `--offset-x`, `--offset-y`, `--ghost-strength`, `--amplitude`,
  `--direction-deg`, `--num-rays`, `--ray-length`, `--threshold`.

- **test: CPU⇄GLSL shader equivalence regression** (#17): GPU-free software
  simulator (`crates/core/tests/shader_equivalence.rs`) mirrors the GLSL ES
  math in Rust and asserts that CPU and shader outputs agree within tolerance
  — ≤ 2/255 per channel for matrix filters, PSNR ≥ 30 dB for blur/directional
  filters. 13 tests covering protanopia/deuteranopia/tritanopia/achromatopsia
  (strength = 0, 0.5, 1), myopia/hyperopia/presbyopia disk blur, and
  astigmatism at 0°/45°/90°.

- **vision: depth-aware blur** (#19): `vision::depth_aware_blur(img,
  depth_map, focus_depth, max_radius_ratio, kind)` accepts a greyscale PNG
  depth map (bright = near, dark = far) and applies per-pixel disk blur
  whose radius scales with distance from `focus_depth`. Three kinds:
  `Myopia` (far side blurs), `Hyperopia` (near side blurs),
  `DepthOfField` (both sides blur). Depth maps of a different resolution
  than the source image are auto-resized with Lanczos3. CLI gains
  `--filter myopia-depth / hyperopia-depth / depth-of-field`,
  `--depth <PATH>`, `--focus <f32>` (validated to 0.0..=1.0);
  combining depth filters with other `--filter` flags is now a hard error.

- **vision: GLSL ES 3.00 shader source API** (#16): `sensus_core::shaders`
  exposes `*_glsl()` functions returning `&'static str` for each visual
  filter, plus matching `*_uniforms()` helpers that compute ready-to-upload
  uniform structs (`ColorMatrixUniforms`, `LumaUniforms`, `BlurUniforms`,
  `AstigmatismUniforms`, `DiplopiaUniforms`, `NystagmusUniforms`,
  `StarburstsUniforms`). All shaders target GLSL ES 3.00 (`#version 300 es`)
  for Flutter `FragmentProgram` compatibility. The CPU implementation is the
  normative reference; shaders are authored to reproduce the same math.

- **hearing filters** (#7, #8, #9): `sensus_core::hearing` module with
  `AudioBuffer` (f32 interleaved PCM), `BiquadFilter`, and 10 pure-function
  hearing filters — `hearing_loss`, `sudden_deafness`, `noise_induced_loss`,
  `tinnitus`, `diplacusis`, `hyperacusis`, `amusia`, `presbycusis`,
  `recruitment`, `temporary_threshold_shift`. Three vestibular-visual filters
  added to `vision`: `vertigo`, `bppv_rotation`, `vestibular_neuritis`.
  `HearingFilter` enum and `apply_hearing()` added to `lib.rs`. CLI gains
  `--filter vertigo / bppv-rotation / vestibular-neuritis`.

## [0.1.0] - 2026-05-22

### Added

- **Phase 3 visual field & light filters** (#5, #6): `glaucoma`,
  `macular_degeneration`, `hemianopia`, `tunnel_vision`, `cataract`,
  `floaters`, `photophobia`, `nyctalopia` (night-blindness). All implemented
  as composable single-pass image operations in linear sRGB. `glaucoma` and
  `tunnel_vision` apply a radial vignette mask; `hemianopia` blanks the
  appropriate half-field; `macular_degeneration` blurs and dims the foveal
  region; `cataract` adds a haze overlay; `floaters` composites translucent
  blobs; `photophobia` brightens and halates highlights; `nyctalopia` darkens
  and desaturates the image.
- **Pipeline support** via `sensus_core::pipeline`: apply multiple filters
  in sequence in a single command with `--filter f1 --filter f2 …`.
- **tetrachromacy** exploration filter (#3): expands the chrominance gamut
  to simulate four-cone perception. Implemented via a heuristic gamut
  expansion in LMS space.
- First stable crates.io release (`v0.1.0`). `sensus-core` and `sensus` are
  now published; `cargo install sensus` is the recommended install path (#12).
- **Phase 2 focus / refraction filters** (#4): `myopia`, `hyperopia`,
  `presbyopia`, `astigmatism`. All implemented as **disk (pillbox) blur**
  in linear sRGB — Gaussian is intentionally rejected because the
  defocused eye images a point source as a *circle of confusion*, not a
  Gaussian. `strength = 1.0` corresponds to the clinical maxima -6 D /
  +4 D / +3 D add / -3 CD respectively, mapped to a `min(W, H)`-relative
  radius assuming a 4 mm mesopic pupil and a 30° image FOV at ~50 cm
  viewing distance. The Smith–Helmholtz small-angle approximation
  `θ ≈ pupil(m) × |D|` returns angular *diameter*, so the disk radius is
  `θ / 2`. `astigmatism()` is **1D directional blur** (pure cylindrical
  lens / line spread function), not an elliptical disk: a cylindrical
  refractive error focuses light to a line, so the optically correct
  point-spread is one-dimensional in the meridian perpendicular to the
  cylinder axis. The kernel's short axis is sub-pixel
  (`MIN_BLUR_RADIUS_PX = 0.5 px`), making the implementation a 1-row
  directional box filter. `axis_deg` denotes the sharp meridian (medical
  convention); the blurred direction is at `axis_deg + 90°`. Alpha is
  preserved.
  Implementation uses precomputed per-row spans + a horizontal prefix
  sum so the cost is `O(W × H × kernel_height)` (≈ 1 s for myopia at
  1024 × 1024, well under the 5 s target).
- CLI gains an `--axis` flag (range `0.0..=180.0`, default `90.0`) for
  astigmatism. Other filters ignore it. `apply(Filter::Astigmatism, …)`
  always uses the default 90° axis; library users who need a custom axis
  call `vision::astigmatism()` directly.
- **Phase 1 color vision deficiency filters** (#2): `protanopia`,
  `deuteranopia`, `tritanopia`, `achromatopsia`. Implemented in linear
  sRGB space. `protanopia` / `deuteranopia` / `tritanopia` use the
  Machado, Oliveira & Fernandes 2009 severity = 1.0 matrices
  (DOI: [10.1109/TVCG.2009.113](https://doi.org/10.1109/TVCG.2009.113))
  and blend towards the original in linear space for intermediate
  `strength` values. `achromatopsia` uses CIE photopic luminance with
  BT.709 primaries (`0.2126 R + 0.7152 G + 0.0722 B`); BT.601 is
  intentionally avoided. Alpha is preserved.
- `sensus_core::apply()` dispatches all implemented filters and returns
  `Error::NotImplemented` only for variants not yet landed.
- CLI now writes the transformed image to `--output` on success
  (exit code `0`) for any implemented filter.
- Cargo workspace scaffold with two crates: `sensus-core` (pure logic) and
  `sensus` (CLI binary). `sensus-core` is centralized in
  `[workspace.dependencies]`. (#1)
- `sensus_core::Filter` enum (17 variants covering all planned vision
  filters) plus `sensus_core::apply()` facade returning `Result`. CLI
  derives clap-side `Filter` and converts via `to_core()`. (#1)
- `sensus_core::Error` (thiserror-derived) with `NotImplemented(Filter)`
  and `Image(image::ImageError)` variants, and `sensus_core::Result<T>`
  alias. (#1)
- GitHub Actions workflows: `ci.yml` (test / fmt / clippy with
  `--all-targets --locked`) and `release.yml` (tag-driven build with
  `-p sensus --locked` for x86_64-linux, x86_64-apple, aarch64-apple,
  x86_64-windows; uploads tarballs / zips to GitHub Releases). (#1)
- Documentation: `README.md` (English, end-user master), `docs/overview.md`
  (English, design), `docs/roadmap.md` (Japanese, phase tracker),
  `CLAUDE.md` (Japanese, AI-facing internal notes). (#1)
- MIT license. (#1)

[0.3.0]: https://github.com/kako-jun/sensus/releases/tag/v0.3.0
[0.2.0]: https://github.com/kako-jun/sensus/releases/tag/v0.2.0
[0.1.0]: https://github.com/kako-jun/sensus/releases/tag/v0.1.0
