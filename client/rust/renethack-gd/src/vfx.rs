//! The map's effects: bursts of particles (hit sparks, blood, the magic of
//! quaffing, reading, zapping and casting, fire, frost, lightning, poison,
//! explosions, healing, sparkles) with a short flash of light, beams along
//! a ray with a light travelling on them, blood splats on the floor that
//! fade, and the dust in the air that shows only where light falls.
//! Emitters, lights and decals are pooled per kind; nothing here is ever
//! more than a picture of what the game already said.
//!
//! Other parts of the client call `Vfx::burst(kind, at)` and
//! `Vfx::beam(kind, from, to)` (world positions) or the map's cell helpers
//! `MapView::burst_at` and `MapView::beam_between`.

use std::collections::HashMap;

use godot::classes::base_material_3d::{
    BillboardMode, BlendMode, Flags, ShadingMode, Transparency,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::gpu_particles_3d::TransformAlign;
use godot::classes::image::Format;
use godot::classes::light_3d::Param;
use godot::classes::particle_process_material::{EmissionShape, Parameter};
use godot::classes::{
    Decal, GpuParticles3D, Gradient, GradientTexture1D, Image, ImageTexture, Material,
    MeshInstance3D, Node3D, OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader, ShaderMaterial,
    StandardMaterial3D, Texture2D,
};
use godot::prelude::*;

/// What an effect shows.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // most are for the hero's item use (Phase E)
pub enum VfxKind {
    /// A potion drunk: motes of its colour rising around the drinker.
    Quaff(Color),
    /// A scroll read: golden runes rising, the light warming for a moment.
    Read,
    /// A wand zapped: a flare of its colour (at the tip, or where it hits).
    Zap(Color),
    /// A spell cast: a swirl of motes of its colour.
    Cast(Color),
    Fire,
    Frost,
    Lightning,
    Poison,
    /// A blast of this colour.
    Explosion(Color),
    Heal,
    Sparkle,
    /// A weapon's blow landing.
    Sparks,
    /// A creature bleeding, of this colour (droplets and a splat).
    Blood(Color),
}

/// A particle recipe: what a pooled emitter of a kind is built with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Recipe {
    Motes,
    Runes,
    Flare,
    Swirl,
    Flames,
    Shards,
    Bolts,
    Cloud,
    Blast,
    Glints,
    Sparks,
    /// A flare at the point of contact.
    Flash,
    Droplets,
}

struct Params {
    amount: i32,
    life: f64,
    speed: (f32, f32),
    spread: f32,
    dir: Vector3,
    gravity: Vector3,
    size: (f32, f32),
    radius: f32,
    damping: f32,
    /// The ramp over a particle's life: hot at birth, fading out.
    hdr: f32,
    additive: bool,
}

