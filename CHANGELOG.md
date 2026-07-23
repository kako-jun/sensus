# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **feat: protanopia / deuteranopia / tritanopia now resolve intermediate `strength` against the full Machado 2009 per-severity table (11 entries, 0.1 steps) instead of blending toward the severity=1.0 matrix**: 旧実装（ADR-0002）は severity=1.0 行列の結果と原色を `strength` で linear blend する近似で、真の per-severity 行列と最大 ~111/255（tritanopia、mean ~4.7/255）ズレていた。`crates/core/src/vision/color.rs` に `PROTANOMALY_TABLE` / `DEUTERANOMALY_TABLE` / `TRITANOMALY_TABLE`（各 11 段、`table[0]`=単位行列・`table[10]`=既存 severity=1.0 const と一致、Machado 2009 / VIP-Sim `myRecolour.cs` と照合済み）を追加し、`strength` をグリッド上ならそのまま・非グリッド点は隣接 2 エントリを行列要素空間で lerp して解決した行列を直接適用する方式（`resolve_severity_matrix`）に変更。`strength=0.0`（identity）は byte-exact 不変。`strength=1.0` は既存 severity=1.0 結果と実数演算としては同一だが、f32 の非結合性により 256^3 全数実測で protanopia 28 / deuteranopia 11 / tritanopia 6（各 16,777,216 ピクセル中）で ±1 LSB の差が発生する（詳細 ADR-0008、`tests/color_severity1_full_sweep.rs`）。GLSL シェーダ（`protanopia.frag` / `deuteranopia.frag` / `tritanopia.frag`）も CPU 側で解決済みの行列を uniform で受け取り直接適用するよう追従（従来の `uStrength` blend は削除）。**breaking**: `.frag` ソースを直接取得して自前で uniforms を組んでいる消費者は、`uStrength` blend の撤去により `*_uniforms()`（`protanopia_uniforms` 等）経由での行列取得へ移行が必要（自前で severity=1.0 行列のみを渡す実装は中間 strength で誤った結果になる）。`achromatopsia` は対象外で ADR-0002 のまま変更なし。ADR-0002 は本変更（ADR-0008）で protanopia/deuteranopia/tritanopia について supersede（achromatopsia は継続）。KAT (`crates/core/tests/color_kat.rs`) に severity=0.5（テーブルのグリッド点）・0.25（非グリッド点の補間）の新規 cross-check を追加。詳細は ADR-0008・`docs/adr/matrix-provenance.md`（Issue #165）。
- **internal: split CLI `main.rs` into `arguments` / `filter_mapping` / `depth_resolver` modules (behavior unchanged)**: 約 963 行の `crates/cli/src/main.rs` に同居していた clap 引数定義・CLI→core 変換・depth blur 統合・オーケストレーションを責務ごとに分割。`arguments`（`Cli` struct / `Filter`・`Hearing` ValueEnum / `parse_*` バリデータ）、`filter_mapping`（`Filter::to_core` 等の core enum 変換 / `warn_unused_flags`）、`depth_resolver`（`depth_kinds` / `apply_filters_to_image` / `apply_non_depth_filters` / `DEPTH_BLUR_MAX_RADIUS_RATIO`）へ純粋移動し、`main.rs` には `main` / `run` / `run_audio` / `run_pipe` / `split_jpeg_frames` / `RunError` を残置。clap 属性・default・help・value_parser・mapping・depth 計算・seed 既定はすべて不変で、`sensus --help` の出力はバイト等価。CLI 統合テスト（cli / pipe / depth_compose / mpo / portrait / audio）と workspace 全テストが緑であることを確認（Issue #159）。
- **internal: split `vision` god-file into submodules (behavior unchanged)**: 約 5892 行に 28 フィルタが同居していた `crates/core/src/vision.rs` を、症状領域ごとの `vision/` サブモジュール（`color` / `refraction` / `field` / `light` / `motion` / `fatigue` / `phenomena`）＋クロスドメイン共有ヘルパー `common` に分割。`vision/mod.rs` が全公開アイテムを `pub use` で再エクスポートし、`crate::vision::*` の解決パスは分割前と完全に一致する（`lib.rs` の `apply()` / tests / CLI / universal-experience(FFI) の参照は不変）。純粋な移動リファクタで、ロジック・定数・数値・seed 既定・丸めは一切変更していない。298 unit test / KAT / shader_equivalence / CLI integration の全テストが緑であることを確認。

### Added

- **`vision::srgb_to_linear` / `vision::linear_to_srgb` are now public utilities**: GLSL の `srgbToLinear` / `linearToSrgb` と同一式の sRGB ⇄ linear gamma 変換を公開 util 化。`tests/shader_equivalence.rs` が private に持っていた同一式の重複定義を削除し、CPU 正本（`sensus_core::vision`）を参照するよう統合した（Issue #157）。

### Fixed

- **fix: astigmatism の NaN 軸で CPU/GLSL の挙動が分岐していた問題を修正**: `shaders::astigmatism_uniforms` は `axis_deg + 90.0` を無条件計算しており、CPU 側（`vision::refraction::astigmatism`）が持つ正規化（NaN→90.0 フォールバック、有限値は `rem_euclid(180.0)`）を欠いていた。NaN 軸を library から直接渡すと GLSL は cos/sin=NaN で全 tap 不採用となり黒画像になる一方、CPU は 90° フォールバックでボケを返すという分岐が起きていた（CLI 経由は `parse_axis` が弾くため library 直呼びのみの問題）。正規化ロジックを `vision::refraction::normalize_axis_deg` に切り出し、CPU/GLSL 両方から呼ぶよう統一した（Issue #169）。
- **fix: tinnitus が低サンプルレート + 既定周波数で無音化する Nyquist 退化を修正**: `tinnitus` は正弦波周波数 `freq_hz` をサンプルレートに対して clamp しておらず、低レート WAV（例 8kHz）と既定 freq 4000Hz（= Nyquist ちょうど）の組み合わせでは `sin(2π・(fs/2)・n/fs) = sin(nπ) = 0` が全サンプルで成立し、耳鳴りトーンが実質無音化していた。同ファイルの biquad 系フィルタが既に持つ `[1.0, fs * 0.4999]` clamp 規約に統一した（Issue #169）。
- **fix: tinnitus の `sample_rate=0` で時間軸が NaN/inf になる問題を修正**: 時間軸 `t` を `buf.sample_rate as f32` で直接計算しており、`sample_rate=0` の `AudioBuffer` では `0.0/0.0`（frame=0）や `x/0.0`（frame>0）で `t` が NaN/inf になっていた。biquad 系と同じ `sample_rate=0 → 44100Hz` フォールバックを `effective_sample_rate` として一元化し、時間軸・周波数 clamp の両方でこれを共有するよう統一した（Issue #169）。

## [0.5.0] - 2026-05-30

### Added

- **feat: CLI から残り 5 つの vision フィルタを利用可能に（metamorphopsia / contrast-sensitivity / detail-loss / teichopsia / flickering-stars）**: core には実装済み・GLSL シェーダもあるのに CLI の `--filter` から選べなかった 5 フィルタを公開。併せて専用フラグを追加: `--size`（floaters サイズ倍率）/ `--dispersion`（starbursts 虹色度）/ `--cell-size`（detail-loss タイルサイズ）/ `--meta-freq` / `--meta-seed`（metamorphopsia）。`cargo test -p sensus --test cli` に新フィルタ疎通テストを追加。

### Changed

- **refactor: フィルタ固有パラメータを `Filter` enum payload に一元化し、`FilterStep` を `{filter, strength}` に簡約**: 従来は一部パラメータが `Filter` enum に、一部が `pipeline::FilterStep` のスカラフィールドに二重に存在し、CLI が両方へ値を入れていたため、enum を読むフィルタでは CLI 値が無視されていた（下記 B2 の根本原因）。`Cataract { seed }` / `Floaters { …, gaze_x, gaze_y }` / `Diplopia { offset_x, offset_y, ghost_strength }` / `Nystagmus { amplitude, direction_deg }` / `Metamorphopsia { freq, seed }` へパラメータを移動。`FilterStep::apply` は `crate::apply` に委譲するだけになり、**単体適用と pipeline 適用の挙動が常に一致**することを型で保証。`pipeline.rs:12` の設計メモ（「パラメータは enum 側のみ」）に実装を揃えた。

### Fixed

- **fix: (B2) `--axis` / `--side` / `--num-rays` / `--ray-length` / `--threshold` / `--seed` / `--density` 等の CLI フラグが pipeline まで届かず固定値が使われていた問題を修正**: `Filter::to_core` が enum payload にハードコードのデフォルトを詰めていたため、astigmatism の軸・hemianopia の側・starbursts の各パラメータ・floaters の seed/density 等が常に既定値で適用されていた。`to_core(&Cli)` が全パラメータを CLI フラグから構築するよう修正。回帰テスト `cli_axis_actually_changes_astigmatism_output`（axis=0 と 90 で出力が変わる）を追加。
- **fix: (B1) `apply(Filter::Vertigo)` / `apply(Filter::BppvRotation)` が `time_t=0` 固定で恒等変換（無効果）になっていた問題を修正**: 静止画は時間軸を持てないため、効果がピークになる代表位相（`VERTIGO_STILL_TIME_S` / `BPPV_STILL_TIME_S`）で 1 フレームを描くようにした。`Experience::MENIERE` / `LABYRINTHITIS` / `BPPV` の視覚側もこれで実際に回転する。回帰テスト `bppv_and_vertigo_are_not_identity_through_apply` を追加。
- **fix: `cargo fmt --check` が `crates/cli/src/audio.rs` / `main.rs` で失敗していた整形ずれを修正**。

