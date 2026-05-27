#version 300 es
precision mediump float;

// 飛蚊症（Floaters）シミュレーション。
// hash ベースの floater パターン（seed 依存の擬似ランダム blob）。
// CPU 実装 vision::floaters に対応。
//
// GPU 版は LCG ブロックノイズで floater マスクを生成し、
// 乗算ブレンドで暗いドットを重ねるシンプル実装。
// CPU の精密な blob/strand 描画とは異なり、近似的な見た目を提供する。
//
// 注意: GPU 版はブレンドを sRGB 空間で直接行う（linear 変換なし）。
// CPU 実装は linear sRGB 空間で乗算するため、厳密な色値は異なるが
// 視覚的な近似として許容している（「GPU 版は近似」はこの差異を指す）。
//
// #134（parity 未達・別追跡）: flickering_stars は 32bit spatial hash で CPU↔GLSL を
// 統一済みだが、floaters は (1) 乱歩ストランドが「データ依存の RNG ストリーム」を
// 逐次消費し、(2) 最終マスクに 3×3 box blur を掛けるため、単一パスでの忠実再現は
// 200 点ぶんのストランドを毎フラグメントで replay しつつ 9 近傍のマスクを再計算する
// 必要があり、実機（Flutter）描画として非現実的。本シェーダは意図的な近似のまま据え置く。
// 真の parity には「ストランド廃止の blob-only モデルに CPU/GLSL 双方を寄せる」か
// 「CPU 生成マスクをテクスチャとして渡す」かの設計判断が必要（freeza で起票・追跡）。

uniform sampler2D uTexture;
uniform float uStrength;
uniform uint uSeed;    // u64 シードの下位 32bit を uint として渡す（float 経由の精度損失を回避）

in vec2 vTexCoord;
out vec4 fragColor;

// 単純なハッシュ関数（floater パターン生成用）
float hash21(vec2 p) {
    p = fract(p * vec2(127.1, 311.7));
    p += dot(p, p + 19.19);
    return fract(p.x * p.y);
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);

    // ブロック単位でハッシュを計算（8x8 相当の粗さ）
    float seedF = float(uSeed);
    vec2 blockUV = floor(vTexCoord * 16.0 + seedF * 0.01) / 16.0;
    float noise = hash21(blockUV + seedF * 0.001);

    // noise が閾値を下回る領域を floater として暗化
    float floaterMask = 1.0;
    if (noise < 0.05 * uStrength) {
        floaterMask = 1.0 - uStrength * 0.7;
    }

    fragColor = vec4(
        orig.r * floaterMask,
        orig.g * floaterMask,
        orig.b * floaterMask,
        orig.a
    );
}
