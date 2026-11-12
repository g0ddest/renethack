// A detailed creature's edge (art.rs: `ModelSpec::outline`, a generated
// body of thousands of ridges and spikes): its outline only, a thin shell
// drawn behind it in the cold light of `creature_edge`, which would trace
// every ridge of such a mesh in glitter.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_front, shadows_disabled, fog_disabled;

uniform vec3 color : source_color = vec3(0.62, 0.70, 1.0);
uniform float strength = 0.35;
// the shell's width, metres
uniform float width = 0.008;

void vertex() {
	vec4 p = MODELVIEW_MATRIX * vec4(VERTEX, 1.0);
	vec3 n = normalize(MODELVIEW_NORMAL_MATRIX * NORMAL);
	p.xyz += n * width;
	POSITION = PROJECTION_MATRIX * p;
}

void fragment() {
	ALBEDO = color * strength;
}
