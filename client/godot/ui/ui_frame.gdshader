// The forged frame of every HUD panel and dialog (ui-design §6.4), lit
// from the top left: a dark outer line; a rim of `border_color` metal
// `border_px` wide, with `carved` rounded (its outer half facing out, its
// inner half in, so the top and left rims are bright outside and the
// bottom and right ones inside; the other way round when `sunk`, as a
// socket's lip); a groove; a thin gilt trim inset from the rim; the ground
// a vertical gradient under an inner shadow, with `leather` its grain
// (pebbled, creased, mottled) and a warm sheen near the top; chamfered
// corners, and with `cap_px` a cast cap over each corner (a quarter disc,
// a ring cut in it, a rivet). Drawn on the panel's StyleBoxFlat, whose
// expand margin leaves room for the drop shadow outside the rect.
// theme.rs keeps `size` current and has the StyleBoxFlat look as the
// fallback.
shader_type canvas_item;
render_mode unshaded;

uniform vec2 size = vec2(100.0, 100.0);
uniform vec4 top_color : source_color = vec4(0.106, 0.078, 0.059, 0.94);
uniform vec4 bottom_color : source_color = vec4(0.055, 0.043, 0.035, 0.94);
uniform vec4 outer_color : source_color = vec4(0.027, 0.02, 0.016, 1.0);
uniform vec4 border_color : source_color = vec4(0.227, 0.184, 0.145, 1.0);
uniform float border_px = 2.0;
uniform vec4 bevel_light : source_color = vec4(0.42, 0.337, 0.22, 0.9);
uniform vec4 bevel_dark : source_color = vec4(0.0, 0.0, 0.0, 0.6);
uniform float inner_shadow_px = 12.0;
uniform float grain_amount = 0.03;
uniform float corner_cut_px = 5.0;
uniform vec4 trim_color : source_color = vec4(0.431, 0.325, 0.157, 0.8);
uniform float trim_px = 1.0;
uniform float trim_inset = 5.0;
uniform float shadow_px = 10.0;
uniform float shadow_alpha = 0.55;
// the rim drawn rounded and lit (else flat, as `border_color`)
uniform bool carved = false;
// a socket: its rim lit from below, the ground sunk under it
uniform bool sunk = false;
// the corner caps' radius (0: none)
uniform float cap_px = 0.0;
uniform vec4 cap_color : source_color = vec4(0.55, 0.41, 0.2, 1.0);
// the leather's grain, 0..1
uniform float leather = 0.0;
// the rim widens by `bracket_px` into a bracket at each corner, tapering
// back over `bracket_len` along the edges
uniform float bracket_px = 0.0;
uniform float bracket_len = 48.0;

varying vec2 local;

void vertex() {
	local = VERTEX;
}

// Signed distance to a box of half size `h` with corners cut by `cut`
// (negative inside).
float chamfered(vec2 p, vec2 h, float cut) {
	vec2 q = abs(p) - h;
	float box = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
	float corner = (abs(p.x) + abs(p.y) - (h.x + h.y - cut)) * 0.70710678;
	return max(box, corner);
}

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

