#version 300 es
precision mediump float;

// スターバースト（輝度閾値マスク）シミュレーション。
// GPU 上でのフルレイマーチングは重いため、輝度が threshold を超える画素を
// 強調するシンプルなブライトニングを行う。
// uDispersion=0.0 → 白い強調、uDispersion=1.0 → 虹色（色相=方向角）の強調。
// フルレイマーチング版は CPU 実装（vision::starbursts）を参照。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uThreshold;
uniform float uDispersion;

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// HSL(H, S=1, L=0.5) → linear sRGB の虹色変換（GPU 用）
vec3 hslRainbow(float hueDeg) {
    float h = mod(hueDeg, 360.0);
    float sector = floor(h / 60.0);
    float f = h / 60.0 - sector;
    float r, g, b;
    if (sector < 1.0)      { r = 1.0;     g = f;       b = 0.0; }
    else if (sector < 2.0) { r = 1.0 - f; g = 1.0;     b = 0.0; }
    else if (sector < 3.0) { r = 0.0;     g = 1.0;     b = f; }
    else if (sector < 4.0) { r = 0.0;     g = 1.0 - f; b = 1.0; }
    else if (sector < 5.0) { r = f;       g = 0.0;     b = 1.0; }
    else                   { r = 1.0;     g = 0.0;     b = 1.0 - f; }
    return vec3(srgbToLinear(r), srgbToLinear(g), srgbToLinear(b));
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    float rl = srgbToLinear(orig.r);
    float gl = srgbToLinear(orig.g);
    float bl = srgbToLinear(orig.b);

    // BT.709 輝度
    float luma = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;

    if (luma > uThreshold) {
        float excess = (luma - uThreshold) / max(1.0 - uThreshold, 0.001);
        float boost = excess * uStrength;
        // UV 座標を角度に変換して虹色レイを近似
        float angle = degrees(atan(vTexCoord.y - 0.5, vTexCoord.x - 0.5));
        vec3 rainbow = hslRainbow(angle);
        vec3 white = vec3(1.0, 1.0, 1.0);
        vec3 rayColor = mix(white, rainbow, uDispersion);
        fragColor = vec4(
            linearToSrgb(clamp(rl + boost * rayColor.r, 0.0, 1.0)),
            linearToSrgb(clamp(gl + boost * rayColor.g, 0.0, 1.0)),
            linearToSrgb(clamp(bl + boost * rayColor.b, 0.0, 1.0)),
            orig.a
        );
    } else {
        fragColor = orig;
    }
}
