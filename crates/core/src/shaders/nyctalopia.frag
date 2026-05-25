#version 300 es
precision mediump float;

// 夜盲（Nyctalopia）シミュレーション。
// 暗化 + 脱色（グレースケール寄り）。
// CPU 実装 vision::nyctalopia と同じ式:
//   dark_factor = 1.0 - uStrength * 0.7
//   desat       = uStrength * 0.8
//   desaturated = orig + (luma - orig) * desat
//   output      = desaturated * dark_factor

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

    float luma = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;

    float darkFactor = 1.0 - uStrength * 0.7;
    float desat = uStrength * 0.8;

    // 脱色（グレーに寄せる）してから暗化
    float dr = rl + (luma - rl) * desat;
    float dg = gl + (luma - gl) * desat;
    float db = bl + (luma - bl) * desat;

    float fr = clamp(dr * darkFactor, 0.0, 1.0);
    float fg = clamp(dg * darkFactor, 0.0, 1.0);
    float fb = clamp(db * darkFactor, 0.0, 1.0);

    fragColor = vec4(
        linearToSrgb(fr),
        linearToSrgb(fg),
        linearToSrgb(fb),
        orig.a
    );
}
