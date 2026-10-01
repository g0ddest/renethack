//! The hero's own look on the map: the gear the pack says they carry (read
//! again only when the pack changes), what their lamp does to the light
//! over their head, and the effects of the items they use.

use godot::classes::{MeshInstance3D, SphereMesh};
use godot::prelude::*;
use nh_art::{ArtManifest, Gear};
use nh_protocol::{Catalog, InvItem, ObjectTile};
use nh_world::{ItemUse, Pack, UseKind};

use crate::vfx::{Vfx, VfxKind};

/// The hero's gear, as of the last pack it was read from.
#[derive(Default)]
pub struct HeroGear {
    items: Vec<InvItem>,
    twoweap: bool,
    read: bool,
    gear: Gear,
    /// Whether the light over the hero is the lamp's helper now.
    lamp: bool,
}

impl HeroGear {
    /// The gear of `pack`: read again only when it changed.
    pub fn update(&mut self, pack: &Pack, catalog: &Catalog, art: &ArtManifest) -> &Gear {
        if !self.read || pack.items() != self.items.as_slice() || pack.twoweap() != self.twoweap {
            self.items = pack.items().to_vec();
            self.twoweap = pack.twoweap();
            self.gear = art.gear(pack, catalog);
            self.read = true;
        }
        &self.gear
    }

    pub fn gear(&self) -> &Gear {
        &self.gear
    }

    /// A lit lamp in hand (or not) from now on: true when that changed.
    pub fn set_lamp(&mut self, lit: bool) -> bool {
        std::mem::replace(&mut self.lamp, lit) != lit
    }

    /// Read the pack again next time (a new game).
    pub fn reset(&mut self) {
        *self = HeroGear::default();
    }
}

/// What using an item shows around the hero (spec §3.9), timed to its
/// clip: a drink's motes in the potion's colour, runes rising off a page,
/// a wand's flare and beam, a spell's swirl, the thrown thing flying off,
/// dust at a kick. Bursts and beams are the map's effects (`Vfx`); the
/// flight is drawn here. The colour of an effect never tells more than the
/// appearance (a ruby potion's motes are red; a wand's beam is its wood or
/// metal's colour, else arcane violet).
pub struct HeroFx {
    root: Gd<Node3D>,
    pending: Vec<(f32, Fx)>,
    flights: Vec<Flight>,
    /// Effects started since the start, by name (self-tests).
    started: Vec<&'static str>,
}

/// An effect, as plain data.
#[derive(Debug, Clone)]
pub enum Fx {
    /// `at`, or where `anchor` is when it starts (a hand, as it moves).
    Burst {
        name: &'static str,
        kind: VfxKind,
        at: Vector3,
        anchor: Option<Gd<Node3D>>,
    },
    Beam {
        kind: VfxKind,
        from: Vector3,
        to: Vector3,
        anchor: Option<Gd<Node3D>>,
    },
    /// A thrown thing's model flying from one point to another.
    Throw {
        model: Option<Gd<Node3D>>,
        from: Vector3,
        to: Vector3,
    },
}

struct Flight {
    node: Gd<Node3D>,
    age: f32,
    life: f32,
    from: Vector3,
    to: Vector3,
}

/// Seconds a thrown thing flies per metre, and how high it arcs.
const FLIGHT_SECS: f32 = 0.07;
const FLIGHT_ARC: f32 = 0.35;

impl HeroFx {
    pub fn new(root: Gd<Node3D>) -> HeroFx {
        HeroFx {
            root,
            pending: Vec::new(),
            flights: Vec::new(),
            started: Vec::new(),
        }
    }

    /// Start `fx` in `delay` seconds.
    pub fn after(&mut self, delay: f32, fx: Fx) {
        self.pending.push((delay, fx));
    }

    /// Effects started so far, by name (self-tests).
    pub fn started(&self) -> &[&'static str] {
        &self.started
    }

