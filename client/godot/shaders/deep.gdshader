// The deep of the Plane of Water (map_view, `Under::Deep`: its water cells,
// and the ground under and round the level): dark water looked down into,
// the light from above gathering in threads near it and fading below,
// motes drifting at three depths, each layer seen through the cell where
// the eye's line would reach it (no geometry under the surface).
shader_type spatial;
render_mode unshaded, shadows_disabled;

uniform vec3 abyss : source_color = vec3(0.005, 0.03, 0.07);
uniform vec3 light : source_color = vec3(0.35, 0.65, 0.8);
uniform vec3 mote : source_color = vec3(0.6, 0.85, 0.95);

varying vec3 wpos;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

vec2 hash2(vec2 p) {
	return vec2(hash(p), hash(p + 19.19));
}

float threads(vec2 p, float t) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	float d1 = 8.0;
	float d2 = 8.0;
	for (int y = -1; y <= 1; y++) {
		for (int x = -1; x <= 1; x++) {
			vec2 g = vec2(float(x), float(y));
			vec2 o = 0.5 + 0.4 * sin(t + 6.2831 * hash2(i + g));
			float d = length(g + o - f);
			if (d < d1) {
				d2 = d1;
				d1 = d;
			} else if (d < d2) {
				d2 = d;
			}
		}
	}
	return 1.0 - smoothstep(0.0, 0.2, d2 - d1);
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec3 dir = normalize(wpos - CAMERA_POSITION_WORLD);
	float down = max(-dir.y, 0.25);
	vec3 col = abyss;
	for (int k = 0; k < 3; k++) {
		float fk = float(k);
		float depth = 0.8 + fk * 2.4;
		vec2 at = wpos.xz + dir.xz / down * depth;
		float fade = exp(-depth * 0.32);
		vec2 w = at * (0.6 - fk * 0.1);
		w += vec2(sin(w.y * 2.0 + TIME * 0.5), cos(w.x * 2.0 - TIME * 0.4)) * 0.2;
		col += light * threads(w, TIME * (0.4 - fk * 0.07) + fk * 1.7) * 0.065 * fade;
		vec2 g = at * 1.6 + vec2(TIME * 0.05, TIME * 0.02 * (1.0 + fk));
		vec2 h = hash2(floor(g) + fk * 7.3);
		float speck = step(0.78, h.x) * smoothstep(0.09, 0.0, length(fract(g) - 0.5 - (h - 0.5) * 0.6));
		col += mote * speck * 0.6 * fade;
	}
	ALBEDO = col;
}