impl Recipe {
    fn params(self) -> Params {
        let base = Params {
            amount: 20,
            life: 0.6,
            speed: (0.3, 0.8),
            spread: 25.0,
            dir: Vector3::UP,
            gravity: Vector3::ZERO,
            size: (0.03, 0.06),
            radius: 0.3,
            damping: 0.5,
            hdr: 1.8,
            additive: true,
        };
        match self {
            Recipe::Motes => base,
            Recipe::Runes => Params {
                amount: 16,
                life: 0.9,
                speed: (0.4, 0.9),
                size: (0.05, 0.09),
                ..base
            },
            Recipe::Flare => Params {
                amount: 14,
                life: 0.3,
                speed: (1.0, 2.2),
                spread: 180.0,
                radius: 0.05,
                size: (0.03, 0.06),
                hdr: 2.2,
                ..base
            },
            Recipe::Swirl => Params {
                amount: 28,
                life: 0.8,
                speed: (0.3, 0.7),
                spread: 70.0,
                radius: 0.45,
                ..base
            },
            Recipe::Flames => Params {
                amount: 36,
                life: 0.55,
                speed: (0.6, 1.6),
                spread: 40.0,
                gravity: Vector3::new(0.0, 1.5, 0.0),
                size: (0.08, 0.16),
                radius: 0.25,
                hdr: 2.5,
                ..base
            },
            Recipe::Shards => Params {
                amount: 26,
                life: 0.7,
                speed: (1.0, 2.4),
                spread: 70.0,
                gravity: Vector3::new(0.0, -6.0, 0.0),
                size: (0.03, 0.06),
                radius: 0.15,
                hdr: 2.5,
                ..base
            },
            Recipe::Bolts => Params {
                amount: 24,
                life: 0.25,
                speed: (2.5, 5.0),
                spread: 180.0,
                size: (0.02, 0.05),
                radius: 0.1,
                hdr: 3.0,
                ..base
            },
            Recipe::Cloud => Params {
                amount: 18,
                life: 1.4,
                speed: (0.15, 0.4),
                spread: 90.0,
                gravity: Vector3::new(0.0, 0.2, 0.0),
                size: (0.2, 0.38),
                radius: 0.35,
                damping: 0.3,
                hdr: 1.0,
                additive: false,
                dir: Vector3::UP,
            },
            Recipe::Blast => Params {
                amount: 60,
                life: 0.6,
                speed: (1.5, 4.0),
                spread: 180.0,
                gravity: Vector3::new(0.0, 1.0, 0.0),
                size: (0.1, 0.22),
                radius: 0.3,
                damping: 3.0,
                hdr: 2.8,
                ..base
            },
            Recipe::Glints => Params {
                amount: 8,
                life: 0.8,
                speed: (0.05, 0.2),
                spread: 180.0,
                size: (0.03, 0.06),
                radius: 0.3,
                hdr: 2.5,
                ..base
            },
            // streaks thrown out from the point of contact (each drawn
            // along its own way), round a short flare
            Recipe::Sparks => Params {
                amount: 44,
                life: 0.34,
                speed: (2.5, 6.5),
                spread: 55.0,
                gravity: Vector3::new(0.0, -7.0, 0.0),
                size: (0.025, 0.05),
                radius: 0.06,
                damping: 2.5,
                hdr: 1.7,
                ..base
            },
            Recipe::Flash => Params {
                amount: 2,
                life: 0.14,
                speed: (0.0, 0.05),
                spread: 180.0,
                size: (0.45, 0.6),
                radius: 0.0,
                damping: 0.0,
                hdr: 3.5,
                ..base
            },
            Recipe::Droplets => Params {
                amount: 16,
                life: 0.5,
                speed: (1.2, 2.6),
                spread: 60.0,
                gravity: Vector3::new(0.0, -9.8, 0.0),
                size: (0.025, 0.05),
                radius: 0.05,
                damping: 0.2,
                hdr: 1.0,
                additive: false,
                dir: Vector3::UP,
            },
        }
    }
}

/// The colour of a kind's particles, its light, and the recipe.
fn look_of(kind: VfxKind) -> (Recipe, Color, Option<(Color, f32, f32)>) {
    let rgb = Color::from_rgb;
    match kind {
        VfxKind::Quaff(c) => (Recipe::Motes, c, Some((c, 0.5, 0.5))),
        VfxKind::Read => {
            let gold = rgb(1.0, 0.8, 0.4);
            (Recipe::Runes, gold, Some((gold, 0.8, 0.5)))
        }
        VfxKind::Zap(c) => (Recipe::Flare, c, Some((c, 1.2, 0.15))),
        VfxKind::Cast(c) => (Recipe::Swirl, c, Some((c, 0.9, 0.4))),
        VfxKind::Fire => {
            let c = rgb(1.0, 0.48, 0.13);
            (Recipe::Flames, c, Some((c, 2.5, 0.4)))
        }
        VfxKind::Frost => {
            let c = rgb(0.56, 0.88, 1.0);
            (Recipe::Shards, c, Some((c, 1.2, 0.3)))
        }
        VfxKind::Lightning => {
            let c = rgb(0.91, 0.94, 1.0);
            (Recipe::Bolts, c, Some((c, 4.0, 0.12)))
        }
        VfxKind::Poison => (Recipe::Cloud, rgb(0.56, 1.0, 0.25), None),
        VfxKind::Explosion(c) => (Recipe::Blast, c, Some((c, 4.0, 0.3))),
        VfxKind::Heal => {
            let c = rgb(0.6, 1.0, 0.55);
            (Recipe::Motes, c, Some((c, 0.8, 0.6)))
        }
        VfxKind::Sparkle => (Recipe::Glints, rgb(1.0, 0.95, 0.8), None),
        VfxKind::Sparks => {
            let c = rgb(1.0, 0.6, 0.22);
            (Recipe::Sparks, c, Some((c, 5.0, 0.1)))
        }
        VfxKind::Blood(c) => (Recipe::Droplets, c, None),
    }
}

/// The colour of a beam of this kind.
fn beam_color(kind: VfxKind) -> Color {
    let rgb = Color::from_rgb;
    match kind {
        VfxKind::Fire => rgb(1.0, 0.48, 0.13),
        VfxKind::Frost => rgb(0.56, 0.88, 1.0),
        VfxKind::Lightning => rgb(0.91, 0.94, 1.0),
        VfxKind::Poison => rgb(0.56, 1.0, 0.25),
        VfxKind::Zap(c) | VfxKind::Cast(c) | VfxKind::Quaff(c) | VfxKind::Explosion(c) => c,
        _ => rgb(0.38, 0.56, 1.0),
    }
}

