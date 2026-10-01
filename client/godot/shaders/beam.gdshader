// A ray (vfx.rs): a ribbon along the ray that always faces the camera, a
// white-hot core in a halo of the ray's colour, soft at its sides and
// ends. `head` is how far it has run from its start, `fade` takes it away.
shader_type spatial;
render_mode unshaded, blend_add, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled, skip_vertex_transform;

uniform vec4 color : source_color = vec4(0.38, 0.56, 1.0, 1.0);
uniform float fade = 1.0;
uniform float head = 1.0;
// the halo's full width, metres
uniform float width = 0.42;
uniform float energy = 1.6;

void vertex() {
	// the quad's y runs along the ray (the model's y axis is the ray),
	// its x across it, turned to face the camera
	vec3 axis = MODEL_MATRIX[1].xyz;
	vec3 mid = MODEL_MATRIX[3].xyz;
	vec3 p = mid + axis * VERTEX.y;
	vec3 eye = INV_VIEW_MATRIX[3].xyz;
	vec3 side = normalize(cross(normalize(axis), normalize(eye - p)));
	p += side * VERTEX.x * width;
	VERTEX = (VIEW_MATRIX * vec4(p, 1.0)).xyz;
}

void fragment() {
	float x = abs(UV.x - 0.5) * 2.0;
	float core = exp(-pow(x / 0.12, 2.0));
	float halo = exp(-pow(x / 0.55, 2.0));
	// UV.y runs from the end (0) to the start (1)
	float from_start = 1.0 - UV.y;
	float ends = smoothstep(0.0, 0.04, from_start) * smoothstep(0.0, 0.04, UV.y);
	float shown = 1.0 - smoothstep(head - 0.03, head, from_start);
	float a = fade * ends * shown;
	vec3 c = color.rgb * halo * 0.9 + mix(color.rgb, vec3(1.0), 0.5) * core * 1.4;
	ALBEDO = c * energy * a;
	ALPHA = clamp((halo * 0.6 + core) * a, 0.0, 1.0);
}
