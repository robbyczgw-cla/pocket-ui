#version 300 es
// Variants are picked by the host with defines inserted after the version line (`video.rs`), followed by the
// constants of `picture_fx.rs` (GAMMA, BEAM_*, MASK, HALO*, LCD_*, GBA_*):
//   SHARPNESS n  0 sharp (NEAREST), 1 crisp, 2 soft (LINEAR)
//   EFFECT n     0 none, 1 LCD grid, 2 scanlines, 3 CRT
//   LCD_COLOUR   the Game Boy Advance screen's colours (with EFFECT 1)
// Every effect works from the picture's own pixel and line positions, so it stays locked to the game at any
// scale, and mixes light in linear light (decode, mix, encode). One pass:
//   none, sharp/soft: 1 tap; crisp: 4 texel fetches; LCD: as its sharpness; scanlines: 2 taps; CRT: 3 taps.
precision highp float;
uniform mediump sampler2D tex;
uniform vec2 src;     // source size in texels
uniform vec2 scale;   // output pixels per source texel, per axis
uniform int swizzle;  // 1: texture holds XRGB8888 uploaded as RGBA, so bytes are B, G, R
uniform float lines;  // scanlines (EFFECT 2, 3): src.y, or the console's lines for a 3D core rendering above them
in vec2 v;
out vec4 o;

#ifndef EFFECT
#define EFFECT 0
#endif
#ifndef SHARPNESS
#define SHARPNESS 0
#endif

vec3 sw(vec3 c) { return swizzle == 1 ? c.bgr : c; }
// Linear light with gamma 2 (a square and a square root: cheap, and within a few per cent of the panel's 2.2 for
// mixing; `picture_fx::GAMMA`).
vec3 lin(vec3 c) { return c * c; }
vec3 enc(vec3 c) { return sqrt(clamp(c, 0.0, 1.0)); }

// Crisp: the far texel's weight for a position t (0..1) between two texel centres at s output pixels per texel.
// Flat texels, a ramp one output pixel wide around their edge; exactly 0 or 1 at whole-number scales.
vec2 crisp_w(vec2 t, vec2 s) { return clamp((t - 0.5) * max(s, vec2(1.0)) + 0.5, 0.0, 1.0); }

#if EFFECT < 2
// The game's colour at this pixel, encoded, by sharpness.
vec3 base() {
#if SHARPNESS == 1
    // Crisp in linear light: the texture unit would blend
    // the encoded values, which makes a bright pixel on a dark ground lighter or heavier with its phase.
    vec2 p = v * src - 0.5;
    vec2 i = floor(p);
    vec2 w = crisp_w(p - i, scale);
    ivec2 a = ivec2(i), m = ivec2(src) - 1;
    vec3 c00 = texelFetch(tex, clamp(a, ivec2(0), m), 0).rgb;
    vec3 c10 = texelFetch(tex, clamp(a + ivec2(1, 0), ivec2(0), m), 0).rgb;
    vec3 c01 = texelFetch(tex, clamp(a + ivec2(0, 1), ivec2(0), m), 0).rgb;
    vec3 c11 = texelFetch(tex, clamp(a + ivec2(1, 1), ivec2(0), m), 0).rgb;
    vec3 top = mix(lin(c00), lin(c10), w.x);
    vec3 bot = mix(lin(c01), lin(c11), w.x);
    return sw(enc(mix(top, bot, w.y)));
#else
    return sw(texture(tex, v).rgb);
#endif
}
#endif

#if EFFECT == 1
// Share of this output pixel the LCD gaps cover, per axis: gaps LCD_GAP of a texel wide at each texel's start.
vec2 gap_cover(vec2 p, vec2 s) {
    vec2 hp = 0.5 / s;
    vec2 a = p - hp, b = p + hp;
    vec2 k = floor(a);
    vec2 o1 = max(min(b, k + LCD_GAP) - max(a, k), 0.0);
    vec2 o2 = max(min(b, k + 1.0 + LCD_GAP) - max(a, k + 1.0), 0.0);
    return min((o1 + o2) * s, 1.0);
}
#endif

