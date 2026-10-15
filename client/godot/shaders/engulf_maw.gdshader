// The inside of a creature that engulfed the hero (engulf.rs): a purple
// worm's gullet (a tube through them), the fold of a trapper's or a
// lurker's flesh, a jelly's goo (a blob round them). Drawn from inside:
// only its far half and its floor show (its near half, between the hero
// and the camera, faces away), lit by the hero's light and its own. Its
// walls swell and draw in as it breathes, rings pass down a gullet, folds
// crease a hide; wet, its veins and crests catching the light. Goo is
// smooth and glossy, bubbles rising in it. What lies towards the camera,
// far round the hero or high over them fades out: no edge where its near
// half is not drawn.
shader_type spatial;
render_mode cull_front, blend_mix, depth_draw_always, shadows_disabled, fog_disabled;

uniform vec4 flesh : source_color = vec4(0.45, 0.13, 0.32, 1.0);
// its folds' and rings' crests, and its crevices
uniform vec4 ridge : source_color = vec4(0.78, 0.36, 0.55, 1.0);
uniform vec4 deep : source_color = vec4(0.12, 0.02, 0.08, 1.0);
// rings passing down a gullet, folds creasing it, how much of a jelly's
// goo it is (0..1 each)
uniform float rings = 1.0;
uniform float folds = 0.4;
uniform float goo = 0.0;
// bands of muscle up its walls, meeting over the hero (a hide wrapped
// round them): how many round it, 0 for none
uniform float ribs = 0.0;
// the rings a turn of its texture's v holds
uniform float ring_k = 30.0;
// how far its walls swell and draw in, metres
uniform float breath = 0.06;
// its floor, under its middle (its own y): its walls never sink through
uniform float floor_y = -1.8;
// how far round the hero and how high over the floor it shows
uniform float reach = 3.0;
uniform float tall = 3.2;

varying vec3 local;
// where it is from the floor under the hero
varying vec3 rel;
varying float crest;
varying float fold_h;

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

void vertex() {
	local = VERTEX;
	// (down a gullet, away from the camera)
	float ring = sin(UV.y * ring_k + TIME * 2.2);
	// a rib's crest, drawn tight and let go in turn
	float rib = ribs > 0.5 ? pow(abs(sin(UV.x * PI * ribs)), 6.0) * (0.75 + 0.25 * sin(TIME * 1.6 + UV.x * 9.0)) : 0.0;
	crest = max(rings * ring, rib);
	float fold = fbm(VERTEX * 0.9 + vec3(0.0, TIME * 0.08, 0.0)) - 0.5;
	fold_h = fold * folds;
	// its floor breathes less: the hero stands on it
	float wall = smoothstep(0.15, 0.6, (VERTEX.y - floor_y) / -floor_y);
	float swell = breath * sin(TIME * 1.3) + 0.06 * crest + 0.5 * fold_h;
	VERTEX += NORMAL * swell * wall;
	VERTEX.y = max(VERTEX.y, floor_y);
	rel = VERTEX - vec3(0.0, floor_y, 0.0);
	// seen from inside
	NORMAL = -NORMAL;
}

void fragment() {
	vec3 p = local * 1.4 + vec3(0.0, TIME * 0.03, 0.0);
	float n = fbm(p);
	// veins: thin dark lines where the noise crosses its middle
	float v = 1.0 - smoothstep(0.0, 0.045, abs(noise(local * 2.2 + 5.0) - 0.5));
	v *= 1.0 - goo;
	// crests of its rings, ribs and folds catch the light
	float c = max(smoothstep(0.2, 1.0, crest), smoothstep(0.08, 0.3, fold_h) * 0.8);
	vec3 col = mix(deep.rgb, flesh.rgb, smoothstep(0.2, 0.7, n));
	col = mix(col, ridge.rgb, max(c * 0.75, smoothstep(0.6, 0.85, n) * 0.5));
	col = mix(col, deep.rgb, v * 0.8);
	// bubbles rising in goo
	vec3 q = local * 2.5 + vec3(0.0, -TIME * 0.35, 0.0);
	vec3 cell = floor(q);
	vec3 f = fract(q) - 0.5;
	float r = length(f);
	float bubble = step(0.78, hash(cell)) * smoothstep(0.26, 0.2, r) * smoothstep(0.08, 0.18, r);
	bubble *= goo;
	col = mix(col, ridge.rgb * 1.4, bubble * 0.8);
	// bumps from its height: its folds, veins and crests
	float h = n * (1.0 - 0.7 * goo) - v * 0.3 + c * 0.4 + bubble * 0.3;
	vec3 dpdx = dFdx(VERTEX);
	vec3 dpdy = dFdy(VERTEX);
	vec3 r1 = cross(dpdy, NORMAL);
	vec3 r2 = cross(NORMAL, dpdx);
	float det = dot(dpdx, r1);
	vec3 grad = sign(det) * (dFdx(h) * r1 + dFdy(h) * r2);
	NORMAL = normalize(abs(det) * NORMAL - grad * 0.9);
	ALBEDO = col;
	ROUGHNESS = mix(0.32, 0.12, goo) + 0.12 * n;
	SPECULAR = 0.65;
	// a dim light of its own, so its depths are never black
	EMISSION = deep.rgb * 0.6 + flesh.rgb * mix(0.12, 0.25, goo) + ridge.rgb * c * 0.08;
	// gone towards the camera, far round the hero, high up
	float near = smoothstep(0.5, 1.6, rel.z);
	float far = smoothstep(reach * 0.55, reach, length(rel.xz));
	float high = smoothstep(tall * 0.5, tall, rel.y);
	ALPHA = (1.0 - near) * (1.0 - far) * (1.0 - high);
}
