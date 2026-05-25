#version 300 es
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
in vec2 vTexCoord;
out vec4 fragColor;

// 簡易ハッシュ関数（シェーダ内ノイズ生成用）
float hash(vec2 p) {
    p = fract(p * vec2(127.1, 311.7));
    p += dot(p, p + 19.19);
    return fract(p.x * p.y);
}

void main() {
    // タイル座標（32x32 タイル想定）でノイズ値を決定
    vec2 tile_coord = floor(vTexCoord * 16.0); // 画面を 16 分割
    float noise = hash(tile_coord + vec2(42.0, 42.0));
    float blur_amount = noise * uStrength;

    // 近傍サンプルの平均でソフトブラーを近似
    vec2 texel_size = vec2(1.0) / vec2(textureSize(uTexture, 0));
    vec4 color = vec4(0.0);
    float total = 0.0;
    float r = blur_amount * 2.0;
    for (int dy = -2; dy <= 2; dy++) {
        for (int dx = -2; dx <= 2; dx++) {
            float dist = length(vec2(float(dx), float(dy)));
            if (dist <= r + 0.5) {
                vec2 offset = vec2(float(dx), float(dy)) * texel_size;
                color += texture(uTexture, vTexCoord + offset);
                total += 1.0;
            }
        }
    }
    if (total > 0.0) {
        color /= total;
    } else {
        color = texture(uTexture, vTexCoord);
    }
    fragColor = vec4(color.rgb, texture(uTexture, vTexCoord).a);
}
