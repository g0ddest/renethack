// A round medallion in a bronze ring: the portrait, condition gems, the
// level plaque's jewel. `inner` is the face's radius as a share of the
// whole; `face_*` its radial gradient; `rim` tints the ring (gold, amber,
// red for deadly conditions) and `glow` lights it (pulses).
shader_type canvas_item;
render_mode unshaded;

uniform float inner = 0.9;
uniform vec4 face_center : source_color = vec4(0.16, 0.12, 0.09, 1.0);
uniform vec4 face_edge : source_color = vec4(0.03, 0.025, 0.02, 1.0);
uniform vec4 rim : source_color = vec4(0.72, 0.55, 0.3, 1.0);
uniform float glow = 0.0;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = UV * 2.0 - 1.0;
	float r = length(p);
	float aa = fwidth(r) * 1.2;
	float outer = 1.0 - smoothstep(1.0 - aa, 1.0, r);
	if (outer <= 0.0) {
		discard;
	}
	float lit = dot(normalize(p + 1e-5), vec2(-0.6, -0.8)) * 0.5 + 0.5;
	float band = smoothstep(inner, 1.0, r);
	float ridge = 1.0 - abs(band - 0.45) * 2.0;
	vec3 ring = rim.rgb * mix(0.22, 1.05, clamp(ridge * lit + 0.1, 0.0, 1.0));
	ring *= mix(0.3, 1.0, smoothstep(0.0, 0.04, 1.0 - r));
	ring = mix(ring, rim.rgb * 1.4 + 0.08, glow * 0.6);
	vec2 q = p / inner;
	vec3 face = mix(face_center.rgb, face_edge.rgb, smoothstep(0.0, 1.0, length(q - vec2(0.0, -0.25)) * 0.9));
	face += (hash(floor(UV * 256.0)) - 0.5) * 0.02;
	// inner shadow under the ring
	face *= mix(0.35, 1.0, smoothstep(0.78, 0.55, length(q)) * 0.6 + 0.4);
	face *= mix(1.0, 0.4, smoothstep(0.85, 1.0, length(q)));
	float disc = 1.0 - smoothstep(inner - aa, inner, r);
	COLOR = vec4(mix(ring, face, disc), outer) * tint;
}
