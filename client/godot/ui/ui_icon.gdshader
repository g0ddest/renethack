// Engraved glyphs drawn from distance fields until the icon set (phase I)
// exists: one stroke weight, gold on transparent. `icon` picks the glyph:
// 0 bag (inventory), 1 book (spells), 2 bust (character), 3 map
// (overview), 4 scroll (message history), 5 shield (AC), 6 coin (gold),
// 7 gear (settings), 8 medal (achievements).
shader_type canvas_item;
render_mode unshaded;

uniform int icon = 0;
uniform vec4 color : source_color = vec4(0.72, 0.54, 0.23, 1.0);
uniform vec4 shade : source_color = vec4(0.0, 0.0, 0.0, 0.7);
uniform float stroke = 0.075;

float sd_circle(vec2 p, float r) {
	return length(p) - r;
}

float sd_box(vec2 p, vec2 b, float r) {
	vec2 q = abs(p) - b + r;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

float sd_segment(vec2 p, vec2 a, vec2 b) {
	vec2 pa = p - a, ba = b - a;
	float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
	return length(pa - ba * h);
}

// Outline of a shape (its distance field), `stroke` wide.
float ring(float d) {
	return abs(d) - stroke * 0.5;
}

float line(float d) {
	return d - stroke * 0.5;
}

float glyph(vec2 p) {
	float d = 1e3;
	if (icon == 0) {
		// a pouch: round body, gathered neck, tie
		float body = sd_circle(p - vec2(0.0, 0.18), 0.5);
		float neck = sd_box(p - vec2(0.0, -0.42), vec2(0.2, 0.1), 0.05);
		d = min(ring(body), ring(neck));
		d = min(d, line(sd_segment(p, vec2(-0.28, -0.28), vec2(0.28, -0.28))));
		d = min(d, line(sd_segment(p, vec2(0.14, -0.28), vec2(0.34, -0.1))));
	} else if (icon == 1) {
		// a closed book with a star
		d = ring(sd_box(p, vec2(0.46, 0.6), 0.06));
		d = min(d, line(sd_segment(p, vec2(-0.3, -0.6), vec2(-0.3, 0.6))));
		vec2 s = p - vec2(0.08, -0.02);
		float star = 1e3;
		for (int i = 0; i < 4; i++) {
			float a = float(i) * 0.785398;
			vec2 dir = vec2(cos(a), sin(a)) * (i % 2 == 0 ? 0.22 : 0.13);
			star = min(star, sd_segment(s, -dir, dir));
		}
		d = min(d, line(star));
	} else if (icon == 2) {
		// a bust: head and shoulders
		d = ring(sd_circle(p - vec2(0.0, -0.3), 0.24));
		vec2 q = p - vec2(0.0, 0.62);
		float shoulders = max(sd_circle(q, 0.55), -q.y - 0.02);
		d = min(d, ring(max(shoulders, q.y)));
	} else if (icon == 3) {
		// a folded map: three panels, a path
		float outline = sd_box(p, vec2(0.6, 0.46), 0.02);
		d = ring(outline);
		d = min(d, line(sd_segment(p, vec2(-0.2, -0.46), vec2(-0.2, 0.46))));
		d = min(d, line(sd_segment(p, vec2(0.2, -0.46), vec2(0.2, 0.46))));
		d = min(d, line(sd_circle(p - vec2(0.38, -0.2), 0.02)));
		float path = sd_segment(p, vec2(-0.45, 0.25), vec2(-0.05, 0.05));
		path = min(path, sd_segment(p, vec2(-0.05, 0.05), vec2(0.38, -0.2)));
		d = min(d, abs(path) - stroke * 0.3);
	} else if (icon == 4) {
		// a scroll: sheet with rolled ends and text lines
		d = ring(sd_box(p, vec2(0.42, 0.52), 0.03));
		d = min(d, ring(sd_box(p - vec2(0.0, -0.56), vec2(0.52, 0.08), 0.08)));
		d = min(d, ring(sd_box(p - vec2(0.0, 0.56), vec2(0.52, 0.08), 0.08)));
		for (int i = 0; i < 3; i++) {
			float y = -0.22 + float(i) * 0.2;
			d = min(d, abs(sd_segment(p, vec2(-0.24, y), vec2(0.24, y))) - stroke * 0.3);
		}
	} else if (icon == 5) {
		// a heater shield
		vec2 q = vec2(abs(p.x), p.y);
		float top = -q.y - 0.55;
		float side = q.x - 0.52;
		float tip = length(q - vec2(-0.52, -0.2)) - 1.02;
		float shield = max(max(top, side), max(tip, q.y - 0.75));
		d = ring(shield);
		d = min(d, abs(sd_segment(p, vec2(0.0, -0.45), vec2(0.0, 0.55))) - stroke * 0.3);
	} else if (icon == 8) {
		// a medal on a ribbon: the ribbon's two strips, a disc with a star
		d = line(sd_segment(p, vec2(-0.34, -0.66), vec2(-0.08, -0.16)));
		d = min(d, line(sd_segment(p, vec2(0.34, -0.66), vec2(0.08, -0.16))));
		vec2 m = p - vec2(0.0, 0.24);
		d = min(d, ring(sd_circle(m, 0.4)));
		float star = 1e3;
		for (int i = 0; i < 5; i++) {
			float a = float(i) * 1.256637 - 1.570796;
			star = min(star, sd_segment(m, vec2(0.0), vec2(cos(a), sin(a)) * 0.22));
		}
		d = min(d, abs(star) - stroke * 0.35);
	} else if (icon == 7) {
		// a gear: a toothed rim around a hub
		float a = atan(p.y, p.x);
		float teeth = 0.44 + 0.12 * smoothstep(-0.25, 0.25, cos(a * 8.0));
		d = ring(length(p) - teeth);
		d = min(d, ring(sd_circle(p, 0.17)));
	} else {
		// a coin with a milled edge
		d = ring(sd_circle(p, 0.56));
		d = min(d, abs(sd_circle(p, 0.36)) - stroke * 0.3);
	}
	return d;
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = UV * 2.0 - 1.0;
	float px = fwidth(p.x) * 1.0;
	float d = glyph(p);
	float a = 1.0 - smoothstep(-px, px, d);
	// a dark halo under the stroke, so it reads over the world
	float h = 1.0 - smoothstep(0.0, px * 3.0 + 0.06, d);
	vec4 c = vec4(color.rgb, color.a * a);
	vec4 s = vec4(shade.rgb, shade.a * h * (1.0 - a));
	COLOR = (c + s) * tint;
}
