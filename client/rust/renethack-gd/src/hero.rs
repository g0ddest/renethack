//! The hero's own look on the map: the gear the pack says they carry (read
//! again only when the pack changes), what their lamp does to the light
//! over their head, and the effects of the items they use.

use godot::classes::{MeshInstance3D, SphereMesh};
use godot::prelude::*;
use nh_art::{ArtManifest, Gear};
use nh_protocol::{Catalog, InvItem, ObjectTile, Slot};
use nh_world::{ItemUse, Pack, UseKind};

use crate::vfx::{Vfx, VfxKind, beam_color};

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

    /// A lit lamp in hand.
    pub fn lamp(&self) -> bool {
        self.lamp
    }

    /// Read the pack again next time (a new game).
    pub fn reset(&mut self) {
        *self = HeroGear::default();
    }
}

/// What using an item shows around the hero (spec §3.9), timed to its
/// clip: a drink's motes in the potion's colour and its glow rising round
/// the body, runes rising off a page and round the reader, a wand's flare
/// and beam (a flash where it strikes stone), a spell's swirl, the thrown
/// thing flying off, dust at a kick. Bursts and beams are the map's effects (`Vfx`); the
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
    /// An effect on the hero's body (`Vfx::on_body`): at their feet,
    /// where `anchor` is when it starts (they may have moved), `tall`
    /// metres tall.
    Body {
        name: &'static str,
        kind: VfxKind,
        feet: Vector3,
        tall: f32,
        anchor: Option<Gd<Node3D>>,
    },
    /// A ray, and the flash where it stops (at the first wall on its way):
    /// a strike on stone at arm's length is that flash alone.
    Beam {
        kind: VfxKind,
        from: Vector3,
        to: Vector3,
        anchor: Option<Gd<Node3D>>,
        impact: Option<VfxKind>,
    },
    /// A thrown thing's model flying from one point to another: from
    /// where `anchor` is when it starts (the hand that lets go), else
    /// `from`.
    Throw {
        model: Option<Gd<Node3D>>,
        from: Vector3,
        to: Vector3,
        anchor: Option<Gd<Node3D>>,
    },
}

struct Flight {
    node: Gd<Node3D>,
    age: f32,
    life: f32,
    from: Vector3,
    to: Vector3,
}

/// A beam that strikes stone nearer than this (metres from where it
/// leaves the hand) is only its flash there.
const POINT_BLANK: f32 = 1.0;
/// How far past its way's end a beam looks for what it strikes (metres).
const STRIKE_PAST: f32 = 0.3;
/// When a drink's glow rises round the drinker, and a page's runes round
/// the reader (seconds into the use).
const DRUNK_AT: f32 = 1.0;
const READ_AT: f32 = 0.65;

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

    /// The things in flight: (its node, where it set off) (self-tests).
    pub fn flights(&self) -> Vec<(Gd<Node3D>, Vector3)> {
        let flying = self.flights.iter().filter(|f| f.node.is_instance_valid());
        flying.map(|f| (f.node.clone(), f.from)).collect()
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
                // a scroll of fire burst into flames as it was read (the
                // engine showed them before the reading is shown): no
                // runes rise after its fire
                if kind == VfxKind::Read && vfx.fire_lately() {
                    return;
                }
                vfx.burst(kind, where_now(anchor.as_ref(), at));
            }
            Fx::Body {
                name,
                kind,
                feet,
                tall,
                anchor,
            } => {
                self.started.push(name);
                vfx.on_body(kind, where_now(anchor.as_ref(), feet), tall);
            }
            Fx::Beam {
                kind,
                from,
                to,
                anchor,
                impact,
            } => {
                self.started.push("beam");
                // a ray the map has just drawn cell by cell (as what it
                // is, bouncing where it bounced) was this use's beam: it
                // is not drawn again, nor its end
                if vfx.ray_lately() {
                    return;
                }
                // its way (as it was aimed: the hand may have reached past
                // the end of it at arm's length from a wall) ends at the
                // last open cell's far edge; what is a little further on
                // is what it strikes
                let way = (to - from).normalized();
                let past = to + way * STRIKE_PAST;
                let at = vfx.clip(from, past);
                let struck = at.distance_to(past) > 0.05;
                let at = if struck { at } else { to };
                let from = where_now(anchor.as_ref(), from);
                // stone at arm's length: no room for rings to spread, it
                // flashes where it strikes
                if struck && (at - from).dot(way) < POINT_BLANK {
                    match kind {
                        VfxKind::Shock(c) => vfx.burst(VfxKind::Force(c, true), at - way * 0.2),
                        _ => vfx.impact(beam_color(kind), at, at - way),
                    }
                    return;
                }
                vfx.beam(kind, from, to);
                if let Some(kind) = impact {
                    let kind = match kind {
                        VfxKind::Zap(c) if struck => VfxKind::Impact(c),
                        VfxKind::Force(c, _) => VfxKind::Force(c, struck),
                        k => k,
                    };
                    self.after(
                        0.15,
                        Fx::Burst {
                            name: "impact",
                            kind,
                            at,
                            anchor: None,
                        },
                    );
                }
            }
            Fx::Throw {
                model,
                from,
                to,
                anchor,
            } => {
                self.started.push("throw");
                let from = where_now(anchor.as_ref(), from);
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

/// The hero's body for an effect on it: where they stand, how tall they
/// are, and their model's node (followed as they move).
#[derive(Debug, Clone)]
pub struct Body {
    pub feet: Vector3,
    pub tall: f32,
    pub node: Option<Gd<Node3D>>,
}

/// Healing told by a line of the engine's: a potion's, a spell's, a
/// prayer's.
pub fn healed(text: &str) -> bool {
    matches!(
        text,
        "You feel better." | "You feel much better." | "You feel completely healed."
    )
}

/// A lit field round the hero (a scroll's or a spell's of light): said
/// only when the hero sees it (blind, NetHack says nothing).
pub fn lit_field(text: &str) -> bool {
    text == "A lit field surrounds you!"
}

/// A curse lifted from what the hero has (a scroll of remove curse's
/// line); felt even blind, so the map shows it only to a hero who sees.
pub fn helped(text: &str) -> bool {
    text == "You feel like someone is helping you."
}

/// What of the body an item glowing is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowOn {
    Head,
    Body,
    Hands,
    Feet,
    Shield,
    Weapon,
}

