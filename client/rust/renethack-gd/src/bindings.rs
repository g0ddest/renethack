//! What the keys and the gamepad do, as the settings page's controls show
//! them, read only: the gamepad's in the controller's own button names,
//! and what stays as it is on the keyboard (the commands a player moves
//! are `rebind.rs`'s). Each binding is an input drawn as key caps, buttons
//! and words, and what it does.

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
}

/// An input and what it does (a key of the catalogs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub input: &'static [Part],
    pub action: &'static str,
}

use Part::{Key, Or, Pad, Plus, Words};

const fn b(input: &'static [Part], action: &'static str) -> Binding {
    Binding { input, action }
}

/// What stays as it is on the keyboard, under the commands a player moves
/// (`rebind.rs`).
const KEYS_FIXED: [Binding; 6] = [
    b(
        &[Key("Shift"), Plus, Words("controls-direction")],
        "controls-run",
    ),
    b(&[Words("controls-keypad")], "controls-keypad-moves"),
    b(&[Words("controls-left-click")], "controls-click"),
    b(
        &[Words("controls-right-click")],
        "controls-right-click-does",
    ),
    b(&[Words("controls-wheel")], "controls-zoom"),
    b(
        &[Words("controls-any-key"), Or, Key("Esc")],
        "controls-stop",
    ),
];

/// Classic's count: Alt and the top-row digits (Modern's `n` moves).
const KEYS_CLASSIC_FIXED: [Binding; 1] = [b(
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
)];

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

    /// Its bindings that stay as they are, in the order the page lists
    /// them: the gamepad's all, the keyboard's under the commands a player
    /// moves.
    pub fn bindings(self) -> Vec<Binding> {
        match self {
            Shown::Keys(KeyProfile::Modern) => KEYS_FIXED.to_vec(),
            Shown::Keys(KeyProfile::Classic) => KEYS_FIXED
                .iter()
                .chain(&KEYS_CLASSIC_FIXED)
                .copied()
                .collect(),
            Shown::Pad => PAD.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_list_says_each_thing_once() {
        for shown in Shown::ALL {
            let list = shown.bindings();
            assert!(!list.is_empty(), "{shown:?}");
            for (i, b) in list.iter().enumerate() {
                assert!(
                    !list[i + 1..].iter().any(|o| o.action == b.action),
                    "{shown:?}: {} twice",
                    b.action
                );
                assert!(!b.input.is_empty(), "{}", b.action);
            }
        }
        assert!(Shown::Pad.bindings().len() >= 12);
        // Classic counts with Alt; Modern's n is a command the player moves
        assert!(
            Shown::Keys(KeyProfile::Classic)
                .bindings()
                .iter()
                .any(|b| b.action == "controls-count")
        );
    }
}
