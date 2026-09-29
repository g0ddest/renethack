// A torch's flame (map_view.rs): a billboard turning about y to face the
// camera, shaped and licked by scrolling noise, white-hot at its root.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform float energy = 1.3;
// each flame its own
instance uniform float phase = 0.0;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1, 0)), f.x),
		mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), f.x), f.y);
}

float fbm(vec2 p) {
	return 0.5 * noise(p) + 0.3 * noise(p * 2.1 + 3.7) + 0.2 * noise(p * 4.3 + 9.1);
}

void vertex() {
	vec3 side = normalize(cross(vec3(0.0, 1.0, 0.0), INV_VIEW_MATRIX[2].xyz));
	vec3 front = cross(side, vec3(0.0, 1.0, 0.0));
	float s = length(MODEL_MATRIX[0].xyz);
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(vec4(side * s, 0.0), vec4(0.0, s, 0.0, 0.0),
		vec4(front * s, 0.0), MODEL_MATRIX[3]);
}

void fragment() {
	float y = 1.0 - UV.y;
	float t = TIME + phase;
	float n = fbm(vec2(UV.x * 3.0, y * 2.4 - t * 2.3));
	float x = (UV.x - 0.5) * 2.0 + (n - 0.5) * 0.7 * y;
	float width = mix(0.75, 0.04, pow(y, 0.75));
	float shape = 1.0 - smoothstep(width * 0.45, width, abs(x));
	shape *= smoothstep(0.0, 0.1, y) * (1.0 - smoothstep(0.45 + n * 0.45, 1.0, y));
	float heat = shape * (1.25 - y);
	vec3 col = mix(vec3(1.0, 0.22, 0.03), vec3(1.0, 0.68, 0.3), smoothstep(0.25, 0.8, heat));
	col = mix(col, vec3(1.0, 0.93, 0.75), smoothstep(0.8, 1.1, heat));
	ALBEDO = col * energy;
	ALPHA = clamp(shape * 1.2, 0.0, 1.0);
}
