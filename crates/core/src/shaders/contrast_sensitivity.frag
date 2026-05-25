#version 300 es
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
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
    vec4 c = texture(uTexture, vTexCoord);
    vec3 lin = vec3(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));
    // output = 0.5 + (input - 0.5) * (1.0 - strength * 0.5)
    float scale = 1.0 - uStrength * 0.5;
    vec3 result = clamp(vec3(0.5) + (lin - vec3(0.5)) * scale, 0.0, 1.0);
    fragColor = vec4(linear_to_srgb(result.r), linear_to_srgb(result.g), linear_to_srgb(result.b), c.a);
}
