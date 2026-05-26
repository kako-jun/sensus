#version 300 es
precision highp float;
precision highp int;

// 歪視（Metamorphopsia）シミュレーション。
// CPU 実装 vision::metamorphopsia と同一のノイズモデル（#99 で統一）。
//
// ## アルゴリズム
// 画像を cell_size ピクセルの仮想グリッドに分割し、各グリッド頂点に
// 32bit 整数 spatial hash で擬似ランダムな変位 (dx, dy) を割り当てる。
// 各フラグメントが属するセルの 4 頂点の変位を双線形補間し、その変位で
// サンプリング座標を移動して元画像をサンプリングする（エッジは clamp）。
//
// ## CPU との一致根拠
// - `gridHash` は CPU の `grid_hash` と完全に同じ 32bit 整数演算（GLSL の uint は
//   CPU の u32 と同じく mod 2^32 で wrap する）。同じ (seed, gx, gy, axis) から
//   bit 単位に同じ 0..1 値を返す。float 経由の精度損失を避けるため uSeed は uint。
// - グリッド頂点インデックス gx0/gy0 は CPU の整数ピクセル座標基準 (x/cell_size の
//   floor) に合わせるため、フラグメント中心 uv からピクセル座標を
//   `vTexCoord * resolution - 0.5` で復元する。
// - 唯一の乖離源はサンプリング段の浮動小数丸めのみ。変位場は CPU と一致する。

uniform sampler2D uTexture;
uniform float uStrength;   // 0.0..=1.0
uniform float uFreq;       // 空間周波数（グリッド分割数）
uniform uint  uSeed;       // ランダムシード（uint で精度損失なく渡す）
uniform vec2  uTexelSize;  // vec2(1.0/width, 1.0/height)

in vec2 vTexCoord;
out vec4 fragColor;

// 1 頂点・1 軸ぶんの 32bit ハッシュ（axis: 0=dx, 1=dy）。0.0..=1.0 を返す。
// CPU 実装 vision::metamorphopsia の grid_hash と完全に同じ系列。
float gridHash(uint gx, uint gy, uint axis) {
    uint h = uSeed * 0x9e3779b9u
        + gx * 0x85ebca6bu
        + gy * 0xc2b2ae35u
        + axis * 0x27d4eb2fu;
    h ^= h >> 15;
    h *= 0x2c1b3c6du;
    h ^= h >> 12;
    h *= 0x297a2d39u;
    h ^= h >> 15;
    return float(h) / float(0xFFFFFFFFu); // 0.0..=1.0
}

void main() {
    if (uStrength <= 0.0) {
        fragColor = texture(uTexture, vTexCoord);
        return;
    }

    // 最大変位量（ピクセル）。CPU の MAX_DISPLACEMENT_PX と同値。
    const float MAX_DISP_PX = 8.0;
    float maxDisp = uStrength * MAX_DISP_PX;

    // 解像度を texel size から復元（width = 1/uTexelSize.x）。
    vec2 resolution = vec2(1.0) / uTexelSize;
    float minDim = min(resolution.x, resolution.y);

    // グリッドセルサイズ（CPU と同じ: max(min_dim / clamp(freq), 1.0)）。
    float freqClamped = clamp(uFreq, 0.1, 1000.0);
    float cellSize = max(minDim / freqClamped, 1.0);

    // CPU の整数ピクセル座標 x（top-left 規約）を復元する。
    vec2 pixelPos = vTexCoord * resolution - 0.5;
    vec2 cell = pixelPos / cellSize;
    vec2 ci = floor(cell);
    float tx = cell.x - ci.x; // 0.0..=1.0 のセル内位置
    float ty = cell.y - ci.y;

    // グリッド頂点インデックス（CPU と同じく非負。floor は負になり得ないが念のため max）。
    uint gx0 = uint(max(ci.x, 0.0));
    uint gy0 = uint(max(ci.y, 0.0));
    uint gx1 = gx0 + 1u;
    uint gy1 = gy0 + 1u;

    // 4 頂点の変位 [-1,1]*maxDisp を双線形補間（CPU と同じ重み）。
    vec2 d00 = vec2(gridHash(gx0, gy0, 0u), gridHash(gx0, gy0, 1u)) * 2.0 - 1.0;
    vec2 d10 = vec2(gridHash(gx1, gy0, 0u), gridHash(gx1, gy0, 1u)) * 2.0 - 1.0;
    vec2 d01 = vec2(gridHash(gx0, gy1, 0u), gridHash(gx0, gy1, 1u)) * 2.0 - 1.0;
    vec2 d11 = vec2(gridHash(gx1, gy1, 0u), gridHash(gx1, gy1, 1u)) * 2.0 - 1.0;

    vec2 disp = (d00 * (1.0 - tx) * (1.0 - ty)
               + d10 * tx * (1.0 - ty)
               + d01 * (1.0 - tx) * ty
               + d11 * tx * ty) * maxDisp;

    // 変位後のピクセル座標（CPU と同じく clamp でエッジ処理）→ uv に戻す。
    vec2 sampledPx = clamp(pixelPos + disp, vec2(0.0), resolution - 1.0);
    vec2 sampledUv = (sampledPx + 0.5) * uTexelSize;
    fragColor = texture(uTexture, sampledUv);
}
