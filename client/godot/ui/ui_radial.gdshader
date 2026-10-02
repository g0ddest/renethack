// The gamepad's radial menu: a dark translucent ring of `count` sectors
// (the first at the top, clockwise), the selected one lit gold, thin
// bronze separators and rims, and a dark disc in the middle for the name
// of the action.
shader_type canvas_item;
render_mode unshaded;

uniform int count = 8;
uniform int selected = -1;
// radii as a share of the half size
uniform float inner = 0.44;
uniform float outer = 0.97;
uniform vec4 base_color : source_color = vec4(0.07, 0.055, 0.04, 0.84);
uniform vec4 disc_color : source_color = vec4(0.045, 0.035, 0.028, 0.9);
uniform vec4 rim_color : source_color = vec4(0.43, 0.33, 0.16, 1.0);
uniform vec4 gold : source_color = vec4(0.906, 0.761, 0.478, 1.0);

void fragment() {
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float aa = fwidth(r) * 1.2;
	// clockwise from the top
	float a = atan(p.x, -p.y);
	if (a < 0.0) {
		a += 6.2831853;
	}
	float seg = 6.2831853 / float(count);
	float shifted = mod(a + seg * 0.5, 6.2831853);
	int idx = int(floor(shifted / seg));
	float local = shifted - (float(idx) + 0.5) * seg;
	// distance to the nearest separator, along the arc
	float sep_d = (seg * 0.5 - abs(local)) * r;

	float in_ring = smoothstep(outer + aa, outer - aa, r) * smoothstep(inner - aa, inner + aa, r);
	float in_disc = smoothstep(inner - 0.04 + aa, inner - 0.04 - aa, r);

	bool on = idx == selected;
	vec3 c = base_color.rgb;
	float alpha = base_color.a;
	if (on) {
		// gold rising toward the outer edge, along the stick
		float t = clamp((r - inner) / (outer - inner), 0.0, 1.0);
		c = mix(gold.rgb * 0.35, gold.rgb * 0.75, t);
		alpha = 0.94;
	}
	float sep = 1.0 - smoothstep(0.006, 0.006 + aa, sep_d);
	float rim_o = 1.0 - smoothstep(0.012, 0.012 + aa, abs(r - outer + 0.012));
	float rim_i = 1.0 - smoothstep(0.01, 0.01 + aa, abs(r - inner - 0.008));
	vec3 rim = on ? gold.rgb : rim_color.rgb;
	c = mix(c, rim_color.rgb, sep * 0.9);
	c = mix(c, rim, max(rim_o, rim_i));

	vec4 ring = vec4(c, alpha * in_ring);
	// the disc, with its own thin rim
	float disc_rim = 1.0 - smoothstep(0.01, 0.01 + aa, abs(r - inner + 0.05));
	vec4 disc = vec4(mix(disc_color.rgb, rim_color.rgb, disc_rim), disc_color.a * in_disc);
	COLOR = ring.a > 0.0 ? ring : disc;
}
