// What metal mirrors in a dungeon (map_view's environment, and the doll's
// and the portrait's, for their reflections only: it is never drawn and
// lights nothing, and the ground, water and lava take none of it). Nothing
// over a dungeon reflects, and every blade, coin and ring of mail read flat
// or dark. Here they mirror a vault of stone in shadow with lit ribs across
// it, lamps in it, the lit floor under it and the walls round the horizon
// in pools of torchlight: polished metal is dark where it turns to the
// shadow and bright where it turns to a rib, a lamp or the walls, and the
// bright parts slide over it as the eye moves.
//
// The camera looks north (-z) and down (47 to 60 degrees): a thing lying
// flat mirrors the vault to the north about as high, a face turned to the
// eye the floor to the south, an edge the band round the horizon.
shader_type sky;

// lit stone (the walls, the ribs) and the torches' own colour (the
// branch's torch)
uniform vec3 stone : source_color = vec3(0.8, 0.75, 0.68);
uniform vec3 lamp : source_color = vec3(1.0, 0.76, 0.5);
// how much of it the level has (the branch's `reflect`); what metal
// mirrors where it has none (linear): a dull fill as the level's ambient
// light is, or metal there would read near black
uniform float strength = 1.0;
uniform vec3 flat_fill = vec3(0.0);
// the vault in shadow, its ribs, the floor to the south, the walls round
// the horizon, the lamps
uniform float dark = 0.035;
uniform float ribs = 2.0;
uniform float floor_glow = 0.2;
uniform float band = 0.5;
uniform float energy = 5.0;

// a soft lamp at a bearing from the north and an elevation (radians);
// `sharp` the power of the cosine (100: about 9 degrees across; sharper
// lamps flicker on a polished coin or a blade that moves)
float spot(vec3 d, float bearing, float elevation, float sharp) {
	vec3 l = vec3(cos(elevation) * sin(bearing), sin(elevation), -cos(elevation) * cos(bearing));
	return pow(max(dot(d, l), 0.0), sharp);
}

void sky() {
	vec3 d = normalize(EYEDIR);
	float up = d.y;
	float north = -d.z;
	// (straight up or down has no bearing: atan(0, 0) is undefined)
	float bearing = atan(d.x, abs(d.x) + abs(d.z) > 1e-6 ? -d.z : 1.0);
	// the vault over the north, dark but for its ribs: lit lines every 15
	// degrees, so a blade's mirror passes from the dark to a rib and on as
	// the eye moves a step (the roughest metal blurs them to a glow)
	float v = smoothstep(0.1, 0.45, up) * (1.0 - smoothstep(0.8, 0.97, up))
		* smoothstep(-0.4, 0.4, north);
	float rib = pow(0.5 + 0.5 * cos(bearing * 24.0 + up * 3.0), 12.0);
	// the lit floor below, most to the south (the faces turned to the eye)
	float f = (1.0 - smoothstep(-0.5, -0.05, up)) * (0.35 + 0.65 * smoothstep(-0.3, 0.6, d.z));
	// the walls round the horizon, in pools of torchlight: edges catch it
	float t = (up - 0.06) / 0.14;
	float b = exp(-t * t)
		* (0.45 + 0.55 * pow(0.5 + 0.5 * cos(bearing * 9.0 + 0.7), 3.0));
	vec3 c = stone * (dark + ribs * rib * v + band * b)
		+ mix(stone, lamp, 0.5) * floor_glow * f;
	// lamps in the vault at uneven bearings, and a few low round the
	// horizon and behind
	float s = spot(d, 0.0, 0.85, 60.0) + spot(d, 0.55, 1.05, 80.0)
		+ spot(d, -0.6, 0.7, 70.0) + spot(d, 0.35, 0.55, 100.0)
		+ spot(d, -0.3, 1.2, 80.0) + spot(d, 1.1, 0.75, 90.0)
		+ spot(d, -1.15, 0.95, 90.0)
		+ spot(d, 2.2, 0.3, 100.0) + spot(d, 3.0, 0.45, 100.0)
		+ spot(d, -2.4, 0.25, 100.0) + spot(d, 1.7, 0.15, 100.0)
		+ spot(d, -1.75, 0.2, 100.0);
	c += lamp * energy * s;
	// (never below zero: one bad texel spreads through every mip)
	COLOR = mix(flat_fill, max(c, vec3(0.0)), strength);
}
