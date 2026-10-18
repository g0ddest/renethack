//! Falling through a trap door or a hole, as it is seen. The hero who
//! falls ("A trap door opens up under you!", "There's a gaping hole under
//! you!", a hole they dug: "You fall through...", or with nothing said
//! from the hole drawn under them) is shown falling before the level
//! they land on: the map holds the level they leave a moment,
//! the opening under them cut in its floor (a trap door's lid swinging
//! down into the dark), and they drop into it fast, faster as they go,
//! their arms up, then all goes black and the new level comes up as a
//! level does. A child of map_view.

use godot::classes::{AnimationPlayer, MeshInstance3D, Node3D};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::World;

use super::fall::Gone;
use super::opening::{self, OPENING, Opening};
use super::{Look, MapView, Paint, Solid, at, cuboid};
use crate::animator::{Motion, pace, yaw_toward};
use crate::art::{Clips, Model};
use crate::audio::Sound;

/// Seconds a trap door's lid takes to swing down; the body drops this
/// long after it opens, this far, in this long (from rest, faster as it
/// goes); the view goes black from this long after the opening, over
/// this long.
const LID_SECS: f32 = 0.16;
const DROP_AFTER: f32 = 0.04;
const DROP_DEPTH: f32 = 2.4;
const DROP_SECS: f32 = 0.35;
const BLACK_FROM: f32 = 0.24;
const BLACK_SECS: f32 = 0.2;
/// The clip a body drops with, its arms up (the hero's own clips': the
/// jump's loop in the air).
const DROP_CLIP: &str = "Jump";

/// A body dropping through an opening.
pub(super) struct Dropping {
    /// The opening's cell, and which it is.
    cell: (i32, i32),
    opening: Opening,
    node: Gd<Node3D>,
    /// A monster's model, given back once it is gone (the hero's stays
    /// theirs).
    model: Option<Model>,
    /// Its own node in it (the one the map turns and lifts), and where
    /// that stood.
    inner: Option<(Gd<Node3D>, Vector3)>,
    player: Option<Gd<AnimationPlayer>>,
    /// Its step onto the opening from the cell it was drawn on (the
    /// engine moves it there and drops it in one breath), this long: the
    /// opening opens as it gets there.
    walk: Option<Motion>,
    lead: f32,
    /// Seconds since it began.
    t: f32,
    /// The trap door's lid, its two leaves swinging down on their hinges.
    lid: Option<[Gd<Node3D>; 2]>,
    /// The cell's look before it opened, and its solids opened: put back
    /// once the body is gone (a lid shut again over the shaft), if the
    /// cell still shows it so. None: not opened yet.
    was: Option<Look>,
    open: Option<Vec<Solid>>,
}

impl Dropping {
    /// Seconds since the opening opened under it (less than 0: it is
    /// still stepping onto it).
    fn since(&self) -> f32 {
        self.t - self.lead
    }

    /// Over: the body is down and, for the hero, all is black.
    fn over(&self, hero: bool) -> bool {
        let t = self.since();
        let down = t >= DROP_AFTER + DROP_SECS;
        down && (!hero || t >= BLACK_FROM + BLACK_SECS)
    }
}

/// A level come while the hero stood on a hole they know, nothing said of
/// a fall ('>' where they had escaped it): whether they went down it is
/// known once they are drawn, or a command is asked where they stand (a
/// redraw puts them where they stood, the level below elsewhere), and
/// the engine's drawing of it may take some frames to read. The level
/// they leave is held meanwhile, this long at most.
pub(super) struct Undecided {
    from: (i32, i32),
    at: (i32, i32),
    opening: Opening,
    waited: f32,
}
const UNDECIDED_SECS: f32 = 10.0;

/// What lines of the log tell of the hero and the floor: a fall through
/// it, or one they escaped.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fall {
    Told(Opening),
    Refused,
}

/// What the lines tell, the last word kept.
fn fall_of(lines: &[&str]) -> Option<Fall> {
    let mut fall = None;
    for line in lines {
        // down on purpose ('>'): "You jump through the trap door.", "You
        // fly down the hole."
        let went = |way: &str| line.starts_with("You ") && line.ends_with(way);
        if line.contains("trap door opens up under you")
            || line.contains("down through the trap door")
            || went(" through the trap door.")
        {
            fall = Some(Fall::Told(Opening::TrapDoor));
        } else if line.contains("gaping hole under you")
            || line.contains("into the gaping hole")
            || line.starts_with("You fall through")
            || line.ends_with("opens up under you!")
            || went(" down the hole.")
        {
            if fall != Some(Fall::Told(Opening::TrapDoor)) {
                fall = Some(Fall::Told(Opening::Hole));
            }
        } else if line.contains("don't fall in")
            || line.contains("don't fit through")
            || line.contains("are jerked back by your pet")
        {
            fall = Some(Fall::Refused);
        }
    }
    fall
}

