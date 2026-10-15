// The inside of a vortex or an air elemental that engulfed the hero
// (engulf.rs): its funnel round them and the floor of its stuff under
// them. Banded by noise flowing round and up it, as vortex.gdshader's
// creatures are; the far wall dense, the near one (between the hero and
// the camera) faint; bits of its stuff whirled in it (grit, glints, arcs,
// embers). The floor (its texture's u past 2: its v runs from its middle
// out) is whole under the hero, over the cell the level drew there, and
// its swirl thins out into the dark round them.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform vec4 color : source_color = vec4(1.0);
// the brighter stuff in it: flames' yellow, arcs' white, glints
uniform vec4 core : source_color = vec4(1.0);
// how bright it burns (fire, energy), 0 for none
uniform float glow = 0.0;
// the bands round it (0 for none), the turns they make from its foot to
// its top, how fast they flow
uniform float bands = 3.0;
uniform float twist = 1.5;
uniform float speed = 0.6;
uniform float density = 0.6;
// 1 for fog: soft puffs, no bands
uniform float soft = 0.0;
// 1 for a gale: thin fast streaks instead of bands
uniform float streaks = 0.0;
// the bits whirled in it: 0 none, 1 dark grit, 2 bright glints, 3 arcs,
// 4 embers rising
uniform int specks = 0;
// how far out the floor is whole, of its radius
uniform float core_r = 0.45;

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

// bits on a grid that turns round it and rises: 1 on a bit, 0 off
float bits(vec2 at, float t) {
	vec2 cell = floor(at);
	vec2 f = fract(at) - 0.5;
	float h = hash(vec3(cell, 7.0));
	vec2 off = vec2(hash(vec3(cell, 1.0)), hash(vec3(cell, 2.0))) - 0.5;
	// an arc is a short streak along the swirl, kinked
	vec2 d = f - 0.6 * off;
	if (specks == 3) {
		d.y += 0.12 * sign(sin(d.x * 40.0 + h * 9.0)) * step(abs(d.x), 0.3);
		d.x *= 0.3;
	}
	float r = length(d);
	float on = step(0.72, h);
	if (specks == 3) {
		// an arc flashes now and then, for a moment
		on *= step(0.9, fract(h * 13.0 + t * (0.7 + h)));
	}
	if (specks == 2) {
		// a glint twinkles
		on *= 0.5 + 0.5 * sin(t * 6.0 + h * 40.0);
	}
	return on * (1.0 - smoothstep(0.05, 0.16, r));
}

void vertex() {
	local = VERTEX;
}

void fragment() {
	bool ground = UV.x > 1.5;
	float around = fract(UV.x) * TAU;
	float y = UV.y;
	float t = TIME * speed;
	// the stuff, turned round it and rising
	float c = cos(t);
	float s = sin(t);
	vec3 p = vec3(c * local.x + s * local.z, local.y - 0.5 * t, c * local.z - s * local.x);
	float n = fbm(p * 1.6);
	float wind = 0.5 + 0.5 * sin(around * bands + y * twist * TAU - 2.5 * t + 5.0 * n);
	float band = mix(bands > 0.5 ? smoothstep(0.1, 0.9, wind) : 1.0, 1.0, soft);
	// a gale: thin streaks laid round it, racing
	float streak = smoothstep(0.62, 0.95, noise(vec3(around * 3.0 - 4.0 * t, y * 18.0, 0.5 * t)));
	band = mix(band, streak, streaks);
	float a;
	vec3 col;
	if (ground) {
		// y: 0 at the middle, 1 at the rim; drawn in to the middle
		float spiral = 0.5 + 0.5 * sin(around * max(bands, 1.0) - y * 9.0 + 3.0 * t + 4.0 * n);
		float swirl = mix(smoothstep(0.2, 0.9, spiral), n, soft);
		col = color.rgb * mix(0.12, 0.6, swirl * (0.4 + n));
		// whole under the hero, then only its swirl, thinning out
		float whole = 1.0 - smoothstep(core_r, core_r + 0.2, y);
		a = mix(swirl * (0.3 + 0.8 * n), 1.0, whole) * (1.0 - smoothstep(0.6, 1.0, y));
	} else {
		float ends = smoothstep(0.0, 0.12, y) * (1.0 - smoothstep(0.6, 1.0, y));
		float edge = 1.0 - abs(dot(NORMAL, VIEW));
		// soft where seen edge on, no line along its sides
		float body = mix(1.0 - 0.6 * edge * edge, 1.0 - 0.5 * edge, soft);
		// the near wall is seen through, the hero inside it
		float near = FRONT_FACING ? 0.3 : 1.0;
		a = density * ends * body * near * mix(0.25, 1.0, band) * (0.6 + 0.8 * n);
		col = color.rgb * mix(0.65, 1.3, n * band);
		// hotter, brighter low down in it (a fire's heart)
		col = mix(col, core.rgb, glow * 0.25 * band * (1.0 - y));
	}
	if (specks > 0) {
		vec2 at = vec2(around * 7.0 - 3.0 * t, y * (ground ? 10.0 : 12.0) - (specks == 4 ? 2.0 : 0.6) * t);
		float b = bits(at, TIME);
		vec3 bit = specks == 1 ? color.rgb * 0.35 : core.rgb * 1.5;
		col = mix(col, bit, b);
		a = max(a, b * (ground ? 1.0 : 0.9));
	}
	ALBEDO = col * (1.0 + glow * 0.4 * band);
	ALPHA = clamp(a, 0.0, 1.0);
}
