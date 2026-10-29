// The sea of clouds far below the Plane of Air (map_view: under a level
// whose branch lies over the sky, `Under::Sky`): banks of cloud lit from
// above with the deep blue sky between them, drifting slowly, paling into
// a haze far off. Never in the level's fog: the dark at the frame's depth
// is the dungeon's, not the sky's.
shader_type spatial;
render_mode unshaded, fog_disabled, shadows_disabled, cull_disabled;

uniform vec3 deep : source_color = vec3(0.12, 0.27, 0.58);
uniform vec3 gap : source_color = vec3(0.3, 0.5, 0.84);
uniform vec3 lit : source_color = vec3(1.0, 0.97, 0.93);
uniform vec3 shade : source_color = vec3(0.56, 0.65, 0.84);
uniform vec3 horizon : source_color = vec3(0.8, 0.87, 0.97);

varying vec3 wpos;

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
	float v = 0.0;
	float a = 0.5;
	for (int k = 0; k < 5; k++) {
		v += a * noise(p);
		p = p * 2.03 + vec2(1.7, 9.2);
		a *= 0.5;
	}
	return v;
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec2 p = wpos.xz * 0.025 + vec2(TIME * 0.006, TIME * 0.0025);
	float n = fbm(p);
	float fine = fbm(p * 3.7 + vec2(4.1, 1.3) - TIME * 0.004);
	float cover = smoothstep(0.45, 0.6, n + (fine - 0.5) * 0.2);
	// the banks' tops lit and their flanks shaded, as if from the north
	// west: by the slope of the noise
	float slope = n - fbm(p + vec2(0.03, 0.03));
	float light = clamp(0.55 + slope * 12.0 + (fine - 0.5) * 0.5, 0.0, 1.0);
	vec3 cloud = mix(shade, lit, light);
	vec3 sky = mix(deep, gap, smoothstep(0.2, 0.55, n));
	vec3 col = mix(sky, cloud, cover);
	float far = length(wpos - CAMERA_POSITION_WORLD);
	ALBEDO = mix(col, horizon, smoothstep(70.0, 280.0, far));
}
