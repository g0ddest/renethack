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
// the dark over the water, seen in it at a glance
uniform vec4 sheen : source_color = vec4(0.16, 0.2, 0.26, 1.0);
// metres of water over which it turns from shallow to deep
uniform float depth_range = 0.4;
uniform float ripple = 1.0;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
// per cell, full in the channel of its ground: water, lava, ice, land
uniform sampler2D ground_tex : filter_linear, repeat_disable;
uniform sampler2D depth_tex : hint_depth_texture, filter_linear_mipmap;
uniform sampler2D fow_tex : filter_linear, repeat_disable;
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
	// and there wandering on a larger scale too, a long shore never
	// straight along the cells
	vec2 q = p * 0.42;
	vec4 slow = vec4(tl2(q + vec2(7.3, 2.9)), tl2(q + vec2(1.7, 15.1)),
		tl2(q + vec2(13.9, 8.3)), tl2(q + vec2(4.1, 11.7)));
	f += (slow - 0.5) * 0.3 * big * step(0.004, wide);
	vec2 middle = floor(p + 0.5);
	ivec2 at = clamp(ivec2(middle), ivec2(0), ivec2(FIELD) - 1);
	float core = 1.0 - smoothstep(0.15, 0.45, length(p - middle));
	return f + texelFetch(ground_tex, at, 0) * core * 0.35;
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
	if (max(max(field.g, field.b), field.a) > 0.0) {
		vec4 v = ground_lead(wpos.xz, field);
		float k = ground_kind(v);
		if (k > 0.0 && k != 2.0) {
			discard;
		}
		shore = 1.0 - smoothstep(0.0, 0.3, v.r - max(max(v.g, v.b), v.a));
	}
	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
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
	col = mix(mix(col, luma * vec3(0.54, 0.59, 0.72), mix(0.8, 0.15, overview))
		* mix(0.4, 0.75, overview), col, seen);

	// the dark above it, mirrored more the flatter it is seen
	float facing = clamp(dot(n, VIEW), 0.0, 1.0);
	float fresnel = 0.04 + 0.96 * pow(1.0 - facing, 5.0);
	// and broken up by the ripples into bands of sheen
	float streak = smoothstep(0.5, 0.75, n2(wpos.xz * 0.8 + wn.xz * 3.0 + t * 0.01));
	EMISSION = sheen.rgb * (fresnel * 3.0 + 0.08 + streak * 0.4) * mix(0.3, 1.0, seen)
		+ own * overview * OVERVIEW_GLOW;

	// foam lapping at the shore: a band at its very edge, its reach
	// breathing, broken up by the noise
	float lap = 0.5 + 0.5 * sin(t * 1.3 + wpos.x * 1.9 + wpos.z * 1.4);
	float band = smoothstep(mix(0.45, 0.8, lap), 0.97, shore);
	float breakup = smoothstep(0.3, 0.62, n2(wpos.xz * 2.4 + vec2(t * 0.06, -t * 0.04)) + 0.35 * shore);
	float foam = band * breakup;
	vec3 froth = mix(luma * vec3(0.54, 0.59, 0.72) * 0.4, foam_color.rgb, seen);
	col = mix(col, froth, foam * 0.85);

	ALBEDO = col;
	ALPHA = max(mix(0.8, 0.97, depth_t), foam * 0.9);
	// rough enough, and rougher where the ripples turn faster than
	// a pixel: the lights glint as soft streaks along them, not as
	// points (stars)
	float turn = length(fwidth(wn));
	ROUGHNESS = clamp(0.28 + turn * 6.0, 0.28, 0.85);
	METALLIC = 0.0;
	SPECULAR = 0.45;
	NORMAL = n;
}
