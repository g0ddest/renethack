//! The spells' icons (`make spell-icons`): each of 5.0's spells as its own
//! symbol (game-icons.net, CC BY 3.0, in `art/ccby/game-icons/`; the
//! credits name each) painted on its school's plate. A school has its
//! colour and its plate's shape, so colour is never the only cue:
//!
//! | school | plate | colour |
//! |---|---|---|
//! | attack | a disc with four spikes | ember red |
//! | healing | a disc | green |
//! | divination | a hexagon | blue |
//! | enchantment | a quatrefoil | violet |
//! | clerical | a lancet arch | amber |
//! | escape | a slanted plate | teal |
//! | matter | a chamfered block | slate grey |
//!
//! The plate sits in an aged gold rim lit from the top left, as the HUD's
//! medallions; its field is brushed paint, darker at the rim; the symbol
//! is a lit relief on it, glowing in the school's light, with a dark
//! outline and a cast shadow; the whole plate casts a soft shadow, as the
//! item icons' objects do. Painted at three times the size and scaled
//! down: 128 px PNGs in `art/icons/spells/<slug>.png`. The `spell-icons`
//! self-test bakes them (`make spell-icons`, headless).

use std::path::PathBuf;

use godot::classes::image::Format;
use godot::classes::{FileAccess, Image, ProjectSettings};
use godot::prelude::*;

use crate::spells::{self, Known, School};

/// Where the icons go, and where their symbols are.
pub const DIR: &str = "res://art/icons/spells";
const ART: &str = "res://art/ccby/game-icons";
/// An icon's side, and how many times larger it is painted.
pub const SIDE: usize = 128;
const SS: usize = 3;
const N: usize = SIDE * SS;
/// The SVGs' black square under the white symbol (left out: the symbol's
/// alpha is its mask).
const BACKDROP: &str = r#"<path d="M0 0h512v512H0z"/>"#;
/// The rim's width, the room kept between the symbol and the rim, and the
/// most of the canvas a symbol spans (in the plate's units: the canvas is
/// -1..1 across).
const RIM: f32 = 0.10;
const ROOM: f32 = 0.05;
const MOST: f32 = 1.34;

// ---- colours ----

type Rgb = [f32; 3];