/// An item of the hero's glowing (a scroll of enchant armour's or enchant
/// weapon's line, "Your leather armor glows silver for a moment."): where
/// on the body, found by the worn or wielded item the line names, and the
/// glow's colour as the line says it. Blind, the line says the item
/// vibrates or feels warm: nothing glows.
pub fn glowing(text: &str, pack: &[InvItem]) -> Option<(GlowOn, Color)> {
    if text == "Your skin glows then fades." {
        return Some((GlowOn::Body, glow_color("silver")));
    }
    let rest = text.strip_prefix("Your ")?;
    let at = rest.find(" glow")?;
    let (name, after) = rest.split_at(at);
    let name = name
        .strip_suffix(" violently")
        .or_else(|| name.strip_suffix(" faintly"))
        .unwrap_or(name);
    // "glows silver for a moment", "glow blue for a while"
    let word = after
        .split_whitespace()
        .nth(1)
        .filter(|w| *w != "for" && *w != "then")
        .unwrap_or("white");
    if !after.contains(" for a ") {
        return None;
    }
    let on = pack
        .iter()
        .filter(|i| !i.slots.is_empty() && i.text.contains(name))
        .find_map(|i| i.slots.iter().find_map(glow_on))
        .or_else(|| {
            // a weapon named its own way: the one in hand
            pack.iter()
                .any(|i| i.slots.contains(&Slot::Weapon) && name.contains(i.text.as_str()))
                .then_some(GlowOn::Weapon)
        })
        .unwrap_or(GlowOn::Body);
    Some((on, glow_color(word.trim_end_matches(['.', ',']))))
}

fn glow_on(slot: &Slot) -> Option<GlowOn> {
    Some(match slot {
        Slot::Helmet => GlowOn::Head,
        Slot::Body | Slot::Cloak | Slot::Shirt => GlowOn::Body,
        Slot::Gloves => GlowOn::Hands,
        Slot::Boots => GlowOn::Feet,
        Slot::Shield => GlowOn::Shield,
        Slot::Weapon => GlowOn::Weapon,
        _ => return None,
    })
}

