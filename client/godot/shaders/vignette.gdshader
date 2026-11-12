// The map's frame (map_view.rs), graded grim and low key (the owner: a
// gloomy Diablo, not a brighter, richer picture): its mid-tones pressed
// down while the darks and the hottest lights keep theirs, only the
// hottest highlights lifted (a torch, lava, a glint: small and hot, never
// a lit room lifted whole), the lights a little warmer and the shadows a
// little cooler, earth, ash, rust and bone drained of colour while fire,
// magic and blood keep theirs; the corners darken towards a cold black
// and a faint film grain moves over the darks. Drawn under the HUD.
shader_type canvas_item;

uniform sampler2D screen : hint_screen_texture, filter_linear_mipmap;
uniform vec3 edge_color : source_color = vec3(0.016, 0.018, 0.026);
// where the darkening starts and ends, as a share of the way to a corner,
// and how dark it goes: the corners heavy, the sides a fifth darker (from
// 0.74 and 0.48 the frame's outer 15 % lost a third of its light, the
// sides near half, and a foe standing there, where archers stand, was
// near invisible)
uniform float radius = 0.88;
uniform float softness = 0.46;
uniform float strength = 0.58;
uniform float grain = 0.025;
// a new level comes up out of black (map_view.rs)
uniform float fade = 0.0;
// how much of the frame's grade shows: none in the overview, whose far,
// plain view has no mood to set (map_view.rs eases it)
uniform float shown = 1.0;
// how far the mid-tones are pressed down (0: none)
uniform float gloom = 0.14;
// how far the hottest highlights are lifted towards white, their hue kept
uniform float lift = 0.45;
// the split tone: the lights a little warmer where they are warm, the
// shadows a little cooler (each keeping its luma: the dark stays as dark)
uniform vec3 warm = vec3(1.06, 1.0, 0.9);
uniform vec3 cool = vec3(0.95, 1.0, 1.07);
// how much of the cool the branch takes in its shadows (none where its
// dark is its fire's: Gehennom's red haze turned grey)
uniform float cool_amount = 1.0;
// how much colour drains from what is not fire, magic or blood (its own
// colour's purity under `keep_from` loses this share of its saturation)
uniform float drab = 0.25;
uniform float keep_from = 0.55;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

void fragment() {
	vec3 col = texture(screen, SCREEN_UV).rgb;
	const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
	float l0 = dot(col, LUMA);
	// the mid-tones pressed down, the darks and the hottest kept
	float g = gloom * shown * smoothstep(0.02, 0.15, l0) * (1.0 - smoothstep(0.35, 0.8, l0));
	float l1 = l0 * (1.0 - g);
	// only the hottest lifted (by the luma: lifting each channel turned a
	// torch's orange redder, not brighter)
	l1 += lift * shown * smoothstep(0.45, 0.85, l1) * (1.0 - l1);
	col *= l1 / max(l0, 1e-4);
	// (each tint scaled to keep the luma it is laid over; warmer only
	// where the light is already warm, a torch's or a fire's: ice, water
	// and the moon's light keep their cold, a floor of ice warmed read as
	// sand)
	vec3 w = warm / dot(warm, LUMA);
	vec3 c = cool / dot(cool, LUMA);
	float warmth = smoothstep(0.0, 0.15, (col.r - col.b) / max(l1, 1e-3));
	float hi = smoothstep(0.3, 0.75, l1) * warmth;
	float lo = (1.0 - smoothstep(0.04, 0.3, l1)) * cool_amount;
	col *= mix(vec3(1.0), w, hi * shown) * mix(vec3(1.0), c, lo * shown);
	// earth, ash, rust and bone drained; a pure colour (fire, magic,
	// blood) keeps its own
	float mx = max(col.r, max(col.g, col.b));
	float purity = (mx - min(col.r, min(col.g, col.b))) / max(mx, 1e-4);
	float keep = smoothstep(keep_from, keep_from + 0.3, purity);
	float l2 = dot(col, LUMA);
	col = max(mix(vec3(l2), col, 1.0 - drab * shown * (1.0 - keep)), vec3(0.0));
	// a little wider than tall: the frame follows the 16:9 screen
	vec2 d = (UV - 0.5) * vec2(1.0, 0.92) * 1.4142;
	float v = smoothstep(radius - softness, radius, length(d)) * strength * shown;
	col = mix(col, edge_color, v);
	float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
	float n = hash(floor(FRAGCOORD.xy) + fract(TIME * 7.3) * 517.0) - 0.5;
	col += n * grain * (1.0 - smoothstep(0.0, 0.6, luma) * 0.7);
	col = mix(col, edge_color * 0.3, fade);
	COLOR = vec4(col, 1.0);
}
