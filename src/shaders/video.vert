#version 300 es
// One quad from gl_VertexID, so no vertex buffer is needed. `rect` is the destination in NDC.
uniform vec4 rect;
uniform int rot;  // quarter turns counter-clockwise (libretro SET_ROTATION)
out vec2 v;
void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    vec2 d = vec2(p.x, 1.0 - p.y);  // position on screen, 0..1, y down
    if (rot == 1) v = vec2(1.0 - d.y, d.x);
    else if (rot == 2) v = 1.0 - d;
    else if (rot == 3) v = vec2(d.y, 1.0 - d.x);
    else v = d;
    gl_Position = vec4(mix(rect.xy, rect.zw, p), 0.0, 1.0);
}
