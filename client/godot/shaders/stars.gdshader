// The heavens round the Astral Plane (map_view: far under a level whose
// branch lies among the stars, `Under::Stars`): deep violet night with
// clouds of nebula in rose and blue drifting slowly, and stars at three
// depths, the nearest twinkling. Never in the level's fog.
shader_type spatial;
render_mode unshaded, fog_disabled, shadows_disabled, cull_disabled;

uniform vec3 night : source_color = vec3(0.012, 0.01, 0.035);
uniform vec3 rose : source_color = vec3(0.42, 0.16, 0.38);
uniform vec3 blue : source_color = vec3(0.12, 0.2, 0.5);
uniform vec3 star : source_color = vec3(1.0, 0.96, 0.9);
// what is known of the level (map_view's fog of war, A: near something
// known): behind it on the screen and about it the heavens are veiled in
// the dark, so the temple's edges fall away into it, never a slab cut out
// against the stars
uniform sampler2D fow_tex : filter_linear, repeat_disable;
uniform bool has_fow = false;
const vec2 FIELD = vec2(80.0, 21.0);

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
		p = p * 2.07 + vec2(3.1, 7.7);
		a *= 0.5;
	}
	return v;
}

// stars on a grid of cells, one at most in each, by `density`
float stars(vec2 p, float density, float size, float t) {
	vec2 cell = floor(p);
	vec2 f = fract(p) - 0.5;
	float h = hash(cell);
	if (h > density) {
		return 0.0;
	}
	vec2 o = vec2(hash(cell + 3.7), hash(cell + 9.1)) - 0.5;
	float d = length(f - o * 0.7);
	float twinkle = 0.65 + 0.35 * sin(t * (1.5 + h * 4.0) + h * 40.0);
	return smoothstep(size, 0.0, d) * twinkle * (0.5 + h / density * 0.5);
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	vec2 p = wpos.xz * 0.02 + vec2(TIME * 0.002, 0.0);
	float n = fbm(p);
	float m = fbm(p * 1.7 + vec2(5.2, 1.3) - TIME * 0.001);
	vec3 col = night;
	col += rose * smoothstep(0.45, 0.85, n) * 0.55;
	col += blue * smoothstep(0.5, 0.9, m) * 0.6;
	vec2 s = wpos.xz;
	col += star * stars(s * 0.6, 0.06, 0.09, TIME);
	col += star * stars(s * 1.3 + 17.0, 0.05, 0.12, TIME * 0.7) * 0.7;
	col += star * stars(s * 2.6 + 41.0, 0.06, 0.16, 0.0) * 0.45;
	if (has_fow) {
		// where the eye's ray through this point crossed the level's plane
		vec3 eye = CAMERA_POSITION_WORLD;
		vec3 at = mix(eye, wpos, eye.y / max(eye.y - wpos.y, 1e-3));
		vec2 uv = (at.xz + 0.5) / FIELD;
		vec2 d = vec2(1.6) / FIELD;
		float near = texture(fow_tex, uv).a * 2.0;
		for (int i = 0; i < 6; i++) {
			float a = float(i) * 1.0472;
			near += texture(fow_tex, uv + vec2(cos(a), sin(a)) * d).a;
		}
		near /= 8.0;
		float veil = smoothstep(0.03, 0.42, near + (fbm(at.xz * 0.35) - 0.5) * 0.15);
		col = mix(col, night * 0.25, veil);
	}
	ALBEDO = col;
}