struct Emitter {
    node: Gd<GpuParticles3D>,
    process: Gd<ParticleProcessMaterial>,
    /// Seconds until it may be used again.
    busy: f32,
}

struct Flash {
    light: Gd<OmniLight3D>,
    energy: f32,
    left: f32,
    total: f32,
}

struct Beam {
    node: Gd<MeshInstance3D>,
    mat: Gd<ShaderMaterial>,
    light: Gd<OmniLight3D>,
    from: Vector3,
    to: Vector3,
    age: f32,
    flicker: bool,
    /// A whole ray (the hero's), not a cell's piece of one.
    whole: bool,
}

struct Splat {
    decal: Gd<Decal>,
    age: f32,
}

/// A burst waiting for its moment (a blow lands a little after it starts).
struct Pending {
    kind: VfxKind,
    at: Vector3,
    left: f32,
    tint: Option<Color>,
    /// Which way the particles go (None: the recipe's).
    dir: Option<Vector3>,
}

/// Emitters kept per recipe.
const POOL: usize = 6;
/// Splats kept on the floor at once; each fades out over its life.
const SPLATS: usize = 24;
const SPLAT_SECS: f32 = 14.0;
const BEAM_SECS: f32 = 0.45;

pub struct Vfx {
    root: Gd<Node3D>,
    soft: Gd<Texture2D>,
    splat: Gd<Texture2D>,
    emitters: HashMap<Recipe, Vec<Emitter>>,
    flashes: Vec<Flash>,
    beams: Vec<Beam>,
    splats: Vec<Splat>,
    next_splat: usize,
    pending: Vec<Pending>,
    beam_shader: Option<Gd<Shader>>,
    dust: Gd<GpuParticles3D>,
    /// Decals land on these render layers only (the level, not models).
    decal_mask: u32,
    clock: f32,
    /// Cells a ray stops at, and how high their solid stands.
    solids: HashMap<(i32, i32), f32>,
}

/// A white disc fading out from the middle.
pub fn soft_texture() -> Gd<Texture2D> {
    let n = 64;
    texture(n, |x, y| {
        let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
        let a = (1.0 - d).clamp(0.0, 1.0);
        a * a
    })
}

/// A soft ring with a faint inner glow, for markers on the ground.
pub fn ring_texture() -> Gd<Texture2D> {
    texture(128, |x, y| {
        let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
        let ring = (-((d - 0.84) / 0.07).powi(2)).exp();
        let inner = (1.0 - d).clamp(0.0, 1.0).powi(2) * 0.18;
        (ring + inner).min(1.0)
    })
}

/// A rounded square outline, for the cell under the pointer.
pub fn frame_texture() -> Gd<Texture2D> {
    texture(128, |x, y| {
        let (qx, qy) = ((x - 0.5).abs() * 2.0, (y - 0.5).abs() * 2.0);
        let r = 0.18;
        let (dx, dy) = ((qx - (1.0 - r)).max(0.0), (qy - (1.0 - r)).max(0.0));
        let d = (dx * dx + dy * dy).sqrt() + qx.max(qy).min(1.0 - r) - (1.0 - r);
        let edge = (-((d - (r - 0.06)) / 0.035).powi(2)).exp();
        let inside = if d < r - 0.06 { 0.08 } else { 0.0 };
        (edge + inside).min(1.0)
    })
}

/// An irregular splat: a blob with droplets around it.
fn splat_texture() -> Gd<Texture2D> {
    let drops = [
        (0.78, 0.3, 0.06),
        (0.2, 0.72, 0.05),
        (0.7, 0.8, 0.04),
        (0.25, 0.25, 0.035),
        (0.88, 0.6, 0.03),
    ];
    texture(128, move |x, y| {
        let (dx, dy) = (x - 0.5, y - 0.5);
        let ang = dy.atan2(dx);
        let r = 0.26 + 0.05 * (ang * 5.0).sin() + 0.03 * (ang * 9.0 + 1.3).sin();
        let d = (dx * dx + dy * dy).sqrt();
        let mut a = ((r - d) / 0.03).clamp(0.0, 1.0);
        for (cx, cy, cr) in drops {
            let dd = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            a = a.max(((cr - dd) / 0.012).clamp(0.0, 1.0));
        }
        a * 0.9
    })
}

