#version 300 es
precision highp float;
precision highp int;

// 閃輝暗点・光の星（Flickering stars）シミュレーション。
// CPU 実装 vision::flickering_stars と同一のモデル（#134 で CPU↔GLSL を統一）。
//
// 点 i ごとに 32bit spatial hash（star_hash32）で位置(fx,fy)と輝度を生成し、
// 整数ピクセル中心の 5×5 正方ボックス（|dx|<=2 && |dy|<=2）内の画素に
// linear sRGB 空間で輝度を加算する。加算は点ごとに min(1.0) でクランプする
// （= CPU の per-star clamp と同順・同演算）。
//
// 旧実装は Wang hash + 円形 distance + 合計後クランプで、CPU の 64bit LCG +
// 正方ボックス + per-star clamp と乖離していた（#134）。

uniform sampler2D uTexture;
uniform float uStrength;
uniform uint uSeed;       // u64 シードの下位 32bit
uniform int uCount;       // 点数 = (strength * 200) as usize（CPU 側で算出して渡す）
uniform vec2 uResolution;

in vec2 vTexCoord;
out vec4 fragColor;

// #134: CPU vision::star_hash32 と同一の 32bit hash（cataract gridNoise と同系列）。
uint starHash(uint seed, uint k) {
    uint h = seed * 0x9e3779b9u + k * 0x85ebca6bu;
    h ^= h >> 15;
    h *= 0x2c1b3c6du;
    h ^= h >> 12;
    h *= 0x297a2d39u;
    h ^= h >> 15;
    return h;
}
float hash01(uint k) {
    return float(starHash(uSeed, k)) / float(0xFFFFFFFFu); // 0.0..=1.0
}

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 c = texture(uTexture, vTexCoord);
    vec3 lin = vec3(srgbToLinear(c.r), srgbToLinear(c.g), srgbToLinear(c.b));

    // 現フラグメントの整数ピクセル座標（CPU の px, py に対応）
    ivec2 p = ivec2(floor(vTexCoord * uResolution));

    const int BLOB_RADIUS = 2;
    for (int i = 0; i < uCount; i++) {
        uint ui = uint(i);
        float fx = hash01(3u * ui);
        float fy = hash01(3u * ui + 1u);
        float fb = hash01(3u * ui + 2u);
        int cx = int(fx * uResolution.x);
        int cy = int(fy * uResolution.y);
        float brightness = 0.5 + fb * 0.5;
        if (abs(p.x - cx) <= BLOB_RADIUS && abs(p.y - cy) <= BLOB_RADIUS) {
            // CPU と同じく点ごとに min(1.0) でクランプ
            lin = min(lin + vec3(brightness), 1.0);
        }
    }

    fragColor = vec4(linearToSrgb(lin.r), linearToSrgb(lin.g), linearToSrgb(lin.b), c.a);
}
