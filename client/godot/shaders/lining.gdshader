// The inside of a hero's open coat or robe (art.rs: after her edge, on a
// two-sided material, a generated body's): its back faces darkened over
// what the material drew there, a lining in shadow, not a pale sheet lit
// like the outside (a lunge turned the coat's inside to the light). A
// closed body's back faces lie behind its front: the depth test keeps
// them unseen and untouched.
shader_type spatial;
render_mode unshaded, blend_mul, depth_draw_never, cull_front, shadows_disabled, fog_disabled;

uniform float shade = 0.3;

void fragment() {
	ALBEDO = vec3(shade);
}
