// A creature behind the hero (map_view.rs: the overlay on its meshes while
// the hero's body hides it): where it is hidden it marks the stencil and
// draws nothing, so that its contour (xray_contour.gdshader, the next
// pass) draws only round it, never over its inner folds. Pulled towards
// the camera by about a body's depth first, as xray.gdshader, so its own
// surfaces never hide it.
shader_type spatial;
render_mode unshaded, blend_mix, depth_test_inverted, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;
stencil_mode write, compare_always, 2;

uniform float pull = 0.35;

void vertex() {
	VERTEX = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
	VERTEX += normalize(-VERTEX) * pull;
	NORMAL = normalize((MODELVIEW_MATRIX * vec4(NORMAL, 0.0)).xyz);
}

void fragment() {
	// a camera that does not see the map's own layer is the doll's
	if ((CAMERA_VISIBLE_LAYERS & 1u) == 0u) {
		discard;
	}
	ALBEDO = vec3(0.0);
	ALPHA = 0.0;
}
