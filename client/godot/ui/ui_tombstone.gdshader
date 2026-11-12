// The end screen's tombstone (genl_outrip's picture, drawn): a granite
// headstone with a round top on a plinth, lit from the top left, its face
// polished inside a carved line, moss at its foot, on a strip of earth
// and grass with pebbles and a few dead stalks, casting a soft shadow; the
// night behind it a cold glow round its top, its edge caught by that
// light, a candle burning on its plinth. All of it fades out at the
// control's edges, into the page. tombstone.rs sets `size` to the
// control's and puts the stone's words on its face.
shader_type canvas_item;
render_mode unshaded;

uniform vec2 size = vec2(300.0, 460.0);
// room around the stone for its shadow
uniform float pad = 16.0;
// the slab's sides in from the plinth's
uniform float inset = 16.0;
// the polished face in from the slab's edge
uniform float face_in = 13.0;
uniform float plinth_h = 30.0;
uniform float ground_h = 54.0;
uniform vec4 stone_light : source_color = vec4(0.6, 0.58, 0.54, 1.0);
uniform vec4 stone_dark : source_color = vec4(0.26, 0.25, 0.24, 1.0);
uniform vec4 moss_color : source_color = vec4(0.2, 0.26, 0.12, 1.0);
uniform vec4 earth_color : source_color = vec4(0.13, 0.095, 0.065, 1.0);
uniform vec4 grass_color : source_color = vec4(0.15, 0.22, 0.09, 1.0);
// (the dead stalks at its foot)
uniform vec4 flower_color : source_color = vec4(0.2, 0.17, 0.11, 1.0);
// the night behind the stone, and the cold light round its top
uniform vec4 night_color : source_color = vec4(0.05, 0.06, 0.09, 1.0);
uniform vec4 moon_color : source_color = vec4(0.44, 0.52, 0.66, 1.0);
// the candle on the plinth: its flame, and how far its light reaches
uniform vec4 flame_color : source_color = vec4(1.0, 0.72, 0.32, 1.0);
uniform float candle_reach = 95.0;

varying vec2 local;

void vertex() {
	local = VERTEX;
}

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	float a = hash(i);
	float b = hash(i + vec2(1.0, 0.0));
	float c = hash(i + vec2(0.0, 1.0));
	float d = hash(i + vec2(1.0, 1.0));
	return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

float fbm(vec2 p) {
	float v = 0.0;
	float a = 0.5;
	for (int i = 0; i < 5; i++) {
		v += a * noise(p);
		p = p * 2.03 + vec2(17.0, 9.0);
		a *= 0.5;
	}
	return v;
}

// Signed distance to a box of half size `h` (negative inside).
float sd_box(vec2 p, vec2 h) {
	vec2 q = abs(p) - h;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
}

// A slab with a round top: `o` its top left corner, `s` its size.
float sd_headstone(vec2 p, vec2 o, vec2 s) {
	float r = s.x * 0.5;
	vec2 q = p - o;
	float body = sd_box(q - vec2(r, (r + s.y) * 0.5), vec2(r, max((s.y - r) * 0.5, 0.0)));
	float arch = length(q - vec2(r, r)) - r;
	return min(body, arch);
}

float plinth_top() {
	return size.y - ground_h - plinth_h;
}

// The slab and the plinth under it.
float sd_stone(vec2 p) {
	float top = plinth_top();
	float w = size.x - 2.0 * (pad + inset);
	float slab = sd_headstone(p, vec2(pad + inset, pad), vec2(w, top - pad + 4.0));
	vec2 half_plinth = vec2(size.x * 0.5 - pad, plinth_h * 0.5) - 3.0;
	float plinth = sd_box(p - vec2(size.x * 0.5, top + plinth_h * 0.5), half_plinth) - 3.0;
	return min(slab, plinth);
}

// The polished face of the slab.
float sd_face(vec2 p) {
	float o = pad + inset + face_in;
	return sd_headstone(p, vec2(o, pad + face_in), vec2(size.x - 2.0 * o, plinth_top() - pad - 2.0 * face_in));
}

