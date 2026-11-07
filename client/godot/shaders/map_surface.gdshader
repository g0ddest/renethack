// The map's stone, earth and wood (surface.rs): world-space triplanar with
// a top texture of its own (a wall's masonry sides under a rock top), slow
// variation over the level instead of a brightness per cell, wet patches
// on floors, a fine detail normal, grime at the foot of walls, and the
// fog of war: what the hero sees now in full colour, what they remember
// darker, cold and grey, what they never saw fading into darkness.
// Walls and rock between the eye and the hero open round them: a hole in
// the stone, its rim crumbling in the world's own noise (no screen dither:
// with no temporal blend it showed as a grid of dots).
// The ground (a floor, ice) shows only where its kind wins over the
// cells' beside it (`ground_kind`): a shore wanders, wet, scorched or
// frosted along it, instead of running along the cells' edges.
shader_type spatial;
render_mode diffuse_burley, specular_schlick_ggx, cull_back;

uniform sampler2D side_albedo : source_color, hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D side_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D side_orm : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D top_albedo : source_color, hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D top_normal : hint_normal, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D top_orm : hint_default_white, filter_linear_mipmap_anisotropic, repeat_enable;
uniform sampler2D detail_normal : hint_normal, filter_linear_mipmap, repeat_enable;
// per cell of the level (80 x 21): R seen now, G known, B wall, A near
// something known (the darkness beyond fades by it)
uniform sampler2D fow_tex : filter_linear, repeat_disable;
// a tiling fractal noise (surface.rs): the fragments read noise from it
// rather than work it out (a fragment of rock would hash hundreds of times)
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
// per cell of the level, full in the channel of its ground: water, lava,
// ice, land (surface.rs `GroundField`)
uniform sampler2D ground_tex : filter_linear, repeat_disable;
// per cell of the level: R full where something stands or lies (a
// creature, a thing: the fire's cracks keep clear of it), G where nothing
// is drawn at all (a floor frays towards it) (surface.rs `StandField`)
uniform sampler2D stand_tex : filter_linear, repeat_disable;
// the ground's own kind (1 land, 4 ice), shown where it wins; 0: whatever
// lies around it
uniform float edge_kind = 0.0;

uniform vec4 side_color : source_color = vec4(1.0);
uniform vec4 top_color : source_color = vec4(1.0);
uniform float side_scale = 0.5;
uniform float top_scale = 0.5;
uniform float normal_strength = 1.0;
uniform float detail_strength = 0.3;
uniform float detail_scale = 2.0;
uniform float roughness_bias = 0.0;
uniform float metallic = 0.0;
// untextured: a plain roughness
uniform float roughness = 1.0;
// how much a surface mirrors the lights (dry rock hardly does: under a
// light held close, a sheen over it reads as cloth or as wet)
uniform float specular = 0.5;
// albedo varies over the level by this much
uniform float macro = 1.0;
// wet patches (floors): darker and glossy
uniform float wet = 0.0;
// darker at the foot of walls (floors)
uniform float grime = 0.0;
// opens between the eye and the hero, all but its top line
uniform float occlude = 0.0;
uniform float solid_top = 2.0;
// a broken stump of masonry: pushed by a noise of the world position
// (metres), its top in ridges
uniform float displace = 0.0;
// rock: broken in chunks, each pushed its own way by up to this (metres),
// as rock breaks along its joints
uniform float chunks = 0.0;
// lit as it is cut: each face of the mesh by its own slope, flat
uniform float facets = 0.0;
// natural rock (a cave's): pushed in and out by lumps of a metre and of a
// hand, by up to this (metres), smooth, never cut in facets; and rounded
// down by up to `round_off` where open ground lies beside it (the land of
// `ground_tex`), so it slopes to the passage instead of standing in a cut
uniform float boulder = 0.0;
uniform float round_off = 0.0;
// how dark the tops of rock lie over its lit faces (1: as the faces):
// what stands tall, and what is cut down to a ledge at the hero's feet
// (that one shows as rock in their light, not as a hole in the floor)
uniform float top_shade = 1.0;
uniform float ledge_shade = 1.0;
// natural rock: the texture's blocks give way to a noise of the place,
// and this much of its relief to the noise's own
uniform float natural = 0.0;
uniform float natural_relief = 1.0;
// natural rock split by crevices (a cave's: dark seams wandering over its
// lumps, a fine grit in its relief, so it never reads as crumpled sheets)
uniform float crevices = 0.0;
// a faint cold light along the edges the camera sees grazing (rock)
uniform float rim = 0.0;
// the same on the tops only (a wall's rock top)
uniform float natural_top = 0.0;
// how much a surface glows with its own colour: in view, and remembered
uniform float glow = 0.0;
uniform float memory_glow = 1.0;
// the ground under the level: rock that fades into darkness away from
// what is known
uniform float void_fade = 0.0;
// it glows through its cracks too where the branch's floors do (the fire
// under Gehennom), this much of theirs
uniform float fissures = 0.0;
// what the fog of war takes from what is remembered (0: nothing)
uniform float memory = 1.0;
// the least light a surface gives out (linear): never pure black
uniform vec3 lift : source_color = vec3(0.0);

