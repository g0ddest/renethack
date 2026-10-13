//! What the hero hears of the map, as cue names (`cues.rs`): a step by its
//! floor and the walker's gait, a blow by the weapon or the body that
//! strikes, a hurt or a death by the creature's body plan. Never more than
//! the map shows or the messages tell: the caller hears only monsters the
//! hero sees, and a use by what the item looks like. Plain rules (no Godot).

use nh_protocol::{Catalog, MonsterInfo};
use nh_world::{Branch, Plane, Terrain, UseKind, World};

/// The floor a step on `t` sounds on, in `branch`; None: there is none
/// (air, cloud).
pub fn floor(t: Terrain, branch: Branch) -> Option<&'static str> {
    use Terrain::*;
    Some(match t {
        Air | Cloud => return None,
        Corridor | Grave => "dirt",
        OpenDoor | BrokenDoor | DrawbridgeDown | LadderUp | LadderDown => "wood",
        Tree => "grass",
        Pool | Water => "water",
        Ice => "ice",
        Lava | LavaWall => "lava",
        // rooms: the branch's own floor
        _ => match branch {
            Branch::Sokoban => "wood",
            Branch::Mines | Branch::Planes(Plane::Earth) => "dirt",
            Branch::Planes(Plane::Water) => "water",
            _ => "stone",
        },
    })
}

/// How a creature's steps sound: "foot", "paw", "hoof", "slither",
/// "ooze", "skitter", "wings"; None: it makes none (it floats, or has no
/// body to tread with).
pub fn gait(m: &MonsterInfo) -> Option<&'static str> {
    let has = |flag: &str| m.body.iter().any(|b| b == flag);
    if has("unsolid") {
        return None;
    }
    if has("fly") {
        // bats, birds and dragons beat their wings; eyes, lights and the
        // like drift without a sound
        return matches!(m.class.as_str(), "B" | "D").then_some("wings");
    }
    if has("amorphous") {
        return Some("ooze");
    }
    if has("slithy") || has("nolimbs") {
        return Some("slither");
    }
    Some(match m.class.as_str() {
        "u" | "C" => "hoof",
        "d" | "f" | "r" | ":" | "q" | "c" | "Y" => "paw",
        "a" | "s" | "x" => "skitter",
        _ => "foot",
    })
}

/// The cue names of a step, most specific first: the floor and the gait,
/// the gait alone, then (for feet and the like) the floor and any step. A
/// snake never falls back to a boot's step.
pub fn step(floor: &str, gait: &str) -> Vec<String> {
    match gait {
        "foot" => vec![format!("step.{floor}"), "step".into()],
        "paw" | "hoof" => vec![
            format!("step.{floor}.{gait}"),
            format!("step.{gait}"),
            format!("step.{floor}"),
            "step".into(),
        ],
        _ => vec![format!("step.{floor}.{gait}"), format!("step.{gait}")],
    }
}

/// A creature's body plan, for its hurts and its death.
pub fn plan(m: &MonsterInfo) -> &'static str {
    let has = |flag: &str| m.body.iter().any(|b| b == flag);
    if has("undead") {
        return "undead";
    }
    if has("demon") {
        return "demon";
    }
    match m.class.as_str() {
        "D" => "dragon",
        "a" | "s" | "x" => "insect",
        "B" | "c" => "bird",
        "b" | "j" | "P" | "F" => "slime",
        _ if has("humanoid") => "humanoid",
        _ if has("animal") => "beast",
        _ => "creature",
    }
}

/// The class of a held weapon's blows, by the art manifest's held model
/// (None: unarmed).
pub fn weapon(held: Option<&str>) -> &'static str {
    match held {
        None => "unarmed",
        Some("dagger" | "short_blade" | "long_blade" | "curved_blade" | "great_blade") => "blade",
        Some("axe" | "great_axe" | "mattock" | "pick") => "axe",
        Some("spear" | "lance" | "polearm") => "pierce",
        Some("whip") => "whip",
        Some(_) => "blunt",
    }
}

/// The sound a monster's blow makes landing, by its verb ("bite").
pub fn blow(verb: &str) -> &'static str {
    match verb {
        "bite" => "bite",
        "claw" | "scratch" => "claw",
        "sting" => "sting",
        "kick" | "butt" | "headbutt" => "blunt",
        "lash" => "whip",
        "touch" | "tentacle" | "peck" => "touch",
        _ => "",
    }
}

