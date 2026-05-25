#version 300 es
precision mediump float;
uniform sampler2D u_image;
uniform float u_strength;
in vec2 v_texcoord;
out vec4 out_color;
void main() {
    vec4 c = texture(u_image, v_texcoord);
    // contrast compression
    vec3 compressed = vec3(0.5) + (c.rgb - vec3(0.5)) * (1.0 - u_strength * 0.15);
    // vignette
    vec2 uv = v_texcoord * 2.0 - 1.0;
    float d = dot(uv, uv);
    float vignette = 1.0 - u_strength * 0.3 * smoothstep(0.3, 1.2, d);
    out_color = vec4(clamp(compressed * vignette, 0.0, 1.0), c.a);
}
