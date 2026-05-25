#version 300 es
precision mediump float;

// スターバースト（輝度閾値マスク）シミュレーション。
// GPU 上でのフルレイマーチングは重いため、輝度が threshold を超える画素を
// 強調するシンプルなブライトニングを行う。
// フルレイマーチング版は CPU 実装（vision::starbursts）を参照。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uThreshold;

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

    if (luma > uThreshold) {
        float excess = (luma - uThreshold) / max(1.0 - uThreshold, 0.001);
        float boost = excess * uStrength;
        fragColor = vec4(
            linearToSrgb(clamp(rl + boost, 0.0, 1.0)),
            linearToSrgb(clamp(gl + boost, 0.0, 1.0)),
            linearToSrgb(clamp(bl + boost, 0.0, 1.0)),
            orig.a
        );
    } else {
        fragColor = orig;
    }
}
