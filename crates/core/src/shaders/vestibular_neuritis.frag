#version 300 es
precision mediump float;

// 前庭神経炎（Vestibular Neuritis）シミュレーション。
// 水平シフト + 1D 水平 blur（motion blur）。
// CPU 実装 vision::vestibular_neuritis に対応。
//
// GPU 版は 16-tap 水平 blur で motion blur を再現。
// シフト量: strength * 0.05 (テクセル単位)

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uRadiusPx;     // 水平 blur 半径（ピクセル単位）= strength * 0.04 * width
uniform float uShiftTexel;   // 水平シフト（テクセル単位）= strength * 0.05
uniform vec2  uTexelSize;    // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    // 水平シフト
    vec2 shiftedUV = vec2(clamp(vTexCoord.x - uShiftTexel, 0.0, 1.0), vTexCoord.y);

    if (uRadiusPx < 0.5) {
        fragColor = texture(uTexture, shiftedUV);
        return;
    }

    // 16-tap 水平 1D blur
    const int N = 16;
    vec3 acc = vec3(0.0);
    for (int i = 0; i < N; i++) {
        float t = (float(i) / float(N - 1)) * 2.0 - 1.0;
        float offsetU = t * uRadiusPx * uTexelSize.x;
        vec4 s = texture(uTexture, vec2(clamp(shiftedUV.x + offsetU, 0.0, 1.0), shiftedUV.y));
        acc += vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
    }
    vec3 blurred = acc / float(N);

    fragColor = vec4(
        linearToSrgb(clamp(blurred.r, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.g, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.b, 0.0, 1.0)),
        texture(uTexture, shiftedUV).a
    );
}
