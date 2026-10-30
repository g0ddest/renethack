// Lava (map_view.rs): a crust of dark cooling rock floating on a molten
// glow that shows through its cracks and seethes, drifting slowly; it
// lights itself. It shows where lava wins over the ground beside it
// (`ground_tex`, as the ground's own shader decides): along its shore it
// cools to a crust, its very edge aglow. The fog of war dims what is
// remembered.
shader_type spatial;
render_mode blend_mix, cull_back, diffuse_burley;

uniform vec4 molten : source_color = vec4(1.0, 0.35, 0.06, 1.0);
uniform vec4 crust : source_color = vec4(0.07, 0.05, 0.045, 1.0);
uniform float energy = 3.5;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
uniform sampler2D fow_tex : filter_linear, repeat_disable;
// per cell, full in the channel of its ground: water, lava, ice, land
uniform sampler2D ground_tex : filter_linear, repeat_disable;
// a photographed flow (TextureCan's Volcanic Lava Flow): its crust and
// its molten channels; without it the crust is all noise
uniform sampler2D flow_albedo : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D flow_glow : source_color, filter_linear_mipmap, repeat_enable;
uniform bool has_flow = false;
uniform float flow_scale = 0.45;

const vec2 FIELD = vec2(80.0, 21.0);

varying vec3 wpos;

// the ground's noise at its own level (map_surface.gdshader's `tl2`)
float tl2(vec2 p) {
	return textureLod(noise_tex, p * 0.125, 0.0).r;
}

// the cells' ground about a point, blurred over a cell and a half: a
// shore cuts the cells' corners, never steps along their edges (the same
// in every shader of the ground)
vec4 ground_blur(vec2 p) {
	vec2 uv = (p + 0.5) / FIELD;
	vec2 d = vec2(0.6) / FIELD;
	return (texture(ground_tex, uv) * 2.0
		+ texture(ground_tex, uv + d) + texture(ground_tex, uv - d)
		+ texture(ground_tex, uv + vec2(d.x, -d.y))
		+ texture(ground_tex, uv + vec2(-d.x, d.y))) / 6.0;
}

// the kinds' strengths at a point of the ground (as map_surface.gdshader
// works them out, alike to the bit: where its kind wins, the lava shows)
vec4 ground_lead(vec2 p, vec4 f) {
	vec2 q = p * 1.7;
	vec4 n = vec4(tl2(q + vec2(3.1, 7.7)), tl2(q + vec2(11.3, 1.9)),
		tl2(q + vec2(5.9, 13.1)), tl2(q + vec2(17.7, 4.3)));
	return (f + (n - 0.5) * 0.6) * step(0.004, f);
}

// the kind that wins: 1 land, 2 water, 3 lava, 4 ice (0: none near)
float ground_kind(vec4 v) {
	float k = 0.0;
	float best = 0.0;
	if (v.a > best) { best = v.a; k = 1.0; }
	if (v.r > best) { best = v.r; k = 2.0; }
	if (v.g > best) { best = v.g; k = 3.0; }
	if (v.b > best) { best = v.b; k = 4.0; }
	return k;
}



void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	// the surface heaves a little
	float h = texture(noise_tex, wpos.xz * 0.2 + TIME * 0.01).r;
	VERTEX.y += (h - 0.5) * 0.03;
}

void fragment() {
	float t = TIME;
	// it shows where lava wins over the ground beside it; how little it
	// leads there, its shore
	vec4 field = ground_blur(wpos.xz);
	float shore = 0.0;
	if (max(max(field.r, field.b), field.a) > 0.0) {
		vec4 v = ground_lead(wpos.xz, field);
		float k = ground_kind(v);
		if (k > 0.0 && k != 3.0) {
			discard;
		}
		shore = 1.0 - smoothstep(0.0, 0.3, v.g - max(max(v.r, v.b), v.a));
	}
	vec2 flow = vec2(t * 0.015, t * 0.008);
	float plates = texture(noise_tex, wpos.xz * 0.45 + flow).r;
	float fine = texture(noise_tex, wpos.xz * 1.7 - flow * 2.0).r;
	// cracks between the plates of crust, wider where it is hotter
	float heat = texture(noise_tex, wpos.xz * 0.12 + vec2(t * 0.004, 0.0)).r;
	float gap = 0.012 + heat * 0.03;
	float crack = 1.0 - smoothstep(gap * 0.4, gap, abs(plates - 0.5) - (fine - 0.5) * 0.08);
	float seethe = 0.8 + 0.2 * sin(t * 2.1 + plates * 13.0);
	vec3 glow = molten.rgb * crack * seethe;
	vec3 rock = crust.rgb * mix(0.7, 1.3, fine);
	if (has_flow) {
		// the flow creeps; the noise's cracks open on its channels
		vec2 uv = wpos.xz * flow_scale + flow * 0.6;
		vec3 g = texture(flow_glow, uv).rgb;
		float channel = dot(g, vec3(0.5, 0.4, 0.1));
		crack = max(channel * 1.2, crack * 0.35);
		glow = molten.rgb * crack * seethe;
		rock = texture(flow_albedo, uv).rgb * 0.6;
	}

	// along its shore the crust thickens over it, its very edge a line of
	// glow where it meets the ground
	float rim = smoothstep(0.75, 0.97, shore);
	crack *= 1.0 - smoothstep(0.2, 0.75, shore) * (1.0 - rim);
	glow = molten.rgb * max(crack, rim * 0.8) * seethe;

	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
	ALBEDO = mix(rock, molten.rgb * 0.3, crack);
	ROUGHNESS = mix(0.9, 0.4, crack);
	EMISSION = (glow * energy + rock * 0.15 * heat) * mix(0.35, 1.0, seen);
}
