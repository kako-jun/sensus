#version 300 es
precision mediump float;

// 緑内障（glaucoma）シミュレーション — 周辺ビネット（smoothstep マスク）
// 中心を残し、周辺を暗化する。strength=1.0 で最強の周辺暗化。

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

    // UV 空間での中心からの距離（コーナー距離 = sqrt(0.5^2+0.5^2) で正規化）
    vec2 uv = vTexCoord - vec2(0.5, 0.5);
    float d = length(uv) / 0.7071067811865476; // sqrt(0.5^2+0.5^2)

    // vision.rs と同じ定数
    float inner_r = 1.0 - uStrength * 0.7;
    float outer_r = min(inner_r + 0.2, 1.0);

    float t = clamp((d - inner_r) / (outer_r - inner_r), 0.0, 1.0);
    float fade = t * t * (3.0 - 2.0 * t); // smoothstep
    float mul = 1.0 - uStrength * fade;

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
