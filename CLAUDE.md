# sensus - Sensory Perception Simulation

五感（主に視覚・聴覚）の特性シミュレーションを行う Rust crate。色覚特性、ぼやけ、視野欠損、聴覚異常などのフィルタを画像/音声に適用する。`image::DynamicImage` を入出力とする。universal-experience（Flutter）から内部ライブラリとして利用される。

## ビルド・テスト

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
cargo run -p sensus -- --help
```

## ドキュメント

| ファイル | 内容 | 言語 |
|---|---|---|
| `README.md` | エンドユーザー向けの使い方 | 英語（マスター） |
| `docs/overview.md` | 設計思想・I/O 規約・モジュール責務 | 英語 |
| `docs/roadmap.md` | フェーズ別進捗（内部運用メモ） | 日本語 |
| `CLAUDE.md` | AI 向け内部ドキュメント | 日本語 |
| `CHANGELOG.md` | リリースノート（Keep a Changelog 形式） | 英語 |

### 言語ルール

- README は英語マスターのみ
- `docs/overview.md` は英語
- `docs/roadmap.md` と `CLAUDE.md` は日本語（内部用）

## ソース構成

`v0.0.1` から Cargo workspace 構成。GUI / native アプリ（universal-experience）が純粋ロジックコアだけを依存できるよう、I/O は CLI 側に隔離する。WASM は対象外。

```
sensus/
├── Cargo.toml              # [workspace] members = ["crates/core", "crates/cli"]
└── crates/
    ├── core/               # sensus-core: 純粋ロジックコア（I/O 一切なし）
    │   ├── Cargo.toml      #   crate-type = ["rlib"]
    │   └── src/
    │       ├── lib.rs
    │       ├── vision.rs   # 色覚 / 屈折 / 視野 / 光・透明度
    │       ├── hearing.rs  # 聴力 / 音質 / 平衡
    │       └── pipeline.rs # フィルタ組み合わせ
    └── cli/                # sensus: CLI バイナリ（image::open / file write）
        ├── Cargo.toml      #   [[bin]] name = "sensus", path = "src/main.rs"
        └── src/
            └── main.rs     # clap derive で引数パース
```

`std::fs` / file I/O / `image::open` / `image::save` を使うのは `crates/cli/` だけ。`crates/core/` は I/O を持たない純粋関数の集合。

## 主要な設計判断

- **WASM ターゲットは持たない** — sensus の主クライアントは universal-experience（Flutter の native）。Web GUI はやらない方針なので、wasm32 用の getrandom 等の追加依存は避ける
- **入出力は `image::DynamicImage` で統一** — orber と同じ規約。動画はフレーム単位で同関数を呼ぶ
- **CLI は scaffold（#1）では未実装メッセージで `exit(2)`** — Phase 1（#2）で `--filter deuteranopia` から実装を埋める
- **色覚特性は linear sRGB + Machado 2009 severity=1.0 行列 + linear blend** — gamma 適用済み sRGB に直接行列を掛けない。中間 strength は linear 空間で補間する。`achromatopsia` だけは LMS 経路を捨てて BT.709 photopic luminance（NTSC 用 BT.601 ではない）でグレースケール化する
- **焦点・屈折 (Phase 2 / #4) は disk blur (pillbox) を linear sRGB で適用** — Gaussian は採用しない（defocus の点像は瞳孔 = 円の投影 = circle of confusion であり Gaussian ではない）。strength → 画素半径の換算前提は「Smith-Helmholtz `θ_diameter ≈ pupil(m) × |D|`、radius = θ/2」「視距離 50 cm / 画像 FOV 30° ≈ 0.5236 rad」「`min(W,H)` 比率で myopia 2.3% / hyperopia 1.5% / presbyopia 1.1% / astigmatism 1.1% (ボケ軸のみ)」。境界は edge replication、内側は per-row span + horizontal prefix sum で `O(W·H·kernel_height)` に抑える。astigmatism は **1D directional blur (純粋 cylinder lens の line spread function)** で実装 — `axis_deg` は **シャープ方向 (cylinder lens 軸)** を指す医学的慣習で、ボケ方向は `axis_deg + 90°`。短軸は `MIN_BLUR_RADIUS_PX = 0.5 px` で sub-pixel に縮退し、1 行の directional box filter として動作。`apply()` ファサードは既定軸 90°、軸を変えたい場合は `vision::astigmatism()` を直接呼ぶ。臨床合併乱視は #10 pipeline で myopia + astigmatism を合成する想定
- **フィルタは純粋関数** — 内部 RNG 状態を持たない。乱数が必要な場合は `seed` パラメータを明示的に受け取る（飛蚊症など）
- **`strength` は 0.0..=1.0 に正規化** — 0.0 = 元画像、1.0 = フル効果
- **clap derive を採用** — 引数定義はコード生成で簡潔に。`Filter` enum を `ValueEnum` で公開する
- **医学的注記をドキュメントに付ける** — 各フィルタに「こうなったらすぐ病院へ」を併記。エンタメ + 早期発見の二重価値（vision: 半盲突発 = 即救急、飛蚊症急増 = 即受診、緑内障 / 黄斑変性 = 早期受診）
- **味覚 / 嗅覚 / 触覚はスコープ外** — 汎用デジタル出力経路がないため。sensus は視覚 + 聴覚に絞る
- **release は GitHub Release のみ自動化** — crates.io publish はタグ駆動にせず `/publish` スキルから手動で発火する

## 技術ルール

- コミットメッセージは Conventional Commits、日本語
- Issue 番号を含める（例: `feat: kako-jun/sensus#1 Cargo workspace を scaffold`）
- コミットメッセージに Co-Authored-By を付けない
- `--no-verify` は使わない
- main への直接コミット禁止（feature ブランチ → PR）

## 関連プロジェクト

- [universal-experience](https://github.com/kako-jun/universal-experience) — sensus を内蔵する Flutter アプリ
- [orber](https://github.com/kako-jun/orber) — 同じ workspace 構成・同じ `image::DynamicImage` 規約のテンプレート元