### Changed

- **chore: kako-jun/sensus#113 v0.1-0.2 コア独立レビュー実施 — 色覚関数の strength 正規化を共通化**: 5 ラウンドレビュー（#68-#90）が未カバーだった v0.1-0.2 コア（色覚行列・視野マスク・初代 hearing DSP・stereo/MPO/XMP パーサ・SAD 深度推定）を correctness 観点で精査。**致命的なバグは検出されず**（パーサは bounds-checked、SAD は overflow なし、色覚・マスクは linear 空間で clamp 済み）。唯一の整合性所見として、色覚関数（`apply_machado_matrix` / `achromatopsia`）が `normalize_strength` 相当の NaN/clamp 処理をインラインで重複していたのを、#120 で `pub(crate)` 化した `normalize_strength` に統一。挙動は不変（`achromatopsia_nan_strength_returns_identity` 等で確認）。

- **fix: kako-jun/sensus#108 depth フィルタを他フィルタと合成可能に（CLI 合成、hard error 解消）**: depth フィルタ（myopia-depth / hyperopia-depth / depth-of-field）は深度マップという第2入力が必要で Pipeline/Filter（単一画像）に載らないため、これまで他フィルタとの併用を hard error で拒否していた（`TODO(#19)`）。Pipeline を 2 入力対応に拡張する代わりに、**CLI 側で合成**する方針（#107 の「depth は単一入力契約に載せない」判断と整合）: 非 depth フィルタを Pipeline で先に適用し、その結果に `depth_aware_blur` をかける。`--depth` / `--mpo` / `--portrait` の 3 経路すべてで `--filter <color> --filter <depth>` の併用が可能に。depth フィルタは 1 つだけ許可（`depth_aware_blur` は単一 kind）。統合テスト 2 件（color+depth 合成が depth-only と異なる / depth 2 つは拒否）。

### Removed

- **fix: kako-jun/sensus#111 死にコードの `Error::NotImplemented` / exit-2 経路を削除し、dry_eye の seed doc を是正**: 全 `Filter` バリアントは実装済みで `apply()` の match は網羅的（新バリアント追加は未実装ならコンパイルエラー）なため、`Error::NotImplemented` は core から二度と返らず、CLI の exit-2 ハンドラは到達不能な死にコードだった。さらに `--pipe` 経路ではこの経路が `RunError::Pipeline`（exit 1）に誤マップされ、文書化された exit-2 契約と矛盾していた。`Error::NotImplemented` / `RunError::NotImplemented` / 両ハンドラ / 旧 scaffold の exit-2 記述を**まとめて削除**（終了コードは成功 0 / 失敗 1 に一本化）。`dry_eye` の doc が約束していた未実装関数 `dry_eye_with_seed` への参照を削除し、固定 seed=42 が `dry_eye.frag` との CPU↔GLSL 等価の前提である旨に是正（動画用 seed 対応は CPU/.frag 双方の uniform 化が要るためスコープ外）。

### Fixed

- **fix: kako-jun/sensus#112 `split_mpo` の JPEG マーカー無視（素朴窓スキャン）を marker 走査に統一**: `split_mpo` は `windows(4)` で `FFD9 FFD8` を素朴スキャンしており、第1フレームの entropy-coded scan data や APPn ペイロード中に同バイト列があると誤分割しえた（session515 が `split_jpeg_frames` で直したのと同じ untrusted-input パーサのバグ class）。SOI から marker/length を正しく走査して**真の EOI** を見つける `first_jpeg_end()` を追加（SOS 後の entropy data の FF00 スタッフィング・FFDn RST を正しく読み飛ばす）。回帰テスト 3 件（APP1 ペイロードに埋め込んだ FFD9 FFD8 を誤認しない / SOS entropy + stuffing + RST を走査 / 非 SOI・EOI 欠落の拒否）。

### Changed

- **fix: kako-jun/sensus#110 `Filter::Floaters.size` の死にフィールドを配線（受け取って捨てていた）**: `apply()` が `let _ = size` で無視し「現在は無視（0.0 を渡すこと）」と doc されていた死にフィールドを、blob 半径・糸くず幅の相対倍率として機能させた。`vision::floaters` / `floaters_mask` に `size` 引数を追加（0.1..=5.0 に clamp、0/NaN は 1.0 フォールバック）、`blob_radius` と strand `half_w` に乗じる。`apply()` / `Pipeline::FilterStep` から enum の `size` を渡すよう配線。マスクは CPU 生成（#134 方針 B）なので GLSL 側変更は不要。効果アサート `floaters_size_scales_coverage`（size 大 → 被覆面積増＝平均マスク低下、NaN→1.0 フォールバック）。

### Tests / Findings

- **test: kako-jun/sensus#114 hearing 効果アサートテストを網羅追加**: 従来は strength=0 恒等 / 空 / パニック耐性のみで「実際に効いているか」の効果検証が薄かった。6 件追加 — `diplacusis_left_and_right_differ`（L≠R = 左右別音程）/ `sudden_hearing_loss_attenuates_notch_band`（ノッチ帯域の RMS 低下）/ `noise_induced_hearing_loss_attenuates_4khz_more_than_low`（4 kHz を 500 Hz より強く減衰）/ `pitch_shift_changes_signal_but_keeps_length`（波形変化＋長さ保持）/ `tinnitus_adds_tone_on_nonsilent_signal`（無音だけでなく信号入りにもトーン重畳）/ `audio_pipeline_matches_sequential_apply_hearing`（AudioPipeline = 逐次 apply_hearing と bit 一致）。

### Fixed

- **fix: kako-jun/sensus#120 GLSL uniforms の strength を CPU と同じく正規化 / sim の nearest 規約を明文化**: (1) 全 `*_uniforms` helper（30 個）が strength を生値のまま構造体に格納しており、範囲外/NaN を渡す将来の呼び出し元で CPU（`normalize_strength` = clamp 0..1 + NaN→0）と `uStrength` が乖離しえた。`normalize_strength` を `pub(crate)` 化し、各 helper 入口で適用（有効範囲 0..1 は素通しなので既存等価テストに影響なし）。回帰テスト `uniforms_normalize_strength_like_cpu` を追加。(2) `shader_equivalence` の sim が NEAREST を `(uv*dim).round()` で近似する規約（実 GLSL は `floor`）を、PSNR ≥ 30 dB に吸収される半テクセル差として module doc に明文化（floor 統一は全 sim 再チューニングが必要で精度向上は閾値内のため round 据え置き）。

- **fix: kako-jun/sensus#109 depth-blur のマジック定数 0.023 を名前付き定数化して文書化**: depth フィルタ（myopia-depth / hyperopia-depth / depth-of-field）の CLI が `cli.strength * 0.023` という未文書化のマジック数で半径を算出していた。`DEPTH_BLUR_MAX_RADIUS_RATIO`（= 0.023、非深度の近視ディスクブラー `MYOPIA_MAX_RADIUS_RATIO` と同値・Smith–Helmholtz の近視最大相当）として名前付き定数化し、`--strength 1.0` がこの比＝**全効果（上限）**であることを doc で明記（縮小ではない）。3 箇所（mpo / portrait / depth）を定数参照に統一。

- **docs: kako-jun/sensus#115 CHANGELOG の架空 hearing 名 / overview APD 番号 / noise_induced 帯域幅の doc 不一致を修正**: (1) v0.2.0 節の hearing フィルタ一覧が架空名（`sudden_deafness`/`presbycusis`/`recruitment`/`temporary_threshold_shift`/`noise_induced_loss`）で「10」と誤記 → 実関数名 11 個に訂正。(2) overview.md の APD セクション見出しを Issue #38 → **#37**（#38 は floaters）。(3) `noise_induced_hearing_loss` の doc コメント「±1 kHz / 帯域幅 2000」を実装（`50 + s*950` Hz、最大 1000 Hz）に合わせて訂正。

- **fix: kako-jun/sensus#117 sensus-core 単体テストが jpeg feature 不足でビルド不能だったのを修正**: `stereo.rs` のテストが `image::codecs::jpeg::JpegEncoder` を使うが core の依存は `png` feature のみで、`cargo test -p sensus-core` / `cargo clippy --all-targets`（core 単体）が pre-existing で落ちていた。workspace test は cli の image default-features による feature unification で隠れていた。core の **dev-dependencies に `image` の `jpeg` feature** を追加（本体依存は png のみ維持）。CI に `cargo test -p sensus-core` 単体ステップを追加して回帰防止。

