//! What monsters do, shown as they do it. The messages tell it ("The
//! kobold shaman casts a spell at you!"), as they tell the hero's sounds,
//! and only of a monster the map shows as seen: a caster turns to its
//! target and casts (its clip, else a turn), a glow comes into its hand
//! as the spell leaves it, and the spell strikes where the line says it
//! went, the hero; one that cannot cast points and curses, its glow
//! fizzling; a fumbled spell crackles in the air around its caster. A
//! monster zapping a wand at the hero turns and zaps it, a flare at its
//! tip (the map draws the ray); one drinking a potion drinks, motes of
//! the potion's colour rising; one reading a scroll reads, runes rising
//! from the page; one wielding a weapon takes it in hand, a glint on it.
//! A monster's gaze flares in its eyes, turned on the hero; one spitting
//! venom lunges at them, the venom at its mouth (the map draws it
//! flying); one breathing opens its mouth on the hero (its attack clip),
//! its breath coming out of it as the ray goes. A monster seen to vanish
//! (teleported, turned invisible) blinks out where it stood, and one that
//! appears grows in where it came, in a sparkle; one that turns into
//! another bursts in smoke as the one shrinks away and the other grows;
//! one that summons help bursts where the new ones come.
//! Nothing waits for it: the next turn plays alongside.

use godot::classes::Skeleton3D;
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::World;

use super::MapView;
use crate::vfx::{Ray, VfxKind};

/// The clips monsters do things with (the hero's own, `held.uses`): a
/// spell cast, a wand zapped, a potion drunk, a scroll read, a weapon
/// taken in hand.
const CAST_CLIP: &str = "Spell_Simple_Shoot";
const ZAP_CLIP: &str = "Pistol_Shoot";
const DRINK_CLIP: &str = "ual2/Consume";
const HANDLE_CLIP: &str = "Interact";
/// The hands: a spell leaves the left, a potion is drunk from it; a wand,
/// a page, a weapon are in the right.
const OFF_HAND: &str = "hand_l";
const MAIN_HAND: &str = "hand_r";
/// The colour of a monster's magic, of a wand's flare, of a potion whose
/// colour the line does not tell.
const MAGIC: Color = Color::from_rgb(0.66, 0.5, 1.0);
const WAND: Color = Color::from_rgb(0.8, 0.85, 1.0);
const POTION: Color = Color::from_rgb(0.6, 0.75, 1.0);
/// Seconds into a zap its flare comes, into a drink its motes rise, into
/// a reading its runes, into taking a weapon its glint.
const FLARE_AT: f32 = 0.35;
const MOTES_AT: f32 = 0.8;
const RUNES_AT: f32 = 0.5;
const GLINT_AT: f32 = 0.4;
/// Seconds into a lunge or a breath what leaves the mouth comes.
const MOUTH_AT: f32 = 0.15;
/// Seconds into a cast its glow comes, and its spell strikes.
const GLOW_AT: f32 = 0.25;
const STRIKES_AT: f32 = 0.55;

/// Where a spell went, as its line tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aim {
    /// At the hero.
    Hero,
    /// Somewhere the hero is not: thin air, a spot near them, their
    /// displaced image.
    Astray,
    /// At nothing: a spell of the caster's own (haste, cure, summoning).
    Around,
}

/// What a monster does, as a message tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActKind {
    /// A spell cast, aimed so.
    Cast(Aim),
    /// A spell it could not cast: it points and curses.
    Curse(Aim),
    /// A spell fumbled: the air crackles around it.
    Crackle,
    /// A wand zapped: at the hero (or whoever the hero is not), or at
    /// itself.
    Zap { at_self: bool },
    /// A potion drunk.
    Drink,
    /// A scroll read.
    Read,
    /// A weapon taken in hand.
    Wield,
    /// A gaze turned on the hero.
    Gaze,
    /// Venom spat at the hero.
    Spit,
    /// A breath at the hero.
    Breath,
    /// Gone from sight (teleported away, invisible, left).
    Vanish,
    /// Come in sight (or back in it, `again`: teleported closer).
    Appear { again: bool },
    /// Turned into another monster (the line's `what`).
    Change,
    /// Help summoned: the new ones come round it.
    Summon,
}

