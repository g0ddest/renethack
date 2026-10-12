// Weather that moves about as a creature (creature_kit.rs): a vortex's
// funnel and an air elemental's wind, banded by noise that flows round and
// up them and fades at their foot and top, denser where they are seen
// edge on; a fog cloud's puffs (`soft`), thinning to nothing at their
// rims. A funnel's inner shell has its u moved on by 2: it turns faster
// and is denser. The shape spins here, its mesh's bounds still.
shader_type spatial;
render_mode blend_mix, depth_draw_never, cull_disabled, shadows_disabled, specular_disabled;

uniform vec4 color : source_color = vec4(1.0);
// light of its own (energy, fire), 0 for none
uniform float glow = 0.0;
// the bands round it (0 for none), the turns they make from its foot to
// its top, how fast they flow
uniform float bands = 3.0;
uniform float twist = 1.5;
uniform float speed = 0.6;
uniform float density = 0.6;
// 1 for a cloud: soft at its rims, with no ends
uniform float soft = 0.0;
// the turns a second it spins (the way a creature's Body spins)
uniform float spin = 0.0;

varying vec3 local;

float hash(vec3 p) {
	p = fract(p * 0.1031);
	p += dot(p, p.zyx + 31.32);
	return fract((p.x + p.y) * p.z);
}

float noise(vec3 p) {
	vec3 i = floor(p);
	vec3 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	return mix(
		mix(mix(hash(i), hash(i + vec3(1, 0, 0)), f.x),
			mix(hash(i + vec3(0, 1, 0)), hash(i + vec3(1, 1, 0)), f.x), f.y),
		mix(mix(hash(i + vec3(0, 0, 1)), hash(i + vec3(1, 0, 1)), f.x),
			mix(hash(i + vec3(0, 1, 1)), hash(i + vec3(1, 1, 1)), f.x), f.y),
		f.z);
}

float fbm(vec3 p) {
	return 0.5 * noise(p) + 0.3 * noise(p * 2.03 + 3.7) + 0.2 * noise(p * 4.01 + 9.1);
}

vec2 turned(vec2 xz, float a) {
	return vec2(cos(a) * xz.x - sin(a) * xz.y, sin(a) * xz.x + cos(a) * xz.y);
}

void vertex() {
	local = VERTEX;
	float a = TAU * spin * TIME;
	VERTEX.xz = turned(VERTEX.xz, a);
	NORMAL.xz = turned(NORMAL.xz, a);
}

void fragment() {
	float inner = step(1.5, UV.x);
	float around = fract(UV.x) * TAU;
	float y = UV.y;
	float t = TIME * speed * (1.0 + 0.7 * inner);
	// the stuff it carries, turned round it (the way its body spins) and
	// rising
	float c = cos(t);
	float s = sin(t);
	vec3 p = vec3(c * local.x + s * local.z, local.y - 0.4 * t, c * local.z - s * local.x);
	float n = fbm(p * 5.0);
	// the bands, wound round it and ragged
	float wind = 0.5 + 0.5 * sin(around * bands + y * twist * TAU - 2.5 * t + 5.0 * n);
	float band = bands > 0.5 ? smoothstep(0.1, 0.9, wind) : 1.0;
	float ends = smoothstep(0.0, 0.18, y) * (1.0 - smoothstep(0.65, 1.0, y));
	ends = mix(ends, 1.0, soft);
	float edge = 1.0 - abs(dot(NORMAL, VIEW));
	float body = mix(0.3 + 0.7 * edge * edge, (1.0 - edge) * (1.0 - edge), soft);
	float a = density * ends * body * mix(0.25, 1.0, band) * (0.5 + n) * (1.0 + 0.5 * inner);
	vec3 col = color.rgb * mix(0.7, 1.25, n * band);
	ALBEDO = col;
	EMISSION = col * glow * (0.6 + 0.8 * band);
	ROUGHNESS = 1.0;
	ALPHA = clamp(a, 0.0, 1.0);
}
