// A creature behind the hero, its contour seen through them (map_view.rs:
// the next pass of xray_mask.gdshader): its shell grown a pixel or two on
// screen, drawn flat where the hero hides it and outside what the mask
// marked: one clean line round its silhouette, in the hostile's or the
// pet's colour.
shader_type spatial;
render_mode unshaded, blend_mix, depth_test_inverted, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;
stencil_mode read, compare_not_equal, 2;

uniform vec4 color : source_color = vec4(1.0, 0.3, 0.22, 0.85);
uniform float pull = 0.35;
// how far the contour stands out of the silhouette (pixels)
uniform float width = 1.8;

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

void fragment() {
	if ((CAMERA_VISIBLE_LAYERS & 1u) == 0u) {
		discard;
	}
	ALBEDO = color.rgb * 1.3;
	ALPHA = color.a;
}
