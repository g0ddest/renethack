//! The hero's states as 5.0's status line shows them (botl.c): hunger,
//! encumbrance and the conditions, each by the word the line prints. Each
//! has how alarming it is, the symbol of its icon (game-icons.net, in
//! `art/ccby/game-icons/`), and a line on what it means and what usually
//! helps (Fluent `state-help-<slug>`), general game knowledge and nothing
//! the hero does not see. A state is shown only while the status line
//! shows it.
//!
//! No Godot here: the rail by the orbs is `states_view`.

use crate::hud::Tone;
use crate::spell_bake::Paint;
use crate::spells::School;

/// One state: the status line's word ("Hungry", "Blind", "Stone"), how
/// alarming it is, its icon's symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub word: &'static str,
    pub tone: Tone,
    pub art: &'static str,
}

const fn st(word: &'static str, tone: Tone, art: &'static str) -> State {
    State { word, tone, art }
}

const FOOD: &str = "delapouite/fork-knife-spoon";
const LOAD: &str = "delapouite/weight";

/// Every word the status line may show for hunger and encumbrance (eat.c's
/// hu_stat, botl.c's enc_stat) and the conditions of botl.c's table but
/// "Iron" (5.0 never sets it), as `hud`'s tones grade them.
pub const STATES: [State; 39] = {
    use Tone::{Bad as B, Deadly as D, Info as I, Warn as W};
    [
        // hunger
        st("Satiated", W, "skoll/fat"),
        st("Hungry", W, FOOD),
        st("Weak", B, FOOD),
        st("Fainting", D, FOOD),
        st("Fainted", D, FOOD),
        // encumbrance
        st("Burdened", W, LOAD),
        st("Stressed", B, LOAD),
        st("Strained", B, LOAD),
        st("Overtaxed", B, LOAD),
        st("Overloaded", B, LOAD),
        // kill unless cured
        st("Stone", D, "lorc/stoned-skull"),
        st("Slime", D, "delapouite/slime"),
        st("Strngl", D, "delapouite/lungs"),
        st("FoodPois", D, "lorc/vomiting"),
        st("TermIll", D, "delapouite/plague-doctor-profile"),
        // dangerous now
        st("InLava", B, "sbed/lava"),
        st("Grab", B, "lorc/shadow-grasp"),
        st("Parlyz", B, "delapouite/frozen-body"),
        st("Out", B, "delapouite/dead-head"),
        st("Zzz", B, "lorc/sleepy"),
        // impair the hero
        st("Blind", W, "skoll/sight-disabled"),
        st("Conf", W, "lorc/swirl-string"),
        st("Stun", W, "skoll/knockout"),
        st("Hallu", W, "lorc/spotted-mushroom"),
        st("Deaf", W, "skoll/hearing-disabled"),
        st("Held", W, "lorc/manacles"),
        st("Trap", W, "lorc/wolf-trap"),
        st("WLegs", W, "lorc/broken-bone"),
        st("Submrg", W, "lorc/drowning"),
        st("Teth", W, "lorc/harpoon-chain"),
        st("Slip", W, "delapouite/oil-can"),
        // worth knowing
        st("Lev", I, "delapouite/air-balloon"),
        st("Fly", I, "lorc/feathered-wing"),
        st("Ride", I, "lorc/horse-head"),
        st("Glow", I, "lorc/glowing-hands"),
        st("Icy", I, "delapouite/ice-skate"),
        st("UHold", I, "lorc/grab"),
        st("Bare", I, "lorc/fist"),
        st("Busy", I, "lorc/hourglass"),
    ]
};

/// The state the status line's word `word` is.
pub fn state(word: &str) -> Option<&'static State> {
    STATES.iter().find(|s| s.word == word)
}

/// A state's file and Fluent name: "FoodPois" is `foodpois`.
pub fn slug(word: &str) -> String {
    word.to_lowercase()
}

/// The Fluent id of what the state means and what usually helps.
pub fn help_id(word: &str) -> String {
    format!("state-help-{}", slug(word))
}

/// The plate of a tone: its shape (a spell school's) and its paint. Shape
/// and colour both grade it, so the colour is never the only cue: deadly a
/// spiked disc in red, dangerous a hexagon in orange, impairing a disc in
/// amber, worth knowing a block in steel blue.
pub fn plate(tone: Tone) -> (School, Paint) {
    let p = |deep: u32, mid: u32, light: u32| Paint {
        deep: rgb(deep),
        mid: rgb(mid),
        light: rgb(light),
    };
    match tone {
        Tone::Deadly => (School::Attack, p(0x2a0404, 0xa81818, 0xffb4a4)),
        Tone::Bad => (School::Divination, p(0x2a1204, 0xb4561c, 0xffcc9c)),
        Tone::Warn => (School::Healing, p(0x2a2006, 0xa8841e, 0xfff0b8)),
        Tone::Info => (School::Matter, p(0x0a1624, 0x2e5e86, 0xc8e4ff)),
    }
}

fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{self, Lang};

    #[test]
    fn every_state_once_toned_as_the_hud_tones_it() {
        for (i, s) in STATES.iter().enumerate() {
            assert!(
                STATES[i + 1..].iter().all(|o| o.word != s.word),
                "{}",
                s.word
            );
            assert_eq!(state(s.word), Some(s));
        }
        // the conditions grade as the HUD's chips do
        for s in &STATES[10..] {
            assert_eq!(crate::hud::condition_tone(s.word), s.tone, "{}", s.word);
        }
        assert_eq!(state("Weak").map(|s| s.tone), Some(Tone::Bad));
        assert_eq!(state("Fainting").map(|s| s.tone), Some(Tone::Deadly));
        assert_eq!(state("Iron"), None, "5.0 never shows it");
        assert_eq!(slug("FoodPois"), "foodpois");
    }

    #[test]
    fn every_state_says_what_it_means_and_what_helps_in_both_languages() {
        for s in &STATES {
            let id = help_id(s.word);
            let en = i18n::tr_in(Lang::En, &id).unwrap_or_else(|| panic!("no {id} in English"));
            let ru = i18n::tr_in(Lang::Ru, &id).unwrap_or_else(|| panic!("no {id} in Russian"));
            assert!(
                en.len() > 12 && ru.chars().any(|c| ('а'..='я').contains(&c)),
                "{id}"
            );
            assert_ne!(en, ru, "{id}");
        }
    }

    #[test]
    fn the_tones_differ_in_shape_and_in_paint() {
        let tones = [Tone::Deadly, Tone::Bad, Tone::Warn, Tone::Info];
        for (i, a) in tones.iter().enumerate() {
            for b in &tones[i + 1..] {
                let ((sa, pa), (sb, pb)) = (plate(*a), plate(*b));
                assert_ne!(sa, sb, "{a:?} and {b:?} share a shape");
                assert_ne!(pa.mid, pb.mid, "{a:?} and {b:?} share a paint");
            }
        }
    }
}
