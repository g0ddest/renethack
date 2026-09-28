// A liquid orb (ui-design §1.3, §6.4): HP or Pw in a glass sphere set in a
// bronze ring. `fill` is the level (0..1, animated from Rust), `ghost` the
// level before a recent loss, shown pale while it drains; `flash` whitens
// the liquid for a moment after a hit, `pulse` makes it throb (low HP).
shader_type canvas_item;
render_mode unshaded;

uniform float fill : hint_range(0.0, 1.0) = 1.0;
uniform float ghost : hint_range(0.0, 1.0) = 0.0;
uniform float flash : hint_range(0.0, 1.0) = 0.0;
uniform float pulse : hint_range(0.0, 1.0) = 0.0;
uniform vec4 deep : source_color = vec4(0.541, 0.059, 0.071, 1.0);
uniform vec4 bright : source_color = vec4(0.847, 0.149, 0.118, 1.0);
// the liquid disc's radius as a share of the orb's (120 of 148)
uniform float inner = 0.81;
uniform vec4 ring_dark : source_color = vec4(0.16, 0.11, 0.06, 1.0);
uniform vec4 ring_light : source_color = vec4(0.72, 0.55, 0.3, 1.0);

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float aa = fwidth(r) * 1.2;
	float outer = 1.0 - smoothstep(1.0 - aa, 1.0, r);
	if (outer <= 0.0) {
		discard;
	}
	// the bronze ring: a bevelled band, lit from the top left
	float lit = dot(normalize(p + 1e-5), vec2(-0.6, -0.8)) * 0.5 + 0.5;
	float band = smoothstep(inner, 1.0, r);
	float ridge = 1.0 - abs(band - 0.45) * 2.0;
	vec3 ring = mix(ring_dark.rgb, ring_light.rgb, clamp(ridge * 0.8 * lit + 0.12, 0.0, 1.0));
	ring *= 0.8 + 0.2 * noise(p * 40.0);
	// dark hairlines on both edges of the ring
	ring *= mix(0.25, 1.0, smoothstep(0.0, 0.03, 1.0 - r));
	ring *= mix(0.25, 1.0, smoothstep(0.0, 0.025, r - inner));

	// the liquid, in the disc's own coordinates (y 0 at the bottom)
	vec2 q = p / inner;
	float h = 1.0 - (q.y * 0.5 + 0.5);
	float wave = (noise(vec2(q.x * 3.0 + TIME * 0.7, TIME * 0.4)) - 0.5) * 0.035;
	float level = fill + wave * step(0.002, fill) * step(fill, 0.998);
	vec3 glass = vec3(0.02, 0.018, 0.02) + deep.rgb * 0.08;
	vec3 col = glass;
	float n1 = noise(q * 3.5 + vec2(TIME * 0.12, -TIME * 0.25));
	float n2 = noise(q * 7.0 + vec2(-TIME * 0.2, -TIME * 0.4));
	vec3 liquid = mix(deep.rgb, bright.rgb, clamp(h / max(level, 0.05), 0.0, 1.0) * 0.85);
	liquid *= 0.8 + 0.35 * (n1 * 0.6 + n2 * 0.4);
	liquid = mix(liquid, vec3(1.0), flash * 0.6);
	liquid *= 1.0 + pulse * 0.45;
	float in_liquid = 1.0 - smoothstep(level - 0.006, level + 0.006, h);
	float in_ghost = (1.0 - smoothstep(ghost - 0.006, ghost + 0.006, h)) * (1.0 - in_liquid);
	col = mix(col, mix(bright.rgb, vec3(1.0), 0.3) * 0.6, in_ghost * 0.45);
	col = mix(col, liquid, in_liquid);
	// the meniscus: a lighter band on the surface
	float men = (1.0 - smoothstep(0.0, 0.02, abs(h - level))) * step(0.002, level) * step(level, 0.995);
	col = mix(col, bright.rgb * 1.5 + 0.1, men * 0.8);
	// glass: darker rim, a highlight at the top left, a small bottom glint
	float rr = r / inner;
	col *= mix(1.0, 0.35, smoothstep(0.6, 1.0, rr));
	float hl = 1.0 - smoothstep(0.0, 0.42, length((q - vec2(-0.32, -0.45)) * vec2(1.0, 1.6)));
	col += vec3(1.0, 0.95, 0.85) * hl * 0.28;
	float glint = 1.0 - smoothstep(0.0, 0.18, length((q - vec2(0.28, 0.62)) * vec2(1.0, 2.2)));
	col += vec3(1.0, 0.9, 0.8) * glint * 0.08;

	float disc = 1.0 - smoothstep(inner - aa, inner, r);
	vec3 outc = mix(ring, col, disc);
	COLOR = vec4(outc, outer) * tint;
}
