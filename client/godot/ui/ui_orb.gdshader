// A liquid orb (ui-design §1.3, §6.4): HP or Pw in a glass sphere set in a
// bronze ring. `fill` is the level (0..1 of the height, animated from
// Rust), `ghost` the level before a recent loss, shown pale while it
// drains; `flash` whitens the liquid for a moment after a hit, `pulse`
// makes it throb (low HP).
//
// The liquid is a lit volume in a ball: brightest in its heart and just
// under its surface, darker to the glass and at the bottom, churning
// slowly in veins of light, a few bubbles rising. Its surface sways, and
// is seen a little from above: an ellipse of lighter liquid, a bright lip
// along its front. The glass is dark where it is empty, with a rim of
// light round its edge, an arc and a window of light at the top left and
// the liquid's glow caught low on the right. The ring is cast bronze, a
// bead along it and eight rivets, lit on its inner edge by the liquid;
// round it a soft glow of the liquid's colour, throbbing with `pulse`.
// The rect is `reach` times the orb's diameter: the glow needs the room.
shader_type canvas_item;
render_mode unshaded;

uniform float fill : hint_range(0.0, 1.0) = 1.0;
uniform float ghost : hint_range(0.0, 1.0) = 0.0;
uniform float flash : hint_range(0.0, 1.0) = 0.0;
uniform float pulse : hint_range(0.0, 1.0) = 0.0;
uniform vec4 deep : source_color = vec4(0.541, 0.059, 0.071, 1.0);
uniform vec4 bright : source_color = vec4(0.847, 0.149, 0.118, 1.0);
// the liquid disc's radius as a share of the orb's (120 of 148)
uniform float inner = 0.81;
uniform vec4 ring_dark : source_color = vec4(0.16, 0.11, 0.06, 1.0);
uniform vec4 ring_light : source_color = vec4(0.72, 0.55, 0.3, 1.0);
// the rect's side over the orb's: the glow drawn round the ring
uniform float reach = 1.16;
// the glow's strength (a full orb, no pulse)
uniform float glow = 0.4;

