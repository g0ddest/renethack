// A living creature's edge (art.rs: the next pass of the materials it
// takes its rim in): a soft cold light round its outline, brightest along
// its top, added over whatever lights it, so a dark one reads on a dark
// floor (a homunculus in a dark corridor, a manes on Gehennom's floor). A
// light's own rim is as dark as the creature it falls on.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_back, shadows_disabled, fog_disabled;

uniform vec3 color : source_color = vec3(0.62, 0.70, 1.0);
uniform float strength = 0.35;
// how far from facing the eye the edge begins (0 facing, 1 side-on)
uniform float from = 0.4;

void fragment() {
	vec3 n = normalize(NORMAL);
	float edge = smoothstep(from, 0.95, 1.0 - clamp(dot(n, normalize(VIEW)), 0.0, 1.0));
	float up = (INV_VIEW_MATRIX * vec4(n, 0.0)).y;
	ALBEDO = color * strength * edge * (0.55 + 0.45 * clamp(up + 0.3, 0.0, 1.0));
}
