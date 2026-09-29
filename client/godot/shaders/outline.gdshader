// The outline of a creature or an object under the pointer (map_view.rs,
// an overlay on its meshes): its back faces pushed out a little, flat.
shader_type spatial;
render_mode unshaded, blend_mix, cull_front, depth_draw_never, shadows_disabled, fog_disabled;

uniform vec4 color : source_color = vec4(0.88, 0.29, 0.23, 0.9);
// metres, whatever the model's scale
uniform float width = 0.018;

void vertex() {
	float s = max(length(MODEL_MATRIX[0].xyz), 1e-4);
	VERTEX += normalize(NORMAL) * width / s;
}

void fragment() {
	ALBEDO = color.rgb * 1.6;
	ALPHA = color.a;
}