const fn rgb(hex: u32) -> Rgb {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn add(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `a` lit by `b` as a screen: never darker, never past white.
fn screen(a: Rgb, b: Rgb) -> Rgb {
    let s = |x: f32, y: f32| 1.0 - (1.0 - x.clamp(0.0, 1.0)) * (1.0 - y.clamp(0.0, 1.0));
    [s(a[0], b[0]), s(a[1], b[1]), s(a[2], b[2])]
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A school's paint: its field's depth and middle, and its light.
#[derive(Debug, Clone, Copy)]
pub struct Paint {
    pub deep: Rgb,
    pub mid: Rgb,
    pub light: Rgb,
}

pub fn paint_of(s: School) -> Paint {
    let p = |deep, mid, light| Paint {
        deep: rgb(deep),
        mid: rgb(mid),
        light: rgb(light),
    };
    match s {
        School::Attack => p(0x2a0705, 0x9a2a18, 0xffb27a),
        School::Healing => p(0x05200e, 0x23803f, 0xbaf5a6),
        School::Divination => p(0x061330, 0x2558ae, 0xb6dcff),
        School::Enchantment => p(0x1c0828, 0x7c2fa2, 0xf2c4ff),
        School::Clerical => p(0x2c1904, 0xa8781e, 0xfff0c4),
        School::Escape => p(0x03201e, 0x1a8a84, 0xaefcf0),
        School::Matter => p(0x111417, 0x5c6672, 0xe6eef6),
    }
}

const GOLD_DARK: Rgb = rgb(0x3e2a10);
const GOLD: Rgb = rgb(0xb8893b);
const GOLD_BRIGHT: Rgb = rgb(0xfbe6a6);
const IVORY: Rgb = rgb(0xfff6e2);
/// The light comes from the top left, a little in front.
const LIGHT: [f32; 3] = [-0.55, -0.70, 0.65];

fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Diffuse and specular light on a surface of normal `n`.
fn lit(n: [f32; 3], shine: f32) -> (f32, f32) {
    let l = norm3(LIGHT);
    let h = norm3([l[0], l[1], l[2] + 1.0]);
    (dot3(n, l).max(0.0), dot3(n, h).max(0.0).powf(shine))
}

// ---- the plates ----

fn len(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// A box of half sides `hx`, `hy` with corners rounded by `r`.
fn round_box(x: f32, y: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = x.abs() - hx + r;
    let qy = y.abs() - hy + r;
    len(qx.max(0.0), qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// The signed distance from the plate's edge of school `s` at `(x, y)`
/// (the canvas is -1..1, y down): negative inside.
pub fn plate(s: School, x: f32, y: f32) -> f32 {
    const R2: f32 = std::f32::consts::FRAC_1_SQRT_2;
    match s {
        School::Attack => {
            // a disc with four spikes: a diamond through it
            let spikes = (x.abs() + y.abs() - 1.0) * R2;
            (len(x, y) - 0.83).min(spikes)
        }
        School::Healing => len(x, y) - 0.88,
        School::Divination => {
            // pointy-top, its corners 0.95 from the middle
            let (px, py) = (x.abs(), y.abs());
            let k = [0.5f32, 0.866_025_4];
            let r = 0.95 * 0.866_025_4;
            // the hexagon's apothem along its six normals
            let d1 = px * 1.0 - r;
            let d2 = px * k[0] + py * k[1] - r;
            d1.max(d2)
        }
        School::Enchantment => {
            let lobe = |cx: f32, cy: f32| len(x - cx, y - cy) - 0.5;
            let a = 0.38;
            // a disc in the middle keeps the field deep there (the lobes'
            // distances alone are too shallow inside)
            lobe(a, 0.0)
                .min(lobe(-a, 0.0))
                .min(lobe(0.0, a))
                .min(lobe(0.0, -a))
                .min(len(x, y) - 0.62)
        }
        School::Clerical => {
            // a lancet: each side an arc of radius 1 above the springing
            // line, straight down below it; a flat foot
            let spring = 0.05;
            let side = |sx: f32| {
                if y < spring {
                    len(sx + 0.26, y - spring) - 1.0
                } else {
                    sx - 0.74
                }
            };
            side(x).max(side(-x)).max(y - 0.90)
        }
        School::Escape => {
            // leaning forwards: its top further right than its foot
            let sx = x + y * 0.26;
            round_box(sx, y, 0.62, 0.84, 0.10) * 0.97
        }
        School::Matter => {
            let block = round_box(x, y, 0.82, 0.82, 0.03);
            block.max((x.abs() + y.abs() - 1.34) * R2)
        }
    }
}

/// Where a symbol's middle goes on a school's plate.
fn centre(s: School) -> (f32, f32) {
    match s {
        School::Clerical => (0.0, 0.10),
        _ => (0.0, 0.0),
    }
}

// ---- the symbol's mask ----

/// A symbol's coverage, `w` by `h`.
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
}

impl Mask {
    fn at(&self, x: i64, y: i64) -> f32 {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return 0.0;
        }
        self.a[y as usize * self.w + x as usize]
    }

    /// Bilinear, outside it nothing.
    fn sample(&self, x: f32, y: f32) -> f32 {
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (ix, iy) = (x0 as i64, y0 as i64);
        let a = self.at(ix, iy) * (1.0 - fx) + self.at(ix + 1, iy) * fx;
        let b = self.at(ix, iy + 1) * (1.0 - fx) + self.at(ix + 1, iy + 1) * fx;
        a * (1.0 - fy) + b * fy
    }

    /// The box of what it covers (more than half): left, top, right,
    /// bottom.
    fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let mut b: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.h {
            for x in 0..self.w {
                if self.a[y * self.w + x] > 0.5 {
                    b = Some(match b {
                        None => (x, y, x, y),
                        Some((l, t, r, bo)) => (l.min(x), t.min(y), r.max(x), bo.max(y)),
                    });
                }
            }
        }
        b.map(|(l, t, r, bo)| (l as f32, t as f32, r as f32 + 1.0, bo as f32 + 1.0))
    }
}

/// How the symbol sits on the plate: the plate's point of a mask pixel is
/// `centre + (m - middle) * scale`.
#[derive(Debug, Clone, Copy)]
struct Fit {
    middle: (f32, f32),
    centre: (f32, f32),
    scale: f32,
}

impl Fit {
    fn mask_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.middle.0 + (x - self.centre.0) / self.scale,
            self.middle.1 + (y - self.centre.1) / self.scale,
        )
    }
}

/// The largest the symbol may be on the plate of `s`: every point it
/// covers inside the field with ROOM to spare, and it spans at most MOST.
fn fit(mask: &Mask, s: School) -> Option<Fit> {
    let (l, t, r, b) = mask.bounds()?;
    let middle = ((l + r) / 2.0, (t + b) / 2.0);
    let centre = centre(s);
    let points: Vec<(f32, f32)> = (0..mask.h)
        .step_by(3)
        .flat_map(|y| (0..mask.w).step_by(3).map(move |x| (x, y)))
        .filter(|&(x, y)| mask.a[y * mask.w + x] > 0.5)
        .map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5))
        .collect();
    let inside = |scale: f32| {
        points.iter().all(|&(mx, my)| {
            let x = centre.0 + (mx - middle.0) * scale;
            let y = centre.1 + (my - middle.1) * scale;
            plate(s, x, y) < -(RIM + ROOM)
        })
    };
    let span = (r - l).max(b - t);
    let (mut lo, mut hi) = (0.0f32, MOST / span);
    if inside(hi) {
        lo = hi;
    } else {
        for _ in 0..16 {
            let m = (lo + hi) / 2.0;
            if inside(m) {
                lo = m;
            } else {
                hi = m;
            }
        }
    }
    (lo > 0.0).then_some(Fit {
        middle,
        centre,
        scale: lo,
    })
}

