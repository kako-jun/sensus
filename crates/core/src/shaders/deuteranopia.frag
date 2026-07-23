#version 300 es
precision mediump float;

// Machado 2009 per-severity 行列（linear sRGB → simulated linear sRGB）。
// uMatrix は CPU 側 (deuteranopia_uniforms) で strength から 11 段テーブルを
// 補間済みの解決済み行列（#165）。severity=0.0 で単位行列・severity=1.0 で
// 完全 dichromacy 行列になるため、この行列を直接適用するだけでよい
// （旧実装のような追加の uStrength blend は不要）。
// 出典: https://www.inf.ufrgs.br/~oliveira/pubs_files/CVD_Simulation/CVD_Simulation.html

uniform sampler2D uTexture;
uniform float uStrength; // 参考値（severity）。行列に解決済みのため本シェーダの計算では未使用。
uniform float uMatrix[9]; // 3x3 解決済み行列（行優先: row0col0, row0col1, row0col2, row1col0, ...）

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

    float sr = uMatrix[0] * r + uMatrix[1] * g + uMatrix[2] * b;
    float sg = uMatrix[3] * r + uMatrix[4] * g + uMatrix[5] * b;
    float sb = uMatrix[6] * r + uMatrix[7] * g + uMatrix[8] * b;

    fragColor = vec4(
        linearToSrgb(clamp(sr, 0.0, 1.0)),
        linearToSrgb(clamp(sg, 0.0, 1.0)),
        linearToSrgb(clamp(sb, 0.0, 1.0)),
        tex.a
    );
}
