// The end screen's tombstone (genl_outrip's picture, drawn): a granite
// headstone with a round top on a plinth, lit from the top left, its face
// polished inside a carved line, moss at its foot, on a mound of earth
// and grass with a few flowers, casting a soft shadow. tombstone.rs sets
// `size` to the control's and puts the stone's words on its face.
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
uniform float ground_h = 42.0;
uniform vec4 stone_light : source_color = vec4(0.6, 0.58, 0.54, 1.0);
uniform vec4 stone_dark : source_color = vec4(0.26, 0.25, 0.24, 1.0);
uniform vec4 moss_color : source_color = vec4(0.2, 0.26, 0.12, 1.0);
uniform vec4 earth_color : source_color = vec4(0.075, 0.055, 0.04, 1.0);
uniform vec4 grass_color : source_color = vec4(0.12, 0.18, 0.07, 1.0);
uniform vec4 flower_color : source_color = vec4(0.86, 0.82, 0.64, 1.0);

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
	// the mound fades out at its sides and the control's bottom
	float spread = (1.0 - smoothstep(0.78, 1.0, across)) * (1.0 - smoothstep(size.y - 10.0, size.y, p.y));

	// the shadow the stone casts down and to the right
	float shadow = 1.0 - smoothstep(0.0, 16.0, sd_stone(p - vec2(8.0, 6.0)));
	vec4 col = vec4(0.0, 0.0, 0.0, 0.5 * shadow * shadow);

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
		col = mix(col, vec4(stone, 1.0), cover);
	}

	// grass in front of the plinth's foot, and flowers in it
	float front = ground_edge(p.x, 9.0);
	if (p.y > front) {
		col = mix(col, vec4(earth(p, p.y - front), 1.0), spread);
	}
	vec2 flowers[4] = vec2[](vec2(0.2, -1.0), vec2(0.27, 5.0), vec2(0.76, 2.0), vec2(0.83, -2.0));
	for (int i = 0; i < 4; i++) {
		vec2 c = vec2(flowers[i].x * size.x, size.y - ground_h + 8.0 + flowers[i].y);
		float r = length(p - c);
		float petal = 1.0 - smoothstep(2.2, 3.4, r);
		vec3 bloom = mix(vec3(0.85, 0.68, 0.25), flower_color.rgb, smoothstep(0.6, 1.4, r));
		col = mix(col, vec4(bloom, 1.0), petal);
	}
	COLOR = col * tint;
}
