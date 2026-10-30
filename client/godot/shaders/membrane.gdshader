// The skin of an air bubble on the Plane of Water (map_view: on an air
// cell's sides that meet the water, its face outwards): thin and clear,
// the water beyond seen bent through it, a soap film's colours shifting
// with the angle and brightest where it is seen edge on. Seen from
// outside, in front of the bubble, it is fainter, and so is any of it in
// front of the hero, close to them: whoever stands at an edge, in or out
// of the bubble, reads through it.
shader_type spatial;
render_mode unshaded, depth_draw_never, cull_disabled, shadows_disabled;

uniform sampler2D screen : hint_screen_texture, filter_linear_mipmap;
// how tall it stands over the ground (metres)
uniform float tall = 1.6;
// how much of it shows seen from outside, or in front of the hero
uniform float outside = 0.3;

global uniform vec3 hero_pos;

varying vec3 wpos;

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

void fragment() {
	float edge = 1.0 - abs(dot(normalize(NORMAL), normalize(VIEW)));
	vec2 bend = vec2(sin(wpos.y * 7.0 + TIME * 1.3 + wpos.x * 3.0),
		cos(wpos.x * 5.0 + wpos.z * 5.0 - TIME * 1.1)) * 0.008;
	vec3 behind = textureLod(screen, SCREEN_UV + bend, 0.0).rgb;
	vec3 film = 0.55 + 0.45 * cos(6.2831 * (vec3(0.0, 0.33, 0.67) + edge * 1.2 + wpos.y * 0.25 + TIME * 0.05));
	float h = clamp(wpos.y / tall, 0.0, 1.0);
	float shown = smoothstep(0.0, 0.08, h) * smoothstep(1.0, 0.6, h);
	ALBEDO = mix(behind, film, 0.12 + 0.3 * edge) + vec3(0.5, 0.8, 1.0) * pow(edge, 3.0) * 0.4;
	// in front of the hero (their row or south of it, the camera's side)
	// and within a step or so of them
	float near = 1.0 - smoothstep(1.0, 1.8, length(wpos.xz - hero_pos.xz));
	float before = step(hero_pos.z - 0.6, wpos.z) * near;
	float faint = FRONT_FACING ? outside : mix(1.0, outside, before);
	ALPHA = (0.35 + 0.5 * edge) * shown * faint;
}
