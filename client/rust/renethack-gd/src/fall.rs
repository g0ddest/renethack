//! A monster the hero sees die falls where it stood, instead of vanishing
//! as its glyph turns at once to its corpse or to nothing. Its model is
//! kept at its cell (`Gone`) till the frame's messages are read: only a
//! death they tell falls. One with a death clip plays it, and its corpse
//! (that clip's last frame, `Pose::Corpse`) takes over in place; a body
//! built in code topples onto its side as its corpse lies; a blob, a
//! jelly, a pudding or a fungus slumps; a ghost, a light, a vortex or an
//! elemental dissolves; a gas spore is gone in its own blast. It darkens
//! as it falls to its corpse's shade; with no corpse to take over, it
//! sinks away. Only a monster the map showed (seen, not sensed) falls;
//! nothing waits for a fall: the next turn plays alongside, and a fall
//! whose cell changes again ends at once.

use godot::classes::base_material_3d::{Flags, ShadingMode, Transparency};
use godot::classes::{GeometryInstance3D, Material, StandardMaterial3D};
use godot::prelude::*;
use nh_protocol::{Catalog, MonsterInfo};

use super::{CONTACT_SECS, CORPSE_DARKEN, MapView, Placed, set_overlay};
use crate::art::{Model, Pose};

/// Seconds a body takes to topple, to slump, to dissolve; the longest a
/// death clip plays (a longer one plays quicker); the seconds a body with
/// no corpse to take over takes to sink away, and how deep it sinks (a
/// share of its height).
const TOPPLE_SECS: f32 = 0.45;
const SLUMP_SECS: f32 = 0.5;
const DISSOLVE_SECS: f32 = 0.7;
const CLIP_MOST: f32 = 1.1;
const SINK_SECS: f32 = 0.5;
/// Seconds a body takes to blink out.
const BLINK_SECS: f32 = 0.25;
const SINK_DEPTH: f32 = 0.3;
/// A slumped body's height and breadth, of its own, and the share of its
/// fall it sags in when it rolls on into its corpse's way of lying.
const SLUMPED: (f32, f32) = (0.3, 1.3);
const SAG_SHARE: f32 = 0.6;
/// How far up a dissolving body drifts, a share of its height.
const DISSOLVE_RISE: f32 = 0.25;

/// How a body falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallKind {
    /// Its death clip.
    Clip,
    /// Onto its side, as its corpse lies.
    Topple,
    /// Down into a heap.
    Slump,
    /// Fading as it drifts up.
    Dissolve,
    /// Gone at once (its blast is drawn).
    Burst,
    /// Gone in a blink, not dead: teleported away, turned invisible, turned
    /// into something else.
    Blink,
}

impl FallKind {
    /// How a monster falls: by what it is, then by what its model can do.
    pub fn of(info: &MonsterInfo, clip: bool) -> FallKind {
        match info.class.as_str() {
            _ if info.name == "gas spore" => FallKind::Burst,
            " " | "v" | "y" | "E" => FallKind::Dissolve,
            _ if clip => FallKind::Clip,
            "b" | "j" | "P" | "F" => FallKind::Slump,
            _ => FallKind::Topple,
        }
    }
}

/// Seconds a death told waits for its creature to leave its cell (the
/// engine may draw its corpse after the message, past a --More--).
const TOLD_SECS: f64 = 1.5;

/// A death the messages tell: by the name they use, the cell a blow found
/// the creature on, and when.
pub(super) struct Told {
    name: String,
    at: Option<(i32, i32)>,
    when: f64,
    /// It is gone, not dead: it blinks out.
    blink: bool,
}

impl Told {
    pub fn new(name: &str, at: Option<(i32, i32)>, when: f64) -> Told {
        Told {
            name: plain_name(name).to_string(),
            at,
            when,
            blink: false,
        }
    }

    /// A monster the messages tell gone (teleported away, invisible, turned
    /// into another): it blinks out.
    pub fn gone(name: &str, when: f64) -> Told {
        Told {
            blink: true,
            ..Told::new(name, None, when)
        }
    }
}

/// A monster's model gone from its cell this frame (not stepped off it):
/// kept till the frame's messages are read, in case one tells its death.
pub(super) struct Gone {
    pub cell: (i32, i32),
    pub model: Model,
    pub placed: Placed,
    pub mon: i32,
}

