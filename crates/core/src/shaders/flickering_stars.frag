#version 300 es
precision mediump float;
uniform sampler2D u_image;
uniform float u_strength;
uniform float u_seed;
uniform vec2 u_resolution;
in vec2 v_texcoord;
out vec4 out_color;

// simple hash: float -> float in [0,1]
float hash(float n) {
    return fract(sin(n) * 43758.5453123);
}

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

    float count = u_strength * 200.0;
    float blob_radius = 2.0;
    float brightness_add = 0.0;

    vec2 px = v_texcoord * u_resolution;

    for (float i = 0.0; i < 200.0; i++) {
        if (i >= count) break;
        float h = hash(i + u_seed * 1000.0);
        float h2 = hash(i + u_seed * 1000.0 + 7654.321);
        float h3 = hash(i + u_seed * 1000.0 + 9876.543);
        vec2 star = vec2(h * u_resolution.x, h2 * u_resolution.y);
        float dist = length(px - star);
        if (dist <= blob_radius) {
            brightness_add += 0.5 + h3 * 0.5;
        }
    }

    vec3 result = clamp(lin + vec3(brightness_add), 0.0, 1.0);
    out_color = vec4(linear_to_srgb(result.r), linear_to_srgb(result.g), linear_to_srgb(result.b), c.a);
}
