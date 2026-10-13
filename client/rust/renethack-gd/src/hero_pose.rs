//! What the hero's body shows of their state (qa C3). Caught in a trap, the
//! hero's glyph hides the trap under them: the trap the host's hero notice
//! names is drawn here at their feet, and the hero stands sunk into a pit,
//! held by a bear trap's shut jaws, wrapped in a web, sinking into lava, or
//! under water (the status condition Submrg), each with a pose. Out of a
//! pit the hero climbs. A prayer (qa C4) is shown kneeling, with what the
//! god answered as the messages tell it: a shaft of light on a pleased
//! god, darkness closing in on an angry one. A spell of helplessness (qa
//! C16), seen once it passed, is shown after it: asleep or knocked out the
//! hero crumples to the floor and lies there (a sleeper's letters rising),
//! then gets up; paralysed they slump where they stand, still. A child of
//! map_view: it moves the hero's own model (the rider's while they ride,
//! who keep their seat: no sinking, kneeling or lying in the saddle).

use godot::classes::base_material_3d::{
    BillboardMode, BlendMode, Flags, ShadingMode, Transparency,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::particle_process_material::{EmissionShape, Parameter};
use godot::classes::rendering_server::FogVolumeShape;
use godot::classes::{
    Decal, FogMaterial, FogVolume, GpuParticles3D, Gradient, GradientTexture1D, Label3D,
    MeshInstance3D, OmniLight3D, ParticleProcessMaterial, QuadMesh, Shader, ShaderMaterial,
    StandardMaterial3D,
};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::{Helpless, HelplessSpan, PrayerOutcome, TrapKind, World};

use super::{
    Finish, MapView, MeshKey, Paint, Role, SHADE_LIT, at, bevel, cuboid, cylinder, marker, plane,
    prism, torus,
};

/// What the hero is caught in or under, as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    Trap(TrapKind),
    /// Under water (the status condition Submrg).
    Water,
}

impl Held {
    /// How far below the floor the hero stands (metres).
    fn sink(self) -> f32 {
        match self {
            Held::Trap(t) if t.is_pit() => PIT_SINK,
            Held::Trap(TrapKind::Lava) => 0.45,
            Held::Trap(TrapKind::Floor) => 0.25,
            Held::Water => 0.95,
            Held::Trap(_) => 0.0,
        }
    }

    /// The clip the hero holds while caught (None: their idle; sunk into
    /// lava or water they stand, a swimmer's clip lies flat).
    fn clip(self) -> Option<&'static str> {
        match self {
            Held::Trap(TrapKind::BearTrap) => Some(CROUCH),
            Held::Trap(TrapKind::Web) => Some(STRUGGLE),
            _ => None,
        }
    }
}

/// A pit swallows the hero to the waist.
const PIT_SINK: f32 = 0.85;
/// Metres a second the hero sinks into a trap or rises out of it.
const SINK_SPEED: f32 = 1.4;
const CROUCH: &str = "Crouch_Idle";
const STRUGGLE: &str = "Push";
const CLIMB: &str = "ual2/ClimbUp_1m";
const KNEEL: &str = "Fixing_Kneeling";
/// Seconds a prayer is shown: the three turns it takes, kneeling.
const PRAYER_SECS: f32 = 3.2;
/// Seconds the god's answer takes to come and to go.
const ANSWER_IN: f32 = 0.6;
const ANSWER_OUT: f32 = 0.9;
const HOLY: Color = Color::from_rgb(1.0, 0.93, 0.72);
/// The shaft's height (its middle at the hero's chest height).
const SHAFT_HEIGHT: f32 = 7.0;
/// Seconds the motes in the shaft keep rising from the hero's feet, and
/// each one's life (all gone before the prayer ends).
const MOTES_SECS: (f32, f32) = (0.3, 1.6);
const MOTE_LIFE: f32 = 1.5;
const DARK: Color = Color::from_rgb(0.05, 0.02, 0.08);
/// The body gives way (a death's fall, quicker).
const COLLAPSE: &str = "Death01";
const COLLAPSE_SPEED: f32 = 1.4;
const GET_UP: &str = "ual2/LayToIdle";
/// Paralysed: slumped where they stand, the stance held still once taken.
const SLUMP: &str = "ual2/Zombie_Idle";
const SLUMP_SECS: f32 = 0.5;
/// A body on the floor slides this much of its height the way it faced:
/// the fall is backwards, and it would lie half on the next cell.
const LIE_SHIFT: f32 = 0.3;
/// Seconds a helpless hero stays down for each turn they were, at least
/// and at most.
const DOWN_PER_TURN: f32 = 0.25;
const DOWN_SECS: (f32, f32) = (0.8, 3.0);
/// A sleeper's letters: how many rise at a time, the seconds each takes.
const SNORES: usize = 3;
const SNORE_SECS: f32 = 1.8;
const SNORE: Color = Color::from_rgb(0.84, 0.9, 1.0);
const SILK: Color = Color::from_rgba(0.88, 0.88, 0.84, 0.55);
const MOLTEN: Color = Color::from_rgb(1.0, 0.42, 0.08);
const RIPPLE: Color = Color::from_rgba(0.62, 0.78, 0.86, 0.45);

