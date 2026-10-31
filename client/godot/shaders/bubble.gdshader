// The floor of an air bubble on the Plane of Water (map_view, `Under::Deep`:
// its air cells): a pale membrane with the light gathering in threads over
// it, wavering as the water moves, the deep seen through it. Its edge
// curves where the water round it closes in (`ground_tex`, the field the
// shores read: rounded at a corner, a little uneven), never a cell's
// square.
shader_type spatial;
render_mode unshaded, depth_draw_never, cull_disabled, shadows_disabled;

uniform sampler2D ground_tex : filter_linear, repeat_disable;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;

const vec2 FIELD = vec2(80.0, 21.0);

varying vec3 wpos;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

vec2 hash2(vec2 p) {
	return vec2(hash(p), hash(p + 19.19));
}

// bright threads where the light gathers: the edges between drifting
// cells of a Voronoi pattern
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
	return 1.0 - smoothstep(0.0, 0.07, d2 - d1);
}

// the water about a point as the bubble's edge reads it: bilinear between
// the cells' middles, smoothed over two cells and a half where water and
// air each hold a share about (a bubble of a few cells reads round, not
// as their stepped cross), an air cell's middle always its own (the same
// in the bubble's floor and its skin)
float bubble_water(vec2 p) {
	vec2 uv = (p + 0.5) / FIELD;
	float near = texture(ground_tex, uv).r;
	vec2 d = vec2(1.0) / FIELD;
	float wide = (near * 4.0
		+ (texture(ground_tex, uv + vec2(d.x, 0.0)).r + texture(ground_tex, uv - vec2(d.x, 0.0)).r
		+ texture(ground_tex, uv + vec2(0.0, d.y)).r + texture(ground_tex, uv - vec2(0.0, d.y)).r) * 2.0
		+ texture(ground_tex, uv + d).r + texture(ground_tex, uv - d).r
		+ texture(ground_tex, uv + vec2(d.x, -d.y)).r
		+ texture(ground_tex, uv + vec2(-d.x, d.y)).r) / 16.0;
	// (never eating far into the air: a bubble a cell or two across
	// stays whole)
	float f = min(mix(near, wide, smoothstep(0.2, 0.35, min(wide, 1.0 - wide))), near + 0.3);
	vec2 middle = floor(p + 0.5);
	ivec2 at = clamp(ivec2(middle), ivec2(0), ivec2(FIELD) - 1);
	float air = 1.0 - texelFetch(ground_tex, at, 0).r;
	return f - air * (1.0 - smoothstep(0.15, 0.45, length(p - middle))) * 0.35;
}

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	// (the threads wander: no straight edge)
	vec2 p = wpos.xz * 0.75;
	p += vec2(sin(p.y * 2.3 + TIME * 0.7), cos(p.x * 2.1 - TIME * 0.6)) * 0.18;
	float c = threads(p, TIME * 0.5) * 0.6
		+ threads(p * 1.6 + 3.1, TIME * 0.65 + 1.3) * 0.4;
	// how much water is about this point (half at a side meeting it, more
	// at a corner), unevenly
	float water = bubble_water(wpos.xz);
	water += (textureLod(noise_tex, wpos.xz * 0.21, 0.0).r - 0.5) * 0.18;
	float inside = smoothstep(0.62, 0.44, water);
	// (faint: brighter, they were a white web over the floor)
	ALBEDO = vec3(0.3, 0.55, 0.7) + vec3(0.55, 0.85, 1.0) * c * 0.45;
	ALPHA = (0.26 + c * 0.18) * inside;
}