- **fix: kako-jun/sensus#134 floaters / flickering_stars のノイズを CPU↔GLSL で統一**:
  - **flickering_stars**: CPU の 64bit LCG（`(state>>32)` 抽出・32bit GPU で再現不能）を、点 index ベースの 32bit spatial hash（`vision::star_hash32`、#99/#125 cataract と同系列の黄金比混合 + XOR-shift）に統一。`flickering_stars.frag` も旧 Wang hash + 円形 distance + 合計後クランプから、同一 hash + 整数ピクセル中心の 5×5 正方ボックス + **点ごとの min(1.0) クランプ**（CPU と同順・同演算）に書き換え。点数は float 再計算による不一致を避けるため `FlickeringStarsUniforms.count`（= `(strength*200) as i32`）として渡す。`sim_flickering_stars_glsl` 忠実ミラーで CPU↔GLSL **PSNR ≥ 40 dB**（strength 1.0 / 0.5 非正方形 / seed 差）を検証。
  - **floaters**: CPU↔GLSL を **方針 B（CPU 生成マスクのテクスチャ受け渡し）**で bit 一致させた。`vision::floaters_mask(w, h, density, seed, gaze_x, gaze_y) -> GrayImage` を切り出し、blob+strand+3×3 box blur のマスク生成はライブラリ側に集約（乱歩ストランドの見た目を完全維持）。`vision::floaters` はこの u8 マスクから linear sRGB 乗算ブレンドするよう refactor。`floaters.frag` は別モデルのブロック hash 近似を廃し、`uMask` テクスチャ（`vision::floaters_mask` の出力）を `1 - strength*(1-mask)` で乗算ブレンドするだけに書き換え（depth_aware_blur の uDepth と同じ第2テクスチャパターン）。マスクは strength 非依存なので host は density/seed/gaze ごとに 1 回生成すれば strength を uniform で可変にできる。`FloatersUniforms` は strength のみに簡約。`sim_floaters_glsl` 忠実ミラーで CPU↔GLSL **PSNR ≥ 50 dB（bit 一致）**（strength 1.0 / 0.5 非正方形）+ マスクの strength 非依存・決定論を検証。

### Added

- **feat: kako-jun/sensus#107 depth_aware_blur の GLSL シェーダを追加（移植の残り半分）**: `vision::depth_aware_blur` は CPU 実装・ユニットテスト済みだったが GLSL シェーダが無く、universal-experience の Flutter FragmentProgram 経路から到達できなかった。`depth_aware_blur.frag` + `shaders::depth_aware_blur_glsl()` + `depth_aware_blur_uniforms()` を追加。深度マップを `uDepth` テクスチャで渡す単一パスシェーダで、per-fragment に深度から半径を求め Fibonacci lattice 16 tap disk（eye_strain/photophobia と同方式）で円盤近似ブラー。CPU は 8 ビン box blur の多パスと算法が異なるため bit/PSNR 等価は取らず、`.frag` 忠実ミラー sim で効果（ピント面鮮明 / 離れるほどぼける / kind による前後選択 / DoF 両側）を検証（5 件）。**`Filter` enum/apply() には載せない判断**: 深度ブラーは深度マップという第 2 入力を要し、`Copy` な単一入力 `Filter`/`apply(filter,img,strength)` 契約に収まらないため。consumer は Rust=`vision::depth_aware_blur`（既に pub）/ GLSL=`*_glsl()` + `uDepth` で到達する（doc に明記）。

- **feat: kako-jun/sensus#106 cataract に輝度・コントラスト低下を追加（VIP-Sim 二段モデルの未移植分）**: これまで黄変マトリクス + 白濁ノイズのみで、白内障の霞み感の核である輝度・コントラスト低下が移植されていなかった。VIP-Sim の BrightnessContrast 段に倣い、linear sRGB 空間で pivot 0.5 中心の per-channel コントラスト収縮（ContrastCoeff = (0.7, 0.7, 0.4)、青の散乱が最大）+ severity 比例の輝度低下を追加。`c_ch = 1 - s*(1 - coeff_ch)` で strength=0 のとき恒等。**CPU `vision::cataract` / `cataract.frag` / `sim_cataract_glsl` の 3 箇所に同一演算で実装し、既存の CPU↔GLSL 等価テスト（PSNR ≥ 30 dB）を維持**。効果アサート `cataract_reduces_brightness_and_contrast`（白黒 1×1 の輝度差が圧縮されることを検証）。

- **feat: kako-jun/sensus#105 聴覚フィルタ + AudioPipeline を CLI から利用可能に（`--audio` / WAV）**: 聴覚モジュール全体（14 フィルタ）と `AudioPipeline` が CLI から一切叩けず、Cargo.toml の description が "hearing loss" を宣伝しながら binary では到達不可だった矛盾を解消。
  - `--audio <in.wav> --hearing <filter>... -o <out.wav>`: WAV を読み、`--hearing` の聴覚フィルタを `AudioPipeline` で順に適用して WAV 出力。`--hearing` は複数指定でチェーン可。パラメータ付きフィルタ用に `--freq`（tinnitus/sudden-hearing-loss/misophonia）と `--semitones`（pitch-shift）を追加。
  - WAV I/O は `hound` で実装（`crates/cli/src/audio.rs`）。整数/浮動小数 PCM を正規化 f32 で読み、出力時に入力の bit 深度・形式へ戻す。チャンネル数は適用後バッファに追従（diplacusis の mono→stereo を保つ）。**mp3/flac 等の広域デコードは非対象**。
  - `--audio` は `--input`/`--filter`/`--pipe`/`--mpo`/`--portrait` と排他。`--hearing` 無し・`-o` 無しは明示エラー。
  - 統合テスト `crates/cli/tests/audio_integration.rs` 5 件（hearing-loss が 8 kHz を減衰 / チェーン / --hearing 無しで失敗 / diplacusis mono→stereo / --filter 併用で失敗）。README の CLI usage・フラグ表・hearing/Experience セクションを更新。
- **feat: kako-jun/sensus#104 前庭性めまいの聴覚側を医学的に正しく移植（BPPV/前庭神経炎は聴力温存、迷路炎を追加）**: 監査は「vertigo/BPPV/前庭神経炎は視覚半分のみ配線、聴覚側未移植」と疑ったが、調査の結果 **BPPV と前庭神経炎は定義上 聴力が保たれる**（前庭神経炎で難聴を伴えばそれは迷路炎）。難聴・耳鳴りを捏造せず、医学的に正しい配線にした。
  - `Experience::BPPV`（BppvRotation + `hearing: None` + 緊急性なし）/ `Experience::VESTIBULAR_NEURITIS`（VestibularNeuritis + `hearing: None` + Emergency〔脳卒中鑑別〕）: 聴力温存を doc で明記。
  - `Experience::LABYRINTHITIS` + `HearingFilter::Labyrinthitis` + `hearing::labyrinthitis()`: 「めまい＋難聴＋耳鳴り」の前庭性複合を医学的に正しく表せる迷路炎を追加。内耳（蝸牛含む）炎症で**高音域感音難聴 + 高音(4 kHz)耳鳴り**。前庭神経炎との鑑別点（聴覚症状の有無）を体験で示す。高音が低音より強く減衰（メニエールの低音減衰と逆向き）を効果アサート。
- **feat: kako-jun/sensus#103 メニエール病フィルタ + `Experience` 複合記述子を追加**: 仕様の三徴候「回転性めまい + 低音域難聴 + 低い唸る耳鳴り」のうち、フィルタ自体が存在しなかった移植漏れを解消。
  - `hearing::meniere()` + `HearingFilter::Meniere`: 聴覚側を合成。**低音域**感音難聴（メニエールの特徴。加齢性 = 高音カットの `hearing_loss` とは逆向き。100→800 Hz ハイパスの部分ブレンド）+ ~200 Hz の低い唸る耳鳴り。低音(60 Hz)が高音(2 kHz)より強く減衰することを効果アサート（`meniere_attenuates_low_more_than_high`）。
  - `Experience { id, vision: Option<Filter>, hearing: Option<HearingFilter>, urgency: Urgency }`: 視覚と聴覚にまたがる複合体験の正準記述子。sensus は pure・別バッファ（画像/音声）アーキテクチャで単一バッファに複合症状を持てないため、「どの視覚フィルタとどの聴覚フィルタを組にすれば仕様どおりか」をライブラリ側で正準化し、consumer（universal-experience GUI 等）が組み合わせをハードコードせず取得できるようにした。`Experience::MENIERE`（Vertigo + Meniere + 早期受診）。`apply_vision()` / `apply_audio()` は欠けた modality で `Ok(None)`。
  - `Urgency { None, EarlyConsultation, Emergency }`: 受診喚起の緊急度分類。局所化文字列を core に埋めず分類のみ保持し、consumer 側で i18n メッセージを出し分けられる設計。
