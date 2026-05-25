#version 300 es
precision mediump float;

// めまい（Vertigo）シミュレーション。
// 時間依存の回転変位（逆変換サンプリング）。
// CPU 実装 vision::vertigo に対応。
//
// 最大回転角 15° = 0.2618 rad、周波数 0.3 Hz のサイン波で変動。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uTime;   // 秒単位の時間

in vec2 vTexCoord;
out vec4 fragColor;

void main() {
    const float PI = 3.14159265358979;
    const float MAX_ANGLE = 0.2618; // 15°

    float angle = uStrength * MAX_ANGLE * sin(2.0 * PI * 0.3 * uTime);
    float cosA = cos(angle);
    float sinA = sin(angle);

    // 中心を原点に変換して逆回転
    vec2 center = vec2(0.5, 0.5);
    vec2 uv = vTexCoord - center;
    // 逆変換（出力座標 → 入力座標）
    vec2 srcUV = vec2(
        cosA * uv.x + sinA * uv.y,
        -sinA * uv.x + cosA * uv.y
    ) + center;

    fragColor = texture(uTexture, clamp(srcUV, 0.0, 1.0));
}
