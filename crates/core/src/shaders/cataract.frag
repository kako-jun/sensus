#version 300 es
precision mediump float;

// 白内障（Cataract）シミュレーション。
// 黄変マトリクス + Simplex-like LCG ノイズ（空間相関あり）による白濁。
// CPU 実装 vision::cataract に対応。
//
// ### 黄変マトリクス（linear sRGB → yellowed linear sRGB）
// 係数出典: Pokorny et al. (1987) "Aging of the human lens" Applied Optics 26(8)
//          および van Norren & Vos (1974) "Spectral transmission of the human ocular
//          media" Vision Research 14(11)
//
//   | yr |   | 1.00  0.05 -0.05 | | r |
//   | yg | = | 0.02  1.00 -0.02 | | g |
//   | yb |   | 0.00  0.00  0.85 | | b |
//
// ### 散乱ノイズ
// 格子頂点に LCG ノイズを割り当て、smoothstep bilinear 補間で空間相関を付与。
// CPU 実装の CELL_SIZE=32 に対応する格子周波数で uResolution から格子座標を計算する。

uniform sampler2D uTexture;
uniform float uStrength;
uniform uint uSeed;    // u64 シードの下位 32bit を uint として渡す（float 経由の精度損失を回避）
uniform vec2 uResolution;

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// LCG ハッシュ: 格子頂点 (gx, gy) の擬似ランダム値（0.0..=1.0）
float gridNoise(float gx, float gy) {
    // CPU 実装と同じハッシュ定数
    uint s = uint(uSeed);
    uint h = s * 0x9e3779b9u
        + uint(gx) * 0x517cc1b7u
        + uint(gy) * 0x6c62272eu;
    // LCG 1 ステップ
    uint lcg = h * 1664525u + 1013904223u;
    return float(lcg) / float(0xFFFFFFFFu);
}

// smoothstep bilinear 補間でグリッドノイズをサンプリング
float smoothNoise(vec2 pixelPos) {
    const float CELL_SIZE = 32.0;
    vec2 cell = pixelPos / CELL_SIZE;
    vec2 ci = floor(cell);
    vec2 ct = cell - ci;

    // smoothstep
    vec2 st = ct * ct * (3.0 - 2.0 * ct);

    float v00 = gridNoise(ci.x,       ci.y      );
    float v10 = gridNoise(ci.x + 1.0, ci.y      );
    float v01 = gridNoise(ci.x,       ci.y + 1.0);
    float v11 = gridNoise(ci.x + 1.0, ci.y + 1.0);

    return v00 * (1.0 - st.x) * (1.0 - st.y)
         + v10 * st.x         * (1.0 - st.y)
         + v01 * (1.0 - st.x) * st.y
         + v11 * st.x         * st.y;
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    float r = srgbToLinear(orig.r);
    float g = srgbToLinear(orig.g);
    float b = srgbToLinear(orig.b);

    // 黄変マトリクス適用（Pokorny 1987 / van Norren & Vos 1974）
    float yr = clamp(r * 1.00 + g * 0.05 + b * (-0.05), 0.0, 1.0);
    float yg = clamp(r * 0.02 + g * 1.00 + b * (-0.02), 0.0, 1.0);
    float yb = clamp(r * 0.00 + g * 0.00 + b * 0.85,    0.0, 1.0);

    // strength でブレンド: orig * (1-s) + yellowed * s
    float nr = r + (yr - r) * uStrength;
    float ng = g + (yg - g) * uStrength;
    float nb = b + (yb - b) * uStrength;

    // Simplex-like 格子補間ノイズによる白濁
    vec2 pixelPos = vTexCoord * uResolution;
    float noise = smoothNoise(pixelPos);
    const float WHITE_BLEND_MAX = 0.4;
    float whiteBlend = uStrength * noise * WHITE_BLEND_MAX;

    float fr = clamp(nr + (1.0 - nr) * whiteBlend, 0.0, 1.0);
    float fg = clamp(ng + (1.0 - ng) * whiteBlend, 0.0, 1.0);
    float fb = clamp(nb + (1.0 - nb) * whiteBlend, 0.0, 1.0);

    fragColor = vec4(
        linearToSrgb(fr),
        linearToSrgb(fg),
        linearToSrgb(fb),
        orig.a
    );
}