- **feat: kako-jun/sensus#102 ミソフォニア（misophonia）聴覚フィルタを追加**: 仕様リーフ（sensus.md）に挙がっていたが未実装だった移植漏れを解消。`HearingFilter::Misophonia { freq_hz }` + `hearing::misophonia()`。ハイパーアクーシス（[`hyperacusis`] = 全帯域一様増幅）と違い、`freq_hz` 中心のトリガー帯域（band-reject の補集合）だけを最大 6 倍ブースト + tanh 倍音歪みで耳障り化し、帯域外は残す。この帯域選択性を効果アサートテスト（`misophonia_is_band_selective`: トリガー帯域 2 kHz の RMS 変化 > 帯域外 200 Hz の RMS 変化）で固定。strength=0 恒等 / 空バッファ / ステレオ保持も検証。

### Docs

- **docs: kako-jun/sensus#101 README を v0.4 に更新**: version 表記と `sensus-core` 依存 pin を `0.1` → `0.4` に更新。hearing を「(soon) / Phase 4」から「11 フィルタ実装済み（ライブラリ API 専用、CLI 非対応・#105 で追跡）」に修正。vision フィルタ表を実装済みの全フィルタ（balance/vertigo・eye fatigue 等を含む）に追従。GLSL シェーダの解像度依存 uniform（`uRadiusPx` / `uTexelSize`）を外部ホストで使う際は `*_uniforms()` ヘルパの値を設定する必要がある旨を consumer 向けに明記。

### Tests / Findings

- **test: kako-jun/sensus#100 等価テスト皆無だったフィルタ群に CPU↔GLSL 等価テストを追加 + 乖離を調査記録**:
  - 実 `.frag` を 1:1 ミラーする sim を作り PSNR 等価検証する確立パターン（#97〜#99）に従い、テストを追加。インライン別アルゴリズムによる偽装合格はしていない。
  - **nyctalopia**: `.frag` は CPU と式が完全 1:1（暗化 `1-s*0.7`・脱色 `s*0.8`・photopic/scotopic blend・Purkinje shift）。`sim_nyctalopia_glsl` で検証し strength 0.0/0.5/1.0・非正方形 64×32 すべて **max channel err 0（PSNR=∞、完全一致）**。
  - **diplopia**: `.frag`（texel オフセット + UV clamp + 最近傍参照の alpha blend）を `sim_diplopia_glsl`（GPU 最近傍を `floor(uv*dim)` で再現）でミラー。CPU の整数ピクセルオフセットを texel に変換して同じ ghost 変位を渡し、strength 0.0/0.5/1.0・非正方形 64×32/32×64 で **PSNR ≥ 38dB（実測 ∞）**。
  - **nystagmus**: `.frag` は astigmatism.frag と同一構造（16-tap 1D directional blur、+90° しない）なので `sim_astigmatism` で忠実ミラー。滑らか gradient では **PSNR 37.8dB（≥30）**。strength=0 identity・radius<0.5px passthrough・非正方形を追加。**乖離（別 Issue 候補）**: CPU は `ellipse_blur`（filled-ellipse box、短軸 0.5px）、GLSL は 16-tap 直線で、同じ 1D motion blur を別カーネルで量子化している。鋭いエッジ（実コンテンツ）では ~20dB まで乖離し、特に radius<1.0px で CPU の楕円が原点のみに退化して blur がほぼ消える。**astigmatism も同じ乖離を共有**するが radius<0.5px の passthrough 領域でしかテストされておらず顕在化していなかった。
  - **vertigo / bppv_rotation**: `.frag` は UV 空間（正方化）逆回転サンプリング、CPU はピクセル空間逆回転 + bilinear。**正方形画像では両者一致**（`sim_uv_rotation_glsl` で bilinear ミラー、vertigo 49.9dB / bppv 53.8dB）。strength=0 identity も追加。**乖離（別 Issue 候補）**: ① 非正方形では UV 空間回転が角度を歪ませ CPU と不一致。② vertigo CPU は回転後に周辺 disk blur（`s*0.015*min_dim`）を加えるが `.frag` に無い（32px 正方形では 0.48px<0.5px で blur がスキップされる領域で等価を取った）。
  - **starbursts（大乖離・別 Issue 必須）**: CPU は明部画素起点のレイマーチング（num_rays 本のレイを別レイヤーに加算）だが、`starbursts.frag` は単一パス制約で各画素を「自身の輝度」でその場ブライトニングするだけ。`.frag` コメント自身が「フルレイマーチング版は CPU 実装を参照」と明記。**根本的に別効果**で PSNR 等価は原理的に不成立。仮の等価テストは作らず、strength=0 恒等・決定論・レイ放射の効果アサートのみ追加。
  - **cataract（ノイズハッシュ乖離・別 Issue 候補）**: 黄変マトリクス（Pokorny 1987）は一致するが、白濁ノイズの LCG ハッシュが CPU（64bit、`(lcg>>32)/u32::MAX` の高位ビット抽出）と GLSL（同定数の下位 32bit で 32bit 演算）で異なり、頂点ノイズ値が完全に別物。加えて格子サンプリング規約も食い違う（CPU は整数ピクセル index `px/CELL` で頂点参照、.frag は `(x+0.5)/CELL` の 0.5px オフセット）。`sim_cataract_glsl`（.frag の 32bit ハッシュを忠実ミラー）で比較し **PSNR 19.6dB（<30、乖離をテストで固定）**。#99 と同様に 32bit spatial hash へ統一すれば等価化できるが、その際は**ハッシュだけでなく格子座標規約（±0.5px）も統一対象**にする必要がある。本 Issue では調査記録に留め別 Issue 化を推奨（昇格用の assert を `finding_cataract_noise_hash_diverges` に明記。→ #125 で解消済み、`shader_equiv_cataract_noise_hash_*` に昇格）。
  - **glaucoma 弧状暗点（GLSL 移植漏れ・別 Issue 必須）**: `glaucoma.frag` は **Vignette モードしか実装していない**。極座標 Bjerrum 弧状暗点（`ArcuateSuperior`/`ArcuateInferior`/`Biarcuate`）のマスクも、モード選択用 uniform も `.frag` に一切存在しない。CPU 弧状モードの等価テストは作れないため、非クラッシュ・上下マスク非対称・strength=0 恒等のみ検証。**提案: glaucoma.frag に極座標弧状暗点モードを追加する別 Issue を起票**（→ #123 で解消済み）。
  - テスト総数: shader_equivalence は #100 時点で 118 件 pass（#100 で +24 件）。`.frag` の修正は #100 では行わず、各乖離を別 Issue 化（#123-#127）。以降の Issue 解消で総数はさらに増える。

### Fixed

- **fix: kako-jun/sensus#126 `nystagmus` / `astigmatism` のブラーカーネルを CPU↔GLSL で統一**:
  - CPU `vision::astigmatism` / `vision::nystagmus` は `ellipse_blur`（長軸 a=radius、短軸 b=0.5px の filled-ellipse box、整数格子点を一様平均・edge replication）だったが、`astigmatism.frag` / `nystagmus.frag` は `±radius_px` を 16-tap で直線サンプリングする別カーネルで、鋭エッジで ~20dB 乖離していた（#100 で調査記録）。astigmatism は #18 の差別化点（軸付き乱視）として謳いながら、既存等価テストが radius<0.5px の passthrough 領域（32px 画像で radius=0.35px）しか検証しておらず実ブラー領域の乖離が隠れていた。
  - **方針 (a)（CPU 正本・`.frag` を寄せる）を採用**: 直線 16-tap を廃止し、`.frag` を CPU `build_ellipse_spans` / `ellipse_blur` と**同一の整数格子点列挙**に書き換えた。各オフセット `(dx, dy)` を回転座標 `(u, v)` に写し `u²/a² + v²/b² ≤ 1` の格子点だけを一様加算（短軸 `b = MIN_BLUR_RADIUS_PX = 0.5px`）、端は clamp-to-edge（CPU edge replication と一致）。`texture()` は texel 中心 nearest fetch で CPU の整数ピクセル参照と一致する。lattice 近似（方針 b）ではなく整数格子点の完全ミラーを選んだ理由は、薄楕円カーネルは格子点数が少なく（realistic 半径で片側 ≤ 12px）2D 窓走査でも単一パスに収まり、**近似ではなくバイト完全一致**にできるため。
  - **軸方向の一致**: astigmatism は `axis_deg`（シャープ方向）+90° をぼかし方向に使う規約が CPU・`astigmatism_uniforms`・`.frag` で共通。nystagmus は揺れ方向をそのままぼかし方向に使う（+90° しない）。`.frag` の回転行列 `(cos, sin; -sin, cos)` は CPU `build_ellipse_spans` と同符号で、軸 0/45/90 でブラー方向が CPU↔GLSL で一致することをテストで確認（軸 0↔90 のクロス PSNR < 軸対応 PSNR）。
  - **半径上限**: 回転した薄楕円の dy 範囲は軸 90° 付近で最大 ±ceil(a) になるため `.frag` は 2D 窓 `[-RMAX, RMAX]²`（RMAX=15）を走査。`min(W,H) ≲ 1363` まで `ceil(radius) ≤ RMAX` を満たし CPU と bit 等価。これを超える巨大半径（amplitude 大の nystagmus 等）では窓が飽和し近似となる（#97 disk-blur と同じ単一パス制約の扱い、≥30dB 目安）。
  - **CPU は無変更**（正本維持）。`ellipse_blur` の数式・出力は不変で、既存 CPU テスト（`astigmatism_axes_are_orthogonal_byte_exact` 等のバイト固定含む）は全て pass。
  - **一致根拠**: 実 `.frag` を 1:1 ミラーする `sim_astigmatism`（直線 16-tap を整数格子点列挙に書き換え、別アルゴリズムのインライン化なし）で CPU↔`.frag`↔sim の三者を PSNR 検証。256px 鋭エッジ画像で astigmatism 軸 0/45/90、64px 鋭エッジ画像で nystagmus 方向 0/45/90 すべて **PSNR = ∞（バイト完全一致）**。整数格子点・nearest fetch・linear sRGB が完全一致するため残る乖離源なし（realistic 半径領域）。
  - **実ブラー領域の等価テストを追加**（既存は passthrough のみ）: `shader_equiv_astigmatism_real_blur_sharp_edge_axes`（256px・radius 2.82px・軸 0/45/90・PSNR ≥ 45dB + 効果アサート）、`shader_equiv_astigmatism_axis_direction_matches_cpu_glsl`（軸方向の CPU↔GLSL 一致根拠）、`shader_equiv_nystagmus_real_blur_sharp_edge_axes`（64px・radius 7.68px・方向 0/45/90）。既存 passthrough 等価テスト（radius<0.5 で identity）は無変更で pass。
