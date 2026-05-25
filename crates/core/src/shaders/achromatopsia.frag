#version 300 es
precision mediump float;

// BT.709 photopic luminance によるグレースケール化（全色盲シミュレーション）
// 係数: R=0.2126, G=0.7152, B=0.0722

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uRWeight; // 0.2126
uniform float uGWeight; // 0.7152
uniform float uBWeight; // 0.0722

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 tex = texture(uTexture, vTexCoord);
    float r = srgbToLinear(tex.r);
    float g = srgbToLinear(tex.g);
    float b = srgbToLinear(tex.b);

    float y = uRWeight * r + uGWeight * g + uBWeight * b;

    float nr = r + (y - r) * uStrength;
    float ng = g + (y - g) * uStrength;
    float nb = b + (y - b) * uStrength;

    fragColor = vec4(
        linearToSrgb(clamp(nr, 0.0, 1.0)),
        linearToSrgb(clamp(ng, 0.0, 1.0)),
        linearToSrgb(clamp(nb, 0.0, 1.0)),
        tex.a
    );
}
