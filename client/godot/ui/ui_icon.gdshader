// Emblems drawn from distance fields until the icon set (phase I) exists:
// each a solid shape struck in gold on transparent, its edges rounded and
// lit from the top left, its details cut into it as grooves, a dark rim
// round it and a shadow under it, down and to the right, so it reads over
// the world. `icon` picks the emblem:
// 0 bag (inventory), 1 book (spells), 2 bust (character), 3 map
// (overview), 4 scroll (message history), 5 shield (AC), 6 coin (gold),
// 7 gear (settings), 8 medal (achievements), 9 question mark (help),
// 10 a list (every command).
shader_type canvas_item;
render_mode unshaded;

uniform int icon = 0;
uniform vec4 color : source_color = vec4(0.72, 0.54, 0.23, 1.0);
uniform vec4 shade : source_color = vec4(0.0, 0.0, 0.0, 0.7);
// a groove's width, and the rounded edge's, in the emblem's units (it
// spans -1..1)
uniform float groove = 0.075;
uniform float bevel = 0.16;

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

// a groove along a field's zero line
float cut(float d) {
	return abs(d) - groove * 0.5;
}

// a groove along a segment, `k` grooves wide
float cut_line(vec2 p, vec2 a, vec2 b, float k) {
	return sd_segment(p, a, b) - groove * 0.5 * k;
}

// a five-pointed star, `r` to its points
float sd_star(vec2 p, float r) {
	float a = atan(p.x, -p.y);
	float seg = 6.2831853 / 5.0;
	float f = abs(mod(a + seg * 0.5, seg) - seg * 0.5);
	float k = mix(1.0, 0.45, smoothstep(0.0, seg * 0.5, f));
	return length(p) - r * k;
}

// The emblem at `p`: x its solid shape's distance (negative inside), y
// its grooves' (negative in one).
vec2 emblem(vec2 p) {
	float d = 1e3;
	float g = 1e3;
	if (icon == 0) {
		// a sack: a full body narrowing into its neck, the cloth above
		// the cord splayed in a ruffle, the cord's end hanging
		float body = sd_circle(p - vec2(0.0, 0.26), 0.54);
		float shoulder = sd_circle(p - vec2(0.0, -0.08), 0.3);
		float k = 0.22;
		float hb = clamp(0.5 + 0.5 * (shoulder - body) / k, 0.0, 1.0);
		d = mix(shoulder, body, hb) - k * hb * (1.0 - hb);
		d = min(d, sd_box(p - vec2(0.0, -0.4), vec2(0.13, 0.1), 0.02));
		// the ruffle: wider than the neck, scalloped along its top
		vec2 f = p - vec2(0.0, -0.58);
		float fan = sd_box(f, vec2(0.3 - f.y * 0.5, 0.1), 0.05);
		fan = max(fan, -sd_circle(vec2(abs(f.x) - 0.15, f.y + 0.2), 0.08));
		d = min(d, fan);
		g = cut_line(p, vec2(-0.16, -0.44), vec2(0.16, -0.44), 1.0);
		d = min(d, sd_segment(p, vec2(0.12, -0.44), vec2(0.36, -0.2)) - 0.055);
		// a fold down the cloth
		g = min(g, cut_line(p, vec2(-0.12, -0.24), vec2(-0.22, 0.3), 0.7));
	} else if (icon == 1) {
		// a closed book: its cover, the spine, a star on the cover
		d = sd_box(p - vec2(0.04, 0.0), vec2(0.46, 0.64), 0.06);
		g = cut_line(p, vec2(-0.26, -0.62), vec2(-0.26, 0.62), 1.0);
		g = min(g, cut(sd_star(p - vec2(0.12, -0.04), 0.24)));
		// the pages' edge along the foot
		g = min(g, cut_line(p, vec2(-0.2, 0.5), vec2(0.46, 0.5), 0.6));
	} else if (icon == 2) {
		// a bust: a head and shoulders, a collar between
		d = sd_circle(p - vec2(0.0, -0.32), 0.27);
		vec2 q = p - vec2(0.0, 0.66);
		float shoulders = max(sd_circle(q * vec2(0.85, 1.0), 0.55), q.y);
		d = min(d, shoulders);
		// the collar: an arc round the neck, a line down the front
		float collar = cut(sd_circle(p - vec2(0.0, -0.32), 0.33));
		g = p.y > 0.05 ? collar : 1e3;
		g = min(g, cut_line(p, vec2(0.0, 0.22), vec2(0.0, 0.62), 0.8));
	} else if (icon == 3) {
		// a folded map: three panels, a path to a cross
		vec2 q = p;
		// the panels zigzag a little
		q.y += (abs(fract(p.x * 1.25 + 0.5) - 0.5) - 0.25) * 0.12;
		d = sd_box(q, vec2(0.66, 0.5), 0.03);
		g = cut_line(p, vec2(-0.22, -0.56), vec2(-0.22, 0.56), 0.8);
		g = min(g, cut_line(p, vec2(0.22, -0.56), vec2(0.22, 0.56), 0.8));
		float path = sd_segment(p, vec2(-0.48, 0.3), vec2(-0.1, 0.08));
		path = min(path, sd_segment(p, vec2(-0.1, 0.08), vec2(0.3, -0.16)));
		g = min(g, path - groove * 0.35);
		g = min(g, cut_line(p, vec2(0.32, -0.34), vec2(0.5, -0.16), 0.9));
		g = min(g, cut_line(p, vec2(0.5, -0.34), vec2(0.32, -0.16), 0.9));
	} else if (icon == 4) {
		// a scroll: a sheet, its two rolls, lines of text
		d = sd_box(p, vec2(0.42, 0.5), 0.03);
		float roll_top = sd_box(p - vec2(0.0, -0.56), vec2(0.54, 0.11), 0.11);
		float roll_foot = sd_box(p - vec2(0.0, 0.56), vec2(0.54, 0.11), 0.11);
		d = min(d, min(roll_top, roll_foot));
		g = cut(roll_top + 0.0);
		g = min(g, cut(roll_foot));
		for (int i = 0; i < 3; i++) {
			float y = -0.2 + float(i) * 0.2;
			g = min(g, cut_line(p, vec2(-0.24, y), vec2(i == 2 ? 0.06 : 0.24, y), 0.7));
		}
	} else if (icon == 5) {
		// a heater shield, a border round its face
		vec2 q = vec2(abs(p.x), p.y) * 0.88;
		float top = -q.y - 0.62;
		float side = q.x - 0.56;
		float tip = length(q - vec2(-0.56, -0.24)) - 1.1;
		d = max(max(top, side), max(tip, q.y - 0.82));
		g = cut(d + 0.13);
	} else if (icon == 8) {
		// a medal on a ribbon: the ribbon's two strips, a disc, a star
		d = sd_segment(p, vec2(-0.32, -0.7), vec2(-0.06, -0.2)) - 0.12;
		d = min(d, sd_segment(p, vec2(0.32, -0.7), vec2(0.06, -0.2)) - 0.12);
		vec2 m = p - vec2(0.0, 0.26);
		d = min(d, sd_circle(m, 0.44));
		g = cut(sd_circle(m, 0.32));
		g = min(g, cut(sd_star(m, 0.2)) - groove * 0.1);
	} else if (icon == 7) {
		// a gear: a toothed wheel round a hole
		float a = atan(p.y, p.x);
		float teeth = 0.5 + 0.15 * smoothstep(-0.3, 0.3, cos(a * 8.0));
		d = max(length(p) - teeth, -sd_circle(p, 0.2));
		g = cut(sd_circle(p, 0.36));
	} else if (icon == 9) {
		// a question mark: the hook round from the left over the top to
		// the right, down to the stem; a dot under it
		vec2 c = vec2(0.0, -0.28);
		vec2 q = p - c;
		float a = atan(q.y, q.x);
		float w = 0.11;
		float hook = a > 0.7 && a < 2.9 ? 1e3 : abs(length(q) - 0.3) - w;
		vec2 end = c + 0.3 * vec2(cos(0.7), sin(0.7));
		d = min(hook, sd_segment(p, end, vec2(0.0, 0.2)) - w);
		d = min(d, sd_circle(p - vec2(0.0, 0.52), 0.13));
	} else if (icon == 10) {
		// a list: three bars, a stud before each
		for (int i = 0; i < 3; i++) {
			float y = -0.46 + float(i) * 0.46;
			d = min(d, sd_circle(p - vec2(-0.5, y), 0.13));
			d = min(d, sd_box(p - vec2(0.18, y), vec2(0.42, 0.1), 0.06));
		}
	} else {
		// a coin: a disc, its rim raised round the face
		d = sd_circle(p, 0.68);
		g = cut(sd_circle(p, 0.5));
	}
	return vec2(d, g);
}