float hash(vec2 p) {
	return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

float noise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	vec2 u = f * f * (3.0 - 2.0 * f);
	return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
		mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

float fbm(vec2 p) {
	float v = 0.0;
	float a = 0.5;
	for (int i = 0; i < 4; i++) {
		v += a * noise(p);
		p = p * 2.03 + vec2(1.7, 9.2);
		a *= 0.5;
	}
	return v;
}

void fragment() {
	// the vertex colour carries modulate and self_modulate
	vec4 tint = COLOR;
	vec2 p = (UV * 2.0 - 1.0) * reach;
	float r = length(p);
	float aa = fwidth(r) * 1.2;
	vec2 key = normalize(vec2(-0.6, -0.8));
	vec2 rn = normalize(p + 1e-5);
	float facing = dot(rn, key);
	float live = clamp(fill * 1.2, 0.0, 1.0);

	// ---- outside the ring: a soft glow, a shadow under it ----
	float out_t = clamp((r - 1.0) / max(reach - 1.0, 1e-3), 0.0, 1.0);
	float halo = pow(1.0 - out_t, 2.2) * glow * (0.35 + 0.65 * live) * (1.0 + pulse * 1.4);
	float shadow = pow(1.0 - out_t, 1.6) * 0.55 * smoothstep(-0.6, 0.9, p.y / max(r, 1e-3));
	vec3 halo_c = mix(bright.rgb, vec3(1.0), 0.15) * (1.0 + pulse * 0.3);
	float a_out = clamp(halo + shadow * (1.0 - halo), 0.0, 1.0);
	vec3 c_out = halo_c * halo / max(a_out, 1e-3);

	// ---- the bronze ring: cast, rounded, a bead and eight rivets ----
	float band = clamp((r - inner) / (1.0 - inner), 0.0, 1.0);
	// a rounded profile across it: its inner half faces in, its outer out
	float slope = 1.0 - band * 2.0;
	float lit = clamp(facing * slope * 0.85 + 0.5, 0.0, 1.0);
	vec3 ring = mix(ring_dark.rgb, ring_light.rgb, lit);
	float bead = 1.0 - smoothstep(0.0, 0.16, abs(band - 0.5));
	ring += ring_light.rgb * bead * 0.3 * (0.35 + 0.65 * clamp(facing * 0.5 + 0.5, 0.0, 1.0));
	// cast metal, mottled and a little worn
	ring *= 0.78 + 0.3 * fbm(p * 18.0);
	float ang = atan(p.y, p.x);
	float seg = 6.2831853 / 8.0;
	float a8 = mod(ang + seg * 0.5, seg) - seg * 0.5;
	float mid = (inner + 1.0) * 0.5;
	vec2 rv = vec2(a8 * mid, r - mid) / 0.034;
	float rd = length(rv);
	if (rd < 1.0) {
		float rl = clamp(dot(normalize(rv + 1e-4), key) * 0.5 + 0.5, 0.0, 1.0) * (1.0 - rd * 0.3);
		vec3 stud = mix(ring_dark.rgb * 0.7, ring_light.rgb * 1.4, rl);
		ring = mix(ring, stud, smoothstep(1.0, 0.7, rd));
	}
	// dark hairlines on both edges of the ring
	ring *= mix(0.2, 1.0, smoothstep(0.0, 0.03, 1.0 - r));
	ring *= mix(0.15, 1.0, smoothstep(0.0, 0.03, r - inner));
	// the liquid's light on the ring's inner edge
	float edge_glow = (1.0 - smoothstep(inner, inner + 0.07, r)) * step(inner, r);
	ring += bright.rgb * edge_glow * 0.4 * live * (1.0 + pulse * 0.6);

	// ---- the liquid in its glass ball ----
	vec2 q = p / inner;
	float rr = length(q);
	float z = sqrt(max(1.0 - rr * rr, 0.0));
	// the surface's height in q (y down): 1 empty, -1 full; swaying a
	// little and waving
	float full = step(0.002, fill) * step(fill, 0.998);
	float wave = (noise(vec2(q.x * 2.6 + TIME * 0.7, TIME * 0.45)) - 0.5) * 0.06
		+ sin(q.x * 5.0 + TIME * 1.6) * 0.012
		+ q.x * sin(TIME * 0.8) * 0.035;
	float ys = 1.0 - 2.0 * fill + wave * full;
	// the surface seen a little from above: an ellipse as wide as the
	// ball's section there, `tilt` as tall
	float rs = sqrt(max(1.0 - ys * ys, 0.0));
	float tilt = 0.16;
	vec2 ev = vec2(q.x / max(rs, 1e-3), (q.y - ys) / max(rs * tilt, 1e-3));
	float e = length(ev);
	float px = fwidth(q.y) * 1.5;
	// on the top face; under the front lip; in the body (below the lip)
	float face_d = (e - 1.0) * rs * tilt;
	float on_face = (1.0 - smoothstep(-px, px, face_d)) * full;
	float below = smoothstep(-px, px, q.y - ys);
	float in_body = max(below * (1.0 - on_face), step(0.998, fill) * 1.0);
	in_body *= step(0.002, fill);
	float in_liquid = clamp(in_body + on_face, 0.0, 1.0);

	// the volume churning slowly: two flows warping each other, wrapped
	// round the ball
	vec2 sq = q / (0.55 + 0.45 * z);
	vec2 flow = sq * 1.6;
	vec2 warp = vec2(
		fbm(flow + vec2(TIME * 0.09, -TIME * 0.14)),
		fbm(flow + vec2(-TIME * 0.11, TIME * 0.06) + 5.2));
	float swirl = fbm(flow * 1.2 + warp * 1.6 + vec2(0.0, -TIME * 0.16));
	// faint veins of light where the flows fold
	float vein = pow(1.0 - abs(swirl * 2.0 - 1.0), 4.0);
	// lit from within: brightest in its heart, darker to the glass, in
	// soft clouds
	float heart = pow(z, 0.7);
	float cloud = smoothstep(0.25, 0.75, warp.x * 0.5 + swirl * 0.5);
	float dens = clamp(heart * 0.62 + (cloud - 0.5) * 0.55, 0.0, 1.0);
	vec3 liquid = mix(deep.rgb * 0.28, bright.rgb, dens);
	liquid += mix(bright.rgb, vec3(1.0), 0.2) * vein * 0.22 * (0.3 + 0.7 * heart);
	// darker in the deep, lighter just under the surface
	float under = clamp((q.y - ys) / max(2.0 * fill, 0.05), 0.0, 1.0);
	liquid *= mix(1.25, 0.6, under);
	// bubbles rising
	vec2 bq = vec2(q.x * 7.0, q.y * 7.0 + TIME * 1.6);
	vec2 bc = floor(bq);
	vec2 bf = fract(bq) - 0.5 - (vec2(hash(bc), hash(bc + 3.1)) - 0.5) * 0.6;
	float bub = step(0.92, hash(bc + 7.7)) * (1.0 - smoothstep(0.04, 0.1, length(bf)));
	liquid += vec3(1.0, 0.95, 0.9) * bub * 0.3 * heart * below;
	// the top face: lighter, a sheen moving across it
	vec3 top = bright.rgb * (0.75 + 0.35 * noise(vec2(q.x * 4.0 - TIME * 0.5, ev.y * 2.0)))
		+ vec3(1.0) * 0.06;
	top *= mix(1.15, 0.8, clamp(ev.y * 0.5 + 0.5, 0.0, 1.0));
	liquid = mix(liquid, top, on_face * 0.85);
	// the lip: a bright line along the face's front, a fainter one behind
	float lip_d = abs(face_d);
	float front = step(ys, q.y);
	float lip = (1.0 - smoothstep(0.0, 0.025, lip_d)) * full * (0.45 + 0.55 * front);
	liquid = mix(liquid, mix(bright.rgb, vec3(1.0), 0.45) * 1.3, lip * 0.8);
	liquid = mix(liquid, vec3(1.0), flash * 0.6);
	liquid *= 1.0 + pulse * 0.45;

	// the empty glass: dark, faintly tinted by what it holds, lighter to
	// the top left
	vec3 col = vec3(0.02, 0.018, 0.02) + deep.rgb * 0.06;
	col += vec3(0.05, 0.05, 0.055) * clamp(dot(q, key) * 0.5 + 0.2, 0.0, 1.0) * (1.0 - z * 0.5);
	// the ghost of a loss: pale, from the surface up to where it was
	float gys = 1.0 - 2.0 * ghost;
	float in_ghost = smoothstep(-px, px, q.y - gys) * (1.0 - in_liquid) * step(0.002, ghost);
	col = mix(col, mix(bright.rgb, vec3(1.0), 0.3) * 0.6, in_ghost * 0.45);
	col = mix(col, liquid, in_liquid);

	// glass: darker to its edge, the liquid's glow showing through
	col *= mix(1.0, 0.45, smoothstep(0.6, 1.0, rr));
	col += bright.rgb * in_liquid * pow(1.0 - rr, 2.0) * 0.2;
	// a rim of light round the glass's edge, brightest at the top left
	float rim = smoothstep(0.82, 0.97, rr) * (1.0 - smoothstep(0.97, 1.0, rr));
	col += vec3(0.9, 0.88, 0.85) * rim * 0.16 * (0.35 + 0.65 * clamp(dot(normalize(q + 1e-5), key), 0.0, 1.0));
	// an arc of light along the top left, inside the rim
	float arc_r = smoothstep(0.6, 0.78, rr) * (1.0 - smoothstep(0.84, 0.93, rr));
	float arc_a = pow(clamp(dot(normalize(q + 1e-5), key), 0.0, 1.0), 5.0);
	col += vec3(1.0, 0.97, 0.92) * arc_r * arc_a * 0.45;
	// a window of light at the top left, a sharp glint in it
	vec2 wq = q - vec2(-0.3, -0.46);
	wq = vec2(wq.x * 0.8 + wq.y * 0.6, -wq.x * 0.6 + wq.y * 0.8);
	float window = 1.0 - smoothstep(0.1, 0.24, length(wq * vec2(0.8, 1.9)));
	col += vec3(1.0, 0.96, 0.9) * window * 0.3;
	col += vec3(1.0) * (1.0 - smoothstep(0.02, 0.075, length(q - vec2(-0.38, -0.5)))) * 0.55;
	// the liquid's light caught low on the right
	float low = 1.0 - smoothstep(0.0, 0.22, length((q - vec2(0.36, 0.6)) * vec2(1.0, 2.0)));
	col += bright.rgb * low * 0.14 * (0.3 + 0.7 * live) + vec3(1.0, 0.9, 0.8) * low * 0.04;

	float disc = 1.0 - smoothstep(inner - aa, inner, r);
	vec3 orb = mix(ring, col, disc);
	float in_orb = 1.0 - smoothstep(1.0 - aa, 1.0, r);
	vec4 c = mix(vec4(c_out, a_out), vec4(orb, 1.0), in_orb);
	if (c.a <= 0.001) {
		discard;
	}
	COLOR = c * tint;
}