/// What a message says a monster did: the monster by the name the line
/// uses, what, and the thing it did it with as the line names it.
pub fn act_of(text: &str) -> Option<(&str, ActKind, &str)> {
    if let Some(gaze) = gaze_of(text) {
        return Some(gaze);
    }
    if let Some(come) = comings_of(text) {
        return Some(come);
    }
    let rest = text.strip_prefix("The ");
    if let Some((name, what)) = rest.and_then(|r| r.split_once(" breathes ")) {
        return Some((name, ActKind::Breath, what.trim_end_matches('!')));
    }
    if let Some((name, _)) = rest.and_then(|r| r.split_once(" spits venom")) {
        return Some((name, ActKind::Spit, "venom"));
    }
    for (verb, kind) in [
        (" zaps itself with ", ActKind::Zap { at_self: true }),
        (" drinks ", ActKind::Drink),
        (" reads ", ActKind::Read),
        (" wields ", ActKind::Wield),
    ] {
        if let Some((name, what)) = rest.and_then(|r| r.split_once(verb)) {
            return Some((name, kind, what.trim_end_matches(['!', '.'])));
        }
    }
    // a wand at the hero (not the spell's ray "zaps you with")
    if let Some((name, what)) = rest.and_then(|r| r.split_once(" zaps "))
        && (what.starts_with("a ") || what.starts_with("an "))
    {
        let what = what.split(", which").next().unwrap_or(what);
        return Some((
            name,
            ActKind::Zap { at_self: false },
            what.trim_end_matches('!'),
        ));
    }
    spell_of(text).map(|(name, kind)| (name, kind, ""))
}

/// A line of a monster coming, going or changing: "The X vanishes!", "The
/// X vanishes and reappears next to you.", "The X suddenly disappears!",
/// "A X appears next to you.", "The X turns into a Y!", "The X summons
/// help!".
fn comings_of(text: &str) -> Option<(&str, ActKind, &str)> {
    let rest = text.strip_prefix("The ");
    if let Some(rest) = rest {
        if let Some((name, _)) = rest.split_once(" vanishes and reappears") {
            return Some((name, ActKind::Appear { again: true }, ""));
        }
        for tail in [
            " vanishes",
            " suddenly disappears!",
            " disappears!",
            " scowls at you menacingly, then vanishes.",
        ] {
            if let Some((name, _)) = rest.split_once(tail) {
                return Some((name, ActKind::Vanish, ""));
            }
        }
        if let Some((name, what)) = rest.split_once(" turns into ") {
            let what = what.trim_end_matches(['!', '.']);
            let what = what
                .strip_prefix("a ")
                .or_else(|| what.strip_prefix("an "))
                .unwrap_or(what);
            return Some((name, ActKind::Change, what));
        }
        if let Some((name, _)) = rest.split_once(" summons ") {
            return Some((name, ActKind::Summon, ""));
        }
    }
    // "The X appears!", "A X appears next to you." (made, or come back)
    let rest = rest
        .or_else(|| text.strip_prefix("A "))
        .or_else(|| text.strip_prefix("An "))?;
    let name = rest
        .split_once(" suddenly appears")
        .or_else(|| rest.split_once(" appears"))
        .map(|(n, _)| n)?;
    Some((name, ActKind::Appear { again: false }, ""))
}

/// A gaze's line: the gazer by the name the line uses, and what the gaze
/// does ("confuses", "frozen"...): "You are frozen by the floating eye's
/// gaze!", "The umber hulk's gaze confuses you!", "You meet Medusa's
/// gaze.", "The X stares piercingly at you!".
fn gaze_of(text: &str) -> Option<(&str, ActKind, &str)> {
    if let Some(rest) = text.strip_prefix("The ") {
        for (tail, what) in [
            (" stares piercingly at you!", "stuns"),
            (" attacks you with a fiery gaze!", "burns"),
        ] {
            if let Some(name) = rest.strip_suffix(tail) {
                return Some((name, ActKind::Gaze, what));
            }
        }
    }
    let (before, after) = text
        .split_once("'s gaze")
        .or_else(|| text.split_once("' gaze"))?;
    let what = match after.trim_start() {
        a if a.starts_with("confuses") => "confuses",
        a if a.starts_with("makes you very sleepy") => "sleeps",
        a if a.starts_with("is reflected") => "reflected",
        _ if before.starts_with("You meet ") => "stones",
        _ => "freezes",
    };
    let name = [
        "You are frozen by ",
        "You momentarily stiffen under ",
        "You meet ",
    ]
    .iter()
    .find_map(|p| before.strip_prefix(p))
    .unwrap_or(before);
    let name = name
        .strip_prefix("the ")
        .or_else(|| name.strip_prefix("The "))
        .unwrap_or(name);
    (!name.is_empty() && !name.contains(['!', '.'])).then_some((name, ActKind::Gaze, what))
}

/// The colour a gaze flares, by what it does.
fn gaze_color(what: &str) -> Color {
    match what {
        "confuses" => Color::from_rgb(0.75, 0.45, 1.0),
        "sleeps" => Color::from_rgb(0.5, 0.6, 1.0),
        "burns" => Color::from_rgb(1.0, 0.55, 0.2),
        "stones" => Color::from_rgb(0.45, 1.0, 0.55),
        "freezes" => Color::from_rgb(0.5, 0.95, 1.0),
        _ => Color::from_rgb(1.0, 1.0, 1.0),
    }
}

