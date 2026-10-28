#version 300 es
// Ambient frame, step 3 (src/ambient.rs), run with the tiny passes only: the whole screen's glow
// at a third of its resolution (it is smooth). The blurred tiny image is upscaled with a cubic
// B-spline (four bilinear taps, without the creases of plain bilinear), dimmed and a little more
// saturated, darker next to the picture and towards the panel's edges.
precision highp float;
uniform mediump sampler2D tex;
uniform vec2 tsize;   // tiny image size in texels
uniform vec2 view;    // screen size in pixels
uniform vec4 game;    // picture rectangle in screen pixels, top-left origin: x0, y0, x1, y1
uniform vec4 cover;   // where the tiny image lies on the screen: x, y, w, h
in vec2 v;            // 0..1 over the screen, top-left origin
out vec4 o;

vec3 bspline(vec2 uv) {
    vec2 t = uv * tsize - 0.5;
    vec2 i = floor(t);
    vec2 f = t - i;
    vec2 f2 = f * f, f3 = f2 * f;
    vec2 w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
    vec2 w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
    vec2 w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
    vec2 w3 = f3 / 6.0;
    vec2 g0 = w0 + w1, g1 = w2 + w3;
    vec2 h0 = (i - 0.5 + w1 / g0) / tsize;
    vec2 h1 = (i + 1.5 + w3 / g1) / tsize;
    return g0.y * (g0.x * texture(tex, vec2(h0.x, h0.y)).rgb + g1.x * texture(tex, vec2(h1.x, h0.y)).rgb)
         + g1.y * (g0.x * texture(tex, vec2(h0.x, h1.y)).rgb + g1.x * texture(tex, vec2(h1.x, h1.y)).rgb);
}

void main() {
    vec2 p = v * view;
    vec2 uv = (p - cover.xy) / cover.zw;
    // The tiny image is stored bottom-up.
    vec3 c = bspline(vec2(uv.x, 1.0 - uv.y));
    float l = dot(c, vec3(0.299, 0.587, 0.114));
    c = max(mix(vec3(l), c, 1.5), 0.0);
    // Dim, more so for bright scenes, so a white screen glows instead of blinding.
    c *= 0.56 / (1.0 + 0.7 * l);
    // A soft shadow around the picture keeps its edge crisp.
    vec2 d = max(max(game.xy - p, p - game.zw), 0.0);
    c *= mix(0.3, 1.0, smoothstep(0.0, 26.0, length(d)));
    // Falls off towards the panel's edges: a product, so the corners round off without a mitre.
    vec2 e = smoothstep(vec2(0.0), vec2(120.0), min(p, view - p));
    c *= mix(0.25, 1.0, e.x * e.y);
    o = vec4(c, 1.0);
}