/// An n x n texture whose alpha and grey are `alpha(u, v)`.
fn texture(n: i32, alpha: impl Fn(f32, f32) -> f32) -> Gd<Texture2D> {
    let mut bytes = PackedByteArray::new();
    bytes.resize((n * n * 4) as usize);
    let out = bytes.as_mut_slice();
    for y in 0..n {
        for x in 0..n {
            let a = alpha((x as f32 + 0.5) / n as f32, (y as f32 + 0.5) / n as f32);
            let i = ((y * n + x) * 4) as usize;
            // white where it shows, black where not: an emission map of it
            // shows nothing outside its shape either
            let v = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            out[i..i + 4].copy_from_slice(&[v, v, v, v]);
        }
    }
    let image = Image::create_from_data(n, n, false, Format::RGBA8, &bytes);
    image
        .and_then(|i| ImageTexture::create_from_image(&i))
        .map(|t| t.upcast())
        .unwrap_or_else(|| ImageTexture::new_gd().upcast())
}

/// A billboard particle material: the soft disc in the particles' colour.
fn particle_material(tex: &Gd<Texture2D>, additive: bool, shaded: bool) -> Gd<Material> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_billboard_mode(BillboardMode::PARTICLES);
    m.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
    m.set_texture(godot::classes::base_material_3d::TextureParam::ALBEDO, tex);
    m.set_transparency(Transparency::ALPHA);
    if additive {
        m.set_blend_mode(BlendMode::ADD);
    }
    if !shaded {
        m.set_shading_mode(ShadingMode::UNSHADED);
    }
    m.set_flag(Flags::DISABLE_FOG, !shaded);
    m.upcast()
}

fn ramp(color: Color, hdr: f32) -> Gd<GradientTexture1D> {
    let mut g = Gradient::new_gd();
    let hot = Color::from_rgba(color.r * hdr, color.g * hdr, color.b * hdr, 1.0);
    // it cools in its own colour, never whiter or redder
    let mid = Color::from_rgba(
        color.r * hdr * 0.55,
        color.g * hdr * 0.55,
        color.b * hdr * 0.55,
        0.75,
    );
    let end = Color::from_rgba(color.r * 0.3, color.g * 0.3, color.b * 0.3, 0.0);
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.45, 1.0][..]));
    g.set_colors(&PackedColorArray::from(&[hot, mid, end][..]));
    let mut t = GradientTexture1D::new_gd();
    t.set_gradient(&g);
    t.set_use_hdr(true);
    t
}

impl Vfx {
    /// Effects live under `root`; decals land on `decal_mask` layers.
    pub fn new(mut root: Gd<Node3D>, decal_mask: u32) -> Vfx {
        let soft = soft_texture();
        let mut node = Node3D::new_alloc();
        node.set_name("Vfx");
        root.add_child(&node);
        let dust = dust(&mut node, &soft);
        let beam_shader = godot::tools::try_load::<Shader>("res://shaders/beam.gdshader").ok();
        Vfx {
            root: node,
            splat: splat_texture(),
            soft,
            emitters: HashMap::new(),
            flashes: Vec::new(),
            beams: Vec::new(),
            splats: Vec::new(),
            next_splat: 0,
            pending: Vec::new(),
            beam_shader,
            dust,
            decal_mask,
            clock: 0.0,
            solids: HashMap::new(),
        }
    }

    /// An effect at a point (its middle; the ground for blood).
    pub fn burst(&mut self, kind: VfxKind, at: Vector3) {
        self.burst_as(kind, at, None);
    }

    /// An effect in another colour (a potion's appearance, a neutral
    /// arcane colour for a wand the hero does not know).
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn burst_tinted(&mut self, kind: VfxKind, at: Vector3, color: Color) {
        self.burst_as(kind, at, Some(color));
    }

    fn burst_as(&mut self, kind: VfxKind, at: Vector3, tint: Option<Color>) {
        self.burst_dir(kind, at, tint, None);
    }

    /// A blow landing at `at`, its sparks thrown out towards `toward` (the
    /// striker's side) after `secs`.
    pub fn hit(&mut self, at: Vector3, toward: Vector3, secs: f32) {
        self.pending.push(Pending {
            kind: VfxKind::Sparks,
            at,
            left: secs,
            tint: None,
            dir: Some((toward.normalized() + Vector3::UP * 0.6).normalized()),
        });
    }

