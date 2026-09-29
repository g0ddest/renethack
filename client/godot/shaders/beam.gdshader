// A ray (vfx.rs): a glowing rod, white-hot along its core, of the ray's
// colour at its edge; `fade` takes it away.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform vec4 color : source_color = vec4(0.38, 0.56, 1.0, 1.0);
uniform float fade = 1.0;

void fragment() {
	float edge = abs(dot(normalize(NORMAL), normalize(VIEW)));
	float core = pow(edge, 3.0);
	ALBEDO = mix(color.rgb, vec3(1.0), core * 0.35) * (0.8 + core * 2.2);
	ALPHA = fade * edge;
}