/// A body falling.
pub(super) struct Fall {
    cell: (i32, i32),
    model: Model,
    kind: FallKind,
    /// Seconds before it falls (the blow lands first), whether it began,
    /// into the fall, the fall's own, and into sinking away.
    wait: f32,
    begun: bool,
    t: f32,
    secs: f32,
    sunk: f32,
    /// The corpse that takes over, hidden till then.
    corpse: Option<Gd<Node3D>>,
    /// Its inner node alive, and as its corpse lies; its height; its
    /// node's place alive.
    from: Transform3D,
    to: Transform3D,
    height: f32,
    at: Vector3,
    /// The share of its idle its corpse is held at; its death clip.
    idle_at: f32,
    clip: Option<(String, f32)>,
    /// The dark it takes on as it falls; its meshes (they fade).
    shade: Gd<StandardMaterial3D>,
    meshes: Vec<Gd<GeometryInstance3D>>,
}

/// A fall that ended (self-tests): where, how, whether a corpse took over,
/// and how far the corpse stands from where the body fell (metres and
/// degrees of yaw).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fell {
    pub cell: (i32, i32),
    pub kind: FallKind,
    pub corpse: bool,
    pub off: (f32, f32),
}

impl MapView {
    /// A creature's model gone from its cell: kept till the messages are
    /// read (`claim_falls`), unless it was not seen there.
    pub(super) fn keep_gone(&mut self, gone: Gone) {
        if gone.model.is_pending() {
            self.art.give(gone.model);
            return;
        }
        self.gone.push(gone);
    }

    /// The deaths told lately (`deaths`): the creature of that name gone
    /// from its cell (the one a blow found, else the nearest the hero)
    /// falls; the bodies no death claims go back to the pool, and a death
    /// that found none in time is forgotten.
    pub(super) fn claim_falls(&mut self, catalog: &Catalog) {
        let now = self.clock;
        self.deaths.retain(|d| now - d.when <= TOLD_SECS);
        let mut claimed = Vec::new();
        for (d, told) in self.deaths.iter().enumerate() {
            if self.gone.is_empty() {
                break;
            }
            let (name, at) = (told.name.as_str(), told.at);
            let named = |g: &Gone| {
                usize::try_from(g.mon)
                    .ok()
                    .and_then(|i| catalog.monsters.get(i))
                    .is_some_and(|m| {
                        m.name == name
                            || m.male.as_deref() == Some(name)
                            || m.female.as_deref() == Some(name)
                    })
            };
            let hero = self.hero_at.unwrap_or((0, 0));
            let far = |c: (i32, i32)| (c.0 - hero.0).pow(2) + (c.1 - hero.1).pow(2);
            let pick = self
                .gone
                .iter()
                .enumerate()
                .filter(|(_, g)| named(g))
                .min_by_key(|(_, g)| (Some(g.cell) != at, far(g.cell)))
                .map(|(i, _)| i);
            if let Some(i) = pick {
                claimed.push((d, self.gone.swap_remove(i)));
            }
        }
        for (d, gone) in claimed.into_iter().rev() {
            let told = self.deaths.remove(d);
            let info = usize::try_from(gone.mon)
                .ok()
                .and_then(|i| catalog.monsters.get(i));
            match info {
                Some(_) if told.blink => self.fall_as(gone, FallKind::Blink),
                Some(info) => self.fall(gone, info),
                None => self.art.give(gone.model),
            }
        }
        for gone in std::mem::take(&mut self.gone) {
            self.art.give(gone.model);
        }
    }

    /// A body begins to fall where it stood (after the blow lands).
    fn fall(&mut self, gone: Gone, info: &MonsterInfo) {
        let clip = self.art.death(&gone.model).is_some();
        self.fall_as(gone, FallKind::of(info, clip));
    }

