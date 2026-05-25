#version 300 es
precision mediump float;

// BPPV（良性発作性頭位めまい症）シミュレーション。
// nystagmus パターン（sawtooth 波）による回転変位。
// CPU 実装 vision::bppv_rotation に対応。
//
// 周期 2 秒、急速相 0.3 秒 → 緩徐相 1.7 秒の sawtooth 波。
// 最大回転角 20° = 0.3491 rad。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uTime;   // 秒単位の時間

in vec2 vTexCoord;
out vec4 fragColor;

void main() {
    const float MAX_ANGLE = 0.3491; // 20°
    const float PERIOD = 2.0;
    const float FAST_FRACTION = 0.3;

    float phase = mod(uTime, PERIOD) / PERIOD;
    float angleNorm;
    if (phase < FAST_FRACTION) {
        angleNorm = phase / FAST_FRACTION;
    } else {
        angleNorm = 1.0 - (phase - FAST_FRACTION) / (1.0 - FAST_FRACTION);
    }

    float angle = uStrength * MAX_ANGLE * angleNorm;
    float cosA = cos(angle);
    float sinA = sin(angle);

    vec2 center = vec2(0.5, 0.5);
    vec2 uv = vTexCoord - center;
    vec2 srcUV = vec2(
        cosA * uv.x + sinA * uv.y,
        -sinA * uv.x + cosA * uv.y
    ) + center;

    fragColor = texture(uTexture, clamp(srcUV, 0.0, 1.0));
}
