//! What an effect leaves on the floor for a while: a fire's scorch, its
//! embers dying in it; frost; the etching of acid or poison; the glitter of
//! magic; the dust a body raises where it falls. Each is a decal on the
//! level (never on a creature), laid where the effect struck or along a
//! ray's way under it, and fading out over its own time. A glow is a second
//! decal that adds light only (its albedo untouched), dying out sooner.

use super::*;
use godot::classes::decal::DecalTexture;

/// A mark of an effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    /// Burnt black, embers of this colour glowing in it a moment (fire and
    /// a fiery blast orange, lightning a cold white).
    Scorch(Color),
    /// Burnt black, no glow (a ray of death, disintegration).
    Soot,
    /// Rime, a cold glow in it (cold).
    Frost,
    /// Pitted and stained in this colour (acid, poison).
    Etch(Color),
    /// Specks of light of this colour, twinkling out (magic missiles,
    /// sleep, a magic the client cannot tell, a magical blast).
    Glitter(Color),
    /// A ring of dust of this colour (a body falling, digging).
    Dust(Color),
}

/// The drawn shapes of marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    Blotch,
    Crystal,
    Pits,
    Specks,
    Puff,
}

/// Shapes of each kind drawn (a mark takes one by where it falls).
const SHAPES: u32 = 2;
/// Decals kept on the floor at once, of each sort (dark and glowing).
const MARKS: usize = 64;
/// How deep a mark reaches (metres, its box's height): the floor, not the
/// wall beside it.
const MARK_DEEP: f32 = 0.5;
/// A ray marks the floor under it in streaks along its way, each this
/// long (metres), one every this many (they overlap: one long mark),
/// from this far past its start (the caster's own cell is left clean).
const ALONG_LONG: f32 = 1.15;
const ALONG_STEP: f32 = 0.5;
const ALONG_FROM: f32 = 0.6;

/// How a mark looks: its shape; its dark part's colour, opacity and
/// seconds; its glow's colour, energy and seconds.
struct Look {
    shape: Shape,
    ground: Option<(Color, f32, f32)>,
    glow: Option<(Color, f32, f32)>,
}

fn look(kind: Mark) -> Look {
    let rgb = Color::from_rgb;
    match kind {
        Mark::Scorch(c) => Look {
            shape: Shape::Blotch,
            ground: Some((rgb(0.05, 0.04, 0.035), 0.92, 12.0)),
            glow: Some((c, 3.5, 2.4)),
        },
        Mark::Soot => Look {
            shape: Shape::Blotch,
            ground: Some((rgb(0.03, 0.025, 0.035), 0.9, 10.0)),
            glow: None,
        },
        Mark::Frost => Look {
            shape: Shape::Crystal,
            ground: Some((rgb(0.8, 0.9, 1.0), 0.85, 9.0)),
            glow: Some((rgb(0.5, 0.82, 1.0), 1.6, 4.0)),
        },
        Mark::Etch(c) => Look {
            shape: Shape::Pits,
            ground: Some((
                rgb(0.12 + c.r * 0.2, 0.1 + c.g * 0.2, 0.06 + c.b * 0.15),
                0.8,
                10.0,
            )),
            glow: Some((vivid(c), 0.25, 1.4)),
        },
        Mark::Glitter(c) => Look {
            shape: Shape::Specks,
            ground: None,
            glow: Some((vivid(c), 3.0, 2.6)),
        },
        Mark::Dust(c) => Look {
            shape: Shape::Puff,
            ground: Some((
                rgb(
                    (c.r * 1.3).min(1.0),
                    (c.g * 1.3).min(1.0),
                    (c.b * 1.3).min(1.0),
                ),
                0.55,
                6.0,
            )),
            glow: None,
        },
    }
}

/// A decal lying on the floor: its age, its life, and (a glow) its energy
/// at the start, (a dark one) its opacity.
struct Lying {
    decal: Gd<Decal>,
    age: f32,
    secs: f32,
    start: f32,
}

/// The marks on the floor, and their shapes.
pub(super) struct Marks {
    dark: Vec<Lying>,
    glow: Vec<Lying>,
    next_dark: usize,
    next_glow: usize,
    shapes: HashMap<Shape, Vec<Gd<Texture2D>>>,
}

