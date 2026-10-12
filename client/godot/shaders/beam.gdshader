// A ray (vfx.rs): a ribbon along the ray that always faces the camera,
// drawn as what the ray is of (`style`, vfx.rs `Ray`): a glowing tube, darts
// of light, a stream of flame, an icy core among shards of ice in a cold
// mist, a soft drifting haze, a black ray eating the light, a jagged bolt
// of lightning with forks, a spray of droplets, billows of gas, churning
// rock dust, rings of force. `head` is how far it has run from its start,
// `fade` takes it away. What glows adds light; what is dark, misty or
// dusty covers what is behind (premultiplied alpha).
shader_type spatial;
render_mode unshaded, blend_premul_alpha, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(0.38, 0.56, 1.0, 1.0);
uniform float fade = 1.0;
uniform float head = 1.0;
// the ribbon's full width, metres
uniform float width = 0.42;
uniform float energy = 1.6;
// what the ray is of (the order of vfx.rs `Ray::style`)
uniform int style = 0;
// the ray's length in metres (its patterns are of a fixed size, however
// long it is)
uniform float ray_len = 1.0;
// each ray its own pattern
uniform float seed = 0.0;

const int PLAIN = 0;
const int MISSILE = 1;
const int FIRE = 2;
const int COLD = 3;
const int SLEEP = 4;
const int DEATH = 5;
const int LIGHTNING = 6;
const int SPRAY = 7;
const int DUST = 8;
const int FORCE = 9;
const int GAS = 10;

// lightning: how many times a second the bolt strikes anew; a jag's length
// along the bolt, and how far either way it may swing, as a share of the
// ribbon's half width
const float STRIKES = 14.0;
const float JAG = 0.22;
const float SWING = 0.55;

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

// a smooth wander in -1..1: the bolt's course, a jag's points on it
float wander(float x, float s) {
	float i = floor(x);
	float f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	return mix(hash(vec2(i, s)), hash(vec2(i + 1.0, s)), f) * 2.0 - 1.0;
}

// straight lines between points every `len` metres, each at its height
// on a wander: a zigzag with corners, no two alike
float zigzag(float m, float len, float s) {
	float i = floor(m / len);
	float f = fract(m / len);
	// a point's place along its jag varies too, so the jags are uneven
	float a = wander(i * 0.37, s) + (hash(vec2(i, s + 3.0)) - 0.5) * 0.9;
	float b = wander((i + 1.0) * 0.37, s) + (hash(vec2(i + 1.0, s + 3.0)) - 0.5) * 0.9;
	return mix(a, b, f);
}

