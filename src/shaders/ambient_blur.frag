#version 300 es
// Ambient frame, step 2 (src/ambient.rs): one direction of a Gaussian blur on the tiny image,
// 9 taps folded into 5 bilinear ones. Texel for texel, so the image keeps its orientation.
precision mediump float;
uniform mediump sampler2D tex;
uniform vec2 dir;  // one step in texture coordinates, along x or y
uniform vec2 tsize;
out vec4 o;
void main() {
    vec2 uv = gl_FragCoord.xy / tsize;
    vec3 c = texture(tex, uv).rgb * 0.2270270270;
    c += texture(tex, uv + dir * 1.3846153846).rgb * 0.3162162162;
    c += texture(tex, uv - dir * 1.3846153846).rgb * 0.3162162162;
    c += texture(tex, uv + dir * 3.2307692308).rgb * 0.0702702703;
    c += texture(tex, uv - dir * 3.2307692308).rgb * 0.0702702703;
    o = vec4(c, 1.0);
}
