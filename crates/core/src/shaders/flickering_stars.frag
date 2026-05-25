#version 300 es
precision mediump float;
uniform sampler2D uTexture;
uniform float uStrength;
uniform uint uSeed;
uniform vec2 uResolution;
in vec2 vTexCoord;
out vec4 fragColor;

// uint ベースのハッシュ（精度劣化なし）
// Wang hash
uint hash_uint(uint n) {
    n = (n ^ 61u) ^ (n >> 16u);
    n *= 9u;
    n = n ^ (n >> 4u);
    n *= 0x27d4eb2du;
    n = n ^ (n >> 15u);
    return n;
}

// [0, 1) の float に変換
float hash_to_float(uint h) {
    return float(h & 0x00ffffffu) / float(0x01000000u);
}

// sRGB -> linear
float srgb_to_linear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
// linear -> sRGB
float linear_to_srgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 c = texture(uTexture, vTexCoord);
    vec3 lin = vec3(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));

    float count = uStrength * 200.0;
    float blob_radius = 2.0;
    float brightness_add = 0.0;

    vec2 px = vTexCoord * uResolution;

    for (int i = 0; i < 200; i++) {
        if (float(i) >= count) break;
        uint ui = uint(i);
        // uSeed * 1000u: 各光点のハッシュ入力を seed でシフトする定数倍。
        // uint 演算なのでラップアラウンド（オーバーフロー）は意図的な動作。
        uint h1 = hash_uint(ui + uSeed * 1000u);
        uint h2 = hash_uint(ui + uSeed * 1000u + 7654u);
        uint h3 = hash_uint(ui + uSeed * 1000u + 9876u);
        float fx = hash_to_float(h1) * uResolution.x;
        float fy = hash_to_float(h2) * uResolution.y;
        float dist = length(px - vec2(fx, fy));
        if (dist <= blob_radius) {
            brightness_add += 0.5 + hash_to_float(h3) * 0.5;
        }
    }

    vec3 result = clamp(lin + vec3(brightness_add), 0.0, 1.0);
    fragColor = vec4(linear_to_srgb(result.r), linear_to_srgb(result.g), linear_to_srgb(result.b), c.a);
}
