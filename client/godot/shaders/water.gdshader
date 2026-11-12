// Water (map_view.rs): a surface over the pool's stone bed, deeper colour
// where it is deeper, ripples running over it, the dark above sheening on
// it at a glance, the torches glinting on it. It shows where water wins
// over the ground beside it (`ground_tex`, as the ground's own shader
// decides): its shore wanders through the cells, shallow there, foam
// lapping at it. The fog of war darkens it as it does the stone.
shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec4 shallow : source_color = vec4(0.1, 0.22, 0.24, 1.0);
uniform vec4 deep : source_color = vec4(0.015, 0.05, 0.075, 1.0);
uniform vec4 foam_color : source_color = vec4(0.62, 0.68, 0.68, 1.0);
// how much foam laps at its shores: none on a swamp's still water
uniform float foam_amount = 1.0;
// water in a feature's basin (a fountain's): all of it shows, whatever
// the ground round it (the cell's own floor, which would cut it away)
uniform bool basin = false;
// the dark over the water, seen in it at a glance
uniform vec4 sheen : source_color = vec4(0.16, 0.2, 0.26, 1.0);
// metres of water over which it turns from shallow to deep
uniform float depth_range = 0.4;
uniform float ripple = 1.0;
// how far toward the cold grey of memory it turns where it is
// remembered (a swamp keeps more of its own hue)
uniform float memory_tint = 0.8;
// the misty air over still water mirrored softly over all of it, in
// drifting patches (a swamp's; 0: only at a glance, as the sea's)
uniform float sky_reflect = 0.0;
// the least colour it shows by in the overview (a swamp's dark water
// read as holes in the ground)
uniform vec4 overview_min : source_color = vec4(0.0, 0.0, 0.0, 1.0);
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
// per cell, full in the channel of its ground: water, lava, ice, land
uniform sampler2D ground_tex : filter_linear, repeat_disable;
uniform sampler2D depth_tex : hint_depth_texture, filter_linear_mipmap;
uniform sampler2D fow_tex : filter_linear, repeat_disable;
// what is drawn on each cell (G: nothing), so that toward what nothing is
// known of the water's edge frays and fades into the dark as a floor's
uniform sampler2D stand_tex : filter_linear, repeat_disable;
uniform bool has_stand = false;
global uniform vec4 branch_chasm;
global uniform vec4 branch_void;
// the whole level framed (map_view's overview, as map_surface.gdshader)
global uniform float overview;

const vec2 FIELD = vec2(80.0, 21.0);
// what the overview shows water by, as the stone shows by its own light
const float OVERVIEW_GLOW = 1.2;

varying vec3 wpos;

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

float n2(vec2 p) {
	return texture(noise_tex, p).r;
}

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
// works them out, alike to the bit: where its kind wins, the water shows)
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


// three ripple fields drifting across each other
float waves(vec2 p, float t) {
	return n2(p * 0.35 + vec2(t * 0.03, t * 0.02))
		+ n2(p * 0.7 - vec2(t * 0.025, -t * 0.035)) * 0.6
		+ n2(p * 1.9 + vec2(-t * 0.05, t * 0.04)) * 0.1;
}

