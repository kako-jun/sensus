#version 300 es
precision highp float;
precision highp int;

// ドライアイ（dry eye）シミュレーション。
// CPU 実装 vision::dry_eye と同一のノイズモデル（#99 で統一）。
//
// ## アルゴリズム
// 画面を 32x32 ピクセルのタイルに分割し、各タイルに 32bit 整数 spatial hash で
// 擬似ランダムなノイズ値 noise(0..1) を割り当てる。各フラグメントが属するタイルの
// noise から blur 半径 = noise * uStrength * 3.0 px を決め、その半径の等方 disk
// （pillbox）で linear sRGB 空間の近傍を平均する。半径 < 0.5px のタイルは
// パススルー（元画像そのまま）。
//
// ## CPU との一致根拠
// - `tileHash` は CPU の `tile_hash` と完全に同じ 32bit 整数演算（seed=42 固定）。
//   GLSL の uint は CPU の u32 同様 mod 2^32 で wrap し、同じ (tx, ty) から bit 単位に
//   同じ noise を返す。float 経由の精度損失を避けるため整数で計算する。
// - disk のメンバシップ `dx*dx + dy*dy <= r*r`（dx,dy in -ceil(r)..=ceil(r)）と
//   edge replication（clamp）は CPU の build_ellipse_spans(r, r, 0) と一致。
// - 半径係数 *3.0、TILE_SIZE=32、linear sRGB サンプリングを CPU と統一。
// - 唯一の乖離源は浮動小数丸めのみ。

uniform sampler2D uTexture;
uniform float uStrength;
uniform vec2 uTexelSize; // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// linear sRGB でサンプリング（座標は整数ピクセル、edge clamp）。
vec3 sampleLinear(vec2 pixelCoord, vec2 resolution) {
    vec2 p = clamp(pixelCoord, vec2(0.0), resolution - 1.0);
    vec2 uv = (p + 0.5) * uTexelSize;
    vec4 c = texture(uTexture, uv);
    return vec3(srgbToLinear(c.r), srgbToLinear(c.g), srgbToLinear(c.b));
}

// タイル座標の 32bit 整数 spatial hash（0.0..=1.0）。CPU の tile_hash と同一。
float tileHash(uint tx, uint ty) {
    const uint SEED = 42u;
    uint h = SEED * 0x9e3779b9u
        + tx * 0x85ebca6bu
        + ty * 0xc2b2ae35u;
    h ^= h >> 15;
    h *= 0x2c1b3c6du;
    h ^= h >> 12;
    h *= 0x297a2d39u;
    h ^= h >> 15;
    return float(h) / float(0xFFFFFFFFu);
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    if (uStrength <= 0.0) {
        fragColor = orig;
        return;
    }

    const float TILE_SIZE = 32.0;
    const float MIN_BLUR_RADIUS_PX = 0.5;

    vec2 resolution = vec2(1.0) / uTexelSize;
    // CPU の整数ピクセル座標 (x, y)（top-left 規約）を復元する。
    // フラグメント中心の uv = (x+0.5)/w なので floor(uv*w) = x。
    vec2 pixelPos = floor(vTexCoord * resolution);
    // タイルインデックス（CPU: x / TILE_SIZE の整数除算）。
    uint tx = uint(floor(pixelPos.x / TILE_SIZE));
    uint ty = uint(floor(pixelPos.y / TILE_SIZE));

    float noise = tileHash(tx, ty);
    float blurRadius = noise * uStrength * 3.0;

    if (blurRadius < MIN_BLUR_RADIUS_PX) {
        // blur なし: 元画像そのまま（CPU と同じパススルー）。
        fragColor = orig;
        return;
    }

    // 等方 disk（pillbox）平均。CPU build_ellipse_spans(r, r, 0) と同じメンバシップ。
    int rMax = int(ceil(blurRadius));
    float r2 = blurRadius * blurRadius;
    vec3 acc = vec3(0.0);
    float count = 0.0;
    for (int dy = -rMax; dy <= rMax; dy++) {
        for (int dx = -rMax; dx <= rMax; dx++) {
            float fdx = float(dx);
            float fdy = float(dy);
            if (fdx * fdx + fdy * fdy <= r2) {
                acc += sampleLinear(pixelPos + vec2(fdx, fdy), resolution);
                count += 1.0;
            }
        }
    }
    vec3 blurred = count > 0.0 ? acc / count : sampleLinear(pixelPos, resolution);

    fragColor = vec4(
        linearToSrgb(blurred.r),
        linearToSrgb(blurred.g),
        linearToSrgb(blurred.b),
        orig.a
    );
}
