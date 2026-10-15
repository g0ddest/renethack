// A trapper's or a lurker above's body (creature_kit.rs): a cloak of
// flesh lying in wait. It breathes, its hump rising and falling slowly; a
// ripple runs round its ragged rim (not over its maw, in front), and one
// aloft (a lurker above) beats its sides like wings; its hide
// leathery in its colour, creased, darker over its hump, its rim dark where
// its underside curls up; its underside (its texture's u past 2) dark and
// wet.
shader_type spatial;

uniform vec4 color : source_color = vec4(0.3, 0.5, 0.2, 1.0);
uniform vec4 belly : source_color = vec4(0.13, 0.05, 0.04, 1.0);
// how far its rim lifts as the ripple runs round, and its hump as it
// breathes, metres
uniform float ripple = 0.05;
uniform float breath = 0.018;
// the ripple's turns round it a second
uniform float pace = 0.16;
// 1 for a corpse: none of it moves
uniform float still = 0.0;

varying vec3 local;
varying float out_r;
varying float under;

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

// how far it lifts `r` of the way out at `a` round it (from its front),
// `aloft` of the way to flying
float lift(float r, float a, float t, float aloft) {
	// its front, where the maw is, is still
	float front = pow(max(cos(a), 0.0), 3.0);
	float wave = sin(3.0 * a - TAU * pace * t) * (0.65 + 0.35 * sin(t * 0.7 + 2.0 * a));
	// both sides beating together, from the front back
	float beat = sin(TAU * 0.45 * t - 2.0 * cos(a)) * pow(abs(sin(a)), 1.5);
	float edge = mix(ripple * wave, 0.11 * beat, aloft);
	return edge * r * r * r * (1.0 - front)
		+ breath * (1.0 - r * r) * sin(t * TAU / 3.4);
}

void vertex() {
	local = VERTEX;
	under = step(1.5, UV.x);
	float a = fract(UV.x) * TAU;
	float r = UV.y;
	out_r = r;
	float t = TIME * (1.0 - still);
	// one that hangs in the air (its model's foot well over the floor)
	float aloft = smoothstep(0.3, 0.8, NODE_POSITION_WORLD.y);
	VERTEX.y += lift(r, a, t, aloft);
	// the slope the ripple gives it, out from its middle
	float e = 0.02;
	float slope = (lift(min(r + e, 1.0), a, t, aloft) - lift(max(r - e, 0.0), a, t, aloft)) / (2.0 * e * 0.5);
	vec2 outward = vec2(sin(a), cos(a));
	NORMAL = normalize(NORMAL - vec3(outward.x, 0.0, outward.y) * slope * (1.0 - 2.0 * under));
}

void fragment() {
	float n = fbm(local * 4.0);
	// blotches darker on its back, creases in its hide and pores
	float blotch = smoothstep(0.5, 0.75, fbm(local * 7.0 + 11.0));
	float crease = 1.0 - smoothstep(0.0, 0.06, abs(noise(local * 16.0) - 0.5));
	float fine = 1.0 - smoothstep(0.0, 0.05, abs(noise(local * 37.0 + 7.0) - 0.5));
	// its colour on a creature's skin, not a painted thing's
	float grey = dot(color.rgb, vec3(0.3, 0.55, 0.15));
	vec3 skin = mix(color.rgb, vec3(0.42, 0.36, 0.28) * (0.6 + grey), 0.5);
	vec3 hide = skin * mix(0.65, 1.12, n);
	hide = mix(hide, skin * vec3(0.45, 0.4, 0.38), 0.6 * blotch);
	hide *= 1.0 - 0.2 * crease - 0.12 * fine;
	// darker over its thick hump; its rim dark, its underside curling up
	hide *= mix(0.72, 1.0, smoothstep(0.0, 0.5, out_r));
	hide = mix(hide, belly.rgb, 0.85 * smoothstep(0.78, 0.98, out_r));
	vec3 wet = belly.rgb * mix(0.7, 1.3, n);
	ALBEDO = mix(hide, wet, under);
	ROUGHNESS = mix(0.5 + 0.2 * crease, 0.22, under);
	SPECULAR = 0.5;
	// lit round its edge, as the creatures' rim is
	RIM = 0.5;
	RIM_TINT = 0.5;
}