- **fix: kako-jun/sensus#124 `starbursts.frag` に放射状擬似レイ（gather 型）を実装し CPU レイマーチングと等価化**:
  - `starbursts.frag` が各画素を「自身の輝度」でその場ブライトニングするだけで、CPU `vision::starbursts` の放射状の光条（明部起点のレイマーチング）を一切再現していなかった問題を解消（#100 で「根本的に別効果」と調査記録、`.frag` コメント自身も「フルレイマーチング版は CPU 実装を参照」と明記していた）。
  - **scatter → gather 転置で単一パス化**: CPU は明部画素を起点に num_rays 本のレイを放射して別レイヤーに additive 合成する scatter（散乱）型。GPU の単一パスでは scatter を直接書けないため、その厳密な転置である gather（収集）型に `.frag` を書き換えた。出力画素に寄与しうる明部は各レイ方向 θ_i の逆方向（θ_i+180°）に距離 t だけ離れた位置にあるので、出力画素から各レイの逆方向へ t=1..ray_length_px だけ遡って明部を探し、CPU と同一の重み `src_intensity * (1 - t/L) * strength * rayColor` を加算する。`sx = px - round(t·cosθ)` / `sy = py - round(t·sinθ)` は scatter の dest 計算 `dest = source + round(t·cosθ)` の逆で、両者が訪れる (source, t, ray) タプル集合は完全一致する。
  - **uniform を追加**: `uNumRays`（CPU `num_rays`）・`uRayLengthPx`（CPU `ray_length_px` = `(ray_length_ratio.clamp(0,1) * min(W,H)) as u32`）・`uTexelSize`（逆方向サンプリングの texCoord 変換用）。`StarburstsUniforms` と `starbursts_uniforms()` に対応フィールド/引数を追加（uniform は camelCase、float seed なし）。`uDispersion` はレイ方向角の色相（HSL 虹色）で CPU と同一。
  - **Rust `f32::round`（0 から離れる半丸め）に座標 round を bit 一致させた**: `.frag` の `roundHalfAwayFromZero` を `floor(abs(x)+0.5)` 方式から trunc/fract ベースに修正。旧 `floor(abs(x)+0.5)` 方式は `abs(x)+0.5` の加算で精度を失い、`x` が 0.5 の直下（`0.49999997`）のとき和が f32 丸めで `1.0` に切り上がって `floor=1` を返し、Rust `f32::round(0.49999997)=0` と 1px 乖離していた（dispersion=1 で孤立画素に 131/255 の誤差を出し、40dB しきい値の rainbow テストがこの bug を覆い隠していた）。trunc ベース `f = x - trunc(x)`（Sterbenz により厳密）+ `|f| >= 0.5` 判定に直し、全 f32 値で `f32::round` と bit 一致することを網羅検証済み（0.49999997→0, 0.5→1, -0.5→-1, 2.5→3）。整数 uniform 丸め（`uNumRays`/`uRayLengthPx`）も同関数に統一。`.frag` の precision は `mediump` → `highp`（レイ累積の精度確保）。
  - **CPU 変更なし**: gather は scatter の数学的転置なので CPU 側の定式変更は不要（dispersion 色・距離減衰・閾値・additive 合成いずれも CPU の式をそのまま gather で評価）。座標 round が CPU・`.frag`・sim とも `f32::round` と bit 一致するため訪れる画素集合は完全に同一。残る唯一の乖離源は dispersion=0 等で複数寄与が重なるときの additive 合成の加算順序（scatter=source 走査順、gather=ray→t 順）に由来する f32 丸めのみ。
  - **一致根拠**: 実 `.frag` の gather ループを 1:1 ミラーする `sim_starbursts_glsl`（別アルゴリズムのインライン化なし）で CPU↔`.frag`↔sim を検証。**dispersion=1（虹色）は各 dest 画素への寄与が高々 1 本のため bit 完全一致（PSNR=∞、diff 画素 0）**で、rainbow テストを PSNR≥40 から bit 一致アサートに昇格。dispersion=0（白）・複数明部・中間 strength・非正方形 64×32 は加算順序由来の丸めのみが乖離源で **PSNR ≥ 40dB**。
  - #100 で「別効果なので等価不成立」としていた starbursts テスト群を昇格: strength=0 恒等・決定論・レイ放射効果を CPU/GLSL 両方で検証し、`shader_equiv_starbursts_white`/`_rainbow`/`_multi_source`/`_non_square` の等価テストを追加。
- **fix: kako-jun/sensus#127 `vertigo`/`bppv_rotation` の非正方形回転と `vertigo` 周辺 disk blur を GLSL に実装**:
  - `vertigo.frag`/`bppv_rotation.frag` が回転を UV 空間（0..1）で直接行っていたため、**非正方形画像で角度が歪み**（剪断変形）CPU（ピクセル空間回転）と不一致だった問題を解消。正方形では aspect=1 のため一致していたが、非正方形では回転が楕円状にせん断していた。
  - **aspect 補正回転を `.frag` に実装**: 中心化 UV を `uv.x *= uAspect`（aspect = width/height）でピクセル比例空間に写してから逆回転し、`uv.x /= uAspect` で UV へ戻す。これにより非正方形でも CPU のピクセル空間回転と一致する。両 `.frag` に `uniform float uAspect;` を追加。
  - **`vertigo.frag` に disk blur 段を追加**: CPU `vision::vertigo` は回転後に等方 disk blur（半径 `strength*0.015*min(W,H)`）を画像全体へ適用するが `.frag` には blur 段が無かった（32px 正方形では半径 0.48px < 0.5px で blur がスキップされる領域でしか等価が取れていなかった）。#97/#98 と同じ Fibonacci lattice 16tap 近似で linear sRGB 空間に実装。CPU は「回転後の像」を blur するため、出力（回転後）空間の tap オフセットを逆回転して元 UV へ写してからサンプリングする。`vertigo.frag` に `uniform float uRadiusPx;` / `uniform vec2 uTexelSize;` を追加。
  - **`bppv_rotation` は回転のみ**（CPU/.frag とも blur 無し）であることを確認。blur uniform は追加せず aspect 補正のみ。
  - **API 変更**: `vertigo_uniforms(strength, time)` → `vertigo_uniforms(strength, time, width, height)`（aspect・radius_px・texel_size を算出）、`bppv_rotation_uniforms(strength, time)` → `bppv_rotation_uniforms(strength, time, width, height)`（aspect を算出）。外部 Rust 呼び出し元は無し（grep 確認済み）。
  - **CPU は無変更**（正本維持）。`.frag` を CPU に寄せたため CPU 側の数式・出力は不変（既存 CPU テストも変更なし）。
  - **一致根拠**: 実 `.frag` を 1:1 ミラーする `sim_vertigo_glsl`（aspect 補正回転 + 16tap disk blur）/ `sim_uv_rotation_glsl`（aspect 補正回転）で CPU↔`.frag`↔sim を PSNR 検証（別アルゴリズムのインライン化なし）。実測 PSNR は vertigo 32×32 ≈ 49.9 dB / 64×32 ≈ 37.2 dB / 32×64 ≈ 34.0 dB、回転+blur の 128×96 ≈ 38.4 dB、bppv 64×32 ≈ 45.1 dB / 32×64 ≈ 41.5 dB（おおむね 34〜50 dB）で、すべて **PSNR ≥ 30 dB**。残る乖離源は pixel 座標 vs pixel-center UV の約 0.5px bilinear サンプリング差と 16tap disk 近似のみ。既存の正方形テスト・strength=0 identity は不変で pass。
