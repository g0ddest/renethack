//! Idle life: what stands still is not a prop. A body with no clip of its
//! own playing (no idle among its clips) breathes, its root swelling and
//! settling a little, and shifts its weight, each in its own time; one
//! that stands turned away from the hero since it last stepped looks round
//! at them now and then. The hero, standing, glances now and then at what
//! they see nearest (else to a side): the neck and the head only, over the
//! idle clip (`bearing::Glance`), and back. Roots and bones only: no mesh
//! is read or touched, no material made. A child of map_view.
//!
//! Self-tests measure bodies standing: they run without it (`set_on`),
//! but `idle` and the soak.

use std::sync::atomic::{AtomicBool, Ordering};

use godot::classes::SkeletonModifier3D;
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::World;

use crate::animator::yaw_toward;
use crate::art::Pose;
use crate::bearing::Glance;

use super::MapView;

/// A breath: the share of its height a body swells by (half of it taken
/// from its breadth), and its seconds. A shift of weight: the degrees it
/// leans to a side and those it turns by, and its seconds. Only within
/// `NEAR` cells of the hero.
const BREATH: f32 = 0.035;
const BREATH_SECS: f32 = 3.4;
const SHIFT_DEG: (f32, f32) = (2.0, 3.0);
const SHIFT_SECS: f32 = 7.3;
const NEAR: i32 = 14;
/// A body standing turned more than `TURNED_AWAY` degrees from the hero
/// looks round at them about this often (seconds; each its own moment).
const LOOK_ROUND_SECS: f64 = 6.0;
const TURNED_AWAY: f32 = 30.0;
/// The hero's glance: the seconds between two (from, to), the seconds
/// they must have stood first, how long the look is held, the most the
/// head turns (degrees), what is too near straight ahead to turn for, a
/// glance to a side with nobody to look at, how far off somebody is
/// looked at (cells), and how fast the head goes and comes back (a share
/// of what is left, a second).
const GLANCE_EVERY: (f64, f64) = (5.0, 9.0);
const GLANCE_STOOD: f32 = 1.5;
const GLANCE_HOLD: f32 = 1.1;
const GLANCE_MOST: f32 = 62.0;
const GLANCE_LEAST: f32 = 14.0;
const GLANCE_ASIDE: f32 = 34.0;
const GLANCE_FAR: i32 = 9;
const GLANCE_RATE: (f32, f32) = (7.0, 4.5);
/// The name of the glance's modifier on the hero's skeleton.
const GLANCE: &str = "Glance";

static ON: AtomicBool = AtomicBool::new(true);

/// Idle life on or off (self-tests).
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// The hero's glance.
pub(super) struct Idle {
    /// The head's turn now and what it eases to (degrees to their left).
    turn: f32,
    to: f32,
    /// Seconds the look is still held; seconds the hero has stood.
    hold: f32,
    stood: f32,
    /// When the next glance is due (the map's clock), and how many were.
    next: f64,
    count: u32,
    /// The modifier on the hero's skeleton, with the model's inner node.
    glance: Option<(InstanceId, Gd<Glance>)>,
    /// Every body turned away looks round now (self-tests).
    all_now: bool,
}

impl Idle {
    pub(super) fn new() -> Self {
        Idle {
            turn: 0.0,
            to: 0.0,
            hold: 0.0,
            stood: 0.0,
            next: GLANCE_EVERY.0,
            count: 0,
            glance: None,
            all_now: false,
        }
    }
}

/// `a` to `b` in degrees, the short way round (-180..180).
fn off(a: f32, b: f32) -> f32 {
    (b - a + 540.0).rem_euclid(360.0) - 180.0
}

/// A number in 0..1 from a count (the glances' own time, not a clock's).
fn scatter(n: u32) -> f64 {
    (f64::from(n) * 0.618_033_988_75 + 0.37).fract()
}

/// Where a glance turns the head: toward `toward` (the heading to what is
/// looked at) from the body's heading `yaw`, no further than the neck
/// goes; None when that is straight ahead already.
fn glance_to(yaw: f32, toward: f32) -> Option<f32> {
    let d = off(yaw, toward);
    (d.abs() >= GLANCE_LEAST).then(|| d.clamp(-GLANCE_MOST, GLANCE_MOST))
}

impl MapView {
    pub(super) fn idle_life(
        &mut self,
        world: &World,
        catalog: &Catalog,
        hero: Option<(i32, i32)>,
        delta: f64,
    ) {
        if !ON.load(Ordering::Relaxed) {
            return;
        }
        self.breathe();
        self.look_round(world, catalog, hero, delta);
        self.hero_glance(world, hero, delta as f32);
    }

    /// The hero's glance is due now, and every body turned away looks
    /// round (self-tests: a strip shows them).
    pub fn idle_now(&mut self) {
        self.idle.next = self.clock;
        self.idle.stood = GLANCE_STOOD;
        self.idle.all_now = true;
    }

