//! What the keys and the gamepad do, as the settings page's controls show
//! them (read only): the keyboard's for either key profile, the gamepad's
//! in the controller's own button names. Each binding is an input drawn
//! as key caps, buttons and words, and what it does.

use nh_world::KeyProfile;

use crate::gamepad::PadButton;

/// A piece of an input as it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A key's cap: its name as keyboards print it (the same in every
    /// language).
    Key(&'static str),
    /// A gamepad's button, as the controller names it.
    Pad(PadButton),
    /// Words of the catalogs ("the arrows", "the left stick").
    Words(&'static str),
    /// Held together: "+".
    Plus,
    /// Either: "/".
    Or,
    /// From one to the other: "–".
    Through,
}

/// An input and what it does (a key of the catalogs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub input: &'static [Part],
    pub action: &'static str,
}

use Part::{Key, Or, Pad, Plus, Through, Words};

const fn b(input: &'static [Part], action: &'static str) -> Binding {
    Binding { input, action }
}

/// The keyboard's bindings that both profiles share, after the ones they
/// differ in.
const KEYS_COMMON: [Binding; 15] = [
    b(&[Key("1"), Through, Key("0")], "controls-bar"),
    b(&[Key("i")], "controls-inventory"),
    b(&[Key("#")], "controls-palette"),
    b(&[Words("controls-left-click")], "controls-click"),
    b(
        &[Words("controls-right-click")],
        "controls-right-click-does",
    ),
    b(&[Key("<"), Or, Key(">")], "controls-stairs"),
    b(&[Key("F5")], "controls-rest"),
    b(
        &[Words("controls-any-key"), Or, Key("Esc")],
        "controls-stop",
    ),
    b(
        &[Key("F8"), Or, Key("Ctrl"), Plus, Key("0")],
        "controls-overview",
    ),
    b(
        &[
            Words("controls-wheel"),
            Or,
            Key("Ctrl"),
            Plus,
            Key("-"),
            Or,
            Key("Ctrl"),
            Plus,
            Key("="),
        ],
        "controls-zoom",
    ),
    b(&[Key("F9")], "controls-log"),
    b(&[Key("F1")], "controls-help"),
    b(
        &[Key("Alt"), Plus, Words("controls-letter")],
        "controls-meta",
    ),
    b(
        &[Key("Ctrl"), Plus, Words("controls-letter")],
        "controls-ctrl",
    ),
    b(&[Words("controls-usual-key")], "controls-nethack"),
];

/// Modern keys (number_pad on).
const KEYS_MODERN: [Binding; 4] = [
    b(
        &[
            Words("controls-arrows"),
            Or,
            Key("Home"),
            Key("PgUp"),
            Key("End"),
            Key("PgDn"),
            Or,
            Words("controls-keypad"),
        ],
        "controls-move",
    ),
    b(
        &[Key("Shift"), Plus, Words("controls-direction")],
        "controls-run",
    ),
    b(&[Key("n"), Key("2"), Key("0"), Key("s")], "controls-count"),
    b(
        &[Key("k"), Key("j"), Key("l"), Key("u")],
        "controls-number-pad-letters",
    ),
];

/// Classic keys (number_pad off: NetHack's vi-keys).
const KEYS_CLASSIC: [Binding; 3] = [
    b(
        &[
            Key("h"),
            Key("j"),
            Key("k"),
            Key("l"),
            Key("y"),
            Key("u"),
            Key("b"),
            Key("n"),
            Or,
            Words("controls-arrows"),
        ],
        "controls-move",
    ),
    b(
        &[Key("Shift"), Plus, Words("controls-direction")],
        "controls-run",
    ),
    b(
        &[
            Key("Alt"),
            Plus,
            Key("2"),
            Key("Alt"),
            Plus,
            Key("0"),
            Key("s"),
        ],
        "controls-count",
    ),
];

/// The gamepad's bindings.
const PAD: [Binding; 15] = [
    b(
        &[Words("controls-left-stick"), Or, Words("controls-dpad")],
        "controls-pad-walk",
    ),
    b(&[Words("controls-right-stick")], "controls-pad-cursor"),
    b(&[Pad(PadButton::A)], "controls-pad-a"),
    b(&[Pad(PadButton::B)], "controls-pad-b"),
    b(&[Pad(PadButton::X)], "controls-pad-x"),
    b(&[Pad(PadButton::Y)], "controls-pad-y"),
    b(
        &[
            Pad(PadButton::Lb),
            Plus,
            Pad(PadButton::A),
            Pad(PadButton::B),
            Pad(PadButton::X),
            Pad(PadButton::Y),
        ],
        "controls-pad-bar-first",
    ),
    b(
        &[
            Pad(PadButton::Rb),
            Plus,
            Pad(PadButton::A),
            Pad(PadButton::B),
            Pad(PadButton::X),
            Pad(PadButton::Y),
        ],
        "controls-pad-bar-second",
    ),
    b(
        &[
            Pad(PadButton::Lb),
            Or,
            Pad(PadButton::Rb),
            Plus,
            Pad(PadButton::Left),
            Pad(PadButton::Right),
        ],
        "controls-pad-bar-page",
    ),
    b(
        &[Pad(PadButton::Lb), Or, Pad(PadButton::Rb)],
        "controls-pad-bumpers",
    ),
    b(&[Pad(PadButton::Lt)], "controls-pad-radial"),
    b(&[Pad(PadButton::Rt)], "controls-pad-fire"),
    b(&[Pad(PadButton::Start)], "controls-pad-start"),
    b(&[Pad(PadButton::Back)], "controls-pad-history"),
    b(
        &[Pad(PadButton::L3), Or, Pad(PadButton::R3)],
        "controls-pad-sticks",
    ),
];

/// What the controls show: a keyboard profile's keys, or the gamepad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Keys(KeyProfile),
    Pad,
}

impl Shown {
    pub const ALL: [Shown; 3] = [
        Shown::Keys(KeyProfile::Modern),
        Shown::Keys(KeyProfile::Classic),
        Shown::Pad,
    ];

    /// Its bindings, in the order the page lists them.
    pub fn bindings(self) -> Vec<Binding> {
        match self {
            Shown::Keys(KeyProfile::Modern) => {
                KEYS_MODERN.iter().chain(&KEYS_COMMON).copied().collect()
            }
            Shown::Keys(KeyProfile::Classic) => {
                KEYS_CLASSIC.iter().chain(&KEYS_COMMON).copied().collect()
            }
            Shown::Pad => PAD.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_list_says_each_thing_once_and_moving_comes_first() {
        for shown in Shown::ALL {
            let list = shown.bindings();
            assert!(list.len() >= 12, "{shown:?}");
            for (i, b) in list.iter().enumerate() {
                assert!(
                    !list[i + 1..].iter().any(|o| o.action == b.action),
                    "{shown:?}: {} twice",
                    b.action
                );
                assert!(!b.input.is_empty(), "{}", b.action);
            }
        }
        for profile in [KeyProfile::Modern, KeyProfile::Classic] {
            assert_eq!(Shown::Keys(profile).bindings()[0].action, "controls-move");
        }
        // the profiles differ where they should
        let modern = Shown::Keys(KeyProfile::Modern).bindings();
        let classic = Shown::Keys(KeyProfile::Classic).bindings();
        assert!(
            modern
                .iter()
                .any(|b| b.action == "controls-number-pad-letters")
        );
        assert!(
            !classic
                .iter()
                .any(|b| b.action == "controls-number-pad-letters")
        );
    }
}