impl Marks {
    pub(super) fn new() -> Marks {
        let shapes = [
            Shape::Blotch,
            Shape::Crystal,
            Shape::Pits,
            Shape::Specks,
            Shape::Puff,
        ]
        .into_iter()
        .map(|s| (s, (0..SHAPES).map(|i| shape_texture(s, i)).collect()))
        .collect();
        Marks {
            dark: Vec::new(),
            glow: Vec::new(),
            next_dark: 0,
            next_glow: 0,
            shapes,
        }
    }
}

impl Vfx {
    /// A mark of `kind` on the floor at `at` (its ground), `size` metres
    /// across.
    pub(super) fn mark(&mut self, kind: Mark, at: Vector3, size: f32) {
        let h = ((at.x * 12.9898 + at.z * 78.233).sin() * 43_758.547)
            .fract()
            .abs();
        self.mark_laid(kind, at, size, size, h * std::f32::consts::TAU);
    }

    /// A mark `wide` across and `long` along `turn` (radians about the
    /// up axis, from +x).
    fn mark_laid(&mut self, kind: Mark, at: Vector3, wide: f32, long: f32, turn: f32) {
        let l = look(kind);
        // its shape by where it falls, the same every time
        let h = ((at.x * 12.9898 + at.z * 78.233).sin() * 43_758.547)
            .fract()
            .abs();
        let tex = self
            .marks
            .shapes
            .get(&l.shape)
            .and_then(|v| v.get(((h * 97.0) as usize) % v.len().max(1)))
            .cloned();
        let Some(tex) = tex else {
            return;
        };
        let k = 0.85 + 0.3 * h;
        let (wide, long) = (wide * k, long * k);
        if let Some((color, alpha, secs)) = l.ground {
            let mask = self.decal_mask;
            let root = self.root.clone();
            let m = &mut self.marks;
            let d = lay(&mut m.dark, &mut m.next_dark, root, mask, false);
            d.decal.set_texture(DecalTexture::ALBEDO, &tex);
            d.decal
                .set_modulate(Color::from_rgba(color.r, color.g, color.b, alpha));
            place(d, at, Vector2::new(long, wide), turn, secs, alpha);
        }
        if let Some((color, energy, secs)) = l.glow {
            let mask = self.decal_mask;
            let root = self.root.clone();
            let m = &mut self.marks;
            let d = lay(&mut m.glow, &mut m.next_glow, root, mask, true);
            d.decal.set_texture(DecalTexture::EMISSION, &tex);
            d.decal.set_modulate(color);
            d.decal.set_emission_energy(energy);
            // the embers in the middle of a scorch, not to its edge
            let (kw, kl) = match kind {
                Mark::Scorch(_) => (0.45, 0.8),
                _ => (1.0, 1.0),
            };
            place(
                d,
                at,
                Vector2::new(long * kl, wide * kw),
                turn,
                secs,
                energy,
            );
        }
    }

    /// Marks of `kind` along the floor under a ray from `from` to `to`,
    /// `size` across, none in a solid cell nor under its very start.
    pub(super) fn mark_along(&mut self, kind: Mark, from: Vector3, to: Vector3, size: f32) {
        let flat = |p: Vector3| Vector3::new(p.x, 0.0, p.z);
        let (a, b) = (flat(from), flat(to));
        let len = a.distance_to(b);
        if len < ALONG_FROM {
            return;
        }
        let dir = (b - a) / len;
        let side = Vector3::new(-dir.z, 0.0, dir.x);
        // (a decal's x runs along its turn: from +x towards -z)
        let along = (-dir.z).atan2(dir.x);
        let mut t = ALONG_FROM + ALONG_LONG * 0.3;
        let mut i = 0u32;
        while t <= len {
            let p = a + dir * t;
            let cell = (p.x.round() as i32, p.z.round() as i32);
            if !self.solids.contains_key(&cell) {
                // streaks a little off the line and turned, none alike
                let j = |n: u32| hash01(self.marks.dark.len() as u32 + i * 7 + n, 31) - 0.5;
                let p = p + side * (j(1) * 0.12);
                let turn = along + j(2) * 0.25;
                self.mark_laid(kind, p, size * (0.85 + 0.3 * j(3)), ALONG_LONG, turn);
            }
            t += ALONG_STEP;
            i += 1;
        }
    }

