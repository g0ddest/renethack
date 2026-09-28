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
/// ended yet, or two cells in one turn) is quicker and runs.
pub const STEP_SECS: f32 = 0.24;
pub const HURRY_SECS: f32 = 0.18;

/// Seconds to cross `cells` cells (1 or 2), and whether that is a run.
pub fn pace(cells: i32, hurry: bool) -> (f32, bool) {
    match (cells, hurry) {
        (c, _) if c > 1 => (HURRY_SECS * 0.85 * c as f32, true),
        (_, true) => (HURRY_SECS, true),
        _ => (STEP_SECS, false),
    }
}
/// Seconds of a strike's turn and lunge (the attack clip plays on).
const STRIKE_SECS: f32 = 0.3;
/// The share of a motion spent turning to the new direction.
const TURN_SHARE: f32 = 0.35;
/// Playback speed of the gait clips: the stride keeps up with the step.
const WALK_SPEED: f32 = 1.6;
const RUN_SPEED: f32 = 1.25;
/// How far a body without an attack clip lunges at its target (metres).
const LUNGE: f32 = 0.3;
const NUDGE: f32 = 0.08;
/// Roll of a swaying body (degrees).
const SWAY_ROLL: f32 = 6.0;

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
    Step { hop: f32 },
    Strike { toward: Vector3, reach: f32 },
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
    clips: Clips,
    /// Where it is now without hops or lunges: the camera, the hero's ring
    /// and torch follow it.
    pub base: Vector3,
}

fn place(node: &mut Gd<Node3D>, pos: Vector3, yaw: f32, roll: f32) {
    if !node.is_instance_valid() {
        return;
    }
    let rot = Vector3::new(0.0, yaw.to_radians(), roll.to_radians());
    node.set_transform(Transform3D::new(
        Basis::from_euler(EulerOrder::YXZ, rot),
        pos,
    ));
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
                let speed = if hurry { RUN_SPEED } else { WALK_SPEED };
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
        let reach = match (&mut clips.player, &clips.attack) {
            (Some(p), Some(attack)) => {
                p.play_ex().name(attack.as_str()).custom_blend(0.08).done();
                if let Some(idle) = &clips.idle {
                    p.queue(idle.as_str());
                }
                NUDGE
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
            clips,
            base: at,
        };
        m.apply(0.0);
        m
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
        let yaw = turn(self.yaw_from, self.yaw_to, t / TURN_SHARE);
        match self.kind {
            Kind::Step { hop } => {
                self.base = self.from.lerp(self.to, ease(t));
                let (lift, roll) = sway(hop, t);
                let pos = self.base + Vector3::new(0.0, lift, 0.0);
                place(&mut self.node, pos, yaw, roll);
            }
            Kind::Strike { toward, reach } => {
                self.base = self.to;
                let out = reach * (std::f32::consts::PI * t).sin();
                place(&mut self.node, self.to + toward * out, yaw, 0.0);
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

    /// Put the node where the world model has it, facing its way, idle
    /// again after a step; true when it had arrived by itself.
    pub fn finish(mut self) -> bool {
        let arrived = self.elapsed >= self.duration;
        place(&mut self.node, self.to, self.yaw_to, 0.0);
        self.base = self.to;
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
    fn a_hold_stops_a_motion_midway() {
        assert_eq!(progress(0.12, 0.24, None), 0.5);
        assert_eq!(progress(1.0, 0.24, None), 1.0);
        assert_eq!(progress(1.0, 0.24, Some(0.45)), 0.45);
        assert_eq!(progress(0.0, 0.0, None), 1.0);
    }
}
