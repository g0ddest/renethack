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
// what the ground under the level shows where nothing is known
// (map_surface.gdshader's), for the crust cooled toward it
global uniform vec4 branch_chasm;
global uniform vec4 branch_void;
global uniform float overview;
// per cell, full in the channel of its ground: water, lava, ice, land
uniform sampler2D ground_tex : filter_linear, repeat_disable;
// per cell: G full where nothing is drawn at all (surface.rs `StandField`)
uniform sampler2D stand_tex : filter_linear, repeat_disable;
// a photographed flow (TextureCan's Volcanic Lava Flow): its crust and
// its molten channels; without it the crust is all noise
uniform sampler2D flow_albedo : source_color, filter_linear_mipmap, repeat_enable;
uniform sampler2D flow_glow : source_color, filter_linear_mipmap, repeat_enable;
uniform bool has_flow = false;
// how much of a field's middle runs molten (map_view sets it by the
// branch): its rivers and pools; the rest cooled crust
uniform float running = 0.35;
uniform float flow_scale = 0.45;

const vec2 FIELD = vec2(80.0, 21.0);

varying vec3 wpos;

// the ground's noise at its own level (map_surface.gdshader's `tl2`)
float tl2(vec2 p) {
	return textureLod(noise_tex, p * 0.125, 0.0).r;
}