    /// Every frame: the marks fade, a glow's sooner and by its light.
    pub(super) fn age_marks(&mut self, delta: f32) {
        for d in &mut self.marks.dark {
            if d.age >= d.secs {
                continue;
            }
            d.age += delta;
            let fade = 1.0 - ((d.age - d.secs * 0.6) / (d.secs * 0.4)).clamp(0.0, 1.0);
            let mut c = d.decal.get_modulate();
            c.a = d.start * fade;
            d.decal.set_modulate(c);
            if d.age >= d.secs {
                d.decal.set_visible(false);
            }
        }
        for d in &mut self.marks.glow {
            if d.age >= d.secs {
                continue;
            }
            d.age += delta;
            // bright at first, then dying out (an ember's, a frost's glow)
            let left = (1.0 - d.age / d.secs).clamp(0.0, 1.0);
            d.decal.set_emission_energy(d.start * left * left);
            if d.age >= d.secs {
                d.decal.set_visible(false);
            }
        }
    }

    /// No mark left (a new level).
    pub(super) fn clear_marks(&mut self) {
        for d in self.marks.dark.iter_mut().chain(self.marks.glow.iter_mut()) {
            d.age = d.secs;
            d.decal.set_visible(false);
        }
    }

    /// Marks showing now (self-tests).
    pub fn marks_shown(&self) -> usize {
        self.marks
            .dark
            .iter()
            .chain(&self.marks.glow)
            .filter(|d| d.age < d.secs)
            .count()
    }
}

/// The next decal of a pool, made while there are fewer than `MARKS`.
fn lay<'a>(
    pool: &'a mut Vec<Lying>,
    next: &mut usize,
    mut root: Gd<Node3D>,
    mask: u32,
    glow: bool,
) -> &'a mut Lying {
    if pool.len() < MARKS {
        let mut decal = Decal::new_alloc();
        decal.set_cull_mask(mask);
        // a glow adds light only; a dark mark is all colour, no light
        decal.set_albedo_mix(if glow { 0.0 } else { 1.0 });
        decal.set_emission_energy(0.0);
        root.add_child(&decal);
        pool.push(Lying {
            decal,
            age: 0.0,
            secs: 0.0,
            start: 0.0,
        });
    }
    let i = *next % pool.len();
    *next += 1;
    &mut pool[i]
}

fn place(d: &mut Lying, at: Vector3, size: Vector2, turn: f32, secs: f32, start: f32) {
    d.decal.set_size(Vector3::new(size.x, MARK_DEEP, size.y));
    d.decal.set_rotation(Vector3::new(0.0, turn, 0.0));
    d.decal.set_position(at);
    d.decal.set_visible(true);
    d.age = 0.0;
    d.secs = secs;
    d.start = start;
}

/// Smooth noise in 0..1 over a `cells` grid (`seed`'s), at (u, v) in
/// 0..1.
fn noise(seed: u32, cells: f32, u: f32, v: f32) -> f32 {
    let (x, y) = (u * cells, v * cells);
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let at = |dx: f32, dy: f32| {
        let (gx, gy) = ((ix + dx) as i32, (iy + dy) as i32);
        hash01(
            seed,
            (gx.wrapping_mul(73_856_093) ^ gy.wrapping_mul(19_349_663)) as u32,
        )
    };
    let top = at(0.0, 0.0) + (at(1.0, 0.0) - at(0.0, 0.0)) * s(fx);
    let bottom = at(0.0, 1.0) + (at(1.0, 1.0) - at(0.0, 1.0)) * s(fx);
    top + (bottom - top) * s(fy)
}

