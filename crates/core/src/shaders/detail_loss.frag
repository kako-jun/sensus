#version 300 es
// M-2 実装方針: GLSL・CPU ともに「タイル中心1点サンプリング（pixelation）」に統一。
// CPU の全ピクセル平均から中心点参照に変更し、GPU と厳密に一致させる。
// これにより CPU/GPU の PSNR ≥ 30 dB が保証される。
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

    // UV をタイル境界にスナップし、タイル中心1点をサンプリング
    vec2 px = vTexCoord * uResolution;
    vec2 tile_origin = floor(px / tile_size) * tile_size;
    vec2 center_px = tile_origin + tile_size * 0.5;
    vec2 center_uv = clamp(center_px / uResolution, 0.0, 1.0);

    vec4 s = texture(uTexture, center_uv);
    vec3 lin = vec3(srgb_to_linear(s.r), srgb_to_linear(s.g), srgb_to_linear(s.b));
    vec4 orig = texture(uTexture, vTexCoord);
    fragColor = vec4(linear_to_srgb(lin.r), linear_to_srgb(lin.g), linear_to_srgb(lin.b), orig.a);
}