vec2 hash2(vec2 p) {
	return fract(sin(vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

// The pebbled grain: how far from a crease between cells `p` is (0 on
// one), and the pebble's own shade.
vec2 pebbles(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	float f1 = 8.0;
	float f2 = 8.0;
	float id = 0.0;
	for (int y = -1; y <= 1; y++) {
		for (int x = -1; x <= 1; x++) {
			vec2 g = vec2(float(x), float(y));
			vec2 o = hash2(i + g);
			float d = length(g + o - f);
			if (d < f1) {
				f2 = f1;
				f1 = d;
				id = hash(i + g + 7.3);
			} else if (d < f2) {
				f2 = d;
			}
		}
	}
	return vec2(smoothstep(0.0, 0.18, f2 - f1), id);
}

// The leather's tone at `x` (pixels): 1 is the gradient's own colour.
float grain(vec2 x) {
	vec2 pb = pebbles(x / 3.6);
	float k = 1.0 - 0.3 * (1.0 - pb.x) + (pb.y - 0.5) * 0.1;
	// creases running across, and broad mottling
	float crease = noise(vec2(x.x / 70.0, x.y / 7.0));
	k *= 1.0 - 0.09 * smoothstep(0.6, 0.85, crease);
	float m = noise(x / 90.0) * 0.6 + noise(x / 37.0) * 0.4;
	k *= 0.8 + 0.4 * m;
	return k;
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = local - size * 0.5;
	float d = chamfered(p, size * 0.5, corner_cut_px);
	if (d > 0.0) {
		float s = 1.0 - smoothstep(0.0, max(shadow_px, 0.001), d);
		COLOR = vec4(0.0, 0.0, 0.0, shadow_alpha * s * s) * tint;
	} else {
		float depth = -d;
		float t = clamp(local.y / max(size.y, 1.0), 0.0, 1.0);
		vec4 base = mix(top_color, bottom_color, t);
		vec3 col = base.rgb;
		col += (hash(floor(local)) - 0.5) * grain_amount;
		if (leather > 0.0) {
			col *= mix(1.0, grain(local), leather);
		}
		// a faint warm sheen near the top, like light on leather
		col += vec3(0.02, 0.014, 0.008) * (1.0 - smoothstep(0.0, 0.35, t)) * (1.0 + leather);
		// which side is nearest decides the light: the top and left face
		// it, the bottom and right away (the other way round when sunk)
		float left = local.x, top = local.y;
		float right = size.x - local.x, bottom = size.y - local.y;
		bool lit = min(left, top) <= min(right, bottom);
		float side = (lit != sunk) ? 1.0 : -1.0;
		// the rim's width here: wider in the corners' brackets
		vec2 c = vec2(min(left, right), min(top, bottom));
		float along = c.y < c.x ? c.x : c.y;
		float rim = border_px + bracket_px * (1.0 - smoothstep(bracket_len * 0.5, bracket_len, along));
		float shade = smoothstep(0.0, max(inner_shadow_px, 0.001), depth - rim);
		// a carved rim casts its shadow from the lit side, less from the other
		float cast = (carved && side < 0.0) ? 0.45 : 0.0;
		col *= mix(sunk ? 0.4 : 0.55, 1.0, mix(shade, 1.0, cast));
		float a = base.a;
		float band = depth - 1.0 - rim;
		if (depth < 1.0) {
			col = outer_color.rgb;
			a = outer_color.a;
		} else if (depth < 1.0 + rim) {
			if (carved) {
				// across the rim: its outer half faces out, its inner in
				float u = (depth - 1.0) / max(rim, 0.001);
				float l = 0.5 + 0.5 * side * (1.0 - 2.0 * u);
				float crest = 1.0 - clamp(abs(u - 0.35) / 0.5, 0.0, 1.0);
				// old bronze: dull, its crest only a little brighter
				col = border_color.rgb * (0.42 + 0.8 * l + (side > 0.0 ? 0.1 * crest : 0.0));
			} else {
				col = border_color.rgb;
			}
			a = max(a, border_color.a);
		} else if (band < 1.0) {
			vec4 b = carved ? bevel_dark : (lit ? bevel_light : bevel_dark);
			col = mix(col, b.rgb, b.a);
			a = max(a, b.a);
		} else if (trim_px > 0.0 && depth >= trim_inset && depth < trim_inset + trim_px
				&& depth > rim + 2.0) {
			col = mix(col, trim_color.rgb, trim_color.a);
			a = max(a, trim_color.a);
		}
		// the corner caps: a quarter disc of cast metal over each corner
		if (cap_px > 0.0 && depth >= 1.0) {
			float r = length(c - 0.5);
			if (r < cap_px) {
				// the top left cap faces the light, the bottom right away
				float up = (left < right && top < bottom) ? 1.0
					: ((left >= right && top >= bottom) ? -1.0 : 0.0);
				float e = cap_px - r;
				float k;
				if (e < 1.0) {
					k = 0.3;
				} else if (e < 2.5) {
					k = 0.95 + 0.5 * up;
				} else {
					k = 0.8 + 0.25 * up + 0.12 * (noise(local * 0.7) - 0.5);
				}
				vec3 m = cap_color.rgb * k;
				// a ring cut in it
				float ring = abs(r - cap_px * 0.62);
				m *= 1.0 - 0.45 * (1.0 - smoothstep(0.4, 1.2, ring));
				// its rivet, lit on the side toward the panel's corner
				float rv = cap_px * 0.32;
				vec2 sv = c - vec2(rv);
				float rd = length(sv);
				float rr = max(cap_px * 0.12, 1.6);
				if (rd < rr) {
					float to = -(sv.x + sv.y) / max(rd, 1e-3);
					m = mix(m, cap_color.rgb * (to > 0.3 ? 1.7 : (to < -0.3 ? 0.5 : 1.1)),
						clamp((rr - rd) / 0.8, 0.0, 1.0));
				}
				col = m;
				a = max(a, cap_color.a);
			}
		}
		// soft outer edge (one pixel)
		a *= clamp(depth * 1.5, 0.0, 1.0);
		COLOR = vec4(col, a) * tint;
	}
}
