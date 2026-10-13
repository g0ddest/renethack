//! The map's effects: bursts of particles (hit sparks, blood, the magic of
//! quaffing, reading, zapping and casting, fire, frost, lightning, poison,
//! explosions, healing, sparkles, a ring of force) with a short flash of
//! light, rays drawn as what they are of (`Ray`: a stream of flame, an
//! icy core shedding shards, a drifting haze, darts of light, a black ray,
//! a spray, rock dust, rings of force) with a light on them and particles
//! shed along them, blood splats on the floor that fade, and the dust in
//! the air that shows only where light falls. Emitters, lights and decals
//! are pooled per kind; nothing here is ever more than a picture of what
//! the game already said.
//!
//! Other parts of the client call `Vfx::burst(kind, at)` and
//! `Vfx::beam(kind, from, to)` (world positions), `Vfx::ray` for the
//! pieces of a ray the map draws cell by cell, or the map's cell helpers
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
    Curve, CurveTexture, Decal, GpuParticles3D, Gradient, GradientTexture1D, Image, ImageTexture,
    Material, MeshInstance3D, Node3D, OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader,
    ShaderMaterial, StandardMaterial3D, Texture2D,
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
    /// A ring of force spreading where a bolt of force (or magic the
    /// client cannot tell) strikes, of this colour.
    Shock(Color),
}

/// What a ray is of: NetHack's beam types (`ZT_*`), a wand of digging's
/// beam, and the force of a bolt that is no ray. Each has its own look
/// (beam.gdshader's `style`) and sheds its own particles along its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ray {
    /// What the client cannot tell: a glowing tube of its colour.
    Plain,
    /// Darts of light.
    MagicMissile,
    /// A stream of flame.
    Fire,
    /// An icy core in a glittering mist, shedding shards of ice.
    Cold,
    /// A soft haze drifting.
    Sleep,
    /// Black, eating the light (death, disintegration).
    Death,
    /// A jagged bolt with forks.
    Lightning,
    /// Billows of noxious gas.
    Poison,
    /// A spray of acid.
    Acid,
    /// Rock dust (a wand of digging).
    Dig,
    /// Rings of force (force bolt, a wand of striking).
    Force,
}

impl Ray {
    /// Every kind (the effects' rehearsal draws each once).
    pub const ALL: [Ray; 11] = [
        Ray::Plain,
        Ray::MagicMissile,
        Ray::Fire,
        Ray::Cold,
        Ray::Sleep,
        Ray::Death,
        Ray::Lightning,
        Ray::Poison,
        Ray::Acid,
        Ray::Dig,
        Ray::Force,
    ];

    /// The ray of a zap glyph's beam type: NetHack's `ZT_*`, the glyph's
    /// offset from the first zap glyph over four (its directions).
    pub fn of_zap(beam: i32) -> Ray {
        match beam {
            0 => Ray::MagicMissile,
            1 => Ray::Fire,
            2 => Ray::Cold,
            3 => Ray::Sleep,
            4 => Ray::Death,
            5 => Ray::Lightning,
            6 => Ray::Poison,
            7 => Ray::Acid,
            _ => Ray::Plain,
        }
    }

    /// The shader's style for it (beam.gdshader's constants).
    fn style(self) -> i32 {
        match self {
            Ray::Plain => 0,
            Ray::MagicMissile => 1,
            Ray::Fire => 2,
            Ray::Cold => 3,
            Ray::Sleep => 4,
            Ray::Death => 5,
            Ray::Lightning => 6,
            Ray::Acid => 7,
            Ray::Dig => 8,
            Ray::Force => 9,
            Ray::Poison => 10,
        }
    }

    fn look(self) -> RayLook {
        let rgb = |r, g, b| Some(Color::from_rgb(r, g, b));
        let plain = RayLook {
            color: None,
            width: RAY_WIDTH,
            light: 3.0,
            secs: BEAM_SECS,
            trails: &[],
        };
        match self {
            Ray::Plain => plain,
            Ray::MagicMissile => RayLook {
                width: 0.55,
                light: 2.5,
                secs: 0.5,
                trails: &[Recipe::Twinkles],
                ..plain
            },
            Ray::Fire => RayLook {
                color: rgb(1.0, 0.48, 0.13),
                width: 0.8,
                light: 4.0,
                secs: 0.6,
                trails: &[Recipe::Tongues],
            },
            Ray::Cold => RayLook {
                color: rgb(0.62, 0.86, 1.0),
                width: 0.6,
                light: 2.2,
                secs: 0.6,
                trails: &[Recipe::Rime],
            },
            Ray::Sleep => RayLook {
                color: rgb(0.6, 0.56, 1.0),
                width: 1.0,
                light: 1.0,
                secs: 1.4,
                trails: &[Recipe::Haze],
            },
            // its light darkens what is round it
            Ray::Death => RayLook {
                color: rgb(0.62, 0.34, 0.95),
                width: 0.6,
                light: -2.5,
                secs: 0.6,
                trails: &[Recipe::Smoke],
            },
            Ray::Lightning => RayLook {
                color: rgb(0.91, 0.94, 1.0),
                width: BOLT_WIDTH,
                ..plain
            },
            Ray::Poison => RayLook {
                color: rgb(0.74, 0.93, 0.25),
                width: 0.9,
                light: 1.0,
                secs: 1.4,
                trails: &[Recipe::Gas],
            },
            Ray::Acid => RayLook {
                color: rgb(0.42, 1.0, 0.3),
                width: 0.8,
                light: 1.2,
                secs: 0.7,
                trails: &[Recipe::Spray],
            },
            Ray::Dig => RayLook {
                color: rgb(0.42, 0.36, 0.3),
                width: 0.9,
                light: 0.0,
                secs: 1.2,
                trails: &[Recipe::Dust, Recipe::Grit],
            },
            Ray::Force => RayLook {
                color: rgb(0.78, 0.86, 1.0),
                width: 0.6,
                light: 1.5,
                secs: 0.5,
                ..plain
            },
        }
    }
}

/// How a ray of a kind is drawn: its colour (None: the one it is drawn
/// in), its ribbon's width (metres), its light's energy (below zero it
/// darkens what is round it; zero, none), how long it shows once it stops
/// growing (seconds: a haze, gas and dust linger) and what it sheds along
/// its way.
struct RayLook {
    color: Option<Color>,
    width: f32,
    light: f32,
    secs: f32,
    trails: &'static [Recipe],
}

/// What an explosion is of: NetHack's `EXPL_*`, each its own look. A
/// fire's heart is a tower of flame (a scroll of fire's), the rest of it
/// rolling fire; frost bursts in shards and a cold mist, gas and darkness
/// in clouds, water and mud in a spray, magic in a blast of sparks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Blast {
    Dark,
    Noxious,
    Muddy,
    Wet,
    Magical,
    Fiery,
    Frosty,
}

impl Blast {
    /// The explosion of a type: an explosion glyph's offset from the first
    /// one over nine (the cells of an explosion's 3 x 3).
    pub fn of_type(t: i32) -> Blast {
        match t {
            0 => Blast::Dark,
            1 => Blast::Noxious,
            2 => Blast::Muddy,
            3 => Blast::Wet,
            4 => Blast::Magical,
            6 => Blast::Frosty,
            _ => Blast::Fiery,
        }
    }
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
    /// A ring spreading out and fading.
    Ring,
    /// A column of rolling fire rising (a fire's heart).
    Pillar,
    /// Rolling fire, rising (a fire's other cells).
    Blaze,
    /// Embers flung up and drifting.
    Embers,
    // shed along a ray once it has run its length (`Vfx::shed`):
    /// Tongues of flame rising.
    Tongues,
    /// Shards of ice falling, glinting.
    Rime,
    /// Soft puffs of haze, lingering.
    Haze,
    /// Specks of light twinkling.
    Twinkles,
    /// Black smoke fraying.
    Smoke,
    /// Puffs of noxious gas, lingering.
    Gas,
    /// Drops of acid falling.
    Spray,
    /// Rock dust billowing.
    Dust,
    /// Chips of rock falling.
    Grit,
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
    /// The ramp over a particle's life: hot at birth, fading out (a
    /// smoke's: how thick it is at its thickest, its alpha).
    hdr: f32,
    additive: bool,
    /// Shaded by the lights about it (dust, blood), not glowing.
    lit: bool,
}

