// A soft mass of cloud on a cloud cell of the Plane of Air (map_view: a
// flat square the batch places, stood up here to face the eye): a puff
// ragged at its rim, lit from above, its underside in its own shade,
// drifting slowly. Several overlap into a bank.
shader_type spatial;
render_mode unshaded, depth_draw_never, cull_disabled, shadows_disabled, skip_vertex_transform;

uniform vec3 lit : source_color = vec3(1.0, 0.98, 0.95);
uniform vec3 shade : source_color = vec3(0.42, 0.5, 0.68);
uniform float opacity = 0.75;

varying vec2 corner;
varying vec3 centre;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p) {
	return 0.5 * noise(p) + 0.3 * noise(p * 2.1 + 3.7) + 0.2 * noise(p * 4.3 + 9.1);
}

void vertex() {
	centre = MODEL_MATRIX[3].xyz;
	// -1..1 across the square, y down the screen
	corner = (UV - 0.5) * 2.0;
	vec3 right = INV_VIEW_MATRIX[0].xyz;
	vec3 up = INV_VIEW_MATRIX[1].xyz;
	vec3 p = centre + right * VERTEX.x - up * VERTEX.z;
	VERTEX = (VIEW_MATRIX * vec4(p, 1.0)).xyz;
	NORMAL = vec3(0.0, 0.0, 1.0);
}

void fragment() {
	vec2 q = corner * 1.3 + centre.xz * 0.9 + vec2(TIME * 0.04, TIME * 0.015);
	float n = fbm(q);
	// (and nothing at the square's edge, whatever the noise)
	float square = smoothstep(1.0, 0.8, max(abs(corner.x), abs(corner.y)));
	float puff = smoothstep(1.0, 0.25, length(corner) + (n - 0.5) * 1.1) * square;
	// lit from above: its top bright, its underside in shade, the lower
	// puffs of a bank darker than the high ones
	float top = clamp(0.5 - corner.y * 0.6, 0.0, 1.0);
	float high = clamp((centre.y - 0.3) / 1.0, 0.0, 1.0);
	float light = clamp(top * 0.75 + (n - 0.5) * 0.9 + 0.1 + high * 0.25, 0.0, 1.0);
	ALBEDO = mix(shade, lit, light);
	ALPHA = puff * opacity;
}