/// A glow's colour by NetHack's word for it (hcolor's; a hallucinating
/// hero's own words come out pale).
fn glow_color(word: &str) -> Color {
    let rgb = Color::from_rgb;
    match word {
        "silver" | "silvery" => rgb(0.85, 0.9, 1.0),
        "blue" => rgb(0.4, 0.62, 1.0),
        "black" => rgb(0.45, 0.25, 0.6),
        "gold" | "golden" | "amber" => rgb(1.0, 0.78, 0.35),
        "red" => rgb(1.0, 0.35, 0.3),
        "green" => rgb(0.45, 1.0, 0.45),
        "white" => rgb(1.0, 1.0, 1.0),
        _ => rgb(0.9, 0.9, 1.0),
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
        UseKind::Pray => "pray",
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
    body: Body,
) -> Vec<(f32, Fx)> {
    let burst = |name, kind, at: Vector3| Fx::Burst {
        name,
        kind,
        at,
        anchor: anchor.clone(),
    };
    let on_body = |name, kind| Fx::Body {
        name,
        kind,
        feet: body.feet,
        tall: body.tall,
        anchor: body.node.clone(),
    };
    let beam = |kind, impact| {
        ahead.map(|to| {
            (
                0.4,
                Fx::Beam {
                    kind,
                    from: hand,
                    to,
                    anchor: anchor.clone(),
                    impact: Some(impact),
                },
            )
        })
    };
    match u.kind {
        UseKind::Quaff => vec![
            (0.8, burst("quaff", VfxKind::Quaff(color), hand)),
            (DRUNK_AT, on_body("drunk", VfxKind::Aura(color))),
        ],
        UseKind::Eat => vec![(0.6, burst("eat", VfxKind::Quaff(color), hand))],
        UseKind::Read => vec![
            (0.5, burst("read", VfxKind::Read, hand)),
            (READ_AT, on_body("lore", VfxKind::Halo)),
        ],
        // a wand or a spell whose way the engine draws no ray along
        // (striking, force bolt, the rest that strike what is there) sends
        // rings of force; a ray's own look is the map's (`Fx::Beam`)
        // what it strikes takes the force: dust thrown up, and chips of
        // stone flung where it is a wall or a door
        UseKind::Zap => {
            let mut v = vec![(0.35, burst("zap", VfxKind::Zap(color), hand))];
            v.extend(beam(VfxKind::Shock(color), VfxKind::Force(color, false)));
            v
        }
        UseKind::Cast => {
            let mut v = vec![(0.3, burst("cast", VfxKind::Cast(color), hand))];
            v.extend(beam(VfxKind::Shock(color), VfxKind::Force(color, false)));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Light, an item's glow and a curse lifted are told by NetHack's own
    /// lines, and only the ones a seeing hero gets (blind, an item
    /// vibrates, feels warm: nothing glows); the glow is where the item
    /// the line names is worn or held.
    #[test]
    fn magic_is_shown_as_its_lines_tell_it() {
        assert!(lit_field("A lit field surrounds you!"));
        assert!(!lit_field("A lit field briefly surrounds you!"));
        assert!(helped("You feel like someone is helping you."));
        let item = |text: &str, slot: &str| InvItem {
            letter: 'a',
            class: '[',
            tile: 0,
            quan: 1,
            slots: vec![Slot::from(slot.to_string())],
            lit: false,
            text: text.into(),
        };
        let pack = [
            item("a +1 long sword (weapon in hand)", "weapon"),
            item("a +3 small shield (being worn)", "shield"),
            item("a +0 pair of leather gloves (being worn)", "gloves"),
            item("an uncursed +0 elven leather helm (being worn)", "helmet"),
        ];
        let on = |line: &str| glowing(line, &pack).map(|(on, _)| on);
        assert_eq!(
            on("Your small shield glows silver for a moment."),
            Some(GlowOn::Shield)
        );
        assert_eq!(
            on("Your long sword glows blue for a while."),
            Some(GlowOn::Weapon)
        );
        assert_eq!(
            on("Your leather gloves glow silver for a moment."),
            Some(GlowOn::Hands)
        );
        assert_eq!(
            on("Your elven leather helm violently glows silver for a while, then evaporates."),
            Some(GlowOn::Head)
        );
        assert_eq!(on("Your skin glows then fades."), Some(GlowOn::Body));
        // blind, or no glow of an enchanting
        assert_eq!(on("Your small shield vibrates for a moment."), None);
        assert_eq!(on("Your weapon feels warm for a moment."), None);
        assert_eq!(on("Your hands begin to glow red."), None);
        let blue = glowing("Your long sword glows blue for a moment.", &pack).map(|(_, c)| c);
        assert!(blue.is_some_and(|c| c.b > c.r));
    }

    /// Drinking and reading show on the body too, after what shows in
    /// the hand; healing is told by the lines a potion, a spell or a
    /// prayer gives.
    #[test]
    fn a_drink_and_a_page_work_on_the_body() {
        let body = Body {
            feet: Vector3::ZERO,
            tall: 1.8,
            node: None,
        };
        let names = |kind: UseKind| -> Vec<(f32, &'static str)> {
            let u = ItemUse {
                kind,
                letter: Some('a'),
                tile: None,
                class: None,
                dir: None,
                vertical: None,
                spot: None,
                prayer: None,
                spell: None,
            };
            let fx = use_effects(&u, Color::WHITE, Vector3::UP, None, None, body.clone());
            fx.into_iter()
                .map(|(t, fx)| match fx {
                    Fx::Burst { name, .. } | Fx::Body { name, .. } => (t, name),
                    _ => (t, "other"),
                })
                .collect()
        };
        assert_eq!(names(UseKind::Quaff), [(0.8, "quaff"), (DRUNK_AT, "drunk")]);
        assert_eq!(names(UseKind::Read), [(0.5, "read"), (READ_AT, "lore")]);
        assert!(healed("You feel better.") && healed("You feel completely healed."));
        assert!(!healed("You feel feverish.") && !healed("You feel much better now."));
    }
}
