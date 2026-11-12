// A thing on the floor's glint (art.rs: over each one): a small point of
// light that flares now and then, each thing at its own moment (by where
// it lies), so what lies about catches the eye at the game's distance.
// A flat square stood up here to face the eye.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;

uniform vec3 color : source_color = vec3(1.0, 0.95, 0.85);
// across, metres
uniform float size = 0.26;
// seconds between two flares of one thing
uniform float period = 4.5;

varying vec2 corner;
varying float phase;

void vertex() {
	vec3 centre = MODEL_MATRIX[3].xyz;
	corner = (UV - 0.5) * 2.0;
	phase = fract(sin(dot(centre.xz, vec2(12.9898, 78.233))) * 43758.5453);
	vec3 right = INV_VIEW_MATRIX[0].xyz;
	vec3 up = INV_VIEW_MATRIX[1].xyz;
	vec3 p = centre + (right * corner.x - up * corner.y) * size * 0.5;
	VERTEX = (VIEW_MATRIX * vec4(p, 1.0)).xyz;
}

void fragment() {
	// a quick flare and a slower fade, once a period
	float t = fract(TIME / period + phase);
	float flare = smoothstep(0.0, 0.05, t) * (1.0 - smoothstep(0.05, 0.2, t));
	// a small hot point, drawn out a little to either side: no star
	vec2 c = abs(corner);
	float heart = exp(-dot(corner, corner) * 14.0);
	float streak = exp(-c.y * 22.0) * exp(-c.x * 3.5) * (1.0 - c.x);
	ALBEDO = color * (streak * 0.7 + heart * 2.2) * flare;
}