impl Recipe {
    /// Shed along a ray (from a box its length), not burst at a point.
    fn is_trail(self) -> bool {
        matches!(
            self,
            Recipe::Tongues
                | Recipe::Rime
                | Recipe::Haze
                | Recipe::Twinkles
                | Recipe::Smoke
                | Recipe::Gas
                | Recipe::Spray
                | Recipe::Dust
                | Recipe::Grit
        )
    }

    /// Its own colour, whatever the ray's: smoke is black, dust and grit
    /// the colours of rock.
    fn own_color(self) -> Option<Color> {
        match self {
            Recipe::Smoke => Some(Color::from_rgb(0.05, 0.03, 0.07)),
            Recipe::Dust => Some(Color::from_rgb(0.62, 0.55, 0.47)),
            Recipe::Grit => Some(Color::from_rgb(0.3, 0.26, 0.22)),
            _ => None,
        }
    }

    /// A smoke or a haze: it veils what is behind it (its ramp's alpha is
    /// low and it lingers), it does not glow.
    fn is_smoke(self) -> bool {
        matches!(
            self,
            Recipe::Haze | Recipe::Smoke | Recipe::Gas | Recipe::Dust
        )
    }

    /// The frames of its texture's flipbook (columns, rows), played once
    /// over a particle's life.
    fn frames(self) -> Option<(i32, i32)> {
        match self {
            Recipe::Tongues
            | Recipe::Pillar
            | Recipe::Blaze
            | Recipe::Haze
            | Recipe::Smoke
            | Recipe::Gas
            | Recipe::Dust => Some((8, 8)),
            _ => None,
        }
    }

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
            lit: false,
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
                lit: true,
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
                lit: true,
                dir: Vector3::UP,
            },
            // one ring, spreading from a small one (its scale curve)
            Recipe::Ring => Params {
                amount: 1,
                life: 0.4,
                speed: (0.0, 0.0),
                spread: 0.0,
                size: (1.7, 1.7),
                radius: 0.0,
                damping: 0.0,
                hdr: 2.4,
                ..base
            },
            Recipe::Pillar => Params {
                amount: 56,
                life: 1.0,
                speed: (1.2, 3.4),
                spread: 12.0,
                gravity: Vector3::new(0.0, 0.8, 0.0),
                size: (0.55, 1.1),
                radius: 0.35,
                damping: 1.2,
                hdr: 0.85,
                ..base
            },
            Recipe::Blaze => Params {
                amount: 10,
                life: 0.8,
                speed: (0.4, 1.4),
                spread: 35.0,
                gravity: Vector3::new(0.0, 1.4, 0.0),
                size: (0.35, 0.7),
                radius: 0.3,
                damping: 1.5,
                hdr: 0.8,
                ..base
            },
            Recipe::Embers => Params {
                amount: 40,
                life: 1.4,
                speed: (0.6, 2.6),
                spread: 30.0,
                gravity: Vector3::new(0.0, 0.5, 0.0),
                size: (0.03, 0.06),
                radius: 0.45,
                damping: 0.6,
                hdr: 3.0,
                ..base
            },
            // shed along a ray: amounts are for a ray of `TRAIL_FULL`
            // metres, a shorter one sheds fewer; a ray shows for a moment,
            // so they come thick while it does
            Recipe::Tongues => Params {
                amount: 300,
                life: 0.8,
                speed: (0.2, 0.7),
                spread: 30.0,
                gravity: Vector3::new(0.0, 2.2, 0.0),
                size: (0.3, 0.6),
                damping: 1.2,
                hdr: 1.6,
                ..base
            },
            Recipe::Rime => Params {
                amount: 180,
                life: 1.1,
                speed: (0.05, 0.3),
                spread: 180.0,
                gravity: Vector3::new(0.0, -1.2, 0.0),
                size: (0.12, 0.24),
                damping: 0.6,
                hdr: 3.0,
                ..base
            },
            Recipe::Haze => Params {
                amount: 160,
                life: 2.2,
                speed: (0.05, 0.25),
                spread: 180.0,
                gravity: Vector3::new(0.0, 0.05, 0.0),
                size: (0.45, 0.8),
                damping: 0.5,
                hdr: 0.34,
                additive: false,
                ..base
            },
            Recipe::Twinkles => Params {
                amount: 160,
                life: 0.7,
                speed: (0.05, 0.3),
                spread: 180.0,
                size: (0.15, 0.3),
                hdr: 3.0,
                ..base
            },
            Recipe::Smoke => Params {
                amount: 130,
                life: 1.3,
                speed: (0.1, 0.35),
                spread: 50.0,
                gravity: Vector3::new(0.0, 0.35, 0.0),
                size: (0.35, 0.7),
                damping: 0.6,
                hdr: 0.7,
                additive: false,
                ..base
            },
            Recipe::Gas => Params {
                amount: 160,
                life: 2.0,
                speed: (0.05, 0.3),
                spread: 180.0,
                gravity: Vector3::new(0.0, 0.08, 0.0),
                size: (0.5, 0.9),
                damping: 0.5,
                hdr: 0.38,
                additive: false,
                ..base
            },
            Recipe::Spray => Params {
                amount: 220,
                life: 0.8,
                speed: (0.4, 1.2),
                spread: 55.0,
                gravity: Vector3::new(0.0, -7.0, 0.0),
                size: (0.04, 0.08),
                damping: 0.3,
                hdr: 1.6,
                ..base
            },
            Recipe::Dust => Params {
                amount: 180,
                life: 1.8,
                speed: (0.1, 0.4),
                spread: 180.0,
                gravity: Vector3::new(0.0, -0.05, 0.0),
                size: (0.5, 0.9),
                damping: 0.8,
                hdr: 0.65,
                additive: false,
                lit: true,
                ..base
            },
            Recipe::Grit => Params {
                amount: 150,
                life: 0.7,
                speed: (0.3, 1.0),
                spread: 70.0,
                gravity: Vector3::new(0.0, -9.8, 0.0),
                size: (0.025, 0.05),
                damping: 0.2,
                hdr: 1.0,
                additive: false,
                lit: true,
                ..base
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
        VfxKind::Shock(c) => (Recipe::Ring, c, Some((c, 2.0, 0.2))),
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
        VfxKind::Zap(c)
        | VfxKind::Cast(c)
        | VfxKind::Quaff(c)
        | VfxKind::Explosion(c)
        | VfxKind::Shock(c) => c,
        _ => rgb(0.38, 0.56, 1.0),
    }
}