    fn burst_dir(&mut self, kind: VfxKind, at: Vector3, tint: Option<Color>, dir: Option<Vector3>) {
        let (recipe, color, light) = look_of(kind);
        let color = tint.unwrap_or(color);
        let light = light.map(|(c, e, s)| (tint.unwrap_or(c), e, s));
        self.emit(recipe, color, at, dir);
        if recipe == Recipe::Sparks {
            self.emit(Recipe::Flash, color, at, None);
        }
        if let Some((c, energy, secs)) = light {
            self.flash(at + Vector3::new(0.0, 0.3, 0.0), c, energy, secs);
        }
        if let VfxKind::Blood(c) = kind {
            self.splat(Vector3::new(at.x, 0.0, at.z), c);
        }
        if let VfxKind::Explosion(c) = kind {
            // the scorch of the blast, fading like blood
            self.splat(
                Vector3::new(at.x, 0.0, at.z),
                Color::from_rgb(0.05, 0.04, 0.035),
            );
            let _ = c;
        }
    }

    /// An effect a little later (seconds).
    pub fn burst_after(&mut self, kind: VfxKind, at: Vector3, secs: f32) {
        self.pending.push(Pending {
            kind,
            at,
            left: secs,
            tint: None,
            dir: None,
        });
    }

    /// A ray from one point to another, a light travelling along it, and
    /// the kind's burst where it ends.
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn beam(&mut self, kind: VfxKind, from: Vector3, to: Vector3) {
        self.beam_as(kind, from, to, None);
    }

    /// A ray in another colour (see `burst_tinted`).
    #[allow(dead_code)] // for the hero's item use (Phase E)
    pub fn beam_tinted(&mut self, kind: VfxKind, from: Vector3, to: Vector3, color: Color) {
        self.beam_as(kind, from, to, Some(color));
    }

    fn beam_as(&mut self, kind: VfxKind, from: Vector3, to: Vector3, tint: Option<Color>) {
        // a ray stops at the first wall, door or rock in its way
        let to = self.clip(from, to);
        self.place_ray(tint.unwrap_or_else(|| beam_color(kind)), from, to, true);
        self.pending.push(Pending {
            kind,
            at: to,
            left: BEAM_SECS * 0.4,
            tint,
            dir: None,
        });
    }

    /// A ray of this colour from one point to another, a light travelling
    /// along it, with no burst (the map's zaps, cell by cell). A piece of
    /// a ray already drawn whole (the hero's own zap) is not drawn again.
    pub fn ray(&mut self, color: Color, from: Vector3, to: Vector3) {
        let mid = (from + to) / 2.0;
        let covered = self
            .beams
            .iter()
            .any(|b| b.whole && b.age < BEAM_SECS && distance_to_segment(mid, b.from, b.to) < 0.6);
        if !covered {
            self.place_ray(color, from, to, false);
        }
    }

    /// The solid cells a ray stops at: (cell, how high the solid stands).
    pub fn set_solid(&mut self, cell: (i32, i32), top: Option<f32>) {
        match top {
            Some(t) => {
                self.solids.insert(cell, t);
            }
            None => {
                self.solids.remove(&cell);
            }
        }
    }

    /// Where a ray from `from` towards `to` first meets a solid cell (not
    /// the one it starts in), or `to` (where its impact belongs).
    pub fn clip(&self, from: Vector3, to: Vector3) -> Vector3 {
        let cell = |p: Vector3| (p.x.round() as i32, p.z.round() as i32);
        let start = cell(from);
        let len = from.distance_to(to);
        let steps = (len / 0.05).ceil().max(1.0) as i32;
        let mut last = from;
        for i in 1..=steps {
            let p = from.lerp(to, i as f32 / steps as f32);
            let c = cell(p);
            if c != start && self.solids.get(&c).is_some_and(|&top| p.y < top) {
                return last;
            }
            last = p;
        }
        to
    }

    fn place_ray(&mut self, color: Color, from: Vector3, to: Vector3, whole: bool) {
        let i = match self.beams.iter().position(|b| b.age >= BEAM_SECS) {
            Some(i) => i,
            None => {
                let beam = self.new_beam();
                self.beams.push(beam);
                self.beams.len() - 1
            }
        };
        let b = &mut self.beams[i];
        b.from = from;
        b.to = to;
        b.age = 0.0;
        b.whole = whole;
        // lightning's light is white-blue and jitters
        b.flicker = color.b > 0.95 && color.r > 0.85 && color.g > 0.9;
        let len = from.distance_to(to).max(0.05);
        let mid = (from + to) / 2.0;
        let dir = (to - from) / len;
        // the ribbon's y is the ray, stretched; the shader turns it to
        // the camera
        let up = if dir.y.abs() > 0.99 {
            Vector3::RIGHT
        } else {
            Vector3::UP
        };
        let x = up.cross(dir).normalized();
        let z = x.cross(dir);
        let basis = Basis::from_cols(x, dir * len, z);
        b.mat.set_shader_parameter("head", &0.0f32.to_variant());
        b.node.set_transform(Transform3D::new(basis, mid));
        b.node.set_visible(true);
        b.mat.set_shader_parameter("color", &color.to_variant());
        b.light.set_color(color);
        b.light.set_position(from);
        b.light.set_visible(true);
    }