/// What comes out of a breather's mouth as its breath leaves it, by what
/// the line says it breathes.
fn breath_kind(what: &str) -> VfxKind {
    match what {
        "fire" => VfxKind::Fire,
        "frost" => VfxKind::Frost,
        "lightning" => VfxKind::Lightning,
        "poison gas" | "acid" => VfxKind::Poison,
        "sleep gas" => VfxKind::Shock(Color::from_rgb(0.7, 0.6, 1.0)),
        "a disintegration blast" => VfxKind::Explosion(Color::from_rgb(0.15, 0.1, 0.2)),
        _ => VfxKind::Shock(MAGIC),
    }
}

/// A spell's line: the caster by the name the line uses, and what.
fn spell_of(text: &str) -> Option<(&str, ActKind)> {
    if let Some(name) = text
        .strip_prefix("The air crackles around the ")
        .and_then(|r| r.strip_suffix('.'))
    {
        return Some((name, ActKind::Crackle));
    }
    let rest = text.strip_prefix("The ")?;
    if let Some((name, tail)) = rest.split_once(" casts a spell") {
        let aim = match tail {
            "!" => Aim::Around,
            " at you!" => Aim::Hero,
            _ => Aim::Astray,
        };
        return Some((name, ActKind::Cast(aim)));
    }
    if let Some((name, tail)) = rest.split_once(" points ") {
        let aim = match tail {
            "at you, then curses." => Aim::Hero,
            "all around, then curses." => Aim::Around,
            t if t.starts_with("and curses") => Aim::Astray,
            _ => return None,
        };
        return Some((name, ActKind::Curse(aim)));
    }
    // a spell's ray at the hero (the map draws the ray)
    if let Some((name, _)) = rest.split_once(" zaps you with a ") {
        return Some((name, ActKind::Cast(Aim::Hero)));
    }
    None
}

/// An act's effect waiting its moment: a glow in a hand (where the hand
/// is when it comes), a burst where something stands.
pub(super) struct ActFx {
    wait: f32,
    kind: VfxKind,
    at: FxAt,
    /// A thin ray of `kind`'s colour from where it is to here (a gaze).
    to: Option<Vector3>,
}

/// A ray the map draws, waiting a breather's mouth to open: its look, its
/// glyph's colour, and its piece (along the cell, or through its middle).
pub(super) struct WaitingRay {
    wait: f32,
    kind: Ray,
    color: Color,
    along: Option<(Vector3, Vector3)>,
    at: Vector3,
}

/// Seconds a breath's rays wait for the mouth to open.
const BREATH_WINDUP: f32 = 0.25;

enum FxAt {
    /// A bone of a model's skeleton; where its body's middle is, if it has
    /// none.
    Hand(Gd<Node3D>, &'static str, Vector3),
    /// Its head: the head's bone, else the top of its body to the front.
    Head(Gd<Node3D>, f32),
    /// Its eyes: before its head's bone, else the front of its body (an
    /// eye's pupil).
    Eyes(Gd<Node3D>, f32),
    Point(Vector3),
}

/// A body growing in where it came: its inner node, its own transform,
/// the transform last set (another's: the node was taken for something
/// else, and is let be), and how far it has grown (0..=1).
pub(super) struct Arrival {
    inner: Gd<Node3D>,
    base: Transform3D,
    last: Transform3D,
    t: f32,
}

/// Seconds a body takes to grow in; how far from a summoner the help it
/// summons comes, and how long after its line.
const ARRIVE_SECS: f32 = 0.3;
const SUMMONED_NEAR: i32 = 6;
const SUMMON_SECS: f64 = 1.5;

/// An act shown (self-tests): what, by whom, and whether its clip played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Acted {
    pub kind: ActKind,
    pub cell: (i32, i32),
    pub clip: bool,
}

