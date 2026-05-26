#version 300 es
precision mediump float;

// 緑内障（glaucoma）シミュレーション
//
// uMode で 2 系統のマスクを切り替える:
//   0 = Vignette        : 中心保存 + 周辺 smoothstep 暗化（後方互換）
//   1 = ArcuateSuperior : 上方 Bjerrum 弧状暗点（極座標マスク）
//   2 = ArcuateInferior : 下方 Bjerrum 弧状暗点
//   3 = Biarcuate       : 上下両方の弧状暗点
// 値は vision::GlaucomaMode の判別値（CLI/pipeline 側）と 1 対 1 対応する。

uniform sampler2D uTexture;
uniform float uStrength;
uniform float uAspect;  // width / height。非正方形画像の aspect 補正に使用。
uniform int uMode;      // 0=Vignette, 1=ArcuateSuperior, 2=ArcuateInferior, 3=Biarcuate

in vec2 vTexCoord;
out vec4 fragColor;

float srgbToLinear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}

float linearToSrgb(float c) {
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// Vignette モードの暗化係数（mul）。
// vision::glaucoma の Vignette と同一の定数・smoothstep。
float vignetteMul() {
    vec2 uv = vTexCoord - vec2(0.5, 0.5);
    vec2 uvA = vec2(uv.x * uAspect, uv.y);
    float cornerDist = sqrt(0.5 * uAspect * 0.5 * uAspect + 0.5 * 0.5);
    float d = length(uvA) / cornerDist;

    float inner_r = 1.0 - uStrength * 0.7;
    float outer_r = min(inner_r + 0.2, 1.0);

    float t = clamp((d - inner_r) / (outer_r - inner_r), 0.0, 1.0);
    float fade = t * t * (3.0 - 2.0 * t); // smoothstep
    return 1.0 - uStrength * fade;
}

// 弧状暗点モードの暗化係数（mul）。
// vision::glaucoma の ArcuateSuperior/Inferior/Biarcuate を width 正規化座標で
// 1 対 1 にミラーする。CPU 実装は pixel 座標 (x, y) で計算するが、全項を画像幅 w
// で割っても比は保たれる:
//   dx_px = x - (cx + w*0.15) = w*(u - 0.65)  → dxN = u - 0.65
//   dy_px = y - cy            = h*(v - 0.5)    → dyN = (v - 0.5) / uAspect
//   min_dim/w = min(w,h)/w    = min(1.0, 1.0/uAspect)
// atan2(dy_px, dx_px) は dxN/dyN の比が保たれるため不変。
float arcuateMul(bool applySuperior, bool applyInferior) {
    float u = vTexCoord.x;
    float v = vTexCoord.y;

    // ON head（視神経乳頭）からの width 正規化ベクトル
    float dxN = u - 0.65;          // (cx + w*0.15) / w = 0.65
    float dyN = (v - 0.5) / uAspect; // h*(v-0.5)/w = (v-0.5)/aspect
    float rN = length(vec2(dxN, dyN));

    float minDimN = min(1.0, 1.0 / uAspect); // min(w,h)/w
    float rMin = minDimN * 0.20;
    float rMax = minDimN * 0.55 * sqrt(uStrength); // strength で外側境界が拡大

    // 弧状帯の外なら暗化なし。境界包含は CPU 正本（r < rMin || r > rMax、band は
    // [rMin, rMax] 閉区間）と一致させる。strength≈0.133 付近で rMax≈rMin になりうるが、
    // その場合 band が潰れて早期 return するため、ゼロ除算・NaN は発生しない。
    if (rN < rMin || rN > rMax) {
        return 1.0;
    }

    float tR = (rN - rMin) / (rMax - rMin);
    float fadeR = tR * tR * (3.0 - 2.0 * tR); // smoothstep
    float fadeRadial = 1.0 - abs(fadeR * 2.0 - 1.0); // 帯中央で最大

    // 角度条件: dyN < 0 が画像上方（superior）、dyN > 0 が下方（inferior）
    bool inSuperior = dyN < 0.0;
    bool inInferior = dyN > 0.0;
    bool inArc = (applySuperior && inSuperior) || (applyInferior && inInferior);
    if (!inArc) {
        return 1.0;
    }

    // ON head に近い角度（x 軸付近）では弧状暗点が弱くなる。
    float theta = atan(dyN, dxN); // -π..=π
    float arcFade = clamp(sqrt(abs(sin(theta))), 0.0, 1.0);

    float fade = uStrength * fadeRadial * arcFade;
    return 1.0 - fade;
}

void main() {
    vec4 src = texture(uTexture, vTexCoord);

    float mul;
    if (uMode == 0) {
        mul = vignetteMul();
    } else {
        bool applySuperior = (uMode == 1) || (uMode == 3);
        bool applyInferior = (uMode == 2) || (uMode == 3);
        mul = arcuateMul(applySuperior, applyInferior);
    }

    float rl = srgbToLinear(src.r);
    float gl = srgbToLinear(src.g);
    float bl = srgbToLinear(src.b);

    fragColor = vec4(
        linearToSrgb(clamp(rl * mul, 0.0, 1.0)),
        linearToSrgb(clamp(gl * mul, 0.0, 1.0)),
        linearToSrgb(clamp(bl * mul, 0.0, 1.0)),
        src.a
    );
}