    fn new_beam(&mut self) -> Beam {
        let mut node = MeshInstance3D::new_alloc();
        let mut ribbon = QuadMesh::new_gd();
        ribbon.set_size(Vector2::new(1.0, 1.0));
        node.set_mesh(&ribbon);
        // the ribbon is placed in the shader: never culled by its quad
        node.set_custom_aabb(Aabb::new(
            Vector3::new(-2.0, -2.0, -2.0),
            Vector3::new(4.0, 4.0, 4.0),
        ));
        node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        let mut mat = ShaderMaterial::new_gd();
        if let Some(s) = &self.beam_shader {
            mat.set_shader(s);
        }
        node.set_material_override(&mat);
        self.root.add_child(&node);
        let mut light = OmniLight3D::new_alloc();
        light.set_param(Param::RANGE, 4.0);
        light.set_param(Param::ENERGY, 3.0);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.0);
        light.set_shadow(false);
        self.root.add_child(&light);
        Beam {
            node,
            mat,
            light,
            from: Vector3::ZERO,
            to: Vector3::ZERO,
            age: BEAM_SECS,
            flicker: false,
            whole: false,
        }
    }

    fn emit(&mut self, recipe: Recipe, color: Color, at: Vector3, dir: Option<Vector3>) {
        let pool = self.emitters.entry(recipe).or_default();
        let i = match pool.iter().position(|e| e.busy <= 0.0) {
            Some(i) => i,
            None if pool.len() < POOL => {
                let e = new_emitter(&mut self.root, recipe, &self.soft);
                pool.push(e);
                pool.len() - 1
            }
            // all busy: the oldest starts again
            None => pool
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.busy.total_cmp(&b.1.busy))
                .map_or(0, |(i, _)| i),
        };
        let p = recipe.params();
        let e = &mut pool[i];
        e.process.set_color_ramp(&ramp(color, p.hdr));
        e.process.set_direction(dir.unwrap_or(p.dir));
        e.node.set_position(at);
        e.node.restart();
        e.node.set_emitting(true);
        e.busy = p.life as f32 + 0.1;
    }

    fn flash(&mut self, at: Vector3, color: Color, energy: f32, secs: f32) {
        let i = match self.flashes.iter().position(|f| f.left <= 0.0) {
            Some(i) => i,
            None => {
                let mut light = OmniLight3D::new_alloc();
                light.set_param(Param::RANGE, 2.5);
                light.set_param(Param::ATTENUATION, 1.2);
                light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 1.0);
                light.set_shadow(false);
                self.root.add_child(&light);
                self.flashes.push(Flash {
                    light,
                    energy: 0.0,
                    left: 0.0,
                    total: 0.0,
                });
                self.flashes.len() - 1
            }
        };
        let f = &mut self.flashes[i];
        f.light.set_color(color);
        f.light.set_position(at);
        f.light.set_param(Param::ENERGY, energy);
        f.light.set_visible(true);
        f.energy = energy;
        f.left = secs;
        f.total = secs;
    }

    fn splat(&mut self, at: Vector3, color: Color) {
        if self.splats.len() < SPLATS {
            let mut decal = Decal::new_alloc();
            decal.set_texture(godot::classes::decal::DecalTexture::ALBEDO, &self.splat);
            decal.set_cull_mask(self.decal_mask);
            decal.set_albedo_mix(1.0);
            self.root.add_child(&decal);
            self.splats.push(Splat { decal, age: 0.0 });
        }
        let i = self.next_splat % self.splats.len();
        self.next_splat += 1;
        let s = &mut self.splats[i];
        // turned and sized by where it falls, the same every time
        let h = ((at.x * 12.9898 + at.z * 78.233).sin() * 43_758.547)
            .fract()
            .abs();
        let size = 0.4 + 0.3 * h;
        s.decal.set_size(Vector3::new(size, 0.5, size));
        s.decal
            .set_rotation(Vector3::new(0.0, h * std::f32::consts::TAU, 0.0));
        s.decal.set_position(at);
        s.decal.set_modulate(color);
        s.decal.set_visible(true);
        s.age = 0.0;
    }

    /// Advance everything by `delta` seconds; the dust follows `focus`.
    pub fn process(&mut self, delta: f32, focus: Vector3) {
        self.clock += delta;
        self.dust.set_position(focus + Vector3::new(0.0, 1.2, 0.0));
        for e in self.emitters.values_mut().flatten() {
            e.busy -= delta;
        }
        for f in &mut self.flashes {
            if f.left > 0.0 {
                f.left -= delta;
                let k = (f.left / f.total.max(1e-3)).clamp(0.0, 1.0);
                f.light.set_param(Param::ENERGY, f.energy * k);
                if f.left <= 0.0 {
                    f.light.set_visible(false);
                }
            }
        }
        let clock = self.clock;
        for b in &mut self.beams {
            if b.age >= BEAM_SECS {
                continue;
            }
            b.age += delta;
            let t = (b.age / BEAM_SECS).clamp(0.0, 1.0);
            // the head of the ray runs ahead, then the ray fades
            let head = (t / 0.4).min(1.0);
            b.light.set_position(b.from.lerp(b.to, head));
            b.mat.set_shader_parameter("head", &head.to_variant());
            let mut fade = 1.0 - ((t - 0.4) / 0.6).clamp(0.0, 1.0);
            if b.flicker {
                fade *= 0.55 + 0.45 * (clock * 20.0 * std::f32::consts::TAU).sin().abs();
            }
            b.mat.set_shader_parameter("fade", &fade.to_variant());
            b.light.set_param(Param::ENERGY, 3.0 * fade);
            if b.age >= BEAM_SECS {
                b.node.set_visible(false);
                b.light.set_visible(false);
            }
        }
        for s in &mut self.splats {
            if s.age < SPLAT_SECS {
                s.age += delta;
                let fade = 1.0 - ((s.age - SPLAT_SECS * 0.6) / (SPLAT_SECS * 0.4)).clamp(0.0, 1.0);
                let mut c = s.decal.get_modulate();
                c.a = fade;
                s.decal.set_modulate(c);
                if s.age >= SPLAT_SECS {
                    s.decal.set_visible(false);
                }
            }
        }
        let mut due = Vec::new();
        self.pending.retain_mut(|p| {
            p.left -= delta;
            if p.left <= 0.0 {
                due.push((p.kind, p.at, p.tint, p.dir));
                false
            } else {
                true
            }
        });
        for (kind, at, tint, dir) in due {
            self.burst_dir(kind, at, tint, dir);
        }
    }

    /// Forget every effect under way (a new level, a new game).
    pub fn clear(&mut self) {
        self.pending.clear();
        for e in self.emitters.values_mut().flatten() {
            e.node.set_emitting(false);
            e.busy = 0.0;
        }
        for f in &mut self.flashes {
            f.left = 0.0;
            f.light.set_visible(false);
        }
        for b in &mut self.beams {
            b.age = BEAM_SECS;
            b.node.set_visible(false);
            b.light.set_visible(false);
        }
        for s in &mut self.splats {
            s.age = SPLAT_SECS;
            s.decal.set_visible(false);
        }
    }

    /// Embers rising from a flame: a small emitter to put under a torch.
    pub fn embers(&self) -> Gd<GpuParticles3D> {
        let mut p = GpuParticles3D::new_alloc();
        let mut m = ParticleProcessMaterial::new_gd();
        m.set_emission_shape(EmissionShape::SPHERE);
        m.set_emission_sphere_radius(0.04);
        m.set_direction(Vector3::UP);
        m.set_spread(18.0);
        m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.3);
        m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.7);
        m.set_gravity(Vector3::new(0.0, 0.15, 0.0));
        m.set_param_min(Parameter::SCALE, 0.65);
        m.set_param_max(Parameter::SCALE, 1.35);
        m.set_turbulence_enabled(true);
        m.set_turbulence_noise_strength(0.4);
        m.set_turbulence_noise_scale(1.5);
        m.set_color_ramp(&ramp(Color::from_rgb(1.0, 0.62, 0.25), 4.0));
        p.set_process_material(&m);
        p.set_amount(14);
        p.set_lifetime(1.4);
        p.set_randomness_ratio(0.5);
        p.set_draw_pass_mesh(0, &quad(0.035));
        p.set_material_override(&particle_material(&self.soft, true, false));
        p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        p.set_visibility_aabb(Aabb::new(
            Vector3::new(-0.5, -0.2, -0.5),
            Vector3::new(1.0, 1.6, 1.0),
        ));
        p
    }
}

