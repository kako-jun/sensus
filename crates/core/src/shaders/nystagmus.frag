#version 300 es
// 楕円カーネルの境界判定 (u²/a²+v²/b²≤1) を CPU(f32) と bit 一致させるため highp 必須。
// mediump だと境界格子点の内外が flip し採用 tap が 1 点ずれる。
precision highp float;
precision highp int;

// 1D directional blur — 眼振シミュレーション（motion blur による方向性ぼけ）
// astigmatism.frag と同一カーネル（CPU ellipse_blur の filled-ellipse box ミラー, #126）。
// uniform 名のみ違い: uAxisDeg → uDirectionDeg（揺れ方向をそのままぼかし方向に使う,
// astigmatism と異なり +90° しない）。詳細は astigmatism.frag のコメント参照。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uRadiusPx;       // ぼかし半径（長軸 a, ピクセル単位）
uniform float uDirectionDeg;   // 揺れ方向 = ぼかし方向（度数法: 0°=水平, 90°=垂直）
uniform vec2  uTexelSize;      // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

// RMAX=15 はカーネル半径（タップ）の上限。CPU `ellipse_blur` は半径無制限なので、
// `uRadiusPx > 15` では GLSL が CPU より弱くぼけ、両者は乖離する。
// nystagmus の半径は `amplitude × strength × min(W,H)` で、既定 amplitude=0.03 では
// min(W,H) ≳ 500 px で 15 px を超える（astigmatism は 0.011×min なので ≳1363 px）。
// 大半径で CPU↔GLSL の厳密一致が要るなら、per-pixel ボックスではなく
// 軸方向の固定タップ数サンプリングへ書き換えること（別 issue）。
const int RMAX = 15;
const float B_RADIUS = 0.5;

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

    float rad = uDirectionDeg * 3.14159265358979 / 180.0;
    float cosT = cos(rad);
    float sinT = sin(rad);
    float a2 = max(uRadiusPx * uRadiusPx, 1e-6);
    float b2 = max(B_RADIUS * B_RADIUS, 1e-6);

    float aMax = max(uRadiusPx, B_RADIUS);
    int rMax = int(ceil(aMax));
    if (rMax > RMAX) {
        rMax = RMAX;
    }

    vec3 acc = vec3(0.0);
    float n = 0.0;

    for (int dy = -RMAX; dy <= RMAX; dy++) {
        if (dy < -rMax || dy > rMax) {
            continue;
        }
        for (int dx = -RMAX; dx <= RMAX; dx++) {
            if (dx < -rMax || dx > rMax) {
                continue;
            }
            float fdx = float(dx);
            float fdy = float(dy);
            float u = fdx * cosT + fdy * sinT;
            float v = -fdx * sinT + fdy * cosT;
            if ((u * u) / a2 + (v * v) / b2 <= 1.0) {
                vec2 offset = vec2(fdx, fdy) * uTexelSize;
                vec4 s = texture(uTexture, vTexCoord + offset);
                acc += vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
                n += 1.0;
            }
        }
    }

    vec3 blurred = acc / max(n, 1.0);
    fragColor = vec4(
        linearToSrgb(clamp(blurred.r, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.g, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.b, 0.0, 1.0)),
        texture(uTexture, vTexCoord).a
    );
}
