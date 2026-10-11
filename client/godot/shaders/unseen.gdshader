// Something unseen remembered on a cell (map_view.rs, NetHack's `I`): the
// air there bent as if a body stood in it, a rounded column of it with a
// faint cold shimmer running round its edge. No shape of a creature (the
// hero does not know what it is), and no letter floating in the dark.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

uniform sampler2D screen_tex : hint_screen_texture, filter_linear_mipmap;
uniform sampler2D noise_tex : filter_linear_mipmap, repeat_enable;
// how far what is behind it is bent, and its edge's colour
uniform float strength = 0.02;
uniform vec4 edge_color : source_color = vec4(0.62, 0.72, 1.0, 1.0);
// the quad's size (the mesh: a plane 0.9 wide and 1.6 long, stood up)
const vec2 SIZE = vec2(0.9, 1.6);
// the column: its middle's height, its half height and half width
const vec3 COLUMN = vec3(0.62, 0.55, 0.27);

void vertex() {
	// a flat plane (x, z) stood up into a quad (x, y), its foot on the ground
	VERTEX = vec3(VERTEX.x, SIZE.y / 2.0 - VERTEX.z, 0.0);
	// facing the camera about its upright axis
	vec3 side = normalize(cross(vec3(0.0, 1.0, 0.0), INV_VIEW_MATRIX[2].xyz));
	vec3 front = cross(side, vec3(0.0, 1.0, 0.0));
	MODELVIEW_MATRIX = VIEW_MATRIX * mat4(vec4(side, 0.0), vec4(0.0, 1.0, 0.0, 0.0),
		vec4(front, 0.0), MODEL_MATRIX[3]);
}

void fragment() {
	// metres on the quad: x across, y up from the ground
	vec2 p = vec2((UV.x - 0.5) * SIZE.x, (1.0 - UV.y) * SIZE.y);
	float t = TIME + dot(NODE_POSITION_WORLD.xz, vec2(1.7, 3.1));
	// a rounded column, its outline wavering as the air does
	vec2 q = (p - vec2(0.0, COLUMN.x)) / COLUMN.zy;
	float wobble = texture(noise_tex, vec2(p.x * 0.6, p.y * 0.5 - t * 0.12)).r - 0.5;
	float d = (length(q) - 1.0) * COLUMN.z + wobble * 0.05;
	float inside = smoothstep(0.015, -0.03, d);
	// the edge: a thin band, brighter where the shimmer runs
	float run = texture(noise_tex, vec2(atan(q.x, q.y) * 0.4 + t * 0.05, t * 0.08)).r;
	float edge = exp(-abs(d) / 0.012) * mix(0.35, 1.0, smoothstep(0.35, 0.7, run));
	// the air in it bent, slowly and finer and faster over that
	vec2 a = p * vec2(1.4, 0.9) + vec2(0.0, -t * 0.25);
	vec2 off = vec2(texture(noise_tex, a).r, texture(noise_tex, a + 0.37).r) - 0.5;
	vec2 b = p * vec2(3.5, 2.5) + vec2(0.0, -t * 0.8);
	off += (vec2(texture(noise_tex, b).r, texture(noise_tex, b + 0.61).r) - 0.5) * 0.5;
	vec3 behind = texture(screen_tex, SCREEN_UV + off * strength * inside).rgb;
	// it never quite vanishes: a breath of cold light in it, more at its edge
	float breathe = 0.75 + 0.25 * sin(t * 1.7);
	ALBEDO = behind + edge_color.rgb * (edge * 0.22 + inside * 0.012) * breathe;
	ALPHA = clamp(max(inside, edge), 0.0, 1.0);
}