/// The cue names of a hero's blow, landing or swung: by the held model,
/// then its class (`hit.long_blade`, `hit.blade`, `hit`).
pub fn armed(what: &str, held: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(h) = held {
        names.push(format!("{what}.{h}"));
    }
    names.push(format!("{what}.{}", weapon(held)));
    names
}

/// What of the hero's state the map's sounds depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Senses {
    /// They float or fly: their feet touch nothing.
    pub afloat: bool,
    /// They see nothing: a monster on the map is only sensed, its steps
    /// unheard (also when the engine does not tell it so).
    pub blind: bool,
    /// They hear nothing well: the world is muffled.
    pub deaf: bool,
}

impl Senses {
    pub fn of(world: &World, catalog: &Catalog) -> Senses {
        let names = world.status.condition_names(catalog);
        let has = |c: &str| names.iter().any(|n| n == c);
        Senses {
            afloat: has("Lev") || has("Fly"),
            blind: has("Blind"),
            deaf: has("Deaf"),
        }
    }
}

/// The game's scene, for its loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// The title and the character's making, the title's scene behind.
    Title,
    Playing,
    /// The end: the tombstone, an error.
    Over,
}

/// The ambience of a scene, most specific first: the title's fires, the
/// branch's own (any ambience after it).
pub fn ambience(scene: Scene, branch: Branch) -> Vec<String> {
    match scene {
        Scene::Title => vec!["amb.title".to_string()],
        Scene::Playing => vec![format!("amb.{}", branch.name())],
        Scene::Over => Vec::new(),
    }
}

/// A fight starts the combat music once the orders' Combat mode has held
/// this long, and the music stays this long after the mode ends (real
/// seconds: a player thinking between turns keeps what plays).
pub const FIGHT_HOLDS: f64 = 4.0;
pub const CALM_HOLDS: f64 = 20.0;

/// The music by the scene: the title's, the branch's to explore by, a
/// fight's once it lasts (the Combat mode comes only from what the hero
/// perceives: a hostile seen or sensed, a fight told, a wound).
#[derive(Debug, Clone, Default)]
pub struct Score {
    fighting: bool,
    /// Since when the mode has differed from `fighting`.
    since: Option<f64>,
}

impl Score {
    /// The music's names now, most specific first (none: silence).
    pub fn music(&mut self, now: f64, scene: Scene, branch: Branch, combat: bool) -> Vec<String> {
        if scene != Scene::Playing {
            self.fighting = false;
            self.since = None;
        } else if combat == self.fighting {
            self.since = None;
        } else {
            let since = *self.since.get_or_insert(now);
            let hold = if combat { FIGHT_HOLDS } else { CALM_HOLDS };
            if now - since >= hold {
                self.fighting = combat;
                self.since = None;
            }
        }
        let b = branch.name();
        let explore = [format!("music.explore.{b}"), "music.explore".to_string()];
        match scene {
            Scene::Title => vec!["music.title".to_string()],
            // no music of its own for a fight: the exploring goes on
            Scene::Playing if self.fighting => {
                let mut names = vec![format!("music.combat.{b}"), "music.combat".to_string()];
                names.extend(explore);
                names
            }
            Scene::Playing => explore.to_vec(),
            Scene::Over => Vec::new(),
        }
    }

    pub fn fighting(&self) -> bool {
        self.fighting
    }
}

/// A door beside the hero (the one their door messages are about).
pub fn door_beside(world: &World, catalog: &Catalog) -> Option<(i32, i32)> {
    let (hx, hy) = world.hero()?;
    let around = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (hx + dx, hy + dy)));
    around.filter(|&c| c != (hx, hy)).find(|&(x, y)| {
        let t = world
            .map
            .cell(x, y)
            .and_then(|c| nh_world::cell_terrain(c, catalog));
        matches!(
            t,
            Some(Terrain::ClosedDoor | Terrain::OpenDoor | Terrain::BrokenDoor | Terrain::Doorway)
        )
    })
}

/// The monster a message says died before the hero's eyes ("The newt is
/// killed!", "The zombie is destroyed!", "... by the falling rock!"), by
/// the name the message uses.
pub fn died(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("The ")?;
    let end = [" is killed", " is destroyed"]
        .iter()
        .find_map(|tail| rest.find(tail))?;
    Some(&rest[..end]).filter(|n| !n.is_empty())
}