- **fix: kako-jun/sensus#125 `cataract` 白濁ノイズを 32bit spatial hash で CPU↔GLSL 統一**:
  - 黄変マトリクス（Pokorny 1987）は CPU↔GLSL 一致していたが、白濁ノイズが乖離していた（#100 で調査記録、PSNR 19.6dB）。要因は2つ: ① CPU は 64bit Knuth LCG の高位ビット抽出（`(lcg>>32)/u32::MAX`）、`.frag` は同定数の下位 32bit 切り詰めで頂点ノイズ系列が別物。② 格子サンプリング規約のズレ（CPU は整数ピクセル index `px/CELL` で頂点参照、`.frag` は `(x+0.5)/CELL` の 0.5px オフセット）。これを #99（metamorphopsia / dry_eye）で確立した手法に揃えて解消した。
  - **ノイズハッシュを統一**: CPU・GLSL とも頂点ノイズを #99 と同一の 32bit 整数 spatial hash（黄金比定数 `0x9e3779b9`/`0x85ebca6b`/`0xc2b2ae35` 混合 + XOR-shift finalizer）に置き換え。各頂点の値は seed と頂点座標 (gx, gy) だけの決定論的関数で逐次状態を持たない（GPU 単一パスで再現可能）。`uSeed` は `uint` のまま整数演算（float 化しない）。
  - **格子座標規約を統一**: 整数ピクセル座標 (top-left) 基準 `floor(px/CELL)` に揃えた。`.frag` は `vTexCoord * uResolution - 0.5` でフラグメント中心 uv から整数ピクセル座標を復元（旧 `(x+0.5)/CELL` の 0.5px オフセットを廃止）。CELL_SIZE=32px・smoothstep bilinear 補間の幾何は不変。
  - **CPU 変更**: 白濁ノイズ生成を 64bit LCG（高位ビット抽出を含み GPU の `uint` で再現不可）+ 事前計算した格子頂点配列から、頂点座標だけの 32bit spatial hash に変更。これにより**白濁ノイズパターンが変わる**（strength=0 の identity・黄変方向・寸法・alpha 保持・strength=1 no-crash は不変。既存 CPU テストはバイト固定しておらず全て pass）。黄変マトリクスは無変更。全処理 linear sRGB。
  - **一致根拠**: 実 `.frag` を 1:1 ミラーする `sim_cataract_glsl`（別アルゴリズムのインライン化なし）で CPU↔`.frag`↔sim の三者を PSNR 検証。strength 0.5/1.0・非正方形 64×32/32×64 すべて **PSNR = ∞（バイト完全一致）**。整数ハッシュ・補間・linear sRGB が完全一致するため残る乖離源なし。
  - #100 で乖離を固定していた `finding_cataract_noise_hash_diverges` を等価検証テスト `shader_equiv_cataract_noise_hash_strength_1_0` に昇格。strength=0.5・非正方形・seed 差分の等価テストも追加。
  - **`.frag` の precision を `mediump` → `highp`（int も `highp`）に変更**: 32bit 整数ハッシュは `highp` でないと mod 2^32 の wrap が保証されない（metamorphopsia/dry_eye と同じ）。
- **fix: kako-jun/sensus#123 `glaucoma.frag` に弧状暗点（Arcuate）モードを実装**:
  - `glaucoma.frag` が Vignette モード（中心保存 + 周辺 smoothstep 暗化）しか実装しておらず、CPU `vision::glaucoma` の `GlaucomaMode::ArcuateSuperior`/`ArcuateInferior`/`Biarcuate`（極座標 Bjerrum 弧状暗点マスク）と、モード選択用 uniform が `.frag` に欠落していた問題を解消（移植漏れ。#100 で調査記録済み）。universal-experience の GLSL 表示で弧状モードを選んでも Vignette しか出ない無言フォールバックを修正
  - **モード選択 uniform を追加**: `uniform int uMode;`（0=Vignette / 1=ArcuateSuperior / 2=ArcuateInferior / 3=Biarcuate）。CPU `GlaucomaMode` の判別値と 1 対 1 対応（`GlaucomaMode::to_glsl_mode()` を新設してマッピングを正本化）
  - **極座標 Bjerrum 弧状マスクを `.frag` に実装**: CPU と同じく ON head（視神経乳頭）を画像中心から水平 +15%（=幅 0.65 の位置）に置き、ON head からの距離帯 `r_min = minDim*0.20`〜`r_max = minDim*0.55*√strength`、帯中央が最暗の radial fade（`1-|smoothstep*2-1|`）、ON head 近傍角度を弱める `√|sin θ|` の arc fade を strength 倍して暗化。CPU は pixel 座標、`.frag` は UV 座標だが、全項を画像幅 `w` で割っても比が保たれる（`atan2` の角度・距離比とも不変）ため `uAspect` だけで width 正規化座標に 1 対 1 変換できることを利用。linear sRGB 空間で処理
  - **API 変更**: `glaucoma_uniforms(strength, width, height)` → `glaucoma_uniforms(strength, width, height, mode)`、戻り値を共有 `FieldOfVisionUniforms`（tunnel_vision / macular_degeneration と共用）から専用 `GlaucomaUniforms`（`uMode` フィールド追加）に変更。`glaucoma_uniforms` / `glaucoma_glsl` の外部 Rust 呼び出し元は無し（grep 確認済み）。`Filter::Glaucoma { mode }`（CLI/pipeline）は既存どおり mode を保持しており波及なし
  - **一致根拠**: 実 `.frag` の `arcuateMul` を width 正規化座標で 1 対 1 にミラーする `sim_glaucoma_arcuate`（別アルゴリズムのインライン化なし）で CPU↔`.frag`↔sim の三者を PSNR 検証。strength 0.0/0.5/1.0・非正方形 64×32 の弧状 3 モードすべて **PSNR ≥ 30 dB**（strength=0.0 はバイト完全一致）。CPU・GLSL とも pixel 座標 vs pixel-center UV の約 0.5px サンプリング差のみが残る乖離源で、他フィルタ同様しきい値で吸収
  - **CPU は無変更**（正本維持）。`.frag` を CPU に合わせて実装したため CPU 側の数式・出力は不変（既存 CPU テストも変更なし）
  - 既存の弧状「決定論/効果のみ」テスト（#100 で移植漏れのため等価検証できず効果アサートに留めていたもの）を CPU↔GLSL 等価テストに昇格。上下非対称も sim 側で代表点（superior=上方暗化 / inferior=下方暗化）を検証。Vignette モードの既存等価テスト（strength 1.0/0.5・非正方形）は無変更で pass
- **fix: kako-jun/sensus#99 `metamorphopsia` / `dry_eye` のノイズモデルを CPU と統一**:
  - 両フィルタの `.frag` が CPU と別アルゴリズムのノイズモデルを使っており原理的に一致しなかった問題を解消。正本は CPU（医学的に正しく決定論的）。**option (a)** を採用し、両者を「GPU でも単一パスで bit 再現可能な 32bit 整数 spatial hash」に寄せて統一した。CPU 側の出力が変わるため CHANGELOG に明記する（下記）。
  - **metamorphopsia**: 旧 `.frag` は `uSeed` を即 float 化し `hash2`/`smoothNoise`（`sin` ベース）で変位場を作っており、CPU の LCG グリッド変位と無関係だった。CPU・GLSL ともに「グリッド頂点ごとの決定論的変位場 + 双線形補間」に統一。変位ハッシュ `gridHash(seed, gx, gy, axis)` を CPU の `grid_hash` と完全に同じ 32bit 整数演算（黄金比定数混合 + XOR-shift finalizer、`uint` は CPU `u32` と同じく mod 2^32 で wrap）にし、`uSeed` は `uint` のまま整数演算に通す（float 化しない）。グリッド頂点インデックスは `uTexelSize` から解像度を復元して CPU と同じ整数ピクセル座標基準で算出。**CPU 変更**: 変位生成を 64bit Knuth LCG（`(state>>32)` の高位ビット抽出を含み GPU の `uint` で再現不可）から 32bit 整数 spatial hash に変更（strength=0 の identity・寸法・alpha 保持・seed 差分は不変。既存 CPU テストはバイト固定しておらず全て pass）。
  - **dry_eye**: 旧 `.frag` は gamma sRGB サンプリング・画面 16 固定分割タイル・`hash()`・半径 `noise*s*2` で、CPU（linear sRGB・32px タイル・seed=42 LCG・半径 `*3`）と再現不可と宣言されていた。CPU・GLSL ともに「linear sRGB サンプリング・32px ピクセルタイル・seed=42 の 32bit spatial hash・半径 `noise*strength*3px`・等方 disk（pillbox）平均（メンバシップ `dx²+dy²≤r²`、edge clamp）」に統一。**CPU 変更**: タイルノイズを「行優先で走査しながら逐次更新する 64bit LCG 状態」（各タイルが先行タイル数 = グリッド寸法に依存し GPU の並列実行で再現不可）から、タイル座標だけの 32bit spatial hash に変更（strength=0 の identity は不変）。
  - **一致根拠**: 両フィルタとも CPU と `.frag` を 1:1 でミラーする sim（`sim_metamorphopsia_glsl` / `sim_dry_eye_glsl`、実 `.frag` と同式・別アルゴリズムのインライン化なし）で検証し、テストフィクスチャ上 **PSNR = ∞（バイト完全一致）**。整数ハッシュ・補間/disk・linear sRGB が完全一致するため、残る乖離源は f32 丸めのみ（フィクスチャ上は丸め後も同一バイト）。
  - **API 追加**: `dry_eye_uniforms(strength, width, height)` と `DryEyeUniforms`（`uTexelSize` 追加。タイル座標・disk 半径のテクスチャ座標変換に必要）。`MetamorphopsiaUniforms` は既存の `seed: u32` / `texel_size` をそのまま使用（struct 変更なし）。いずれも既存シグネチャ変更ではなく追加。
  - CPU↔GLSL 等価テストを追加（metamorphopsia: strength 0.0/0.5/1.0・非正方形 64×32、dry_eye: strength 0.0/0.5/1.0・非正方形 64×32、いずれも PSNR ≥ 30 dB 判定で実測 ∞）。
