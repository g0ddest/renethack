// The light a pleased god sends down on a praying hero (hero_pose.rs): a
// shaft that glows over what is behind it. Soft at its sides (a face seen
// edge-on fades: light in dusty air has no rim) and fading out upwards,
// brightest at the hero's feet. `strength` is how far the answer has come
// (0..1) times the shaft's own weight.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform vec4 color : source_color = vec4(1.0, 0.93, 0.72, 1.0);
uniform float strength = 0.0;
// the mesh's height, its middle at its origin
uniform float height = 7.0;

varying float rise;

void vertex() {
	rise = VERTEX.y / height + 0.5;
}

void fragment() {
	// how squarely the face looks at the camera, round the shaft only (the
	// camera looks down at it from above)
	vec3 n = (INV_VIEW_MATRIX * vec4(NORMAL, 0.0)).xyz;
	vec3 v = (INV_VIEW_MATRIX * vec4(VIEW, 0.0)).xyz;
	n.y = 0.0;
	v.y = 0.0;
	float facing = abs(dot(normalize(n + vec3(1e-4)), normalize(v + vec3(1e-4))));
	float soft = facing * facing;
	float fade = smoothstep(0.0, 0.03, rise) * (1.0 - smoothstep(0.06, 0.42, rise));
	ALBEDO = color.rgb;
	ALPHA = clamp(strength * soft * fade, 0.0, 1.0);
}
