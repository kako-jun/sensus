#version 300 es
precision highp float;

// 飛蚊症（Floaters）シミュレーション — 方針 B（#134）。
//
// CPU 実装 vision::floaters_mask が生成した u8 マスクを uMask テクスチャ（.r）として
// 受け取り、linear sRGB 空間で乗算ブレンドする。マスク生成（円形 blob + 乱歩ストランド
// + 3×3 box blur）はライブラリ側で行い、本シェーダはブレンドのみを担う。
//
// これにより CPU vision::floaters と **bit 一致**する（同じ u8 マスク・同じブレンド式
// `1 - strength*(1-mask)`・同じ linear sRGB 乗算）。マスクは strength 非依存なので、
// host は density/seed/gaze ごとに 1 回マスクを生成・アップロードすれば strength は
// uniform で可変にできる。depth_aware_blur の uDepth と同じ「第2テクスチャ」パターン。
//
// 旧実装はブロック hash による別モデルの近似で CPU と乖離していた（#134 で解消）。

uniform sampler2D uTexture;
uniform sampler2D uMask;   // .r = フローターマスク（0 = 不透明フローター .. 1 = 透明）
uniform float uStrength;

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 orig = texture(uTexture, vTexCoord);
    float mask = texture(uMask, vTexCoord).r;
    float blend = 1.0 - uStrength * (1.0 - mask);
    vec3 lin = vec3(srgbToLinear(orig.r), srgbToLinear(orig.g), srgbToLinear(orig.b));
    lin *= blend;
    fragColor = vec4(linearToSrgb(lin.r), linearToSrgb(lin.g), linearToSrgb(lin.b), orig.a);
}
