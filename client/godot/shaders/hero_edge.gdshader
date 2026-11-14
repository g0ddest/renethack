// The hero's edge (art.rs `HERO_EDGE_SHADER`): creature_edge.gdshader's
// soft rim, and where her body, what she wears or what she holds is the
// nearest thing it leaves 4 in the stencil, so that her x-ray
// (xray.gdshader) draws nowhere they are: her own shield held before her
// or her cloak seen from behind never sets it off; a wall before her
// leaves no mark there.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_back, shadows_disabled, fog_disabled;
stencil_mode write, compare_always, 4;

#include "res://shaders/creature_edge.gdshaderinc"
