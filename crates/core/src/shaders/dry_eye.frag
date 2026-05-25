#version 300 es
precision mediump float;
uniform sampler2D u_image;
uniform float u_strength;
in vec2 v_texcoord;
out vec4 out_color;

// 簡易ハッシュ関数（シェーダ内ノイズ生成用）
float hash(vec2 p) {
    p = fract(p * vec2(127.1, 311.7));
    p += dot(p, p + 19.19);
    return fract(p.x * p.y);
}

void main() {
    // タイル座標（32x32 タイル想定）でノイズ値を決定
    vec2 tile_coord = floor(v_texcoord * 16.0); // 画面を 16 分割
    float noise = hash(tile_coord + vec2(42.0, 42.0));
    float blur_amount = noise * u_strength;

    // 近傍サンプルの平均でソフトブラーを近似
    vec2 texel_size = vec2(1.0) / vec2(textureSize(u_image, 0));
    vec4 color = vec4(0.0);
    float total = 0.0;
    float r = blur_amount * 2.0;
    for (int dy = -2; dy <= 2; dy++) {
        for (int dx = -2; dx <= 2; dx++) {
            float dist = length(vec2(float(dx), float(dy)));
            if (dist <= r + 0.5) {
                vec2 offset = vec2(float(dx), float(dy)) * texel_size;
                color += texture(u_image, v_texcoord + offset);
                total += 1.0;
            }
        }
    }
    if (total > 0.0) {
        color /= total;
    } else {
        color = texture(u_image, v_texcoord);
    }
    out_color = vec4(color.rgb, texture(u_image, v_texcoord).a);
}
