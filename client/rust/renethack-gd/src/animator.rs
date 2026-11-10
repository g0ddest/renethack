//! Cosmetic motion of the map's model nodes (spec 5.2): a step carries an
//! entity's own node from the cell it left to the one it reached, turning it
//! the way it goes, with its walk (or run) clip, or a hop and a sway for a
//! body without one; a strike turns an attacker to its target with its
//! attack clip, or a lunge. The world model stays authoritative: a motion
//! only ever ends where the map already put the entity, and the map ends
//! every motion at once when the next batch of drawing arrives.

use godot::prelude::*;

use crate::art::Clips;

/// Seconds a step takes; a step taken in a hurry (the previous one had not
/// ended yet, or two cells in one turn) is quicker and runs (not so quick
/// that three cells go by in 0.4 s, a dash).
pub const STEP_SECS: f32 = 0.24;
pub const HURRY_SECS: f32 = 0.2;

/// Seconds to cross `cells` cells (1 or 2), and whether that is a run.
pub fn pace(cells: i32, hurry: bool) -> (f32, bool) {
    match (cells, hurry) {
        (c, _) if c > 1 => (HURRY_SECS * 0.9 * c as f32, true),
        (_, true) => (HURRY_SECS, true),
        _ => (STEP_SECS, false),
    }
}
/// Seconds of a strike's turn and lunge (the attack clip plays on).
const STRIKE_SECS: f32 = 0.3;
/// Seconds a body takes to turn to a new heading, wherever it turns (a
/// step, a blow, facing a foe): never a snap in one frame.
pub const TURN_SECS: f32 = 0.15;
/// How fast the ground goes by under the walk and the run clips played at
/// their own speed (metres a second, for a body 1.75 m tall; the UAL
/// clips, measured): a gait clip is played so its stride keeps up with the
/// step, its feet not sliding, within these bounds.
const WALK_GROUND: f32 = 1.1;
const RUN_GROUND: f32 = 3.2;
const BODY_HEIGHT: f32 = 1.75;
const GAIT_SCALE: (f32, f32) = (0.6, 2.0);
/// The hero runs (the run clip) when a step goes faster than this (metres
/// a second); a slow step (a burdened hero, a slow order) walks.
pub const HERO_RUNS_FROM: f32 = 2.0;
/// How far a body without an attack clip lunges at its target, how far a
/// monster's attack clip carries it (a bite, a claw), and the hero's
/// (her clips carry her) (metres).
const LUNGE: f32 = 0.3;
const BITE: f32 = 0.22;
const NUDGE: f32 = 0.08;
/// Seconds the idle takes to blend in after an attack clip.
const AFTER_BLOW: f64 = 0.15;
/// Roll of a swaying body (degrees).
const SWAY_ROLL: f32 = 6.0;
/// A blow's reel: how far the one struck is knocked back (metres), and
/// for how long, out and back; how far it staggers back from the blow, a
/// lean from its feet (degrees).
pub const REEL: f32 = 0.16;
const REEL_SECS: f32 = 0.32;
const STAGGER: f32 = 11.0;

/// The speed a gait clip plays at for a step of `metres` in `secs` by a
/// body `height` tall: its stride keeps up with the step (a bigger body's
/// stride is longer), the run's or the walk's.
pub fn gait_speed(metres: f32, secs: f32, height: f32, run: bool) -> f32 {
    let ground = if run { RUN_GROUND } else { WALK_GROUND } * (height / BODY_HEIGHT).max(0.2);
    (metres / secs.max(0.01) / ground).clamp(GAIT_SCALE.0, GAIT_SCALE.1)
}

/// The share of a lunge or a reel out from where it stands at progress t:
/// quick out, slower back.
fn out_and_back(t: f32) -> f32 {
    (std::f32::consts::PI * t.clamp(0.0, 1.0).powf(0.55))
        .sin()
        .max(0.0)
}

