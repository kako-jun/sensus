#version 300 es
precision mediump float;

// 半盲（hemianopia）シミュレーション — 左右半側マスク
// uSide=1.0: 右半分を暗化, uSide=-1.0: 左半分を暗化（vision.rs 規約に合わせた外部値）
// 内部では vision.rs の side=0.0/1.0 規約に変換: uSide=1.0(右欠損) → side=1.0

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uSide;     // 1.0=右側欠損, -1.0=左側欠損

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 src = texture(uTexture, vTexCoord);

    float x = vTexCoord.x;
    float split = 0.5;
    float blur_w = 0.02; // vision.rs と同じ 2% ぼかし幅

    // 左側フェード量（左端=1.0, 右端=0.0）
    float t_left = clamp((x - (split - blur_w)) / (2.0 * blur_w), 0.0, 1.0);
    float left_fade = 1.0 - t_left * t_left * (3.0 - 2.0 * t_left);

    // vision.rs 規約: side=0→左欠損, side=1→右欠損
    // uSide: 1.0=右欠損(-1を0に変換), -1.0=左欠損(1を1に変換)
    float side = (uSide + 1.0) * 0.5; // [-1,1] → [0,1]

    float fade = mix(left_fade, 1.0 - left_fade, side);
    float mul = 1.0 - fade * uStrength;

    float rl = srgbToLinear(src.r);
    float gl = srgbToLinear(src.g);
    float bl = srgbToLinear(src.b);

    fragColor = vec4(
        linearToSrgb(clamp(rl * mul, 0.0, 1.0)),
        linearToSrgb(clamp(gl * mul, 0.0, 1.0)),
        linearToSrgb(clamp(bl * mul, 0.0, 1.0)),
        src.a
    );
}
