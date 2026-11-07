// A blow's flash on the one it struck (map_view.rs: an overlay on its
// meshes for a moment, fading): the flash's colour added over it, most
// round its outline, so it reads at the camera's distance as a hit.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_back, shadows_disabled, fog_disabled;

instance uniform float flash = 0.0;
instance uniform vec3 flash_color : source_color = vec3(1.0, 0.86, 0.7);

void fragment() {
	float edge = 1.0 - clamp(dot(normalize(NORMAL), normalize(VIEW)), 0.0, 1.0);
	ALBEDO = flash_color * flash * (0.45 + 0.55 * edge * edge);
}
