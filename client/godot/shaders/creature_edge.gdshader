// A living creature's edge (art.rs: the next pass of the materials it
// takes its rim in): a soft cold light round its outline, brightest along
// its top, added over whatever lights it, so a dark one reads on a dark
// floor (a homunculus in a dark corridor, a manes on Gehennom's floor). A
// light's own rim is as dark as the creature it falls on. A thing on the
// floor takes it too, in dark gold, with a glint that travels over it now
// and then (`sweep`), both stronger the farther off it is seen (`far`).
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_back, shadows_disabled, fog_disabled;

#include "res://shaders/creature_edge.gdshaderinc"
