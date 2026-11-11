// The game's name on the title, until a logo is drawn: its letters cast
// in old, tarnished gold, lit from below as by the embers of a fire: dark
// at their tops, the dull gold warming down them to an ember-lit glow at
// their feet; mottled with tarnish; each stroke's lower edge caught by the
// light from below and its upper edge in shadow. The label's own colour is
// white: what comes in white is the letters, anything darker (their
// outline, their shadow) keeps its colour. `top` and `tall`: where the
// letters stand in the label (local pixels).
shader_type canvas_item;

uniform float top = 0.0;
uniform float tall = 72.0;
// the gold at the letters' tops, their middle and their feet
uniform vec4 dark : source_color = vec4(0.17, 0.12, 0.07, 1.0);
uniform vec4 mid : source_color = vec4(0.42, 0.31, 0.16, 1.0);
uniform vec4 lit_foot : source_color = vec4(0.72, 0.42, 0.16, 1.0);
// the ember light on the lower edges
uniform vec4 ember : source_color = vec4(0.95, 0.5, 0.18, 1.0);
// how far in from a stroke's edge its bevel reaches, in texels
uniform float bevel = 1.6;

varying vec2 local;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

void vertex() {
	local = VERTEX;
}

void fragment() {
	vec4 c = COLOR;
	float white = smoothstep(0.85, 0.98, min(c.r, min(c.g, c.b)));
	float t = clamp((local.y - top) / max(tall, 1.0), 0.0, 1.0);
	// dark above, warming down to the feet
	vec3 g = t < 0.55
		? mix(dark.rgb, mid.rgb, smoothstep(0.0, 0.55, t))
		: mix(mid.rgb, lit_foot.rgb, smoothstep(0.55, 1.0, t));
	// tarnish: darker, greener mottling in the gold
	float tarnish = noise(local * 0.09) * 0.6 + noise(local * 0.31) * 0.4;
	g = mix(g, g * vec3(0.62, 0.66, 0.6), smoothstep(0.45, 0.85, tarnish) * 0.7);
	// the bevel: the glyph's coverage a little above and below (and to
	// the left and right) of this texel; where it falls off, an edge
	vec2 tp = TEXTURE_PIXEL_SIZE * bevel;
	float a0 = texture(TEXTURE, UV).a;
	float up = texture(TEXTURE, UV - vec2(0.0, tp.y)).a;
	float dn = texture(TEXTURE, UV + vec2(0.0, tp.y)).a;
	float lf = texture(TEXTURE, UV - vec2(tp.x, 0.0)).a;
	float rt = texture(TEXTURE, UV + vec2(tp.x, 0.0)).a;
	// lit from below: the lower edges catch the embers' light, the upper
	// ones are in shadow
	float from_below = clamp((a0 - dn) + 0.4 * (a0 - lf), 0.0, 1.0);
	float shaded = clamp((a0 - up) + 0.4 * (a0 - rt), 0.0, 1.0);
	g = mix(g, ember.rgb, from_below * (0.35 + 0.4 * t));
	g *= 1.0 - shaded * 0.5;
	COLOR = vec4(mix(c.rgb, g, white), c.a);
}