void fragment() {
	float t = TIME;
	// it shows where water wins over the ground beside it; how little it
	// leads there, its shore
	vec4 field = ground_blur(wpos.xz);
	float shore = 0.0;
	if (!basin && max(max(field.g, field.b), field.a) > 0.0) {
		vec4 v = ground_lead(wpos.xz, field);
		float k = ground_kind(v);
		if (k > 0.0 && k != 2.0) {
			discard;
		}
		shore = 1.0 - smoothstep(0.0, 0.3, v.r - max(max(v.g, v.b), v.a));
	}
	// toward what nothing is known of: frayed as a floor's edge is there,
	// and fading into the dark over the edge cell (it ended square, the
	// side of a slab standing out of the void)
	float edge_fade = 0.0;
	if (has_stand && !basin) {
		vec2 suv = (wpos.xz + 0.5) / FIELD;
		float empty = texture(stand_tex, suv).g;
		if (empty > 0.001) {
			if (empty + (tl2(wpos.xz * 1.7 + vec2(23.1, 9.4)) - 0.5) * 0.6 > 0.5) {
				discard;
			}
			vec2 d = vec2(1.0) / FIELD;
			float wide = (empty * 4.0
				+ (texture(stand_tex, suv + vec2(d.x, 0.0)).g + texture(stand_tex, suv - vec2(d.x, 0.0)).g
				+ texture(stand_tex, suv + vec2(0.0, d.y)).g + texture(stand_tex, suv - vec2(0.0, d.y)).g) * 2.0
				+ texture(stand_tex, suv + d).g + texture(stand_tex, suv - d).g
				+ texture(stand_tex, suv + vec2(d.x, -d.y)).g
				+ texture(stand_tex, suv + vec2(-d.x, d.y)).g) / 16.0;
			vec2 middle = floor(wpos.xz + 0.5);
			float core = 1.0 - smoothstep(0.2, 0.42, length(wpos.xz - middle));
			float fray = mix(empty, wide, 0.5) + (tl2(wpos.xz * 1.7 + vec2(23.1, 9.4)) - 0.5) * 0.3
				- core * 0.5;
			edge_fade = smoothstep(0.08, 0.45, fray) * (1.0 - 0.6 * overview);
		}
	}
	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
	// what the hero sees of it meets what they remember wandering through
	// the cells (a square of brighter water round them)
	if (seen > 0.0 && seen < 1.0) {
		float s = seen + (tl2(wpos.xz * 1.7 + vec2(41.3, 17.9)) - 0.5) * 0.5
			+ (tl2(wpos.xz * 0.42 + vec2(9.7, 33.1)) - 0.5) * 0.3;
		seen = smoothstep(0.3, 0.7, s);
	}
	const float DS = 0.02;
	float h = waves(wpos.xz, t);
	float hx = waves(wpos.xz + vec2(DS, 0.0), t);
	float hz = waves(wpos.xz + vec2(0.0, DS), t);
	vec3 wn = normalize(vec3((h - hx) / DS * ripple * 0.12, 1.0, (h - hz) / DS * ripple * 0.12));
	vec3 n = normalize((VIEW_MATRIX * vec4(wn, 0.0)).xyz);

	// how much water lies between the surface and what is under it
	float d = texture(depth_tex, SCREEN_UV).r;
	vec4 under = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, d, 1.0);
	float thickness = abs(VERTEX.z - under.z / under.w);
	float depth_t = clamp(thickness / depth_range, 0.0, 1.0);
	// shallow along its shore, whatever lies under it there
	depth_t = min(depth_t, mix(1.0, 0.1, shore));

	vec3 col = mix(shallow.rgb, deep.rgb, depth_t);
	// light caught in the crests
	col *= mix(0.85, 1.2, smoothstep(0.9, 1.4, h));
	// remembered: grey and dark as the stone around it; in the overview
	// nearly as it is, in its own colour, by its own faint light (black,
	// a moat or a sea read as a hole)
	float luma = dot(col, vec3(0.299, 0.587, 0.114));
	vec3 own = col;
	col = mix(mix(col, luma * vec3(0.54, 0.59, 0.72), mix(memory_tint, 0.15, overview))
		* mix(0.4, 0.75, overview), col, seen);

	// the dark above it, mirrored more the flatter it is seen
	float facing = clamp(dot(n, VIEW), 0.0, 1.0);
	float fresnel = 0.04 + 0.96 * pow(1.0 - facing, 5.0);
	// and broken up by the ripples into bands of sheen
	float streak = smoothstep(0.5, 0.75, n2(wpos.xz * 0.8 + wn.xz * 3.0 + t * 0.01));
	float mist = smoothstep(0.42, 0.85, n2(wpos.xz * 0.12 + wn.xz * 0.6 + vec2(t * 0.004, t * 0.003)));
	EMISSION = sheen.rgb * (fresnel * 3.0 + 0.08 + streak * 0.4) * mix(0.3, 1.0, seen)
		+ sheen.rgb * sky_reflect * mist * mix(0.6, 1.0, seen)
		+ max(own, overview_min.rgb) * overview * OVERVIEW_GLOW;

	// foam lapping at the shore: a narrow band at its very edge, its reach
	// breathing, broken up by the noise into flecks and gaps, as much as
	// there is water about to drive it (a pool of a few cells little, a
	// moat or a sea its whole); none on a swamp (`foam_amount`). Wide and
	// white, it ringed every pool: a swamp read as a cow's hide, a dark
	// floor's water as a white crackle
	vec2 wu = (wpos.xz + 0.5) / FIELD;
	vec2 w2 = vec2(2.0) / FIELD;
	float about = (texture(ground_tex, wu + vec2(w2.x, 0.0)).r + texture(ground_tex, wu - vec2(w2.x, 0.0)).r
		+ texture(ground_tex, wu + vec2(0.0, w2.y)).r + texture(ground_tex, wu - vec2(0.0, w2.y)).r
		+ texture(ground_tex, wu + w2).r + texture(ground_tex, wu - w2).r
		+ texture(ground_tex, wu + vec2(w2.x, -w2.y)).r
		+ texture(ground_tex, wu + vec2(-w2.x, w2.y)).r) / 8.0;
	float lap = 0.5 + 0.5 * sin(t * 1.3 + wpos.x * 1.9 + wpos.z * 1.4);
	float band = smoothstep(mix(0.66, 0.86, lap), 0.98, shore);
	float breakup = smoothstep(0.42, 0.72, n2(wpos.xz * 3.1 + vec2(t * 0.06, -t * 0.04)) + 0.25 * shore)
		* smoothstep(0.3, 0.6, n2(wpos.xz * 0.9 + vec2(5.3, 1.7)));
	float foam = band * breakup * foam_amount * mix(0.15, 1.0, smoothstep(0.3, 0.6, about));
	vec3 froth = mix(luma * vec3(0.54, 0.59, 0.72) * 0.4, foam_color.rgb * 0.85, seen);
	col = mix(col, froth, foam * 0.7);

	ALBEDO = col;
	ALPHA = max(mix(0.8, 0.97, depth_t), foam * 0.75);
	if (edge_fade > 0.0) {
		vec3 beyond = vec3(0.0);
		if (branch_chasm.a > 0.0) {
			float smoke = tl2(wpos.xz * 0.05 + TIME * vec2(0.01, 0.004));
			float glow = smoothstep(0.35, 0.75, tl2(wpos.xz * 0.11 + 7.0 - TIME * 0.006));
			beyond = branch_chasm.rgb * mix(0.45, 1.0, glow) * mix(0.6, 1.0, smoke);
		} else if (branch_void.a > 0.0) {
			float drift = tl2(wpos.xz * 0.045 + TIME * vec2(0.008, 0.003));
			float grain = tl2(wpos.xz * 0.23 + vec2(3.1, 7.7));
			beyond = branch_void.rgb * mix(0.55, 1.0, drift) * mix(0.8, 1.0, grain) * (1.0 - overview);
		}
		ALBEDO *= 1.0 - edge_fade;
		EMISSION = mix(EMISSION, beyond, edge_fade);
		ALPHA = mix(ALPHA, 1.0, edge_fade);
	}
	// rough enough, and rougher where the ripples turn faster than
	// a pixel: the lights glint as soft streaks along them, not as
	// points (stars)
	float turn = length(fwidth(wn));
	ROUGHNESS = clamp(0.28 + turn * 6.0, 0.28, 0.85);
	METALLIC = 0.0;
	SPECULAR = 0.45;
	// nor does water mirror the metal sky (its glints are the lights')
	RADIANCE = vec4(0.0, 0.0, 0.0, 1.0);
	NORMAL = n;
}
