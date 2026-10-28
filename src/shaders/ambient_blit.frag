#version 300 es
// Ambient frame, every frame (src/ambient.rs): the glow composed at the last sample, one bilinear
// tap per pixel on the bands around the picture, dithered at full resolution (the dark gradients
// would band on the OLED panel).
precision highp float;
uniform mediump sampler2D tex;
in vec2 v;  // 0..1 over the screen, top-left origin
out vec4 o;
void main() {
    vec3 c = texture(tex, vec2(v.x, 1.0 - v.y)).rgb;
    float n = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    o = vec4(c + (n - 0.5) / 255.0, 1.0);
}