    /// The hero's head turned by a glance now (degrees), and the glances
    /// so far (self-tests).
    pub fn hero_glance_now(&self) -> (f32, u32) {
        (self.idle.turn, self.idle.count)
    }

    /// A creature standing still with no clip of its own playing breathes
    /// and shifts its weight, in its own time. Its root's basis only.
    fn breathe(&mut self) {
        let Some((hx, hy)) = self.hero_at else {
            return;
        };
        let t = self.clock as f32;
        let mut breathing = 0;
        for y in hy - NEAR..=hy + NEAR {
            for x in hx - NEAR..=hx + NEAR {
                let Some(nodes) = self.cells.get(&(x, y)) else {
                    continue;
                };
                let Some(i) = nodes.look.entity.filter(|_| !nodes.look.hero) else {
                    continue;
                };
                let (Some(m), Some(p)) = (nodes.models.get(i), nodes.look.models.get(i)) else {
                    continue;
                };
                if nodes.look.mon.is_none()
                    || p.look.pose != Pose::Alive
                    || !m.node.is_instance_valid()
                    || m.player().is_some_and(|pl| pl.is_playing())
                    || self.motions.iter().any(|mo| mo.cell == (x, y))
                {
                    continue;
                }
                let own = ((x * 7 + y * 13) as f32 * 0.618).fract();
                let breath = (std::f32::consts::TAU * (t / BREATH_SECS + own)).sin();
                let shift = (std::f32::consts::TAU * (t / SHIFT_SECS + own * 3.0)).sin();
                let swell = Vector3::new(
                    1.0 - 0.5 * BREATH * breath,
                    1.0 + BREATH * breath,
                    1.0 - 0.5 * BREATH * breath,
                );
                let turn = Vector3::new(
                    0.0,
                    (p.yaw + SHIFT_DEG.1 * shift).to_radians(),
                    (SHIFT_DEG.0 * shift).to_radians(),
                );
                let basis = Basis::from_euler(EulerOrder::YXZ, turn) * Basis::from_scale(swell);
                let at = Vector3::new(x as f32, 0.0, y as f32) + p.pos;
                let mut node = m.node.clone();
                node.set_transform(Transform3D::new(basis, at));
                breathing += 1;
            }
        }
        self.stats.breathing = breathing;
    }

    /// Now and then a creature the hero sees, standing turned away from
    /// them since it last stepped or fought, looks round at the hero (its
    /// heading forgotten: the cell drawn again faces the hero, a turn).
    fn look_round(
        &mut self,
        world: &World,
        catalog: &Catalog,
        hero: Option<(i32, i32)>,
        delta: f64,
    ) {
        let Some((hx, hy)) = hero else {
            return;
        };
        let (now, then) = (self.clock, self.clock - delta);
        let all = std::mem::take(&mut self.idle.all_now);
        let due: Vec<(i32, i32)> = self
            .facing
            .iter()
            .filter(|&(&(x, y), &(_, yaw))| {
                let own = ((x * 11 + y * 5) as f64 * 0.618).fract() * LOOK_ROUND_SECS;
                let tick = |c: f64| ((c + own) / LOOK_ROUND_SECS).floor();
                (all || tick(now) != tick(then))
                    && off(yaw, yaw_toward((x, y), (hx, hy))).abs() > TURNED_AWAY
            })
            .map(|(&c, _)| c)
            .filter(|c| {
                self.cells.get(c).is_some_and(|n| {
                    n.look.entity.is_some()
                        && n.look.mon.is_some()
                        && !n.look.hero
                        && !n.look.sensed
                        && !self.motions.iter().any(|m| m.cell == *c)
                })
            })
            .collect();
        for (x, y) in due {
            self.stats.glances += 1;
            self.facing.remove(&(x, y));
            self.update_cell(x, y, world, catalog, hero);
        }
    }

