#version 300 es
precision highp float;
precision highp int;

// 白内障（Cataract）シミュレーション。
// 黄変マトリクス + 32bit spatial hash ノイズ（空間相関あり）による白濁。
// CPU 実装 vision::cataract と同一のノイズモデル（#125 で統一）。
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
// 格子頂点に 32bit 整数 spatial hash でノイズを割り当て、smoothstep bilinear
// 補間で空間相関を付与する。CELL_SIZE=32px の格子で、CPU の整数ピクセル座標
// 基準 (px/CELL の floor) に合わせるため、フラグメント中心 uv から
// `vTexCoord * uResolution - 0.5` で整数ピクセル座標を復元する。

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

// 格子頂点 (gx, gy) の擬似ランダム値（0.0..=1.0）。
// #125: CPU 実装 vision::cataract の grid_hash と完全に同じ 32bit 整数演算
// （= metamorphopsia/dry_eye と同一の系列。GLSL の uint は CPU の u32 と同じく
// mod 2^32 で wrap する）。黄金比定数混合 + XOR-shift finalizer。
float gridNoise(float gx, float gy) {
    uint h = uSeed * 0x9e3779b9u
        + uint(gx) * 0x85ebca6bu
        + uint(gy) * 0xc2b2ae35u;
    h ^= h >> 15;
    h *= 0x2c1b3c6du;
    h ^= h >> 12;
    h *= 0x297a2d39u;
    h ^= h >> 15;
    return float(h) / float(0xFFFFFFFFu); // 0.0..=1.0
}

// smoothstep bilinear 補間でグリッドノイズをサンプリング。
// pixelPos は整数ピクセル座標（CPU の x, y に対応）。
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

    // VIP-Sim 二段モデルの**再解釈**による輝度・コントラスト低下（#106）。
    // pivot 0.5 中心の per-channel コントラスト収縮（ContrastCoeff = 0.7, 0.7, 0.4）
    // + 輝度低下。CPU vision::cataract と同一演算。原典 VIP-Sim との差分（原典は
    // brightness ×(1-severity) の乗算・pivot=ContrastCoeff 自身）は CPU 側
    // `vision::cataract` の doc コメント参照（#170）。
    const float PIVOT = 0.5;
    const float BRIGHTNESS_DROP = 0.1;
    nr = clamp((nr - PIVOT) * (1.0 - uStrength * (1.0 - 0.7)) + PIVOT - uStrength * BRIGHTNESS_DROP, 0.0, 1.0);
    ng = clamp((ng - PIVOT) * (1.0 - uStrength * (1.0 - 0.7)) + PIVOT - uStrength * BRIGHTNESS_DROP, 0.0, 1.0);
    nb = clamp((nb - PIVOT) * (1.0 - uStrength * (1.0 - 0.4)) + PIVOT - uStrength * BRIGHTNESS_DROP, 0.0, 1.0);

    // 32bit spatial hash 格子補間ノイズによる白濁
    // CPU の整数ピクセル座標 (x, y) を復元（フラグメント中心 uv = (x+0.5)/res）。
    vec2 pixelPos = vTexCoord * uResolution - 0.5;
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