/// Smooth in and out, 0..1.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// From one yaw towards another (degrees) the short way round.
pub fn turn(from: f32, to: f32, t: f32) -> f32 {
    let d = (to - from + 540.0).rem_euclid(360.0) - 180.0;
    from + d * ease(t)
}

/// The yaw (degrees about y, 0 facing +z) of a look from one cell to another.
pub fn yaw_toward(from: (i32, i32), to: (i32, i32)) -> f32 {
    ((to.0 - from.0) as f32)
        .atan2((to.1 - from.1) as f32)
        .to_degrees()
}

/// How high a body without a walk hops (two hops a step), by its height.
pub fn hop_height(height: f32) -> f32 {
    (height * 0.3).clamp(0.03, 0.09)
}

/// A swaying body's (lift, roll in degrees) at progress t of a step.
pub fn sway(hop: f32, t: f32) -> (f32, f32) {
    let w = std::f32::consts::TAU * t;
    let roll = if hop > 0.0 { SWAY_ROLL * w.sin() } else { 0.0 };
    (hop * w.sin().abs(), roll)
}

/// Progress 0..1 of a motion; `hold` (self-tests) stops it at that share.
pub fn progress(elapsed: f32, duration: f32, hold: Option<f32>) -> f32 {
    let t = if duration > 0.0 {
        elapsed / duration
    } else {
        1.0
    };
    t.min(hold.unwrap_or(1.0)).clamp(0.0, 1.0)
}

enum Kind {
    Step {
        hop: f32,
    },
    Strike {
        toward: Vector3,
        reach: f32,
    },
    Reel {
        away: Vector3,
    },
    /// Turning where it stands (a new heading, no step).
    Turn,
}

pub struct Motion {
    node: Gd<Node3D>,
    /// The cell the entity stands on in the world model.
    pub cell: (i32, i32),
    pub hero: bool,
    kind: Kind,
    pub from: Vector3,
    pub to: Vector3,
    yaw_from: f32,
    pub yaw_to: f32,
    elapsed: f32,
    duration: f32,
    /// When its turn from `yaw_from` to `yaw_to` began (seconds in).
    turn_start: f32,
    clips: Clips,
    /// Where it is now without hops or lunges: the camera, the hero's ring
    /// and torch follow it.
    pub base: Vector3,
}

fn place(node: &mut Gd<Node3D>, pos: Vector3, yaw: f32, roll: f32) {
    place_leaning(node, pos, yaw, roll, (Vector3::ZERO, 0.0));
}

/// As `place`, leaning by `lean.1` degrees from its feet toward `lean.0`
/// (a flat unit vector; a stagger back from a blow).
fn place_leaning(node: &mut Gd<Node3D>, pos: Vector3, yaw: f32, roll: f32, lean: (Vector3, f32)) {
    if !node.is_instance_valid() {
        return;
    }
    let rot = Vector3::new(0.0, yaw.to_radians(), roll.to_radians());
    let mut basis = Basis::from_euler(EulerOrder::YXZ, rot);
    let axis = Vector3::UP.cross(lean.0);
    if lean.1 != 0.0 && axis.length_squared() > 1e-6 {
        basis = Basis::from_axis_angle(axis.normalized(), lean.1.to_radians()) * basis;
    }
    node.set_transform(Transform3D::new(basis, pos));
}