/// A monster by any of its names ("jackal", "gnome lord", "priestess").
pub fn monster_named<'a>(catalog: &'a Catalog, name: &str) -> Option<&'a MonsterInfo> {
    catalog.monsters.iter().find(|m| {
        m.name == name || m.male.as_deref() == Some(name) || m.female.as_deref() == Some(name)
    })
}

/// The cue of a use, and when in its clip it sounds (seconds): the bottle
/// at the lips, the page turned, the wand's spark, the foot landing.
pub fn use_cue(kind: UseKind) -> (&'static str, f32) {
    match kind {
        UseKind::Quaff => ("use.quaff", 0.5),
        UseKind::Read => ("use.read", 0.2),
        UseKind::Zap => ("use.zap", 0.35),
        UseKind::Cast => ("use.cast", 0.3),
        UseKind::Eat => ("use.eat", 0.4),
        UseKind::Apply => ("use.apply", 0.3),
        UseKind::Throw => ("use.throw", crate::hero::THROW_AT),
        UseKind::Fire => ("use.fire", crate::hero::THROW_AT),
        UseKind::Wear => ("use.wear", 0.1),
        UseKind::PutOn => ("use.puton", 0.1),
        UseKind::TakeOff => ("use.takeoff", 0.1),
        UseKind::Remove => ("use.remove", 0.1),
        UseKind::Wield => ("use.wield", 0.1),
        UseKind::PickUp => ("use.pickup", 0.2),
        UseKind::Kick => ("use.kick", 0.3),
        UseKind::Pray => ("use.pray", 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monster(class: &str, size: &str, body: &[&str]) -> MonsterInfo {
        MonsterInfo {
            idx: 0,
            name: "x".into(),
            male: None,
            female: None,
            class: class.into(),
            class_name: String::new(),
            size: size.into(),
            color: 0,
            light: 0,
            body: body.iter().map(|b| b.to_string()).collect(),
            tile_male: 0,
            tile_female: 0,
        }
    }

    #[test]
    fn a_step_sounds_on_its_floor() {
        assert_eq!(floor(Terrain::Floor, Branch::Main), Some("stone"));
        assert_eq!(floor(Terrain::Floor, Branch::Sokoban), Some("wood"));
        assert_eq!(floor(Terrain::DarkFloor, Branch::Mines), Some("dirt"));
        assert_eq!(floor(Terrain::Corridor, Branch::Main), Some("dirt"));
        assert_eq!(floor(Terrain::OpenDoor, Branch::Gehennom), Some("wood"));
        assert_eq!(floor(Terrain::Pool, Branch::Main), Some("water"));
        assert_eq!(floor(Terrain::Ice, Branch::Main), Some("ice"));
        assert_eq!(floor(Terrain::Lava, Branch::Gehennom), Some("lava"));
        assert_eq!(floor(Terrain::Air, Branch::Planes(Plane::Air)), None);
    }

    #[test]
    fn a_creature_treads_as_its_body_does() {
        let human = monster("@", "medium", &["humanoid"]);
        assert_eq!(gait(&human), Some("foot"));
        assert_eq!(gait(&monster("d", "small", &["animal"])), Some("paw"));
        assert_eq!(gait(&monster("u", "large", &["animal"])), Some("hoof"));
        assert_eq!(
            gait(&monster("S", "medium", &["slithy", "nolimbs"])),
            Some("slither")
        );
        assert_eq!(
            gait(&monster("P", "medium", &["amorphous", "nolimbs"])),
            Some("ooze")
        );
        assert_eq!(gait(&monster("s", "large", &["animal"])), Some("skitter"));
        assert_eq!(
            gait(&monster("B", "tiny", &["fly", "animal"])),
            Some("wings")
        );
        assert_eq!(
            gait(&monster("e", "small", &["fly", "nolimbs"])),
            None,
            "an eye floats"
        );
        assert_eq!(
            gait(&monster(" ", "medium", &["fly", "unsolid", "undead"])),
            None
        );
    }

    #[test]
    fn a_snake_never_sounds_like_boots() {
        assert_eq!(step("stone", "foot"), ["step.stone", "step"]);
        assert_eq!(
            step("wood", "hoof"),
            ["step.wood.hoof", "step.hoof", "step.wood", "step"]
        );
        assert_eq!(
            step("dirt", "slither"),
            ["step.dirt.slither", "step.slither"]
        );
    }

    #[test]
    fn hurts_and_deaths_go_by_body_plan() {
        assert_eq!(
            plan(&monster("Z", "medium", &["humanoid", "undead"])),
            "undead"
        );
        assert_eq!(
            plan(&monster("&", "large", &["humanoid", "demon"])),
            "demon"
        );
        assert_eq!(
            plan(&monster("D", "gigantic", &["fly", "animal"])),
            "dragon"
        );
        assert_eq!(plan(&monster("a", "tiny", &["animal"])), "insect");
        assert_eq!(plan(&monster("P", "medium", &["amorphous"])), "slime");
        assert_eq!(plan(&monster("o", "medium", &["humanoid"])), "humanoid");
        assert_eq!(plan(&monster("d", "small", &["animal"])), "beast");
    }

    #[test]
    fn each_branch_has_its_own_ambience_and_the_end_none() {
        assert_eq!(ambience(Scene::Title, Branch::Main), ["amb.title"]);
        assert_eq!(ambience(Scene::Playing, Branch::Mines), ["amb.mines"]);
        assert_eq!(
            ambience(Scene::Playing, Branch::Planes(Plane::Fire)),
            ["amb.fire"]
        );
        assert!(ambience(Scene::Over, Branch::Gehennom).is_empty());
    }

    #[test]
    fn a_fight_plays_its_music_only_once_it_lasts_and_for_a_while_after() {
        let mut score = Score::default();
        let at = |s: &mut Score, now: f64, combat: bool| {
            s.music(now, Scene::Playing, Branch::Mines, combat)
        };
        let explore = ["music.explore.mines", "music.explore"];
        assert_eq!(at(&mut score, 0.0, false), explore);
        // a hostile glimpsed for a moment: no fight's music
        assert_eq!(at(&mut score, 1.0, true), explore);
        assert_eq!(at(&mut score, 2.0, false), explore);
        // a fight that lasts
        assert_eq!(at(&mut score, 10.0, true), explore);
        assert_eq!(at(&mut score, 13.9, true), explore);
        let fight = at(&mut score, 14.0, true);
        assert_eq!(
            fight,
            [
                "music.combat.mines",
                "music.combat",
                "music.explore.mines",
                "music.explore"
            ]
        );
        // calm for a while: the fight's music goes on, then the exploring
        assert_eq!(at(&mut score, 20.0, false), fight);
        assert_eq!(
            at(&mut score, 30.0, true),
            fight,
            "a blow again: the calm starts over"
        );
        assert_eq!(at(&mut score, 31.0, false), fight);
        assert_eq!(at(&mut score, 50.9, false), fight);
        assert_eq!(at(&mut score, 51.0, false), explore);
        // the title's music, the end's silence; a fight does not outlive its game
        assert_eq!(
            score.music(60.0, Scene::Title, Branch::Main, false),
            ["music.title"]
        );
        assert!(
            score
                .music(61.0, Scene::Over, Branch::Main, true)
                .is_empty()
        );
        assert!(!score.fighting());
    }

    #[test]
    fn a_death_seen_is_told_by_name() {
        assert_eq!(died("The newt is killed!"), Some("newt"));
        assert_eq!(
            died("The gnome lord is killed by the arrow!"),
            Some("gnome lord")
        );
        assert_eq!(died("The zombie is destroyed!"), Some("zombie"));
        assert_eq!(
            died("You kill the newt!"),
            None,
            "the hero's own blow tells it"
        );
        assert_eq!(died("Hachi is killed!"), None);
        assert_eq!(
            died("The door is destroyed."),
            Some("door"),
            "a name the map has not"
        );
    }

    #[test]
    fn a_blow_sounds_as_what_strikes() {
        assert_eq!(weapon(Some("long_blade")), "blade");
        assert_eq!(weapon(Some("mace")), "blunt");
        assert_eq!(weapon(Some("great_axe")), "axe");
        assert_eq!(weapon(None), "unarmed");
        assert_eq!(armed("hit", Some("mace")), ["hit.mace", "hit.blunt"]);
        assert_eq!(armed("swing", None), ["swing.unarmed"]);
        assert_eq!(blow("bite"), "bite");
        assert_eq!(blow("scratch"), "claw");
        assert_eq!(blow("hit"), "", "a monster's weapon is not known");
    }
}