fn distance_to_segment(p: Vector3, a: Vector3, b: Vector3) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance_to(a + ab * t)
}

fn quad(size: f32) -> Gd<QuadMesh> {
    let mut q = QuadMesh::new_gd();
    q.set_size(Vector2::new(size, size));
    q
}

fn new_emitter(root: &mut Gd<Node3D>, recipe: Recipe, soft: &Gd<Texture2D>) -> Emitter {
    let p = recipe.params();
    let mut m = ParticleProcessMaterial::new_gd();
    m.set_emission_shape(EmissionShape::SPHERE);
    m.set_emission_sphere_radius(p.radius);
    m.set_direction(p.dir);
    m.set_spread(p.spread);
    m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, p.speed.0);
    m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, p.speed.1);
    m.set_gravity(p.gravity);
    m.set_param_min(Parameter::DAMPING, p.damping);
    m.set_param_max(Parameter::DAMPING, p.damping);
    // the quad is the particle's mean size and the scale varies it (a
    // scale of centimetres on a metre quad is not what Godot draws)
    let mean = (p.size.0 + p.size.1) / 2.0;
    m.set_param_min(Parameter::SCALE, p.size.0 / mean);
    m.set_param_max(Parameter::SCALE, p.size.1 / mean);
    if recipe == Recipe::Swirl {
        m.set_param_min(Parameter::ORBIT_VELOCITY, 0.4);
        m.set_param_max(Parameter::ORBIT_VELOCITY, 0.8);
    }
    let mut node = GpuParticles3D::new_alloc();
    node.set_process_material(&m);
    node.set_amount(p.amount);
    node.set_lifetime(p.life);
    node.set_one_shot(true);
    node.set_explosiveness_ratio(
        if matches!(recipe, Recipe::Motes | Recipe::Runes | Recipe::Cloud) {
            0.6
        } else {
            1.0
        },
    );
    node.set_emitting(false);
    if recipe == Recipe::Sparks {
        // a streak along its velocity, facing the camera round it
        let mut streak = QuadMesh::new_gd();
        streak.set_size(Vector2::new(mean * 0.9, mean * 4.0));
        node.set_draw_pass_mesh(0, &streak);
        node.set_transform_align(TransformAlign::Z_BILLBOARD_Y_TO_VELOCITY);
        let mut m = particle_material(soft, true, false);
        if let Ok(mut b) = m.clone().try_cast::<StandardMaterial3D>() {
            b.set_billboard_mode(BillboardMode::DISABLED);
            m = b.upcast();
        }
        node.set_material_override(&m);
    } else {
        node.set_draw_pass_mesh(0, &quad(mean));
        node.set_material_override(&particle_material(soft, p.additive, !p.additive));
    }
    node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    node.set_visibility_aabb(Aabb::new(
        Vector3::new(-3.0, -2.0, -3.0),
        Vector3::new(6.0, 5.0, 6.0),
    ));
    root.add_child(&node);
    Emitter {
        node,
        process: m,
        busy: 0.0,
    }
}

