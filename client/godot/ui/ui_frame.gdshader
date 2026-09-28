// The forged frame of every HUD panel and dialog (ui-design §6.4): a
// vertical gradient, a dark outer line, an iron border, a one-pixel bevel
// (light top-left, dark bottom-right), an inner shadow, grain, chamfered
// corners and a thin gold trim inset from the border. Drawn on the panel's
// StyleBoxFlat, whose expand margin leaves room for the drop shadow outside
// the rect. theme.rs keeps `size` current and has the StyleBoxFlat look as
// the fallback.
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
		// a faint warm sheen near the top, like light on leather
		col += vec3(0.02, 0.014, 0.008) * (1.0 - smoothstep(0.0, 0.35, t));
		float shade = smoothstep(0.0, max(inner_shadow_px, 0.001), depth - border_px);
		col *= mix(0.55, 1.0, shade);
		float a = base.a;
		// which side is nearest decides the bevel's light or dark
		float left = local.x, top = local.y;
		float right = size.x - local.x, bottom = size.y - local.y;
		bool lit = min(left, top) <= min(right, bottom);
		float band = depth - 1.0 - border_px;
		if (depth < 1.0) {
			col = outer_color.rgb;
			a = outer_color.a;
		} else if (depth < 1.0 + border_px) {
			col = border_color.rgb;
			a = max(a, border_color.a);
		} else if (band < 1.0) {
			vec4 b = lit ? bevel_light : bevel_dark;
			col = mix(col, b.rgb, b.a);
			a = max(a, b.a);
		} else if (trim_px > 0.0 && depth >= trim_inset && depth < trim_inset + trim_px) {
			col = mix(col, trim_color.rgb, trim_color.a);
			a = max(a, trim_color.a);
		}
		// soft outer edge (one pixel)
		a *= clamp(depth * 1.5, 0.0, 1.0);
		COLOR = vec4(col, a) * tint;
	}
}