    /// Effects still to come, or a thing in flight.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.flights.is_empty()
    }

    /// End everything at once (a new level, a new game).
    pub fn clear(&mut self) {
        for (_, fx) in self.pending.drain(..) {
            if let Fx::Throw {
                model: Some(mut m), ..
            } = fx
            {
                m.queue_free();
            }
        }
        for mut f in self.flights.drain(..) {
            if f.node.is_instance_valid() {
                f.node.queue_free();
            }
        }
    }

    pub fn advance(&mut self, delta: f32, vfx: &mut Vfx) {
        let mut due = Vec::new();
        for (left, fx) in std::mem::take(&mut self.pending) {
            if left - delta <= 0.0 {
                due.push(fx);
            } else {
                self.pending.push((left - delta, fx));
            }
        }
        for fx in due {
            self.spawn(fx, vfx);
        }
        let mut i = 0;
        while i < self.flights.len() {
            let f = &mut self.flights[i];
            f.age += delta;
            let t = (f.age / f.life).clamp(0.0, 1.0);
            if f.node.is_instance_valid() {
                let arc = FLIGHT_ARC * (std::f32::consts::PI * t).sin();
                f.node
                    .set_position(f.from.lerp(f.to, t) + Vector3::new(0.0, arc, 0.0));
                f.node.rotate_object_local(Vector3::RIGHT, delta * 14.0);
            }
            if f.age >= f.life {
                let mut f = self.flights.swap_remove(i);
                if f.node.is_instance_valid() {
                    f.node.queue_free();
                }
            } else {
                i += 1;
            }
        }
    }

    fn spawn(&mut self, fx: Fx, vfx: &mut Vfx) {
        match fx {
            Fx::Burst {
                name,
                kind,
                at,
                anchor,
            } => {
                self.started.push(name);
                vfx.burst(kind, where_now(anchor.as_ref(), at));
            }
            Fx::Beam {
                kind,
                from,
                to,
                anchor,
            } => {
                self.started.push("beam");
                vfx.beam(kind, where_now(anchor.as_ref(), from), to);
            }
            Fx::Throw { model, from, to } => {
                self.started.push("throw");
                let mut node = Node3D::new_alloc();
                node.set_name("Fx_throw");
                node.set_position(from);
                match model {
                    Some(m) => node.add_child(&m),
                    None => {
                        let mut mi = MeshInstance3D::new_alloc();
                        let mut ball = SphereMesh::new_gd();
                        ball.set_radius(0.04);
                        ball.set_height(0.08);
                        mi.set_mesh(&ball);
                        node.add_child(&mi);
                    }
                }
                self.root.add_child(&node);
                self.flights.push(Flight {
                    node,
                    age: 0.0,
                    life: FLIGHT_SECS * from.distance_to(to).max(1.0) + 0.1,
                    from,
                    to,
                });
            }
        }
    }
}

/// Where an anchor is now, else `at`.
fn where_now(anchor: Option<&Gd<Node3D>>, at: Vector3) -> Vector3 {
    anchor
        .filter(|a| a.is_instance_valid() && a.is_inside_tree())
        .map_or(at, |a| a.get_global_position())
}

/// The hand a use's clip moves: drinking, eating and applying bring the
/// left hand up (Consume, Interact), a spell leaves the left hand; a wand,
/// a page and a throw are in the right.
pub fn use_hand(kind: UseKind) -> &'static str {
    match kind {
        UseKind::Quaff | UseKind::Eat | UseKind::Apply | UseKind::Cast => "hand_l",
        _ => "hand_r",
    }
}