/// Motes in the air around the camera's focus: lit like everything else,
/// so they show only in the light of a torch or the hero's.
fn dust(root: &mut Gd<Node3D>, soft: &Gd<Texture2D>) -> Gd<GpuParticles3D> {
    let mut m = ParticleProcessMaterial::new_gd();
    m.set_emission_shape(EmissionShape::BOX);
    m.set_emission_box_extents(Vector3::new(7.0, 1.25, 4.5));
    m.set_direction(Vector3::UP);
    m.set_spread(180.0);
    m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.01);
    m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.05);
    m.set_gravity(Vector3::new(0.0, -0.005, 0.0));
    m.set_param_min(Parameter::SCALE, 0.6);
    m.set_param_max(Parameter::SCALE, 1.25);
    m.set_turbulence_enabled(true);
    m.set_turbulence_noise_strength(0.2);
    m.set_turbulence_noise_scale(4.0);
    let mut g = Gradient::new_gd();
    let c = Color::from_rgba(0.9, 0.86, 0.78, 0.35);
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.2, 0.8, 1.0][..]));
    g.set_colors(&PackedColorArray::from(
        &[c.with_alpha(0.0), c, c, c.with_alpha(0.0)][..],
    ));
    let mut ramp = GradientTexture1D::new_gd();
    ramp.set_gradient(&g);
    m.set_color_ramp(&ramp);
    let mut p = GpuParticles3D::new_alloc();
    p.set_name("Dust");
    p.set_process_material(&m);
    p.set_amount(220);
    p.set_lifetime(8.0);
    p.set_pre_process_time(8.0);
    p.set_use_local_coordinates(false);
    p.set_draw_pass_mesh(0, &quad(0.02));
    p.set_material_override(&particle_material(soft, false, true));
    p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    p.set_visibility_aabb(Aabb::new(
        Vector3::new(-9.0, -3.0, -7.0),
        Vector3::new(18.0, 6.0, 14.0),
    ));
    root.add_child(&p);
    p
}
