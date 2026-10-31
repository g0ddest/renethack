// The skin of an air bubble on the Plane of Water (map_view: on an air
// cell's sides that meet the water, a meniscus curving in over it, its
// face outwards): thin and clear, the water beyond seen bent through it,
// a bright line where it meets the floor, fading as it rises, faintly
// iridescent and brightest where it is seen edge on. Seen from
// outside, in front of the bubble, it is fainter, and so is any of it in
// front of the hero, close to them: whoever stands at an edge, in or out
// of the bubble, reads through it.
shader_type spatial;
render_mode unshaded, depth_draw_never, cull_disabled, shadows_disabled;

uniform sampler2D screen : hint_screen_texture, filter_linear_mipmap;
// the water about each point (the bubble's floor reads it too): the
// skin fades out at a corner, so the bubble's edge turns round it
uniform sampler2D ground_tex : filter_linear, repeat_disable;
// the noise the bubble's floor wanders its edge by
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;

const vec2 FIELD = vec2(80.0, 21.0);
// how high it curves over the ground (metres)
uniform float tall = 0.8;
// how much of it shows seen from outside, or in front of the hero
uniform float outside = 0.3;

global uniform vec3 hero_pos;

varying vec3 wpos;

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
	float edge = 1.0 - abs(dot(normalize(NORMAL), normalize(VIEW)));
	vec2 bend = vec2(sin(wpos.y * 7.0 + TIME * 1.3 + wpos.x * 3.0),
		cos(wpos.x * 5.0 + wpos.z * 5.0 - TIME * 1.1)) * 0.008;
	vec3 behind = textureLod(screen, SCREEN_UV + bend, 0.0).rgb;
	// a soap film's colours, faint: mostly clear water's own blue-white
	vec3 film = 0.55 + 0.45 * cos(6.2831 * (vec3(0.0, 0.33, 0.67) + edge * 1.2 + wpos.y * 0.6 + TIME * 0.05));
	film = mix(vec3(0.85, 0.93, 1.0), film, 0.3);
	float h = clamp(wpos.y / tall, 0.0, 1.0);
	// the line where water meets the floor, and the sheet over it fading
	// as it curves in
	float rim = exp(-h * 14.0);
	float sheet = (1.0 - smoothstep(0.25, 1.0, h)) * 0.22;
	ALBEDO = mix(behind, film, 0.08 + 0.15 * edge)
		+ vec3(0.6, 0.85, 1.0) * (rim * 0.45 + pow(edge, 3.0) * 0.25);
	// in front of the hero (their row or south of it, the camera's side)
	// and within a step or so of them
	float near = 1.0 - smoothstep(1.0, 1.8, length(wpos.xz - hero_pos.xz));
	float before = step(hero_pos.z - 0.6, wpos.z) * near;
	float faint = FRONT_FACING ? outside : mix(1.0, outside, before);
	// only where the bubble's floor has its edge (`bubble.gdshader`): it
	// fades where the round edge leaves the cells' side, out at a corner
	// or in over an inner one
	float water = bubble_water(wpos.xz)
		+ (textureLod(noise_tex, wpos.xz * 0.21, 0.0).r - 0.5) * 0.18;
	float corner = 1.0 - smoothstep(0.08, 0.2, abs(water - 0.53));
	ALPHA = clamp(sheet + rim * 0.55 + 0.3 * edge * (1.0 - h), 0.0, 1.0) * faint * corner;
}
