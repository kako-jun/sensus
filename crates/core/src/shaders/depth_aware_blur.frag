#version 300 es
precision highp float;

// 深度マップ付き距離依存ぼけ（depth-aware defocus blur）。
// CPU 実装 vision::depth_aware_blur に対応する GLSL 経路（#107）。
//
// CPU は 8 段階の深度ビンごとに ellipse_blur（box 状）を生成して線形補間する
// 多パス方式だが、GPU は単一パスで処理するため、各フラグメントで深度から
// ぼけ半径を直接求め、eye_strain.frag / photophobia.frag と同じ Fibonacci lattice
// 16 tap で円盤（pillbox）を近似サンプリングする。アルゴリズムが異なるため
// CPU との bit / PSNR 等価は取らず、効果（ピント面は鮮明・離れるほどぼける・
// kind による前後選択）を shader_equivalence の sim ミラーで検証する。
//
// 深度マップは grayscale（CPU は to_luma8）を想定し、ここでは .r を深度として読む。
// 明るい（1.0）= 近い、暗い（0.0）= 遠い。

uniform sampler2D uTexture;
uniform sampler2D uDepth;
uniform float uFocusDepth;   // ピント深度（0.0..=1.0）
uniform float uMaxRadiusPx;  // 最大ぼけ半径（px）= max_radius_ratio * min(W,H)
uniform int uKind;           // 0=Myopia(遠方ボケ), 1=Hyperopia(近方ボケ), 2=DepthOfField(両側)
uniform vec2 uTexelSize;     // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

// blur が視認できない最小半径（CPU の MIN_BLUR_RADIUS_PX と一致）
const float kMinRadiusPx = 0.5;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 c = texture(uTexture, vTexCoord);
    float depth = texture(uDepth, vTexCoord).r;
    float delta = depth - uFocusDepth;

    float radiusPx;
    if (uKind == 0) {
        // Myopia: 遠方（depth < focus, delta < 0）がボケる
        radiusPx = delta < 0.0 ? (-delta) * uMaxRadiusPx : 0.0;
    } else if (uKind == 1) {
        // Hyperopia: 近方（depth > focus, delta > 0）がボケる
        radiusPx = delta > 0.0 ? delta * uMaxRadiusPx : 0.0;
    } else {
        // DepthOfField: 両側がボケる
        radiusPx = abs(delta) * uMaxRadiusPx;
    }

    if (radiusPx < kMinRadiusPx) {
        // ピント面付近は鮮明（blur なし）
        fragColor = c;
        return;
    }

    // Fibonacci lattice 16 tap で円盤内を均等サンプリングし linear 空間で平均する。
    const int N = 16;
    const float PHI = 2.399963229728653; // 黄金角 ≈ 137.508°
    vec3 acc = vec3(0.0);
    for (int i = 0; i < N; i++) {
        float ft = float(i) / float(N);
        float r = sqrt(ft) * radiusPx;
        float theta = float(i) * PHI;
        vec2 offset = vec2(cos(theta), sin(theta)) * r * uTexelSize;
        vec3 s = texture(uTexture, vTexCoord + offset).rgb;
        acc += vec3(srgbToLinear(s.r), srgbToLinear(s.g), srgbToLinear(s.b));
    }
    vec3 result = acc / float(N);

    fragColor = vec4(linearToSrgb(result.r), linearToSrgb(result.g), linearToSrgb(result.b), c.a);
}