impl MapView {
    /// An act a message tells, of the monster it names where the map shows
    /// it seen; none for one only sensed, unseen or not on the map.
    pub(super) fn watch_act(&mut self, text: &str, world: &World, catalog: &Catalog) {
        let Some((name, kind, what)) = act_of(text) else {
            return;
        };
        // one gone from its cell blinks out there, if the map showed it
        // (`fall`); the one it turned into, or it again, comes in
        let comes = match kind {
            ActKind::Vanish => {
                self.deaths.push(super::fall::Told::gone(name, self.clock));
                return;
            }
            ActKind::Appear { again: true } => {
                self.deaths.push(super::fall::Told::gone(name, self.clock));
                name
            }
            ActKind::Change => {
                self.deaths.push(super::fall::Told::gone(name, self.clock));
                what
            }
            _ => name,
        };
        let Some(cell) = seen_named(world, catalog, comes) else {
            return;
        };
        if kind == ActKind::Summon {
            // the help comes where the map draws it, now or a moment on
            self.summons.push((cell, self.clock));
            return;
        }
        if matches!(kind, ActKind::Appear { .. } | ActKind::Change) {
            self.arrive(cell, kind);
            self.acts_shown.push(Acted {
                kind,
                cell,
                clip: false,
            });
            return;
        }
        let hero = world.hero();
        let aim_at = match kind {
            ActKind::Cast(Aim::Hero) | ActKind::Curse(Aim::Hero) => hero,
            _ => None,
        };
        // it turns to the hero it casts at, points or zaps at; a potion, a
        // scroll, a weapon it handles where it stands
        let toward = aim_at.or(hero).filter(|&h| h != cell);
        let clip = match kind {
            ActKind::Crackle => false,
            ActKind::Cast(_) | ActKind::Curse(_) => {
                toward.is_some_and(|to| self.act_toward(cell, to, world, CAST_CLIP))
            }
            ActKind::Zap { at_self: false } => {
                toward.is_some_and(|to| self.act_toward(cell, to, world, ZAP_CLIP))
            }
            ActKind::Zap { at_self: true } => self.act_here(cell, ZAP_CLIP),
            ActKind::Drink => self.act_here(cell, DRINK_CLIP),
            ActKind::Read | ActKind::Wield => self.act_here(cell, HANDLE_CLIP),
            // the eyes turn on the hero
            ActKind::Gaze => {
                if let Some(to) = toward {
                    self.turn_to(cell, to, world);
                }
                false
            }
            // the mouth opens on the hero: its blow's clip, else a lunge
            ActKind::Spit | ActKind::Breath => toward.is_some_and(|to| {
                let has = self.act_clips(cell).is_some_and(|c| c.attack.is_some());
                self.strike(cell, to, world);
                has
            }),
            // shown above, as they come and go
            ActKind::Vanish | ActKind::Appear { .. } | ActKind::Change | ActKind::Summon => {
                return;
            }
        };
        let Some((node, middle)) = self.body_of(cell) else {
            return;
        };
        let height = self.height_of(cell);
        let eyes_node = node.clone();
        let head = FxAt::Head(node.clone(), height);
        let hand = FxAt::Hand(node.clone(), OFF_HAND, middle);
        let main = FxAt::Hand(node, MAIN_HAND, middle);
        match kind {
            ActKind::Cast(_) => {
                self.act_fx.push(ActFx {
                    wait: GLOW_AT,
                    kind: VfxKind::Cast(MAGIC),
                    at: hand,
                    to: None,
                });
                if let Some(h) = aim_at {
                    let at = self.sound_at(h);
                    self.act_fx.push(ActFx {
                        wait: STRIKES_AT,
                        kind: VfxKind::Shock(MAGIC),
                        at: FxAt::Point(at),
                        to: None,
                    });
                }
            }
            // the glow comes and dies at once
            ActKind::Curse(_) => self.act_fx.push(ActFx {
                wait: GLOW_AT,
                kind: VfxKind::Sparkle,
                at: hand,
                to: None,
            }),
            ActKind::Crackle => self.act_fx.push(ActFx {
                wait: 0.0,
                kind: VfxKind::Lightning,
                at: FxAt::Point(middle),
                to: None,
            }),
            ActKind::Zap { .. } => self.act_fx.push(ActFx {
                wait: FLARE_AT,
                kind: VfxKind::Zap(WAND),
                at: main,
                to: None,
            }),
            ActKind::Drink => {
                let color = self.potion_color(what);
                self.act_fx.push(ActFx {
                    wait: MOTES_AT,
                    kind: VfxKind::Quaff(color),
                    at: hand,
                    to: None,
                });
            }
            ActKind::Read => self.act_fx.push(ActFx {
                wait: RUNES_AT,
                kind: VfxKind::Read,
                at: main,
                to: None,
            }),
            ActKind::Wield => self.act_fx.push(ActFx {
                wait: GLINT_AT,
                kind: VfxKind::Sparkle,
                at: main,
                to: None,
            }),
            ActKind::Gaze => {
                // a bright pulse from the pupil, and a thin flash of the
                // gaze's colour from the eyes to the hero
                let color = gaze_color(what);
                let at_hero = hero.map(|h| self.sound_at(h));
                for (kind, to) in [
                    (VfxKind::Shock(color), None),
                    (VfxKind::Zap(color), at_hero),
                ] {
                    self.act_fx.push(ActFx {
                        wait: 0.05,
                        kind,
                        at: FxAt::Eyes(eyes_node.clone(), height),
                        to,
                    });
                }
            }
            ActKind::Spit => self.act_fx.push(ActFx {
                wait: MOUTH_AT,
                kind: VfxKind::Poison,
                at: head,
                to: None,
            }),
            ActKind::Breath => self.act_fx.push(ActFx {
                wait: MOUTH_AT,
                kind: breath_kind(what),
                at: head,
                to: None,
            }),
            ActKind::Vanish | ActKind::Appear { .. } | ActKind::Change | ActKind::Summon => {}
        }
        self.acts_shown.push(Acted { kind, cell, clip });
        if self.acts_shown.len() > 32 {
            self.acts_shown.remove(0);
        }
    }

