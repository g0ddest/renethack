// The game's name on the title, until a logo is drawn: its letters struck
// in gold, bright at their tops, a polished band across their middle and
// dark at their feet; each stroke's upper edge caught by the light and its
// lower edge in shadow, so the letters stand out of the gold like a die's.
// The label's own colour is white: what comes in white is the letters,
// anything darker (their outline, their shadow) keeps its colour. `top`
// and `tall`: where the letters stand in the label (local pixels).
shader_type canvas_item;

uniform float top = 0.0;
uniform float tall = 72.0;
uniform vec4 light : source_color = vec4(0.99, 0.9, 0.64, 1.0);
uniform vec4 mid : source_color = vec4(0.84, 0.6, 0.25, 1.0);
uniform vec4 dark : source_color = vec4(0.4, 0.24, 0.08, 1.0);
// how far in from a stroke's edge its bevel reaches, in texels
uniform float bevel = 1.6;

varying vec2 local;

void vertex() {
	local = VERTEX;
}

void fragment() {
	vec4 c = COLOR;
	float white = smoothstep(0.85, 0.98, min(c.r, min(c.g, c.b)));
	float t = clamp((local.y - top) / max(tall, 1.0), 0.0, 1.0);
	vec3 g = t < 0.5
		? mix(light.rgb, mid.rgb, t / 0.5)
		: mix(mid.rgb * 0.62, dark.rgb, (t - 0.5) / 0.5);
	// the polished band where the two halves meet
	g += vec3(1.0, 0.92, 0.74) * (1.0 - smoothstep(0.0, 0.045, abs(t - 0.5))) * 0.4;
	// the bevel: the glyph's coverage a little above and below (and to
	// the left and right) of this texel; where it falls off, an edge
	vec2 tp = TEXTURE_PIXEL_SIZE * bevel;
	float a0 = texture(TEXTURE, UV).a;
	float up = texture(TEXTURE, UV - vec2(0.0, tp.y)).a;
	float dn = texture(TEXTURE, UV + vec2(0.0, tp.y)).a;
	float lf = texture(TEXTURE, UV - vec2(tp.x, 0.0)).a;
	float rt = texture(TEXTURE, UV + vec2(tp.x, 0.0)).a;
	float lit = clamp((a0 - up) + 0.6 * (a0 - lf), 0.0, 1.0);
	float shaded = clamp((a0 - dn) + 0.6 * (a0 - rt), 0.0, 1.0);
	g = mix(g, vec3(1.0, 0.95, 0.8), lit * 0.55);
	g *= 1.0 - shaded * 0.45;
	COLOR = vec4(mix(c.rgb, g, white), c.a);
}