impl Motion {
    /// A step from `from` to `to` (world positions) of the node at `cell`.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        node: Gd<Node3D>,
        cell: (i32, i32),
        hero: bool,
        (from, yaw_from): (Vector3, f32),
        (to, yaw_to): (Vector3, f32),
        mut clips: Clips,
        height: f32,
        (duration, hurry): (f32, bool),
    ) -> Motion {
        let hop = match (&mut clips.player, &clips.gait) {
            (Some(p), Some(gait)) => {
                let flat = Vector2::new(to.x - from.x, to.z - from.z).length();
                let speed = gait_speed(flat, duration, height, hurry);
                p.play_ex()
                    .name(gait.as_str())
                    .custom_blend(0.1)
                    .custom_speed(speed)
                    .done();
                0.0
            }
            _ => hop_height(height),
        };
        let mut m = Motion {
            node,
            cell,
            hero,
            kind: Kind::Step { hop },
            from,
            to,
            yaw_from,
            yaw_to,
            elapsed: 0.0,
            duration,
            turn_start: 0.0,
            clips,
            base: from,
        };
        m.apply(0.0);
        m
    }

    /// The node at `cell` (standing at `at`) turns from `yaw_from` to
    /// `yaw_to` and attacks along `toward` (a flat unit vector).
    pub fn strike(
        node: Gd<Node3D>,
        cell: (i32, i32),
        hero: bool,
        at: Vector3,
        (yaw_from, yaw_to): (f32, f32),
        toward: Vector3,
        mut clips: Clips,
    ) -> Motion {
        // the hero's clips carry her; a monster's bite or claw carries it
        // at her too, a body with no clip lunges
        let reach = match (&mut clips.player, &clips.attack) {
            (Some(p), Some(attack)) => {
                p.play_ex().name(attack.as_str()).custom_blend(0.08).done();
                if let Some(idle) = &clips.idle {
                    // a blow that does not end as the idle stands (a dig
                    // ends with the pick raised) eases into it
                    p.set_blend_time(attack.as_str(), idle.as_str(), AFTER_BLOW);
                    p.queue(idle.as_str());
                }
                if hero { NUDGE } else { BITE }
            }
            _ => LUNGE,
        };
        let mut m = Motion {
            node,
            cell,
            hero,
            kind: Kind::Strike { toward, reach },
            from: at,
            to: at,
            yaw_from,
            yaw_to,
            elapsed: 0.0,
            duration: STRIKE_SECS,
            turn_start: 0.0,
            clips,
            base: at,
        };
        m.apply(0.0);
        m
    }

    /// The node at `cell` (standing at `at`, facing `yaw`) is knocked back
    /// along `away` (a flat unit vector) by a blow, and comes back: fast
    /// out, slower back. Its hit clip plays on (`Art::flinch`).
    pub fn reel(
        node: Gd<Node3D>,
        cell: (i32, i32),
        hero: bool,
        at: Vector3,
        yaw: f32,
        away: Vector3,
        clips: Clips,
    ) -> Motion {
        let mut m = Motion {
            node,
            cell,
            hero,
            kind: Kind::Reel { away },
            from: at,
            to: at,
            yaw_from: yaw,
            yaw_to: yaw,
            elapsed: 0.0,
            duration: REEL_SECS,
            turn_start: 0.0,
            clips,
            base: at,
        };
        m.apply(0.0);
        m
    }

    /// The node at `cell` (standing at `at`) turns from `yaw_from` to
    /// `yaw_to` where it stands, over `TURN_SECS` (a new heading with no
    /// step: facing a foe, a monster facing the hero who moved).
    pub fn turn(
        node: Gd<Node3D>,
        cell: (i32, i32),
        hero: bool,
        at: Vector3,
        (yaw_from, yaw_to): (f32, f32),
        clips: Clips,
    ) -> Motion {
        let mut m = Motion {
            node,
            cell,
            hero,
            kind: Kind::Turn,
            from: at,
            to: at,
            yaw_from,
            yaw_to,
            elapsed: 0.0,
            duration: TURN_SECS,
            turn_start: 0.0,
            clips,
            base: at,
        };
        m.apply(0.0);
        m
    }

    /// The heading it turns to now, and from where it faces this moment,
    /// over `TURN_SECS` (a blow on its way in, a foe beside it).
    pub fn turn_to(&mut self, yaw: f32) {
        let now = self.yaw_now();
        self.yaw_from = now;
        self.yaw_to = yaw;
        self.turn_start = self.elapsed;
        // a turn alone lasts the turn
        if matches!(self.kind, Kind::Turn) {
            self.duration = self.elapsed + TURN_SECS;
        }
    }

    /// The heading it faces this moment.
    pub fn yaw_now(&self) -> f32 {
        turn(
            self.yaw_from,
            self.yaw_to,
            (self.elapsed - self.turn_start) / TURN_SECS,
        )
    }

    /// (the gait clip a step plays, the clip playing now) (self-tests).
    pub fn clips_now(&self) -> (Option<String>, Option<String>) {
        let now = self
            .clips
            .player
            .as_ref()
            .filter(|p| p.is_instance_valid() && p.is_playing())
            .map(|p| p.get_current_animation().to_string());
        (self.clips.gait.clone(), now)
    }

    pub fn is_step(&self) -> bool {
        matches!(self.kind, Kind::Step { .. })
    }

    fn apply(&mut self, t: f32) {
        let yaw = self.yaw_now();
        match self.kind {
            Kind::Step { hop } => {
                self.base = self.from.lerp(self.to, ease(t));
                let (lift, roll) = sway(hop, t);
                let pos = self.base + Vector3::new(0.0, lift, 0.0);
                place(&mut self.node, pos, yaw, roll);
            }
            Kind::Strike { toward, reach } => {
                self.base = self.to;
                let out = reach * out_and_back(t);
                place(&mut self.node, self.to + toward * out, yaw, 0.0);
            }
            Kind::Reel { away } => {
                // knocked back and staggering back from the blow, a lean
                // from its feet (the camera, the ring and the torch stay
                // where it stands)
                self.base = self.to;
                let k = out_and_back(t);
                let lean = (away, STAGGER * k);
                place_leaning(&mut self.node, self.to + away * REEL * k, yaw, 0.0, lean);
            }
            Kind::Turn => {
                self.base = self.to;
                place(&mut self.node, self.to, yaw, 0.0);
            }
        }
    }

    /// Move on by `delta` seconds; true once it has arrived.
    pub fn advance(&mut self, delta: f32, hold: Option<f32>) -> bool {
        self.elapsed += delta;
        let t = progress(self.elapsed, self.duration, hold);
        self.apply(t);
        t >= 1.0
    }

    /// Reached the self-test's hold point (or the end).
    pub fn is_held(&self, hold: Option<f32>) -> bool {
        progress(self.elapsed, self.duration, None) >= hold.unwrap_or(1.0)
    }

    /// At its end: not short of it, nor stopped short by a self-test's
    /// hold.
    pub fn arrived(&self, hold: Option<f32>) -> bool {
        progress(self.elapsed, self.duration, hold) >= 1.0
    }

    /// Put the node where the world model has it, facing its way, idle
    /// again after a step (unless `keep_gait`: an order walks on, the
    /// stride goes on into the next step); true when it had arrived by
    /// itself.
    pub fn finish(mut self, keep_gait: bool) -> bool {
        let arrived = self.elapsed >= self.duration;
        place(&mut self.node, self.to, self.yaw_to, 0.0);
        self.base = self.to;
        if keep_gait {
            return arrived;
        }
        if let (Kind::Step { .. }, Some(p), Some(_), Some(idle)) = (
            &self.kind,
            self.clips.player.as_mut(),
            &self.clips.gait,
            &self.clips.idle,
        ) && p.is_instance_valid()
        {
            p.play_ex().name(idle.as_str()).custom_blend(0.2).done();
        }
        arrived
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_eases_in_and_out() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(0.5), 0.5);
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
    }

    #[test]
    fn turns_go_the_short_way_round() {
        assert_eq!(turn(0.0, 90.0, 1.0), 90.0);
        assert_eq!(turn(0.0, 90.0, 0.0), 0.0);
        // from facing west to facing north-west-ish across +-180
        let t = turn(170.0, -170.0, 1.0);
        assert!((t - 190.0).abs() < 1e-3, "{t}");
        assert!((turn(170.0, -170.0, 0.5) - 180.0).abs() < 1e-3);
        assert!((turn(-90.0, 90.0, 1.0).rem_euclid(360.0) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn yaw_points_along_the_step() {
        assert_eq!(yaw_toward((5, 5), (5, 6)), 0.0, "south faces the camera");
        assert_eq!(yaw_toward((5, 5), (6, 5)), 90.0);
        assert_eq!(yaw_toward((5, 5), (4, 5)), -90.0);
        assert_eq!(yaw_toward((5, 5), (5, 4)).abs(), 180.0);
        assert_eq!(yaw_toward((5, 5), (6, 6)), 45.0);
    }

    #[test]
    fn bodies_without_a_walk_hop_twice_and_sway() {
        let hop = hop_height(0.5);
        assert!(hop > 0.0 && hop <= 0.09);
        assert_eq!(sway(hop, 0.0).0, 0.0);
        assert!((sway(hop, 0.25).0 - hop).abs() < 1e-4);
        assert!(sway(hop, 0.5).0.abs() < 1e-4);
        assert!((sway(hop, 0.75).0 - hop).abs() < 1e-4);
        assert!(sway(hop, 1.0).0.abs() < 1e-4);
        assert!(sway(hop, 0.25).1 > 0.0 && sway(hop, 0.75).1 < 0.0);
        // with a clip: no hop, no roll
        assert_eq!(sway(0.0, 0.25), (0.0, 0.0));
        // tiny and huge bodies stay within bounds
        assert_eq!(hop_height(0.01), 0.03);
        assert_eq!(hop_height(5.0), 0.09);
    }

    #[test]
    fn hurried_and_double_steps_run_quicker_per_cell() {
        assert_eq!(pace(1, false), (STEP_SECS, false));
        assert_eq!(pace(1, true), (HURRY_SECS, true));
        let (two, run) = pace(2, false);
        assert!(run && two > STEP_SECS && two / 2.0 < HURRY_SECS);
    }

    #[test]
    fn the_stride_keeps_up_with_the_step() {
        // the run's stride over a cell at the step's pace: its own speed
        let run = gait_speed(1.0, 1.0 / RUN_GROUND, BODY_HEIGHT, true);
        assert!((run - 1.0).abs() < 1e-4);
        // quicker steps play it quicker, a smaller body's legs quicker still
        let step = gait_speed(1.0, STEP_SECS, BODY_HEIGHT, true);
        assert!(step > 1.0 && gait_speed(1.0, HURRY_SECS, BODY_HEIGHT, true) > step);
        assert!(gait_speed(1.0, STEP_SECS, 1.2, true) > step);
        // within bounds however slow or quick
        assert_eq!(gait_speed(1.0, 10.0, BODY_HEIGHT, false), GAIT_SCALE.0);
        assert_eq!(gait_speed(1.0, 0.0, BODY_HEIGHT, false), GAIT_SCALE.1);
        // never three cells in 0.4 s
        assert!(pace(2, true).0 / 2.0 * 3.0 > 0.5);
    }

    #[test]
    fn a_lunge_and_a_reel_go_out_quick_and_come_back() {
        assert_eq!(out_and_back(0.0), 0.0);
        assert!(out_and_back(1.0) < 1e-4);
        // the farthest early, the way back slower
        let peak = (1..100)
            .map(|i| i as f32 / 100.0)
            .max_by(|a, b| out_and_back(*a).total_cmp(&out_and_back(*b)))
            .unwrap();
        assert!(peak < 0.4, "{peak}");
    }

    #[test]
    fn a_hold_stops_a_motion_midway() {
        assert_eq!(progress(0.12, 0.24, None), 0.5);
        assert_eq!(progress(1.0, 0.24, None), 1.0);
        assert_eq!(progress(1.0, 0.24, Some(0.45)), 0.45);
        assert_eq!(progress(0.0, 0.0, None), 1.0);
    }
}