// ---- noise and blurs ----

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8da6_b343)
        .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0 * 2.0 - 1.0
}

/// Smooth value noise, -1..1.
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy, seed) * (1.0 - ux) + hash(ix + 1, iy, seed) * ux;
    let b = hash(ix, iy + 1, seed) * (1.0 - ux) + hash(ix + 1, iy + 1, seed) * ux;
    a * (1.0 - uy) + b * uy
}

/// The paint's grain at a plate point: a mottle and brush strokes across
/// it, -1..1.
fn grain(x: f32, y: f32, seed: u32) -> f32 {
    let mottle = noise(x * 3.0, y * 3.0, seed) * 0.5
        + noise(x * 7.0, y * 7.0, seed + 1) * 0.3
        + noise(x * 16.0, y * 16.0, seed + 2) * 0.2;
    // strokes laid from the bottom left to the top right
    let (u, v) = (x * 0.866 - y * 0.5, x * 0.5 + y * 0.866);
    let strokes = noise(u * 2.2, v * 22.0, seed + 3);
    mottle * 0.6 + strokes * 0.4
}

/// A box blur of radius `r` three times over (near a Gaussian), the edges
/// held.
fn blur(src: &[f32], n: usize, r: usize) -> Vec<f32> {
    if r == 0 {
        return src.to_vec();
    }
    let mut a = src.to_vec();
    let mut b = vec![0.0f32; a.len()];
    let w = (2 * r + 1) as f32;
    for _ in 0..3 {
        for pass in 0..2 {
            for line in 0..n {
                let idx = |i: usize| {
                    if pass == 0 {
                        line * n + i
                    } else {
                        i * n + line
                    }
                };
                let get = |i: i64| a[idx(i.clamp(0, n as i64 - 1) as usize)];
                let mut sum: f32 = (-(r as i64)..=r as i64).map(get).sum();
                for i in 0..n {
                    b[idx(i)] = sum / w;
                    sum += get(i as i64 + r as i64 + 1) - get(i as i64 - r as i64);
                }
            }
            std::mem::swap(&mut a, &mut b);
        }
    }
    a
}