    /// A body begins to go `kind`'s way where it stood.
    fn fall_as(&mut self, gone: Gone, kind: FallKind) {
        let Gone {
            cell,
            model,
            placed,
            ..
        } = gone;
        let clip = self.art.death(&model);
        // what moved the body ends now: it falls where it stands
        self.finish_motions_where(|m| m.cell == cell);
        let inner = model
            .node
            .get_child(0)
            .and_then(|n| n.try_cast::<Node3D>().ok());
        if let Some(n) = inner.as_ref() {
            self.arrived(n);
        }
        let from = inner
            .as_ref()
            .map_or(Transform3D::IDENTITY, |n| n.get_transform());
        let (to, idle_at) = self.art.lying(&placed.look);
        // its corpse here, hidden till it takes over, where the body falls
        let corpse = self
            .cells
            .get(&cell)
            .filter(|_| kind != FallKind::Blink)
            .and_then(|n| {
                n.look
                    .models
                    .iter()
                    .zip(&n.models)
                    .find(|(p, _)| {
                        p.look.pose == Pose::Corpse && p.look.art.model == placed.look.art.model
                    })
                    .map(|(_, m)| m.node.clone())
            });
        let at = model.node.get_position();
        if let Some(mut c) = corpse.clone() {
            c.set_visible(false);
            c.set_transform(model.node.get_transform());
        }
        let secs = match (&clip, kind) {
            (Some((_, len)), FallKind::Clip) => len.min(CLIP_MOST),
            (_, FallKind::Slump) => SLUMP_SECS,
            (_, FallKind::Dissolve) => DISSOLVE_SECS,
            (_, FallKind::Burst) => 0.0,
            (_, FallKind::Blink) => BLINK_SECS,
            _ => TOPPLE_SECS,
        };
        if kind == FallKind::Blink {
            let middle = model.node.get_global_position()
                + Vector3::new(0.0, placed.look.art.height * 0.5, 0.0);
            self.vfx.burst(crate::vfx::VfxKind::Sparkle, middle);
        }
        let mut shade = StandardMaterial3D::new_gd();
        shade.set_shading_mode(ShadingMode::UNSHADED);
        shade.set_transparency(Transparency::ALPHA);
        shade.set_flag(Flags::DISABLE_FOG, true);
        shade.set_albedo(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        let fall = Fall {
            cell,
            model,
            kind,
            wait: if matches!(kind, FallKind::Burst | FallKind::Blink) {
                0.0
            } else {
                CONTACT_SECS
            },
            begun: false,
            t: 0.0,
            secs,
            sunk: 0.0,
            corpse,
            from,
            to,
            height: placed.look.art.height,
            at,
            idle_at,
            clip,
            shade,
            meshes: Vec::new(),
        };
        self.falls.push(fall);
        self.stats.falls += 1;
    }

    /// Every frame: the falls go on; those at their end hand over to their
    /// corpse, or sink away, and their bodies go back to the pool.
    pub(super) fn advance_falls(&mut self, delta: f32) {
        if self.falls.is_empty() {
            return;
        }
        let mut ended = Vec::new();
        for (i, f) in self.falls.iter_mut().enumerate() {
            if f.wait > 0.0 {
                f.wait -= delta;
                if f.wait > 0.0 {
                    continue;
                }
            }
            if !f.begun {
                f.begun = true;
                begin(f);
            }
            f.t += delta;
            let k = if f.secs > 0.0 {
                (f.t / f.secs).min(1.0)
            } else {
                1.0
            };
            fall_to(f, k);
            if k < 1.0 {
                continue;
            }
            let stays = matches!(f.kind, FallKind::Clip | FallKind::Topple | FallKind::Slump);
            if f.corpse.is_some() || !stays {
                ended.push(i);
                continue;
            }
            // no corpse: it sinks away
            f.sunk += delta;
            let s = (f.sunk / SINK_SECS).min(1.0);
            let mut node = f.model.node.clone();
            node.set_position(f.at - Vector3::new(0.0, SINK_DEPTH * f.height * s, 0.0));
            see_through(&mut f.meshes, s);
            if s >= 1.0 {
                ended.push(i);
            }
        }
        for i in ended.into_iter().rev() {
            let f = self.falls.swap_remove(i);
            self.end_fall(f);
        }
    }

    /// The falls on `pick`'s cells end at once (their cells change again,
    /// a new level).
    pub(super) fn finish_falls_where(&mut self, pick: impl Fn((i32, i32)) -> bool) {
        let (done, going): (Vec<_>, Vec<_>) = std::mem::take(&mut self.falls)
            .into_iter()
            .partition(|f| pick(f.cell));
        self.falls = going;
        for f in done {
            self.end_fall(f);
        }
    }

    /// A fall over: its corpse shown where the body lay (if the cell still
    /// has it), the body back to the pool as it was.
    fn end_fall(&mut self, f: Fall) {
        let Fall {
            cell,
            mut model,
            kind,
            corpse,
            meshes: f_meshes,
            ..
        } = f;
        let still = corpse.filter(|c| {
            c.is_instance_valid()
                && self
                    .cells
                    .get(&cell)
                    .is_some_and(|n| n.models.iter().any(|m| m.node == *c))
        });
        let off = match still.as_ref() {
            Some(c) => {
                let mut c = c.clone();
                c.set_visible(true);
                let (a, b) = (c.get_transform(), model.node.get_transform());
                let yaw = |t: Transform3D| t.basis.col_c().x.atan2(t.basis.col_c().z).to_degrees();
                (
                    (a.origin - b.origin).length(),
                    ((yaw(a) - yaw(b) + 540.0).rem_euclid(360.0) - 180.0).abs(),
                )
            }
            None => (0.0, 0.0),
        };
        self.fell = Some(Fell {
            cell,
            kind,
            corpse: still.is_some(),
            off,
        });
        set_overlay(&model.node, None);
        let mut meshes = f_meshes;
        see_through(&mut meshes, 0.0);
        if let Some(mut p) = model.player().cloned() {
            p.set_speed_scale(1.0);
        }
        model.node.set_visible(false);
        self.art.give(model);
    }

    /// The bodies falling now: where, how, how far into it (self-tests).
    pub fn falls(&self) -> Vec<((i32, i32), FallKind, f32)> {
        self.falls
            .iter()
            .map(|f| {
                (
                    f.cell,
                    f.kind,
                    if f.secs > 0.0 { f.t / f.secs } else { 1.0 },
                )
            })
            .collect()
    }

    /// The last fall that ended (self-tests).
    pub fn fell(&self) -> Option<Fell> {
        self.fell
    }
}

/// The name a death message uses, without its article ("the newt"); a
/// ghost by whose ghost it is ("Andries' ghost").
fn plain_name(name: &str) -> &str {
    let name = ["the ", "The ", "your ", "Your ", "poor "]
        .iter()
        .find_map(|a| name.strip_prefix(a))
        .unwrap_or(name);
    if name.ends_with("' ghost") || name.ends_with("'s ghost") {
        return "ghost";
    }
    name
}

/// The monster the hero slew, as a message names it without "the" (a
/// ghost by its name, a pet: "You destroy Andries' ghost!").
pub(super) fn slain(text: &str) -> Option<&str> {
    let rest = text
        .strip_prefix("You kill ")
        .or_else(|| text.strip_prefix("You destroy "))?;
    let name = rest.strip_suffix('!')?;
    (!name.is_empty() && !name.contains([',', '.', '!']) && name != "it").then_some(name)
}

/// The fall begins: its clip plays, or its idle stops where its corpse's
/// is held; its dark comes over it.
fn begin(f: &mut Fall) {
    f.meshes = f
        .model
        .node
        .find_children_ex("*")
        .type_("GeometryInstance3D")
        .owned(false)
        .done()
        .iter_shared()
        .filter_map(|n| n.try_cast::<GeometryInstance3D>().ok())
        .collect();
    if let Some(mut p) = f.model.player().cloned() {
        p.clear_queue();
        match (&f.clip, f.kind) {
            (Some((name, len)), FallKind::Clip) => {
                p.play_ex().name(name.as_str()).custom_blend(0.1).done();
                p.set_speed_scale((len / f.secs.max(0.01)).max(1.0));
            }
            _ => {
                let len = p.get_current_animation_length();
                p.seek_ex(len * f64::from(f.idle_at)).update(true).done();
                p.pause();
            }
        }
    }
    if !matches!(
        f.kind,
        FallKind::Burst | FallKind::Dissolve | FallKind::Blink
    ) {
        set_overlay(&f.model.node, Some(&f.shade.clone().upcast::<Material>()));
    }
}

/// The body `k` of the way through its fall.
fn fall_to(f: &mut Fall, k: f32) {
    let inner = f
        .model
        .node
        .get_child(0)
        .and_then(|n| n.try_cast::<Node3D>().ok());
    // as dark as its corpse at the end
    f.shade
        .set_albedo(Color::from_rgba(0.0, 0.0, 0.0, CORPSE_DARKEN * k));
    match f.kind {
        FallKind::Clip => {}
        FallKind::Topple => {
            if let Some(mut n) = inner {
                // falling faster as it goes, a little bounce as it lands
                let w = (k / 0.8).min(1.0);
                let mut t = f.from.interpolate_with(&f.to, w * w);
                if k > 0.8 {
                    let b = ((k - 0.8) / 0.2 * std::f32::consts::PI).sin();
                    t.origin.y += 0.04 * f.height * b;
                }
                n.set_transform(t);
            }
        }
        FallKind::Slump => {
            if let Some(mut n) = inner {
                // it sags; with a corpse to take over it rolls on into
                // the way that lies
                let sag = if f.corpse.is_some() { k / SAG_SHARE } else { k }.min(1.0);
                let w = sag * sag * (3.0 - 2.0 * sag);
                let (tall, wide) = (1.0 + (SLUMPED.0 - 1.0) * w, 1.0 + (SLUMPED.1 - 1.0) * w);
                let mut t = f.from;
                t.basis = t.basis * Basis::from_scale(Vector3::new(wide, tall, wide));
                if f.corpse.is_some() && k > SAG_SHARE {
                    let roll = (k - SAG_SHARE) / (1.0 - SAG_SHARE);
                    t = t.interpolate_with(&f.to, roll * roll * (3.0 - 2.0 * roll));
                }
                n.set_transform(t);
            }
        }
        FallKind::Dissolve => {
            let mut node = f.model.node.clone();
            node.set_position(f.at + Vector3::new(0.0, DISSOLVE_RISE * f.height * k, 0.0));
            see_through(&mut f.meshes, k);
        }
        FallKind::Burst => {
            let mut node = f.model.node.clone();
            node.set_visible(false);
        }
        FallKind::Blink => {
            if let Some(mut n) = inner {
                // squeezed thin and gone
                let mut t = f.from;
                let w = (1.0 - k * k).max(0.01);
                t.basis = t.basis * Basis::from_scale(Vector3::new(w, 1.0 + 0.4 * k, w));
                n.set_transform(t);
            }
        }
    }
}

/// Every mesh of a body this far see-through (0: as it is).
fn see_through(meshes: &mut [Gd<GeometryInstance3D>], k: f32) {
    for g in meshes.iter_mut().filter(|g| g.is_instance_valid()) {
        g.set_transparency(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, class: &str) -> MonsterInfo {
        MonsterInfo {
            idx: 0,
            name: name.into(),
            male: None,
            female: None,
            class: class.into(),
            class_name: String::new(),
            size: "small".into(),
            color: 0,
            light: 0,
            body: Vec::new(),
            tile_male: 0,
            tile_female: 0,
        }
    }

    #[test]
    fn each_body_falls_its_own_way() {
        assert_eq!(
            FallKind::of(&info("gas spore", "e"), false),
            FallKind::Burst
        );
        assert_eq!(FallKind::of(&info("ghost", " "), true), FallKind::Dissolve);
        assert_eq!(
            FallKind::of(&info("fog cloud", "v"), false),
            FallKind::Dissolve
        );
        assert_eq!(
            FallKind::of(&info("yellow light", "y"), false),
            FallKind::Dissolve
        );
        assert_eq!(
            FallKind::of(&info("air elemental", "E"), true),
            FallKind::Dissolve
        );
        assert_eq!(FallKind::of(&info("ogre", "O"), true), FallKind::Clip);
        assert_eq!(
            FallKind::of(&info("acid blob", "b"), false),
            FallKind::Slump
        );
        assert_eq!(
            FallKind::of(&info("brown pudding", "P"), false),
            FallKind::Slump
        );
        assert_eq!(FallKind::of(&info("lizard", ":"), false), FallKind::Topple);
        assert_eq!(FallKind::of(&info("newt", ":"), false), FallKind::Topple);
    }

    #[test]
    fn a_death_names_the_monster_without_its_article() {
        assert_eq!(plain_name("the newt"), "newt");
        assert_eq!(plain_name("Your little dog"), "little dog");
        assert_eq!(plain_name("newt"), "newt");
        assert_eq!(plain_name("Andries' ghost"), "ghost");
        assert_eq!(plain_name("Fred's ghost"), "ghost");
        assert_eq!(slain("You destroy Andries' ghost!"), Some("Andries' ghost"));
        assert_eq!(slain("You kill poor Fido!"), Some("poor Fido"));
        assert_eq!(slain("You kill it!"), None);
        assert_eq!(slain("You kill the newt!"), Some("the newt"));
    }
}