/// The hero model's inner node (its own transform) and the height it had
/// before the map lowered it.
struct Lowered {
    inner: Gd<Node3D>,
    base: f32,
}

/// The state drawn now.
pub(super) struct HeroPose {
    root: Gd<Node3D>,
    drawn: Option<(Held, (i32, i32))>,
    parts: Vec<Gd<Node3D>>,
    sink: f32,
    lowered: Option<Lowered>,
    /// The pose clip played on the hero now.
    clip: Option<&'static str>,
    /// Seconds left of a climb out of a pit.
    climbing: f32,
    prayer: Option<Prayer>,
    lying: Option<Lying>,
    /// The last spell of helplessness shown (its serial).
    shown_spell: u64,
    /// How far above the floor the hero floats (hero_look's: levitating,
    /// flying), lifted as a pit lowers them.
    pub(super) lift: f32,
}

/// A spell of helplessness being shown.
struct Lying {
    kind: Helpless,
    phase: HelplessPhase,
    /// Seconds into the phase, and how long it lasts.
    t: f32,
    len: f32,
    /// Seconds to stay down.
    hold: f32,
    /// A sleeper's letters.
    snores: Vec<Gd<Label3D>>,
    shift: Option<Shift>,
}

/// The hero's model slid along the floor while they lie there.
struct Shift {
    /// The model's inner node, its own place, and the way to slide it (in
    /// the space of the node it sits in).
    inner: Gd<Node3D>,
    base: Vector3,
    by: Vector3,
}

impl Shift {
    /// Slid `f` of the way (0 back in place).
    fn set(&mut self, f: f32) {
        if !self.inner.is_instance_valid() {
            return;
        }
        // across the floor only: the height is the pit's (`lower_hero`)
        let mut p = self.inner.get_position();
        p.x = self.base.x + self.by.x * f;
        p.z = self.base.z + self.by.z * f;
        self.inner.set_position(p);
    }
}

/// Where a spell of helplessness shown is: going down, down, getting up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelplessPhase {
    Down,
    Held,
    Up,
}

/// A prayer being shown.
struct Prayer {
    outcome: PrayerOutcome,
    /// Seconds since it began.
    t: f32,
    nodes: Vec<Gd<Node3D>>,
    /// The shaft's own materials (their strength grows and fades), with
    /// their full strength.
    veils: Vec<(Gd<ShaderMaterial>, f32)>,
    /// The warm glow on the floor round the hero, its full alpha.
    glow: Option<(Gd<Decal>, f32)>,
    /// Motes rising up the shaft.
    motes: Option<Gd<GpuParticles3D>>,
    /// Its light (a negative one darkens), with its full energy.
    light: Option<(Gd<OmniLight3D>, f32)>,
    /// The darkness's own fog, with its full density.
    fog: Option<(Gd<FogMaterial>, f32)>,
}

/// How strongly the god's answer shows `t` seconds into the prayer.
fn answer_strength(t: f32) -> f32 {
    let rise = (t / ANSWER_IN).clamp(0.0, 1.0);
    let fall = ((PRAYER_SECS - t) / ANSWER_OUT).clamp(0.0, 1.0);
    rise.min(fall)
}

impl HeroPose {
    pub(super) fn new(parent: &mut Gd<Node3D>) -> HeroPose {
        let mut root = Node3D::new_alloc();
        root.set_name("HeroPose");
        parent.add_child(&root);
        HeroPose {
            root,
            drawn: None,
            parts: Vec::new(),
            sink: 0.0,
            lowered: None,
            clip: None,
            climbing: 0.0,
            prayer: None,
            lying: None,
            shown_spell: 0,
            lift: 0.0,
        }
    }

    /// A new game: what was shown of the last one's hero goes, and their
    /// model is put back as it was (the map's cells give their models back
    /// to be used again).
    pub(super) fn forget(&mut self) {
        self.clear_parts();
        self.drawn = None;
        self.sink = 0.0;
        if let Some(mut l) = self.lowered.take()
            && l.inner.is_instance_valid()
        {
            let mut p = l.inner.get_position();
            p.y = l.base;
            l.inner.set_position(p);
        }
        self.clip = None;
        self.climbing = 0.0;
        let mut nodes = self.prayer.take().map(|p| p.nodes).unwrap_or_default();
        if let Some(mut l) = self.lying.take() {
            if let Some(shift) = l.shift.as_mut() {
                shift.set(0.0);
            }
            nodes.extend(l.snores.into_iter().map(Gd::upcast));
        }
        for mut n in nodes {
            if n.is_instance_valid() {
                n.queue_free();
            }
        }
        self.shown_spell = 0;
        self.lift = 0.0;
    }

