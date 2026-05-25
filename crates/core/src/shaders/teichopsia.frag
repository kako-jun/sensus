#version 300 es
precision mediump float;
uniform sampler2D u_image;
uniform float u_strength;
in vec2 v_texcoord;
out vec4 out_color;

#define PI 3.14159265358979323846

// sRGB -> linear
float srgb_to_linear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
// linear -> sRGB
float linear_to_srgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 c = texture(u_image, v_texcoord);
    vec3 lin = vec3(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));

    // 正規化 UV (-0.5..0.5)
    vec2 uv = v_texcoord - vec2(0.5);
    float dist = length(uv);

    vec3 result = lin;

    if (dist < 0.2) {
        // scotoma: 内側を暗化
        float dark = 1.0 - u_strength * 0.7 * (1.0 - dist / 0.2);
        result = lin * dark;
    } else if (dist <= 0.5) {
        // ジグザグリング: saw wave
        float angle = atan(uv.y, uv.x);
        float saw = fract(angle / PI * 8.0);
        float ring_t = (dist - 0.2) / 0.3;
        float fade = clamp(ring_t * (1.0 - ring_t) * 4.0, 0.0, 1.0);
        float brightness = saw * u_strength * fade * 0.6;
        result = clamp(lin + vec3(brightness), 0.0, 1.0);
    }

    out_color = vec4(linear_to_srgb(result.r), linear_to_srgb(result.g), linear_to_srgb(result.b), c.a);
}