/// A mark's shape `i`: white where it shows (its alpha and its grey).
fn shape_texture(shape: Shape, i: u32) -> Gd<Texture2D> {
    let seed = 0x3a7 + i * 101 + shape as u32 * 7919;
    let r = move |k: u32| hash01(seed, k);
    let centre = |u: f32, v: f32| {
        let (dx, dy) = (u - 0.5, v - 0.5);
        ((dx * dx + dy * dy).sqrt(), dy.atan2(dx))
    };
    match shape {
        // a ragged blotch, darkest in its middle, a few streaks thrown out
        Shape::Blotch => {
            let streaks: Vec<(f32, f32)> = (0..7)
                .map(|k| (r(k) * std::f32::consts::TAU, 0.34 + 0.12 * r(20 + k)))
                .collect();
            texture(128, move |u, v| {
                let (d, a) = centre(u, v);
                let edge = 0.3 + 0.12 * (noise(seed, 6.0, u, v) - 0.5) * 2.0;
                let core = ((edge - d) / 0.06).clamp(0.0, 1.0);
                let deep = ((edge - d) / edge).clamp(0.0, 1.0);
                let mut streak: f32 = 0.0;
                for &(sa, reach) in &streaks {
                    let off = ((a - sa + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI)
                        .abs();
                    let w = 0.12 * (1.0 - d / reach).max(0.0);
                    if d < reach && off < w {
                        streak = streak.max((1.0 - off / w.max(1e-3)) * (1.0 - d / reach));
                    }
                }
                let speck = noise(seed + 9, 18.0, u, v);
                (core * (0.55 + 0.45 * deep) * (0.8 + 0.2 * speck)).max(streak * 0.7)
            })
        }
        // rime: frost lying in patches, needles of ice grown every way
        // through it, each with a branch or two
        Shape::Crystal => {
            let needles: Vec<(f32, f32, f32, f32)> = (0..34)
                .map(|k| {
                    let a = r(k * 4) * std::f32::consts::TAU;
                    let d = r(k * 4 + 1).sqrt() * 0.34;
                    (
                        0.5 + d * a.cos(),
                        0.5 + d * a.sin(),
                        r(k * 4 + 2) * std::f32::consts::TAU,
                        0.04 + 0.08 * r(k * 4 + 3),
                    )
                })
                .collect();
            texture(128, move |u, v| {
                let (d, _) = centre(u, v);
                let fade = (1.0 - d / 0.45).clamp(0.0, 1.0);
                let patch = noise(seed, 7.0, u, v) * 0.6 + noise(seed + 5, 19.0, u, v) * 0.4;
                let frost = ((patch - 0.35) / 0.3).clamp(0.0, 1.0) * fade;
                let mut line: f32 = 0.0;
                for &(x, y, a, len) in &needles {
                    for (ba, blen, w0) in [
                        (a, len, 0.006f32),
                        (a + 0.9, len * 0.45, 0.004),
                        (a - 0.9, len * 0.4, 0.004),
                    ] {
                        // a branch starts a third of the way along
                        let (ox, oy) = if blen < len {
                            (x + a.cos() * len * 0.35, y + a.sin() * len * 0.35)
                        } else {
                            (x, y)
                        };
                        let (bx, by) = (ba.cos(), ba.sin());
                        let (px, py) = (u - ox, v - oy);
                        let along = px * bx + py * by;
                        let across = (-px * by + py * bx).abs();
                        if along > 0.0 && along < blen {
                            let w = w0 * (1.0 - along / blen) + 0.0015;
                            line = line.max((1.0 - across / w).max(0.0));
                        }
                    }
                }
                (frost * 0.55).max(line * fade.sqrt())
            })
        }
        // pitted: a blotch eaten through in holes
        Shape::Pits => texture(128, move |u, v| {
            let (d, _) = centre(u, v);
            let edge = 0.3 + 0.1 * (noise(seed, 5.0, u, v) - 0.5) * 2.0;
            let inside = ((edge - d) / 0.05).clamp(0.0, 1.0);
            let pits = noise(seed + 3, 14.0, u, v);
            let holes = ((pits - 0.35) / 0.15).clamp(0.0, 1.0);
            inside * (0.35 + 0.65 * holes)
        }),
        // specks of light in a disc, thicker in its middle
        Shape::Specks => {
            let specks: Vec<(f32, f32, f32)> = (0..46)
                .map(|k| {
                    let a = r(k * 3) * std::f32::consts::TAU;
                    let d = r(k * 3 + 1).sqrt() * 0.4;
                    (
                        0.5 + d * a.cos(),
                        0.5 + d * a.sin(),
                        0.006 + 0.012 * r(k * 3 + 2),
                    )
                })
                .collect();
            texture(128, move |u, v| {
                let mut a: f32 = 0.0;
                for &(x, y, s) in &specks {
                    let d = ((u - x).powi(2) + (v - y).powi(2)).sqrt();
                    a = a.max((1.0 - d / s).max(0.0));
                }
                a
            })
        }
        // a soft ring of dust, thicker at its rim
        Shape::Puff => texture(128, move |u, v| {
            let (d, _) = centre(u, v);
            let rim = (1.0 - ((d - 0.3) / 0.14).abs()).max(0.0);
            let fill = (1.0 - d / 0.42).clamp(0.0, 1.0) * 0.35;
            rim.max(fill) * (0.6 + 0.4 * noise(seed, 9.0, u, v))
        }),
    }
}
