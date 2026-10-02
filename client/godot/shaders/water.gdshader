// Water (map_view.rs): a surface over the pool's stone bed, deeper colour
// where it is deeper, ripples running over it, foam where it meets the
// bank, the torches glinting on it. The fog of war darkens it as it does
// the stone.
shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec4 shallow : source_color = vec4(0.22, 0.38, 0.4, 1.0);
uniform vec4 deep : source_color = vec4(0.04, 0.1, 0.14, 1.0);
uniform vec4 foam_color : source_color = vec4(0.75, 0.8, 0.8, 1.0);
// metres of water over which it turns from shallow to deep
uniform float depth_range = 0.5;
uniform float ripple = 0.35;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
uniform sampler2D depth_tex : hint_depth_texture, filter_linear_mipmap;
uniform sampler2D fow_tex : filter_linear, repeat_disable;

const vec2 FIELD = vec2(80.0, 21.0);

varying vec3 wpos;

void vertex() {
	wpos = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xyz;
}

float n2(vec2 p) {
	return texture(noise_tex, p).r;
}

void fragment() {
	// how much water lies between the surface and what is under it
	float d = texture(depth_tex, SCREEN_UV).r;
	vec4 under = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, d, 1.0);
	float thickness = abs(VERTEX.z - under.z / under.w);
	float depth_t = clamp(thickness / depth_range, 0.0, 1.0);

	// two ripple fields drifting across each other
	float t = TIME;
	vec2 a = wpos.xz * 0.35 + vec2(t * 0.03, t * 0.02);
	vec2 b = wpos.xz * 0.6 - vec2(t * 0.025, -t * 0.035);
	const float DS = 0.01;
	float h = n2(a) + n2(b) * 0.6;
	float hx = n2(a + vec2(DS, 0.0)) + n2(b + vec2(DS, 0.0)) * 0.6;
	float hz = n2(a + vec2(0.0, DS)) + n2(b + vec2(0.0, DS)) * 0.6;
	vec3 wn = normalize(vec3((h - hx) / DS * ripple * 0.02, 1.0, (h - hz) / DS * ripple * 0.02));

	float foam = 1.0 - smoothstep(0.0, 0.12, thickness);
	foam *= smoothstep(0.35, 0.7, n2(wpos.xz * 1.5 + t * 0.05) + 0.3);

	vec3 col = mix(shallow.rgb, deep.rgb, depth_t);
	col = mix(col, foam_color.rgb, foam * 0.8);
	// remembered: grey and dark as the stone around it
	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
	float luma = dot(col, vec3(0.299, 0.587, 0.114));
	col = mix(mix(col, luma * vec3(0.54, 0.59, 0.72), 0.8) * 0.4, col, seen);

	ALBEDO = col;
	ALPHA = mix(0.55, 0.92, depth_t) + foam * 0.3;
	ROUGHNESS = mix(0.06, 0.5, foam);
	METALLIC = 0.0;
	SPECULAR = 0.6;
	NORMAL = normalize((VIEW_MATRIX * vec4(wn, 0.0)).xyz);
}
