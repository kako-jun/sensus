# roadmap

sensus の段階的実装計画。詳細な議論は GitHub Issue で行い、ここでは状態とフェーズだけ反映する。

## フェーズ別進捗

| Phase | 範囲 | 状態 | 関連 Issue |
|---|---|---|---|
| 0 | リポジトリ scaffold（workspace / CI / release / docs） | ⏳ 進行中 | #1 |
| 1 | vision: 色覚特性（protanopia / deuteranopia / tritanopia / achromatopsia） | 未着手 | #2 |
| 1+ | vision: 四色型色覚（tetrachromacy） | 未着手 | #3 |
| 2 | vision: 焦点・屈折（近眼 / 遠眼 / 乱視 / 老眼） | 未着手 | #4 |
| 3 | vision: 視野異常（緑内障 / 黄斑変性 / 半盲 / 視野狭窄） | 未着手 | #5 |
| 3 | vision: 光・透明度（白内障 / 飛蚊症 / 光過敏 / 夜盲） | 未着手 | #6 |
| 4 | hearing: 聴力・音量 | 未着手 | #7 |
| 4 | hearing: 音質・音程 | 未着手 | #8 |
| 4 | hearing: 平衡・めまい | 未着手 | #9 |
| 4 | pipeline: フィルタ組み合わせ | 未着手 | #10 |

## 完成条件（v0.1.0）

- Phase 1（色覚特性 4 種）の実装と CLI 経由での動作確認
- README / overview に動く例を載せられる状態
- crates.io 公開（#12）

## 公開準備

- [x] GitHub Releases workflow（tag `v*` で Linux / macOS / Windows artifact を生成）— #1
- [x] CHANGELOG.md 作成 — #1
- [ ] universal-experience 接続方針メモ — #11
- [ ] `cargo publish`（crates.io v0.1.0）— #12

## 関連リポジトリ

- [universal-experience](https://github.com/kako-jun/universal-experience) — sensus を内蔵する Flutter アプリ。GUI とプリセット集を担当する
- [orber](https://github.com/kako-jun/orber) — 同じ `image::DynamicImage` 入出力規約。ワークスペース構成のテンプレート元

## メモ

- WebAssembly は対象外（universal-experience は native アプリのため）
- 味覚・嗅覚・触覚はスコープ外（汎用デジタル出力経路がない）
- 各フィルタには「こうなったらすぐ病院へ」の医学的注記をドキュメントに付ける（緊急度: 即救急 / 即受診 / 早期受診）
