// The map's stone, earth and wood (surface.rs): world-space triplanar with
// a top texture of its own (a wall's masonry sides under a rock top), slow
// variation over the level instead of a brightness per cell, wet patches
// on floors, a fine detail normal, grime at the foot of walls, and the
// fog of war: what the hero sees now in full colour, what they remember
// darker, cold and grey, what they never saw fading into darkness.
// Walls and rock between the eye and the hero dither away around them.
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
// dithers away between the eye and the hero, all but its top line
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
// how dark the tops of rock lie over its lit faces (1: as the faces):
// what stands tall, and what is cut down to a ledge at the hero's feet
// (that one shows as rock in their light, not as a hole in the floor)
uniform float top_shade = 1.0;
uniform float ledge_shade = 1.0;
// natural rock: the texture's blocks give way to a noise of the place,
// and this much of its relief to the noise's own
uniform float natural = 0.0;
uniform float natural_relief = 1.0;
// a faint cold light along the edges the camera sees grazing (rock)
uniform float rim = 0.0;
// the same on the tops only (a wall's rock top)
uniform float natural_top = 0.0;
// how much a surface glows with its own colour: in view, and remembered
uniform float glow = 0.0;
uniform float memory_glow = 0.06;
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

const vec2 FIELD = vec2(80.0, 21.0);
const vec3 MEMORY_TINT = vec3(0.54, 0.59, 0.72);

varying vec3 wpos;
varying vec3 wnrm;

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

vec3 displacement(vec3 w) {
	vec2 q = w.xz * 1.1 + w.y * vec2(0.37, 0.71);
	vec3 d = vec3(tl2(q), tl2(q + 17.3), tl2(q.yx + 31.1)) - 0.5;
	vec3 dw = d * displace * 3.5;
	float ridge = 1.0 - abs(tl2(w.xz * 0.8) * 2.0 - 1.0);
	dw.y += (ridge - 0.55) * displace * 9.0 * smoothstep(0.2, 0.45, w.y);
	return dw;
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
	if (chunks > 0.0) {
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

// interleaved gradient noise: a fine dither with no visible grid, moved
// a little each frame so it never settles into a pattern
float dither(vec2 p, float t) {
	p += vec2(5.588238, 5.588238) * mod(t * 60.0, 64.0);
	return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715))));
}

void fragment() {
	vec3 n = normalize(wnrm);
	if (facets > 0.0) {
		vec3 f = normalize(cross(dFdx(wpos), dFdy(wpos)));
		n = dot(f, CAMERA_POSITION_WORLD - wpos) < 0.0 ? -f : f;
	}
	if (occlude > 0.0 && wpos.y < solid_top) {
		vec3 ch = hero_pos - eye_pos;
		float len2 = max(dot(ch, ch), 1e-4);
		float t = dot(wpos - eye_pos, ch) / len2;
		// closer to the eye than the hero, near the line to them
		float before = 1.0 - 0.6 / sqrt(len2);
		if (t > 0.0 && t < before) {
			float d = length(wpos - (eye_pos + ch * t));
			// a cone from the eye widening to the hero's own size at the hero
			float r = 0.25 + 0.5 * t;
			float cover = mix(0.2, 1.0, smoothstep(r * 0.7, r * 1.3, d));
			if (dither(FRAGCOORD.xy, TIME) >= cover) {
				discard;
			}
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
		// (fainter on rock's lit faces: dark veins there are marble's)
		rock *= 1.0 - (chunks > 0.0 ? 0.3 : 0.45) * crack;
		albedo = mix(albedo, rock, nat);
		// its relief from the noise's slope, not from the mesh's grid
		const float STEP = 0.04;
		vec3 q = wpos * 3.3;
		float h0 = tn3(q);
		vec3 grad = vec3(tn3(q + vec3(STEP, 0, 0)), tn3(q + vec3(0, STEP, 0)), tn3(q + vec3(0, 0, STEP))) - h0;
		grad /= STEP;
		vec3 bump = normalize(n - (grad - dot(grad, n) * n) * 0.16);
		wn = normalize(mix(wn, bump, nat * natural_relief));
		// nor the texture's cavities and gloss: the noise's own
		orm = mix(orm, vec3(mix(0.55, 1.0, smoothstep(0.25, 0.6, h0)), mix(0.55, 0.9, grain), 0.0), nat * natural_relief);
	}
	albedo *= mix(1.0, mix(ledge_shade, top_shade, smoothstep(0.5, 1.2, wpos.y)), up);

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
		// fire under the floor shows through its cracks
		float c = tn2(wpos.xz * 0.6 + 3.7);
		float crack = smoothstep(0.487, 0.5, c) - smoothstep(0.5, 0.513, c);
		float pulse = 0.75 + 0.25 * sin(TIME * 1.3 + tn2(wpos.xz * 0.2) * 9.0);
		lava = vec3(1.0, 0.28, 0.05) * crack * pulse * branch_cracks * 1.2;
		albedo *= 1.0 - crack * 0.8;
	}
	if (wet > 0.0) {
		float w = smoothstep(0.66, 0.74, tn2(wpos.xz * 0.3 + 11.0)) * wet * bw.y;
		albedo *= mix(1.0, 0.75, w);
		rough = mix(rough, 0.35, w);
	}

	// remembered: the same stone, grey, cold and dark
	float seen = mix(1.0, fow.r, memory);
	float luma = dot(albedo, vec3(0.299, 0.587, 0.114));
	vec3 remembered = mix(albedo, luma * MEMORY_TINT, 0.8) * 0.4;
	vec3 color = mix(remembered, albedo, seen);
	// what is remembered shows by its own faint light: shaded by its
	// relief, as if from above and to one side, so a remembered wall is
	// the same stone, not a flat box
	float relief = 0.45 + 0.8 * clamp(dot(wn, normalize(vec3(-0.35, 0.75, 0.55))), 0.0, 1.0);
	relief *= mix(1.0, orm.r, 0.7);
	vec3 emission = color * mix(memory_glow * relief, glow, seen) + lift;
	float near = 1.0;
	if (void_fade > 0.0) {
		near = mix(1.0, fow.a, void_fade);
		color *= near;
		emission *= near * near;
	}

	ALBEDO = color;
	ROUGHNESS = rough;
	// rock's tops and ledges mirror nothing at all
	SPECULAR = chunks > 0.0 ? specular * (1.0 - up) : specular;
	METALLIC = metallic * orm.b;
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
