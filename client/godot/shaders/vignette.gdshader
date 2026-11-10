// The map's frame (map_view.rs): its highlights lifted, the corners
// darken towards a cold black, and a faint film grain moves over the
// darks. Drawn under the HUD.
shader_type canvas_item;

uniform sampler2D screen : hint_screen_texture, filter_linear_mipmap;
uniform vec3 edge_color : source_color = vec3(0.020, 0.024, 0.035);
// where the darkening starts and ends, as a share of the way to a corner
uniform float radius = 0.78;
uniform float softness = 0.45;
uniform float strength = 0.40;
uniform float grain = 0.025;
// a new level comes up out of black (map_view.rs)
uniform float fade = 0.0;
// how far the highlights are lifted towards white, the darks left as they
// are and the hue kept (the frame lived in its lower half: a lit room's
// brightest read as a grey); none in the overview (map_view.rs)
uniform float lift = 0.37;
// a gentle split tone: the lights a little warmer, the shadows a little
// cooler (each keeping its luma: the dark stays as dark), and the
// mid-tones a little richer (the frame read a murky brown-grey)
uniform vec3 warm = vec3(1.06, 1.0, 0.9);
uniform vec3 cool = vec3(0.95, 1.0, 1.07);
uniform float mid_saturation = 0.18;
// how much of the cool the branch takes in its shadows (none where its
// dark is its fire's: Gehennom's red haze turned grey)
uniform float cool_amount = 1.0;

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

void fragment() {
	vec3 col = texture(screen, SCREEN_UV).rgb;
	// (by the luma: lifting each channel would turn a torch's orange
	// redder, not brighter)
	float l0 = dot(col, vec3(0.2126, 0.7152, 0.0722));
	float l1 = l0 + lift * smoothstep(0.18, 0.6, l0) * (1.0 - l0);
	col *= l1 / max(l0, 1e-4);
	// (each tint scaled to keep the luma it is laid over)
	vec3 w = warm / dot(warm, vec3(0.2126, 0.7152, 0.0722));
	vec3 c = cool / dot(cool, vec3(0.2126, 0.7152, 0.0722));
	// (warmer only where the light is already warm, a torch's or a fire's:
	// ice, water and the moon's light keep their cold, a pale floor of ice
	// warmed read as sand)
	float warmth = smoothstep(0.0, 0.15, (col.r - col.b) / max(l1, 1e-3));
	float hi = smoothstep(0.3, 0.75, l1) * warmth;
	float lo = (1.0 - smoothstep(0.04, 0.3, l1)) * cool_amount;
	col *= mix(vec3(1.0), w, hi) * mix(vec3(1.0), c, lo);
	float mid = smoothstep(0.06, 0.25, l1) * (1.0 - smoothstep(0.45, 0.8, l1));
	float l2 = dot(col, vec3(0.2126, 0.7152, 0.0722));
	col = max(mix(vec3(l2), col, 1.0 + mid_saturation * mid), vec3(0.0));
	// a little wider than tall: the frame follows the 16:9 screen
	vec2 d = (UV - 0.5) * vec2(1.0, 0.92) * 1.4142;
	float v = smoothstep(radius - softness, radius, length(d)) * strength;
	col = mix(col, edge_color, v);
	float luma = dot(col, vec3(0.2126, 0.7152, 0.0722));
	float n = hash(floor(FRAGCOORD.xy) + fract(TIME * 7.3) * 517.0) - 0.5;
	col += n * grain * (1.0 - smoothstep(0.0, 0.6, luma) * 0.7);
	col = mix(col, edge_color * 0.3, fade);
	COLOR = vec4(col, 1.0);
}
