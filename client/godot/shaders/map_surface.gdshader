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
// albedo varies over the level by this much
uniform float macro = 1.0;
// wet patches (floors): darker and glossy
uniform float wet = 0.0;
// darker at the foot of walls (floors)
uniform float grime = 0.0;
// dithers away between the eye and the hero, all but its top line
uniform float occlude = 0.0;
uniform float solid_top = 2.0;
// rock: broken up by a noise of the world position (metres), and cut in
// facets
uniform float displace = 0.0;
uniform float facets = 0.0;
// natural rock: the texture's blocks give way to a noise of the place,
// its relief to the facets
uniform float natural = 0.0;
// the same on the tops only (a wall's rock top)
uniform float natural_top = 0.0;
// how much a surface glows with its own colour: in view, and remembered
uniform float glow = 0.0;
uniform float memory_glow = 0.06;
// the ground under the level: rock that fades into darkness away from
// what is known
uniform float void_fade = 0.0;
// what the fog of war takes from what is remembered (0: nothing)
uniform float memory = 1.0;
// the least light a surface gives out (linear): never pure black
uniform vec3 lift : source_color = vec3(0.0);

global uniform vec3 hero_pos;
global uniform vec3 eye_pos;

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

void vertex() {
	vec3 w = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	if (displace > 0.0) {
		// a function of where the point is only: the corners blocks share
		// move together, no cracks
		// lumps a metre or so across: finer, the grid would show as cobbles
		vec3 d = vec3(noise3(w * 1.1), noise3(w * 1.1 + 17.3), noise3(w * 1.1 + 31.1)) - 0.5;
		vec3 dw = d * displace * 3.5;
		// the tops are uneven on a larger scale too
		dw.y += (fbm2(w.xz * 0.8) - 0.5) * displace * 7.0 * smoothstep(0.2, 0.45, w.y);
		w += dw;
		VERTEX += transpose(mat3(MODEL_MATRIX)) * dw;
	}
	wpos = w;
	wnrm = normalize((MODEL_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

// a tangent-space normal onto a projection (whiteout blend)
vec3 bent(vec3 tn, vec2 base, float z) {
	return vec3(tn.xy * normal_strength + base, abs(tn.z) * z);
}

const float BAYER[16] = { 0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0,
	3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0 };

float bayer4(vec2 p) {
	ivec2 q = ivec2(mod(p, 4.0));
	return (BAYER[q.x + q.y * 4] + 0.5) / 16.0;
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
			if (bayer4(FRAGCOORD.xy) >= cover) {
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
	float nat = max(natural, natural_top * smoothstep(0.5, 0.9, bw.y));
	if (nat > 0.0) {
		// mottled at two scales, never a pattern that could be paving
		float grain = fbm3(wpos * 1.1) * 0.6 + fbm3(wpos * 4.7) * 0.4;
		vec3 base = mix(side_color.rgb, top_color.rgb, bw.y);
		// strata across the faces, pits and cracks in them
		float strata = 0.5 + 0.5 * sin(wpos.y * 11.0 + fbm3(wpos * 0.9) * 7.0);
		float crack = smoothstep(0.46, 0.5, fbm3(wpos * 2.2 + 3.1)) - smoothstep(0.5, 0.54, fbm3(wpos * 2.2 + 3.1));
		vec3 rock = base * mix(0.55, 1.35, grain) * mix(1.0, mix(0.8, 1.1, strata), 1.0 - bw.y);
		rock *= 1.0 - 0.45 * crack;
		albedo = mix(albedo, rock, nat);
		// its relief from the noise's slope, not from the mesh's grid
		const float STEP = 0.04;
		vec3 q = wpos * 3.3;
		float h0 = fbm3(q);
		vec3 grad = vec3(fbm3(q + vec3(STEP, 0, 0)), fbm3(q + vec3(0, STEP, 0)), fbm3(q + vec3(0, 0, STEP))) - h0;
		grad /= STEP;
		vec3 bump = normalize(n - (grad - dot(grad, n) * n) * 0.22);
		wn = normalize(mix(wn, bump, nat));
		// nor the texture's cavities and gloss: the noise's own
		orm = mix(orm, vec3(mix(0.55, 1.0, smoothstep(0.25, 0.6, h0)), mix(0.55, 0.9, grain), 0.0), nat);
	}

	// slow variation over the level, a little warmer or cooler by place
	float m = fbm2(wpos.xz * 0.12 + wpos.y * 0.05);
	albedo *= mix(1.0, mix(0.82, 1.06, m), macro);
	float hue = fbm2(wpos.xz * 0.07 + 5.3) - 0.5;
	albedo *= 1.0 + hue * macro * vec3(0.07, 0.0, -0.07);
	float rough = clamp(orm.g + roughness_bias, 0.0, 1.0) * roughness;

	vec2 cell = wpos.xz + 0.5;
	vec4 fow = texture(fow_tex, (cell - n.xz * 0.3) / FIELD);
	if (grime > 0.0) {
		float g = smoothstep(0.12, 0.5, texture(fow_tex, cell / FIELD).b) * grime;
		albedo *= 1.0 - 0.32 * g;
		rough = min(rough + 0.1 * g, 1.0);
	}
	if (wet > 0.0) {
		float w = smoothstep(0.66, 0.74, fbm2(wpos.xz * 0.3 + 11.0)) * wet * bw.y;
		albedo *= mix(1.0, 0.75, w);
		rough = mix(rough, 0.35, w);
	}

	// remembered: the same stone, grey, cold and dark
	float seen = mix(1.0, fow.r, memory);
	float luma = dot(albedo, vec3(0.299, 0.587, 0.114));
	vec3 remembered = mix(albedo, luma * MEMORY_TINT, 0.8) * 0.4;
	vec3 color = mix(remembered, albedo, seen);
	vec3 emission = color * mix(memory_glow, glow, seen) + lift;
	if (void_fade > 0.0) {
		float near = mix(1.0, fow.a, void_fade);
		color *= near;
		emission *= near * near;
	}

	ALBEDO = color;
	ROUGHNESS = rough;
	METALLIC = metallic * orm.b;
	AO = orm.r;
	AO_LIGHT_AFFECT = 0.3;
	EMISSION = emission;
	NORMAL = normalize((VIEW_MATRIX * vec4(wn, 0.0)).xyz);
}
