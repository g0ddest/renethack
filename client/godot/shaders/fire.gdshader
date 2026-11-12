// Rolling fire (vfx.rs: a blast's flames, a ray's tongues, a thing
// burning): a particle's billboard playing a flipbook of a ball of fire
// (Unity Labs, CC0) once over its life, drawn in a fire's own colours by
// the picture's heat: a deep red at its edges, orange, a small yellow
// heart, never white. It covers what is behind it (premultiplied alpha),
// so flames piled on one another stay orange where added light burned to
// cream. The particle's colour is not a colour: r is how hot it still is
// (1 at birth, cooling), g how much of it has turned to soot (dark smoke
// rolling off it), a how thick it is.
shader_type spatial;
render_mode unshaded, blend_premul_alpha, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform sampler2D book : source_color, filter_linear_mipmap, repeat_disable;
uniform vec2 frames = vec2(8.0, 8.0);
uniform float energy = 1.0;

void vertex() {
	// a particle's billboard, turned by its angle, its scale kept
	mat4 world = mat4(normalize(INV_VIEW_MATRIX[0]), normalize(INV_VIEW_MATRIX[1]),
		normalize(INV_VIEW_MATRIX[2]), MODEL_MATRIX[3]);
	float c = cos(INSTANCE_CUSTOM.x);
	float s = sin(INSTANCE_CUSTOM.x);
	world = world * mat4(vec4(c, -s, 0.0, 0.0), vec4(s, c, 0.0, 0.0),
		vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
	MODELVIEW_MATRIX = VIEW_MATRIX * world;
	MODELVIEW_MATRIX = MODELVIEW_MATRIX * mat4(
		vec4(length(MODEL_MATRIX[0].xyz), 0.0, 0.0, 0.0),
		vec4(0.0, length(MODEL_MATRIX[1].xyz), 0.0, 0.0),
		vec4(0.0, 0.0, length(MODEL_MATRIX[2].xyz), 0.0),
		vec4(0.0, 0.0, 0.0, 1.0));
	MODELVIEW_NORMAL_MATRIX = mat3(MODELVIEW_MATRIX);
	// its page of the flipbook, once through
	float total = frames.x * frames.y;
	float page = clamp(floor(INSTANCE_CUSTOM.z * total), 0.0, total - 1.0);
	UV /= frames;
	UV += vec2(mod(page, frames.x) / frames.x, floor((page + 0.5) / frames.x) / frames.y);
}

void fragment() {
	vec3 c = texture(book, UV).rgb;
	// (the picture's red is full over most of a ball: its heat is in its
	// green)
	float heat = clamp(sqrt(c.g) * 1.5, 0.0, 1.0);
	float hot = COLOR.r;
	float soot = COLOR.g;
	float thick = COLOR.a;
	float body = smoothstep(0.03, 0.45, c.r);
	float h = heat * (0.3 + 0.9 * hot);
	vec3 fire = mix(vec3(0.28, 0.012, 0.0), vec3(0.75, 0.13, 0.01), smoothstep(0.15, 0.5, h));
	fire = mix(fire, vec3(1.0, 0.36, 0.04), smoothstep(0.5, 0.9, h));
	fire = mix(fire, vec3(1.0, 0.7, 0.22), smoothstep(1.0, 1.2, h));
	float burn = body * thick * (1.0 - soot);
	float smoke = body * thick * soot;
	ALBEDO = fire * (0.35 + 0.6 * h) * energy * burn + vec3(0.03, 0.026, 0.024) * smoke * 0.75;
	ALPHA = clamp(burn * 0.7 + smoke * 0.75, 0.0, 1.0);
}
