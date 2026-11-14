// The edge's uniforms and light (creature_edge.gdshader, hero_edge.gdshader:
// the same pass, the hero's also marking where she is the nearest).
uniform vec3 color : source_color = vec3(0.62, 0.70, 1.0);
uniform float strength = 0.35;
// how far from facing the eye the edge begins (0 facing, 1 side-on)
uniform float from = 0.4;
// how much of it is kept to the parts facing up (1: the head's crown and
// the shoulders only, a generated body's inner rim inside its outline)
uniform float top = 0.45;
// a thing on the floor's glint: how bright (0: none), its colour at its
// middle (dark gold, the edge's colour, at its sides), the seconds between
// two passes over it and how much of them it takes to cross it
uniform float sweep = 0.0;
uniform vec3 sweep_color : source_color = vec3(1.0, 0.66, 0.3);
uniform float sweep_period = 2.4;
uniform float sweep_share = 0.45;
// how much stronger the edge and the glint are at the game's far camera
// (11 m) than at its near one (7 m): a small thing's few pixels
uniform float far = 0.0;
// a thing on the floor's dim wash of the edge's colour over the whole of
// it, at the far camera only (a flat blade facing the eye has no rim)
uniform float fill = 0.0;

void fragment() {
	vec3 n = normalize(NORMAL);
	float edge = smoothstep(from, 0.95, 1.0 - clamp(dot(n, normalize(VIEW)), 0.0, 1.0));
	float up = (INV_VIEW_MATRIX * vec4(n, 0.0)).y;
	vec3 lit = color * strength * edge * ((1.0 - top) + top * clamp(up + 0.3, 0.0, 1.0));
	vec3 glint = vec3(0.0);
	if (sweep > 0.0) {
		// a band of light crossing the thing aslant, once a period, each
		// thing at its own moment (by where it lies); none out of the
		// game's light, where the edge is off
		vec3 origin = MODEL_MATRIX[3].xyz;
		vec3 world = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
		float phase = fract(TIME / sweep_period
			+ fract(sin(dot(origin.xz, vec2(12.9898, 78.233))) * 43758.5453));
		float run = clamp(phase / sweep_share, 0.0, 1.0);
		float across = dot(world.xz - origin.xz, vec2(0.8, 0.6));
		float at = mix(-0.32, 0.32, run);
		float on = smoothstep(0.0, 0.08, run) * (1.0 - smoothstep(0.92, 1.0, run));
		float band = exp(-pow((across - at) / 0.05, 2.0)) * on;
		glint = mix(color, sweep_color, band) * band * sweep * step(0.001, strength);
	}
	float distant = smoothstep(7.5, 11.5, length(VERTEX));
	float k = 1.0 + far * distant;
	vec3 wash = color * fill * distant * step(0.001, strength);
	ALBEDO = (lit + glint + wash) * k;
}
