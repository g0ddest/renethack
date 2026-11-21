// A creature's lit value capped (art.rs: a pass after an opaque material
// of its): whatever lights it, torch, the hero's pool or the level's fire,
// its lit side tops out and keeps its hue (the light's own colour partly
// taken out of it), where a pale coat under a torch burned out to cream.
// It reads what the frame holds of the creature, lit, and adds the
// difference to what it should be: no light is told from another, and a
// shadow stays a shadow.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform sampler2D lit_tex : hint_screen_texture, filter_nearest, repeat_disable;
// untouched up to x, never over y (the frame's linear light)
global uniform vec2 creature_knee;
// its lit side multiplied by this (the branch's torchlight colour partly
// taken out), its dark side left as it is
global uniform vec3 creature_balance;

void fragment() {
	vec3 under = max(texture(lit_tex, SCREEN_UV).rgb, vec3(0.0));
	float from = creature_knee.x;
	float span = max(creature_knee.y - from, 0.001);
	float lit = max(under.r, max(under.g, under.b));
	vec3 want = under * mix(vec3(1.0), creature_balance, smoothstep(from * 0.4, from, lit));
	float top = max(want.r, max(want.g, want.b));
	if (top > from) {
		want *= (from + span * (1.0 - exp(-(top - from) / span))) / top;
	}
	ALBEDO = want - under;
}
