#version 300 es
precision mediump float;

// 白内障（Cataract）シミュレーション。
// 黄変マトリクス + haze overlay（明るさ底上げ）。
// CPU 実装 vision::cataract の黄変マトリクス部分のみ再現。
// ブロックノイズ（seed 依存）はリアルタイム GPU では定数 haze に置換。
//
// 黄変マトリクス（linear sRGB → yellowed linear sRGB）:
//   | yr |   | 1.00  0.05 -0.05 | | r |
//   | yg | = | 0.02  1.00 -0.02 | | g |
//   | yb |   | 0.00  0.00  0.85 | | b |
//
// haze: strength * WHITE_BLEND_MAX * 0.5 の一定白混合
//       (CPU の平均 noise = 0.5 と仮定)

uniform sampler2D uTexture;
uniform float uStrength;

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    float r = srgbToLinear(orig.r);
    float g = srgbToLinear(orig.g);
    float b = srgbToLinear(orig.b);

    // 黄変マトリクス適用
    float yr = clamp(r * 1.00 + g * 0.05 + b * (-0.05), 0.0, 1.0);
    float yg = clamp(r * 0.02 + g * 1.00 + b * (-0.02), 0.0, 1.0);
    float yb = clamp(r * 0.00 + g * 0.00 + b * 0.85,    0.0, 1.0);

    // strength でブレンド: orig * (1-s) + yellowed * s
    float nr = r + (yr - r) * uStrength;
    float ng = g + (yg - g) * uStrength;
    float nb = b + (yb - b) * uStrength;

    // haze: CPU の WHITE_BLEND_MAX=0.4, 平均 noise≈0.5 を採用
    const float WHITE_BLEND_MAX = 0.4;
    float whiteBlend = uStrength * 0.5 * WHITE_BLEND_MAX;
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