// The ground's top edge at x: a low mound, tufted.
float ground_edge(float x, float seed) {
	float across = abs(x / size.x - 0.5) * 2.0;
	float mound = size.y - ground_h + 4.0 + 12.0 * across * across;
	float tufts = (noise(vec2(x * 0.42, seed)) - 0.5) * 9.0 + (noise(vec2(x * 1.6, seed + 4.0)) - 0.5) * 5.0;
	return mound + tufts;
}

vec3 earth(vec2 p, float below) {
	vec3 g = mix(grass_color.rgb, earth_color.rgb, smoothstep(3.0, 16.0, below));
	return g * (0.8 + 0.4 * fbm(p * 0.12));
}

void fragment() {
	vec4 tint = COLOR;
	vec2 p = local;
	float across = abs(p.x / size.x - 0.5) * 2.0;
	// the ground fades out at its sides and the control's bottom
	float spread = (1.0 - smoothstep(0.86, 1.0, across)) * (1.0 - smoothstep(size.y - 10.0, size.y, p.y));

	// the night behind: a cold glow round the stone's top, fading out to
	// the control's edges
	vec2 sky_c = vec2(size.x * 0.5, size.y * 0.32);
	float glow = 1.0 - smoothstep(0.0, size.x * 0.75, length((p - sky_c) * vec2(1.0, 0.6)));
	float edges = smoothstep(0.0, 24.0, p.x) * smoothstep(0.0, 24.0, size.x - p.x)
		* smoothstep(0.0, 24.0, p.y);
	vec4 col = vec4(mix(night_color.rgb, moon_color.rgb, glow * glow), glow * 0.95 * edges);
	// the shadow the stone casts down and to the right
	float shadow = 1.0 - smoothstep(0.0, 16.0, sd_stone(p - vec2(8.0, 6.0)));
	float dark = 0.5 * shadow * shadow;
	col = vec4(col.rgb * (1.0 - dark), max(col.a, dark));
	// the candle on the plinth, right of the middle, and its light
	float flicker = 0.9 + 0.1 * sin(TIME * 7.3) * sin(TIME * 3.1 + 1.7);
	vec2 wick = vec2(size.x * 0.72, plinth_top() - 21.0);
	float lit = (1.0 - smoothstep(0.0, candle_reach, length(p - wick))) * flicker;

	// the mound behind the plinth
	float back = ground_edge(p.x, 1.0) - 10.0;
	if (p.y > back) {
		col = mix(col, vec4(earth(p, p.y - back) * 0.75, 1.0), spread);
	}

	float d = sd_stone(p);
	float cover = clamp(0.5 - d, 0.0, 1.0);
	if (cover > 0.0) {
		float depth = max(-d, 0.0);
		// granite: a coarse mottle, fine grain, a few dark flecks
		float mottle = fbm(p * 0.045);
		float grain = noise(p * 0.9);
		vec3 stone = mix(stone_dark.rgb, stone_light.rgb, 0.3 + 0.6 * mottle);
		stone *= 0.9 + 0.16 * grain;
		stone *= 1.0 - 0.22 * step(0.88, noise(p * 0.55 + 31.0));
		// lit from above
		stone *= mix(1.1, 0.78, clamp(p.y / size.y, 0.0, 1.0));
		// the edges' bevel: lit up and left, shaded down and right
		vec2 e = vec2(1.0, 0.0);
		vec2 n = vec2(sd_stone(p + e.xy) - sd_stone(p - e.xy), sd_stone(p + e.yx) - sd_stone(p - e.yx));
		n = normalize(n + vec2(1e-5, 1e-5));
		float rim = 1.0 - smoothstep(0.0, 6.0, depth);
		float facing = dot(n, normalize(vec2(-0.55, -0.85)));
		stone *= 1.0 + rim * 0.45 * facing;
		// the night's light on its edge, from behind and up to the right
		stone += moon_color.rgb * rim * max(dot(n, normalize(vec2(0.75, -0.65))), 0.0) * 0.6;
		// the face: smoother, inside a carved line that shades its upper
		// left and catches the light on its lower right
		float f = sd_face(p);
		if (f < 0.0) {
			stone = mix(stone, vec3(dot(stone, vec3(0.333))), 0.25) * 1.04;
		}
		float groove = 1.0 - smoothstep(0.6, 1.6, abs(f));
		vec2 m = vec2(sd_face(p + e.xy) - sd_face(p - e.xy), sd_face(p + e.yx) - sd_face(p - e.yx));
		float lip = dot(normalize(m + vec2(1e-5, 1e-5)), normalize(vec2(-0.55, -0.85)));
		stone *= 1.0 - groove * (0.45 + 0.25 * lip);
		// moss creeping up from the foot
		float wet = smoothstep(plinth_top() - 70.0, size.y - ground_h, p.y);
		float moss = wet * smoothstep(0.48, 0.72, fbm(p * 0.07 + vec2(5.0, 2.0)));
		stone = mix(stone, moss_color.rgb * (0.7 + 0.6 * grain), moss * 0.85);
		// and the candle's, warm, on the plinth and the slab's foot
		stone += flame_color.rgb * stone * lit * lit * 0.9;
		col = mix(col, vec4(stone, 1.0), cover);
	}

	// grass in front of the plinth's foot, pebbles in its earth, flowers
	float front = ground_edge(p.x, 9.0);
	if (p.y > front) {
		vec3 e = earth(p, p.y - front);
		vec2 cell = floor(p / vec2(11.0, 7.0));
		vec2 at = (cell + vec2(hash(cell), hash(cell + 3.7))) * vec2(11.0, 7.0);
		float pebble = step(0.62, hash(cell + 9.1)) * step(front + 8.0, at.y)
			* (1.0 - smoothstep(1.4, 2.4, length((p - at) * vec2(0.8, 1.3))));
		e = mix(e, vec3(0.3, 0.29, 0.27) * (0.8 + 0.4 * hash(cell + 1.3)), pebble);
		e += flame_color.rgb * e * lit * lit * 1.2;
		col = mix(col, vec4(e, 1.0), spread);
	}
	// what grew at its foot, long dead: dry stalks with a seed head each,
	// dull as the earth, lit only by the candle (no bloom: bright dots of
	// flowers here read as cute)
	vec2 stalks[7] = vec2[](vec2(0.14, 3.0), vec2(0.2, -1.0), vec2(0.27, 5.0), vec2(0.36, 9.0),
		vec2(0.64, 8.0), vec2(0.76, 2.0), vec2(0.83, -2.0));
	for (int i = 0; i < 7; i++) {
		float h = hash(vec2(float(i), 4.2));
		float tall = 7.0 + 8.0 * h;
		vec2 foot = vec2(stalks[i].x * size.x, size.y - ground_h + 12.0 + stalks[i].y);
		// leaning, more towards its head
		float up = clamp((foot.y - p.y) / tall, 0.0, 1.0);
		float lean = (h - 0.5) * 7.0 * up * up;
		float stem = (1.0 - smoothstep(0.4, 1.1, abs(p.x - foot.x - lean)))
			* step(p.y, foot.y) * step(foot.y - tall, p.y);
		vec2 top = foot + vec2((h - 0.5) * 7.0, -tall);
		float head = 1.0 - smoothstep(1.2, 2.2, length((p - top) * vec2(1.0, 0.75)));
		vec3 dry = flower_color.rgb * (0.7 + 0.5 * h);
		dry += flame_color.rgb * dry * lit * lit;
		col = mix(col, vec4(dry, 1.0), max(stem * 0.85, head) * spread);
	}
	// the candle: a stub of wax on the plinth, its flame and the halo
	// round it
	float wax = 1.0 - smoothstep(0.0, 1.0, sd_box(p - (wick + vec2(0.0, 10.0)), vec2(4.5, 10.0)));
	col = mix(col, vec4(vec3(0.86, 0.8, 0.66) * (0.75 + 0.35 * lit), 1.0), wax);
	vec2 fp = (p - (wick + vec2(0.0, -5.0))) / vec2(3.2, 6.5);
	float flame = 1.0 - smoothstep(0.7, 1.1, length(fp + vec2(0.0, max(fp.y, 0.0) * -0.4)));
	col = mix(col, vec4(mix(flame_color.rgb, vec3(1.0, 0.95, 0.8), 0.5), 1.0), flame * flicker);
	float halo = (1.0 - smoothstep(0.0, 40.0, length(p - (wick + vec2(0.0, -5.0))))) * flicker;
	col = vec4(col.rgb + flame_color.rgb * halo * halo * 0.6, max(col.a, halo * halo * 0.8));
	COLOR = col * tint;
}
