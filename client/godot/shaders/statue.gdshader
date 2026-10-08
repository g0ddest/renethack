// A statue's stone (the manifest's `statue` material, art.rs): cold grey
// whatever lights it. Marble under a torch turned the colour of skin, and
// a statue passed for the creature it shows. This stone takes a light's
// strength and next to nothing of its colour, mirrors nothing, and is
// grained, veined and weathered at the scale of the game's camera, so
// that it reads as cut stone and never as flesh.
shader_type spatial;
render_mode cull_back;

uniform vec4 color : source_color = vec4(0.16, 0.18, 0.21, 1.0);
uniform float roughness = 0.95;
// how much of a light's own colour the stone keeps
uniform float warmth = 0.12;

// the model's own space in metres: the grain stays on a statue however it
// is turned
varying vec3 at;

float hash3(vec3 p) {
	p = fract(p * 0.3183099 + 0.1);
	p *= 17.0;
	return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}

float noise3(vec3 x) {
	vec3 i = floor(x);
	vec3 f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	return mix(mix(mix(hash3(i), hash3(i + vec3(1, 0, 0)), f.x),
			mix(hash3(i + vec3(0, 1, 0)), hash3(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(hash3(i + vec3(0, 0, 1)), hash3(i + vec3(1, 0, 1)), f.x),
			mix(hash3(i + vec3(0, 1, 1)), hash3(i + vec3(1, 1, 1)), f.x), f.y), f.z);
}

void vertex() {
	at = VERTEX * length(MODEL_MATRIX[0].xyz);
}

void fragment() {
	// speckled like granite, mottled in patches a hand across, and a few
	// dark veins through it
	float speck = noise3(at * 120.0);
	float patch = noise3(at * 13.0 + 7.3);
	float vein = noise3(at * 5.5 + 3.1);
	float crack = 1.0 - smoothstep(0.0, 0.035, abs(vein - 0.5));
	vec3 stone = color.rgb * mix(0.72, 1.18, speck) * mix(0.78, 1.12, patch);
	stone *= 1.0 - 0.45 * crack;
	// dust settles on what faces up; what faces down stays dark
	float up = (INV_VIEW_MATRIX * vec4(NORMAL, 0.0)).y;
	stone *= mix(0.8, 1.15, smoothstep(-0.3, 0.9, up));
	ALBEDO = stone;
	ROUGHNESS = roughness;
	SPECULAR = 0.08;
	METALLIC = 0.0;
}

void light() {
	// the light's strength, hardly its hue
	float strength = dot(LIGHT_COLOR, vec3(0.2126, 0.7152, 0.0722));
	vec3 cold = mix(vec3(strength), LIGHT_COLOR, warmth);
	DIFFUSE_LIGHT += clamp(dot(NORMAL, LIGHT), 0.0, 1.0) * ATTENUATION * cold / PI;
}
