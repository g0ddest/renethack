// Water (map_view.rs): a surface over the pool's stone bed, deeper colour
// where it is deeper, ripples running over it, the dark above sheening on
// it at a glance, the torches glinting on it. Along a bank (`bank`) it is
// a strip of foam lapping at the stone instead. The fog of war darkens it
// as it does the stone.
shader_type spatial;
render_mode blend_mix, depth_draw_always, cull_back, diffuse_burley, specular_schlick_ggx;

uniform vec4 shallow : source_color = vec4(0.1, 0.22, 0.24, 1.0);
uniform vec4 deep : source_color = vec4(0.015, 0.05, 0.075, 1.0);
uniform vec4 foam_color : source_color = vec4(0.62, 0.68, 0.68, 1.0);
// the dark over the water, seen in it at a glance
uniform vec4 sheen : source_color = vec4(0.16, 0.2, 0.26, 1.0);
// metres of water over which it turns from shallow to deep
uniform float depth_range = 0.4;
uniform float ripple = 1.0;
// a strip of foam along a bank: its UV.y runs from the stone (0) out
uniform bool bank = false;
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

// three ripple fields drifting across each other
float waves(vec2 p, float t) {
	return n2(p * 0.35 + vec2(t * 0.03, t * 0.02))
		+ n2(p * 0.7 - vec2(t * 0.025, -t * 0.035)) * 0.6
		+ n2(p * 1.9 + vec2(-t * 0.05, t * 0.04)) * 0.1;
}

void fragment() {
	float t = TIME;
	float seen = texture(fow_tex, (wpos.xz + 0.5) / FIELD).r;
	const float DS = 0.02;
	float h = waves(wpos.xz, t);
	float hx = waves(wpos.xz + vec2(DS, 0.0), t);
	float hz = waves(wpos.xz + vec2(0.0, DS), t);
	vec3 wn = normalize(vec3((h - hx) / DS * ripple * 0.12, 1.0, (h - hz) / DS * ripple * 0.12));
	vec3 n = normalize((VIEW_MATRIX * vec4(wn, 0.0)).xyz);

	if (bank) {
		// foam lapping at the stone: thick against it, breaking up outwards
		float lap = 0.5 + 0.5 * sin(t * 1.4 + wpos.x * 2.3 + wpos.z * 1.7);
		float reach = mix(0.25, 0.6, lap);
		float edge = 1.0 - smoothstep(reach * 0.4, reach, UV.y);
		float breakup = smoothstep(0.3, 0.65, n2(wpos.xz * 2.4 + vec2(t * 0.06, -t * 0.04)) + 0.25 * (1.0 - UV.y));
		float a = edge * breakup;
		vec3 c = foam_color.rgb;
		float luma = dot(c, vec3(0.299, 0.587, 0.114));
		ALBEDO = mix(luma * vec3(0.54, 0.59, 0.72) * 0.4, c, seen);
		ALPHA = clamp(a * 0.5, 0.0, 1.0);
		ROUGHNESS = 0.6;
		NORMAL = n;
	} else {
		// how much water lies between the surface and what is under it
		float d = texture(depth_tex, SCREEN_UV).r;
		vec4 under = INV_PROJECTION_MATRIX * vec4(SCREEN_UV * 2.0 - 1.0, d, 1.0);
		float thickness = abs(VERTEX.z - under.z / under.w);
		float depth_t = clamp(thickness / depth_range, 0.0, 1.0);

		vec3 col = mix(shallow.rgb, deep.rgb, depth_t);
		// light caught in the crests
		col *= mix(0.85, 1.2, smoothstep(0.9, 1.4, h));
		// remembered: grey and dark as the stone around it
		float luma = dot(col, vec3(0.299, 0.587, 0.114));
		col = mix(mix(col, luma * vec3(0.54, 0.59, 0.72), 0.8) * 0.4, col, seen);

		// the dark above it, mirrored more the flatter it is seen
		float facing = clamp(dot(n, VIEW), 0.0, 1.0);
		float fresnel = 0.04 + 0.96 * pow(1.0 - facing, 5.0);
		// and broken up by the ripples into bands of sheen
		float streak = smoothstep(0.5, 0.75, n2(wpos.xz * 0.8 + wn.xz * 3.0 + t * 0.01));
		EMISSION = sheen.rgb * (fresnel * 3.0 + 0.08 + streak * 0.4) * mix(0.3, 1.0, seen);

		ALBEDO = col;
		ALPHA = mix(0.8, 0.97, depth_t);
		// rough enough, and rougher where the ripples turn faster than
		// a pixel: the lights glint as soft streaks along them, not as
		// points (stars)
		float turn = length(fwidth(wn));
		ROUGHNESS = clamp(0.28 + turn * 6.0, 0.28, 0.85);
		METALLIC = 0.0;
		SPECULAR = 0.45;
		NORMAL = n;
	}
}
