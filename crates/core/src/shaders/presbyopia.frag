#version 300 es
precision mediump float;

// 等方 disk blur（pillbox kernel）— 近視 / 遠視 / 老眼シミュレーション
// Poisson disk サンプリング（16 サンプル）で円形ぼかしを近似する。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uRadiusPx;     // ぼかし半径（ピクセル単位）
uniform vec2  uTexelSize;    // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// Fibonacci lattice による円形サンプリング（16 点）
vec4 diskBlur(vec2 uv, float radius) {
    if (radius < 0.5) return texture(uTexture, uv);

    const int N = 16;
    // 黄金角 phi ≈ 137.508°
    const float PHI = 2.399963229728653;

    vec3 acc = vec3(0.0);
    float totalWeight = 0.0;

    for (int i = 0; i < N; i++) {
        float t = float(i) / float(N);
        float r = sqrt(t) * radius;
        float theta = float(i) * PHI;
        vec2 offset = vec2(cos(theta), sin(theta)) * r * uTexelSize;
        vec4 s = texture(uTexture, uv + offset);
        // linear 空間でサンプリング
        vec3 lin = vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
        acc += lin;
        totalWeight += 1.0;
    }

    vec3 blurred = acc / totalWeight;
    return vec4(
        linearToSrgb(clamp(blurred.r, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.g, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.b, 0.0, 1.0)),
        texture(uTexture, uv).a
    );
}

void main() {
    fragColor = diskBlur(vTexCoord, uRadiusPx);
}
