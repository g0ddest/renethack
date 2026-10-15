//! What monsters do, shown as they do it. The messages tell it ("The
//! kobold shaman casts a spell at you!"), as they tell the hero's sounds,
//! and only of a monster the map shows as seen: a caster turns to its
//! target and casts (its clip, else a turn), a glow comes into its hand
//! as the spell leaves it, and the spell strikes where the line says it
//! went, the hero; one that cannot cast points and curses, its glow
//! fizzling; a fumbled spell crackles in the air around its caster.
//! Nothing waits for it: the next turn plays alongside.

use godot::classes::Skeleton3D;
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::World;

use super::MapView;
use crate::vfx::VfxKind;

/// The clip a caster casts with (the hero's own, `held.uses.cast`), the
/// hand its spell leaves, and the colour of a monster's magic.
const CAST_CLIP: &str = "Spell_Simple_Shoot";
const CAST_HAND: &str = "hand_l";
const MAGIC: Color = Color::from_rgb(0.66, 0.5, 1.0);
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
}

/// What a message says a monster did: the monster by the name the line
/// uses, and what.
pub fn act_of(text: &str) -> Option<(&str, ActKind)> {
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
}

enum FxAt {
    /// A bone of a model's skeleton; where its body's middle is, if it has
    /// none.
    Hand(Gd<Node3D>, &'static str, Vector3),
    Point(Vector3),
}

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
        let Some((name, kind)) = act_of(text) else {
            return;
        };
        let Some(cell) = seen_named(world, catalog, name) else {
            return;
        };
        let hero = world.hero();
        let aim_at = match kind {
            ActKind::Cast(Aim::Hero) | ActKind::Curse(Aim::Hero) => hero,
            _ => None,
        };
        let (clip, body) = match kind {
            ActKind::Crackle => (false, self.body_of(cell)),
            _ => {
                // it turns to the hero it casts at (or points at)
                let toward = aim_at.or(hero).filter(|&h| h != cell);
                let clip = match toward {
                    Some(to) => self.act_toward(cell, to, world, CAST_CLIP),
                    None => false,
                };
                (clip, self.body_of(cell))
            }
        };
        let Some((node, middle)) = body else {
            return;
        };
        let hand = FxAt::Hand(node, CAST_HAND, middle);
        match kind {
            ActKind::Cast(_) => {
                self.act_fx.push(ActFx {
                    wait: GLOW_AT,
                    kind: VfxKind::Cast(MAGIC),
                    at: hand,
                });
                if let Some(h) = aim_at {
                    let at = self.sound_at(h);
                    self.act_fx.push(ActFx {
                        wait: STRIKES_AT,
                        kind: VfxKind::Shock(MAGIC),
                        at: FxAt::Point(at),
                    });
                }
            }
            // the glow comes and dies at once
            ActKind::Curse(_) => self.act_fx.push(ActFx {
                wait: GLOW_AT,
                kind: VfxKind::Sparkle,
                at: hand,
            }),
            ActKind::Crackle => self.act_fx.push(ActFx {
                wait: 0.0,
                kind: VfxKind::Lightning,
                at: FxAt::Point(middle),
            }),
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
        false
    }

    /// The model standing on `cell` and its body's middle.
    fn body_of(&self, cell: (i32, i32)) -> Option<(Gd<Node3D>, Vector3)> {
        let n = self.cells.get(&cell)?;
        let i = n.look.entity?;
        let (m, p) = n.models.get(i).zip(n.look.models.get(i))?;
        let middle = m.node.get_global_position() + Vector3::new(0.0, p.look.art.height * 0.6, 0.0);
        Some((m.node.clone(), middle))
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
            };
            self.vfx.burst(fx.kind, at);
        }
    }

    /// The acts shown lately (self-tests).
    pub fn acts_shown(&self) -> &[Acted] {
        &self.acts_shown
    }

    /// The acts' effects still waiting their moment (self-tests).
    pub fn acts_pending(&self) -> usize {
        self.act_fx.len()
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
            Some(("kobold shaman", ActKind::Cast(Aim::Around)))
        );
        assert_eq!(
            act_of("The gnomish wizard casts a spell at you!"),
            Some(("gnomish wizard", ActKind::Cast(Aim::Hero)))
        );
        assert_eq!(
            act_of("The orc shaman casts a spell at thin air!"),
            Some(("orc shaman", ActKind::Cast(Aim::Astray)))
        );
        assert_eq!(
            act_of("The kobold shaman casts a spell at your displaced image!"),
            Some(("kobold shaman", ActKind::Cast(Aim::Astray)))
        );
        assert_eq!(
            act_of("The orc shaman points at you, then curses."),
            Some(("orc shaman", ActKind::Curse(Aim::Hero)))
        );
        assert_eq!(
            act_of("The orc shaman points all around, then curses."),
            Some(("orc shaman", ActKind::Curse(Aim::Around)))
        );
        assert_eq!(
            act_of("The orc shaman points and curses in your general direction."),
            Some(("orc shaman", ActKind::Curse(Aim::Astray)))
        );
        assert_eq!(
            act_of("The air crackles around the kobold shaman."),
            Some(("kobold shaman", ActKind::Crackle))
        );
        assert_eq!(
            act_of("The Grey-elf zaps you with a blast of fire!"),
            Some(("Grey-elf", ActKind::Cast(Aim::Hero)))
        );
        assert_eq!(act_of("The gnome lord zaps a wand of striking!"), None);
        assert_eq!(act_of("Something casts a spell at you!"), None);
        assert_eq!(act_of("You hear a mumbled curse."), None);
        assert_eq!(act_of("The newt bites!"), None);
    }
}
