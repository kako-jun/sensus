#version 300 es
precision mediump float;

// 歪視（Metamorphopsia）シミュレーション
// hash2D ベースの smooth noise 変位マップで各ピクセルをリマップする。

uniform sampler2D uTexture;
uniform float uStrength;   // 0.0..=1.0
uniform float uFreq;       // 空間周波数（グリッド分割数 / min(W,H)）
uniform float uSeed;       // ランダムシード（整数を float で渡す）
uniform vec2  uTexelSize;  // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

// 2D hash: uv → [0, 1]^2 の擬似ランダム値
vec2 hash2(vec2 p, float seed) {
    p += seed * 0.001;
    p = vec2(
        dot(p, vec2(127.1, 311.7)),
        dot(p, vec2(269.5, 183.3))
    );
    return fract(sin(p) * 43758.5453123);
}

// 格子点間を smooth hermite 補間する value noise（変位マップ用）
vec2 smoothNoise(vec2 uv, float seed) {
    vec2 i = floor(uv);
    vec2 f = fract(uv);

    // smoothstep（3次エルミート）
    vec2 u = f * f * (3.0 - 2.0 * f);

    vec2 a = hash2(i + vec2(0.0, 0.0), seed);
    vec2 b = hash2(i + vec2(1.0, 0.0), seed);
    vec2 c = hash2(i + vec2(0.0, 1.0), seed);
    vec2 d = hash2(i + vec2(1.0, 1.0), seed);

    // 双線形補間
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

void main() {
    if (uStrength <= 0.0) {
        fragColor = texture(uTexture, vTexCoord);
        return;
    }

    // 最大変位量（テクスチャ座標空間、8px 相当）
    const float MAX_DISP_PX = 8.0;

    // ノイズ座標（周波数スケール）
    vec2 noiseUv = vTexCoord * uFreq;

    // [-1, 1]^2 の変位ベクトルを生成
    vec2 noise = smoothNoise(noiseUv, uSeed) * 2.0 - 1.0;

    // テクスチャ座標への変位量（texel 単位 → uv 単位）
    vec2 disp = noise * uStrength * MAX_DISP_PX * uTexelSize;

    vec2 sampledUv = clamp(vTexCoord + disp, vec2(0.0), vec2(1.0));
    fragColor = texture(uTexture, sampledUv);
}