// where the bolt is across the ribbon (-1..1) at `m` metres along it, for
// strike `s`: big jags wandering off the line, finer ones on them, pinned
// to the middle at both ends
float jagged(float m, float s, float k) {
	float big = zigzag(m, JAG, s * 1.7 + k);
	float fine = zigzag(m, JAG * 0.31, s * 2.3 + k + 7.0) * 0.28;
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
	// across the ribbon: -1..1 (side), 0 in the middle to 1 at its edges (x)
	float side = (UV.x - 0.5) * 2.0;
	float x = abs(side);
	// UV.y runs from the end (0) to the start (1); m is metres from the start
	float from_start = 1.0 - UV.y;
	float m = from_start * ray_len;
	// a haze, a spray, gas and dust thin out over a longer way
	bool soft = style == SLEEP || style == SPRAY || style == DUST || style == GAS;
	float taper = soft ? min(0.3 * ray_len, 0.9) : min(0.04 * ray_len, 0.25);
	float ends = smoothstep(0.0, taper * (soft ? 0.5 : 1.0), m)
		* smoothstep(0.0, taper, ray_len - m);
	float shown = 1.0 - smoothstep(head - 0.03, head, from_start);
	float a = fade * ends * shown;
	float t = TIME + seed * 7.3;
	// light it adds, and how much it covers what is behind with its own
	// colour
	vec3 glow = vec3(0.0);
	float cover = 0.0;
	vec3 tint = vec3(0.0);
	if (style == MISSILE) {
		// darts of light one after another down the ray, each a bright head
		// with a short tail, drifting a little to one side or the other as
		// it flies, in a glow
		float darts = 0.0;
		float run = ray_len + 1.0;
		for (int k = 0; k < 4; k++) {
			float fk = float(k);
			float tip = mod(t * 9.0 + fk * run / 4.0 + hash(vec2(fk, seed)) * 0.4, run) - 0.5;
			float weave = sin(tip * 2.6 + fk * 1.9 + seed) * 0.22;
			float behind = tip - m;
			float d = side - weave;
			float head_ = exp(-pow(behind / 0.1, 2.0));
			float tail = behind > 0.0 ? exp(-behind / 0.35) : 0.0;
			float thin = exp(-pow(d / (0.06 + 0.05 * clamp(behind, 0.0, 0.6)), 2.0));
			darts += (head_ * 2.4 + tail * 1.1) * thin;
			darts += exp(-pow(behind / 0.25, 2.0)) * exp(-pow(d / 0.38, 2.0)) * 0.4;
		}
		float guide = exp(-pow(x / 0.3, 2.0)) * 0.06;
		glow = mix(color.rgb, vec3(1.0), 0.5) * darts * 1.8 + color.rgb * guide;
	} else if (style == FIRE) {
		// flame running from the start to the end, widening as it goes,
		// its edge licking and its heat churning: white-yellow at its
		// hottest, orange, a deep red at its ragged edges, a little smoke
		// beyond them
		float flow = m - t * 4.5;
		float n = fbm(vec2(flow * 2.0, side * 1.4 + seed));
		float n2 = fbm(vec2(flow * 4.2 + 5.0, side * 2.8 - t * 1.7));
		float lick = fbm(vec2(flow * 7.0 + 2.0, side * 6.0 + t * 2.3));
		float spread = mix(0.35, 1.0, smoothstep(0.0, 1.5, m));
		float half_w = spread * (0.34 + 0.4 * n + 0.22 * (lick - 0.5));
		float off = (n2 - 0.5) * 0.4 * spread;
		float body = smoothstep(half_w, half_w * 0.2, abs(side - off));
		float heat = body * (0.35 + 0.95 * n2) * mix(1.25, 0.85, smoothstep(0.0, ray_len, m));
		vec3 fire = mix(vec3(0.45, 0.03, 0.0), vec3(1.0, 0.36, 0.04), smoothstep(0.05, 0.45, heat));
		fire = mix(fire, vec3(1.0, 0.75, 0.3), smoothstep(0.45, 0.85, heat));
		fire = mix(fire, vec3(1.0, 0.95, 0.8), smoothstep(0.9, 1.25, heat));
		glow = fire * heat * 2.0;
		float smoke = smoothstep(half_w * 1.6, half_w, abs(side - off)) * (1.0 - body)
			* smoothstep(0.3, 0.7, n) * smoothstep(0.5, 2.0, m);
		cover = smoke * 0.35;
		tint = vec3(0.06, 0.04, 0.03);
	} else if (style == COLD) {
		// a hard white core, jagged like a crystal's edge, among shards of
		// ice (thin diamonds along the ray, catching the light), in a cold
		// mist that comes in patches
		float facet = m * 7.0 + seed;
		float spike = abs(fract(facet) - 0.5) * 2.0;
		float jag = 0.03 + 0.06 * spike * hash(vec2(floor(facet), 1.0));
		float core = smoothstep(jag, jag * 0.3, x);
		vec2 g = vec2(m * 4.0, side * 3.0);
		vec2 cell = floor(g);
		float h = hash(cell + seed);
		vec2 o = (vec2(hash(cell + 1.3), hash(cell + 2.9)) - 0.5) * 0.5;
		vec2 q = abs(fract(g) - 0.5 - o);
		float shard = step(0.5, h) * smoothstep(1.0, 0.6, q.x / 0.42 + q.y / 0.08)
			* exp(-pow(x / 0.8, 2.0));
		float twinkle = 0.5 + 0.5 * sin(t * 23.0 + h * 40.0);
		float mist = exp(-pow(x / 0.62, 2.0))
			* smoothstep(0.25, 0.8, fbm(vec2(m * 1.6 - t * 0.9, side * 2.0 + t * 0.3)));
		glow = color.rgb * mist * 0.55 + mix(color.rgb, vec3(1.0), 0.8) * core * 1.8
			+ vec3(0.8, 0.93, 1.0) * shard * (0.6 + 1.2 * twinkle);
		// a rime of mist, pale over what is behind
		cover = mist * 0.25;
		tint = mix(color.rgb, vec3(1.0), 0.5) * 0.5;
	} else if (style == SLEEP) {
		// a soft haze drifting along in puffs, no core, slow
		float drift = m - t * 0.7;
		float n = fbm(vec2(drift * 1.3, side * 1.2 + t * 0.25 + seed));
		float n2 = fbm(vec2(drift * 2.8 + 7.0, side * 2.4 - t * 0.4));
		float wide = 0.45 + 0.35 * fbm(vec2(drift * 0.9 + 3.0, seed));
		float body = exp(-pow(x / wide, 2.0));
		float haze = body * smoothstep(0.2, 0.8, n) * (0.5 + 0.5 * n2);
		glow = color.rgb * haze * 0.8;
		cover = haze * 0.35;
		tint = mix(color.rgb, vec3(1.0), 0.3) * 0.45;
	} else if (style == DEATH) {
		// black, eating the light, in a thin sickly glow that frays to smoke
		float n = fbm(vec2(m * 2.2 - t * 3.0, side * 1.8 + seed));
		float edge = 0.3 + (n - 0.5) * 0.3;
		float dark = smoothstep(edge + 0.14, edge - 0.1, x);
		float rim = exp(-pow((x - edge - 0.1) / 0.07, 2.0)) * (0.6 + 0.6 * n);
		float aura = smoothstep(edge, edge + 0.12, x) * exp(-pow((x - edge - 0.12) / 0.3, 2.0)) * 0.3 * n;
		glow = color.rgb * (rim * 1.1 + aura);
		cover = dark * 0.93;
		tint = vec3(0.0);
	} else if (style == LIGHTNING) {
		float s = floor(TIME * STRIKES);
		// the bolt, sharp, in a thin glow that follows it
		float main_d = abs(side - jagged(m, s, 0.0));
		float core = exp(-pow(main_d / 0.035, 2.0));
		float halo = exp(-pow(main_d / 0.22, 2.0)) * 0.6;
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
		glow = (color.rgb * halo * 0.9 + mix(color.rgb, vec3(1.0), 0.5) * core * 1.4)
			* clamp(halo * 0.6 + core, 0.0, 1.0);
	} else if (style == SPRAY) {
		// a spray fanning out: droplets flung along it, each its own size,
		// in a fine mist
		float spread = mix(0.25, 1.0, smoothstep(0.0, 2.0, m));
		vec2 g = vec2(m * 5.0 - t * 14.0, side / spread * 5.0);
		vec2 cell = floor(g);
		float h = hash(cell + seed);
		vec2 o = (vec2(hash(cell + 3.1), hash(cell + 7.7)) - 0.5) * 0.6;
		vec2 d = fract(g) - 0.5 - o;
		float size = 0.6 + 0.8 * hash(cell + 5.3);
		float drop = step(0.55, h) * exp(-(pow(d.x / (0.2 * size), 2.0) + pow(d.y / (0.12 * size), 2.0)));
		float body = exp(-pow(x / spread, 2.0));
		float mist = exp(-pow(x / (0.55 * spread), 2.0))
			* smoothstep(0.25, 0.8, fbm(vec2(m * 2.0 - t * 3.0, side * 2.0 + seed)));
		glow = color.rgb * (drop * body * 1.7 + mist * 0.45);
		cover = mist * 0.3;
		tint = color.rgb * 0.35;
	} else if (style == GAS) {
		// billows of gas rolling along, thickening as they spread
		float roll = m - t * 1.8;
		float spread = mix(0.3, 1.0, smoothstep(0.0, 2.0, m));
		float n = fbm(vec2(roll * 1.6, side / spread * 1.5 + seed));
		float n2 = fbm(vec2(roll * 3.4 + 4.0, side / spread * 3.0 - t * 0.6));
		float body = exp(-pow(x / (spread * (0.55 + 0.3 * n)), 2.0));
		float gas = body * smoothstep(0.2, 0.75, n) * (0.55 + 0.45 * n2);
		glow = color.rgb * gas * 0.5;
		cover = gas * 0.5;
		tint = color.rgb * mix(0.25, 0.5, n2);
	} else if (style == DUST) {
		// rock dust churning along: no light of its own, it covers what is
		// behind, billows of it, grit darker in it
		float churn = m - t * 2.6;
		float n = fbm(vec2(churn * 1.8, side * 2.0 + seed));
		float n2 = fbm(vec2(churn * 4.5 + 3.0, side * 4.5 + t));
		float body = exp(-pow(x / (0.5 + 0.3 * n), 2.0));
		float dust = body * smoothstep(0.15, 0.6, n);
		// grit: specks of rock, each round, flung along
		vec2 g = vec2(m - t * 1.4, side * width * 0.5) * 14.0;
		vec2 cell = floor(g);
		vec2 o = (vec2(hash(cell + 1.7), hash(cell + 4.1)) - 0.5) * 0.5;
		float grit = step(0.8, hash(cell + seed))
			* smoothstep(0.3, 0.12, length(fract(g) - 0.5 - o)) * body;
		cover = clamp(dust * 0.9 + grit * 0.5, 0.0, 0.95);
		tint = mix(color.rgb, color.rgb * 0.5, smoothstep(0.3, 0.7, n2) * 0.8 + grit * 0.6);
	} else if (style == FORCE) {
		// rings of force running along it, seen aslant, each a bright rim
		// round a faint glow, in a faint shimmer
		float rings = 0.0;
		for (int k = 0; k < 3; k++) {
			float fk = float(k);
			float run = ray_len + 0.8;
			float at = mod(t * 7.5 + fk * run / 3.0, run) - 0.4;
			float grow = 0.55 + 0.3 * clamp(at / max(ray_len, 0.1), 0.0, 1.0);
			float e = length(vec2((m - at) / 0.13, side / grow));
			float fade_k = 1.0 - 0.5 * clamp(at / max(ray_len, 0.1), 0.0, 1.0);
			rings += (exp(-pow((e - 1.0) / 0.09, 2.0)) * 0.8 + exp(-pow((e - 0.8) / 0.3, 2.0)) * 0.18) * fade_k;
		}
		float shimmer = exp(-pow(x / 0.25, 2.0)) * 0.03;
		glow = mix(color.rgb, vec3(1.0), 0.5) * rings + color.rgb * shimmer;
	} else {
		// a white-hot core in a halo of the ray's colour
		float core = exp(-pow(x / 0.12, 2.0));
		float halo = exp(-pow(x / 0.55, 2.0));
		glow = (color.rgb * halo * 0.9 + mix(color.rgb, vec3(1.0), 0.5) * core * 1.4)
			* clamp(halo * 0.6 + core, 0.0, 1.0);
	}
	cover = clamp(cover, 0.0, 1.0) * a;
	ALBEDO = glow * energy * a + tint * cover;
	ALPHA = cover;
}
