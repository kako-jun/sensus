#version 300 es
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
in vec2 vTexCoord;
out vec4 fragColor;

// sRGB -> linear
float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
// linear -> sRGB
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 c = texture(uTexture, vTexCoord);
    // decode to linear
    vec3 lin = vec3(srgbToLinear(c.r), srgbToLinear(c.g), srgbToLinear(c.b));
    // contrast compression in linear space
    vec3 compressed = vec3(0.5) + (lin - vec3(0.5)) * (1.0 - uStrength * 0.15);
    // vignette
    vec2 uv = vTexCoord * 2.0 - 1.0;
    float d = dot(uv, uv);
    float t = clamp((d - 0.3) / (1.2 - 0.3), 0.0, 1.0);
    float sm = t * t * (3.0 - 2.0 * t);
    float vignette = 1.0 - uStrength * 0.3 * sm;
    vec3 result = clamp(compressed * vignette, 0.0, 1.0);
    // encode back to sRGB
    fragColor = vec4(linearToSrgb(result.r), linearToSrgb(result.g), linearToSrgb(result.b), c.a);
}
