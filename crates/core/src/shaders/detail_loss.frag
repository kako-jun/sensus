#version 300 es
// 細部消失（Detail Loss）シミュレーション。
// 各ピクセルを所属タイルの中心点の色で置き換えることで、
// のっぺりとした塊に見える視覚的効果を実現する（pixelation）。
// CPU 実装（vision::detail_loss / vision::detail_loss_with_cell_size）と同一アルゴリズム。
// kako-jun/sensus#96: 以前は detail_loss_with_cell_size が全平均でこのシェーダと異なっていたが、
// 中心点サンプリングに統一済み。apply(Filter::DetailLoss) 経路もこのシェーダと等価。
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
uniform vec2 uResolution;
in vec2 vTexCoord;
out vec4 fragColor;

// sRGB -> linear
float srgb_to_linear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
// linear -> sRGB
float linear_to_srgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    // tile_size = (strength * 20.0).max(1.0)
    float tile_size = max(uStrength * 20.0, 1.0);

    // UV をタイル境界にスナップし、タイル中心1点をサンプリング
    vec2 px = vTexCoord * uResolution;
    vec2 tile_origin = floor(px / tile_size) * tile_size;
    vec2 center_px = tile_origin + tile_size * 0.5;
    vec2 center_uv = clamp(center_px / uResolution, 0.0, 1.0);

    vec4 s = texture(uTexture, center_uv);
    vec3 lin = vec3(srgb_to_linear(s.r), srgb_to_linear(s.g), srgb_to_linear(s.b));
    vec4 orig = texture(uTexture, vTexCoord);
    fragColor = vec4(linear_to_srgb(lin.r), linear_to_srgb(lin.g), linear_to_srgb(lin.b), orig.a);
}
