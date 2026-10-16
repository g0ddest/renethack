//! The first-steps guide: its pages (the quest page, then a hint card for
//! each first occasion: the first monster seen, the first item underfoot,
//! Hungry...) and the watch that finds those occasions in the world. A card
//! is general advice tied to what the player just saw: it never names what
//! the hero does not know (an item's true kind, a level's secrets).

use nh_protocol::{Catalog, GlyphKind, mg};
use serde::{Deserialize, Serialize};

use crate::{COLNO, ROWNO, Terrain, World, cell_terrain};

/// A page of the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Guide {
    /// "Your quest": the goal, the screen, moving and fighting.
    Quest,
    /// Weak, or few hit points left: a prayer.
    Pray,
    /// A creature in sight (not the pet).
    Monster,
    /// Hit points below their most.
    Wounded,
    Hungry,
    /// A shopkeeper's welcome.
    Shop,
    /// "You see here…": something underfoot.
    ItemHere,
    /// A scroll, potion, ring, wand, amulet or spellbook in the pack whose
    /// kind the hero does not know.
    Unidentified,
    /// "Welcome to experience level…".
    LevelUp,
    /// "You feel more confident in your … skills".
    Enhance,
    Pet,
    Trap,
    StairsDown,
    Door,
    Fountain,
    Altar,
}

impl Guide {
    /// The hint cards, the most pressing first: several occasions at once
    /// come in this order.
    pub const CARDS: [Guide; 15] = [
        Guide::Pray,
        Guide::Monster,
        Guide::Wounded,
        Guide::Hungry,
        Guide::Shop,
        Guide::ItemHere,
        Guide::Unidentified,
        Guide::LevelUp,
        Guide::Enhance,
        Guide::Pet,
        Guide::Trap,
        Guide::StairsDown,
        Guide::Door,
        Guide::Fountain,
        Guide::Altar,
    ];

    /// Its name in the client's catalogs ("guide-pray").
    pub fn key(self) -> &'static str {
        match self {
            Guide::Quest => "guide-quest",
            Guide::Pray => "guide-pray",
            Guide::Monster => "guide-monster",
            Guide::Wounded => "guide-wounded",
            Guide::Hungry => "guide-hungry",
            Guide::Shop => "guide-shop",
            Guide::ItemHere => "guide-item-here",
            Guide::Unidentified => "guide-unidentified",
            Guide::LevelUp => "guide-level-up",
            Guide::Enhance => "guide-enhance",
            Guide::Pet => "guide-pet",
            Guide::Trap => "guide-trap",
            Guide::StairsDown => "guide-stairs-down",
            Guide::Door => "guide-door",
            Guide::Fountain => "guide-fountain",
            Guide::Altar => "guide-altar",
        }
    }
}

/// The classes whose items the hero names by their look until identified.
const UNKNOWN_CLASSES: &str = "?!=/\"+";

/// The cards' occasions as the world shows them.
#[derive(Debug, Clone, Default)]
pub struct HintWatch {
    /// The log read up to here.
    seen_seq: u64,
}

impl HintWatch {
    pub fn new() -> HintWatch {
        HintWatch::default()
    }

    /// The log read up to now: what was said before does not count (a
    /// restored game's history, what came before the guide looked).
    pub fn skip_log(&mut self, world: &World) {
        self.seen_seq = world.log.last_seq();
    }

