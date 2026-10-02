//! Which branch of the dungeon the hero is in, from the level notice: the
//! look of a level follows its branch (and in the endgame its plane), never
//! a special level's name, which the hero may not know.

use nh_protocol::LevelNotice;

use crate::World;

/// A plane of the endgame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plane {
    Earth,
    Air,
    Fire,
    Water,
    Astral,
}

/// A branch of the dungeon, as far as its look goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Branch {
    /// The Dungeons of Doom, and anything not known (no notice yet).
    #[default]
    Main,
    Mines,
    Sokoban,
    Quest,
    Ludios,
    Gehennom,
    Vlad,
    Planes(Plane),
}

impl Branch {
    /// The branch a level notice names (an unknown dungeon looks like the
    /// main one).
    pub fn of(level: &LevelNotice) -> Branch {
        match level.dungeon.as_str() {
            "The Gnomish Mines" => Branch::Mines,
            "Sokoban" => Branch::Sokoban,
            "The Quest" => Branch::Quest,
            "Fort Ludios" => Branch::Ludios,
            "Gehennom" => Branch::Gehennom,
            "Vlad's Tower" => Branch::Vlad,
            "The Elemental Planes" => Branch::Planes(match level.plane.as_deref() {
                Some("earth") => Plane::Earth,
                Some("air") => Plane::Air,
                Some("fire") => Plane::Fire,
                Some("water") => Plane::Water,
                _ => Plane::Astral,
            }),
            _ => Branch::Main,
        }
    }

    /// A short name for logs and self-tests.
    pub fn name(self) -> &'static str {
        match self {
            Branch::Main => "main",
            Branch::Mines => "mines",
            Branch::Sokoban => "sokoban",
            Branch::Quest => "quest",
            Branch::Ludios => "ludios",
            Branch::Gehennom => "gehennom",
            Branch::Vlad => "vlad",
            Branch::Planes(Plane::Earth) => "earth",
            Branch::Planes(Plane::Air) => "air",
            Branch::Planes(Plane::Fire) => "fire",
            Branch::Planes(Plane::Water) => "water",
            Branch::Planes(Plane::Astral) => "astral",
        }
    }
}

impl World {
    /// The branch the hero is in (the main dungeon until a notice says).
    pub fn branch(&self) -> Branch {
        self.level.as_ref().map_or(Branch::Main, Branch::of)
    }

    /// How deep the hero is (None before the first notice).
    pub fn depth(&self) -> Option<i32> {
        self.level.as_ref().map(|l| l.depth)
    }
}

#[cfg(test)]
mod tests {
    use nh_protocol::{EngineMsg, parse_line};

    use super::*;

    fn notice(world: &mut World, line: &str) {
        match parse_line(line).unwrap() {
            EngineMsg::Win(call) => world.apply(&call),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_level_notice_says_which_branch_the_hero_is_in() {
        let mut w = World::new();
        assert_eq!(w.branch(), Branch::Main);
        assert_eq!(w.depth(), None);
        notice(
            &mut w,
            r#"{"t":"win","fn":"level","a":{"dungeon":"The Gnomish Mines","depth":4}}"#,
        );
        assert_eq!(w.branch(), Branch::Mines);
        assert_eq!(w.depth(), Some(4));
        notice(
            &mut w,
            r#"{"t":"win","fn":"level","a":{"dungeon":"Gehennom","depth":30}}"#,
        );
        assert_eq!(w.branch(), Branch::Gehennom);
        notice(
            &mut w,
            r#"{"t":"win","fn":"level","a":{"dungeon":"The Elemental Planes","depth":-4,"plane":"water"}}"#,
        );
        assert_eq!(w.branch(), Branch::Planes(Plane::Water));
        assert_eq!(w.branch().name(), "water");
        // a dungeon this client does not know looks like the main one
        notice(
            &mut w,
            r#"{"t":"win","fn":"level","a":{"dungeon":"The Tutorial","depth":1}}"#,
        );
        assert_eq!(w.branch(), Branch::Main);
    }
}
