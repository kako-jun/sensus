#version 300 es
precision mediump float;

// めまい（Vertigo）シミュレーション。
// 時間依存の回転変位（逆変換サンプリング）+ 全体 disk blur。
// CPU 実装 vision::vertigo に対応。
//
// 最大回転角 15° = 0.2618 rad、周波数 0.3 Hz のサイン波で変動。
// 回転は aspect 補正したピクセル比例空間で行い、非正方形でも CPU
// （ピクセル空間回転）と一致させる。
// 回転後に CPU と同じ等方 disk blur（radius = strength*0.015*min_dim）を
// linear sRGB 空間で適用する。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uTime;       // 秒単位の時間
uniform float uAspect;     // width / height（回転の aspect 補正）
uniform float uRadiusPx;   // disk blur 半径（ピクセル単位）
uniform vec2  uTexelSize;  // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// 出力 UV から回転逆変換した元 UV を求める（aspect 補正つき）。
vec2 rotateSrcUV(vec2 texCoord, float cosA, float sinA) {
    vec2 center = vec2(0.5, 0.5);
    vec2 uv = texCoord - center;
    // ピクセル比例空間へ（x を aspect 倍）
    uv.x *= uAspect;
    // 逆変換（出力座標 → 入力座標）
    vec2 src = vec2(
        cosA * uv.x + sinA * uv.y,
        -sinA * uv.x + cosA * uv.y
    );
    // UV 空間へ戻す
    src.x /= uAspect;
    return src + center;
}

void main() {
    const float PI = 3.14159265358979;
    const float MAX_ANGLE = 0.2618; // 15°
    const int N = 16;
    // 黄金角 phi ≈ 137.508°
    const float PHI = 2.399963229728653;

    float angle = uStrength * MAX_ANGLE * sin(2.0 * PI * 0.3 * uTime);
    float cosA = cos(angle);
    float sinA = sin(angle);

    vec2 srcUV = rotateSrcUV(vTexCoord, cosA, sinA);
    srcUV = clamp(srcUV, 0.0, 1.0);

    // 回転後の中心ピクセルとその alpha
    vec4 centerSample = texture(uTexture, srcUV);

    if (uRadiusPx < 0.5) {
        fragColor = centerSample;
        return;
    }

    // 回転後の像に対し、Fibonacci lattice 16tap の等方 disk blur を
    // linear sRGB 空間で適用する。CPU は「回転後の像」を blur するため、
    // 出力空間（回転後）でのオフセットを逆回転して元 UV 空間に写してから
    // サンプリングする（オフセットも回転させる）。
    vec3 acc = vec3(0.0);
    for (int i = 0; i < N; i++) {
        float t = float(i) / float(N);
        float r = sqrt(t) * uRadiusPx;
        float theta = float(i) * PHI;
        // 出力（回転後）空間でのテクセルオフセット
        vec2 outOffset = vec2(cos(theta), sin(theta)) * r;
        // 逆回転して元 UV 空間のオフセットへ（disk が等方なので形状は不変）
        vec2 srcOffset = vec2(
            cosA * outOffset.x + sinA * outOffset.y,
            -sinA * outOffset.x + cosA * outOffset.y
        ) * uTexelSize;
        vec2 tapUV = clamp(srcUV + srcOffset, 0.0, 1.0);
        vec4 s = texture(uTexture, tapUV);
        acc += vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
    }
    vec3 blurred = acc / float(N);

    fragColor = vec4(
        linearToSrgb(clamp(blurred.r, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.g, 0.0, 1.0)),
        linearToSrgb(clamp(blurred.b, 0.0, 1.0)),
        centerSample.a
    );
}
