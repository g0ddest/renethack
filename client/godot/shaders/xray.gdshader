// The hero seen through what hides them (map_view.rs, an overlay on the
// hero's meshes while something stands between them and the camera):
// drawn only where something nearer the camera covers the model (the depth
// test inverted), a flat tinted silhouette. Pulled towards the camera by
// about a body's depth first, so the model never shows through itself.
shader_type spatial;
render_mode unshaded, blend_mix, depth_test_inverted, depth_draw_never, cull_back, shadows_disabled, fog_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(0.55, 0.68, 1.0, 0.45);
uniform float pull = 0.35;

void vertex() {
	VERTEX = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
	VERTEX += normalize(-VERTEX) * pull;
	NORMAL = normalize((MODELVIEW_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

void fragment() {
	float rim = 1.0 - abs(dot(normalize(NORMAL), normalize(VIEW)));
	ALBEDO = color.rgb * (0.7 + rim * 0.8);
	ALPHA = color.a * (0.6 + rim * 0.4);
}