    /// `cell`'s monster turns to `to` and plays `clip`; without that clip
    /// it only turns. Whether the clip played.
    fn act_toward(&mut self, cell: (i32, i32), to: (i32, i32), world: &World, clip: &str) -> bool {
        let has = self
            .cells
            .get(&cell)
            .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)))
            .and_then(|m| m.player())
            .is_some_and(|p| p.has_animation(clip));
        if has {
            self.strike_with(cell, to, world, Some(clip.to_string()));
            return true;
        }
        self.turn_to(cell, to, world);
        false
    }

    /// Monsters come in sight this frame near a summoner told lately: the
    /// help it summoned, each growing in where it came.
    pub(super) fn greet_summoned(&mut self) {
        let now = self.clock;
        self.summons.retain(|&(_, when)| now - when <= SUMMON_SECS);
        if self.summons.is_empty() || self.appeared.is_empty() {
            return;
        }
        let near = |c: (i32, i32), s: (i32, i32)| {
            (c.0 - s.0).abs().max((c.1 - s.1).abs()) <= SUMMONED_NEAR
        };
        let came: Vec<(i32, i32)> = self
            .appeared
            .iter()
            .copied()
            .filter(|&c| self.summons.iter().any(|&(s, _)| near(c, s)))
            .collect();
        for c in came {
            self.arrive(c, ActKind::Summon);
            self.acts_shown.push(Acted {
                kind: ActKind::Summon,
                cell: c,
                clip: false,
            });
        }
    }

    /// Something come in sight on `cell` grows in, in a sparkle (in smoke,
    /// turned into it; in a flash, summoned).
    fn arrive(&mut self, cell: (i32, i32), kind: ActKind) {
        let puff = match kind {
            ActKind::Change => VfxKind::Explosion(Color::from_rgb(0.55, 0.55, 0.6)),
            ActKind::Summon => VfxKind::Shock(MAGIC),
            _ => VfxKind::Sparkle,
        };
        let Some((node, middle)) = self.body_of(cell) else {
            return;
        };
        self.vfx.burst(puff, middle);
        let Some(mut inner) = node.get_child(0).and_then(|n| n.try_cast::<Node3D>().ok()) else {
            return;
        };
        // a body already growing in goes on from where it is
        if self.arrivals.iter().any(|a| a.inner == inner) {
            return;
        }
        let base = inner.get_transform();
        let mut last = base;
        last.basis = last.basis * Basis::from_scale(Vector3::splat(0.01));
        inner.set_transform(last);
        self.arrivals.push(Arrival {
            inner,
            base,
            last,
            t: 0.0,
        });
    }

    /// Every frame: what came grows in.
    pub(super) fn advance_arrivals(&mut self, delta: f32) {
        if self.arrivals.is_empty() {
            return;
        }
        self.arrivals.retain_mut(|a| {
            if !a.inner.is_instance_valid() || a.inner.get_transform() != a.last {
                return false;
            }
            a.t = (a.t + delta / ARRIVE_SECS).min(1.0);
            let k = a.t * a.t * (3.0 - 2.0 * a.t);
            a.last = a.base;
            a.last.basis = a.last.basis * Basis::from_scale(Vector3::splat(k.max(0.01)));
            a.inner.set_transform(a.last);
            a.t < 1.0
        });
    }

    /// A body growing in is whole at once (it falls, it blinks out).
    pub(super) fn arrived(&mut self, inner: &Gd<Node3D>) {
        if let Some(i) = self.arrivals.iter().position(|a| a.inner == *inner) {
            let mut a = self.arrivals.swap_remove(i);
            if a.inner.is_instance_valid() && a.inner.get_transform() == a.last {
                a.inner.set_transform(a.base);
            }
        }
    }

    /// `cell`'s monster's clips.
    fn act_clips(&self, cell: (i32, i32)) -> Option<crate::art::Clips> {
        let n = self.cells.get(&cell)?;
        let m = n.look.entity.and_then(|i| n.models.get(i))?;
        Some(self.art.clips(m, false))
    }

    /// The height of `cell`'s monster (metres).
    fn height_of(&self, cell: (i32, i32)) -> f32 {
        self.cells
            .get(&cell)
            .and_then(|n| n.look.entity.and_then(|i| n.look.models.get(i)))
            .map_or(1.0, |p| p.look.art.height)
    }

    /// `cell`'s monster turns to face `to`, nothing else.
    fn turn_to(&mut self, cell: (i32, i32), to: (i32, i32), world: &World) {
        let yaw = crate::animator::yaw_toward(cell, to);
        let ident = world
            .map
            .cell(cell.0, cell.1)
            .and_then(|c| c.glyph.as_ref())
            .and_then(nh_world::Ident::of);
        if let Some(id) = ident {
            self.facing.insert(cell, (id, yaw));
        }
        if let Some(n) = self.cells.get_mut(&cell)
            && let Some(i) = n.look.entity
            && let (Some(p), Some(m)) = (n.look.models.get_mut(i), n.models.get_mut(i))
        {
            p.yaw = yaw;
            let mut t = m.node.get_transform();
            t.basis = Basis::from_euler(EulerOrder::YXZ, Vector3::new(0.0, yaw.to_radians(), 0.0));
            m.node.set_transform(t);
        }
    }

    /// `cell`'s monster plays `clip` where it stands, then its idle again.
    /// Whether it has that clip.
    fn act_here(&mut self, cell: (i32, i32), clip: &str) -> bool {
        let Some(m) = self
            .cells
            .get(&cell)
            .and_then(|n| n.look.entity.and_then(|i| n.models.get(i)))
        else {
            return false;
        };
        let idle = self.art.clips(m, false).idle;
        let Some(mut p) = m.player().cloned().filter(|p| p.has_animation(clip)) else {
            return false;
        };
        p.play_ex().name(clip).custom_blend(0.15).done();
        if let Some(idle) = idle {
            p.queue(idle.as_str());
        }
        true
    }

    /// The colour of a potion as a line names it ("a bubbly blue potion"),
    /// else a potion's.
    fn potion_color(&self, what: &str) -> Color {
        let tile = nh_protocol::ObjectTile {
            tile: 0,
            class: "!".into(),
            class_name: String::new(),
            appearance: what.to_string(),
            color: None,
        };
        self.art
            .manifest()
            .appearance_color(&tile)
            .map_or(POTION, |c| Color::from_rgb(c[0], c[1], c[2]))
    }

    /// The model standing on `cell` and its body's middle.
    fn body_of(&self, cell: (i32, i32)) -> Option<(Gd<Node3D>, Vector3)> {
        let n = self.cells.get(&cell)?;
        let i = n.look.entity?;
        let (m, p) = n.models.get(i).zip(n.look.models.get(i))?;
        let middle = m.node.get_global_position() + Vector3::new(0.0, p.look.art.height * 0.6, 0.0);
        Some((m.node.clone(), middle))
    }

    /// Before the frame's cells are drawn: a breath its new lines tell, of a
    /// monster the map shows seen, holds the rays this frame draws till
    /// its mouth is open (`draw_ray`). Nothing waits for it.
    pub(super) fn breath_coming(&mut self, world: &World, catalog: &Catalog) {
        self.ray_wait = 0.0;
        let Some(seen) = self.log_seq.filter(|&s| s <= world.log.last_seq()) else {
            return;
        };
        let breath = world.log.since(seen).filter(|m| !m.from_history).any(|m| {
            act_of(&m.text).is_some_and(|(name, kind, _)| {
                kind == ActKind::Breath && seen_named(world, catalog, name).is_some()
            })
        });
        if breath {
            self.ray_wait = BREATH_WINDUP;
            self.stats.breaths_held += 1;
        }
    }

    /// A piece of a ray the map draws (`along` the cell, else through its
    /// middle `at`): now, or once a breather's mouth is open.
    pub(super) fn draw_ray(
        &mut self,
        kind: Ray,
        color: Color,
        along: Option<(Vector3, Vector3)>,
        at: Vector3,
    ) {
        if self.ray_wait > 0.0 {
            self.rays_waiting.push(WaitingRay {
                wait: self.ray_wait,
                kind,
                color,
                along,
                at,
            });
            return;
        }
        match along {
            Some((from, to)) => self.vfx.ray(kind, color, from, to),
            None => self.vfx.ray_through(kind, color, at),
        }
    }

    /// Every frame: the rays whose breather's mouth is open now.
    pub(super) fn advance_rays(&mut self, delta: f32) {
        if self.rays_waiting.is_empty() {
            return;
        }
        for r in &mut self.rays_waiting {
            r.wait -= delta;
        }
        let due: Vec<WaitingRay> = self
            .rays_waiting
            .extract_if(.., |r| r.wait <= 0.0)
            .collect();
        for r in due {
            match r.along {
                Some((from, to)) => self.vfx.ray(r.kind, r.color, from, to),
                None => self.vfx.ray_through(r.kind, r.color, r.at),
            }
        }
    }

    /// Every frame: the acts' effects whose moment came.
    pub(super) fn advance_acts(&mut self, delta: f32) {
        if self.act_fx.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.act_fx.len() {
            let fx = &mut self.act_fx[i];
            fx.wait -= delta;
            if fx.wait > 0.0 {
                i += 1;
                continue;
            }
            let fx = self.act_fx.swap_remove(i);
            let at = match &fx.at {
                FxAt::Point(p) => *p,
                FxAt::Hand(node, bone, middle) => bone_at(node, bone).unwrap_or(*middle),
                FxAt::Head(node, height) => head_at(node, *height),
                FxAt::Eyes(node, height) => eyes_at(node, *height),
            };
            match (fx.to, fx.kind) {
                (Some(to), VfxKind::Zap(c)) => self.vfx.beam_tinted(fx.kind, at, to, c),
                _ => self.vfx.burst(fx.kind, at),
            }
        }
    }

    /// The acts shown lately (self-tests).
    pub fn acts_shown(&self) -> &[Acted] {
        &self.acts_shown
    }

    /// The acts' effects still waiting their moment, and the bodies still
    /// growing in (self-tests).
    pub fn acts_pending(&self) -> usize {
        self.act_fx.len()
    }

    pub fn arrivals_pending(&self) -> usize {
        self.arrivals.len()
    }
}