impl MapView {
    /// The new lines of the log, not yet watched.
    fn new_lines<'w>(&self, world: &'w World) -> Vec<&'w str> {
        let last = world.log.last_seq();
        let since = self.log_seq.filter(|&s| s <= last).unwrap_or(last);
        world.log.since(since).map(|m| m.text.as_str()).collect()
    }

    /// A fall through the floor the new lines tell, kept till the level it
    /// takes the hero to comes (the engine's drawing of it is read a frame
    /// or more after the lines); one escaped, or the hero gone on where
    /// they stand (`moved`), forgets it.
    pub(super) fn note_fall(&mut self, world: &World, moved: bool) {
        match fall_of(&self.new_lines(world)) {
            Some(Fall::Told(o)) => self.fall_told = Some(o),
            Some(Fall::Refused) => self.fall_told = None,
            None if moved => self.fall_told = None,
            None => {}
        }
    }

    /// The hero falling through the floor as a new level arrives: the
    /// old one held while they drop, black at the end. False once there
    /// is no fall to show (the new level is drawn).
    pub(super) fn hold_for_drop(
        &mut self,
        world: &World,
        catalog: &Catalog,
        generation: u64,
        was_at: Option<(i32, i32)>,
        delta: f32,
    ) -> bool {
        let deciding = self.hero_drop.is_some() || self.undecided.is_some();
        if !deciding && self.dropped != Some(generation) {
            self.dropped = Some(generation);
            let told = match fall_of(&self.new_lines(world)) {
                Some(Fall::Told(o)) => Some(o),
                Some(Fall::Refused) => None,
                None => self.fall_told,
            };
            self.fall_told = None;
            let at = world.map.left_after(self.generation).or(was_at);
            match (told, at, was_at) {
                _ if self.rider.is_some() => {}
                (Some(o), Some(at), Some(from)) => self.hero_drop = self.hero_drops(from, at, o),
                // nothing said: down a hole they stood on, if the opening
                // is drawn under them and they are drawn elsewhere now
                (None, Some(at), Some(from)) => {
                    let under = self.cells.get(&at).and_then(|n| n.look.opening);
                    if let Some(o) = under.filter(|o| !matches!(o, Opening::Pit { .. })) {
                        self.undecided = Some(Undecided {
                            from,
                            at,
                            opening: o,
                            waited: 0.0,
                        });
                    }
                }
                _ => {}
            }
        }
        if let Some(mut u) = self.undecided.take() {
            match world.hero() {
                Some(h) if h != u.at => self.hero_drop = self.hero_drops(u.from, u.at, u.opening),
                // drawn where they stood: the level drawn again
                Some(_) => {}
                None if u.waited < UNDECIDED_SECS => {
                    u.waited += delta;
                    self.undecided = Some(u);
                    return true;
                }
                None => {}
            }
        }
        let Some(mut d) = self.hero_drop.take() else {
            return false;
        };
        d.t += delta;
        self.drop_to(&mut d, delta, catalog);
        // their ring and their lights go with them
        let p = d.node.get_position();
        let ring = self.hero_ring.get_position();
        let moved = Vector3::new(p.x - ring.x, 0.0, p.z - ring.z);
        if moved != Vector3::ZERO {
            for mut mark in [self.hero_light.clone(), self.rim.clone()] {
                let was = mark.get_position();
                mark.set_position(was + moved);
            }
            self.hero_ring.set_position(ring + moved);
        }
        let black = ((d.since() - BLACK_FROM) / BLACK_SECS).clamp(0.0, 1.0);
        if black > self.fade {
            self.fade = black;
            self.show_fade();
        }
        if !d.over(true) {
            self.hero_drop = Some(d);
            return true;
        }
        // under black: the body put back as it stood, the new level next
        if let Some((mut inner, from)) = d.inner.take().filter(|(n, _)| n.is_instance_valid()) {
            inner.set_position(from);
        }
        free_lid(d.lid.take());
        false
    }

    /// The hero (drawn on the cell `from`) begins to drop through the
    /// opening `o` on `at`.
    fn hero_drops(&mut self, from: (i32, i32), at: (i32, i32), o: Opening) -> Option<Dropping> {
        let n = self.cells.get(&from)?;
        let i = n.look.entity?;
        let model = n.models.get(i).filter(|m| m.node.is_instance_valid())?;
        let placed = n.look.models.get(i)?;
        let (node, height, yaw) = (model.node.clone(), placed.look.art.height, placed.yaw);
        let clips = self.art.clips(model, false);
        let (secs, run) = pace(1, false);
        let secs = self.hero_look.pace(secs, run).0;
        Some(self.begin_drop(node, None, clips, (from, yaw, height, secs), at, o))
    }

    /// A body on the cell `from` (standing as `yaw`, so tall, a step
    /// taking it `secs`) begins to drop through the opening `o` on `at`:
    /// it steps onto it first if it is not there.
    fn begin_drop(
        &mut self,
        node: Gd<Node3D>,
        model: Option<Model>,
        clips: Clips,
        (from, yaw, height, secs): ((i32, i32), f32, f32, f32),
        at: (i32, i32),
        o: Opening,
    ) -> Dropping {
        let player = clips.player.clone().or_else(|| find_player(&node));
        let ground = self.ground(at.0, at.1).min(0.0);
        let to = Vector3::new(at.0 as f32, ground, at.1 as f32);
        let near = (from.0 - at.0).abs().max((from.1 - at.1).abs()) == 1;
        let walk = near.then(|| {
            Motion::step(
                node.clone(),
                at,
                model.is_none(),
                (node.get_position(), yaw),
                (to, yaw_toward(from, at)),
                clips,
                height,
                (secs, false),
            )
        });
        if walk.is_none() {
            let mut node = node.clone();
            node.set_position(to);
        }
        let lead = if walk.is_some() { secs } else { 0.0 };
        let inner = node
            .get_child(0)
            .and_then(|n| n.try_cast::<Node3D>().ok())
            .map(|n| {
                let p = n.get_position();
                (n, p)
            });
        // the thud below
        let sound_at = self.sound_at(at);
        self.sounds.push(
            Sound::new("trap.fall")
                .at(sound_at)
                .after(lead + DROP_AFTER + DROP_SECS),
        );
        Dropping {
            cell: at,
            opening: o,
            node,
            model,
            inner,
            player,
            walk,
            lead,
            t: 0.0,
            lid: None,
            was: None,
            open: None,
        }
    }

    /// The cell `at` opened for a drop: its floor cut to the shaft (it may
    /// not have shown the trap), a trap door's lid taken off and made to
    /// swing. What it showed before, and the lid's two leaves.
    fn open_for_drop(
        &mut self,
        at: (i32, i32),
        o: Opening,
        catalog: &Catalog,
    ) -> (Option<Look>, Option<[Gd<Node3D>; 2]>, Vec<Solid>) {
        let Some(was) = self.cells.get(&at).map(|n| n.look.clone()) else {
            return (None, None, Vec::new());
        };
        let mut look = was.clone();
        if look.opening.is_none() || !look.sunk {
            look.solids.retain(|s| !opening::is_closed_part(s));
            let ctx = self.cell_ctx(at, catalog);
            let floor = Paint::Flat(super::FLOOR_UNSEEN, super::Finish::Matte);
            opening::open_floor(&mut look, o, floor, &ctx);
            look.opening = Some(o);
        }
        let paints = opening::unlid(&mut look);
        let lid = match (o, paints) {
            (Opening::TrapDoor, Some((wood, _))) => Some(self.lid_leaves(at, wood)),
            (Opening::TrapDoor, None) => {
                let ctx = self.cell_ctx(at, catalog);
                let wood = opening::wood(&ctx);
                Some(self.lid_leaves(at, wood))
            }
            _ => None,
        };
        let (sunk, open) = (look.sunk, look.solids.clone());
        self.reshape(at, look.solids, look.opening, sunk);
        if std::mem::take(&mut self.bedrock_dirty) {
            self.lay_bedrock();
        }
        (Some(was), lid, open)
    }

    /// A monster seen to fall through the floor (its model gone from its
    /// cell, `gone`) drops into the opening there, or next to it: the
    /// engine moves it onto the trap and drops it before it is drawn
    /// there.
    pub(super) fn drop_gone(&mut self, gone: Gone, o: Opening) {
        let Gone {
            cell,
            model,
            placed,
            ..
        } = gone;
        // what moved the body ends now: it drops where it stands
        self.finish_motions_where(|m| m.cell == cell);
        let at = self.opening_near(cell, o).unwrap_or(cell);
        let node = model.node.clone();
        let clips = self.art.clips(&model, false);
        let how = (cell, placed.yaw, placed.look.art.height, pace(1, false).0);
        let d = self.begin_drop(node, Some(model), clips, how, at, o);
        self.drops.push(d);
    }

    /// The cell that shows the opening `o` laid open: `cell` itself, else
    /// one next to it.
    fn opening_near(&self, (x, y): (i32, i32), o: Opening) -> Option<(i32, i32)> {
        let shows = |c: &(i32, i32)| {
            self.cells
                .get(c)
                .is_some_and(|n| n.look.opening == Some(o) && n.look.sunk)
        };
        let around = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
        std::iter::once((x, y)).chain(around).find(shows)
    }

    /// The hero dropping through the floor: the seconds since it opened
    /// under them, less than 0 while they step onto it (None: they are
    /// not dropping).
    pub fn hero_dropping(&self) -> Option<f32> {
        self.hero_drop.as_ref().map(Dropping::since)
    }

    /// The monsters seen dropping through the floor: the openings they
    /// drop through, and the seconds since each opened.
    pub fn drops_shown(&self) -> Vec<((i32, i32), f32)> {
        self.drops.iter().map(|d| (d.cell, d.since())).collect()
    }

    /// The monsters dropping: on down; one gone gives its model back, the
    /// cell it fell through shut as it was.
    pub(super) fn advance_drops(&mut self, delta: f32, catalog: &Catalog) {
        for mut d in std::mem::take(&mut self.drops) {
            d.t += delta;
            self.drop_to(&mut d, delta, catalog);
            if d.over(false) {
                self.end_drop(d);
            } else {
                self.drops.push(d);
            }
        }
    }

    /// Every drop ended now (a new level, the map cleared).
    pub(super) fn end_drops(&mut self) {
        for d in std::mem::take(&mut self.drops) {
            self.end_drop(d);
        }
    }

    /// Every drop and fall told forgotten (the map cleared for another
    /// game): the hero's as well, its level never to come.
    pub(super) fn forget_drops(&mut self) {
        self.end_drops();
        if let Some(mut d) = self.hero_drop.take() {
            free_lid(d.lid.take());
            if let Some((mut inner, from)) = d.inner.take().filter(|(n, _)| n.is_instance_valid()) {
                inner.set_position(from);
            }
        }
        self.undecided = None;
        self.fall_told = None;
        self.dropped = None;
        self.hero_trap_drawn = None;
    }

    fn end_drop(&mut self, mut d: Dropping) {
        if let Some(walk) = d.walk.take() {
            walk.finish(false);
        }
        free_lid(d.lid.take());
        if let Some((mut inner, from)) = d.inner.take().filter(|(n, _)| n.is_instance_valid()) {
            inner.set_position(from);
        }
        if let Some(model) = d.model.take() {
            self.art.give(model);
        }
        // the trap door shut again over its shaft
        let shown = self.cells.get(&d.cell).map(|n| &n.look.solids);
        if let Some(was) = d.was.take().filter(|_| shown == d.open.as_ref()) {
            self.reshape(d.cell, was.solids, was.opening, was.sunk);
        }
    }

    /// A trap door's two leaves, shut over the opening at `at`, each on
    /// its hinge at the opening's edge.
    fn lid_leaves(&mut self, at: (i32, i32), wood: Paint) -> [Gd<Node3D>; 2] {
        let mesh_key = cuboid(OPENING / 2.0, 0.04, OPENING);
        let mesh = self.art.mesh(mesh_key);
        let mat = self.material(wood);
        let origin = Vector3::new(at.0 as f32, 0.0, at.1 as f32);
        [-1.0f32, 1.0].map(|side| {
            let mut hinge = Node3D::new_alloc();
            hinge.set_position(origin + at3(side * OPENING / 2.0, 0.02, 0.0));
            let mut leaf = MeshInstance3D::new_alloc();
            leaf.set_mesh(&mesh);
            leaf.set_material_override(&mat);
            leaf.set_position(at3(-side * OPENING / 4.0, 0.0, 0.0));
            hinge.add_child(&leaf);
            self.root.add_child(&hinge);
            hinge
        })
    }

    /// The body and the lid where `d` has them now: stepping onto the
    /// opening, then the opening opened under it as it gets there, the
    /// lid swinging down, the body dropping.
    fn drop_to(&mut self, d: &mut Dropping, delta: f32, catalog: &Catalog) {
        if let Some(mut walk) = d.walk.take() {
            if walk.advance(delta, None) {
                // its stride gives way to the drop's clip, else to its idle
                walk.finish(drop_clip(d).is_some());
            } else {
                d.walk = Some(walk);
            }
        }
        let since = d.since();
        if since < 0.0 {
            return;
        }
        if d.open.is_none() {
            if d.model.is_none() {
                // the hero's ring stays on the level they leave
                self.hero_ring.set_visible(false);
            }
            let (was, lid, open) = self.open_for_drop(d.cell, d.opening, catalog);
            (d.was, d.lid, d.open) = (was, lid, Some(open));
            // arms up as it drops
            if let Some(mut p) = drop_clip(d) {
                p.play_ex().name(DROP_CLIP).custom_blend(0.08).done();
            }
        }
        if let Some(lid) = d.lid.as_mut() {
            let k = (since / LID_SECS).clamp(0.0, 1.0);
            let swing = (1.0 - (1.0 - k) * (1.0 - k)) * std::f32::consts::FRAC_PI_2;
            for (leaf, side) in lid.iter_mut().zip([-1.0f32, 1.0]) {
                if leaf.is_instance_valid() {
                    leaf.set_rotation(Vector3::new(0.0, 0.0, side * swing));
                }
            }
        }
        let k = ((since - DROP_AFTER) / DROP_SECS).clamp(0.0, 1.0);
        if let Some((inner, from)) = d.inner.as_mut().filter(|(n, _)| n.is_instance_valid()) {
            inner.set_position(*from - Vector3::new(0.0, DROP_DEPTH * k * k, 0.0));
        }
        if k >= 1.0 && d.node.is_instance_valid() {
            d.node.set_visible(false);
        }
    }
}