global uniform vec3 hero_pos;
global uniform vec3 eye_pos;
global uniform float branch_cracks;
global uniform vec4 branch_crack_color;
global uniform vec4 branch_chasm;
// a cave's branch (the Gnomish Mines): its floors meet the rock as a
// shore, not along the cells' edges
global uniform float branch_cave;
// how much of their own faint light remembered floors show (Gehennom's
// dimmer, its hero's pool bright against them); the overview shows them
// all alike
global uniform float branch_floor_memory;
// the whole level framed (map_view's overview): what is remembered shows
// nearly as it is, lit by its own faint light
global uniform float overview;

const vec2 FIELD = vec2(80.0, 21.0);
const vec3 MEMORY_TINT = vec3(0.54, 0.59, 0.72);
// how far toward it what is remembered turns in the overview
const float OVERVIEW_TINT = 0.15;

varying vec3 wpos;
varying vec3 wnrm;
// natural rock: how far in the face lies here (its hollows darker)
varying float hollow;

float hash3(vec3 p) {
	p = fract(p * 0.3183099 + 0.1);
	p *= 17.0;
	return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise3(vec3 x) {
	vec3 i = floor(x);
	vec3 f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(mix(hash3(i), hash3(i + vec3(1, 0, 0)), f.x),
			mix(hash3(i + vec3(0, 1, 0)), hash3(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(hash3(i + vec3(0, 0, 1)), hash3(i + vec3(1, 0, 1)), f.x),
			mix(hash3(i + vec3(0, 1, 1)), hash3(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

// turned between octaves, so value noise's lattice never shows as squares
const mat3 TURN = mat3(vec3(0.00, 0.80, 0.60), vec3(-0.80, 0.36, -0.48), vec3(-0.60, -0.48, 0.64));

float fbm3(vec3 p) {
	float v = 0.0;
	float a = 0.5;
	p = TURN * p;
	for (int i = 0; i < 3; i++) {
		v += a * noise3(p);
		p = TURN * p * 2.03 + vec3(1.7, 9.2, 4.1);
		a *= 0.5;
	}
	return v / 0.875;
}

float fbm2(vec2 p) {
	return fbm3(vec3(p, 0.37));
}

// the noise texture's tile is 8 units of the noise above
float tn2(vec2 p) {
	return texture(noise_tex, p * 0.125).r;
}

// a slanted slice of the 2D noise: it varies on walls as on floors
float tn3(vec3 p) {
	return tn2(p.xz + p.y * vec2(0.37, 0.71));
}

// how far the rock at a world point is pushed: a function of the point
// only, so the corners blocks share move together and leave no cracks;
// lumps a metre or so across (finer, the grid would show as cobbles), and
// the tops broken on a larger scale still
float tl2(vec2 p) {
	return textureLod(noise_tex, p * 0.125, 0.0).r;
}

// the kinds' strengths at a point of the ground: each cell's own blended
// into its neighbours' and broken by the noise, so where two kinds meet
// the line wanders through both cells. A kind with nothing of it near
// counts for nothing. (The same in every shader of the ground, read at
// the noise's own level: each decides alike where a shore runs.)
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
	// and wherever the cells round a point are all of one kind (between
	// two cells of a line of water, the middle of a field) that kind,
	// whatever the noise: a stream a cell wide never breaks into puddles
	vec4 whole = smoothstep(vec4(0.75), vec4(0.95), near);
	return f + texelFetch(ground_tex, at, 0) * core * 0.35 + whole;
}

vec4 ground_field(vec2 p) {
	return ground_blur(p);
}

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

vec3 displacement(vec3 w) {
	vec2 q = w.xz * 1.1 + w.y * vec2(0.37, 0.71);
	vec3 d = vec3(tl2(q), tl2(q + 17.3), tl2(q.yx + 31.1)) - 0.5;
	vec3 dw = d * displace * 3.5;
	float ridge = 1.0 - abs(tl2(w.xz * 0.8) * 2.0 - 1.0);
	dw.y += (ridge - 0.55) * displace * 9.0 * smoothstep(0.2, 0.45, w.y);
	return dw;
}

// how far natural rock at a world point is pushed: a field of the point
// alone (the faces of the blocks share their corners, and move together),
// lumps of a metre and smaller ones on them; nothing leans out at the
// foot, where somebody may stand, and a top only ever lies lower than
// the block is cut
vec3 rock_field(vec3 w) {
	vec2 q = w.xz * 0.7 + w.y * vec2(0.37, 0.71);
	vec3 big = vec3(tl2(q), tl2(q + 17.3), tl2(q.yx + 31.1)) - 0.5;
	vec2 r = w.xz * 2.1 + w.y * vec2(0.71, -0.37) + 5.1;
	vec3 lump = vec3(tl2(r), tl2(r + 11.9), tl2(r.yx + 23.7)) - 0.5;
	vec3 d = (big * 2.2 + lump * 1.0) * boulder;
	d.xz *= smoothstep(0.0, 0.35, w.y);
	// (its tops rounded down at their lumps, up a little between)
	d.y = (d.y < 0.0 ? d.y * 1.8 : d.y * 0.35) * smoothstep(0.15, 0.6, w.y);
	if (round_off > 0.0) {
		// open ground beside it: the edge over it slopes down, unevenly
		float open = textureLod(ground_tex, (w.xz + 0.5) / FIELD, 0.0).a;
		float k = open * mix(0.6, 1.4, tl2(w.xz * 1.3 + 7.1));
		d.y -= k * round_off * smoothstep(0.1, 0.5, w.y);
	}
	return d;
}

vec3 hash32(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
	q += dot(q, q.yxz + 33.33);
	return fract((q.xxy + q.yzz) * q.zyx);
}

// the column of rock a point stands in, seen from above (the nearest of
// a scattered grid's points): where that point is, and three numbers of
// the column's own in 0..1
void column(vec2 q, out vec2 at, out vec3 id) {
	vec2 i = floor(q);
	vec2 f = fract(q);
	float best = 8.0;
	at = i;
	id = vec3(0.0);
	for (int y = -1; y <= 1; y++) {
		for (int x = -1; x <= 1; x++) {
			vec2 g = vec2(float(x), float(y));
			vec3 h = hash32(i + g);
			vec2 r = g + 0.1 + 0.8 * h.xy - f;
			float d = dot(r, r);
			if (d < best) {
				best = d;
				at = i + g;
				id = vec3(h.z, hash32(i + g + 71.3).xy);
			}
		}
	}
}

// how far the rock at a world point is pushed (again a function of the
// point only: blocks share their corners). Rock breaks along its joints:
// it stands in columns about 0.6 m across, each split in layers of its
// own, and every chunk is pushed in or out a little as a whole, its faces
// left flat. Nothing leans out at the foot, where the floor's edge is and
// somebody may stand; and a top is only ever broken lower, never higher
// than the block is cut.
vec3 chunk_push(vec3 w) {
	vec2 at;
	vec3 id;
	column(w.xz * 1.6, at, id);
	float layer = floor(w.y / mix(0.4, 0.75, id.x) + id.y * 5.0);
	vec3 own = hash32(at + layer * 7.31);
	float rise = smoothstep(0.0, 0.3, w.y);
	vec3 d = vec3(own.x - 0.5, 0.0, own.y - 0.5) * 2.0 * chunks * rise;
	// and the whole face wanders in and out over a few metres, more the
	// higher it stands: no cliff runs along a ruled line
	vec2 wander = vec2(tl2(w.xz * 0.21 + 3.7), tl2(w.zx * 0.21 + 9.1)) - 0.5;
	d.xz += wander * 4.0 * chunks * smoothstep(0.3, 1.8, w.y);
	// (a ledge cut low is broken the more: it is all top)
	d.y = -id.z * chunks * mix(1.5, 1.0, smoothstep(0.5, 1.2, w.y)) * rise;
	return d;
}

void vertex() {
	vec3 w = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	vec3 n = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
	hollow = 0.0;
	if (boulder > 0.0) {
		// the normal of the pushed surface, from two neighbours pushed too
		vec3 dw = rock_field(w);
		vec3 t = normalize(abs(n.y) > 0.9 ? cross(n, vec3(1.0, 0.0, 0.0)) : cross(n, vec3(0.0, 1.0, 0.0)));
		vec3 b = cross(n, t);
		const float DS = 0.06;
		vec3 p0 = w + dw;
		vec3 p1 = w + t * DS + rock_field(w + t * DS);
		vec3 p2 = w + b * DS + rock_field(w + b * DS);
		vec3 pn = normalize(cross(p1 - p0, p2 - p0));
		hollow = clamp(-dot(dw, n) / (boulder * 1.6), 0.0, 1.0);
		n = dot(pn, n) < 0.0 ? -pn : pn;
		w = p0;
		VERTEX += transpose(mat3(MODEL_MATRIX)) * dw;
	} else if (chunks > 0.0) {
		// its faces are lit as they are cut (the fragments' facets)
		vec3 dw = chunk_push(w);
		w += dw;
		VERTEX += transpose(mat3(MODEL_MATRIX)) * dw;
	} else if (displace > 0.0) {
		vec3 dw = displacement(w);
		// the normal of the pushed surface, from two neighbours pushed too
		vec3 t = normalize(abs(n.y) > 0.9 ? cross(n, vec3(1.0, 0.0, 0.0)) : cross(n, vec3(0.0, 1.0, 0.0)));
		vec3 b = cross(n, t);
		const float DS = 0.05;
		vec3 p0 = w + dw;
		vec3 p1 = w + t * DS + displacement(w + t * DS);
		vec3 p2 = w + b * DS + displacement(w + b * DS);
		vec3 pn = normalize(cross(p1 - p0, p2 - p0));
		n = dot(pn, n) < 0.0 ? -pn : pn;
		w = p0;
		VERTEX += transpose(mat3(MODEL_MATRIX)) * dw;
	}
	wpos = w;
	wnrm = n;
}

// a tangent-space normal onto a projection (whiteout blend)
vec3 bent(vec3 tn, vec2 base, float z) {
	return vec3(tn.xy * normal_strength + base, abs(tn.z) * z);
}

void fragment() {
	// the ground shows where its kind wins; how near it comes to losing
	// to water, lava or ice beside it (for the shore's own look)
	float near_water = 0.0;
	float near_lava = 0.0;
	float near_ice = 0.0;
	vec4 field = edge_kind > 0.0 ? ground_field(wpos.xz) : vec4(0.0);
	// (nothing else near: all its own)
	float rivals = edge_kind == 4.0 ? max(max(field.r, field.g), field.a) : max(max(field.r, field.g), field.b);
	if (rivals > 0.0) {
		vec4 v = ground_lead(wpos.xz, field);
		float k = ground_kind(v);
		if (k > 0.0 && k != edge_kind) {
			discard;
		}
		float own = edge_kind == 4.0 ? v.b : v.a;
		near_water = (1.0 - smoothstep(0.0, 0.28, own - v.r)) * step(0.001, v.r);
		near_lava = (1.0 - smoothstep(0.0, 0.35, own - v.g)) * step(0.001, v.g);
		near_ice = edge_kind == 1.0 ? (1.0 - smoothstep(0.0, 0.2, own - v.b)) * step(0.001, v.b) : 0.0;
	}
	// a floor's edge where nothing is drawn beside it (the dark past what
	// is seen, a floor NetHack leaves unmapped out of sight): frayed by the
	// noise as a shore is, never a square step
	if (edge_kind == 1.0) {
		float empty = texture(stand_tex, (wpos.xz + 0.5) / FIELD).g;
		if (empty > 0.001 && empty + (tl2(wpos.xz * 1.7 + vec2(23.1, 9.4)) - 0.5) * 0.6 > 0.5) {
			discard;
		}
	}
	vec3 n = normalize(wnrm);
	if (facets > 0.0) {
		vec3 f = normalize(cross(dFdx(wpos), dFdy(wpos)));
		n = dot(f, CAMERA_POSITION_WORLD - wpos) < 0.0 ? -f : f;
	}
	// how near the see-through's rim this stone is (0 far, 1 at its edge)
	float rim_near = 0.0;
	if (occlude > 0.0 && wpos.y < solid_top) {
		vec3 ch = hero_pos - eye_pos;
		float len2 = max(dot(ch, ch), 1e-4);
		float t = dot(wpos - eye_pos, ch) / len2;
		// closer to the eye than the hero, near the line to them; never
		// what stands right over their head (a doorway's lintel, a stair's
		// arch: it hides nothing of them)
		float before = 1.0 - 0.6 / sqrt(len2);
		float aside = length(wpos.xz - hero_pos.xz);
		bool overhead = wpos.y > hero_pos.y + 0.7 && aside < 0.7;
		if (t > 0.0 && t < before) {
			float d = length(wpos - (eye_pos + ch * t));
			// a cone from the eye widening to the hero's own body at the
			// hero (a lintel over their head stays): open inside, its rim
			// wandering gently in the stone's own noise (fixed in the world:
			// it never crawls nor shows a pixel grid), the stone darkening
			// towards it, never a torn pixel edge
			float r = 0.2 + 0.42 * t;
			float wander = (tn3(wpos * 3.0) - 0.5) * 0.3;
			if (!overhead && smoothstep(r * 0.75, r * 1.2, d) + wander < 0.4) {
				discard;
			}
			// (and fading out where the hole ends: before the hero, and
			// round what stays over their head)
			float lintel = (1.0 - smoothstep(0.55, 0.85, aside))
				* smoothstep(0.6, 0.8, wpos.y - hero_pos.y);
			rim_near = (1.0 - smoothstep(0.4, 1.0, smoothstep(r * 0.75, r * 1.5, d) + wander))
				* smoothstep(0.0, 0.3, (before - t) * sqrt(len2)) * (1.0 - lintel);
		}
	}
	vec3 bw = pow(abs(n), vec3(4.0));
	bw /= bw.x + bw.y + bw.z;
	vec3 albedo = vec3(0.0);
	vec3 orm = vec3(0.0);
	vec3 wn = vec3(0.0);
	vec2 dscale = vec2(detail_scale);
	if (bw.y > 0.01) {
		vec2 uv = wpos.xz * top_scale;
		vec3 tn = texture(top_normal, uv).xyz * 2.0 - 1.0;
		tn.xy += (texture(detail_normal, uv * dscale).xy * 2.0 - 1.0) * detail_strength;
		albedo += texture(top_albedo, uv).rgb * top_color.rgb * bw.y;
		orm += texture(top_orm, uv).rgb * bw.y;
		wn += bent(tn, n.xz, n.y).xzy * bw.y;
	}
	if (bw.x > 0.01) {
		// upright: the image's v runs down the wall
		vec2 uv = vec2(wpos.z, -wpos.y) * side_scale;
		vec3 tn = texture(side_normal, uv).xyz * 2.0 - 1.0;
		tn.xy += (texture(detail_normal, uv * dscale).xy * 2.0 - 1.0) * detail_strength;
		tn.y = -tn.y;
		albedo += texture(side_albedo, uv).rgb * side_color.rgb * bw.x;
		orm += texture(side_orm, uv).rgb * bw.x;
		wn += bent(tn, n.zy, n.x).zyx * bw.x;
	}
	if (bw.z > 0.01) {
		// upright: the image's v runs down the wall
		vec2 uv = vec2(wpos.x, -wpos.y) * side_scale;
		vec3 tn = texture(side_normal, uv).xyz * 2.0 - 1.0;
		tn.xy += (texture(detail_normal, uv * dscale).xy * 2.0 - 1.0) * detail_strength;
		tn.y = -tn.y;
		albedo += texture(side_albedo, uv).rgb * side_color.rgb * bw.z;
		orm += texture(side_orm, uv).rgb * bw.z;
		wn += bent(tn, n.xy, n.z) * bw.z;
	}
	wn = normalize(wn);
	// the top of a block of rock is all of its top face, however its
	// chunks are tilted
	// (and of every ledge on its faces: they lie in their own shadow)
	float up = chunks > 0.0 ? max(smoothstep(0.3, 0.7, normalize(wnrm).y), smoothstep(0.35, 0.8, n.y)) : bw.y;
	float nat = max(natural, natural_top * smoothstep(0.5, 0.9, up));
	if (nat > 0.0) {
		// mottled at two scales, never a pattern that could be paving
		float grain = tn3(wpos * 1.1) * 0.6 + tn3(wpos * 4.7) * 0.4;
		vec3 base = mix(side_color.rgb, top_color.rgb, bw.y);
		// strata across the faces, pits and cracks in them
		float strata = 0.5 + 0.5 * sin(wpos.y * 11.0 + tn3(wpos * 0.9) * 7.0);
		float cn = tn3(wpos * 2.2 + 3.1);
		float crack = smoothstep(0.46, 0.5, cn) - smoothstep(0.5, 0.54, cn);
		vec3 rock = base * mix(0.6, 1.3, grain) * mix(1.0, mix(0.9, 1.05, strata), 1.0 - bw.y);
		// (fainter on rock's lit faces: dark veins there are marble's; on
		// a cave's lumps all but none)
		rock *= 1.0 - (boulder > 0.0 ? 0.1 : chunks > 0.0 ? 0.3 : 0.45) * crack;
		float seam = 0.0;
		if (crevices > 0.0) {
			float cv = tn3(wpos * 1.6 + 7.3);
			seam = smoothstep(0.47, 0.5, cv) - smoothstep(0.5, 0.53, cv);
			// deep in some stretches, closed in others
			seam *= smoothstep(0.35, 0.6, tn3(wpos * 0.7 + 1.9)) * crevices;
			rock *= 1.0 - 0.75 * seam;
		}
		albedo = mix(albedo, rock, nat);
		// its relief from the noise's slope, not from the mesh's grid
		const float STEP = 0.04;
		vec3 q = wpos * 3.3;
		float h0 = tn3(q);
		vec3 grad = vec3(tn3(q + vec3(STEP, 0, 0)), tn3(q + vec3(0, STEP, 0)), tn3(q + vec3(0, 0, STEP))) - h0;
		grad /= STEP;
		vec3 bump = normalize(n - (grad - dot(grad, n) * n) * 0.16);
		if (crevices > 0.0) {
			// a fine grit over the lumps
			vec3 q2 = wpos * 11.0;
			float g0 = tn3(q2);
			vec3 g2 = (vec3(tn3(q2 + vec3(STEP, 0, 0)), tn3(q2 + vec3(0, STEP, 0)),
				tn3(q2 + vec3(0, 0, STEP))) - g0) / STEP;
			bump = normalize(bump - (g2 - dot(g2, n) * n) * 0.035 * crevices);
		}
		wn = normalize(mix(wn, bump, nat * natural_relief));
		// nor the texture's cavities and gloss: the noise's own
		orm = mix(orm, vec3(mix(0.55, 1.0, smoothstep(0.25, 0.6, h0)), mix(0.55, 0.9, grain), 0.0), nat * natural_relief);
		// the crevices in their own shadow
		orm.r *= 1.0 - 0.6 * seam * nat;
	}
	albedo *= mix(1.0, mix(ledge_shade, top_shade, smoothstep(0.5, 1.2, wpos.y)), up);
	// natural rock's hollows lie in their own shadow
	albedo *= mix(1.0, 0.45, hollow);
	orm.r *= mix(1.0, 0.55, hollow);

	// slow variation over the level, a little warmer or cooler by place
	float m = tn2(wpos.xz * 0.12 + wpos.y * 0.05);
	albedo *= mix(1.0, mix(0.82, 1.06, m), macro);
	float hue = tn2(wpos.xz * 0.07 + 5.3) - 0.5;
	albedo *= 1.0 + hue * macro * vec3(0.07, 0.0, -0.07);
	float rough = clamp(orm.g + roughness_bias, 0.0, 1.0) * roughness;

	vec2 cell = wpos.xz + 0.5;
	vec4 fow = texture(fow_tex, (cell - n.xz * 0.3) / FIELD);
	if (grime > 0.0) {
		float g = smoothstep(0.12, 0.5, texture(fow_tex, cell / FIELD).b) * grime;
		albedo *= 1.0 - 0.32 * g;
		rough = min(rough + 0.1 * g, 1.0);
	}
	vec3 lava = vec3(0.0);
	if ((grime > 0.0 || fissures > 0.0) && branch_cracks > 0.0) {
		// fire under the floor shows through its cracks: thin, in patches
		// with whole stretches of stone unbroken between them, and never
		// under nor right round whatever stands or lies on a cell (a
		// creature on the fire was lost in it)
		float c = tn2(wpos.xz * 0.6 + 3.7);
		float crack = smoothstep(0.4925, 0.5, c) - smoothstep(0.5, 0.5075, c);
		crack *= smoothstep(0.44, 0.6, tn2(wpos.xz * 0.09 + 21.0));
		crack *= 1.0 - smoothstep(0.3, 0.6, texture(stand_tex, cell / FIELD).r);
		float pulse = 0.75 + 0.25 * sin(TIME * 1.3 + tn2(wpos.xz * 0.2) * 9.0);
		lava = branch_crack_color.rgb * crack * pulse * branch_cracks * 0.65;
		albedo *= 1.0 - crack * 0.8;
		// and a faint glow at the walls' very feet, a thin line (never a
		// band a player might take for lava's edge): the mazes read by it
		if (grime > 0.0) {
			float foot = smoothstep(0.3, 0.55, texture(fow_tex, cell / FIELD).b);
			lava += branch_crack_color.rgb * foot * pulse * branch_cracks * 0.04;
		}
	}
	if (wet > 0.0) {
		float w = smoothstep(0.66, 0.74, tn2(wpos.xz * 0.3 + 11.0)) * wet * bw.y;
		albedo *= mix(1.0, 0.75, w);
		rough = mix(rough, 0.35, w);
	}
	// along a shore: soaked dark and glossy by the water, frosted white by
	// ice, scorched black by lava (whose own edge glows); not so glossy the
	// hero's light whitens it (a pale rim round each pool, as foam)
	if (near_water > 0.0) {
		albedo *= mix(1.0, 0.5, near_water);
		rough = mix(rough, 0.3, near_water);
	}
	if (near_ice > 0.0) {
		float luma = dot(albedo, vec3(0.299, 0.587, 0.114));
		vec3 frost = vec3(0.78, 0.84, 0.9) * mix(0.55, 0.95, tn2(wpos.xz * 3.1));
		albedo = mix(albedo, frost * max(luma * 2.2, 0.35), near_ice * 0.5);
		rough = mix(rough, 0.3, near_ice);
	}
	if (near_lava > 0.0) {
		albedo *= mix(1.0, 0.25, near_lava);
	}
	// in a cave the floor meets the rock as it meets water: its edge
	// wandering through the cell, the earth darkening into the rock's
	// foot (lit and square, a patch of it read as a tile laid down)
	if (edge_kind == 1.0 && branch_cave > 0.0) {
		vec4 g = texture(ground_tex, (wpos.xz + 0.5) / FIELD);
		float rockward = 1.0 - min(g.r + g.g + g.b + g.a, 1.0);
		float lead = rockward + (tl2(wpos.xz * 1.7 + vec2(29.3, 3.7)) - 0.5) * 0.5
			+ (tl2(wpos.xz * 0.42 + vec2(5.1, 21.7)) - 0.5) * 0.3;
		albedo *= mix(1.0, 0.3, smoothstep(0.12, 0.42, lead) * branch_cave);
	}

	// remembered: the same stone, grey, cold and dark; in the overview
	// nearly as it is, its own hue kept (cold over it all, every level
	// read as one grey clay)
	float seen = mix(1.0, fow.r, memory);
	// in a cave what the hero sees of the floor meets what they remember
	// as a shore does, wandering through the cells (the few cells a dark
	// cave shows round the hero were a square patch of earth)
	if (edge_kind == 1.0 && branch_cave > 0.0 && seen > 0.0 && seen < 1.0) {
		float s = seen + (tl2(wpos.xz * 1.7 + vec2(41.3, 17.9)) - 0.5) * 0.5
			+ (tl2(wpos.xz * 0.42 + vec2(9.7, 33.1)) - 0.5) * 0.3;
		seen = mix(seen, smoothstep(0.3, 0.7, s), branch_cave);
	}
	float luma = dot(albedo, vec3(0.299, 0.587, 0.114));
	vec3 remembered = mix(albedo, luma * MEMORY_TINT, mix(0.8, OVERVIEW_TINT, overview))
		* mix(0.4, 0.75, overview);
	// never so dark the darkest stones drop to black (Gehennom's basalt),
	// the less so in the overview: lifted in value, its hue kept; there
	// rock stays in its dark (lifted, the rock between corridors read as
	// floor)
	bool rock = chunks > 0.0 || boulder > 0.0;
	float least = mix(0.012, rock ? 0.0 : 0.035, overview);
	float rl = dot(remembered, vec3(0.299, 0.587, 0.114));
	remembered *= clamp(least / max(rl, 1e-4), 1.0, 12.0);
	vec3 color = mix(remembered, albedo, seen);
	// what is remembered shows by its own faint light: shaded by its
	// relief, as if from above and to one side, so a remembered wall is
	// the same stone, not a flat box
	float relief = 0.45 + 0.8 * clamp(dot(wn, normalize(vec3(-0.35, 0.75, 0.55))), 0.0, 1.0);
	relief *= mix(1.0, orm.r, 0.7);
	float own_light = memory_glow * (grime > 0.0 ? mix(branch_floor_memory, 1.0, overview) : 1.0);
	vec3 emission = color * mix(own_light * relief * (1.0 + overview), glow, seen) + lift;
	// and what is in view at least as plain as what is remembered, far
	// from any torch, in its own colour (less, a town's street in view
	// read as a burnt smear among the houses remembered); the ground
	// under the level stays in its dark
	emission += albedo * relief * seen * overview * memory * 1.5 * own_light;
	float near = 1.0;
	if (void_fade > 0.0) {
		near = mix(1.0, fow.a, void_fade);
		color *= near;
		emission *= near * near;
		// over a chasm (Gehennom) what lies far from anything known is
		// its smouldering depths, never black: smoke drifting over a red
		// glow, the same everywhere (it shows nothing of the map); near
		// what is known it is the floor a maze's dark remembers as nothing
		// (NetHack maps no unlit floor out of sight), shown by its own
		// faint light as a remembered floor is (alpha: how much)
		if (branch_chasm.a > 0.0) {
			emission += albedo * near * branch_chasm.a;
			float smoke = tn2(wpos.xz * 0.05 + TIME * vec2(0.01, 0.004));
			float deep = smoothstep(0.35, 0.75, tn2(wpos.xz * 0.11 + 7.0 - TIME * 0.006));
			emission += branch_chasm.rgb * mix(0.45, 1.0, deep) * mix(0.6, 1.0, smoke)
				* (1.0 - near);
		}
	}

	// the stone dark towards the see-through's rim
	color *= 1.0 - 0.6 * rim_near;
	emission *= 1.0 - 0.6 * rim_near;
	ALBEDO = color;
	ROUGHNESS = rough;
	// rock's tops and ledges mirror nothing at all
	SPECULAR = (chunks > 0.0 ? specular * (1.0 - up) : specular) * (1.0 - rim_near);
	METALLIC = metallic * orm.b;
	// what metal mirrors (map_view's metal sky) falls on no stone, wet
	// floor or ice (the dark is tuned without it): only on the metal the
	// hero sees (a sink's rim, gilded capitals), never on what is
	// remembered
	RADIANCE = vec4(0.0, 0.0, 0.0, 1.0 - METALLIC * seen);
	AO = orm.r;
	AO_LIGHT_AFFECT = 0.3;
	if (rim > 0.0) {
		float graze = 1.0 - clamp(dot(normalize(wn), normalize(CAMERA_POSITION_WORLD - wpos)), 0.0, 1.0);
		emission += vec3(0.35, 0.42, 0.6) * pow(graze, 3.0) * rim;
	}
	// the ground's fissures fainter than a floor's cracks, and fading
	// away from what is known
	EMISSION = emission + lava * (fissures > 0.0 ? fissures * near * near : 1.0);
	NORMAL = normalize((VIEW_MATRIX * vec4(wn, 0.0)).xyz);
}