    /// The heading from the hero to the creature they see nearest, within
    /// `GLANCE_FAR` cells and not beside them (those they face).
    fn glance_at(&self, world: &World, (hx, hy): (i32, i32)) -> Option<f32> {
        let mut best: Option<(i32, (i32, i32))> = None;
        for y in hy - GLANCE_FAR..=hy + GLANCE_FAR {
            for x in hx - GLANCE_FAR..=hx + GLANCE_FAR {
                let d = (x - hx).abs().max((y - hy).abs());
                let seen = world.map.cell(x, y).is_some_and(|c| {
                    c.glyph.as_ref().is_some_and(|g| {
                        g.kind == GlyphKind::Mon && g.flags & (mg::HERO | mg::DETECT) == 0
                    })
                });
                if d >= 2 && seen && best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, (x, y)));
                }
            }
        }
        best.map(|(_, at)| yaw_toward((hx, hy), at))
    }

    /// The hero standing glances now and then at what they see nearest,
    /// else to a side: the neck and the head, over whatever clip plays,
    /// and back. A step, an order or a pose of its own takes the head back.
    fn hero_glance(&mut self, world: &World, hero: Option<(i32, i32)>, delta: f32) {
        let standing = hero.is_some()
            && self.order_pace.is_none()
            && self.rider.is_none()
            && !self.hero_down
            && !self.motions.iter().any(|m| m.hero)
            && self.hero_helpless().is_none()
            && self.hero_prayer().is_none()
            && self.hero_held().2.is_none();
        let clock = self.clock;
        let yaw = self.hero_yaw;
        let idle = &mut self.idle;
        if !standing {
            (idle.stood, idle.hold, idle.to) = (0.0, 0.0, 0.0);
        } else {
            idle.stood += delta;
            if idle.hold > 0.0 {
                idle.hold -= delta;
                if idle.hold <= 0.0 {
                    idle.to = 0.0;
                    let wait =
                        GLANCE_EVERY.0 + (GLANCE_EVERY.1 - GLANCE_EVERY.0) * scatter(idle.count);
                    idle.next = clock + wait;
                }
            } else if idle.to == 0.0 && idle.stood >= GLANCE_STOOD && clock >= idle.next {
                let n = idle.count;
                let aside = if n.is_multiple_of(2) {
                    GLANCE_ASIDE
                } else {
                    -GLANCE_ASIDE
                };
                let toward = hero.and_then(|h| self.glance_at(world, h));
                let idle = &mut self.idle;
                idle.to = toward.and_then(|t| glance_to(yaw, t)).unwrap_or(aside);
                idle.hold = GLANCE_HOLD;
                idle.count += 1;
            }
        }
        let idle = &mut self.idle;
        let rate = if idle.to == 0.0 {
            GLANCE_RATE.1
        } else {
            GLANCE_RATE.0
        };
        let left = idle.to - idle.turn;
        idle.turn = if left.abs() < 0.3 {
            idle.to
        } else {
            idle.turn + left * (1.0 - (-rate * delta).exp())
        };
        let turn = idle.turn;
        if turn == 0.0 {
            // asleep with nothing to turn
            if let Some((_, g)) = idle.glance.as_ref() {
                let mut m: Gd<SkeletonModifier3D> = g.clone().upcast();
                if m.is_instance_valid() && m.is_active() {
                    g.clone().bind_mut().set_turn(0.0);
                    m.set_active(false);
                }
            }
            return;
        }
        // the modifier on the skeleton of the model the hero is now
        let Some((body, inner)) = self
            .hero_model()
            .and_then(|m| Some((m.node.clone(), m.node.get_child(0)?)))
        else {
            return;
        };
        let id = inner.instance_id();
        if self.idle.glance.as_ref().map(|(i, _)| *i) != Some(id) {
            if let Some((_, mut old)) = self.idle.glance.take()
                && old.is_instance_valid()
            {
                old.bind_mut().set_turn(0.0);
                old.upcast::<SkeletonModifier3D>().set_active(false);
            }
            let skeleton = inner
                .find_children_ex("*")
                .type_("Skeleton3D")
                .owned(false)
                .done()
                .iter_shared()
                .next()
                .and_then(|n| n.try_cast::<godot::classes::Skeleton3D>().ok());
            let Some(mut sk) = skeleton.filter(|s| s.find_bone("Head") >= 0) else {
                return;
            };
            let own = sk
                .get_node_or_null(GLANCE)
                .and_then(|n| n.try_cast::<Glance>().ok());
            let glance = match own {
                Some(g) => g,
                None => {
                    let g = Glance::new_alloc();
                    let mut m: Gd<SkeletonModifier3D> = g.clone().upcast();
                    m.set_name(GLANCE);
                    sk.add_child(&m);
                    // before the others: the hands and the back go by
                    // their own, whatever the head does
                    sk.move_child(&m, 0);
                    g
                }
            };
            self.idle.glance = Some((id, glance));
        }
        if let Some((_, g)) = self.idle.glance.as_mut() {
            g.bind_mut().set_body(body);
            g.bind_mut().set_turn(turn);
            let mut m: Gd<SkeletonModifier3D> = g.clone().upcast();
            if !m.is_active() {
                m.set_active(true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glance_turns_the_short_way_and_no_further_than_a_neck_goes() {
        // straight ahead: nothing to turn for
        assert_eq!(glance_to(90.0, 95.0), None);
        // a little to a side, the short way round
        assert_eq!(glance_to(350.0, 20.0), Some(30.0));
        assert_eq!(glance_to(20.0, 350.0), Some(-30.0));
        // behind her: as far as the neck goes, to that side
        assert_eq!(glance_to(0.0, 170.0), Some(GLANCE_MOST));
        assert_eq!(glance_to(0.0, -170.0), Some(-GLANCE_MOST));
        // the waits between two are spread over their span
        for n in 0..50 {
            assert!((0.0..1.0).contains(&scatter(n)));
        }
        assert!((scatter(0) - scatter(1)).abs() > 0.2);
    }
}