/// `src` moved by `(dx, dy)` pixels.
fn shifted(src: &[f32], n: usize, dx: i64, dy: i64) -> Vec<f32> {
    let mut out = vec![0.0; src.len()];
    for y in 0..n as i64 {
        for x in 0..n as i64 {
            let (sx, sy) = (x - dx, y - dy);
            if sx >= 0 && sy >= 0 && (sx as usize) < n && (sy as usize) < n {
                out[(y as usize) * n + x as usize] = src[sy as usize * n + sx as usize];
            }
        }
    }
    out
}

// ---- the painting ----

/// The icon of a symbol `mask` on the plate of school `s`: RGBA8 (straight
/// alpha), SIDE by SIDE. `seed` varies the paint's grain.
pub fn paint(mask: &Mask, s: School, seed: u32) -> Option<Vec<u8>> {
    let f = fit(mask, s)?;
    let pal = paint_of(s);
    let px = 2.0 / N as f32;
    let at = |i: usize| (i as f32 + 0.5) * px - 1.0;
    // the symbol and the plate at the painting's size
    let mut sym = vec![0.0f32; N * N];
    let mut cover = vec![0.0f32; N * N];
    for y in 0..N {
        for x in 0..N {
            let (u, v) = (at(x), at(y));
            let (mx, my) = f.mask_point(u, v);
            sym[y * N + x] = mask.sample(mx, my);
            cover[y * N + x] = (0.5 - plate(s, u, v) / px).clamp(0.0, 1.0);
        }
    }
    let k = |v: f32| (v * N as f32 / 384.0).round().max(1.0) as usize;
    let height = blur(&sym, N, k(2.0));
    let halo = blur(&sym, N, k(4.0));
    let glow = blur(&sym, N, k(16.0));
    let cast = blur(&shifted(&sym, N, k(5.0) as i64, k(7.0) as i64), N, k(5.0));
    let ground = blur(
        &shifted(&cover, N, k(8.0) as i64, k(12.0) as i64),
        N,
        k(9.0),
    );
    let mut out = vec![[0.0f32; 4]; N * N];
    for y in 0..N {
        for x in 0..N {
            let i = y * N + x;
            let (u, v) = (at(x), at(y));
            let d = plate(s, u, v);
            // the plate's shadow under it
            let shadow_a = ground[i] * 0.55;
            let a = cover[i];
            if a <= 0.0 {
                out[i] = [0.0, 0.0, 0.0, shadow_a];
                continue;
            }
            // the rim: a bead of old gold, its slope by the edge's normal
            let e = px * 0.75;
            let gx = plate(s, u + e, v) - plate(s, u - e, v);
            let gy = plate(s, u, v + e) - plate(s, u, v - e);
            let gl = len(gx, gy).max(1e-6);
            let t = (-d / RIM).clamp(0.0, 1.0);
            let slope = 0.6 * std::f32::consts::PI * (std::f32::consts::PI * t).cos();
            let n = norm3([slope * gx / gl, slope * gy / gl, 1.0]);
            let (diff, spec) = lit(n, 28.0);
            let mut rim = mix(GOLD_DARK, GOLD, (0.25 + diff).min(1.0));
            rim = add(rim, scale(GOLD_BRIGHT, spec * 0.85));
            // a groove where the rim meets the field
            rim = scale(rim, 1.0 - 0.45 * smooth(0.78, 1.0, t));
            // the field: brushed paint, lit from the top left, deep at the rim
            let r = len(u + 0.2, v + 0.3) / 1.35;
            let mut field = mix(scale(pal.mid, 1.12), pal.deep, smooth(0.0, 1.0, r));
            field = scale(field, 1.0 + 0.09 * grain(u, v, seed));
            let inner = -(d + RIM);
            field = scale(field, 0.5 + 0.5 * smooth(0.0, 0.2, inner));
            // the symbol's cast shadow, its glow, its outline
            field = scale(field, 1.0 - 0.7 * cast[i]);
            field = screen(field, scale(pal.light, 0.75 * glow[i]));
            let outline = scale(pal.deep, 0.35);
            field = mix(field, outline, (halo[i] * 2.6).min(1.0));
            // the symbol: a relief of ivory and the school's light
            let m = sym[i];
            if m > 0.0 {
                let hx = height[(i + 1).min(N * N - 1)] - height[i.saturating_sub(1)];
                let hy = height[(i + N).min(N * N - 1)] - height[i.saturating_sub(N)];
                let k = 5.0;
                let n = norm3([-hx * k, -hy * k, 1.0]);
                let (diff, spec) = lit(n, 20.0);
                let top = mix(IVORY, pal.light, 0.35);
                let foot = mix(pal.light, pal.mid, 0.28);
                let base = mix(top, foot, smooth(-0.75, 0.75, v));
                let mut body = scale(base, 0.45 + 0.7 * diff);
                body = add(body, scale(IVORY, spec * 0.45));
                body = scale(body, 1.0 + 0.04 * grain(u * 2.0, v * 2.0, seed + 9));
                field = mix(field, body, m);
            }
            let fa = (0.5 - (d + RIM) / px).clamp(0.0, 1.0);
            let col = mix(rim, field, fa);
            // the plate over its shadow
            let alpha = a + shadow_a * (1.0 - a);
            let c = |j: usize| (col[j].clamp(0.0, 1.0) * a) / alpha.max(1e-6);
            out[i] = [c(0), c(1), c(2), alpha];
        }
    }
    // three by three, premultiplied, to SIDE
    let mut px8 = vec![0u8; SIDE * SIDE * 4];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let mut acc = [0.0f32; 4];
            for dy in 0..SS {
                for dx in 0..SS {
                    let p = out[(y * SS + dy) * N + x * SS + dx];
                    for j in 0..3 {
                        acc[j] += p[j] * p[3];
                    }
                    acc[3] += p[3];
                }
            }
            let a = acc[3] / (SS * SS) as f32;
            let o = (y * SIDE + x) * 4;
            for j in 0..3 {
                let c = if acc[3] > 0.0 { acc[j] / acc[3] } else { 0.0 };
                px8[o + j] = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            px8[o + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    Some(px8)
}

// ---- the bake (Godot) ----

fn global(path: &str) -> PathBuf {
    ProjectSettings::singleton()
        .globalize_path(path)
        .to_string()
        .into()
}

/// The symbol of `known` as a mask, its SVG drawn at `side` pixels.
fn symbol(known: &Known, side: usize) -> Result<Mask, String> {
    let path = format!("{ART}/{}.svg", known.art);
    if !FileAccess::file_exists(&path) {
        return Err(format!("{}: no {path}", known.name));
    }
    let svg = FileAccess::get_file_as_string(&path).to_string();
    let svg = svg.replace(BACKDROP, "");
    let mut img = Image::new_gd();
    let err = img
        .load_svg_from_string_ex(&svg)
        .scale(side as f32 / 512.0)
        .done();
    if err != godot::global::Error::OK {
        return Err(format!("{}: {path} does not draw: {err:?}", known.name));
    }
    img.convert(Format::RGBA8);
    let (w, h) = (img.get_width() as usize, img.get_height() as usize);
    let data = img.get_data().to_vec();
    let a = data.chunks(4).map(|p| f32::from(p[3]) / 255.0).collect();
    Ok(Mask { w, h, a })
}

/// Bake the icon of the spell `known`; its path.
pub fn bake(known: &Known) -> Result<PathBuf, String> {
    let mask = symbol(known, 1024)?;
    let seed = known
        .name
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let rgba = paint(&mask, known.school, seed)
        .ok_or_else(|| format!("{}: its symbol is empty", known.name))?;
    let image = Image::create_from_data(
        SIDE as i32,
        SIDE as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(rgba.as_slice()),
    )
    .ok_or("cannot make an image")?;
    let dir = global(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.png", spells::slug(known.name)));
    crate::icon_bake::save(&image, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [School; 7] = [
        School::Attack,
        School::Healing,
        School::Divination,
        School::Enchantment,
        School::Clerical,
        School::Escape,
        School::Matter,
    ];

    /// A ring as a symbol.
    fn ring(side: usize) -> Mask {
        let c = side as f32 / 2.0;
        let a = (0..side * side)
            .map(|i| {
                let (x, y) = ((i % side) as f32 + 0.5 - c, (i / side) as f32 + 0.5 - c);
                let r = len(x, y) / c;
                if (0.5..0.9).contains(&r) { 1.0 } else { 0.0 }
            })
            .collect();
        Mask {
            w: side,
            h: side,
            a,
        }
    }

    #[test]
    fn every_spell_has_its_symbol_its_icon_and_its_credit() {
        let art = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot/art");
        let credits = std::fs::read_to_string(art.join("CREDITS.md")).unwrap();
        let sources = std::fs::read_to_string(art.join("sources.json")).unwrap();
        for k in &crate::spells::KNOWN {
            let svg = format!("ccby/game-icons/{}.svg", k.art);
            assert!(art.join(&svg).is_file(), "{}: no {svg}", k.name);
            assert!(
                credits.contains(&format!("`{svg}`")),
                "{}: not credited",
                k.name
            );
            assert!(
                sources.contains(&format!("\"out\": \"{svg}\"")),
                "{}: no source",
                k.name
            );
            let png = art.join(format!("icons/spells/{}.png", crate::spells::slug(k.name)));
            assert!(png.is_file(), "{}: no icon (make spell-icons)", k.name);
        }
    }

    #[test]
    fn every_plate_holds_its_middle_and_ends_inside_the_canvas() {
        for s in ALL {
            assert!(plate(s, 0.0, 0.0) < -0.5, "{s:?}");
            let (cx, cy) = centre(s);
            assert!(plate(s, cx, cy) < -0.5, "{s:?}");
            for (x, y) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
                assert!(plate(s, x, y) > 0.0, "{s:?} covers a corner");
            }
        }
    }

    #[test]
    fn the_plates_differ_in_shape_not_only_in_colour() {
        // each pair of plates differs over a good part of the canvas
        let cover = |s: School| -> Vec<bool> {
            (0..64 * 64)
                .map(|i| {
                    let (x, y) = ((i % 64) as f32 / 32.0 - 1.0, (i / 64) as f32 / 32.0 - 1.0);
                    plate(s, x, y) < 0.0
                })
                .collect()
        };
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                let (ca, cb) = (cover(*a), cover(*b));
                let differ = ca.iter().zip(&cb).filter(|(p, q)| p != q).count();
                assert!(differ > 64 * 64 / 25, "{a:?} and {b:?}: {differ}");
            }
        }
    }

    #[test]
    fn a_symbol_is_fitted_inside_its_field_and_painted() {
        let m = ring(96);
        for s in ALL {
            let f = fit(&m, s).expect("a fit");
            assert!(f.scale > 0.0);
            // the ring's outer edge lies inside the field
            let edge = f.centre.0 + (95.0 - f.middle.0) * f.scale * 0.9;
            assert!(plate(s, edge, f.centre.1) < -RIM, "{s:?}");
        }
        let px = paint(&m, School::Healing, 1).expect("painted");
        assert_eq!(px.len(), SIDE * SIDE * 4);
        // the middle of the ring is the field (opaque), a corner is clear
        let alpha = |x: usize, y: usize| px[(y * SIDE + x) * 4 + 3];
        assert_eq!(alpha(SIDE / 2, SIDE / 2), 255);
        assert_eq!(alpha(0, 0), 0);
        // the same paint every time
        assert_eq!(paint(&m, School::Healing, 1), Some(px));
        let empty = Mask {
            w: 8,
            h: 8,
            a: vec![0.0; 64],
        };
        assert!(paint(&empty, School::Attack, 1).is_none());
    }
}