/// What a beam of this kind is drawn as.
fn ray_of(kind: VfxKind) -> Ray {
    match kind {
        VfxKind::Fire => Ray::Fire,
        VfxKind::Frost => Ray::Cold,
        VfxKind::Lightning => Ray::Lightning,
        VfxKind::Poison => Ray::Poison,
        VfxKind::Shock(_) => Ray::Force,
        _ => Ray::Plain,
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
    /// How long it shows once it stops growing (its kind's).
    secs: f32,
    kind: Ray,
    color: Color,
    /// A whole ray (the hero's), not one the map draws piece by piece.
    whole: bool,
    /// Whether it has shed what its kind sheds along its way.
    shed: bool,
}

impl Beam {
    fn live(&self) -> bool {
        self.age < self.secs
    }
}

struct Splat {
    decal: Gd<Decal>,
    age: f32,
}

/// A tower of flame where a fire explodes (a scroll of fire's): flames
/// of a real flame's flipbook (the torches'), turned to the camera, that
/// shoot up and die down.
struct Column {
    root: Gd<Node3D>,
    flames: Vec<Gd<ShaderMaterial>>,
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

/// Emitters kept per recipe (an explosion's nine cells may each want
/// one).
const POOL: usize = 9;
/// How long a tower of flame burns (seconds), and the flames of one:
/// (width, height, across the cell, phase).
const COLUMN_SECS: f32 = 1.4;
const COLUMN_FLAMES: [(f32, f32, f32, f32); 3] = [
    (1.5, 3.4, 0.0, 0.0),
    (1.0, 2.3, -0.38, 0.37),
    (1.0, 2.1, 0.4, 0.71),
];
/// A fire that exploded this lately (seconds) was the scroll of fire the
/// hero read: the reading's runes do not follow its flames.
const FIRE_LATELY: f32 = 2.0;
/// Splats kept on the floor at once; each fades out over its life.
const SPLATS: usize = 24;
/// How much darker a splat lies than the creature's own colour.
const SPLAT_DARK: f32 = 0.5;
const SPLAT_SECS: f32 = 14.0;
const BEAM_SECS: f32 = 0.45;
/// How long a whole ray's head takes to run its length, and a ray shows
/// at its brightest once it stops growing, before it fades.
const RUN_SECS: f32 = BEAM_SECS * 0.4;
/// A ray the map draws is whole once no piece has come for this long
/// (seconds): then it sheds what it sheds.
const SHED_AFTER: f32 = 0.15;
/// A ray this long (metres) or longer sheds a trail's whole amount.
const TRAIL_FULL: f32 = 8.0;
/// Pieces of a ray drawn by the map this lately (seconds) were the ray of
/// the hero's own zap, shown before the hero's use is (`ray_lately`).
const RAY_LATELY: f32 = 1.5;
/// A ray's ribbon is this wide (metres); a bolt of lightning's, wider (its
/// jags swing out of the ray's line).
const RAY_WIDTH: f32 = 0.42;
const BOLT_WIDTH: f32 = 1.1;

/// Particle textures (CC0) by recipe: RPicster's starburst for a blow's
/// flare, his stars for glints, flares and runes; Kenney's ring for a
/// ring of force and his stars for ice and twinkles; Unity Labs' flipbooks
/// of rolling fire and of wisps of smoke for flames, smoke, haze, gas and
/// dust.
fn particle_textures() -> HashMap<Recipe, Gd<Texture2D>> {
    let rpicster = "res://art/cc0/rpicster/256/";
    let kenney = "res://art/cc0/kenney/particles/";
    let books = "res://art/cc0/unity-labs/flipbooks/";
    let smoke = format!("{books}WispySmoke01_8x8.png");
    [
        (Recipe::Flash, format!("{rpicster}effect_3.png")),
        (Recipe::Glints, format!("{rpicster}spotlight_5.png")),
        (Recipe::Flare, format!("{rpicster}spotlight_5.png")),
        (Recipe::Runes, format!("{rpicster}spotlight_8.png")),
        (Recipe::Ring, format!("{kenney}circle_02.png")),
        (Recipe::Rime, format!("{kenney}star_04.png")),
        (Recipe::Twinkles, format!("{kenney}star_01.png")),
        (Recipe::Tongues, format!("{books}FireBall04_8x8.png")),
        (Recipe::Pillar, format!("{books}FireBall01_8x8.png")),
        (Recipe::Blaze, format!("{books}FireBall02_8x8.png")),
        (Recipe::Haze, smoke.clone()),
        (Recipe::Smoke, smoke.clone()),
        (Recipe::Gas, smoke.clone()),
        (Recipe::Dust, smoke),
    ]
    .into_iter()
    .filter_map(|(r, path)| {
        godot::tools::try_load::<Texture2D>(&path)
            .ok()
            .map(|t| (r, t))
    })
    .collect()
}
/// Motes of dust around the camera's focus in the main dungeon.
const DUST: i32 = 220;
/// The thickest dust of any branch (the Plane of Earth's).
const DUST_MOST: f32 = 3.0;

pub struct Vfx {
    root: Gd<Node3D>,
    soft: Gd<Texture2D>,
    /// The particle textures of recipes that have their own (stars,
    /// flipbooks of fire and smoke), else the soft disc.
    textures: HashMap<Recipe, Gd<Texture2D>>,
    splats_tex: Vec<Gd<Texture2D>>,
    emitters: HashMap<Recipe, Vec<Emitter>>,
    flashes: Vec<Flash>,
    beams: Vec<Beam>,
    /// When (`clock`) the map last drew a piece of a ray.
    last_piece: f32,
    /// A piece of a ray with no way of its own that came alone (a wand of
    /// digging's first cell): (kind, colour, where, when).
    loose: Option<(Ray, Color, Vector3, f32)>,
    /// A number for the next ray's pattern.
    rays_drawn: u32,
    columns: Vec<Column>,
    flame_shader: Option<Gd<Shader>>,
    flame_book: Option<Gd<Texture2D>>,
    /// When (`clock`) the last explosion went off, and the last fire.
    last_blast: f32,
    last_fire: f32,
    splats: Vec<Splat>,
    next_splat: usize,
    pending: Vec<Pending>,
    beam_shader: Option<Gd<Shader>>,
    dust: Gd<GpuParticles3D>,
    /// The dust's looks: motes, or embers rising (and which it is).
    mote_ramp: Gd<Texture2D>,
    mote_mat: Gd<Material>,
    ember_ramp: Gd<Texture2D>,
    ember_mat: Gd<Material>,
    embers_in_air: bool,
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

/// A creature's shadow on the floor under it: dark across its middle,
/// fading softly out to its edge.
pub fn contact_texture() -> Gd<Texture2D> {
    texture(64, |x, y| {
        let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() * 2.0;
        let t = ((d - 0.2) / 0.8).clamp(0.0, 1.0);
        1.0 - t * t * (3.0 - 2.0 * t)
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

/// How many shapes of splat there are (each splat takes one by where it
/// falls).
const SPLAT_SHAPES: u32 = 3;

/// A number in 0..1 for (`seed`, `i`): the same every time.
fn hash01(seed: u32, i: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9e37_79b1) ^ i.wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// A splat of blood, shape `seed`: a pool of a few blobs run together, its
/// edge wavering in no regular way, thicker (more opaque) in its middle,
/// and a spatter flung off to one side in drops that thin out with
/// distance, some of them drawn out along the way they flew. Lopsided,
/// so no two turns of it look alike.
fn splat_texture(seed: u32) -> Gd<Texture2D> {
    let r = |i: u32| hash01(seed, i);
    // the pool: blobs about the middle, pushed a little off it
    let blobs: Vec<(f32, f32, f32)> = (0..6)
        .map(|i| {
            let (a, d) = (r(i * 3) * std::f32::consts::TAU, r(i * 3 + 1) * 0.09);
            (
                0.46 + d * a.cos(),
                0.5 + d * a.sin(),
                0.1 + 0.1 * r(i * 3 + 2),
            )
        })
        .collect();
    // its edge's waver: three waves of no common period
    let waves: Vec<(f32, f32, f32)> = [2.0f32, 3.0, 7.0]
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            (
                f,
                0.01 + 0.02 * r(40 + i as u32),
                r(50 + i as u32) * std::f32::consts::TAU,
            )
        })
        .collect();
    // the spatter: drops along a way out of the pool, smaller further
    let way = r(60) * std::f32::consts::TAU;
    let drops: Vec<(f32, f32, f32, f32)> = (0..9)
        .map(|i| {
            let t = i as f32 / 8.0;
            let dist = 0.27 + 0.18 * t + 0.03 * r(70 + i);
            let side = (r(80 + i) - 0.5) * (0.12 + 0.2 * t);
            let (c, s) = (way.cos(), way.sin());
            let (x, y) = (0.46 + c * dist - s * side, 0.5 + s * dist + c * side);
            // the nearer drew out along their flight
            let stretch = 1.0 + 1.6 * (1.0 - t) * r(90 + i);
            (x, y, 0.012 + 0.03 * (1.0 - t) * r(100 + i), stretch)
        })
        .collect();
    texture(128, move |x, y| {
        let mut a: f32 = 0.0;
        let mut depth: f32 = 0.0;
        for &(cx, cy, cr) in &blobs {
            let (dx, dy) = (x - cx, y - cy);
            let ang = dy.atan2(dx);
            let wave: f32 = waves
                .iter()
                .map(|&(f, amp, ph)| amp * (ang * f + ph).sin())
                .sum();
            let d = (dx * dx + dy * dy).sqrt();
            a = a.max(((cr + wave - d) / 0.012).clamp(0.0, 1.0));
            depth = depth.max(((cr - d) / cr).clamp(0.0, 1.0));
        }
        for &(cx, cy, cr, stretch) in &drops {
            // along the way out the drop is drawn long
            let (c, s) = (way.cos(), way.sin());
            let (dx, dy) = (x - cx, y - cy);
            let (along, across) = (dx * c + dy * s, -dx * s + dy * c);
            let d = ((along / stretch).powi(2) + across.powi(2)).sqrt();
            a = a.max(((cr - d) / 0.008).clamp(0.0, 1.0));
        }
        // thin at its edge, thick in its middle
        a * (0.62 + 0.33 * depth)
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
/// `frames`: the texture is a flipbook of these columns and rows, played
/// once over a particle's life.
fn particle_material(
    tex: &Gd<Texture2D>,
    additive: bool,
    shaded: bool,
    frames: Option<(i32, i32)>,
) -> Gd<Material> {
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
    if let Some((h, v)) = frames {
        m.set_particles_anim_h_frames(h);
        m.set_particles_anim_v_frames(v);
        m.set_particles_anim_loop(false);
    }
    m.upcast()
}

/// A recipe's colours over a particle's life, in `color`: smoke fades in
/// and out thin (its `hdr` its thickest), flame burns from white-hot
/// through its colour to a red that fades, chips of rock stay what they
/// are until they are gone, the rest cool in their own colour.
fn ramp_of(recipe: Recipe, color: Color) -> Gd<GradientTexture1D> {
    let p = recipe.params();
    let rgba = Color::from_rgba;
    let (offsets, colors): (&[f32], Vec<Color>) = if recipe.is_smoke() {
        let a = p.hdr;
        (
            &[0.0, 0.2, 0.65, 1.0],
            vec![
                color.with_alpha(0.0),
                color.with_alpha(a),
                color.with_alpha(a * 0.7),
                color.with_alpha(0.0),
            ],
        )
    } else if matches!(recipe, Recipe::Tongues | Recipe::Pillar | Recipe::Blaze) {
        let k = p.hdr;
        (
            &[0.0, 0.3, 0.7, 1.0],
            vec![
                rgba(1.0 * k, 0.9 * k, 0.7 * k, 1.0),
                rgba(color.r * k, color.g * k, color.b * k, 0.9),
                rgba(color.r * 0.7, color.g * 0.25, color.b * 0.1, 0.5),
                rgba(0.3, 0.05, 0.0, 0.0),
            ],
        )
    } else if recipe == Recipe::Grit {
        (&[0.0, 0.85, 1.0], vec![color, color, color.with_alpha(0.0)])
    } else {
        return ramp(color, p.hdr);
    };
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(offsets));
    g.set_colors(&PackedColorArray::from(&colors[..]));
    let mut t = GradientTexture1D::new_gd();
    t.set_gradient(&g);
    t.set_use_hdr(true);
    t
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
        // its looks: motes of dust, or embers
        let mote_ramp = dust
            .get_process_material()
            .and_then(|m| m.try_cast::<ParticleProcessMaterial>().ok())
            .and_then(|m| m.get_color_ramp())
            .unwrap_or_else(|| ramp(Color::from_rgb(0.9, 0.86, 0.78), 0.35).upcast());
        let mote_mat = dust
            .get_material_override()
            .unwrap_or_else(|| particle_material(&soft, false, true, None));
        let ember_ramp = ramp(Color::from_rgb(1.0, 0.42, 0.12), 2.5).upcast();
        let ember_mat = particle_material(&soft, true, false, None);
        let beam_shader = godot::tools::try_load::<Shader>("res://shaders/beam.gdshader").ok();
        Vfx {
            root: node,
            splats_tex: (0..SPLAT_SHAPES).map(splat_texture).collect(),
            soft,
            textures: particle_textures(),
            emitters: HashMap::new(),
            flashes: Vec::new(),
            beams: Vec::new(),
            last_piece: f32::NEG_INFINITY,
            loose: None,
            rays_drawn: 0,
            columns: Vec::new(),
            flame_shader: godot::tools::try_load::<Shader>("res://shaders/flame.gdshader").ok(),
            flame_book: godot::tools::try_load::<Texture2D>(
                "res://art/cc0/unity-labs/flipbooks/Flame02_16x4.png",
            )
            .ok(),
            last_blast: f32::NEG_INFINITY,
            last_fire: f32::NEG_INFINITY,
            splats: Vec::new(),
            next_splat: 0,
            pending: Vec::new(),
            beam_shader,
            dust,
            mote_ramp,
            mote_mat,
            ember_ramp,
            ember_mat,
            embers_in_air: false,
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
            self.flash(at + Vector3::new(0.0, 0.3, 0.0), c, energy, secs, 2.5);
        }
        if let VfxKind::Blood(c) = kind {
            // blood darkens as it lies: never the bright red of the glyph
            let k = SPLAT_DARK;
            let dark = Color::from_rgb(c.r * k, c.g * k, c.b * k);
            self.splat(Vector3::new(at.x, 0.0, at.z), dark, 1.0);
        }
        if let VfxKind::Explosion(_) = kind {
            // the scorch of the blast, fading like blood
            self.splat(
                Vector3::new(at.x, 0.0, at.z),
                Color::from_rgb(0.05, 0.04, 0.035),
                1.0,
            );
        }
    }

    /// A cell of an explosion of `kind` (`heart`: its middle one) at
    /// `at` on the ground, `color` its glyph's: each kind as what it is.
    pub fn blast(&mut self, kind: Blast, heart: bool, color: Color, at: Vector3) {
        self.last_blast = self.clock;
        let mid = at + Vector3::new(0.0, 0.6, 0.0);
        let rgb = Color::from_rgb;
        match kind {
            Blast::Fiery => {
                self.last_fire = self.clock;
                let fire = rgb(1.0, 0.5, 0.16);
                if heart {
                    // a tower of flame, a column of fire rolling up it,
                    // embers flung out, the scorch it leaves
                    self.column(at);
                    self.emit(Recipe::Pillar, fire, at + Vector3::new(0.0, 0.3, 0.0), None);
                    self.emit(Recipe::Embers, rgb(1.0, 0.55, 0.2), mid, None);
                    self.flash(at + Vector3::new(0.0, 1.4, 0.0), fire, 7.0, 1.3, 5.0);
                    self.splat(at, rgb(0.04, 0.03, 0.025), 2.4);
                } else {
                    self.emit(Recipe::Blaze, fire, at + Vector3::new(0.0, 0.25, 0.0), None);
                }
            }
            Blast::Frosty => {
                let ice = rgb(0.62, 0.86, 1.0);
                self.emit(Recipe::Shards, ice, mid, None);
                self.emit_in_cell(Recipe::Rime, ice, mid);
                self.emit_in_cell(Recipe::Haze, rgb(0.75, 0.86, 0.95), mid);
                if heart {
                    self.flash(mid, ice, 3.0, 0.5, 4.0);
                }
            }
            Blast::Noxious => self.emit_in_cell(Recipe::Gas, rgb(0.55, 0.78, 0.2), mid),
            Blast::Dark => self.emit_in_cell(Recipe::Smoke, color, mid),
            Blast::Wet | Blast::Muddy => {
                let (drops, mist) = if kind == Blast::Wet {
                    (rgb(0.35, 0.55, 0.85), rgb(0.7, 0.8, 0.9))
                } else {
                    (rgb(0.3, 0.22, 0.14), rgb(0.36, 0.28, 0.2))
                };
                self.emit(Recipe::Droplets, drops, mid, None);
                self.emit_in_cell(Recipe::Gas, mist, mid);
                if kind == Blast::Muddy {
                    self.splat(at, drops, 1.0);
                }
            }
            Blast::Magical => {
                self.emit(Recipe::Blast, color, mid, None);
                self.emit(Recipe::Glints, color, mid, None);
                if heart {
                    self.flash(mid, color, 4.0, 0.4, 4.0);
                }
            }
        }
    }

    /// What a ray of its kind sheds, shed over a cell about `at` instead
    /// (an explosion's cell): the box of a cell, its share of the amount.
    fn emit_in_cell(&mut self, recipe: Recipe, color: Color, at: Vector3) {
        let i = self.take_emitter(recipe);
        let p = recipe.params();
        let Some(e) = self
            .emitters
            .get_mut(&recipe)
            .and_then(|pool| pool.get_mut(i))
        else {
            return;
        };
        let color = recipe.own_color().unwrap_or(color);
        e.process.set_color_ramp(&ramp_of(recipe, color));
        e.process
            .set_emission_box_extents(Vector3::new(0.45, 0.35, 0.45));
        e.node.set_transform(Transform3D::new(Basis::IDENTITY, at));
        e.node.set_amount_ratio(2.0 / TRAIL_FULL);
        e.node.set_visibility_aabb(Aabb::new(
            Vector3::new(-2.0, -2.0, -2.0),
            Vector3::new(4.0, 5.0, 4.0),
        ));
        e.node.set_visible(true);
        e.node.restart();
        e.node.set_emitting(true);
        e.busy = p.life as f32 + 0.1;
    }

    /// A tower of flame standing at `at` (the ground) for `COLUMN_SECS`.
    fn column(&mut self, at: Vector3) {
        let (Some(shader), Some(book)) = (self.flame_shader.clone(), self.flame_book.clone())
        else {
            return;
        };
        let i = match self.columns.iter().position(|c| c.age >= COLUMN_SECS) {
            Some(i) => i,
            None => {
                let mut root = Node3D::new_alloc();
                self.root.add_child(&root);
                let flames = COLUMN_FLAMES
                    .iter()
                    .map(|&(w, h, x, phase)| {
                        let mut quad = QuadMesh::new_gd();
                        quad.set_size(Vector2::new(w, h));
                        // it rises from its foot
                        quad.set_center_offset(Vector3::new(0.0, h / 2.0, 0.0));
                        let mut mat = ShaderMaterial::new_gd();
                        mat.set_shader(&shader);
                        mat.set_shader_parameter("flipbook", &book.to_variant());
                        mat.set_shader_parameter("has_flipbook", &true.to_variant());
                        let mut mi = MeshInstance3D::new_alloc();
                        mi.set_mesh(&quad);
                        mi.set_material_override(&mat);
                        mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                        mi.set_instance_shader_parameter("phase", &phase.to_variant());
                        mi.set_position(Vector3::new(x, 0.0, 0.0));
                        root.add_child(&mi);
                        mat
                    })
                    .collect();
                self.columns.push(Column {
                    root,
                    flames,
                    age: COLUMN_SECS,
                });
                self.columns.len() - 1
            }
        };
        let c = &mut self.columns[i];
        c.age = 0.0;
        c.root.set_position(at);
        c.root.set_scale(Vector3::ONE * 0.5);
        c.root.set_visible(true);
    }

    /// Whether a fire exploded lately (`FIRE_LATELY`).
    pub fn fire_lately(&self) -> bool {
        self.clock - self.last_fire < FIRE_LATELY
    }

    /// Seconds since the last explosion, and how long the newest tower of
    /// flame has stood (self-tests).
    pub fn since_blast(&self) -> (f32, Option<f32>) {
        let column = self
            .columns
            .iter()
            .filter(|c| c.age < COLUMN_SECS)
            .map(|c| c.age)
            .reduce(f32::min);
        (self.clock - self.last_blast, column)
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
        let color = tint.unwrap_or_else(|| beam_color(kind));
        self.place_ray(ray_of(kind), color, from, to, true);
        self.pending.push(Pending {
            kind,
            at: to,
            left: RUN_SECS,
            tint,
            dir: None,
        });
    }

    /// A piece of a ray the map draws cell by cell (no burst), `kind` its
    /// look, `color` the glyph's. A piece in line with a ray of its kind
    /// still showing, just past either end of it, lengthens that ray (the
    /// end it grows at is where it goes), so a ray is one ribbon however
    /// many cells it crosses; else it starts one. A piece of a ray already
    /// drawn whole (the hero's own zap) is not drawn again.
    pub fn ray(&mut self, kind: Ray, color: Color, from: Vector3, to: Vector3) {
        self.last_piece = self.clock;
        let mid = (from + to) / 2.0;
        let covered = self
            .beams
            .iter()
            .any(|b| b.whole && b.live() && distance_to_segment(mid, b.from, b.to) < 0.6);
        if covered {
            return;
        }
        let grown = self.beams.iter().enumerate().find_map(|(i, b)| {
            let same = !b.whole && b.live() && b.kind == kind && b.color == color;
            same.then(|| joined(b.from, b.to, from, to))
                .flatten()
                .map(|(from, to)| (i, from, to))
        });
        match grown {
            Some((i, from, to)) => self.grow(i, from, to),
            None => self.place_ray(kind, color, from, to, false),
        }
    }

    /// A piece of a ray that has no way of its own (a wand of digging's
    /// beam is the same in every cell), in the middle of its cell: it
    /// lengthens a ray of its kind that ends a cell away, or starts one
    /// from a piece that came alone just before.
    pub fn ray_through(&mut self, kind: Ray, color: Color, at: Vector3) {
        self.last_piece = self.clock;
        let grown = self.beams.iter().enumerate().find_map(|(i, b)| {
            if b.whole || !b.live() || b.kind != kind {
                return None;
            }
            // the last piece's middle, half a cell back from its end
            let last = b.to - (b.to - b.from).normalized() * 0.5;
            let d = last.distance_to(at);
            (d > 0.5 && d < 1.6).then(|| (i, b.from, at + (at - last) / d * 0.5))
        });
        if let Some((i, from, to)) = grown {
            self.grow(i, from, to);
            return;
        }
        let loose = self.loose.take();
        match loose {
            Some((k, _, p, when))
                if k == kind
                    && self.clock - when < 0.5
                    && (0.5..1.6).contains(&p.distance_to(at)) =>
            {
                let dir = (at - p).normalized();
                self.place_ray(kind, color, p - dir * 0.5, at + dir * 0.5, false);
            }
            _ => {
                self.loose = Some((kind, color, at, self.clock));
                // alone, it is a puff of dust (a flash) where it is
                let (recipe, color) = match kind.look().color {
                    Some(dust) if kind == Ray::Dig => (Recipe::Cloud, dust),
                    _ => (Recipe::Flare, color),
                };
                self.emit(recipe, color, at, None);
            }
        }
    }

    /// Whether the map drew a piece of a ray lately: the hero's use is
    /// shown once the turn is over, after the engine drew its ray cell by
    /// cell, so that was the use's beam, drawn as what it is.
    pub fn ray_lately(&self) -> bool {
        self.clock - self.last_piece < RAY_LATELY
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

    fn place_ray(&mut self, kind: Ray, color: Color, from: Vector3, to: Vector3, whole: bool) {
        let i = match self.beams.iter().position(|b| !b.live()) {
            Some(i) => i,
            None => {
                let beam = self.new_beam();
                self.beams.push(beam);
                self.beams.len() - 1
            }
        };
        let look = kind.look();
        let shown = look.color.unwrap_or(color);
        self.rays_drawn = self.rays_drawn.wrapping_add(1);
        let seed = hash01(self.rays_drawn, 7) * 10.0;
        let b = &mut self.beams[i];
        b.kind = kind;
        b.color = color;
        b.whole = whole;
        b.secs = look.secs;
        b.shed = false;
        b.mat
            .set_shader_parameter("style", &kind.style().to_variant());
        b.mat
            .set_shader_parameter("width", &look.width.to_variant());
        b.mat.set_shader_parameter("seed", &seed.to_variant());
        b.mat.set_shader_parameter("color", &shown.to_variant());
        // a whole ray runs out from its start; the map's is drawn as far
        // as its pieces have come
        let head: f32 = if whole { 0.0 } else { 1.0 };
        b.mat.set_shader_parameter("head", &head.to_variant());
        b.light.set_color(shown);
        b.light.set_negative(look.light < 0.0);
        b.light.set_param(Param::ENERGY, look.light.abs());
        b.light.set_visible(look.light != 0.0);
        b.node.set_visible(true);
        b.light.set_position(from);
        self.grow(i, from, to);
    }

    /// Ray `i` now runs from `from` to `to`: shown anew (it fades only
    /// once it stops growing), its light near its far end unless it runs
    /// out whole.
    fn grow(&mut self, i: usize, from: Vector3, to: Vector3) {
        let b = &mut self.beams[i];
        b.from = from;
        b.to = to;
        b.age = 0.0;
        let len = from.distance_to(to).max(0.05);
        b.mat.set_shader_parameter("ray_len", &len.to_variant());
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
        b.node
            .set_transform(Transform3D::new(Basis::from_cols(x, dir * len, z), mid));
        if !b.whole {
            b.light.set_position(lit_end(from, to, 1.0));
        }
    }

    /// What a ray of `kind` sheds along its length, from `from` to `to`
    /// (the ray is whole: it ran its length, or the map drew its last
    /// piece): a burst of each of its recipes over a box the ray's length,
    /// spread over the first part of their lives.
    fn shed(&mut self, kind: Ray, color: Color, from: Vector3, to: Vector3) {
        let len = from.distance_to(to).max(0.05);
        let dir = (to - from) / len;
        let up = if dir.y.abs() > 0.99 {
            Vector3::RIGHT
        } else {
            Vector3::UP
        };
        // the box: along the ray (z), up (y) the world's up
        let x = up.cross(dir).normalized();
        let at = Transform3D::new(Basis::from_cols(x, dir.cross(x), dir), (from + to) / 2.0);
        let color = kind.look().color.unwrap_or(color);
        for &recipe in kind.look().trails {
            let i = self.take_emitter(recipe);
            let p = recipe.params();
            let Some(e) = self
                .emitters
                .get_mut(&recipe)
                .and_then(|pool| pool.get_mut(i))
            else {
                continue;
            };
            let color = recipe.own_color().unwrap_or(color);
            e.process.set_color_ramp(&ramp_of(recipe, color));
            e.process
                .set_emission_box_extents(Vector3::new(0.07, 0.07, len / 2.0));
            e.node.set_transform(at);
            e.node.set_amount_ratio((len / TRAIL_FULL).clamp(0.15, 1.0));
            e.node.set_visibility_aabb(Aabb::new(
                Vector3::new(-2.0, -3.0, -len / 2.0 - 2.0),
                Vector3::new(4.0, 6.0, len + 4.0),
            ));
            e.node.set_visible(true);
            e.node.restart();
            e.node.set_emitting(true);
            e.busy = p.life as f32 + 0.1;
        }
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
        light.set_param(Param::RANGE, 2.8);
        light.set_param(Param::ENERGY, 3.0);
        // (no shadow: the fog it lights would glow through walls)
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.35);
        light.set_shadow(false);
        self.root.add_child(&light);
        Beam {
            node,
            mat,
            light,
            from: Vector3::ZERO,
            to: Vector3::ZERO,
            age: BEAM_SECS,
            secs: BEAM_SECS,
            kind: Ray::Plain,
            color: Color::WHITE,
            whole: false,
            shed: true,
        }
    }

    /// An emitter of `recipe` free to start (made if the pool has room;
    /// all busy, the one nearest to done starts again): its index in the
    /// recipe's pool.
    fn take_emitter(&mut self, recipe: Recipe) -> usize {
        let pool = self.emitters.entry(recipe).or_default();
        match pool.iter().position(|e| e.busy <= 0.0) {
            Some(i) => i,
            None if pool.len() < POOL => {
                let tex = self.textures.get(&recipe).unwrap_or(&self.soft);
                let e = new_emitter(&mut self.root, recipe, tex);
                pool.push(e);
                pool.len() - 1
            }
            None => pool
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.busy.total_cmp(&b.1.busy))
                .map_or(0, |(i, _)| i),
        }
    }

    fn emit(&mut self, recipe: Recipe, color: Color, at: Vector3, dir: Option<Vector3>) {
        let i = self.take_emitter(recipe);
        let p = recipe.params();
        let Some(e) = self
            .emitters
            .get_mut(&recipe)
            .and_then(|pool| pool.get_mut(i))
        else {
            return;
        };
        e.process.set_color_ramp(&ramp_of(recipe, color));
        e.process.set_direction(dir.unwrap_or(p.dir));
        e.node.set_position(at);
        e.node.set_visible(true);
        e.node.restart();
        e.node.set_emitting(true);
        e.busy = p.life as f32 + 0.1;
    }

    /// A light at `at` dying out over `secs`, reaching `range` metres.
    fn flash(&mut self, at: Vector3, color: Color, energy: f32, secs: f32, range: f32) {
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
        f.light.set_param(Param::RANGE, range);
        f.light.set_color(color);
        f.light.set_position(at);
        f.light.set_param(Param::ENERGY, energy);
        f.light.set_visible(true);
        f.energy = energy;
        f.left = secs;
        f.total = secs;
    }

    /// A splat of `color` on the floor at `at`, `scale` times its size.
    fn splat(&mut self, at: Vector3, color: Color, scale: f32) {
        if self.splats.len() < SPLATS {
            let mut decal = Decal::new_alloc();
            decal.set_cull_mask(self.decal_mask);
            decal.set_albedo_mix(1.0);
            self.root.add_child(&decal);
            self.splats.push(Splat { decal, age: 0.0 });
        }
        let i = self.next_splat % self.splats.len();
        self.next_splat += 1;
        let s = &mut self.splats[i];
        // its shape, turn and size by where it falls, the same every time
        let h = ((at.x * 12.9898 + at.z * 78.233).sin() * 43_758.547)
            .fract()
            .abs();
        let shape = ((h * 97.0) as usize) % self.splats_tex.len().max(1);
        if let Some(t) = self.splats_tex.get(shape) {
            s.decal
                .set_texture(godot::classes::decal::DecalTexture::ALBEDO, t);
        }
        // (its pool is about two fifths of it, its spatter reaches out to
        // its edge)
        let size = (0.85 + 0.4 * h) * scale;
        s.decal.set_size(Vector3::new(size, 0.5, size));
        s.decal
            .set_rotation(Vector3::new(0.0, h * std::f32::consts::TAU, 0.0));
        s.decal.set_position(at);
        s.decal.set_modulate(color);
        s.decal.set_visible(true);
        s.age = 0.0;
    }

    /// Rays showing now, and how long the newest has shown (seconds;
    /// self-tests).
    pub fn rays_shown(&self) -> (usize, Option<f32>) {
        let live = self.beams.iter().filter(|b| b.live());
        let newest = live.clone().map(|b| b.age).reduce(f32::min);
        (live.count(), newest)
    }

    /// Whether any burst, ray or what a ray shed still shows (self-tests).
    pub fn busy(&self) -> bool {
        self.beams.iter().any(Beam::live)
            || self.emitters.values().flatten().any(|e| e.busy > 0.0)
            || self.columns.iter().any(|c| c.age < COLUMN_SECS)
            || !self.pending.is_empty()
    }

    /// Splats lying on the floor now (self-tests).
    pub fn splats_shown(&self) -> usize {
        self.splats
            .iter()
            .filter(|s| s.age < SPLAT_SECS && s.decal.is_visible())
            .count()
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
        let mut due_shed = Vec::new();
        for b in &mut self.beams {
            if !b.live() {
                continue;
            }
            b.age += delta;
            // a whole ray's head runs ahead (its light with it), then the
            // ray fades; the map's shows as far as its pieces came
            if b.whole {
                let head = (b.age / RUN_SECS).min(1.0);
                b.light
                    .set_position(lit_end(b.from, b.from.lerp(b.to, head), 1.0));
                b.mat.set_shader_parameter("head", &head.to_variant());
            }
            let mut fade = 1.0 - ((b.age - RUN_SECS) / (b.secs - RUN_SECS)).clamp(0.0, 1.0);
            let mut light = fade;
            match b.kind {
                // a bolt strikes and strikes again
                Ray::Lightning => {
                    fade *= 0.55 + 0.45 * (clock * 20.0 * std::f32::consts::TAU).sin().abs();
                    light = fade;
                }
                // flame's light wavers
                Ray::Fire => {
                    light *= 0.8 + 0.12 * (clock * 23.0).sin() + 0.08 * (clock * 37.0 + 1.3).sin();
                }
                _ => {}
            }
            b.mat.set_shader_parameter("fade", &fade.to_variant());
            b.light
                .set_param(Param::ENERGY, b.kind.look().light.abs() * light);
            // whole at last (its head ran its length, or no piece came
            // for a moment): it sheds what it sheds
            if !b.shed && b.age >= if b.whole { RUN_SECS } else { SHED_AFTER } {
                b.shed = true;
                due_shed.push((b.kind, b.color, b.from, b.to));
            }
            if !b.live() {
                b.node.set_visible(false);
                b.light.set_visible(false);
            }
        }
        for (kind, color, from, to) in due_shed {
            self.shed(kind, color, from, to);
        }
        for c in &mut self.columns {
            if c.age >= COLUMN_SECS {
                continue;
            }
            c.age += delta;
            // it shoots up, stands and wavers, then dies down
            let up = smoothstep(0.0, 0.22, c.age);
            let fade = 1.0 - smoothstep(0.5, COLUMN_SECS, c.age);
            let waver = 0.9 + 0.1 * (clock * 17.0).sin();
            c.root
                .set_scale(Vector3::ONE * (0.45 + 0.55 * up) * (0.85 + 0.15 * fade));
            for m in &mut c.flames {
                m.set_shader_parameter("energy", &(2.4 * fade * waver).to_variant());
            }
            if c.age >= COLUMN_SECS {
                c.root.set_visible(false);
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
        // what they emitted goes too (a restart clears it, and hidden
        // they show nothing that lingers): kept, not freed, their shaders
        // stay compiled
        for e in self.emitters.values_mut().flatten() {
            e.node.restart();
            e.node.set_emitting(false);
            e.node.set_visible(false);
            e.busy = 0.0;
        }
        for f in &mut self.flashes {
            f.left = 0.0;
            f.light.set_visible(false);
        }
        for b in &mut self.beams {
            b.age = b.secs;
            b.shed = true;
            b.node.set_visible(false);
            b.light.set_visible(false);
        }
        self.loose = None;
        for c in &mut self.columns {
            c.age = COLUMN_SECS;
            c.root.set_visible(false);
        }
        for s in &mut self.splats {
            s.age = SPLAT_SECS;
            s.decal.set_visible(false);
        }
    }

    /// How thick the dust in the air is (1: the main dungeon's).
    /// Whether the dust draws (its first frame runs its pre-process).
    pub fn set_dust_shown(&mut self, on: bool) {
        if self.dust.is_visible() != on {
            self.dust.set_visible(on);
        }
    }

    /// `embers`: glowing embers rising slowly instead (the fire below).
    pub fn set_dust(&mut self, thickness: f32, embers: bool) {
        // a new amount restarts the emitter, and its 8 s of pre-process
        // run on the GPU in one frame: the ratio of a fixed amount does not
        self.dust
            .set_amount_ratio((thickness / DUST_MOST).clamp(0.0, 1.0));
        if embers == self.embers_in_air {
            return;
        }
        self.embers_in_air = embers;
        let Some(mut process) = self
            .dust
            .get_process_material()
            .and_then(|m| m.try_cast::<ParticleProcessMaterial>().ok())
        else {
            return;
        };
        // (its looks changed, not its particles: nothing restarts)
        let (ramp, mat, rise) = if embers {
            (&self.ember_ramp, &self.ember_mat, 0.03)
        } else {
            (&self.mote_ramp, &self.mote_mat, -0.005)
        };
        process.set_color_ramp(ramp);
        process.set_gravity(Vector3::new(0.0, rise, 0.0));
        self.dust.set_material_override(mat);
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
        p.set_material_override(&particle_material(&self.soft, true, false, None));
        p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        p.set_visibility_aabb(Aabb::new(
            Vector3::new(-0.5, -0.2, -0.5),
            Vector3::new(1.0, 1.6, 1.0),
        ));
        p
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where a ray from `from` to `to` throws its light: as near its end as
/// `back` metres short of it (it ends at a wall, and its light, with no
/// shadow, would light what lies behind the wall), never before its start.
fn lit_end(from: Vector3, to: Vector3, back: f32) -> Vector3 {
    let len = from.distance_to(to);
    if len < 1e-3 {
        return to;
    }
    from.lerp(to, ((len - back) / len).max(0.0))
}

/// Where a ray running from `from` to `to` runs once a piece of one (`a`
/// to `b`, either way) joins it: a piece in line with it, its middle just
/// past the ray's end, carries it on; just before its start, the ray goes
/// the other way (its first piece could not say which). None: the piece
/// is no part of it (askew, beside it, or too far off).
fn joined(from: Vector3, to: Vector3, a: Vector3, b: Vector3) -> Option<(Vector3, Vector3)> {
    let len = from.distance_to(to).max(1e-3);
    let dir = (to - from) / len;
    if dir.dot((b - a).normalized()).abs() < 0.98 {
        return None;
    }
    // where the piece's middle is along the ray, and how far off its line
    let mid = (a + b) / 2.0;
    let half = a.distance_to(b) / 2.0;
    let along = (mid - from).dot(dir);
    if (mid - (from + dir * along)).length() > 0.2 {
        return None;
    }
    if along > len - 0.2 && along < len + 1.6 {
        Some((from, from + dir * (along + half)))
    } else if along < 0.2 && along > -1.6 {
        Some((to, from + dir * (along - half)))
    } else {
        None
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

/// How a recipe's particles move, grow and turn.
fn process_material(recipe: Recipe, p: &Params) -> Gd<ParticleProcessMaterial> {
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
    // a ring spreads from a small one; smoke swells as it drifts
    let grows: Option<(f32, f32)> = match recipe {
        Recipe::Ring => Some((0.12, 1.0)),
        r if r.is_smoke() => Some((0.55, 1.3)),
        _ => None,
    };
    if let Some((from, to)) = grows {
        let mut curve = Curve::new_gd();
        curve.set_max_value(to.max(1.0));
        curve.add_point(Vector2::new(0.0, from));
        curve.add_point(Vector2::new(1.0, to));
        let mut tex = CurveTexture::new_gd();
        tex.set_curve(&curve);
        m.set_param_texture(Parameter::SCALE, &tex);
    }
    // turned any way: a ring, smoke and flame are no two alike
    if grows.is_some() || recipe.frames().is_some() {
        m.set_param_min(Parameter::ANGLE, 0.0);
        m.set_param_max(Parameter::ANGLE, 360.0);
    }
    if recipe.frames().is_some() {
        // its flipbook once over its life, from a little way in
        m.set_param_min(Parameter::ANIM_SPEED, 1.0);
        m.set_param_max(Parameter::ANIM_SPEED, 1.0);
        m.set_param_min(Parameter::ANIM_OFFSET, 0.0);
        m.set_param_max(Parameter::ANIM_OFFSET, 0.12);
    }
    m
}

fn new_emitter(root: &mut Gd<Node3D>, recipe: Recipe, tex: &Gd<Texture2D>) -> Emitter {
    let p = recipe.params();
    let mut m = process_material(recipe, &p);
    let mean = (p.size.0 + p.size.1) / 2.0;
    let mut node = GpuParticles3D::new_alloc();
    node.set_process_material(&m);
    node.set_amount(p.amount);
    node.set_lifetime(p.life);
    node.set_one_shot(true);
    node.set_explosiveness_ratio(
        if matches!(recipe, Recipe::Motes | Recipe::Runes | Recipe::Cloud) || recipe.is_trail() {
            0.6
        } else {
            1.0
        },
    );
    if recipe.is_trail() {
        // along a ray: `Vfx::shed` sets the box
        m.set_emission_shape(EmissionShape::BOX);
    }
    node.set_emitting(false);
    if recipe == Recipe::Sparks {
        // a streak along its velocity, facing the camera round it
        let mut streak = QuadMesh::new_gd();
        streak.set_size(Vector2::new(mean * 0.9, mean * 4.0));
        node.set_draw_pass_mesh(0, &streak);
        node.set_transform_align(TransformAlign::Z_BILLBOARD_Y_TO_VELOCITY);
        let mut m = particle_material(tex, true, false, None);
        if let Ok(mut b) = m.clone().try_cast::<StandardMaterial3D>() {
            b.set_billboard_mode(BillboardMode::DISABLED);
            m = b.upcast();
        }
        node.set_material_override(&m);
    } else {
        node.set_draw_pass_mesh(0, &quad(mean));
        let mat = particle_material(tex, p.additive, p.lit, recipe.frames());
        if recipe == Recipe::Ring
            && let Ok(mut m) = mat.clone().try_cast::<StandardMaterial3D>()
        {
            // a ring of force strikes a wall: drawn whole over it
            m.set_flag(Flags::DISABLE_DEPTH_TEST, true);
        }
        node.set_material_override(&mat);
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
    p.set_amount((DUST as f32 * DUST_MOST) as i32);
    p.set_amount_ratio(1.0 / DUST_MOST);
    p.set_lifetime(8.0);
    p.set_pre_process_time(8.0);
    p.set_use_local_coordinates(false);
    p.set_draw_pass_mesh(0, &quad(0.02));
    p.set_material_override(&particle_material(soft, false, true, None));
    p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    p.set_visibility_aabb(Aabb::new(
        Vector3::new(-9.0, -3.0, -7.0),
        Vector3::new(18.0, 6.0, 14.0),
    ));
    root.add_child(&p);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, z: f32) -> Vector3 {
        Vector3::new(x, 0.6, z)
    }

    /// The pieces of a ray as the map draws them: a cell's middle and
    /// 0.55 either way along its glyph's way (either sign).
    fn piece(x: f32, z: f32, way: Vector3) -> (Vector3, Vector3) {
        (v(x, z) - way * 0.55, v(x, z) + way * 0.55)
    }

    #[test]
    fn the_pieces_of_a_ray_join_into_one_that_goes_their_way() {
        let east = Vector3::new(1.0, 0.0, 0.0);
        // a ray going west: its first piece cannot say which way, the
        // second can
        let (a, b) = piece(8.0, 5.0, east);
        let (c, d) = piece(7.0, 5.0, east);
        let (from, to) = joined(a, b, c, d).expect("the next cell west joins");
        assert!(from.distance_to(v(8.55, 5.0)) < 1e-4, "{from:?}");
        assert!(to.distance_to(v(6.45, 5.0)) < 1e-4, "{to:?}");
        // then on west, piece by piece, whichever way its glyph points
        let (e, f) = piece(6.0, 5.0, -east);
        let (from, to) = joined(from, to, e, f).expect("on west");
        assert!(to.distance_to(v(5.45, 5.0)) < 1e-4, "{to:?}");
        assert!(from.distance_to(v(8.55, 5.0)) < 1e-4);
        // a ray in the next row, a cell beyond its end, or askew is
        // another one
        let (g, h) = piece(5.0, 6.0, east);
        assert!(joined(from, to, g, h).is_none());
        let (g, h) = piece(3.0, 5.0, east);
        assert!(joined(from, to, g, h).is_none());
        let (g, h) = piece(5.0, 5.0, Vector3::new(0.0, 0.0, 1.0));
        assert!(joined(from, to, g, h).is_none());
    }

    #[test]
    fn the_pieces_of_a_ray_aslant_join_with_no_gap() {
        let way = Vector3::new(1.0, 0.0, 1.0).normalized();
        let (a, b) = piece(8.0, 4.0, way);
        let (c, d) = piece(7.0, 3.0, way);
        let (from, to) = joined(a, b, c, d).expect("the next cell aslant joins");
        // one ribbon from the first cell's far side to the second's
        assert!((from.distance_to(to) - (2.0f32.sqrt() + 1.1)).abs() < 1e-4);
    }

    #[test]
    fn every_beam_type_has_its_own_ray() {
        let rays: Vec<Ray> = (0..8).map(Ray::of_zap).collect();
        for (i, r) in rays.iter().enumerate() {
            assert!(!rays[..i].contains(r), "{r:?} twice");
            assert_ne!(*r, Ray::Plain);
        }
        assert_eq!(Ray::of_zap(8), Ray::Plain);
        // and each its own look in the shader
        let mut styles: Vec<i32> = Ray::ALL.iter().map(|r| r.style()).collect();
        styles.sort_unstable();
        assert_eq!(styles, (0..11).collect::<Vec<_>>());
    }
}