// The relief's height at `p`: rising over the rounded edge, sunk in the
// grooves; and how much of the pixel the shape covers, from `e`.
float height(vec2 p) {
	vec2 e = emblem(p);
	float rise = smoothstep(0.0, bevel, -e.x);
	float sunk = 1.0 - smoothstep(0.0, groove * 0.6, e.y);
	return rise * (1.0 - 0.7 * sunk);
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = UV * 2.0 - 1.0;
	float px = fwidth(p.x);
	vec2 e = emblem(p);
	float a = 1.0 - smoothstep(-px, px, e.x);
	// the relief's normal, from its height round the pixel
	float s = max(px, 0.01);
	float h0 = height(p);
	float hx = height(p + vec2(s, 0.0));
	float hy = height(p + vec2(0.0, s));
	vec3 n = normalize(vec3(-(hx - h0) / s * bevel * 0.9, -(hy - h0) / s * bevel * 0.9, 1.0));
	vec3 l = normalize(vec3(-0.55, -0.7, 0.75));
	float diff = clamp(dot(n, l), 0.0, 1.0);
	float spec = pow(clamp(dot(reflect(-l, n), vec3(0.0, 0.0, 1.0)), 0.0, 1.0), 12.0);
	// gold, brighter at the top than the foot, darker in the grooves
	vec3 gold = color.rgb * mix(1.2, 0.8, UV.y);
	float sunk = 1.0 - smoothstep(-px, px, e.y);
	// old gold, dulled: a little grey, its highlight faint
	gold = mix(gold, vec3(dot(gold, vec3(0.3, 0.59, 0.11))), 0.25);
	vec3 metal = gold * (0.3 + 0.9 * diff) + vec3(0.9, 0.8, 0.62) * spec * 0.18;
	metal = mix(metal, gold * 0.32 * (0.6 + 0.6 * diff), sunk * 0.85);
	// a dark rim round the shape, and its shadow down and to the right
	float rim = 1.0 - smoothstep(0.0, px * 1.5 + 0.05, e.x);
	float ds = emblem(p - vec2(0.06, 0.08)).x;
	float drop = 1.0 - smoothstep(0.0, px * 2.0 + 0.1, ds);
	float sh = max(rim, drop * 0.8) * (1.0 - a);
	vec4 c = vec4(metal * color.a * a, color.a * a);
	vec4 o = vec4(shade.rgb * shade.a * sh, shade.a * sh);
	// premultiplied: the emblem over its rim and shadow
	vec4 pm = c + o * (1.0 - c.a);
	COLOR = vec4(pm.rgb / max(pm.a, 1e-4), pm.a) * tint;
}