/// Where a model's head is now: its head's bone, else the top of its body
/// a little to the front.
fn head_at(node: &Gd<Node3D>, height: f32) -> Vector3 {
    if let Some(at) = bone_at(node, "Head") {
        return at;
    }
    if !node.is_instance_valid() {
        return Vector3::ZERO;
    }
    let t = node.get_global_transform();
    t.origin
        + Vector3::new(0.0, height * 0.85, 0.0)
        + t.basis.col_c().normalized() * (height * 0.25)
}

/// Where a model's eyes are now: a little before its head's bone, else
/// the front of its body at its middle (an eye's pupil).
fn eyes_at(node: &Gd<Node3D>, height: f32) -> Vector3 {
    if !node.is_instance_valid() {
        return Vector3::ZERO;
    }
    let t = node.get_global_transform();
    let ahead = t.basis.col_c().normalized();
    match bone_at(node, "Head") {
        Some(head) => head + ahead * (height * 0.06),
        None => t.origin + Vector3::new(0.0, height * 0.55, 0.0) + ahead * (height * 0.45),
    }
}

/// Where a bone of a model's skeleton is now, in the world.
fn bone_at(node: &Gd<Node3D>, bone: &str) -> Option<Vector3> {
    if !node.is_instance_valid() || !node.is_inside_tree() {
        return None;
    }
    let sk = node
        .find_children_ex("*")
        .type_("Skeleton3D")
        .owned(false)
        .done()
        .iter_shared()
        .next()
        .and_then(|n| n.try_cast::<Skeleton3D>().ok())?;
    let i = sk.find_bone(bone);
    (i >= 0).then(|| sk.get_global_transform() * sk.get_bone_global_pose(i).origin)
}