    /// The cards whose occasion the world gives now, most pressing first:
    /// what the map shows, the status, the pack, and what was said since
    /// the last call.
    pub fn occasions(&mut self, world: &World, catalog: &Catalog) -> Vec<Guide> {
        let mut found = Vec::new();
        let mut add = |g: Guide| {
            if !found.contains(&g) {
                found.push(g);
            }
        };
        // said since the last look
        for m in world.log.since(self.seen_seq).filter(|m| !m.from_history) {
            let t = m.text.as_str();
            if t.starts_with("You see here ")
                || t == "There are several objects here."
                || t == "There are many objects here."
            {
                add(Guide::ItemHere);
            }
            // a shopkeeper's welcome ("Hello, Hero!  Welcome to Asidonhopo's
            // general store!"), or a mute one's ("You enter …")
            if t.contains("!  Welcome to ") || t.contains("!  Welcome again to ") {
                add(Guide::Shop);
            }
            if t.starts_with("Welcome to experience level ") {
                add(Guide::LevelUp);
            }
            // the engine's English (the log shows it translated), asleep
            // too ("You dream that you feel…")
            if t.starts_with("You feel more confident in your ")
                || t.starts_with("You dream that you feel more confident in your ")
            {
                add(Guide::Enhance);
            }
        }
        self.seen_seq = world.log.last_seq();
        // the status
        let hunger = world.status.get("hunger").unwrap_or("").trim();
        let hp = world.status.number("hp");
        let most = world.status.number("hpmax");
        if let (Some(hp), Some(most)) = (hp, most) {
            // NetHack's own "low hit points" for a prayer
            if hp <= 5 || hp * 7 <= most {
                add(Guide::Pray);
            }
            if hp < most {
                add(Guide::Wounded);
            }
        }
        if matches!(hunger, "Weak" | "Fainting" | "Fainted") {
            add(Guide::Pray);
        }
        if !hunger.is_empty() && hunger != "Not Hungry" && hunger != "Satiated" {
            add(Guide::Hungry);
        }
        // the pack: a kind the hero does not know by name
        let unknown = world.inventory.items().iter().any(|i| {
            UNKNOWN_CLASSES.contains(i.class)
                && !i.text.contains(" of ")
                && !i.text.contains("Amulet")
        });
        if unknown {
            add(Guide::Unidentified);
        }
        // the map
        for y in 0..ROWNO {
            for x in 1..COLNO {
                let Some(cell) = world.map.cell(x, y) else {
                    continue;
                };
                if let Some(g) = cell.entity()
                    && g.kind == GlyphKind::Mon
                    && g.flags & mg::HERO == 0
                {
                    add(if g.flags & mg::PET != 0 {
                        Guide::Pet
                    } else {
                        Guide::Monster
                    });
                }
                match cell_terrain(cell, catalog) {
                    Some(Terrain::StairsDown | Terrain::LadderDown) => add(Guide::StairsDown),
                    Some(Terrain::ClosedDoor | Terrain::OpenDoor) => add(Guide::Door),
                    Some(Terrain::Fountain) => add(Guide::Fountain),
                    Some(Terrain::Altar) => add(Guide::Altar),
                    Some(Terrain::Trap) => add(Guide::Trap),
                    _ => {}
                }
            }
        }
        found.sort_by_key(|g| Guide::CARDS.iter().position(|c| c == g));
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::tests::{catalog, cmap, draw};
    use nh_protocol::{EngineMsg, parse_line};

    fn apply(w: &mut World, lines: &[String]) {
        for l in lines {
            if let EngineMsg::Win(call) = parse_line(l).unwrap() {
                w.apply(&call);
            }
        }
    }

    fn status(field: &str, value: &str) -> String {
        format!(r#"{{"t":"win","fn":"status_update","a":{{"field":"{field}","value":"{value}"}}}}"#)
    }

    fn said(texts: &[&str]) -> Vec<String> {
        let mut out = vec![
            r#"{"t":"win","fn":"create_nhwindow","a":{"win":1,"type":"message"}}"#.to_string(),
        ];
        for t in texts {
            out.push(format!(
                r#"{{"t":"win","fn":"putstr","a":{{"win":1,"attr":0,"str":"{t}"}}}}"#
            ));
        }
        out
    }

    #[test]
    fn the_map_shows_the_pet_monsters_and_features() {
        let cat = catalog();
        let mut w = World::new();
        draw(&mut w.map, &cat, 2, &["|.@f..|"]);
        let mut watch = HintWatch::new();
        assert_eq!(watch.occasions(&w, &cat), vec![Guide::Pet]);
        draw(&mut w.map, &cat, 3, &["|J.>+^|"]);
        for (x, sym) in [(2, "S_fountain"), (3, "S_altar")] {
            w.map.print(x, 4, &cmap(&cat, sym), None);
        }
        assert_eq!(
            watch.occasions(&w, &cat),
            vec![
                Guide::Monster,
                Guide::Pet,
                Guide::Trap,
                Guide::StairsDown,
                Guide::Door,
                Guide::Fountain,
                Guide::Altar
            ]
        );
        // the hero alone is no occasion
        let mut w = World::new();
        draw(&mut w.map, &cat, 2, &["|.@..|"]);
        assert!(HintWatch::new().occasions(&w, &cat).is_empty());
    }

    #[test]
    fn the_status_tells_hunger_wounds_and_trouble() {
        let cat = catalog();
        let at = |lines: &[String]| {
            let mut w = World::new();
            apply(&mut w, lines);
            HintWatch::new().occasions(&w, &cat)
        };
        assert!(at(&[status("hp", "16"), status("hpmax", "16")]).is_empty());
        assert_eq!(
            at(&[
                status("hp", "12"),
                status("hpmax", "16"),
                status("hunger", "Hungry")
            ]),
            vec![Guide::Wounded, Guide::Hungry]
        );
        assert_eq!(
            at(&[status("hp", "2"), status("hpmax", "16")]),
            vec![Guide::Pray, Guide::Wounded]
        );
        assert_eq!(
            at(&[status("hunger", "Weak")]),
            vec![Guide::Pray, Guide::Hungry]
        );
        assert!(at(&[status("hunger", "Satiated")]).is_empty());
    }

    #[test]
    fn what_was_said_counts_once_and_history_never() {
        let cat = catalog();
        let mut w = World::new();
        apply(
            &mut w,
            &said(&[
                "You see here a dagger.",
                "Hello, Hero!  Welcome to Asidonhopo's general store!",
                "Welcome to experience level 2.",
                "You feel more confident in your weapon skills.",
            ]),
        );
        let mut watch = HintWatch::new();
        assert_eq!(
            watch.occasions(&w, &cat),
            vec![Guide::Shop, Guide::ItemHere, Guide::LevelUp, Guide::Enhance]
        );
        assert!(watch.occasions(&w, &cat).is_empty(), "read once");
        // skipped: what came before the guide looked
        let mut watch = HintWatch::new();
        watch.skip_log(&w);
        assert!(watch.occasions(&w, &cat).is_empty());
        // a restored game's old news
        let mut w = World::new();
        apply(
            &mut w,
            &[r#"{"t":"win","fn":"putmsghistory","a":{"msg":"You see here a dagger.","restoring":true}}"#
                .to_string()],
        );
        assert!(HintWatch::new().occasions(&w, &cat).is_empty());
        // a skill felt in a dream; no slot left is no skill to advance
        for (text, card) in [
            (
                "You dream that you feel more confident in your fighting skills.",
                true,
            ),
            ("You feel you could be more dangerous!", false),
        ] {
            let mut w = World::new();
            apply(&mut w, &said(&[text]));
            let found = HintWatch::new().occasions(&w, &cat);
            assert_eq!(found == vec![Guide::Enhance], card, "{text}: {found:?}");
        }
    }

    #[test]
    fn an_item_whose_kind_the_hero_does_not_know() {
        let cat = catalog();
        for (text, class, unknown) in [
            ("an uncursed scroll labeled FOOBIE BLETCH", "?", true),
            ("a bubbly potion", "!", true),
            ("a jade ring", "=", true),
            ("a potion of water", "!", false),
            ("a wand of striking (0:5)", "/", false),
            ("a +1 spear (weapon in right hand)", ")", false),
        ] {
            let mut w = World::new();
            apply(
                &mut w,
                &[format!(
                    r#"{{"t":"win","fn":"inventory","a":{{"items":[{{"letter":"f","class":"{class}","tile":1,"quan":1,"slots":[],"lit":false,"text":"{text}"}}],"twoweap":false}}}}"#
                )],
            );
            let found = HintWatch::new().occasions(&w, &cat);
            assert_eq!(found.contains(&Guide::Unidentified), unknown, "{text}");
        }
    }
}
