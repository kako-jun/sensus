#version 300 es
precision mediump float;

// 1D directional blur — 乱視シミュレーション（純粋 cylinder lens）
// 軸方向に沿ったタップで line focus（焦線）を近似する。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uRadiusPx;     // ぼかし半径（ピクセル単位）
uniform float uAxisDeg;      // 軸角度（度数法: 0°=水平, 90°=垂直）
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
    if (uRadiusPx < 0.5) {
        fragColor = texture(uTexture, vTexCoord);
        return;
    }

    const int N = 16;
    float rad = uAxisDeg * 3.14159265358979 / 180.0;
    vec2 dir = vec2(cos(rad), sin(rad));

    vec3 acc = vec3(0.0);
    float totalWeight = 0.0;

    for (int i = 0; i < N; i++) {
        // -radius .. +radius に均等配置
        float t = (float(i) / float(N - 1)) * 2.0 - 1.0;
        vec2 offset = dir * (t * uRadiusPx) * uTexelSize;
        vec4 s = texture(uTexture, vTexCoord + offset);
        acc += vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
        totalWeight += 1.0;
    }

    vec3 blurred = acc / totalWeight;
    fragColor = vec4(
        linearToSrgb(clamp(blurred.r, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.g, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.b, 0.0, 1.0)),
        texture(uTexture, vTexCoord).a
    );
}
