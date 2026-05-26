#version 300 es
precision highp float;

// スターバースト（光芒）シミュレーション。
//
// CPU 実装（vision::starbursts）は明部画素を起点に num_rays 本のレイを放射し、
// 各レイに沿って距離減衰させながら additive 合成する scatter（散乱）型である。
// GPU の単一パス・フラグメントシェーダでは scatter を直接表現できないため、
// その厳密な転置（transpose）である gather（収集）型で同一の結果を得る:
//   出力画素 (px, py) に光を寄与しうる明部画素は、各レイ方向 theta_i の
//   逆方向（theta_i + 180°）に距離 t だけ離れた位置にある。
//   よって出力画素から各レイ方向の逆方向へ t=1..uRayLengthPx だけ遡って
//   元画像をサンプリングし、その位置が明部なら CPU と同一の重み
//   src_intensity * (1 - t/L) * uStrength * rayColor を加算する。
// これは CPU の scatter が訪れる (source, t, ray) タプル集合と完全に一致する。
// 座標の量子化（round(t*cos), round(t*sin)）は roundHalfAwayFromZero で Rust
// f32::round と全 f32 値で bit 一致するため、訪れる画素集合は CPU と同一になる。
// dispersion=1（虹色）のように各 dest 画素への寄与が高々 1 本のときは bit 完全一致
// （PSNR=∞）。残る唯一の乖離源は dispersion=0 等で複数寄与が重なるときの加算順序に
// 由来する f32 丸め差で、PSNR は極めて高い。
//
// uDispersion=0.0 → 白い光芒、uDispersion=1.0 → 虹色（色相=レイ方向角）の光芒。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uThreshold;
uniform float uDispersion;
uniform float uNumRays;       // レイ本数（CPU num_rays と同値、>= 1 を想定）
uniform float uRayLengthPx;   // レイ長（ピクセル, CPU ray_length_px と同値）
uniform vec2 uTexelSize;      // 1 画素ぶんの texCoord（= 1/width, 1/height）

in vec2 vTexCoord;
out vec4 fragColor;

const float PI = 3.14159265358979323846;
const float R_LUMA = 0.2126;
const float G_LUMA = 0.7152;
const float B_LUMA = 0.0722;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// Rust f32::round（0 から離れる方向に半丸め）を GLSL で全 f32 値で bit 一致再現する。
// floor(abs(x)+0.5) 方式は abs(x)+0.5 の加算で精度を失い、x が 0.5 の直下（例
// 0.49999997）のとき和が f32 丸めで 1.0 に切り上がり floor=1 を返してしまう
// （Rust f32::round(0.49999997)=0 と乖離する）。加算で精度を失わない trunc/fract
// ベースで実装する。f = x - trunc(x) は Sterbenz 補題により厳密で、x と同符号。
// |f| >= 0.5 のとき 0 から離れる方向へ 1 だけ進む。負値の .5（-0.5→-1）も正しい。
float roundHalfAwayFromZero(float x) {
    float t = trunc(x);
    float f = x - t;          // 小数部（x と同符号、Sterbenz により厳密）
    if (abs(f) >= 0.5) {
        return t + sign(x);
    }
    return t;
}

// HSL(H, S=1, L=0.5) → linear sRGB の虹色変換（CPU hsl_rainbow_to_linear と同一）
vec3 hslRainbow(float hueDeg) {
    float h = mod(hueDeg, 360.0);
    float sector = floor(h / 60.0);
    float f = h / 60.0 - sector;
    float r, g, b;
    // H=360° は H=0°（赤）と同値になる（HSL の周期性）
    if (sector < 1.0)      { r = 1.0;     g = f;       b = 0.0; }
    else if (sector < 2.0) { r = 1.0 - f; g = 1.0;     b = 0.0; }
    else if (sector < 3.0) { r = 0.0;     g = 1.0;     b = f; }
    else if (sector < 4.0) { r = 0.0;     g = 1.0 - f; b = 1.0; }
    else if (sector < 5.0) { r = f;       g = 0.0;     b = 1.0; }
    else                   { r = 1.0;     g = 0.0;     b = 1.0 - f; }
    return vec3(srgbToLinear(r), srgbToLinear(g), srgbToLinear(b));
}

// 整数ピクセル座標 (px, py) の元画像を linear sRGB で読み、BT.709 輝度を返す。
// 範囲外は luma=0（寄与なし）。
float sampleLuma(float px, float py, float w, float h, out vec3 linRgb) {
    if (px < 0.0 || px > w - 1.0 || py < 0.0 || py > h - 1.0) {
        linRgb = vec3(0.0);
        return -1.0;
    }
    // 画素中心の texCoord = (px + 0.5) / w
    vec2 uv = (vec2(px, py) + 0.5) * uTexelSize;
    vec3 c = texture(uTexture, uv).rgb;
    linRgb = vec3(srgbToLinear(c.r), srgbToLinear(c.g), srgbToLinear(c.b));
    return R_LUMA * linRgb.r + G_LUMA * linRgb.g + B_LUMA * linRgb.b;
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    float rl = srgbToLinear(orig.r);
    float gl = srgbToLinear(orig.g);
    float bl = srgbToLinear(orig.b);

    if (uStrength <= 0.0 || uNumRays < 1.0 || uRayLengthPx < 1.0) {
        fragColor = orig;
        return;
    }

    float w = 1.0 / uTexelSize.x;
    float h = 1.0 / uTexelSize.y;
    // 出力画素の整数座標（フラグメント中心 uv = (px+0.5)/w → px = uv*w - 0.5）
    float px = floor(vTexCoord.x * w);
    float py = floor(vTexCoord.y * h);

    float invDenom = 1.0 / max(1.0 - uThreshold, 1e-6);
    float numRays = roundHalfAwayFromZero(uNumRays);
    float rayLen = roundHalfAwayFromZero(uRayLengthPx);

    vec3 rayAccum = vec3(0.0);

    // gather: 各レイ方向 i の逆方向に遡って明部を探す。
    for (float i = 0.0; i < numRays; i += 1.0) {
        float theta = i * 2.0 * PI / numRays;
        float cosT = cos(theta);
        float sinT = sin(theta);

        // レイ色（CPU と同様、レイ方向角の色相）
        float angleDeg = mod(degrees(theta), 360.0);
        vec3 rainbow = hslRainbow(angleDeg);
        vec3 rayColor = mix(vec3(1.0), rainbow, uDispersion);

        for (float t = 1.0; t <= rayLen; t += 1.0) {
            // scatter: source + round(t*cos, t*sin) = dest
            // gather:  source = dest - round(t*cos, t*sin)
            float sx = px - roundHalfAwayFromZero(t * cosT);
            float sy = py - roundHalfAwayFromZero(t * sinT);
            vec3 srcLin;
            float luma = sampleLuma(sx, sy, w, h, srcLin);
            if (luma <= uThreshold) {
                continue;
            }
            float srcIntensity = (luma - uThreshold) * invDenom;
            float weight = srcIntensity * (1.0 - t / rayLen) * uStrength;
            rayAccum += weight * rayColor;
        }
    }

    fragColor = vec4(
        linearToSrgb(clamp(rl + rayAccum.r, 0.0, 1.0)),
        linearToSrgb(clamp(gl + rayAccum.g, 0.0, 1.0)),
        linearToSrgb(clamp(bl + rayAccum.b, 0.0, 1.0)),
        orig.a
    );
}
