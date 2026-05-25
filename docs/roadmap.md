# roadmap

sensus の段階的実装計画。詳細な議論は GitHub Issue で行い、ここでは状態とフェーズだけ反映する。

## フェーズ別進捗

| Phase | 範囲 | 状態 | 関連 Issue |
|---|---|---|---|
| 0 | リポジトリ scaffold（workspace / CI / release / docs） | ✅ 完了 | #1 |
| 1 | vision: 色覚特性（protanopia / deuteranopia / tritanopia / achromatopsia） | ✅ 完了 | #2 |
| 1+ | vision: 四色型色覚（tetrachromacy） | ✅ 完了 | #3 |
| 2 | vision: 焦点・屈折（近眼 / 遠眼 / 乱視 / 老眼） | ✅ 完了 | #4 |
| 3 | vision: 視野異常（緑内障 / 黄斑変性 / 半盲 / 視野狭窄） | ✅ 完了 | #5 |
| 3 | vision: 光・透明度（白内障 / 飛蚊症 / 光過敏 / 夜盲） | ✅ 完了 | #6 |
| 4 | hearing: 聴力・音量 | ✅ 完了 | #7 |
| 4 | hearing: 音質・音程 | ✅ 完了 | #8 |
| 4 | hearing: 平衡・めまい | ✅ 完了 | #9 |
| 4 | pipeline: フィルタ組み合わせ | ✅ 完了 | #10 |
| 5 | vision: GLSL ES 3.00 シェーダソース API（CPU 実装と正本一元化） | ✅ 完了 | #16 |
| 5 | test: CPU⇄GLSL シェーダ等価性回帰テスト | ✅ 完了 | #17 |
| 5 | research: VIP-Sim 調査（実装比較・差別化根拠） | ✅ 完了 | #18 |
| 5 | vision: depth-aware blur（深度マップ対応距離依存ぼけ） | ✅ 完了 | #19 |
| 5 | vision: diplopia / nystagmus / starbursts | ✅ 完了 | #29 |
| 6 | vision: MPO ステレオ写真 → 深度マップ自動生成（split_mpo / stereo_to_depth） | ✅ 完了 | #31 |
| 6 | vision: Android XMP Depth（JPEG + ポートレートモード深度マップ）対応 | ✅ 完了 | #32 |

## 未着手・ブロック中

| 状態 | 内容 | 関連 |
|---|---|---|
| メモ（アクション不要） | GUI設計メモ: universal-experience との接続方針 | #11 |
| ブロック中（Flutter 未インストール） | universal-experience Flutter FragmentProgram 連携 | ue#2〜#5 |

## 完成条件（v0.1.0）✅ 達成済み

- Phase 1（色覚特性 4 種）の実装と CLI 経由での動作確認
- README / overview に動く例を載せられる状態
- crates.io 公開（#12）

## 公開準備

- [x] GitHub Releases workflow（tag `v*` で Linux / macOS / Windows artifact を生成）— #1
- [x] CHANGELOG.md 作成 — #1
- [x] `cargo publish`（crates.io v0.1.0）— #12
- [ ] universal-experience 接続方針メモ — #11

## 関連リポジトリ

- [universal-experience](https://github.com/kako-jun/universal-experience) — sensus を内蔵する Flutter アプリ。GUI とプリセット集を担当する
- [orber](https://github.com/kako-jun/orber) — 同じ `image::DynamicImage` 入出力規約。ワークスペース構成のテンプレート元

## メモ

- WebAssembly は対象外（universal-experience は native アプリのため）
- 味覚・嗅覚・触覚はスコープ外（汎用デジタル出力経路がない）
- 各フィルタには「こうなったらすぐ病院へ」の医学的注記をドキュメントに付ける（緊急度: 即救急 / 即受診 / 早期受診）
