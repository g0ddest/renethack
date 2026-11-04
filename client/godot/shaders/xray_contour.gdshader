// A creature hidden from the camera (map_view.rs: the next pass of
// xray_mask.gdshader): its shell grown a pixel or two on screen, drawn flat
// where something hides it and outside what the mask marked: one clean
// line round its silhouette, in the hostile's or the pet's colour. Never
// over the hero: a kitten behind a wizard was a teal sticker on her robe
// (the hero's body, `hero_feet` and `hero_size`, keeps it off them).
shader_type spatial;
render_mode unshaded, blend_mix, depth_test_inverted, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;
stencil_mode read, compare_not_equal, 2;

uniform vec4 color : source_color = vec4(1.0, 0.3, 0.22, 0.85);
uniform float pull = 0.35;
// how far the contour stands out of the silhouette (pixels)
uniform float width = 1.8;
// the hero's body as a box: their feet, its half width and depth and its
// height (map_view's `show_through`; none: under the floor)
uniform vec3 hero_feet = vec3(0.0, -100.0, 0.0);
uniform vec3 hero_size = vec3(0.45, 1.8, 0.3);

void vertex() {
	vec3 v = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
	v += normalize(-v) * pull;
	vec3 n = normalize((MODELVIEW_MATRIX * vec4(NORMAL, 0.0)).xyz);
	vec4 clip = PROJECTION_MATRIX * vec4(v, 1.0);
	vec2 cn = (PROJECTION_MATRIX * vec4(n, 0.0)).xy;
	float len = length(cn);
	if (len > 1e-5) {
		clip.xy += cn / len * width * 2.0 / VIEWPORT_SIZE * clip.w;
	}
	POSITION = clip;
	VERTEX = v;
}

// Whether the hero's body lies between the eye and `p` (the line from the
// eye to it crosses their box before it gets there).
bool behind_hero(vec3 eye, vec3 p) {
	vec3 d = p - eye;
	// (no axis exactly parallel: 1 / 0 is undefined)
	d += vec3(equal(d, vec3(0.0))) * 1e-5;
	vec3 lo = hero_feet - vec3(hero_size.x, 0.0, hero_size.z);
	vec3 hi = hero_feet + vec3(hero_size.x, hero_size.y, hero_size.z);
	vec3 a = (lo - eye) / d;
	vec3 b = (hi - eye) / d;
	vec3 near = min(a, b);
	vec3 far = max(a, b);
	float enter = max(max(near.x, near.y), near.z);
	float leave = min(min(far.x, far.y), far.z);
	return enter <= leave && leave > 0.0 && enter < 1.0;
}

void fragment() {
	if ((CAMERA_VISIBLE_LAYERS & 1u) == 0u) {
		discard;
	}
	if (behind_hero(CAMERA_POSITION_WORLD, (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz)) {
		discard;
	}
	ALBEDO = color.rgb * 1.3;
	ALPHA = color.a;
}