/// The colour of a use's effect: the appearance's colour, else the kind's.
pub fn use_color(u: &ItemUse, tile: Option<&ObjectTile>, art: &ArtManifest) -> Color {
    let own = tile.and_then(|t| art.appearance_color(t));
    let fallback = match u.kind {
        UseKind::Read => [1.0, 0.82, 0.4],
        UseKind::Cast => [0.5, 0.7, 1.0],
        UseKind::Eat => [0.8, 0.62, 0.4],
        UseKind::Kick => [0.62, 0.56, 0.5],
        UseKind::Apply => [1.0, 0.86, 0.66],
        _ => [0.66, 0.5, 1.0],
    };
    let c = match (u.kind, own) {
        // a page's runes are gold whatever the cover
        (UseKind::Read | UseKind::Cast | UseKind::Kick | UseKind::Apply, _) | (_, None) => fallback,
        (_, Some(c)) => c,
    };
    Color::from_rgb(c[0], c[1], c[2])
}

/// The name of a use in the manifest's `held.uses`.
pub fn use_name(kind: UseKind) -> &'static str {
    match kind {
        UseKind::Quaff => "quaff",
        UseKind::Read => "read",
        UseKind::Zap => "zap",
        UseKind::Cast => "cast",
        UseKind::Eat => "eat",
        UseKind::Apply => "apply",
        UseKind::Throw => "throw",
        UseKind::Fire => "fire",
        UseKind::Wear => "wear",
        UseKind::PutOn => "put_on",
        UseKind::TakeOff => "take_off",
        UseKind::Remove => "remove",
        UseKind::Wield => "wield",
        UseKind::PickUp => "pick_up",
        UseKind::Kick => "kick",
    }
}

/// Whether the item is shown in the hand while it is used, and for how
/// long of the clip (throws let go early).
pub fn in_hand(kind: UseKind) -> Option<f32> {
    match kind {
        UseKind::Quaff | UseKind::Eat | UseKind::Read | UseKind::Zap | UseKind::Apply => Some(1.6),
        UseKind::Throw | UseKind::Fire => Some(THROW_AT),
        _ => None,
    }
}

/// When a throw lets go (seconds into the clip).
pub const THROW_AT: f32 = crate::art::THROW_LETS_GO;

/// The effects of a use, after their delays: `hand` is where the item
/// is (`anchor` its bone, followed as it moves), `ahead` the far end of
/// its way (a beam's, a flight's), where a beam ends in a flash.
pub fn use_effects(
    u: &ItemUse,
    color: Color,
    hand: Vector3,
    anchor: Option<Gd<Node3D>>,
    ahead: Option<Vector3>,
) -> Vec<(f32, Fx)> {
    let burst = |name, kind, at: Vector3| Fx::Burst {
        name,
        kind,
        at,
        anchor: anchor.clone(),
    };
    let beam = |kind, impact| {
        ahead
            .map(|to| {
                vec![
                    (
                        0.4,
                        Fx::Beam {
                            kind,
                            from: hand,
                            to,
                            anchor: anchor.clone(),
                        },
                    ),
                    (
                        0.55,
                        Fx::Burst {
                            name: "impact",
                            kind: impact,
                            at: to,
                            anchor: None,
                        },
                    ),
                ]
            })
            .unwrap_or_default()
    };
    match u.kind {
        UseKind::Quaff => vec![(0.8, burst("quaff", VfxKind::Quaff(color), hand))],
        UseKind::Eat => vec![(0.6, burst("eat", VfxKind::Quaff(color), hand))],
        UseKind::Read => vec![(0.5, burst("read", VfxKind::Read, hand))],
        UseKind::Zap => {
            let mut v = vec![(0.35, burst("zap", VfxKind::Zap(color), hand))];
            v.extend(beam(VfxKind::Zap(color), VfxKind::Explosion(color)));
            v
        }
        UseKind::Cast => {
            let mut v = vec![(0.3, burst("cast", VfxKind::Cast(color), hand))];
            v.extend(beam(VfxKind::Cast(color), VfxKind::Explosion(color)));
            v
        }
        UseKind::Apply => vec![(0.4, burst("apply", VfxKind::Sparkle, hand))],
        UseKind::Kick => vec![(
            0.3,
            Fx::Burst {
                name: "kick",
                kind: VfxKind::Sparks,
                at: ahead.unwrap_or(hand),
                anchor: None,
            },
        )],
        _ => Vec::new(),
    }
}
