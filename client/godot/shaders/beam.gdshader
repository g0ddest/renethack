// A ray (vfx.rs): a ribbon along the ray that always faces the camera, a
// white-hot core in a halo of the ray's colour, soft at its sides and
// ends. `head` is how far it has run from its start, `fade` takes it away.
// A bolt of lightning (`bolt`) is no straight core but a jagged one that
// strikes anew many times a second, with forks off it.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(0.38, 0.56, 1.0, 1.0);
uniform float fade = 1.0;
uniform float head = 1.0;
// the halo's full width, metres
uniform float width = 0.42;
uniform float energy = 1.6;
// lightning: the jagged bolt instead of the straight core, and the ray's
// length in metres (its jags are of a fixed size, however long it is)
uniform bool bolt = false;
uniform float ray_len = 1.0;
// how many times a second the bolt strikes anew
const float STRIKES = 14.0;
// a jag's length along the bolt, and how far either way it may swing,
// as a share of the ribbon's half width
const float JAG = 0.22;
const float SWING = 0.55;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

// a smooth wander in -1..1: the bolt's course, a jag's points on it
float wander(float x, float seed) {
	float i = floor(x);
	float f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	return mix(hash(vec2(i, seed)), hash(vec2(i + 1.0, seed)), f) * 2.0 - 1.0;
}

// straight lines between points every `len` metres, each at its height
// on a wander: a zigzag with corners, no two alike
float zigzag(float m, float len, float seed) {
	float i = floor(m / len);
	float f = fract(m / len);
	// a point's place along its jag varies too, so the jags are uneven
	float a = wander(i * 0.37, seed) + (hash(vec2(i, seed + 3.0)) - 0.5) * 0.9;
	float b = wander((i + 1.0) * 0.37, seed) + (hash(vec2(i + 1.0, seed + 3.0)) - 0.5) * 0.9;
	return mix(a, b, f);
}

// where the bolt is across the ribbon (-1..1) at `m` metres along it, for
// strike `s`: big jags wandering off the line, finer ones on them, pinned
// to the middle at both ends
float jagged(float m, float s, float seed) {
	float big = zigzag(m, JAG, s * 1.7 + seed);
	float fine = zigzag(m, JAG * 0.31, s * 2.3 + seed + 7.0) * 0.28;
	float pin = smoothstep(0.0, 0.3, m) * smoothstep(0.0, 0.3, ray_len - m);
	return (big + fine) * SWING * 0.6 * pin;
}

void vertex() {
	// the quad's y runs along the ray (the model's y axis is the ray),
	// its x across it, turned to face the camera
	vec3 axis = MODEL_MATRIX[1].xyz;
	vec3 mid = MODEL_MATRIX[3].xyz;
	vec3 p = mid + axis * VERTEX.y;
	vec3 eye = INV_VIEW_MATRIX[3].xyz;
	vec3 side = normalize(cross(normalize(axis), normalize(eye - p)));
	p += side * VERTEX.x * width;
	VERTEX = (VIEW_MATRIX * vec4(p, 1.0)).xyz;
}

void fragment() {
	float x = abs(UV.x - 0.5) * 2.0;
	float core = exp(-pow(x / 0.12, 2.0));
	float halo = exp(-pow(x / 0.55, 2.0));
	// UV.y runs from the end (0) to the start (1)
	float from_start = 1.0 - UV.y;
	float ends = smoothstep(0.0, 0.04, from_start) * smoothstep(0.0, 0.04, UV.y);
	float shown = 1.0 - smoothstep(head - 0.03, head, from_start);
	float a = fade * ends * shown;
	if (bolt) {
		float side = (UV.x - 0.5) * 2.0;
		float m = from_start * ray_len;
		float s = floor(TIME * STRIKES);
		// the bolt, sharp, in a thin glow that follows it
		float main_d = abs(side - jagged(m, s, 0.0));
		core = exp(-pow(main_d / 0.035, 2.0));
		halo = exp(-pow(main_d / 0.22, 2.0)) * 0.6;
		// two forks: each leaves the bolt at a jag, swings off to one side
		// and thins out within a metre
		for (int k = 0; k < 2; k++) {
			float fk = float(k);
			float start = (0.15 + 0.55 * hash(vec2(s, fk + 3.0))) * ray_len;
			float run = m - start;
			float reach = 0.5 + 0.6 * hash(vec2(s, fk + 5.0));
			if (run > 0.0 && run < reach) {
				float away = (hash(vec2(s, fk + 9.0)) < 0.5 ? -1.0 : 1.0) * run / reach * 0.9;
				float fork = jagged(start, s, 0.0) + away + jagged(m, s, 11.0 + fk) * 0.5;
				float thin = 1.0 - run / reach;
				float d = abs(side - fork);
				core = max(core, exp(-pow(d / 0.025, 2.0)) * thin);
				halo = max(halo, exp(-pow(d / 0.15, 2.0)) * 0.4 * thin);
			}
		}
	}
	vec3 c = color.rgb * halo * 0.9 + mix(color.rgb, vec3(1.0), 0.5) * core * 1.4;
	ALBEDO = c * energy * a;
	ALPHA = clamp((halo * 0.6 + core) * a, 0.0, 1.0);
}
