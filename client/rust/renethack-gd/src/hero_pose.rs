//! What the hero's body shows of their state (qa C3). Caught in a trap, the
//! hero's glyph hides the trap under them: the trap the host's hero notice
//! names is drawn here at their feet, and the hero stands sunk into a pit,
//! held by a bear trap's shut jaws, wrapped in a web, sinking into lava, or
//! under water (the status condition Submrg), each with a pose. Out of a
//! pit the hero climbs. A child of map_view: it moves the hero's model.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::{MeshInstance3D, OmniLight3D};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::{TrapKind, World};

use super::{
    Finish, MapView, MeshKey, Paint, Role, SHADE_LIT, at, bevel, cuboid, cylinder, plane, prism,
    torus,
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
        }
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
        let target = now.map_or(0.0, Held::sink);
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
        let sink = self.hero_pose.sink;
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
            if sink <= 0.0 {
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
        p.y = l.base - sink;
        inner.set_position(p);
        if sink <= 0.0 {
            pose.lowered = None;
        }
    }

    /// Hold `clip` on the hero while no motion of theirs plays; back to
    /// their idle when it ends.
    fn hold_clip(&mut self, clip: Option<&'static str>) {
        if self.hero_pose.climbing > 0.0 {
            return;
        }
        let busy = self
            .motions
            .iter()
            .any(|m| m.hero && Some(m.cell) == self.hero_at);
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
                    self.settle_hero();
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
