#version 300 es
// Screen frame (see src/bezel.rs): drawn behind the picture on a black screen, so premultiplied
// colour is all that is needed and no blending is involved.
precision mediump float;
uniform mediump sampler2D tex;
in vec2 v;
out vec4 o;
void main() {
    o = vec4(texture(tex, v).rgb, 1.0);
}