#if EFFECT >= 2
// Antiderivative of the beam's soft edge: 0 below -BEAM_EDGE/2, 1 above BEAM_EDGE/2, linear between.
vec3 ramp_integral(vec3 t) {
    vec3 q = clamp(t + 0.5 * BEAM_EDGE, 0.0, BEAM_EDGE);
    return q * q / (2.0 * BEAM_EDGE) + max(t - 0.5 * BEAM_EDGE, 0.0);
}
// One scanline's beam seen by this output pixel, whose centre is d lines from the line's centre and which is h
// lines tall: a plateau with soft edges, its half-height w following each channel's brightness (a TV's three
// guns), averaged exactly over the pixel and divided by its area (2w), so each line gives off the light of its
// colour whatever its height and whatever the scale, and the plateau never needs more than full brightness.
vec3 beam(vec3 e, float d, float h) {
    vec3 w = mix(vec3(BEAM_DARK), vec3(BEAM_BRIGHT), e * e);
    float a = d - 0.5 * h, b = d + 0.5 * h;
    vec3 ga = ramp_integral(w + a) - ramp_integral(w - a);
    vec3 gb = ramp_integral(w + b) - ramp_integral(w - b);
    return (gb - ga - h) / (h * 2.0 * w);
}
#endif

void main() {
#if EFFECT == 0
    o = vec4(base(), 1.0);
#elif EFFECT == 1
    vec3 e = base();
#ifdef LCD_COLOUR
    // Pokefan531's GBA screen profile (public domain): darker decoding, then the screen's colour mix.
    vec3 c = clamp(GBA_MATRIX * min(pow(e, vec3(GBA_GAMMA + GBA_DARKEN)) * GBA_LUM, 1.0), 0.0, 1.0);
    c = pow(c, vec3(GAMMA / GBA_GAMMA));  // from the profile's gamma 2.2 to ours
#else
    vec3 c = lin(e);
#endif
    vec2 cv = gap_cover(v * src, scale);
    float cover = cv.x + cv.y - cv.x * cv.y;
    float fade = smoothstep(LCD_FADE_LO, LCD_FADE_HI, min(scale.x, scale.y));
    c *= 1.0 + fade * (LCD_GAIN - 1.0 - LCD_DARK * cover * LCD_GAIN);
    o = vec4(enc(c), 1.0);
#else
    // Scanlines in the picture's line space: line k's centre at y = k, this pixel between lines k0 and k0 + 1.
    float y = v.y * lines - 0.5;
    float k0 = floor(y);
    float d0 = y - k0;
    float px = scale.y * src.y / lines;  // output pixels per line
    float h = 1.0 / max(px, 0.25);       // and one pixel's height in lines
    // Across a line the beam is soft like a TV's: crisp steering for Sharp and Crisp (no uneven columns), plain
    // bilinear for Soft. The texture is LINEAR for these effects.
    float x = v.x * src.x;
#if SHARPNESS != 2
    float xp = x - 0.5;
    float xi = floor(xp);
    x = xi + 0.5 + crisp_w(vec2(xp - xi), scale).x;
#endif
    float u = x / src.x;
    vec3 e0 = k0 < 0.0 ? vec3(0.0) : sw(textureLod(tex, vec2(u, (k0 + 0.5) / lines), 0.0).rgb);
    vec3 e1 = k0 + 1.0 >= lines ? vec3(0.0) : sw(textureLod(tex, vec2(u, (k0 + 1.5) / lines), 0.0).rgb);
    vec3 c0 = lin(e0), c1 = lin(e1);
    vec3 lit = c0 * beam(e0, d0, h) + c1 * beam(e1, d0 - 1.0, h);
    vec3 c = mix(d0 < 0.5 ? c0 : c1, lit, smoothstep(SCAN_FADE_LO, SCAN_FADE_HI, px));
#if EFFECT == 3
    // Glow: a little of a blurred picture: the mip level about HALO_LINES texels tall, each of its texels 1/28 of
    // the picture's height (8 lines of a SNES picture).
    float lod = max(log2(src.y / HALO_LINES), 0.0);
    c = mix(c, lin(sw(textureLod(tex, v, lod).rgb)), HALO);
    // Mask: magenta and green columns, two output pixels a pair (0.16 mm: beyond the eye at arm's length, and in
    // step with an OLED's subpixels where they come in pairs); a white pair averages white.
    float g = 1.0 / (1.0 - 0.5 * MASK);
    c *= mod(gl_FragCoord.x, 2.0) < 1.0 ? vec3(g, (1.0 - MASK) * g, g) : vec3((1.0 - MASK) * g, g, (1.0 - MASK) * g);
#endif
    o = vec4(enc(c), 1.0);
#endif
}
