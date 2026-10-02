// The air shimmering over lava (map_view.rs): a quad standing over the
// cell, bending what is behind it a little, never drawing a colour.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform sampler2D screen_tex : hint_screen_texture, filter_linear_mipmap;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
uniform float strength = 0.006;

void vertex() {
	// a flat plane (x, z) stood up into a quad (x, y), its foot on the lava
	VERTEX = vec3(VERTEX.x, 0.6 - VERTEX.z, 0.0);
	UV = vec2(UV.x, UV.y);
	// faces the camera about its upright axis
	vec3 side = normalize(cross(vec3(0.0, 1.0, 0.0), INV_VIEW_MATRIX[2].xyz));
	vec3 front = cross(side, vec3(0.0, 1.0, 0.0));
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(vec4(side, 0.0), vec4(0.0, 1.0, 0.0, 0.0),
		vec4(front, 0.0), MODEL_MATRIX[3]);
}

void fragment() {
	vec2 p = UV * vec2(1.0, 2.0) + vec2(0.0, -TIME * 0.4);
	vec2 off = vec2(texture(noise_tex, p).r, texture(noise_tex, p + 0.37).r) - 0.5;
	// strongest low, fading up and at the sides
	float fade = (1.0 - UV.y) * UV.y * 4.0 * (1.0 - abs(UV.x - 0.5) * 2.0);
	ALBEDO = texture(screen_tex, SCREEN_UV + off * strength * fade).rgb;
	ALPHA = clamp(fade * 2.0, 0.0, 1.0);
}