    fn clear_parts(&mut self) {
        for mut n in self.parts.drain(..) {
            if n.is_instance_valid() {
                n.queue_free();
            }
        }
    }
}

/// What holds the hero now, as the world tells it.
pub fn held(world: &World, catalog: &Catalog) -> Option<Held> {
    world.hero_trap.map(Held::Trap).or_else(|| {
        world
            .status
            .condition_names(catalog)
            .iter()
            .any(|c| c == "Submrg")
            .then_some(Held::Water)
    })
}

impl MapView {
    /// Every frame with the hero on the map: draw what holds them and
    /// pose their model.
    pub(super) fn pose_hero(&mut self, world: &World, catalog: &Catalog, delta: f32) {
        let now = held(world, catalog);
        let at = self.hero_at;
        let was = self.hero_pose.drawn;
        if was.map(|(h, _)| h) != now || was.map(|(_, c)| c) != at.filter(|_| now.is_some()) {
            // out of a pit where they stood: they climb out
            if let (Some((Held::Trap(t), c)), None) = (was, now)
                && t.is_pit()
                && Some(c) == at
            {
                self.climb_out();
            }
            self.hero_pose.clear_parts();
            self.hero_pose.drawn = now.zip(at);
            if let (Some(h), Some(c)) = (now, at) {
                self.draw_held(h, c);
            }
        }
        self.advance_prayer(delta);
        self.pose_helpless(world.helpless(), delta);
        // in the saddle the hero keeps their seat
        let target = if self.rider.is_some() {
            0.0
        } else {
            now.map_or(0.0, Held::sink)
        };
        let pose = &mut self.hero_pose;
        pose.climbing = (pose.climbing - delta).max(0.0);
        let step = SINK_SPEED * delta;
        pose.sink = if pose.sink < target {
            (pose.sink + step).min(target)
        } else {
            (pose.sink - step).max(target)
        };
        self.lower_hero();
        self.hold_clip(now.and_then(Held::clip));
    }

    /// The trap (or water) at the hero's feet.
    fn draw_held(&mut self, held: Held, (x, y): (i32, i32)) {
        let ground = self.ground(x, y);
        let origin = Vector3::new(x as f32, ground, y as f32);
        match held {
            Held::Trap(t) if t.is_pit() => {
                let hole = Paint::Flat(Color::from_rgb(0.006, 0.006, 0.008), Finish::Matte);
                self.part(plane(0.78, 0.78), hole, origin + at(0.0, 0.004, 0.0), 0.0);
                let stone = self.named("bedrock", 100, Color::from_rgb(0.3, 0.29, 0.27));
                for (px, pz, w, d) in [
                    (0.0, -0.4, 0.9, 0.1),
                    (0.0, 0.4, 0.9, 0.1),
                    (-0.4, 0.0, 0.1, 0.7),
                    (0.4, 0.0, 0.1, 0.7),
                ] {
                    self.part(bevel(w, 0.05, d), stone, origin + at(px, 0.025, pz), 0.0);
                }
            }
            Held::Trap(TrapKind::BearTrap) => {
                // the jaws shut round the ankles, teeth in, a chain to a peg
                let iron = self.named("iron", SHADE_LIT, Color::from_rgb(0.35, 0.35, 0.37));
                self.part(torus(0.13, 0.17), iron, origin + at(0.0, 0.05, 0.0), 0.0);
                for i in 0..8 {
                    let a = (i as f32 * 45.0).to_radians();
                    let p = at(0.15 * a.sin(), 0.09, 0.15 * a.cos());
                    self.part(prism(0.035, 0.07, 0.025), iron, origin + p, i as f32 * 45.0);
                }
                self.part(
                    cuboid(0.34, 0.015, 0.02),
                    iron,
                    origin + at(0.3, 0.008, 0.18),
                    35.0,
                );
                self.part(
                    cylinder(0.02, 0.02, 0.06),
                    iron,
                    origin + at(0.44, 0.03, 0.28),
                    0.0,
                );
            }
            Held::Trap(TrapKind::Web) => {
                let silk = Paint::Flat(SILK, Finish::Ghost);
                for i in 0..6 {
                    self.part(
                        cuboid(0.9, 0.006, 0.008),
                        silk,
                        origin + at(0.0, 0.02, 0.0),
                        30.0 * i as f32,
                    );
                }
                // wound round the body every which way, and guy lines
                // from it down to the floor around
                for (i, (r, h)) in [
                    (0.26, 0.35),
                    (0.24, 0.6),
                    (0.22, 0.85),
                    (0.21, 1.1),
                    (0.2, 1.3),
                ]
                .into_iter()
                .enumerate()
                {
                    let tilt = at(if i % 2 == 0 { 24.0 } else { -18.0 }, 70.0 * i as f32, 12.0);
                    self.part_turned(torus(r, r + 0.01), silk, origin + at(0.0, h, 0.0), tilt);
                }
                for i in 0..7 {
                    let a = (i as f32 * 51.4 + 10.0).to_radians();
                    let h = 0.45 + 0.13 * (i % 5) as f32;
                    let from = origin + at(0.18 * a.sin(), h, 0.18 * a.cos());
                    let to = origin + at(0.55 * (a + 0.3).sin(), 0.02, 0.55 * (a + 0.3).cos());
                    self.strand(from, to, silk);
                }
            }
            Held::Trap(TrapKind::Lava) => {
                let crust = Paint::Flat(MOLTEN, Finish::Glow);
                self.part(torus(0.2, 0.34), crust, origin + at(0.0, 0.0, 0.0), 0.0);
                let mut light = OmniLight3D::new_alloc();
                light.set_color(MOLTEN);
                light.set_param(Param::ENERGY, 1.6);
                light.set_param(Param::RANGE, 2.4);
                light.set_shadow(false);
                light.set_position(origin + at(0.0, 0.35, 0.0));
                self.hero_pose.root.add_child(&light);
                self.hero_pose.parts.push(light.upcast());
            }
            Held::Water => {
                let ripple = Paint::Flat(RIPPLE, Finish::Ghost);
                for r in [0.24, 0.4] {
                    self.part(torus(r, r + 0.02), ripple, origin + at(0.0, 0.01, 0.0), 0.0);
                }
            }
            // stuck in the floor, chained to a ball: the hero alone
            Held::Trap(_) => {}
        }
    }

