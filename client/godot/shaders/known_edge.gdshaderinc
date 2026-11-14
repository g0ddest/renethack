// The known ground's edge where nothing is drawn beside it (past what the
// hero has seen, a floor NetHack leaves unmapped out of sight): the same
// in each shader of the ground (map_surface's floors and ice, water,
// lava), so all of them end alike there, raggedly and round, never in a
// line along the cells. Nothing is drawn past what is known: the edge
// eats into the known cell.

// how round the known's corners are: the smooth least's sharpness (lower:
// rounder, a straight edge wavier along the cells, and a strip of the known
// a cell wide thinner: at 5 a path walked through a dark room was eaten
// away from both its sides)
const float KNOWN_ROUND = 8.0;
// how far into the known a point lies when nothing unknown is near
const float KNOWN_FAR = 9.0;

// How far into what is known a point of the ground lies (cells): its
// distance from the cells nothing is drawn on about it (surface.rs
// StandField: G full), a smooth least of them, so a corner of the known
// rounds off instead of meeting the unknown in a square step. Past the
// level's field nothing is drawn either. KNOWN_FAR where neither the
// point's cell nor one beside it is such a cell (B clear).
float known_reach(sampler2D stand, vec2 p) {
	ivec2 c = ivec2(floor(p + 0.5));
	bool inside = c.x >= 0 && c.y >= 0 && c.x < 80 && c.y < 21;
	if (inside && texelFetch(stand, c, 0).b < 0.5) {
		return KNOWN_FAR;
	}
	float sum = 0.0;
	for (int j = -1; j <= 1; j++) {
		for (int i = -1; i <= 1; i++) {
			ivec2 q = c + ivec2(i, j);
			bool out_of_field = q.x < 0 || q.y < 0 || q.x >= 80 || q.y >= 21;
			if (!out_of_field && texelFetch(stand, q, 0).g < 0.5) {
				continue;
			}
			vec2 dq = max(abs(p - vec2(q)) - 0.5, vec2(0.0));
			sum += exp(-KNOWN_ROUND * length(dq));
		}
	}
	return sum > 0.0 ? -log(sum) / KNOWN_ROUND : KNOWN_FAR;
}

// How far the edge eats into the known at a point (cells): a line
// wandering over a couple of metres, broken every half a cell or so and
// torn finer over that; 0.02 to about a third of a cell, never out past
// the cell's own edge, never into its middle (`noise`: the map's tiling
// fractal noise, surface.rs; its values crowd about the middle, so each
// is spread out first: as they came, the line wandered a tenth of a cell
// and read straight; eating up to two thirds, a path a cell or two wide
// walked through a dark room was all but gone).
float known_bite(sampler2D noise, vec2 p) {
	float slow = smoothstep(0.3, 0.7, textureLod(noise, p * 0.05 + vec2(0.37, 0.11), 0.0).r);
	float mid = smoothstep(0.3, 0.7, textureLod(noise, p * 0.16 + vec2(0.71, 0.53), 0.0).r);
	float fine = smoothstep(0.3, 0.7, textureLod(noise, p * 0.42 + vec2(0.13, 0.89), 0.0).r);
	return 0.02 + 0.16 * slow + 0.12 * mid + 0.06 * fine;
}

// How much of the ground at a point the edge has taken: 1 gone (drawn as
// nothing), 0 whole, fading over `width` cells inside the ragged line.
float known_edge(sampler2D stand, sampler2D noise, vec2 p, float width) {
	float reach = known_reach(stand, p);
	if (reach >= KNOWN_FAR) {
		return 0.0;
	}
	float e = reach - known_bite(noise, p);
	return 1.0 - smoothstep(0.0, width, e);
}