- **fix: kako-jun/sensus#98 `eye_strain` GLSL にブラー段を追加し等価テストの偽装を解消**:
  - `eye_strain.frag` を「contrast 圧縮 + vignette のみ」から、CPU `vision::eye_strain` と同じ処理順序（contrast → vignette → disk blur）に実装。CPU が最後に適用する半径 `strength*1.5px` の disk（pillbox）blur 段が `.frag` に欠落しており universal-experience の表示と CPU が乖離していた問題を解消
  - 単一パス制約のため厳密 pillbox を Fibonacci lattice 16 tap で近似（CPU=厳密 pillbox、これが唯一の乖離源）。各 tap で contrast+vignette を再計算してから円盤状に平均し、linear sRGB 空間で処理。PSNR で担保（32×32 で strength=0.5 → 40.0 dB、1.0 → 42.3 dB、いずれも下限 30 dB 超）
  - 等価テストの偽装を解消: `simulate_eye_strain_glsl` は実 `.frag` を読まずブラー段の無い別アルゴリズム（contrast+vignette のみ）をインライン再実装しており、`.frag` のブラー欠落がテストから見えなかった。`#97` の `sim_photophobia_glsl` と同じ「`.frag` と式を 1:1 対応」方針で書き換え、CPU・`.frag`・sim の3者が一致することを保証
  - CPU↔GLSL 等価テストを追加（strength=0.5 で `shader_equiv_eye_strain_strength_0_5_psnr`、既存の strength=1.0 テストも新 sim で検証）
  - **API 追加**: `eye_strain_uniforms(strength, width, height)` と `EyeStrainUniforms` 構造体新設（`uRadiusPx` / `uTexelSize` 追加。blur 半径はピクセル空間でテクスチャ座標に変換するため texel size が必要）。`eye_strain_glsl()` の外部呼び出し元なし（既存シグネチャ変更ではなく純粋な追加）
- **fix: kako-jun/sensus#97 `photophobia` GLSL に disk-blur bloom を実装**:
  - `photophobia.frag` を「ピクセル単位の輝度 boost のみ（近傍へ広がらない）」から、CPU `vision::photophobia` と同じ disk-blur bloom（highlight 抽出しきい値 0.5・BT.709 luma・半径 `strength*0.05*min(W,H)`・加算合成・linear sRGB）に実装。これで universal-experience の Flutter 表示で bloom が近傍へ滲み CPU と一致する
  - 単一パス制約のため厳密 pillbox を Fibonacci lattice 16 tap で近似（CPU=厳密 pillbox）。これが唯一の乖離源で PSNR で担保（strength=0.5 で 35.8 dB、1.0 で 42.7 dB、いずれも下限 30 dB 超）
  - **API 変更**: `photophobia_uniforms(strength)` → `photophobia_uniforms(strength, width, height)`、`PhotophobiaUniforms` 構造体新設（`uRadiusPx` / `uTexelSize` 追加。bloom 半径はピクセル空間計算のため画像サイズが必要）。外部呼び出し元なし
  - CPU↔GLSL 等価テストを追加（strength 0.0/0.5/1.0、radius<0.5 境界、128×128 大画像、非正方形、明点 bloom 拡散の効果アサート）。`.frag` を忠実にミラーする `sim_photophobia_glsl` で検証（乖離隠蔽なし）
- **fix: kako-jun/sensus#96 `detail_loss` の apply 経路を等価テスト対象に統一**:
  - `apply(Filter::DetailLoss)` / `pipeline` が呼ぶ `detail_loss_with_cell_size` を、タイル内全ピクセル linear sRGB 平均からタイル中心点サンプリング（pixelation）に変更。これで GLSL シェーダ（`detail_loss.frag`、universal-experience の表示経路 = 正本）・等価テスト済みの `detail_loss`・公開 API（apply 経由）の3者が同一アルゴリズムになった。`detail_loss_with_cell_size` と `detail_loss` の違いはタイルサイズの決め方（`cell_size` 直接指定 vs `strength` 導出）だけ
  - apply 経路の関数（`detail_loss_with_cell_size`）に対する CPU↔GLSL 等価テストを追加: `shader_equiv_apply_detail_loss_cpu_gpu_psnr`（cell_size=7、半端境界、PSNR ≥ 60 dB）、`shader_equiv_apply_detail_loss_cell_size_20_psnr`（cell_size=20、PSNR ≥ 60 dB）。中心点サンプリング用シミュレータ `sim_detail_loss_shader_cell` も追加
  - `detail_loss.frag` / `detail_loss` / `detail_loss_with_cell_size` の docコメントを統一後の事実に更新

## [0.4.0] - 2026-05-25

### Fixed

