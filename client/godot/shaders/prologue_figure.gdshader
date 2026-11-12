// The prologue's figures (prologue_stage.rs): Marduk, Moloch and the Amulet
// of Yendor, generated models drawn with their own maps and a life no rig
// gives them. The beat's clock moves them: a breath that swells the chest
// and lifts the shoulders, cloth swaying slowly the lower it hangs (only
// what hangs behind the body, for a figure whose legs show), and a light
// from within: a god's own radiance, brightest at his outline, or a glow in
// the maps' glowing texels (a furnace's cracks, a stone) pulsing with the
// breath, strongest round its heart.
shader_type spatial;
render_mode diffuse_burley, specular_schlick_ggx, cull_disabled;

uniform sampler2D albedo_map : source_color, filter_linear_mipmap_anisotropic;
uniform sampler2D orm_map : hint_default_white, filter_linear_mipmap_anisotropic;
uniform sampler2D normal_map : hint_normal, filter_linear_mipmap_anisotropic;
uniform sampler2D glow_map : hint_default_black, filter_linear_mipmap;

// the maps as the grade wants them: the albedo times `tint`, the metal's
// share times `metal`, rougher by `rough`
uniform vec3 tint = vec3(1.0);
uniform float metal = 1.0;
uniform float rough = 0.0;
uniform float bumps = 1.0;

// seconds into the beat, and how far its life has come up (0..1)
uniform float clock = 0.0;
uniform float rise = 1.0;

// the model's own height: where its soles are, how tall it is
uniform float foot = -0.95;
uniform float tall = 1.9;

// a breath every `breath_secs`: the chest (between these shares of the
// height) swelling `breath_depth` at its front, what is above it rising
// `breath_lift`
uniform float breath_secs = 5.0;
uniform float breath_depth = 0.0;
uniform float breath_lift = 0.0;
uniform vec2 chest = vec2(0.6, 0.8);

// cloth swaying `sway` (model units) at its hem every `sway_secs`, still at
// `hang.y` of the height, freest at `hang.x`; `behind` 1: only what hangs
// behind the body
uniform float sway = 0.0;
uniform float sway_secs = 7.0;
uniform vec2 hang = vec2(0.05, 0.55);
uniform float behind = 0.0;

// the glow of the glowing texels: its colour, its strength, how much of it
// pulses with the breath; stronger by `heart_energy` within `heart_radius`
// of `heart` (model space). What glows is matte (a glow from within, not
// a polished stone mirroring the lights round it), its own colour `dark`
// times as light (a garnet dark round the light in it).
uniform vec3 glow_color = vec3(1.0, 0.4, 0.1);
uniform float glow_dark = 1.0;
uniform float glow_energy = 0.0;
uniform float glow_pulse = 0.5;
uniform vec3 heart = vec3(0.0);
uniform float heart_radius = 0.0;
uniform float heart_energy = 0.0;

// a light from within: the albedo's own colour, and `rim_color` at the
// outline (`rim_power` how close to it)
uniform float radiance = 0.0;
uniform vec3 rim_color = vec3(1.0, 0.85, 0.6);
uniform float rim_energy = 0.0;
uniform float rim_power = 3.0;

varying float near_heart;

const float TAU_ = 6.28318530718;

float inhale() {
	return 0.5 + 0.5 * sin(TAU_ * clock / max(breath_secs, 0.1));
}

void vertex() {
	vec3 rest = VERTEX;
	float h = clamp((rest.y - foot) / tall, 0.0, 1.0);
	float in_ = inhale() * rise;
	// the chest swells at its front; the shoulders and the head rise
	float mid = mix(chest.x, chest.y, 0.5);
	float band = smoothstep(chest.x, mid, h) * (1.0 - smoothstep(mid, chest.y, h));
	VERTEX += NORMAL * breath_depth * band * max(NORMAL.z, 0.0) * in_;
	VERTEX.y += breath_lift * smoothstep(chest.x, chest.y, h) * in_;
	// the cloth: still at the waist and the soles, swaying more the lower
	// it hangs
	float w = (1.0 - smoothstep(hang.x, hang.y, h)) * smoothstep(0.0, hang.x, h);
	w *= mix(1.0, 1.0 - smoothstep(-0.3, -0.18, rest.z), behind);
	w *= w;
	float t = TAU_ * clock / max(sway_secs, 0.1);
	VERTEX.x += sway * w * (sin(t + rest.y * 6.0) + 0.35 * sin(2.3 * t + rest.z * 9.0));
	VERTEX.z += sway * 0.5 * w * sin(0.7 * t + rest.x * 8.0 + 1.1);
	near_heart = heart_radius > 0.0
		? 1.0 - smoothstep(0.4 * heart_radius, heart_radius, distance(rest, heart))
		: 0.0;
}

void fragment() {
	vec3 orm = texture(orm_map, UV).rgb;
	float g = texture(glow_map, UV).r;
	// (the tint is the gold's, the cloth's: what glows keeps its own colour)
	vec3 albedo = texture(albedo_map, UV).rgb;
	ALBEDO = mix(albedo * tint, albedo * glow_dark, g);
	ROUGHNESS = mix(clamp(orm.g + rough, 0.0, 1.0), 1.0, g);
	METALLIC = clamp(orm.b * metal, 0.0, 1.0) * (1.0 - g);
	SPECULAR = 0.5 * (1.0 - g);
	NORMAL_MAP = texture(normal_map, UV).rgb;
	NORMAL_MAP_DEPTH = bumps;
	float pulse = inhale();
	float heat = g * (glow_energy * mix(1.0, pulse, glow_pulse)
		+ heart_energy * near_heart * (0.3 + 0.7 * pulse));
	float edge = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), rim_power);
	EMISSION = (glow_color * heat + ALBEDO * radiance + rim_color * edge * rim_energy) * rise;
}