fn at3(x: f32, y: f32, z: f32) -> Vector3 {
    at(x, y, z)
}

/// The player of the body's clips, if it has the drop's.
fn drop_clip(d: &Dropping) -> Option<Gd<AnimationPlayer>> {
    d.player
        .clone()
        .filter(|p| p.is_instance_valid() && p.has_animation(DROP_CLIP))
}

/// The animation player under a model's node.
fn find_player(node: &Gd<Node3D>) -> Option<Gd<AnimationPlayer>> {
    node.find_children_ex("*")
        .type_("AnimationPlayer")
        .owned(false)
        .done()
        .iter_shared()
        .next()
        .and_then(|n| n.try_cast::<AnimationPlayer>().ok())
}

/// A trap door's leaves taken away.
fn free_lid(lid: Option<[Gd<Node3D>; 2]>) {
    for mut leaf in lid.into_iter().flatten() {
        if leaf.is_instance_valid() {
            leaf.queue_free();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fall_through_the_floor_is_told() {
        let told = |o| Some(Fall::Told(o));
        assert_eq!(
            fall_of(&["A trap door opens up under you!"]),
            told(Opening::TrapDoor)
        );
        assert_eq!(
            fall_of(&[
                "There's a gaping hole under you!",
                "You fall down a deep shaft!"
            ]),
            told(Opening::Hole)
        );
        assert_eq!(
            fall_of(&["You dig a hole through the floor.", "You fall through..."]),
            told(Opening::Hole)
        );
        // flying, they dig it and stay
        assert_eq!(fall_of(&["You dig a hole through the floor."]), None);
        assert_eq!(
            fall_of(&["A trap door opens up under you!", "You don't fall in."]),
            Some(Fall::Refused)
        );
        // the last word kept: escaped one, then down another
        assert_eq!(
            fall_of(&[
                "You are jerked back by your pet!",
                "There's a gaping hole under you!"
            ]),
            told(Opening::Hole)
        );
        assert_eq!(
            fall_of(&["You jump through the trap door."]),
            told(Opening::TrapDoor)
        );
        assert_eq!(
            fall_of(&["You slither down the hole."]),
            told(Opening::Hole)
        );
        assert_eq!(fall_of(&["You climb up the stairs."]), None);
    }
}
