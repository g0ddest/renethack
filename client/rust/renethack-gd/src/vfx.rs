//! The map's effects: bursts of particles (hit sparks, blood, the magic of
//! quaffing, reading, zapping and casting, fire, frost, lightning, poison,
//! explosions, healing, sparkles, a ring of force) with a short flash of
//! light, rays drawn as what they are of (`Ray`: a stream of flame, an
//! icy core shedding shards, a drifting haze, darts of light, a black ray,
//! a spray, rock dust, rings of force) with a light on them and particles
//! shed along them, blood splats on the floor that fade, the marks effects
//! leave there a while (`Mark`: scorch, frost, etching, glitter, dust), and
//! the dust in
//! the air that shows only where light falls. Emitters, lights and decals
//! are pooled per kind; nothing here is ever more than a picture of what
//! the game already said.
//!
//! Other parts of the client call `Vfx::burst(kind, at)` and
//! `Vfx::beam(kind, from, to)` (world positions), `Vfx::ray` for the
//! pieces of a ray the map draws cell by cell, or the map's cell helpers
//! `MapView::burst_at` and `MapView::beam_between`.

use std::collections::HashMap;

#[path = "vfx_marks.rs"]
mod marks;
pub use marks::Mark;
#[path = "vfx_shapes.rs"]
mod shapes;

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
    Material, Mesh, MeshInstance3D, Node3D, OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader,
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
    /// Healing: a soft green glow and stars rising round the body healed
    /// (on the body: `Vfx::on_body`).
    Heal,
    /// A potion working on its drinker: a glow of its colour rising round
    /// the body from the feet (on the body).
    Aura(Color),
    /// A scroll's words working on its reader: runes rising round the body
    /// in a warm glow (on the body).
    Halo,
    Sparkle,
    /// A weapon's blow landing.
    Sparks,
    /// A creature bleeding, of this colour (droplets and a splat).
    Blood(Color),
    /// A ring of force spreading where a bolt of force (or magic the
    /// client cannot tell) strikes, of this colour.
    Shock(Color),
    /// A ray or a bolt striking a wall or rock: a flash, sparks thrown back
    /// and a burst of light, of this colour.
    Impact(Color),
    /// A lit field round the hero (a scroll or a spell of light): a burst
    /// of light from them, spreading over the floor and settling.
    Lit,
    /// An item glowing where it is worn or held (a scroll of enchant
    /// armour's, of enchant weapon's), of its glow's colour.
    Gleam(Color),
    /// Force striking (a bolt of force, a wand of striking, any magic the
    /// client cannot tell that leaves no ray): a flash and a ring of this
    /// colour, dust thrown up, and on stone (`true`) chips of it flung.
    Force(Color, bool),
    /// A curse lifted (`You feel like someone is helping you`): a golden
    /// glow and stars round the body (on the body).
    Bless,
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
    /// What a ray of this kind (`color` its glyph's) leaves on the floor
    /// under its way and where it strikes, and how wide (metres).
    fn mark(self, color: Color) -> Option<(Mark, f32)> {
        let rgb = Color::from_rgb;
        match self {
            Ray::Fire => Some((Mark::Scorch(EMBER), 0.9)),
            Ray::Cold => Some((Mark::Frost, 1.0)),
            Ray::Lightning => Some((Mark::Scorch(rgb(0.6, 0.75, 1.0)), 0.55)),
            Ray::Acid => Some((Mark::Etch(rgb(0.6, 0.95, 0.2)), 0.8)),
            Ray::Poison => Some((Mark::Etch(rgb(0.5, 0.8, 0.2)), 0.7)),
            Ray::Death => Some((Mark::Soot, 0.8)),
            Ray::MagicMissile | Ray::Plain | Ray::Sleep => {
                Some((Mark::Glitter(self.look().color.unwrap_or(color)), 0.8))
            }
            Ray::Dig => Some((Mark::Dust(rgb(0.45, 0.4, 0.34)), 0.9)),
            Ray::Force => None,
        }
    }

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
    /// A round burst of light, no rays (magic striking, a light bursting).
    Bloom,
    Droplets,
    /// A ring spreading out and fading.
    Ring,
    /// A column of rolling fire rising (a fire's heart).
    Pillar,
    /// Rolling fire, rising (a fire's other cells).
    Blaze,
    /// Embers flung up and drifting.
    Embers,
    // rising round a body from a ring about it (`Recipe::ring`):
    /// Soft glow.
    Aura,
    /// Wisps of light turning as they rise.
    Wisps,
    /// Runes.
    Halo,
    /// Stars.
    Mend,
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
    /// A puff of dust where something strikes, at a point.
    Puff,
    /// Chips of stone flung from where something strikes, falling.
    Chips,
    /// Specks of light flung out low over the floor all round, slowing
    /// (a lit field spreading).
    Spread,
    /// A soft glow round a point, lingering (an item glowing).
    Glow,
    /// Dark smoke rolling up off a fire.
    Soot,
    /// A small dusky puff (a spell gathering at the hand).
    Murk,
    /// A ball of a colour, drawn over what is behind it: a blast's heart
    /// that keeps its hue (added light whitens what it lies on).
    Core,
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
            Recipe::Soot => Some(Color::from_rgb(0.035, 0.03, 0.028)),
            Recipe::Smoke => Some(Color::from_rgb(0.05, 0.03, 0.07)),
            Recipe::Dust | Recipe::Puff => Some(Color::from_rgb(0.62, 0.55, 0.47)),
            Recipe::Grit | Recipe::Chips => Some(Color::from_rgb(0.3, 0.26, 0.22)),
            _ => None,
        }
    }

    /// Rising round a body: the ring it comes from, as a share of the
    /// body's height (`BODY_TALL` at `k` 1): how high its middle is over
    /// the feet, and how tall the band is.
    fn ring(self) -> Option<(f32, f32)> {
        match self {
            Recipe::Aura => Some((0.05, 0.1)),
            Recipe::Wisps => Some((0.3, 0.5)),
            Recipe::Halo => Some((0.35, 0.25)),
            Recipe::Mend => Some((0.4, 0.6)),
            _ => None,
        }
    }

    /// How much of a burst comes at once (Godot's explosiveness): the rest
    /// comes over that share of its life.
    fn explosiveness(self) -> f32 {
        if self.ring().is_some() {
            0.25
        } else if self == Recipe::Soot {
            // it rolls off the fire while it burns
            0.3
        } else if self == Recipe::Embers {
            0.7
        } else if matches!(self, Recipe::Motes | Recipe::Runes | Recipe::Cloud) || self.is_trail() {
            0.6
        } else {
            1.0
        }
    }

    /// How long a burst of it shows, from its start to its last
    /// particle's end (seconds): what comes late lives its whole life too.
    fn shows(self) -> f32 {
        let p = self.params();
        p.life as f32 * (2.0 - self.explosiveness()) + 0.1
    }

    /// A smoke or a haze: it veils what is behind it (its ramp's alpha is
    /// low and it lingers), it does not glow.
    fn is_smoke(self) -> bool {
        matches!(
            self,
            Recipe::Haze
                | Recipe::Smoke
                | Recipe::Gas
                | Recipe::Dust
                | Recipe::Puff
                | Recipe::Soot
                | Recipe::Murk
                | Recipe::Core
        )
    }

    /// Rolling fire (fire.gdshader): its particles' colour is their heat,
    /// their soot and their thickness (`ramp_of`), not a colour.
    fn is_fire(self) -> bool {
        matches!(
            self,
            Recipe::Flames | Recipe::Tongues | Recipe::Pillar | Recipe::Blaze
        )
    }

    /// Its texture is a sheet of shapes (`frames`) and a particle shows one
    /// of them all its life, upright: runes.
    fn picks(self) -> bool {
        matches!(self, Recipe::Runes | Recipe::Halo)
    }

    /// The frames of its texture's flipbook (columns, rows), played once
    /// over a particle's life.
    fn frames(self) -> Option<(i32, i32)> {
        match self {
            Recipe::Tongues
            | Recipe::Pillar
            | Recipe::Blaze
            | Recipe::Flames
            | Recipe::Haze
            | Recipe::Smoke
            | Recipe::Gas
            | Recipe::Dust
            | Recipe::Soot
            | Recipe::Murk
            | Recipe::Puff => Some((8, 8)),
            Recipe::Runes | Recipe::Halo => Some((2, 2)),
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
            // a few hot points turning about the hand (the spell's body is
            // its murk)
            Recipe::Swirl => Params {
                amount: 9,
                life: 0.8,
                speed: (0.3, 0.7),
                spread: 70.0,
                size: (0.1, 0.18),
                radius: 0.3,
                hdr: 2.2,
                ..base
            },
            // (a fire's: `hdr` is nothing to it, its colours are the
            // shader's)
            Recipe::Flames => Params {
                amount: 9,
                life: 0.7,
                speed: (0.5, 1.3),
                spread: 30.0,
                gravity: Vector3::new(0.0, 1.5, 0.0),
                size: (0.3, 0.55),
                radius: 0.2,
                damping: 1.0,
                ..base
            },
            Recipe::Shards => Params {
                amount: 12,
                life: 0.7,
                speed: (1.0, 2.4),
                spread: 70.0,
                gravity: Vector3::new(0.0, -6.0, 0.0),
                size: (0.1, 0.2),
                radius: 0.3,
                hdr: 1.4,
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
            Recipe::Bloom => Params {
                amount: 2,
                life: 0.16,
                speed: (0.0, 0.05),
                spread: 180.0,
                size: (0.45, 0.6),
                radius: 0.0,
                damping: 0.0,
                hdr: 1.8,
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
                amount: 30,
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
            // flung up, then rising slowly and dying
            Recipe::Embers => Params {
                amount: 34,
                life: 2.2,
                speed: (0.5, 2.4),
                spread: 38.0,
                gravity: Vector3::new(0.0, 0.35, 0.0),
                size: (0.05, 0.09),
                radius: 0.5,
                damping: 0.9,
                ..base
            },
            Recipe::Soot => Params {
                amount: 22,
                life: 2.0,
                speed: (0.5, 1.3),
                spread: 25.0,
                gravity: Vector3::new(0.0, 0.4, 0.0),
                size: (0.7, 1.2),
                radius: 0.45,
                damping: 0.8,
                hdr: 0.6,
                additive: false,
                ..base
            },
            Recipe::Murk => Params {
                amount: 9,
                life: 0.8,
                speed: (0.1, 0.35),
                spread: 180.0,
                gravity: Vector3::new(0.0, 0.25, 0.0),
                size: (0.22, 0.4),
                radius: 0.08,
                damping: 1.0,
                hdr: 0.5,
                additive: false,
                ..base
            },
            Recipe::Core => Params {
                amount: 6,
                life: 0.9,
                speed: (0.0, 0.1),
                spread: 180.0,
                size: (0.8, 1.1),
                radius: 0.1,
                damping: 0.0,
                hdr: 0.9,
                additive: false,
                ..base
            },
            // a body's, seen at the game's distance: the ring's radius. A
            // dusky glow (the body in it must still be read: bright, it
            // whited them out), the wisps and stars in it its hot points
            Recipe::Aura => Params {
                amount: 16,
                life: 1.4,
                speed: (0.4, 0.95),
                spread: 8.0,
                gravity: Vector3::new(0.0, 0.25, 0.0),
                size: (0.35, 0.6),
                radius: 0.3,
                damping: 0.5,
                hdr: 0.5,
                ..base
            },
            Recipe::Wisps => Params {
                amount: 8,
                life: 1.2,
                speed: (0.5, 1.0),
                spread: 10.0,
                size: (0.45, 0.75),
                radius: 0.25,
                damping: 0.6,
                hdr: 1.2,
                ..base
            },
            Recipe::Halo => Params {
                amount: 22,
                life: 1.4,
                speed: (0.3, 0.7),
                spread: 10.0,
                gravity: Vector3::new(0.0, 0.1, 0.0),
                size: (0.25, 0.42),
                radius: 0.45,
                damping: 0.5,
                hdr: 1.9,
                ..base
            },
            Recipe::Mend => Params {
                amount: 18,
                life: 1.3,
                speed: (0.35, 0.8),
                spread: 12.0,
                size: (0.22, 0.4),
                radius: 0.4,
                damping: 0.5,
                hdr: 2.8,
                ..base
            },
            // shed along a ray: amounts are for a ray of `TRAIL_FULL`
            // metres, a shorter one sheds fewer; a ray shows for a moment,
            // so they come thick while it does
            Recipe::Tongues => Params {
                amount: 200,
                life: 0.8,
                speed: (0.2, 0.7),
                spread: 30.0,
                gravity: Vector3::new(0.0, 2.2, 0.0),
                size: (0.45, 0.9),
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
                hdr: 1.6,
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
            Recipe::Puff => Params {
                amount: 30,
                life: 1.3,
                speed: (0.35, 1.0),
                spread: 75.0,
                gravity: Vector3::new(0.0, 0.15, 0.0),
                size: (0.3, 0.6),
                radius: 0.12,
                damping: 1.4,
                hdr: 0.75,
                additive: false,
                lit: true,
                ..base
            },
            Recipe::Glow => Params {
                amount: 18,
                life: 1.4,
                speed: (0.05, 0.3),
                spread: 180.0,
                size: (0.5, 0.9),
                radius: 0.12,
                damping: 1.0,
                hdr: 1.1,
                ..base
            },
            Recipe::Spread => Params {
                amount: 70,
                life: 1.1,
                speed: (3.0, 5.5),
                spread: 82.0,
                gravity: Vector3::new(0.0, -0.6, 0.0),
                size: (0.06, 0.12),
                radius: 0.25,
                damping: 2.4,
                hdr: 3.2,
                ..base
            },
            Recipe::Chips => Params {
                amount: 34,
                life: 0.75,
                speed: (1.6, 3.6),
                spread: 60.0,
                gravity: Vector3::new(0.0, -9.8, 0.0),
                size: (0.03, 0.065),
                radius: 0.06,
                damping: 0.3,
                hdr: 1.0,
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
        VfxKind::Quaff(c) => (Recipe::Motes, c, Some((c, 1.8, 0.8))),
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
            (Recipe::Shards, c, Some((c, 3.0, 0.6)))
        }
        VfxKind::Lightning => {
            let c = rgb(0.91, 0.94, 1.0);
            (Recipe::Bolts, c, Some((c, 4.0, 0.12)))
        }
        VfxKind::Poison => {
            let c = rgb(0.56, 1.0, 0.25);
            (Recipe::Cloud, c, Some((c, 1.6, 0.8)))
        }
        VfxKind::Explosion(c) => (Recipe::Blast, c, Some((c, 4.0, 0.3))),
        VfxKind::Heal => {
            let c = rgb(0.75, 1.0, 0.7);
            (Recipe::Mend, c, Some((rgb(0.6, 1.0, 0.55), 2.2, 1.1)))
        }
        VfxKind::Aura(c) => (Recipe::Aura, c, Some((c, 2.0, 1.0))),
        VfxKind::Halo => {
            let gold = rgb(1.0, 0.8, 0.42);
            (Recipe::Halo, gold, Some((gold, 2.4, 0.9)))
        }
        VfxKind::Impact(c) => (Recipe::Sparks, c, Some((c, 9.0, 0.35))),
        VfxKind::Sparkle => {
            let c = rgb(1.0, 0.95, 0.8);
            (Recipe::Glints, c, Some((c, 1.6, 0.35)))
        }
        VfxKind::Sparks => {
            let c = rgb(1.0, 0.6, 0.22);
            (Recipe::Sparks, c, Some((c, 6.0, 0.2)))
        }
        VfxKind::Blood(c) => (Recipe::Droplets, c, None),
        VfxKind::Shock(c) => (Recipe::Ring, c, Some((c, 2.0, 0.2))),
        // (its light its own: wide, `LIT_LIGHT`)
        VfxKind::Lit => (Recipe::Glints, LIT, None),
        VfxKind::Gleam(c) => {
            let rich = richer(c);
            (Recipe::Swirl, rich, Some((rich, 3.5, 1.3)))
        }
        VfxKind::Force(c, _) => (Recipe::Bloom, c, Some((c, 5.0, 0.3))),
        VfxKind::Bless => {
            // an old gold, not cream
            let gold = rgb(1.0, 0.66, 0.24);
            (Recipe::Mend, gold, Some((gold, 2.2, 1.1)))
        }
    }
}

/// A glow's colour the richer in its hue (a silver or a white glow
/// still a pale colour, not white), its brightest channel full.
fn richer(c: Color) -> Color {
    let luma = c.r * 0.3 + c.g * 0.59 + c.b * 0.11;
    let push = |v: f32| (luma + (v - luma) * 2.2).clamp(0.0, 1.0);
    vivid(Color::from_rgb(push(c.r), push(c.g), push(c.b)))
}

/// A colour as bright as its hue goes (its brightest channel full), a
/// grey one a pale glow.
fn vivid(c: Color) -> Color {
    let most = c.r.max(c.g).max(c.b);
    if most < 0.05 {
        return Color::from_rgb(0.8, 0.8, 0.85);
    }
    Color::from_rgb(c.r / most, c.g / most, c.b / most)
}

/// The colour of a beam of this kind.
pub fn beam_color(kind: VfxKind) -> Color {
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
        | VfxKind::Shock(c)
        | VfxKind::Aura(c)
        | VfxKind::Gleam(c)
        | VfxKind::Force(c, _)
        | VfxKind::Impact(c) => c,
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
    /// Its particles' quad, and how many times its size it is drawn now
    /// (`Vfx::burst_large`).
    quad: Gd<Mesh>,
    sized: f32,
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
    /// The solid cells it has struck (`Vfx::cap`).
    capped: Vec<(i32, i32)>,
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
/// How much of a column's flames' foot fades in, and how bright they burn.
const COLUMN_FOOT: f32 = 0.3;
const COLUMN_ENERGY: f32 = 1.1;
/// A fire's smoke: how far over its foot it rolls off (metres), and how
/// much nearer than the flames it is sorted (drawn over them).
const SOOT_HIGH: f32 = 1.5;
const SOOT_OVER: f32 = 2.0;
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
/// A burst seen at the game's distance (`Vfx::burst_large`): its
/// particles this many times their size (their textures' bright hearts
/// are small) and this much brighter (more, and they bleach to white),
/// its light this many times brighter and longer, and for a kind with no
/// light of its own (a glint) one: its energy and seconds.
const LARGE: f32 = 4.0;
/// The level's stone where no other is told (the main dungeon's walls).
const STONE: Color = Color::from_rgb(0.42, 0.4, 0.37);
/// A curse lifted: the golden glow round the body this many times the
/// size of a healing's (the same size, at the game's camera it was barely
/// seen).
const BLESS_SIZE: f32 = 2.0;
/// The light force's dust holds after the strike: its energy and seconds.
const FORCE_DUST_LIGHT: (f32, f32) = (1.6, 1.1);
const LARGE_GLOW: f32 = 1.25;
const LARGE_LIGHT_UP: (f32, f32) = (2.5, 1.5);
const LARGE_LIGHT: (f32, f32) = (0.6, 0.45);
/// The height of a body an effect on it (`Vfx::on_body`) is made for,
/// metres: a taller one's is as many times larger.
const BODY_TALL: f32 = 1.75;
/// How far past a ray's end the solid it strikes is looked for (metres):
/// a piece of a ray runs a little past its cell's edge.
const CAP_REACH: f32 = 0.3;
/// A strike's flash, this many times a blow's.
const IMPACT_FLASH: f32 = 2.2;
/// The embers glowing in a fire's scorch a moment.
const EMBER: Color = Color::from_rgb(1.0, 0.36, 0.07);
/// A strike's mark, this many times its ray's, and this far back from the
/// stone it struck (metres) at its foot.
const STRIKE_MARK: f32 = 1.7;
const STRIKE_MARK_BACK: f32 = 0.35;
/// A ray bursts on a creature once in this long (seconds): its bounce
/// crosses it again.
const RAY_HIT_ONCE: f32 = 1.2;
/// A cold ray seen no longer than this (metres) was zapped point-blank;
/// rays run this high over the floor (map_view's `bright_flash`).
const POINT_BLANK: f32 = 2.4;
const RAY_OVER: f32 = 0.6;
/// A cold ray's strike: its frost burst's ball (radius, metres) and the
/// rime it leaves (metres across).
const FROST_BURST: f32 = 0.9;
const FROST_RIME: f32 = 2.6;
/// A strike's flash this far off the stone it strikes (metres): half of
/// it was buried in the wall.
const IMPACT_OFF: f32 = 0.25;
/// A lit field's light (`VfxKind::Lit`): its colour, its energy, how long
/// it takes to settle (seconds) and how far it reaches (metres).
const LIT: Color = Color::from_rgb(1.0, 0.93, 0.78);
const LIT_LIGHT: (f32, f32, f32) = (7.0, 1.6, 9.0);
const BEAM_SECS: f32 = 0.45;
/// The longest step effects age by in a frame (seconds).
const SLOW_FRAME: f32 = 0.1;
/// How long a whole ray's head takes to run its length, and a ray shows
/// at its brightest once it stops growing, before it fades.
const RUN_SECS: f32 = BEAM_SECS * 0.4;
/// A ray the map draws is whole once no piece has come for this long
/// (seconds): then it sheds what it sheds.
const SHED_AFTER: f32 = 0.15;
/// A ray this long (metres) or longer sheds a trail's whole amount.
const TRAIL_FULL: f32 = 8.0;
/// An explosion's cell sheds this share of what its kind's ray would along
/// a whole trail: a cloud of it, a ball this wide (radius, metres).
const BLAST_SHARE: f32 = 0.5;
const BLAST_BALL: f32 = 0.55;
/// An explosion's cells beside its heart burst this far nearer it (in
/// cells), its corners further and with this share of the rest: the whole
/// round, its edge about a cell and a half from the heart.
const BLAST_PULL: f32 = 0.25;
const BLAST_PULL_CORNER: f32 = 0.45;
const BLAST_CORNER: f32 = 0.75;
/// A fire's cells: their flames this large, leaning out from the heart
/// (this much up for one out), and the ball of flame at the heart's foot.
const BLAST_BLAZE: f32 = 1.3;
const BLAST_LEAN: f32 = 1.1;
const BLAST_HEART_BLAZE: f32 = 1.5;
/// A frosty or magical blast's body: a haze of its colour (drawn over
/// what is behind it, so it keeps its colour over a lit floor) and a soft
/// glow in it, this much of its colour (an added light whitens what it
/// lies on), this large at the heart and in the other cells.
const BLAST_GLOW: f32 = 0.1;
const BLAST_GLOW_SIZE: (f32, f32) = (3.4, 1.6);
/// The haze at a frosty or magical blast's heart: its puffs this many
/// times their size, in a ball as many times wider as its root.
const BLAST_BODY: f32 = 1.8;
/// A magical blast's heart: this many times its colour (over one it reads
/// as a light at the game's distance, and still as its hue).
const MAGIC_CORE: f32 = 1.5;
/// A spell gathering at the hand: its murk this much of its colour.
const CAST_MURK: f32 = 0.4;
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
    let mut all: HashMap<Recipe, Gd<Texture2D>> = [
        (Recipe::Flash, format!("{rpicster}effect_3.png")),
        (Recipe::Wisps, format!("{kenney}twirl_02.png")),
        (Recipe::Ring, format!("{kenney}circle_02.png")),
        (Recipe::Tongues, format!("{books}FireBall04_8x8.png")),
        // (FireBall01 is burnt out to white over half of each ball: no
        // heat left in it to draw a fire's colours by)
        (Recipe::Pillar, format!("{books}FireBall02_8x8.png")),
        (Recipe::Blaze, format!("{books}FireBall02_8x8.png")),
        (Recipe::Flames, format!("{books}FireBall02_8x8.png")),
        (Recipe::Haze, smoke.clone()),
        (Recipe::Smoke, smoke.clone()),
        (Recipe::Gas, smoke.clone()),
        (Recipe::Dust, smoke.clone()),
        (Recipe::Soot, smoke.clone()),
        (Recipe::Murk, smoke.clone()),
        (Recipe::Puff, smoke),
    ]
    .into_iter()
    .filter_map(|(r, path)| {
        godot::tools::try_load::<Texture2D>(&path)
            .ok()
            .map(|t| (r, t))
    })
    .collect();
    // drawn here (`shapes`): hot specks for glints, flares and what a
    // spell flings, slivers of ice, runes, wisps of light; never a stock
    // four-point star
    let speck = shapes::speck();
    for r in [
        Recipe::Glints,
        Recipe::Flare,
        Recipe::Twinkles,
        Recipe::Spread,
        Recipe::Swirl,
    ] {
        all.insert(r, speck.clone());
    }
    let sliver = shapes::sliver();
    all.insert(Recipe::Rime, sliver.clone());
    all.insert(Recipe::Shards, sliver);
    let runes = shapes::runes();
    all.insert(Recipe::Runes, runes.clone());
    all.insert(Recipe::Halo, runes);
    all.insert(Recipe::Mend, shapes::wisp());
    all
}
/// Motes of dust around the camera's focus in the main dungeon.
const DUST: i32 = 220;
/// Where the dust hangs over the floor: the middle of its box and half
/// its height (metres: from knee height up).
const DUST_HIGH: (f32, f32) = (1.6, 0.9);
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
    /// The creatures a ray burst on lately: their cells and when (a ray
    /// that bounces back crosses them again).
    hits: Vec<((i32, i32), f32)>,
    /// A number for the next ray's pattern.
    rays_drawn: u32,
    columns: Vec<Column>,
    /// Rolling fire's shader (fire.gdshader), a tower of flame's (the
    /// torches').
    fire_shader: Option<Gd<Shader>>,
    flame_shader: Option<Gd<Shader>>,
    flame_book: Option<Gd<Texture2D>>,
    /// When (`clock`) the last explosion went off, and the last fire.
    last_blast: f32,
    last_fire: f32,
    splats: Vec<Splat>,
    next_splat: usize,
    /// What effects leave on the floor a while.
    marks: marks::Marks,
    pending: Vec<Pending>,
    beam_shader: Option<Gd<Shader>>,
    dust: Gd<GpuParticles3D>,
    /// The dust's looks: motes, or embers rising (and which it is).
    mote_ramp: Gd<Texture2D>,
    mote_mat: Gd<Material>,
    ember_ramp: Gd<Texture2D>,
    ember_mat: Gd<Material>,
    embers_in_air: bool,
    /// The colour of the level's stone (its walls'): the dust and chips
    /// force strikes from it.
    stone: Color,
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
pub(crate) fn particle_material(
    tex: &Gd<Texture2D>,
    additive: bool,
    shaded: bool,
    frames: Option<(i32, i32)>,
) -> Gd<Material> {
    let mut m = StandardMaterial3D::new_gd();
    m.set_billboard_mode(BillboardMode::PARTICLES);
    // a billboard drops the particle's own scale unless told to keep it:
    // the sizes a recipe ranges over, a ring's spread, smoke's swell
    m.set_flag(Flags::BILLBOARD_KEEP_SCALE, true);
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
    } else if recipe.is_fire() {
        // not a colour: how hot it is, how much of it is soot, how thick
        // (fire.gdshader): it cools, and rolls off as dark smoke
        return fire_ramp();
    } else if recipe == Recipe::Embers {
        // a yellow-orange speck dying to a dull red
        (
            &[0.0, 0.25, 0.7, 1.0],
            vec![
                rgba(2.6, 1.1, 0.24, 1.0),
                rgba(1.6, 0.5, 0.08, 1.0),
                rgba(0.7, 0.12, 0.02, 0.8),
                rgba(0.3, 0.03, 0.0, 0.0),
            ],
        )
    } else if recipe.ring().is_some() {
        // round a body: it comes up softly, and fades as it rises
        let k = p.hdr;
        let hot = rgba(color.r * k, color.g * k, color.b * k, 1.0);
        (
            &[0.0, 0.2, 0.6, 1.0],
            vec![
                hot.with_alpha(0.0),
                hot,
                rgba(color.r * k * 0.6, color.g * k * 0.6, color.b * k * 0.6, 0.7),
                color.with_alpha(0.0),
            ],
        )
    } else if matches!(recipe, Recipe::Grit | Recipe::Chips) {
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

/// A flame's life for fire.gdshader: (heat, soot, -, thickness).
fn fire_ramp() -> Gd<GradientTexture1D> {
    let rgba = Color::from_rgba;
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.1, 0.45, 0.72, 1.0][..]));
    g.set_colors(&PackedColorArray::from(
        &[
            rgba(1.0, 0.0, 0.0, 0.0),
            rgba(1.0, 0.0, 0.0, 1.0),
            rgba(0.6, 0.1, 0.0, 1.0),
            rgba(0.25, 0.7, 0.0, 0.8),
            rgba(0.0, 1.0, 0.0, 0.0),
        ][..],
    ));
    let mut t = GradientTexture1D::new_gd();
    t.set_gradient(&g);
    t.set_use_hdr(true);
    t
}

pub(crate) fn ramp(color: Color, hdr: f32) -> Gd<GradientTexture1D> {
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
            hits: Vec::new(),
            rays_drawn: 0,
            columns: Vec::new(),
            fire_shader: godot::tools::try_load::<Shader>("res://shaders/fire.gdshader").ok(),
            flame_shader: godot::tools::try_load::<Shader>("res://shaders/flame.gdshader").ok(),
            flame_book: godot::tools::try_load::<Texture2D>(
                "res://art/cc0/unity-labs/flipbooks/Flame02_16x4.png",
            )
            .ok(),
            last_blast: f32::NEG_INFINITY,
            last_fire: f32::NEG_INFINITY,
            splats: Vec::new(),
            next_splat: 0,
            marks: marks::Marks::new(),
            pending: Vec::new(),
            beam_shader,
            dust,
            mote_ramp,
            mote_mat,
            ember_ramp,
            ember_mat,
            embers_in_air: false,
            stone: STONE,
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

    /// An effect drawn larger and brighter (`LARGE`): a monster's drink,
    /// read or wield seen at the game's distance, where its motes, runes
    /// and glints were specks.
    pub fn burst_large(&mut self, kind: VfxKind, at: Vector3) {
        self.burst_by(kind, at, None, None, LARGE);
    }

    /// An effect on a body (`Aura`, `Halo`, `Heal`) standing at `feet`,
    /// `tall` metres tall: it rises round all of it, its light at its
    /// chest.
    pub fn on_body(&mut self, kind: VfxKind, feet: Vector3, tall: f32) {
        let k = (tall / BODY_TALL).clamp(0.5, 2.5);
        self.burst_by(kind, feet, None, None, k);
    }

    /// A ray or a bolt of `color` striking something solid at `at`, from
    /// `from`'s side: its sparks thrown back that way.
    pub fn impact(&mut self, color: Color, at: Vector3, from: Vector3) {
        let back = Vector3::new(from.x - at.x, 0.0, from.z - at.z).normalized();
        let dir = (back + Vector3::UP * 0.5).normalized();
        // a grey wand's strike still flashes white
        let at = at + back * IMPACT_OFF;
        self.burst_dir(VfxKind::Impact(vivid(color)), at, None, Some(dir));
    }

    /// A ray of `kind`'s strike on something solid at `at` from `from`'s
    /// side (the hero's own beam at arm's length from a wall).
    pub fn impact_of(&mut self, kind: VfxKind, at: Vector3, from: Vector3) {
        self.strike(ray_of(kind), beam_color(kind), at, from);
    }

    /// A ray of `kind` and `color` striking stone at `at` from `from`'s
    /// side: the flash and sparks of any strike, then what is its own: a
    /// fire's flames and embers, frost's shards and rime, a bolt's forks,
    /// acid's spatter, gas, glitter; its own light; and its mark at the
    /// foot of what it struck. (At point-blank a cold ray was a small
    /// sparkle.)
    fn strike(&mut self, kind: Ray, color: Color, at: Vector3, from: Vector3) {
        self.impact(color, at, from);
        let back = Vector3::new(from.x - at.x, 0.0, from.z - at.z).normalized();
        let out = (back + Vector3::UP * 0.6).normalized();
        let near = at + back * IMPACT_OFF;
        let foot = Vector3::new(at.x, 0.0, at.z) + back * STRIKE_MARK_BACK;
        let rgb = Color::from_rgb;
        match kind {
            Ray::Fire => {
                let fire = rgb(1.0, 0.4, 0.1);
                self.emit_by(Recipe::Flames, fire, near, Some(Vector3::UP), 1.3);
                self.emit_by(Recipe::Embers, fire, near, Some(out), 1.2);
                let over = near + Vector3::new(0.0, 0.7, 0.0);
                self.emit_by(Recipe::Soot, fire, over, None, 0.6);
                self.flash(near, fire, 5.0, 0.9, 4.0);
            }
            Ray::Cold => self.frost_burst(near + back * 0.2, Some(out), foot + back * 0.3),
            Ray::Lightning => {
                let bolt = rgb(0.85, 0.9, 1.0);
                self.emit_by(Recipe::Bolts, bolt, near, None, 1.4);
                self.flash(near, bolt, 7.0, 0.3, 4.5);
            }
            Ray::Acid => {
                let acid = rgb(0.6, 0.95, 0.2);
                self.emit_by(Recipe::Droplets, acid, near, Some(out), 1.3);
                self.flash(near, acid, 2.0, 0.6, 3.0);
            }
            Ray::Poison => {
                let gas = rgb(0.56, 1.0, 0.25);
                self.emit_by(Recipe::Cloud, gas, near, Some(out), 1.3);
                self.flash(near, gas, 2.0, 0.8, 3.0);
            }
            Ray::MagicMissile | Ray::Plain | Ray::Sleep => {
                let c = vivid(kind.look().color.unwrap_or(color));
                self.emit_by(Recipe::Glints, c, near, None, 2.4);
                self.flash(near, c, 3.0, 0.5, 3.5);
            }
            Ray::Death => self.shade(near, 3.0, 0.8, 3.5),
            Ray::Dig | Ray::Force => {}
        }
        // (cold's rime is its burst's own, wider)
        if let Some((mark, size)) = kind.mark(color)
            && kind != Ray::Cold
        {
            self.mark(mark, foot, size * STRIKE_MARK);
        }
    }

    /// A cold ray's burst at `at`: shards flung out (along `out`, else all
    /// round), rime and a cold haze in a ball a cell or two wide, glints,
    /// its light, and the rime it leaves on the floor at `foot`.
    fn frost_burst(&mut self, at: Vector3, out: Option<Vector3>, foot: Vector3) {
        let rgb = Color::from_rgb;
        let ice = rgb(0.56, 0.88, 1.0);
        let haze = rgb(0.78, 0.9, 1.0);
        self.emit_by(Recipe::Shards, ice, at, out, 2.2);
        self.emit_ball(Recipe::Rime, ice, at, 1.0, FROST_BURST);
        self.emit_ball(Recipe::Haze, haze, at, 0.7, FROST_BURST);
        self.emit_by(Recipe::Glints, ice, at, None, 2.6);
        self.emit_by(Recipe::Cloud, haze, at, out, 1.6);
        self.flash(at, ice, 6.0, 1.0, 5.0);
        self.mark(Mark::Frost, foot, FROST_RIME);
    }

    /// A ray of `kind` crossing a creature at `at` (the ray's height over
    /// its cell; `foot` the floor there): cold bursts into frost on it, a
    /// cell or two wide (at point-blank the ray is all but hidden behind
    /// the creature it strikes). Once a creature a ray, its bounce too.
    pub fn ray_hit(&mut self, kind: Ray, at: Vector3, foot: Vector3) {
        if kind != Ray::Cold {
            return;
        }
        let cell = (foot.x.round() as i32, foot.z.round() as i32);
        let now = self.clock;
        self.hits.retain(|&(_, when)| now - when < RAY_HIT_ONCE);
        if self.hits.iter().any(|&(c, _)| c == cell) {
            return;
        }
        self.hits.push((cell, now));
        self.frost_burst(at, None, foot);
    }

    /// A body come down on the floor at `at` (its feet), `size` metres
    /// across: dust of the level's stone thrown up round it and left
    /// lying a while.
    pub fn thud(&mut self, at: Vector3, size: f32) {
        let s = self.stone;
        let dust = Color::from_rgb(
            (s.r * 1.6).min(1.0),
            (s.g * 1.6).min(1.0),
            (s.b * 1.6).min(1.0),
        );
        self.emit_by(
            Recipe::Puff,
            dust,
            at + Vector3::UP * 0.1,
            Some(Vector3::UP),
            size,
        );
        self.mark(Mark::Dust(s), Vector3::new(at.x, 0.0, at.z), size * 1.3);
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
        self.burst_by(kind, at, tint, dir, 1.0);
    }

    /// As `burst_dir`, `k` times as large (and brighter, its light too:
    /// `LARGE_GLOW`, `LARGE_LIGHT_UP`), a light of its own for a kind with
    /// none.
    fn burst_by(
        &mut self,
        kind: VfxKind,
        at: Vector3,
        tint: Option<Color>,
        dir: Option<Vector3>,
        k: f32,
    ) {
        let (recipe, color, light) = look_of(kind);
        let color = tint.unwrap_or(color);
        let k = if matches!(kind, VfxKind::Bless) {
            k * BLESS_SIZE
        } else {
            k
        };
        // a dark potion's glow still a glow, in its hue
        let (color, light) = match recipe.ring() {
            Some(_) => (vivid(color), light.map(|(c, e, s)| (vivid(c), e, s))),
            None => (color, light),
        };
        let large = k >= LARGE;
        let light = light
            .map(|(c, e, s)| (tint.unwrap_or(c), e, s))
            .or_else(|| large.then_some((color, LARGE_LIGHT.0, LARGE_LIGHT.1)));
        let (glow, (up_k, longer)) = match large {
            true => (LARGE_GLOW, LARGE_LIGHT_UP),
            false => (1.0, (1.0, 1.0)),
        };
        let bright = Color::from_rgba(color.r * glow, color.g * glow, color.b * glow, color.a);
        self.emit_by(recipe, bright, at, dir, k);
        match kind {
            // a strike on stone: a flash, larger than a blow's, and
            // flecks of light flung every way
            VfxKind::Impact(_) => {
                self.emit_by(Recipe::Bloom, bright, at, None, k * IMPACT_FLASH);
                self.emit_by(Recipe::Flare, bright, at, None, k * IMPACT_FLASH);
            }
            _ if recipe == Recipe::Sparks => self.emit_by(Recipe::Flash, bright, at, None, k),
            // a spell gathering: a dusky puff of its colour, the hot
            // points turning in it
            VfxKind::Cast(_) => {
                let m = CAST_MURK;
                let murk = Color::from_rgb(color.r * m, color.g * m, color.b * m);
                self.emit_by(Recipe::Murk, murk, at, None, k);
            }
            // a thing burning: embers off its flames
            VfxKind::Fire => self.emit_by(Recipe::Embers, color, at, None, k * 0.6),
            // a glow rising with the runes; wisps turning in a drink's
            // glow, and in a healing's with its stars
            VfxKind::Halo => self.emit_by(Recipe::Aura, color, at, None, k),
            // cold: glinting ice and a breath of frost about the shards
            // (at point-blank it was a small sparkle)
            VfxKind::Frost => {
                self.emit_by(Recipe::Glints, bright, at, None, k * 2.0);
                let mist = Color::from_rgb(0.78, 0.9, 1.0);
                self.emit_by(Recipe::Cloud, mist, at, None, k * 1.2);
            }
            VfxKind::Aura(_) => self.emit_by(Recipe::Wisps, bright, at, None, k),
            VfxKind::Heal | VfxKind::Bless => {
                self.emit_by(Recipe::Aura, color, at, None, k);
                self.emit_by(Recipe::Wisps, color, at, None, k);
            }
            // the light bursting from the hero: specks of it flung out
            // low over the floor all round, a flare at their chest, and
            // the light itself, bright and settling (the room's own
            // lights come up behind it: `MapView::show_lit`)
            VfxKind::Lit => {
                let low = Vector3::new(at.x, at.y + 0.4, at.z);
                self.emit_by(Recipe::Spread, LIT, low, None, k);
                self.emit_by(Recipe::Bloom, LIT, at + Vector3::UP * 1.0, None, k * 2.0);
                let (energy, secs, range) = LIT_LIGHT;
                self.flash(at + Vector3::UP * 1.4, LIT, energy, secs, range);
            }
            // an item's glow, in its colour (a white flare over it read as
            // a white blob): a small soft glow on it, motes swirling round
            // it and glints, its light round it (map_view's rim on the
            // hero in the same colour)
            VfxKind::Gleam(_) => {
                self.emit_by(Recipe::Glow, color, at, None, k * 0.6);
                self.emit_by(Recipe::Glints, color, at, None, k * 2.0);
            }
            // force striking: a flare, the dust it throws up in the colour
            // of the level's stone, lit by the strike a moment, and on
            // stone chips of it flung back (the bolt's own rings came
            // before; the dust a flat grey sprite read as nothing struck)
            VfxKind::Force(_, stone) => {
                self.emit_by(Recipe::Flare, bright, at, None, k * 1.5);
                let s = self.stone;
                let dust = Color::from_rgb(
                    (s.r * 1.5 + color.r * 0.15).min(1.0),
                    (s.g * 1.5 + color.g * 0.15).min(1.0),
                    (s.b * 1.5 + color.b * 0.15).min(1.0),
                );
                self.emit_by(Recipe::Puff, dust, at, dir, k * 1.4);
                if stone {
                    let chips = Color::from_rgb(s.r * 0.8, s.g * 0.8, s.b * 0.8);
                    self.emit_by(Recipe::Chips, chips, at, dir, k * 1.3);
                    // and its dust settles at the stone's foot
                    self.mark(Mark::Dust(s), Vector3::new(at.x, 0.0, at.z), 1.2 * k);
                }
                // the strike's glow lingering in the dust it raised
                let warm = Color::from_rgb(1.0, 0.82, 0.6);
                self.flash(
                    at + Vector3::UP * 0.2,
                    warm,
                    FORCE_DUST_LIGHT.0,
                    FORCE_DUST_LIGHT.1,
                    2.2,
                );
            }
            _ => {}
        }
        if let Some((c, energy, secs)) = light {
            let range = 2.5 * k.sqrt();
            // on a body, at its chest
            let up = if recipe.ring().is_some() {
                BODY_TALL * 0.55 * k
            } else {
                0.3
            };
            let at = at + Vector3::new(0.0, up, 0.0);
            self.flash(at, c, energy * up_k, secs * longer, range);
        }
        if let VfxKind::Blood(c) = kind {
            // blood darkens as it lies: never the bright red of the glyph
            let k = SPLAT_DARK;
            let dark = Color::from_rgb(c.r * k, c.g * k, c.b * k);
            self.splat(Vector3::new(at.x, 0.0, at.z), dark, 1.0);
        }
        // what it leaves on the floor under it a while
        let ground = Vector3::new(at.x, 0.0, at.z);
        match kind {
            VfxKind::Explosion(_) => self.mark(Mark::Scorch(EMBER), ground, 1.4),
            VfxKind::Fire => self.mark(Mark::Scorch(EMBER), ground, 1.0 * k),
            VfxKind::Frost => self.mark(Mark::Frost, ground, 1.1 * k),
            VfxKind::Poison => self.mark(Mark::Etch(color), ground, 0.9 * k),
            _ => {}
        }
    }

    /// A cell of an explosion of `kind` at `at` on the ground, `off` its
    /// place from the explosion's middle (x and y each -1..1; (0, 0) the
    /// heart), `color` its glyph's: each kind as what it is. The cells
    /// round the heart burst from nearer it (the corners nearer still, and
    /// less), so that the whole is round and not a box of nine; a fire's
    /// flames lean out from the heart and lick past the cells' edges.
    pub fn blast(&mut self, kind: Blast, off: (i32, i32), color: Color, at: Vector3) {
        self.last_blast = self.clock;
        let heart = off == (0, 0);
        let corner = off.0 != 0 && off.1 != 0;
        // the way out from the heart (none at the heart)
        let out = Vector3::new(off.0 as f32, 0.0, off.1 as f32)
            .try_normalized()
            .unwrap_or(Vector3::ZERO);
        let pull = if corner {
            BLAST_PULL_CORNER
        } else {
            BLAST_PULL
        };
        let at = at - Vector3::new(off.0 as f32, 0.0, off.1 as f32) * pull;
        let share = if corner {
            BLAST_SHARE * BLAST_CORNER
        } else {
            BLAST_SHARE
        };
        let size = if corner { BLAST_CORNER } else { 1.0 };
        let mid = at + Vector3::new(0.0, 0.6, 0.0);
        let rgb = Color::from_rgb;
        match kind {
            Blast::Fiery => {
                self.last_fire = self.clock;
                // a deep orange (born yellow-orange: `ramp_of`), so that
                // where the flames overlap it burns yellow and only its
                // very heart white
                let fire = rgb(1.0, 0.4, 0.1);
                if heart {
                    // a tower of flame, a column of fire rolling up it, a
                    // ball of flame round its foot, embers flung out, the
                    // scorch it leaves
                    self.column(at);
                    self.emit(Recipe::Pillar, fire, at + Vector3::new(0.0, 0.3, 0.0), None);
                    let low = at + Vector3::new(0.0, 0.3, 0.0);
                    self.emit_by(Recipe::Blaze, fire, low, None, BLAST_HEART_BLAZE);
                    self.emit(Recipe::Embers, fire, mid, None);
                    // dark smoke rolling up off it
                    let over = at + Vector3::new(0.0, SOOT_HIGH, 0.0);
                    self.emit(Recipe::Soot, fire, over, None);
                    self.flash(at + Vector3::new(0.0, 1.4, 0.0), fire, 5.5, 1.3, 5.0);
                    self.mark(Mark::Scorch(EMBER), at, 2.6);
                } else {
                    // flames leaning out from the heart
                    let lean = (out + Vector3::UP * BLAST_LEAN).normalized();
                    let low = at + Vector3::new(0.0, 0.25, 0.0);
                    self.emit_by(Recipe::Blaze, fire, low, Some(lean), BLAST_BLAZE * size);
                    self.mark(Mark::Scorch(EMBER), at, 1.2 * size);
                }
            }
            Blast::Frosty => {
                // a pale-blue ball of haze glowing from within, shards and
                // rime flung through it
                let ice = rgb(0.45, 0.7, 1.0);
                let lean = if heart {
                    None
                } else {
                    Some((out + Vector3::UP).normalized())
                };
                // dusky: a cold grey-blue, not a bright one
                let haze = Some(rgb(0.4, 0.5, 0.62));
                self.blast_body(rgb(0.35, 0.62, 1.0), haze, mid, heart, size);
                if !corner {
                    self.emit_by(Recipe::Shards, ice, mid, lean, 1.3);
                }
                // (a few glints of rime in it: the body is the haze)
                self.emit_in_cell(Recipe::Rime, ice, mid, share * 0.4);
                if heart {
                    self.flash(mid, ice, 3.0, 0.5, 4.0);
                }
                self.mark(Mark::Frost, at, if heart { 1.9 } else { 1.2 * size });
            }
            // a cloud of its gas over the cell, lit green from within
            Blast::Noxious => {
                let gas = rgb(0.55, 0.78, 0.2);
                self.emit_in_cell(Recipe::Gas, gas, mid, share);
                if heart {
                    self.flash(mid, gas, 2.5, 0.9, 4.0);
                    self.mark(Mark::Etch(gas), at, 1.4);
                }
            }
            // black smoke, the light round it put out a moment
            Blast::Dark => {
                self.emit_in_cell(Recipe::Smoke, color, mid, share);
                if heart {
                    self.shade(mid, 3.0, 1.2, 4.5);
                }
            }
            // a splash flung up and a mist; mud's clods, its brown haze and
            // the mud it leaves
            Blast::Wet | Blast::Muddy => {
                let (drops, mist) = if kind == Blast::Wet {
                    (rgb(0.35, 0.55, 0.85), rgb(0.7, 0.8, 0.9))
                } else {
                    (rgb(0.3, 0.22, 0.14), rgb(0.36, 0.28, 0.2))
                };
                let up = Some(Vector3::UP);
                if kind == Blast::Wet {
                    self.emit(Recipe::Spray, drops, mid, up);
                    self.emit_in_cell(Recipe::Gas, mist, mid, share);
                    if heart {
                        self.flash(mid, drops, 1.6, 0.6, 3.5);
                    }
                } else {
                    self.emit(Recipe::Grit, drops, mid, up);
                    self.emit_in_cell(Recipe::Gas, mist, mid, share);
                    self.splat(at, drops, size);
                }
                self.emit(Recipe::Droplets, drops, mid, None);
            }
            Blast::Magical => {
                // a ball of its light in a haze of its colour, sparks flung
                // from it
                let c = vivid(color);
                // dusky: its colour darkened, as smoke lit from within
                let haze = Color::from_rgb(c.r * 0.42, c.g * 0.34, c.b * 0.5);
                self.blast_body(c, Some(haze), mid, heart, size);
                // at its heart a ball of its own colour (drawn over the
                // haze: it keeps its hue) and sparks flung from it
                if heart {
                    let core =
                        Color::from_rgb(c.r * MAGIC_CORE, c.g * MAGIC_CORE, c.b * MAGIC_CORE);
                    self.emit(Recipe::Core, core, mid, None);
                    self.emit(Recipe::Blast, color, mid, None);
                }
                if heart {
                    self.flash(mid, color, 4.0, 0.4, 4.0);
                }
                self.mark(Mark::Glitter(color), at, 1.2 * size);
            }
        }
    }

    /// A blast's body over a cell at `at`: a haze of `haze` (none if it
    /// has its own), a soft glow of `color` in it, larger at the heart,
    /// `size` its share elsewhere (a corner's less).
    fn blast_body(
        &mut self,
        color: Color,
        haze: Option<Color>,
        at: Vector3,
        heart: bool,
        size: f32,
    ) {
        let k = if heart {
            BLAST_GLOW_SIZE.0
        } else {
            BLAST_GLOW_SIZE.1 * size
        };
        match haze {
            // at the heart a dense ball of it, its puffs larger
            Some(h) if heart => self.emit_by(Recipe::Haze, h, at, None, BLAST_BODY),
            Some(h) => self.emit_by(Recipe::Haze, h, at, None, BLAST_BODY * 0.75 * size),
            None => {}
        }
        let dim = Color::from_rgb(
            color.r * BLAST_GLOW,
            color.g * BLAST_GLOW,
            color.b * BLAST_GLOW,
        );
        self.emit_by(Recipe::Glow, dim, at, None, k);
    }

    /// What a ray of its kind sheds, shed round `at` instead (an
    /// explosion's cell): a ball about a cell wide (`BLAST_BALL`), `share`
    /// of the amount.
    fn emit_in_cell(&mut self, recipe: Recipe, color: Color, at: Vector3, share: f32) {
        self.emit_ball(recipe, color, at, share, BLAST_BALL);
    }

    /// What a ray of its kind sheds, shed in a ball of `radius` metres
    /// round `at`, `share` of the amount.
    fn emit_ball(&mut self, recipe: Recipe, color: Color, at: Vector3, share: f32, radius: f32) {
        let i = self.take_emitter(recipe);
        let Some(e) = self
            .emitters
            .get_mut(&recipe)
            .and_then(|pool| pool.get_mut(i))
        else {
            return;
        };
        let color = recipe.own_color().unwrap_or(color);
        e.process.set_color_ramp(&ramp_of(recipe, color));
        e.process.set_emission_shape(EmissionShape::SPHERE);
        e.process.set_emission_sphere_radius(radius);
        e.node.set_transform(Transform3D::new(Basis::IDENTITY, at));
        e.node.set_amount_ratio(share);
        e.node.set_visibility_aabb(Aabb::new(
            Vector3::new(-2.0, -2.0, -2.0),
            Vector3::new(4.0, 5.0, 4.0),
        ));
        e.node.set_visible(true);
        e.node.restart();
        e.node.set_emitting(true);
        e.busy = recipe.shows();
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
                        // its foot fading in: a torch's flame cut off at
                        // the quad's edge drew a hard bright square
                        mat.set_shader_parameter("foot", &COLUMN_FOOT.to_variant());
                        // a deep orange, not a torch's yellow
                        mat.set_shader_parameter("deeper", &1.0f32.to_variant());
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
        let (i, old) = match grown {
            Some((i, from, to)) => {
                let old = (self.beams[i].from, self.beams[i].to);
                self.grow(i, from, to);
                (i, Some(old))
            }
            None => (self.place_ray(kind, color, from, to, false), None),
        };
        self.cap(i, old);
    }

    /// Where ray `i` has come to (an end new since `old`, or either end of
    /// a new one) runs into a wall or rock: it strikes there, in a flash
    /// (once for each solid it meets), not cut off square.
    fn cap(&mut self, i: usize, old: Option<(Vector3, Vector3)>) {
        let b = &self.beams[i];
        let len = b.from.distance_to(b.to);
        if len < 0.1 {
            return;
        }
        let out = (b.to - b.from) / len;
        let mut ends = Vec::new();
        if old.is_none_or(|(_, to)| to != b.to) {
            ends.push((b.to, out));
        }
        if old.is_none_or(|(from, _)| from != b.from) {
            ends.push((b.from, -out));
        }
        let color = b.kind.look().color.unwrap_or(b.color);
        let kind = b.kind;
        for (end, out) in ends {
            let beyond = end + out * CAP_REACH;
            let cell = (beyond.x.round() as i32, beyond.z.round() as i32);
            let solid = self.solids.get(&cell).is_some_and(|&top| end.y < top);
            if !solid || self.beams[i].capped.contains(&cell) {
                continue;
            }
            self.beams[i].capped.push(cell);
            self.strike(kind, color, end, end - out);
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

    fn place_ray(
        &mut self,
        kind: Ray,
        color: Color,
        from: Vector3,
        to: Vector3,
        whole: bool,
    ) -> usize {
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
        b.capped.clear();
        self.grow(i, from, to);
        i
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
            let Some(e) = self
                .emitters
                .get_mut(&recipe)
                .and_then(|pool| pool.get_mut(i))
            else {
                continue;
            };
            let color = recipe.own_color().unwrap_or(color);
            e.process.set_color_ramp(&ramp_of(recipe, color));
            // (a blast's cell left it a ball)
            e.process.set_emission_shape(EmissionShape::BOX);
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
            e.busy = recipe.shows();
        }
        // and what it leaves on the floor under its way a while
        if let Some((mark, size)) = kind.mark(color) {
            self.mark_along(mark, from, to, size);
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
            capped: Vec::new(),
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
                let fire = self.fire_shader.as_ref().filter(|_| recipe.is_fire());
                let e = new_emitter(&mut self.root, recipe, tex, fire);
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
        self.emit_by(recipe, color, at, dir, 1.0);
    }

    /// As `emit`, its particles `k` times their size and spread over a
    /// sphere as many times wider as `k`'s root.
    fn emit_by(&mut self, recipe: Recipe, color: Color, at: Vector3, dir: Option<Vector3>, k: f32) {
        let i = self.take_emitter(recipe);
        let p = recipe.params();
        let Some(e) = self
            .emitters
            .get_mut(&recipe)
            .and_then(|pool| pool.get_mut(i))
        else {
            return;
        };
        // as large as this burst asks (the pool's emitters are shared): a
        // particle is drawn its quad's size (a billboard keeps nothing of
        // the scale its process gives it), the quad made larger
        if e.sized != k {
            e.sized = k;
            let quad = match e.quad.clone().try_cast::<QuadMesh>() {
                Ok(q) if k != 1.0 => {
                    let mut large = QuadMesh::new_gd();
                    large.set_size(q.get_size() * k);
                    large.upcast()
                }
                _ => e.quad.clone(),
            };
            e.node.set_draw_pass_mesh(0, &quad);
        }
        // and as its recipe makes it: a ray's trail (`shed`) or a blast's
        // cell (`emit_in_cell`) left it turned along the ray, emitting from
        // a box, a share of its amount
        let mut at = at;
        if let Some((over, band)) = recipe.ring() {
            // round a body as wide as it is tall, from its feet
            e.process.set_emission_shape(EmissionShape::RING);
            e.process.set_emission_ring_axis(Vector3::UP);
            e.process.set_emission_ring_radius(p.radius * k);
            e.process.set_emission_ring_inner_radius(p.radius * k * 0.7);
            e.process.set_emission_ring_height(BODY_TALL * band * k);
            at.y += BODY_TALL * over * k;
        } else {
            // (a ray's trail left it a box along the ray)
            e.process.set_emission_shape(EmissionShape::SPHERE);
        }
        e.process.set_emission_sphere_radius(p.radius * k.sqrt());
        e.node.set_amount_ratio(1.0);
        e.node.set_visibility_aabb(EMITTER_SEEN);
        e.process.set_color_ramp(&ramp_of(recipe, color));
        e.process.set_direction(dir.unwrap_or(p.dir));
        e.node.set_transform(Transform3D::new(Basis::IDENTITY, at));
        e.node.set_visible(true);
        e.node.restart();
        e.node.set_emitting(true);
        e.busy = recipe.shows();
    }

    /// A light at `at` dying out over `secs`, reaching `range` metres.
    fn flash(&mut self, at: Vector3, color: Color, energy: f32, secs: f32, range: f32) {
        self.light_up(at, color, energy, secs, range, false);
    }

    /// The light round `at` put out a moment (a darkness's): a light that
    /// takes away, dying out over `secs`.
    fn shade(&mut self, at: Vector3, energy: f32, secs: f32, range: f32) {
        self.light_up(at, Color::WHITE, energy, secs, range, true);
    }

    fn light_up(
        &mut self,
        at: Vector3,
        color: Color,
        energy: f32,
        secs: f32,
        range: f32,
        negative: bool,
    ) {
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
        f.light.set_negative(negative);
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
        // a slow frame ages nothing by more than a step: the particles
        // do not leap ahead by it either, and a burst would be over
        // before it was seen
        let delta = delta.min(SLOW_FRAME);
        self.clock += delta;
        self.dust
            .set_position(focus + Vector3::new(0.0, DUST_HIGH.0, 0.0));
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
                due_shed.push((b.kind, b.color, b.from, b.to, b.capped.is_empty()));
            }
            if !b.live() {
                b.node.set_visible(false);
                b.light.set_visible(false);
            }
        }
        for (kind, color, from, to, unstruck) in due_shed {
            self.shed(kind, color, from, to);
            // a cold ray seen only a cell or two (point-blank: it struck
            // what stands there, or went on unseen) bursts at its end
            if kind == Ray::Cold && unstruck && from.distance_to(to) < POINT_BLANK {
                let foot = to - Vector3::new(0.0, RAY_OVER, 0.0);
                self.ray_hit(kind, to, foot);
            }
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
                m.set_shader_parameter("energy", &(COLUMN_ENERGY * waver).to_variant());
                // (thinning out as it dies: its light gone, its body
                // stood on as a grey veil)
                m.set_shader_parameter("left", &fade.to_variant());
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
        self.age_marks(delta);
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

    /// One of each kind kept (its shader stays compiled), the rest freed:
    /// what a rehearsal fired is no game's to keep.
    pub fn shed_spares(&mut self) {
        self.clear();
        for pool in self.emitters.values_mut() {
            for mut e in pool.drain(1.min(pool.len())..) {
                e.node.queue_free();
            }
        }
        for mut f in self.flashes.drain(1.min(self.flashes.len())..) {
            f.light.queue_free();
        }
        for mut b in self.beams.drain(1.min(self.beams.len())..) {
            b.node.queue_free();
            b.light.queue_free();
        }
        for mut s in self.splats.drain(1.min(self.splats.len())..) {
            s.decal.queue_free();
        }
        for mut c in self.columns.drain(1.min(self.columns.len())..) {
            c.root.queue_free();
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
        self.hits.clear();
        for c in &mut self.columns {
            c.age = COLUMN_SECS;
            c.root.set_visible(false);
        }
        for s in &mut self.splats {
            s.age = SPLAT_SECS;
            s.decal.set_visible(false);
        }
        self.clear_marks();
    }

    /// How thick the dust in the air is (1: the main dungeon's).
    /// Whether the dust draws (its first frame runs its pre-process).
    pub fn set_dust_shown(&mut self, on: bool) {
        if self.dust.is_visible() != on {
            self.dust.set_visible(on);
        }
    }

    /// `embers`: glowing embers rising slowly instead (the fire below).
    /// The colour of the level's stone (its walls'), for the dust force
    /// strikes from it.
    pub fn set_stone(&mut self, color: Color) {
        self.stone = color;
    }

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
            (&self.mote_ramp, &self.mote_mat, 0.0)
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
    if recipe == Recipe::Wisps {
        // each turning as it rises, from any start
        m.set_param_min(Parameter::ANGLE, 0.0);
        m.set_param_max(Parameter::ANGLE, 360.0);
        m.set_param_min(Parameter::ANGULAR_VELOCITY, 160.0);
        m.set_param_max(Parameter::ANGULAR_VELOCITY, 320.0);
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
    // turned any way: a ring, smoke, flame and ice are no two alike
    let turned = matches!(recipe, Recipe::Rime | Recipe::Shards);
    if grows.is_some() || turned || (recipe.frames().is_some() && !recipe.picks()) {
        m.set_param_min(Parameter::ANGLE, 0.0);
        m.set_param_max(Parameter::ANGLE, 360.0);
    }
    if recipe.picks() {
        // one shape of its sheet, all its life
        m.set_param_min(Parameter::ANIM_SPEED, 0.0);
        m.set_param_max(Parameter::ANIM_SPEED, 0.0);
        m.set_param_min(Parameter::ANIM_OFFSET, 0.0);
        m.set_param_max(Parameter::ANIM_OFFSET, 1.0);
    } else if recipe.frames().is_some() {
        // its flipbook once over its life, from a little way in
        m.set_param_min(Parameter::ANIM_SPEED, 1.0);
        m.set_param_max(Parameter::ANIM_SPEED, 1.0);
        m.set_param_min(Parameter::ANIM_OFFSET, 0.0);
        m.set_param_max(Parameter::ANIM_OFFSET, 0.12);
    }
    m
}

/// Where a pooled emitter's burst may reach round it (it is drawn while
/// that box is in view).
const EMITTER_SEEN: Aabb = Aabb {
    position: Vector3::new(-3.0, -2.0, -3.0),
    size: Vector3::new(6.0, 5.0, 6.0),
};

fn new_emitter(
    root: &mut Gd<Node3D>,
    recipe: Recipe,
    tex: &Gd<Texture2D>,
    fire: Option<&Gd<Shader>>,
) -> Emitter {
    let p = recipe.params();
    let mut m = process_material(recipe, &p);
    let mean = (p.size.0 + p.size.1) / 2.0;
    let mut node = GpuParticles3D::new_alloc();
    node.set_process_material(&m);
    node.set_amount(p.amount);
    node.set_lifetime(p.life);
    node.set_one_shot(true);
    node.set_explosiveness_ratio(recipe.explosiveness());
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
    } else if let Some(shader) = fire {
        node.set_draw_pass_mesh(0, &quad(mean));
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(shader);
        mat.set_shader_parameter("book", &tex.to_variant());
        let (h, v) = recipe.frames().unwrap_or((1, 1));
        mat.set_shader_parameter("frames", &Vector2::new(h as f32, v as f32).to_variant());
        node.set_material_override(&mat);
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
    if recipe == Recipe::Soot {
        // drawn over the flames it rolls off
        node.set_sorting_offset(SOOT_OVER);
    }
    node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    node.set_visibility_aabb(EMITTER_SEEN);
    root.add_child(&node);
    let quad = node.get_draw_pass_mesh(0).expect("a draw pass was set");
    Emitter {
        node,
        process: m,
        busy: 0.0,
        quad,
        sized: 1.0,
    }
}

/// Motes in the air around the camera's focus: lit like everything else,
/// so they show only in the light of a torch or the hero's.
fn dust(root: &mut Gd<Node3D>, soft: &Gd<Texture2D>) -> Gd<GpuParticles3D> {
    let mut m = ParticleProcessMaterial::new_gd();
    m.set_emission_shape(EmissionShape::BOX);
    // (from knee height up, cool and drifting: low and warm, still on the
    // floor, they read as the glints of things lying there)
    m.set_emission_box_extents(Vector3::new(7.0, DUST_HIGH.1, 4.5));
    m.set_direction(Vector3::UP);
    m.set_spread(180.0);
    m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.02);
    m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.08);
    m.set_gravity(Vector3::ZERO);
    m.set_param_min(Parameter::SCALE, 0.6);
    m.set_param_max(Parameter::SCALE, 1.25);
    m.set_turbulence_enabled(true);
    m.set_turbulence_noise_strength(0.35);
    m.set_turbulence_noise_scale(4.0);
    let mut g = Gradient::new_gd();
    let c = Color::from_rgba(0.8, 0.83, 0.88, 0.3);
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

    /// A murky potion's glow on the drinker was a few dim motes: what
    /// rises round a body is as bright as its hue goes, a grey one white.
    #[test]
    fn a_glow_on_the_body_is_bright_in_its_own_hue() {
        let murky = vivid(Color::from_rgb(0.2, 0.1, 0.05));
        assert!((murky.r - 1.0).abs() < 1e-5);
        assert!((murky.g - 0.5).abs() < 1e-5 && (murky.b - 0.25).abs() < 1e-5);
        let grey = vivid(Color::from_rgb(0.45, 0.45, 0.45));
        assert!(grey.r == 1.0 && grey.g == 1.0 && grey.b == 1.0);
        // each body's effect rises round it, with a light at its chest
        for kind in [VfxKind::Aura(murky), VfxKind::Halo, VfxKind::Heal] {
            let (recipe, _, light) = look_of(kind);
            assert!(recipe.ring().is_some(), "{kind:?}");
            assert!(
                light.is_some_and(|(_, energy, _)| energy >= 2.0),
                "{kind:?}"
            );
        }
    }
}