/// The cell of the monster the line names, nearest the hero, among those
/// the map shows as seen (not only sensed, detected or seen invisible).
fn seen_named(world: &World, catalog: &Catalog, name: &str) -> Option<(i32, i32)> {
    let hero = world.hero().unwrap_or_default();
    let mut best: Option<((i32, i32), i32)> = None;
    for y in 0..nh_world::ROWNO {
        for x in 0..nh_world::COLNO {
            let Some(cell) = world.map.cell(x, y) else {
                continue;
            };
            let Some(g) = cell.glyph.as_ref() else {
                continue;
            };
            let seen = g.kind == GlyphKind::Mon
                && g.flags & (mg::HERO | mg::DETECT | mg::INVIS) == 0
                && !cell.sensed;
            if !seen {
                continue;
            }
            let named = g
                .mon
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| catalog.monsters.get(i))
                .is_some_and(|m| {
                    m.name == name
                        || m.male.as_deref() == Some(name)
                        || m.female.as_deref() == Some(name)
                });
            let d = (x - hero.0).abs().max((y - hero.1).abs());
            if named && best.is_none_or(|(_, b)| d < b) {
                best = Some(((x, y), d));
            }
        }
    }
    best.map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_tells_who_did_what() {
        assert_eq!(
            act_of("The kobold shaman casts a spell!"),
            Some(("kobold shaman", ActKind::Cast(Aim::Around), ""))
        );
        assert_eq!(
            act_of("The gnomish wizard casts a spell at you!"),
            Some(("gnomish wizard", ActKind::Cast(Aim::Hero), ""))
        );
        assert_eq!(
            act_of("The orc shaman casts a spell at thin air!"),
            Some(("orc shaman", ActKind::Cast(Aim::Astray), ""))
        );
        assert_eq!(
            act_of("The kobold shaman casts a spell at your displaced image!"),
            Some(("kobold shaman", ActKind::Cast(Aim::Astray), ""))
        );
        assert_eq!(
            act_of("The orc shaman points at you, then curses."),
            Some(("orc shaman", ActKind::Curse(Aim::Hero), ""))
        );
        assert_eq!(
            act_of("The orc shaman points all around, then curses."),
            Some(("orc shaman", ActKind::Curse(Aim::Around), ""))
        );
        assert_eq!(
            act_of("The orc shaman points and curses in your general direction."),
            Some(("orc shaman", ActKind::Curse(Aim::Astray), ""))
        );
        assert_eq!(
            act_of("The air crackles around the kobold shaman."),
            Some(("kobold shaman", ActKind::Crackle, ""))
        );
        assert_eq!(
            act_of("The Grey-elf zaps you with a blast of fire!"),
            Some(("Grey-elf", ActKind::Cast(Aim::Hero), ""))
        );
        assert_eq!(
            act_of("The gnome lord zaps a wand of striking!"),
            Some((
                "gnome lord",
                ActKind::Zap { at_self: false },
                "a wand of striking"
            ))
        );
        assert_eq!(
            act_of("The gnome lord zaps an oak wand, which suddenly explodes!"),
            Some(("gnome lord", ActKind::Zap { at_self: false }, "an oak wand"))
        );
        assert_eq!(
            act_of("The dwarf zaps itself with a wand of digging!"),
            Some(("dwarf", ActKind::Zap { at_self: true }, "a wand of digging"))
        );
        assert_eq!(
            act_of("The hill orc drinks a bubbly blue potion!"),
            Some(("hill orc", ActKind::Drink, "a bubbly blue potion"))
        );
        assert_eq!(
            act_of("The gnome reads a scroll labeled FOOBIE BLETCH!"),
            Some(("gnome", ActKind::Read, "a scroll labeled FOOBIE BLETCH"))
        );
        assert_eq!(
            act_of("The hill orc wields a crossbow."),
            Some(("hill orc", ActKind::Wield, "a crossbow"))
        );
        assert_eq!(
            act_of("You are frozen by the floating eye's gaze!"),
            Some(("floating eye", ActKind::Gaze, "freezes"))
        );
        assert_eq!(
            act_of("You momentarily stiffen under the floating eye's gaze!"),
            Some(("floating eye", ActKind::Gaze, "freezes"))
        );
        assert_eq!(
            act_of("The umber hulk's gaze confuses you!"),
            Some(("umber hulk", ActKind::Gaze, "confuses"))
        );
        assert_eq!(
            act_of("You meet Medusa's gaze."),
            Some(("Medusa", ActKind::Gaze, "stones"))
        );
        assert_eq!(
            act_of("The archon stares piercingly at you!"),
            Some(("archon", ActKind::Gaze, "stuns"))
        );
        assert_eq!(
            act_of("The red dragon breathes fire!"),
            Some(("red dragon", ActKind::Breath, "fire"))
        );
        assert_eq!(
            act_of("The black dragon breathes a disintegration blast!"),
            Some(("black dragon", ActKind::Breath, "a disintegration blast"))
        );
        assert_eq!(
            act_of("The cobra spits venom!"),
            Some(("cobra", ActKind::Spit, "venom"))
        );
        assert_eq!(
            act_of("The little dog spits the tripe out in disgust!"),
            None
        );
        assert_eq!(
            act_of("The gnome lord vanishes!"),
            Some(("gnome lord", ActKind::Vanish, ""))
        );
        assert_eq!(
            act_of("The gnome lord suddenly disappears!"),
            Some(("gnome lord", ActKind::Vanish, ""))
        );
        assert_eq!(
            act_of("The gnome lord vanishes and reappears next to you."),
            Some(("gnome lord", ActKind::Appear { again: true }, ""))
        );
        assert_eq!(
            act_of("A gnome lord appears next to you."),
            Some(("gnome lord", ActKind::Appear { again: false }, ""))
        );
        assert_eq!(
            act_of("The gnome lord suddenly appears!"),
            Some(("gnome lord", ActKind::Appear { again: false }, ""))
        );
        assert_eq!(
            act_of("The chameleon turns into a jackal!"),
            Some(("chameleon", ActKind::Change, "jackal"))
        );
        assert_eq!(
            act_of("The werejackal summons help!"),
            Some(("werejackal", ActKind::Summon, ""))
        );
        assert_eq!(act_of("Something casts a spell at you!"), None);
        assert_eq!(act_of("You hear a mumbled curse."), None);
        assert_eq!(act_of("The newt bites!"), None);
    }
}
