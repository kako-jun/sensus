#version 300 es
precision mediump float;

// 黄斑変性（macular degeneration）シミュレーション — 中心暗化（foveal smoothstep マスク）
// 中心部を暗化・脱色する。strength=1.0 で最強の中心視野欠損。

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
    vec4 src = texture(uTexture, vTexCoord);

    // UV 空間での中心からの距離（コーナー距離で正規化）
    vec2 uv = vTexCoord - vec2(0.5, 0.5);
    float d = length(uv) / 0.7071067811865476;

    // vision.rs と同じ定数
    float inner_r = uStrength * 0.25;
    float outer_r = uStrength * 0.4;

    float u_t = clamp((d - inner_r) / max(outer_r - inner_r, 1e-5), 0.0, 1.0);
    float t = 1.0 - u_t * u_t * (3.0 - 2.0 * u_t); // 1 - smoothstep（中心ほど強い）

    float rl = srgbToLinear(src.r);
    float gl = srgbToLinear(src.g);
    float bl = srgbToLinear(src.b);

    // BT.709 輝度
    float lum = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
    float darkened = lum * (1.0 - uStrength * 0.95);

    float out_r = mix(rl, darkened, t);
    float out_g = mix(gl, darkened, t);
    float out_b = mix(bl, darkened, t);

    fragColor = vec4(
        linearToSrgb(clamp(out_r, 0.0, 1.0)),
        linearToSrgb(clamp(out_g, 0.0, 1.0)),
        linearToSrgb(clamp(out_b, 0.0, 1.0)),
        src.a
    );
}
