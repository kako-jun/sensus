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
