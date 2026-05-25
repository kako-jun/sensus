#version 300 es
// M-2 実装方針: GLSL側を9点平均（3×3カーネル）に変更してCPU実装（タイル内全画素平均）に近似させる。
// 厳密には CPU は全タイル内ピクセル平均だが、GLSL は GPU loop 制限のため
// タイル中心を含む 3×3 グリッドサンプル平均で近似する。
// 小さいタイル（< 3px）では中心1点と同じになる。PSNR ≥ 30 dB を満たす。
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
uniform vec2 uResolution;
in vec2 vTexCoord;
out vec4 fragColor;

// sRGB -> linear
float srgb_to_linear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
// linear -> sRGB
float linear_to_srgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    // tile_size = (strength * 20.0).max(1.0)
    float tile_size = max(uStrength * 20.0, 1.0);

    // UV をタイル境界にスナップ（タイル中心）
    vec2 px = vTexCoord * uResolution;
    vec2 tile_origin = floor(px / tile_size) * tile_size;
    vec2 center_px = tile_origin + tile_size * 0.5;

    // 3×3 グリッドサンプルで平均を計算（CPU タイル平均の近似）
    vec3 acc = vec3(0.0);
    float count = 0.0;
    for (int dy = -1; dy <= 1; dy++) {
        for (int dx = -1; dx <= 1; dx++) {
            vec2 sample_px = center_px + vec2(float(dx), float(dy)) * (tile_size / 3.0);
            vec2 sample_uv = clamp(sample_px / uResolution, 0.0, 1.0);
            vec4 s = texture(uTexture, sample_uv);
            acc += vec3(srgb_to_linear(s.r), srgb_to_linear(s.g), srgb_to_linear(s.b));
            count += 1.0;
        }
    }
    vec3 avg_lin = acc / count;
    vec4 orig = texture(uTexture, clamp(center_px / uResolution, 0.0, 1.0));
    fragColor = vec4(linear_to_srgb(avg_lin.r), linear_to_srgb(avg_lin.g), linear_to_srgb(avg_lin.b), orig.a);
}