    /// A manifest material by name as the branch has it, else a colour.
    fn named(&self, name: &str, shade: u8, fallback: Color) -> Paint {
        match self
            .art
            .manifest()
            .material(self.branch_look.material(name))
        {
            Some(m) => Paint::Pbr(m, shade, Role::Prop),
            None => Paint::Flat(fallback, Finish::Matte),
        }
    }

    /// A mesh of the drawn state, turned `yaw` degrees about y.
    fn part(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3, yaw: f32) {
        self.part_turned(mesh, paint, pos, at(0.0, yaw, 0.0));
    }

    /// A mesh of the drawn state, turned by `degrees` (x, y, z).
    fn part_turned(&mut self, mesh: MeshKey, paint: Paint, pos: Vector3, degrees: Vector3) {
        let r = Vector3::new(
            degrees.x.to_radians(),
            degrees.y.to_radians(),
            degrees.z.to_radians(),
        );
        let basis = Basis::from_euler(EulerOrder::YXZ, r);
        self.part_at(mesh, paint, Transform3D::new(basis, pos));
    }

    fn part_at(&mut self, mesh: MeshKey, paint: Paint, xf: Transform3D) {
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&self.art.mesh(mesh));
        mi.set_material_override(&self.material(paint));
        mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        mi.set_transform(xf);
        self.hero_pose.root.add_child(&mi);
        self.hero_pose.parts.push(mi.upcast());
    }

    /// A thread of silk from one point to another.
    fn strand(&mut self, from: Vector3, to: Vector3, paint: Paint) {
        let d = to - from;
        let len = d.length();
        let up = d / len.max(1e-4);
        let side = up.cross(Vector3::new(0.0, 0.0, 1.0));
        let side = if side.length() < 1e-3 {
            Vector3::new(1.0, 0.0, 0.0)
        } else {
            side.normalized()
        };
        let basis = Basis::from_cols(side, up, side.cross(up));
        self.part_at(
            cylinder(0.005, 0.005, len),
            paint,
            Transform3D::new(basis, from + d * 0.5),
        );
    }

    /// The hero's model at its height less the sink (its inner node: the
    /// map places the outer one).
    fn lower_hero(&mut self) {
        let inner = self
            .hero_model()
            .and_then(|m| m.node.get_child(0))
            .and_then(|n| n.try_cast::<Node3D>().ok());
        // a floating hero is lifted as a sunk one is lowered
        let shift = self.hero_pose.lift - self.hero_pose.sink;
        let pose = &mut self.hero_pose;
        // another model (a new level, a look changed): the old one is put
        // back at its height
        if let Some(l) = pose.lowered.as_mut()
            && inner.as_ref().map(|i| i.instance_id()) != Some(l.inner.instance_id())
        {
            if l.inner.is_instance_valid() {
                let mut p = l.inner.get_position();
                p.y = l.base;
                l.inner.set_position(p);
            }
            pose.lowered = None;
        }
        let Some(mut inner) = inner else {
            return;
        };
        if pose.lowered.is_none() {
            if shift == 0.0 {
                return;
            }
            pose.lowered = Some(Lowered {
                base: inner.get_position().y,
                inner: inner.clone(),
            });
        }
        let Some(l) = pose.lowered.as_mut() else {
            return;
        };
        let mut p = inner.get_position();
        p.y = l.base + shift;
        inner.set_position(p);
        if shift == 0.0 {
            pose.lowered = None;
        }
    }

    /// Hold `clip` on the hero while no motion of theirs plays; back to
    /// their idle when it ends.
    fn hold_clip(&mut self, clip: Option<&'static str>) {
        let pose = &self.hero_pose;
        if pose.climbing > 0.0
            || pose.prayer.is_some()
            || pose.lying.is_some()
            || self.rider.is_some()
        {
            return;
        }
        let busy = self.hero_busy();
        let Some(m) = self.hero_model() else {
            return;
        };
        let Some(mut player) = m.player().cloned() else {
            return;
        };
        let was = self.hero_pose.clip;
        match clip.filter(|c| player.has_animation(*c)) {
            Some(c) => {
                if !busy && player.get_current_animation() != c {
                    player.play_ex().name(c).custom_blend(0.3).done();
                }
                self.hero_pose.clip = Some(c);
            }
            None => {
                self.hero_pose.clip = None;
                if was.is_some() {
                    self.idle_hero();
                }
            }
        }
    }

    /// Out of a pit: the climb, then their idle.
    fn climb_out(&mut self) {
        let Some(m) = self.hero_model() else {
            return;
        };
        let idle = self.art.clips(m, false).idle;
        let Some(mut player) = m.player().cloned().filter(|p| p.has_animation(CLIMB)) else {
            return;
        };
        player.play_ex().name(CLIMB).custom_blend(0.15).done();
        if let Some(idle) = idle {
            player.queue(idle.as_str());
        }
        let secs = player.get_animation(CLIMB).map_or(1.0, |a| a.get_length());
        self.hero_pose.climbing = secs;
        self.hero_pose.clip = None;
    }

    /// The hero's own model back to its idle (the rider's, not the
    /// steed's).
    fn idle_hero(&mut self) {
        let Some(m) = self.hero_model() else {
            return;
        };
        let mut clips = self.art.clips(m, false);
        if let (Some(p), Some(idle)) = (clips.player.as_mut(), clips.idle.as_ref())
            && p.is_instance_valid()
            && p.get_current_animation() != idle.as_str()
        {
            p.play_ex().name(idle.as_str()).custom_blend(0.25).done();
        }
    }

    /// A prayer the hero said: kneeling, and the god's answer (`outcome`,
    /// as the messages tell it) over them.
    pub(super) fn show_prayer(&mut self, outcome: PrayerOutcome) {
        self.end_prayer(false);
        let Some((x, y)) = self.hero_at else {
            return;
        };
        if self.rider.is_none()
            && let Some(mut p) = self.hero_model().and_then(|m| m.player().cloned())
            && p.has_animation(KNEEL)
        {
            p.play_ex().name(KNEEL).custom_blend(0.3).done();
        }
        let origin = Vector3::new(x as f32, self.ground(x, y), y as f32);
        let mut prayer = Prayer {
            outcome,
            t: 0.0,
            nodes: Vec::new(),
            veils: Vec::new(),
            glow: None,
            motes: None,
            light: None,
            fog: None,
        };
        match outcome {
            PrayerOutcome::Pleased => {
                // a shaft of light from above, soft and glowing, a brighter
                // core in it; a warm glow on the floor; motes rising
                let middle = origin + at(0.0, SHAFT_HEIGHT / 2.0, 0.0);
                for (top, bottom, weight) in [(0.34, 0.46, 0.5), (0.13, 0.18, 0.3)] {
                    self.shaft(
                        &mut prayer,
                        cylinder(top, bottom, SHAFT_HEIGHT),
                        middle,
                        weight,
                    );
                }
                let mut glow = marker(&mut self.hero_pose.root, &self.soft_tex, 1.5);
                glow.set_position(origin);
                glow.set_modulate(HOLY.with_alpha(0.0));
                glow.set_emission_energy(1.2);
                glow.set_visible(true);
                prayer.nodes.push(glow.clone().upcast());
                prayer.glow = Some((glow, 0.4));
                self.shaft_motes(&mut prayer, origin);
                self.prayer_light(&mut prayer, origin + at(0.0, 2.4, 0.0), 3.0, 4.5);
            }
            PrayerOutcome::Angered => {
                // darkness closing in round the hero: a black fog, and a
                // light that takes light away
                self.darkness(&mut prayer, origin + at(0.0, 0.9, 0.0));
                self.prayer_light(&mut prayer, origin + at(0.0, 1.2, 0.0), 4.0, 3.5);
            }
            PrayerOutcome::Unanswered => {}
        }
        self.hero_pose.prayer = Some(prayer);
    }

    /// Part of a shaft of light, glowing over what is behind it, soft at
    /// its sides and fading upwards (shaders/holy_light.gdshader; without
    /// it, no shaft: the light, the glow and the motes still show).
    fn shaft(&mut self, prayer: &mut Prayer, mesh: MeshKey, pos: Vector3, weight: f32) {
        let Ok(shader) = godot::tools::try_load::<Shader>("res://shaders/holy_light.gdshader")
        else {
            return;
        };
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        mat.set_shader_parameter("color", &HOLY.to_variant());
        mat.set_shader_parameter("height", &SHAFT_HEIGHT.to_variant());
        mat.set_shader_parameter("strength", &0.0f32.to_variant());
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&self.art.mesh(mesh));
        mi.set_material_override(&mat);
        mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        mi.set_position(pos);
        self.hero_pose.root.add_child(&mi);
        prayer.nodes.push(mi.upcast());
        prayer.veils.push((mat, weight));
    }

    /// Motes of light rising round the hero's feet up the shaft.
    fn shaft_motes(&mut self, prayer: &mut Prayer, at_feet: Vector3) {
        let mut process = ParticleProcessMaterial::new_gd();
        process.set_emission_shape(EmissionShape::SPHERE);
        process.set_emission_sphere_radius(0.3);
        process.set_direction(Vector3::UP);
        process.set_spread(10.0);
        process.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.35);
        process.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.8);
        process.set_gravity(Vector3::new(0.0, 0.1, 0.0));
        process.set_param_min(Parameter::SCALE, 0.6);
        process.set_param_max(Parameter::SCALE, 1.4);
        process.set_turbulence_enabled(true);
        process.set_turbulence_noise_strength(0.15);
        process.set_turbulence_noise_scale(2.0);
        // bright as they rise from the floor, gone at the top
        let mut ramp = Gradient::new_gd();
        let bright = Color::from_rgba(HOLY.r * 2.2, HOLY.g * 2.2, HOLY.b * 2.2, 0.0);
        ramp.set_color(0, bright);
        ramp.set_color(1, HOLY.with_alpha(0.0));
        ramp.add_point(0.2, bright.with_alpha(1.0));
        let mut ramp_tex = GradientTexture1D::new_gd();
        ramp_tex.set_gradient(&ramp);
        process.set_color_ramp(&ramp_tex);
        let mut look = StandardMaterial3D::new_gd();
        look.set_billboard_mode(BillboardMode::PARTICLES);
        look.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        look.set_texture(
            godot::classes::base_material_3d::TextureParam::ALBEDO,
            &self.soft_tex,
        );
        look.set_transparency(Transparency::ALPHA);
        look.set_blend_mode(BlendMode::ADD);
        look.set_shading_mode(ShadingMode::UNSHADED);
        look.set_flag(Flags::DISABLE_FOG, true);
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(0.06, 0.06));
        let mut motes = GpuParticles3D::new_alloc();
        motes.set_process_material(&process);
        motes.set_amount(20);
        motes.set_lifetime(f64::from(MOTE_LIFE));
        motes.set_randomness_ratio(0.4);
        motes.set_draw_pass_mesh(0, &quad);
        motes.set_material_override(&look);
        motes.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        motes.set_visibility_aabb(Aabb::new(
            Vector3::new(-1.0, -0.2, -1.0),
            Vector3::new(2.0, 3.0, 2.0),
        ));
        motes.set_position(at_feet + at(0.0, 0.1, 0.0));
        motes.set_emitting(false);
        self.hero_pose.root.add_child(&motes);
        prayer.nodes.push(motes.clone().upcast());
        prayer.motes = Some(motes);
    }

    /// A black fog round the hero.
    fn darkness(&mut self, prayer: &mut Prayer, pos: Vector3) {
        let mut mat = FogMaterial::new_gd();
        mat.set_density(0.0);
        mat.set_albedo(DARK);
        mat.set_edge_fade(0.8);
        let mut fog = FogVolume::new_alloc();
        fog.set_shape(FogVolumeShape::ELLIPSOID);
        fog.set_size(Vector3::new(2.6, 2.4, 2.6));
        fog.set_position(pos);
        fog.set_material(&mat);
        self.hero_pose.root.add_child(&fog);
        prayer.nodes.push(fog.upcast());
        prayer.fog = Some((mat, 2.6));
    }

    /// The answer's light: holy, or on an angry god's a negative one that
    /// takes light away.
    fn prayer_light(&mut self, prayer: &mut Prayer, pos: Vector3, energy: f32, range: f32) {
        let angry = prayer.outcome == PrayerOutcome::Angered;
        let mut light = OmniLight3D::new_alloc();
        light.set_color(if angry { DARK } else { HOLY });
        light.set_negative(angry);
        light.set_param(Param::ENERGY, 0.0);
        light.set_param(Param::RANGE, range);
        light.set_shadow(false);
        light.set_position(pos);
        self.hero_pose.root.add_child(&light);
        prayer.nodes.push(light.clone().upcast());
        prayer.light = Some((light, energy));
    }

    /// A prayer shown goes on, and ends: after its time, or when the hero
    /// moves on.
    fn advance_prayer(&mut self, delta: f32) {
        let busy = self.hero_busy();
        let Some(prayer) = self.hero_pose.prayer.as_mut() else {
            return;
        };
        prayer.t += delta;
        if busy || prayer.t >= PRAYER_SECS {
            self.end_prayer(!busy);
            return;
        }
        let a = answer_strength(prayer.t);
        for (mat, weight) in &mut prayer.veils {
            mat.set_shader_parameter("strength", &(a * *weight).to_variant());
        }
        if let Some((glow, alpha)) = prayer.glow.as_mut() {
            glow.set_modulate(HOLY.with_alpha(a * *alpha));
        }
        if let Some(motes) = prayer.motes.as_mut() {
            let rising = (MOTES_SECS.0..MOTES_SECS.1).contains(&prayer.t);
            if motes.is_emitting() != rising {
                motes.set_emitting(rising);
            }
        }
        if let Some((light, energy)) = prayer.light.as_mut() {
            light.set_param(Param::ENERGY, a * *energy);
        }
        if let Some((fog, density)) = prayer.fog.as_mut() {
            fog.set_density(a * *density);
        }
    }

    /// The prayer shown is over: its light gone, the hero standing up
    /// (`stand`) or already about something else.
    fn end_prayer(&mut self, stand: bool) {
        let Some(prayer) = self.hero_pose.prayer.take() else {
            return;
        };
        for mut n in prayer.nodes {
            if n.is_instance_valid() {
                n.queue_free();
            }
        }
        if stand && self.rider.is_none() {
            self.idle_hero();
        }
    }

    /// The hero's model in a motion of their own (a step, a blow).
    fn hero_busy(&self) -> bool {
        self.motions
            .iter()
            .any(|m| m.hero && Some(m.cell) == self.hero_at)
    }

    /// The spell of helplessness the hero went through before this command
    /// prompt, shown once they are free to show it (the use that brought
    /// it shown first): down, held there a while for its turns, up again.
    /// The hero moving on ends it.
    fn pose_helpless(&mut self, span: Option<HelplessSpan>, delta: f32) {
        let busy = self.hero_busy();
        let inner = self
            .hero_model()
            .and_then(|m| m.node.get_child(0))
            .map(|n| n.instance_id());
        let Some(lying) = self.hero_pose.lying.as_mut() else {
            let new = span.filter(|s| s.serial > self.hero_pose.shown_spell);
            if let Some(s) = new
                && !busy
                && self.hero_pose.prayer.is_none()
            {
                self.hero_pose.shown_spell = s.serial;
                if self.rider.is_none() {
                    self.lie_down(s);
                }
            }
            return;
        };
        // the hero about something else, or another model of them (a look
        // changed): the spell's pose is over
        let other = lying
            .shift
            .as_ref()
            .is_some_and(|s| Some(s.inner.instance_id()) != inner);
        if busy || other {
            self.end_lying(false);
            return;
        }
        lying.t += delta;
        let slid = match lying.phase {
            HelplessPhase::Down => (lying.t / lying.len).min(1.0),
            HelplessPhase::Held => 1.0,
            HelplessPhase::Up => 1.0 - (lying.t / lying.len).min(1.0),
        };
        if let Some(shift) = lying.shift.as_mut() {
            shift.set(slid);
        }
        match lying.phase {
            HelplessPhase::Down if lying.t >= lying.len => {
                lying.phase = HelplessPhase::Held;
                lying.t = 0.0;
                // the slump held still; the fall ends on the floor
                if let Some(mut p) = self.hero_model().and_then(|m| m.player().cloned()) {
                    p.pause();
                }
            }
            HelplessPhase::Held if lying.t >= lying.hold => self.get_up(),
            HelplessPhase::Up if lying.t >= lying.len => self.end_lying(true),
            _ => {}
        }
        self.snore();
    }

    /// The hero goes down: crumpled to the floor, or slumped.
    fn lie_down(&mut self, s: HelplessSpan) {
        let Some(mut p) = self.hero_model().and_then(|m| m.player().cloned()) else {
            return;
        };
        let (clip, speed) = match s.kind {
            Helpless::Paralyzed => (SLUMP, 1.0),
            Helpless::Asleep | Helpless::Unconscious => (COLLAPSE, COLLAPSE_SPEED),
        };
        let Some(anim) = p.get_animation(clip) else {
            return;
        };
        p.play_ex()
            .name(clip)
            .custom_blend(0.25)
            .custom_speed(speed)
            .done();
        let len = match s.kind {
            Helpless::Paralyzed => SLUMP_SECS,
            _ => anim.get_length() / speed,
        };
        let snores = if s.kind == Helpless::Asleep {
            (0..SNORES).map(|_| self.snore_letter()).collect()
        } else {
            Vec::new()
        };
        let shift = match s.kind {
            Helpless::Paralyzed => None,
            Helpless::Asleep | Helpless::Unconscious => self.lying_shift(),
        };
        self.hero_pose.lying = Some(Lying {
            kind: s.kind,
            phase: HelplessPhase::Down,
            t: 0.0,
            len,
            hold: (s.turns as f32 * DOWN_PER_TURN).clamp(DOWN_SECS.0, DOWN_SECS.1),
            snores,
            shift,
        });
    }

    /// How the hero's model slides while they lie: the way they face, by
    /// a share of their height.
    fn lying_shift(&self) -> Option<Shift> {
        let (_, tall) = self.hero_stands()?;
        let m = self.hero_model()?;
        let inner = m.node.get_child(0)?.try_cast::<Node3D>().ok()?;
        let basis = m.node.get_global_transform().basis;
        let facing = basis.col_c();
        let flat = Vector3::new(facing.x, 0.0, facing.z);
        if flat.length() < 1e-4 {
            return None;
        }
        let by = basis.inverse() * (flat.normalized() * (tall * LIE_SHIFT));
        Some(Shift {
            base: inner.get_position(),
            inner,
            by,
        })
    }

    fn snore_letter(&mut self) -> Gd<Label3D> {
        let mut l = self.new_label();
        l.set_text("Z");
        l.set_pixel_size(0.0018);
        l.set_modulate(SNORE.with_alpha(0.0));
        l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        l.set_visible(false);
        self.hero_pose.root.add_child(&l);
        l
    }

    /// The hero gets up: off the floor, or out of the slump.
    fn get_up(&mut self) {
        let Some(lying) = self.hero_pose.lying.as_mut() else {
            return;
        };
        lying.phase = HelplessPhase::Up;
        lying.t = 0.0;
        lying.len = 0.4;
        for mut l in lying.snores.drain(..) {
            if l.is_instance_valid() {
                l.queue_free();
            }
        }
        let floor = lying.kind != Helpless::Paralyzed;
        let player = self.hero_model().and_then(|m| m.player().cloned());
        match player {
            Some(mut p) if floor && p.has_animation(GET_UP) => {
                p.play_ex().name(GET_UP).custom_blend(0.3).done();
                let len = p.get_animation(GET_UP).map_or(1.5, |a| a.get_length());
                if let Some(lying) = self.hero_pose.lying.as_mut() {
                    lying.len = len;
                }
            }
            _ => self.idle_hero(),
        }
    }

    /// The spell shown is over: the hero standing (`stand`) or already
    /// about something else.
    fn end_lying(&mut self, stand: bool) {
        let Some(mut lying) = self.hero_pose.lying.take() else {
            return;
        };
        if let Some(shift) = lying.shift.as_mut() {
            shift.set(0.0);
        }
        for mut l in lying.snores {
            if l.is_instance_valid() {
                l.queue_free();
            }
        }
        if stand {
            self.idle_hero();
        }
    }

    /// A sleeper's letters rise from their head, growing and fading, one
    /// after another.
    fn snore(&mut self) {
        let held = self
            .hero_pose
            .lying
            .as_ref()
            .is_some_and(|l| l.phase == HelplessPhase::Held && !l.snores.is_empty());
        if !held {
            return;
        }
        let head = self.hero_head();
        let clock = self.clock as f32;
        let Some(lying) = self.hero_pose.lying.as_mut() else {
            return;
        };
        for (i, l) in lying.snores.iter_mut().enumerate() {
            let Some(head) = head else {
                l.set_visible(false);
                continue;
            };
            let k = ((clock + i as f32 * SNORE_SECS / SNORES as f32) % SNORE_SECS) / SNORE_SECS;
            let alpha = (k * std::f32::consts::PI).sin();
            l.set_visible(true);
            l.set_global_position(head + Vector3::new(0.12 + 0.2 * k, 0.12 + 0.55 * k, 0.0));
            l.set_scale(Vector3::splat(0.6 + 0.7 * k));
            l.set_modulate(SNORE.with_alpha(alpha));
            l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.6 * alpha));
        }
    }

    /// Where the hero's head is now (the rider's while they ride).
    fn hero_head(&mut self) -> Option<Vector3> {
        let at = self.hero_at?;
        let m = match self.rider.as_mut() {
            Some(r) => &mut r.model,
            None => {
                let nodes = self.cells.get_mut(&at).filter(|n| n.look.hero)?;
                let i = nodes.look.entity.filter(|&i| i < nodes.models.len())?;
                &mut nodes.models[i]
            }
        };
        let head = self.art.slot_anchor(m, "head")?;
        head.is_inside_tree().then(|| head.get_global_position())
    }

    /// The spell of helplessness shown now: how, and where it is
    /// (self-tests).
    pub fn hero_helpless(&self) -> Option<(Helpless, HelplessPhase)> {
        self.hero_pose.lying.as_ref().map(|l| (l.kind, l.phase))
    }

    /// The prayer shown now: the god's answer and seconds since it began
    /// (self-tests).
    pub fn hero_prayer(&self) -> Option<(PrayerOutcome, f32)> {
        self.hero_pose.prayer.as_ref().map(|p| (p.outcome, p.t))
    }

    /// What holds the hero as drawn, how deep they stand, the pose clip
    /// held (self-tests).
    pub fn hero_held(&self) -> (Option<Held>, f32, Option<&'static str>) {
        (
            self.hero_pose.drawn.map(|(h, _)| h),
            self.hero_pose.sink,
            self.hero_pose.clip,
        )
    }
}