- **fix: review 指摘全件修正 round5（should×2 / nit×4）**:
  - [S-1] `eye_strain.frag` / `dry_eye.frag` の uniform・input・output 命名を camelCase に統一（`u_image`→`uTexture`, `u_strength`→`uStrength`, `v_texcoord`→`vTexCoord`, `out_color`→`fragColor`）。関数名も `srgbToLinear`/`linearToSrgb` に統一
  - [S-2] `apply(Filter::DetailLoss)` 経由のテスト追加（`cell_size=1` で identity、`cell_size=20` で変換確認）
  - [S-3] `cataract` strength=1.0 のクラッシュ/動作確認テスト追加
  - [N-1] `detail_loss.frag` の設計コメントを過去の経緯説明から仕様説明に清書き
  - [N-2] `detail_loss_with_cell_size` の `_strength` 引数に `#[allow(unused_variables)]` 追加
  - [N-3] `floaters.frag` に sRGB/linear 差異（GPU 版は sRGB 空間でブレンド、CPU は linear sRGB）のコメントを追記


  - [M-1] `cataract_uniforms` に `seed: u32` フィールドを追加。`CataractUniforms` 構造体を新設し、`cataract_uniforms(strength, seed: u64)` シグネチャに変更。これにより `cataract.frag` の `uniform uint uSeed` に正しく seed を渡せるようになった
  - [M-2] `cataract.frag` の LCG 定数を CPU 実装（Knuth 64bit LCG）に統一。旧: Numerical Recipes 定数（`* 1664525u + 1013904223u`）→ 新: Knuth 定数の下位 32bit（`* 0x4c957f2du + 0xf767814fu`）。`shader_equiv_cataract_strength_zero_psnr` テスト追加
  - [S-1] `vestibular_neuritis_uniforms` 付近（`shaders.rs` の `VestibularNeuritisUniforms` 構造体と `vision.rs` の `vestibular_neuritis` 関数）に CPU/GLSL シフト定義の対応関係コメントを追加
  - [S-2] `HemianopiaUniforms.side` フィールドと `hemianopia_uniforms` 関数のコメントを統一。「1.0=右欠損/-1.0=左欠損（GLSL 内部値）」と「公開 API との規約差」を明記
  - [S-3] `MetamorphopsiaUniforms.seed` を `f32` → `u32` に変更。`metamorphopsia_uniforms` の `seed as f32` → `seed as u32` に修正。`metamorphopsia.frag` の `uniform float uSeed` → `uniform uint uSeed` に変更し、`float(uSeed)` で float 変換
  - [S-4] `dry_eye` の docコメントに「シード値は内部で固定（42）のため同一入力に対して毎回同一パターン」「フレームごとに変えたい場合は将来の `dry_eye_with_seed` を使用（未実装）」を追記
  - [S-5] `detail_loss` の docコメントに「タイル中心点参照（GLSL と同一）、`apply(Filter::DetailLoss)` 経由時は `detail_loss_with_cell_size` を呼ぶ」を明記。`detail_loss_with_cell_size` に「タイル内全ピクセル linear sRGB 平均、GLSL と異なるが視覚的に高品質」を明記
  - [N-1] `starbursts` の docコメントと `hsl_rainbow_to_linear` のコメントが混在していたのを分離。`hsl_rainbow_to_linear` の直前に独立した `///` コメントブロックを配置
  - [N-2] `teichopsia.frag` の aspect 補正コメントを更新。「`uy / aspect` は UV 空間ではなくピクセル空間で円形になるよう補正する」旨を明確化
  - [N-3] `shader_equivalence.rs` に 64×32 非正方形テストを追加: `shader_equiv_teichopsia_non_square_psnr`（PSNR ≥ 25 dB）、`shader_equiv_macular_degeneration_non_square_psnr`（PSNR ≥ 30 dB）、`shader_equiv_tunnel_vision_non_square_psnr`（PSNR ≥ 30 dB）。aspect 補正付き `sim_macular_degeneration_aspect` も追加
  - [N-4] `contrast_sensitivity` の docコメントに「midpoint は linear sRGB 空間で 0.5（知覚的中間輝度 ≈ 0.214 とは異なる数学的中間点の簡易近似）」を追記


  - [M-1] `teichopsia.frag` の aspect 計算を CPU 実装と一致させる: `uv.x * uAspect` 方式から `uv.y / uAspect` 方式に変更。`shader_equiv_teichopsia_strength_05_psnr`（PSNR ≥ 25 dB）テスト追加
  - [M-2] `detail_loss.frag` を9点サンプルから中心1点サンプリング（pixelation）に変更。CPU（`vision.rs`）も同様にタイル全平均から中心点参照に変更し、CPU/GPU を完全統一。`shader_equiv_detail_loss_strength_1_psnr`（PSNR ≥ 30 dB）テスト追加
  - [S-1] M-1 修正後の teichopsia CPU/GLSL 等価 PSNR テストを追加（上記 M-1 に含む）
  - [S-2] `flickering_stars.frag` の `uSeed * 1000u` にラップアラウンドが意図的動作であることを示すコメントを追加
  - [S-3] `vision.rs` glaucoma 弧状モードの `t_r` 計算付近に `r_max ≈ r_min` 時の NaN 非発生を説明するコメントを追加
  - [N-1] `docs/overview.md` の cataract 記述を現行実装（Pokorny/van Norren 黄変行列・32×32 bilinear 補間ノイズ）に更新
  - [N-2] `lib.rs` の `Filter::Floaters.size` フィールドに「将来の blob_radius_ratio に使用予定、現在は無視」コメントを追加
  - [N-3] `pipeline.rs` の `audio_pipeline_two_steps_returns_ok` テストを分割し、silence + HearingLoss → silent 確認テスト（`audio_pipeline_hearing_loss_on_silence_stays_silent`）と非ゼロバッファ減衰確認テスト（`audio_pipeline_hearing_loss_changes_nonzero_buffer`）を追加


  - [S-1] `sim_vignette_fov` に `aspect: f32` 引数を追加してシェーダ（`uAspect`）と一致させる。非正方形（64×32）テスト `shader_equiv_glaucoma_non_square_64x32_psnr` を追加（PSNR ≥ 30 dB）
  - [S-2] `floaters.frag` の `uniform float uSeed` → `uniform uint uSeed` に変更（24bit 精度劣化防止）。`FloatersUniforms.seed: f32 → u32`、`floaters_uniforms` を `seed as u32` に修正
  - [S-3] `nyctalopia.frag` の命名を確認 — `uTexture`, `uStrength`, `vTexCoord`, `fragColor`, `srgbToLinear`, `linearToSrgb` が他シェーダと一致しており修正不要
  - [S-4] `cataract.frag` の `uniform float uSeed` → `uniform uint uSeed` に変更（同 S-2 と同じ精度劣化防止）
  - [N-1] `bppv_rotation.frag` の `clamp` 処理にコメント「範囲外の UV は端ピクセルにクランプする（CPU 実装と同じ動作）」を追記
  - [N-2] `shader_equivalence.rs` に `shader_photophobia_glsl_is_not_empty` テストを追加
  - [N-3] `starbursts.frag` の `sector < 1.0` 分岐付近にコメント「H=360° は H=0°（赤）と同値になる（HSL の周期性）」を追記

- **fix: レビュー指摘全件修正（M×3/S×5/N×2）**:
  `crates/core/src/pipeline.rs` に聴覚フィルタ多段合成用の `AudioPipeline` と `AudioFilterStep` を追加。
  `Pipeline`（視覚）と同じ builder パターンで `push(filter, strength).apply(&buf)` が使えるようになった。
  `lib.rs` に `pub use pipeline::{AudioPipeline, AudioFilterStep};` を追加し外部公開。

- **vision: starbursts に波長分散（虹色光芒）オプション追加** (#67):
  `starbursts()` シグネチャに `dispersion: f32` パラメータを追加。
  `dispersion=0.0`（デフォルト）は既存の白い光芒と後方互換。
  `dispersion=1.0` では各 ray の角度を色相に対応した HSL 虹色（S=1, L=0.5）で着色し additive blend する。
  `pipeline.rs` の `FilterStep` に `dispersion` フィールドを追加（デフォルト: 0.0）。
  `shaders.rs` の `StarburstsUniforms` に `dispersion` フィールドを追加し `starbursts_uniforms()` の引数を更新。
  `starbursts.frag` に `uDispersion` uniform 追加し UV 角度ベースの虹色近似を実装。
  テスト: `dispersion=0.0` → 既存テスト通過、`dispersion=1.0` → 非グレー（虹色）ピクセル生成確認。

### Breaking Changes

- **BREAKING: v0.4.0: Filter enum にパラメータ埋め込み（案 A）** (#65):
  以下のバリアントがパラメータを直接 enum に埋め込む形式に変更された（HearingFilter と同じパターン）。
  `#[derive(PartialEq, Eq)]` は `f32` フィールドを持つバリアントのため `PartialEq` のみに変更。
  - `Astigmatism` → `Astigmatism { axis_deg: f32 }`（シャープ方向の経線角。旧 `apply()` の 90° デフォルト相当）
  - `Glaucoma` → `Glaucoma { mode: GlaucomaMode }`（暗点モード指定。旧デフォルト: Vignette）
  - `Hemianopia` → `Hemianopia { side: f32 }`（0.0=左欠損, 1.0=右欠損。旧デフォルト: 0.0）
  - `Floaters` → `Floaters { seed: u64, density: f32, size: f32 }`（seed/密度/サイズ。旧デフォルト: 0/0.5/1.0）
  - `Starbursts` → `Starbursts { num_rays: u32, ray_length_ratio: f32, threshold: f32, dispersion: f32 }`
  - `DetailLoss` → `DetailLoss { cell_size: u32 }`（タイルサイズ直接指定）
  - `FlickeringStars` → `FlickeringStars { seed: u64 }`（ランダムシード）
  `apply()` は各バリアントのパラメータを使って対応関数を呼ぶよう更新。
  CLI (`main.rs`) の `to_core()` は旧来のデフォルト値でパラメータ付きバリアントに変換。
  `vision::detail_loss_with_cell_size()` を新規追加（`cell_size` 直接指定版）。
  `pipeline.rs` の `FilterStep::apply()` を更新し、パラメータ付きバリアントから直接値を取得するよう変更。


- **vision: starbursts に波長分散（虹色光芒）オプション追加** (#67):
  `starbursts()` シグネチャに `dispersion: f32` パラメータを追加。
  `dispersion=0.0`（デフォルト）は既存の白い光芒と後方互換。
  `dispersion=1.0` では各 ray の角度を色相に対応した HSL 虹色（S=1, L=0.5）で着色し additive blend する。
  `pipeline.rs` の `FilterStep` に `dispersion` フィールドを追加（デフォルト: 0.0）。
  `shaders.rs` の `StarburstsUniforms` に `dispersion` フィールドを追加し `starbursts_uniforms()` の引数を更新。
  `starbursts.frag` に `uDispersion` uniform を追加し UV 角度ベースの虹色近似を実装。
  テスト: `dispersion=0.0` → 既存テスト通過、`dispersion=1.0` → 非グレー（虹色）ピクセル生成確認。

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
  `AudioBuffer` (f32 interleaved PCM), `BiquadFilter`, and 11 pure-function
  hearing filters — `hearing_loss`, `sudden_hearing_loss`,
  `noise_induced_hearing_loss`, `tinnitus`, `hyperacusis`, `paracusis`,
  `amusia`, `dysmelodia`, `pitch_shift_semitones`, `diplacusis`,
  `auditory_processing_disorder`. Three vestibular-visual filters
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

[Unreleased]: https://github.com/kako-jun/sensus/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/kako-jun/sensus/releases/tag/v0.5.0
[0.4.0]: https://github.com/kako-jun/sensus/releases/tag/v0.4.0
[0.3.0]: https://github.com/kako-jun/sensus/releases/tag/v0.3.0
[0.2.0]: https://github.com/kako-jun/sensus/releases/tag/v0.2.0
[0.1.0]: https://github.com/kako-jun/sensus/releases/tag/v0.1.0
