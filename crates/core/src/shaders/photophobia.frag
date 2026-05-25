#version 300 es
precision mediump float;

// 光過敏（Photophobia）シミュレーション。
// 高輝度領域の bloom + 全体ハレーション。
// CPU 実装 vision::photophobia に対応。
//
// 注意: GPU 版は bloom blur を省略し、閾値超過画素の輝度 boost のみ行う
//       シンプル実装。フル bloom は CPU 実装を使用すること。

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
    float rl = srgbToLinear(orig.r);
    float gl = srgbToLinear(orig.g);
    float bl = srgbToLinear(orig.b);

    // BT.709 輝度
    float luma = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;

    // 閾値 0.5 超過分を bloom boost
    const float threshold = 0.5;
    float boost = 0.0;
    if (luma > threshold) {
        float excess = (luma - threshold) / max(1.0 - threshold, 0.001);
        boost = excess * uStrength;
    }

    fragColor = vec4(
        linearToSrgb(clamp(rl + boost, 0.0, 1.0)),
        linearToSrgb(clamp(gl + boost, 0.0, 1.0)),
        linearToSrgb(clamp(bl + boost, 0.0, 1.0)),
        orig.a
    );
}
