// The map's frame (map_view.rs): the corners darken towards a cold black,
// and a faint film grain moves over the darks. Drawn under the HUD.
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

float hash(vec2 p) {
	vec3 q = fract(vec3(p.xyx) * 0.1031);
	q += dot(q, q.yzx + 33.33);
	return fract((q.x + q.y) * q.z);
}

void fragment() {
	vec3 col = texture(screen, SCREEN_UV).rgb;
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
