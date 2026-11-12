// Smoke about Moloch in the prologue (prologue_stage.rs): a fog volume whose
// thickness is a noise drifting up through it and swaying, so the smoke
// rises and stirs; at least `least` of its thickness everywhere (a black
// fog that hides), its edge faded. The fires behind him light it, or it
// smoulders by itself (`emission`).
shader_type fog;

uniform float density = 1.0;
uniform float least = 0.0;
uniform vec3 albedo = vec3(0.02);
uniform vec3 emission = vec3(0.0);
uniform float edge_fade = 0.6;
uniform sampler3D noise : hint_default_white, repeat_enable;
// how many noise tiles a metre, how fast it rises (metres a second), and
// the beat's clock
uniform float grain = 0.12;
uniform float climb = 0.3;
uniform float clock = 0.0;

void fog() {
	vec3 p = WORLD_POSITION;
	p.x += 0.4 * sin(clock * 0.35 + p.y * 0.4);
	p.y -= climb * clock;
	float a = texture(noise, p * grain).r;
	float b = texture(noise, p * grain * 2.3 + vec3(0.37, 0.11, 0.73)).r;
	float n = smoothstep(0.3, 0.75, 0.65 * a + 0.35 * b);
	float shape = clamp(-2.0 * SDF / min(min(SIZE.x, SIZE.y), SIZE.z), 0.0, 1.0);
	DENSITY = density * mix(least, 1.0, n) * pow(shape, edge_fade);
	ALBEDO = albedo;
	EMISSION = emission;
}
