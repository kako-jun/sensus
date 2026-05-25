#version 300 es
precision mediump float;

// 緑内障（glaucoma）シミュレーション — 周辺ビネット（smoothstep マスク）
// 中心を残し、周辺を暗化する。strength=1.0 で最強の周辺暗化。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uAspect;  // width / height。非正方形画像の aspect 補正に使用。

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

    // UV 座標を aspect 補正してから距離計算する。
    // Rust 実装（pixel 座標）との一致: dx/max_r = uv_x*aspect / corner
    vec2 uv = vTexCoord - vec2(0.5, 0.5);
    vec2 uvA = vec2(uv.x * uAspect, uv.y);
    // コーナー距離（aspect 補正済み）: sqrt((0.5*aspect)^2 + 0.5^2)
    float cornerDist = sqrt(0.5 * uAspect * 0.5 * uAspect + 0.5 * 0.5);
    float d = length(uvA) / cornerDist;

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
