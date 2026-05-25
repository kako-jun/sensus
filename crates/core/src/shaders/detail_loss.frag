#version 300 es
precision mediump float;
uniform sampler2D u_image;
uniform float u_strength;
uniform vec2 u_resolution;
in vec2 v_texcoord;
out vec4 out_color;

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
    float tile_size = max(u_strength * 20.0, 1.0);

    // UV をタイル境界にスナップ
    vec2 px = v_texcoord * u_resolution;
    vec2 tile_origin = floor(px / tile_size) * tile_size;
    vec2 snapped_uv = (tile_origin + tile_size * 0.5) / u_resolution;
    snapped_uv = clamp(snapped_uv, 0.0, 1.0);

    vec4 c = texture(u_image, snapped_uv);
    out_color = c;
}