// the cells' ground about a point: bilinear between the cells' middles,
// smoothed over two cells and a half where two kinds each hold a share of
// the ground about (a big field's shore runs smooth past its steps, and
// wanders over a couple of metres), and
// every cell's own kind holding its middle (a lone cell, a stream a cell
// wide keeps its place). The same in every shader of the ground, so each
// decides alike where a shore runs.
vec4 ground_blur(vec2 p) {
	vec2 uv = (p + 0.5) / FIELD;
	vec4 near = texture(ground_tex, uv);
	vec2 d = vec2(1.0) / FIELD;
	vec4 wide = (near * 4.0
		+ (texture(ground_tex, uv + vec2(d.x, 0.0)) + texture(ground_tex, uv - vec2(d.x, 0.0))
		+ texture(ground_tex, uv + vec2(0.0, d.y)) + texture(ground_tex, uv - vec2(0.0, d.y))) * 2.0
		+ texture(ground_tex, uv + d) + texture(ground_tex, uv - d)
		+ texture(ground_tex, uv + vec2(d.x, -d.y))
		+ texture(ground_tex, uv + vec2(-d.x, d.y))) / 16.0;
	// how much of the ground about the second kind holds: a shore of a
	// big field where it is a fair share
	float hi = max(wide.r, wide.g);
	float lo = min(wide.r, wide.g);
	float hi2 = max(wide.b, wide.a);
	float lo2 = min(wide.b, wide.a);
	float second = max(min(hi, hi2), max(lo, lo2));
	float big = smoothstep(0.2, 0.35, second);
	vec4 f = mix(near, wide, big);
	// and a big field's shore smoothed over a couple of cells more, by the
	// ground two cells about: its steps of two and three cells run smooth
	// (a stream a cell or two wide holds too little of that ground to)
	vec2 d2 = d * 2.0;
	vec4 wider = (wide * 4.0
		+ (texture(ground_tex, uv + vec2(d2.x, 0.0)) + texture(ground_tex, uv - vec2(d2.x, 0.0))
		+ texture(ground_tex, uv + vec2(0.0, d2.y)) + texture(ground_tex, uv - vec2(0.0, d2.y))) * 2.0
		+ texture(ground_tex, uv + d2) + texture(ground_tex, uv - d2)
		+ texture(ground_tex, uv + vec2(d2.x, -d2.y))
		+ texture(ground_tex, uv + vec2(-d2.x, d2.y))) / 16.0;
	float second_w = max(min(max(wider.r, wider.g), max(wider.b, wider.a)),
		max(min(wider.r, wider.g), min(wider.b, wider.a)));
	f = mix(f, wider, smoothstep(0.38, 0.48, second_w) * big);
	// and there wandering on a larger scale too, a long shore never
	// straight along the cells
	vec2 q = p * 0.42;
	vec4 slow = vec4(tl2(q + vec2(7.3, 2.9)), tl2(q + vec2(1.7, 15.1)),
		tl2(q + vec2(13.9, 8.3)), tl2(q + vec2(4.1, 11.7)));
	f += (slow - 0.5) * 0.45 * big * step(0.004, wide);
	// and a nearer wander over that: a long shore along a row of cells
	// ran straight, a field's bays square (the Plane of Fire's)
	vec2 q2 = p * 0.95;
	vec4 mid = vec4(tl2(q2 + vec2(19.7, 3.3)), tl2(q2 + vec2(6.1, 23.9)),
		tl2(q2 + vec2(27.3, 12.7)), tl2(q2 + vec2(10.9, 30.1)));
	f += (mid - 0.5) * 0.22 * big * step(0.004, wide);
	vec2 middle = floor(p + 0.5);
	ivec2 at = clamp(ivec2(middle), ivec2(0), ivec2(FIELD) - 1);
	float core = 1.0 - smoothstep(0.15, 0.45, length(p - middle));
	// and wherever the cells round a point are all of one kind (between
	// two cells of a line of water, the middle of a field) that kind,
	// whatever the noise: a stream a cell wide never breaks into puddles
	vec4 whole = smoothstep(vec4(0.75), vec4(0.95), near);
	return f + texelFetch(ground_tex, at, 0) * core * 0.35 + whole;
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
	// how near a shore it lies, over a cell or so (it runs hot there): by
	// the blurred ground itself, smooth (by the shore's lead it stepped on
	// where the blur's reach began, a square block of glow)
	float rival = max(max(field.r, field.b), field.a);
	float nearshore = smoothstep(0.04, 0.45, rival + (tl2(wpos.xz * 1.1 + vec2(7.7, 3.1)) - 0.5) * 0.1);
	if (rival > 0.0) {
		vec4 v = ground_lead(wpos.xz, field);
		float k = ground_kind(v);
		if (k > 0.0 && k != 3.0) {
			discard;
		}
		shore = 1.0 - smoothstep(0.0, 0.3, v.g - max(max(v.r, v.b), v.a));
	}
	// where nothing is drawn beside it (past what the hero has seen) its
	// edge frays as a floor's does, wandering over a couple of metres and
	// its corners rounded over a cell or so, its crust cooling dark there
	// (a field half seen stepped along the cells); each cell's middle
	// keeps its lava
	vec2 uv = (wpos.xz + 0.5) / FIELD;
	float empty = texture(stand_tex, uv).g;
	vec2 d = vec2(1.0) / FIELD;
	float wide = (empty * 4.0
		+ (texture(stand_tex, uv + vec2(d.x, 0.0)).g + texture(stand_tex, uv - vec2(d.x, 0.0)).g
		+ texture(stand_tex, uv + vec2(0.0, d.y)).g + texture(stand_tex, uv - vec2(0.0, d.y)).g) * 2.0
		+ texture(stand_tex, uv + d).g + texture(stand_tex, uv - d).g
		+ texture(stand_tex, uv + vec2(d.x, -d.y)).g
		+ texture(stand_tex, uv + vec2(-d.x, d.y)).g) / 16.0;
	float fray = 0.0;
	if (empty > 0.001) {
		vec2 middle = floor(wpos.xz + 0.5);
		float core = 1.0 - smoothstep(0.15, 0.4, length(wpos.xz - middle));
		fray = mix(empty, wide, 0.6) + (tl2(wpos.xz * 1.7 + vec2(23.1, 9.4)) - 0.5) * 0.45
			+ (tl2(wpos.xz * 0.42 + vec2(5.3, 17.9)) - 0.5) * 0.3 - core;
	}
	vec2 flow = vec2(t * 0.015, t * 0.008);
	float plates = texture(noise_tex, wpos.xz * 0.45 + flow).r;
	float fine = texture(noise_tex, wpos.xz * 1.7 - flow * 2.0).r;
	// cracks between the plates of crust, wider where it is hotter
	float heat = texture(noise_tex, wpos.xz * 0.12 + vec2(t * 0.004, 0.0)).r;
	float gap = 0.012 + heat * 0.03;
	float crack = 1.0 - smoothstep(gap * 0.4, gap, abs(plates - 0.5) - (fine - 0.5) * 0.08);
	// (it breathes slowly)
	float seethe = 0.78 + 0.22 * sin(t * 0.7 + plates * 5.0);
	vec3 glow = molten.rgb * crack * seethe;
	vec3 rock = crust.rgb * mix(0.7, 1.3, fine);
	if (has_flow) {
		// the flow creeps; the noise's cracks open on its channels. Its
		// photograph repeats every two metres: two takes of it, turned and
		// scaled apart and warped by a slow noise, each where the noise
		// has it (one take banded a big field in rows of the same blots)
		vec2 base = wpos.xz * flow_scale;
		vec2 warp = vec2(tl2(wpos.xz * 0.35 + vec2(3.7, 1.3)), tl2(wpos.xz * 0.35 + vec2(8.1, 5.9))) - 0.5;
		vec2 uv1 = base + warp * 0.4 + flow * 0.6;
		const mat2 TURN = mat2(vec2(0.799, 0.602), vec2(-0.602, 0.799));
		vec2 uv2 = TURN * (base * 0.77) + vec2(0.31, 0.57) - warp * 0.35 + flow * 0.45;
		float take = smoothstep(0.4, 0.6, tl2(wpos.xz * 0.21 + vec2(13.3, 2.1)));
		vec3 g = mix(texture(flow_glow, uv1).rgb, texture(flow_glow, uv2).rgb, take);
		float channel = dot(g, vec3(0.5, 0.4, 0.1));
		crack = max(channel * 1.2, crack * 0.35);
		glow = molten.rgb * crack * seethe;
		rock = mix(texture(flow_albedo, uv1).rgb, texture(flow_albedo, uv2).rgb, take) * 0.6;
	}

	// most of a field is a dark cooled crust, its veins aglow faintly (it
	// glowed edge to edge, one bright carpet): it runs molten along its
	// shores, in a few rivers wandering through it with the flow, and in
	// pools; its very edge a line of glow where it meets the ground
	vec2 bend = vec2(tl2(wpos.xz * 0.09 + vec2(4.7, 8.3)), tl2(wpos.xz * 0.09 + vec2(12.1, 1.9))) - 0.5;
	vec2 drift = wpos.xz + vec2(t * 0.02, t * 0.011);
	float course = tl2(drift * 0.13 + bend * 1.6 + vec2(17.3, 5.1));
	float river = 1.0 - smoothstep(running * 0.09, running * 0.09 + 0.03, abs(course - 0.5));
	float pool = smoothstep(0.78 - running * 0.25, 0.86 - running * 0.25,
		tl2(wpos.xz * 0.21 + vec2(9.9, 21.3)));
	float hot = clamp(max(river, pool) + nearshore, 0.0, 1.0);
	crack *= mix(0.06, 1.0, hot);
	// (the cooled crust black-brown: the flow's photograph is a red rock)
	rock = mix(crust.rgb * mix(0.7, 1.3, fine), rock, 0.25 + 0.75 * hot);
	float rim = smoothstep(0.75, 0.97, shore);
	glow = molten.rgb * max(crack, rim * 0.8) * seethe;

	// toward what nothing is known of its crust closes over it and past
	// that it lies cooled black (a hole there showed the ground under the
	// level lit, a square of it); no line of glow where it closes (along
	// the cells it drew the field's blocks in bright lines)
	// (a crust dying back over a cell and a half from every cell nothing
	// is drawn on cut a maze's field into crust blocks and glow blocks)
	float cooled = 0.0;
	if (empty > 0.001) {
		cooled = smoothstep(0.33, 0.39, fray);
		crack *= 1.0 - smoothstep(-0.05, 0.3, fray);
		glow = molten.rgb * max(crack, rim * 0.8) * seethe * (1.0 - cooled);
		rock *= mix(1.0 - 0.5 * smoothstep(0.1, 0.33, fray), 0.06, cooled);
	}
	float seen = texture(fow_tex, uv).r;
	// what the hero sees of it meets what they remember along a curved,
	// wandering line, not along the cells (a big field half seen stepped,
	// its lava dimmed cell by cell); never more than about half a cell
	// either way of the cells' own edge, so each cell still reads plainly
	// as in view or remembered, and softly: a field mostly crust, its
	// remembered glow dimmed as far as before read as crust blocks among
	// the glow in view (a maze's line of sight is rectangles)
	if (seen > 0.001 && seen < 0.999) {
		float s = seen + (tl2(wpos.xz * 1.7 + vec2(41.3, 17.9)) - 0.5) * 0.4
			+ (tl2(wpos.xz * 0.42 + vec2(9.7, 33.1)) - 0.5) * 0.3;
		seen = smoothstep(0.25, 0.75, s);
	}
	ALBEDO = mix(rock, molten.rgb * 0.3, crack);
	ROUGHNESS = mix(0.9, 0.4, crack);
	// (the crust dull: the lava's own lights shone on it as a red sheen)
	SPECULAR = mix(0.08, 0.5, max(crack, hot));
	// nor does it mirror the metal sky (map_view's): it shines by its fire
	RADIANCE = vec4(0.0, 0.0, 0.0, 1.0);
	EMISSION = (glow * energy + rock * 0.15 * heat) * mix(0.62, 1.0, seen);
	// and its crust cooled toward the unknown fades, past its line, into
	// what the ground under the level shows where nothing is known
	// (Gehennom's smouldering chasm, elsewhere the smoky dark: as
	// map_surface draws them), so the field's edge there runs through
	// the cells: a dark crust stopped at each cell's edge against what lay
	// beyond stepped along them
	if (cooled > 0.0) {
		vec3 beyond = vec3(0.0);
		if (branch_chasm.a > 0.0) {
			float smoke = tl2(wpos.xz * 0.05 + TIME * vec2(0.01, 0.004));
			float deep = smoothstep(0.35, 0.75, tl2(wpos.xz * 0.11 + 7.0 - TIME * 0.006));
			beyond = branch_chasm.rgb * mix(0.45, 1.0, deep) * mix(0.6, 1.0, smoke);
		} else if (branch_void.a > 0.0) {
			float drift = tl2(wpos.xz * 0.045 + TIME * vec2(0.008, 0.003));
			float grain = tl2(wpos.xz * 0.23 + vec2(3.1, 7.7));
			beyond = branch_void.rgb * mix(0.55, 1.0, drift) * mix(0.8, 1.0, grain) * (1.0 - overview);
		}
		float gone = smoothstep(0.37, 0.5, fray);
		ALBEDO *= 1.0 - gone;
		EMISSION = mix(EMISSION, beyond, gone);
	}
}
