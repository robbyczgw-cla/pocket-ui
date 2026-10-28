#version 300 es
// Ambient frame, step 1 (src/ambient.rs): box-filters the game picture down to the tiny image,
// 4x4 taps over each tiny texel's footprint. Blended into the running average by the host
// (constant alpha), so the result is smoothed over time.
precision mediump float;
uniform mediump sampler2D tex;
uniform int swizzle;  // 1: XRGB8888 uploaded as RGBA, bytes are B, G, R
uniform vec2 fp;      // footprint of one tiny texel, in source texture coordinates
in vec2 v;
out vec4 o;
void main() {
    vec3 c = vec3(0.0);
    for (int j = 0; j < 4; j++) {
        for (int i = 0; i < 4; i++) {
            c += texture(tex, v + fp * (vec2(float(i), float(j)) - 1.5) / 4.0).rgb;
        }
    }
    c /= 16.0;
    if (swizzle == 1) c = c.bgr;
    o = vec4(c, 1.0);
}
