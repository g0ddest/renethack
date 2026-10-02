// Lava (map_view.rs): a crust of dark cooling rock floating on a molten
// glow that shows through its cracks and seethes, drifting slowly; it
// lights itself. The fog of war dims what is remembered.
shader_type spatial;
render_mode blend_mix, cull_back, diffuse_burley;

uniform vec4 molten : source_color = vec4(1.0, 0.35, 0.06, 1.0);
uniform vec4 crust : source_color = vec4(0.07, 0.05, 0.045, 1.0);
uniform float energy = 3.5;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
uniform sampler2D fow_tex : filter_linear, repeat_disable;

const vec2 FIELD = vec2(80.0, 21.0);

varying vec3 wpos;

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
	// the surface heaves a little
	float h = texture(noise_tex, wpos.xz * 0.2 + TIME * 0.01).r;
	VERTEX.y += (h - 0.5) * 0.03;
}

void fragment() {
	float t = TIME;
	vec2 flow = vec2(t * 0.015, t * 0.008);
	float plates = texture(noise_tex, wpos.xz * 0.45 + flow).r;
	float fine = texture(noise_tex, wpos.xz * 1.7 - flow * 2.0).r;
	// cracks between the plates of crust, wider where it is hotter
	float heat = texture(noise_tex, wpos.xz * 0.12 + vec2(t * 0.004, 0.0)).r;
	float gap = 0.012 + heat * 0.03;
	float crack = 1.0 - smoothstep(gap * 0.4, gap, abs(plates - 0.5) - (fine - 0.5) * 0.08);
	float seethe = 0.8 + 0.2 * sin(t * 2.1 + plates * 13.0);
	vec3 glow = molten.rgb * crack * seethe;
	vec3 rock = crust.rgb * mix(0.7, 1.3, fine);

	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
	ALBEDO = mix(rock, molten.rgb * 0.3, crack);
	ROUGHNESS = mix(0.9, 0.4, crack);
	EMISSION = (glow * energy + rock * 0.15 * heat) * mix(0.35, 1.0, seen);
}
