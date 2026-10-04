//! Russian input (localization phase R7): `pickers` plays a debug-mode
//! game in Russian (unless another language than English was asked for)
//! and makes its wishes and its genocide by the pickers: "благословенный
//! +2 длинный меч" typed is a blessed +2 long sword, "непроклятый свиток
//! геноцида" an uncursed scroll of genocide, and read, "кобольды" wipes
//! out the kobolds; a blessed one read, "гномы" picks the class of gnomes;
//! a gamepad builds a wish (X blessed, Y +2, RB weapons, ↓, A); a wish
//! typed in English instead still works; an engraving typed in Cyrillic is
//! written in Latin letters, "Элберет" as "Elbereth".

use nh_world::{Key, KeyInput, Prompt};

use super::{
    Check, DialogEvent, Step, command, ctrl_key, fail_on_error_screen, key, letter_of, logged,
    open_menu, quit, smudged, start, tap, unmarked,
};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, Lang};
use crate::ui_events::UiEvent;

/// The open dialog is a picker listing something.
fn picker_open(g: &RenethackGame) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(ui.dialogs.kind_name() == Some("picker")
        && ui.dialogs.pick_list().is_some_and(|l| !l.is_empty()))
}

/// What the picker would send is `want`.
fn picks(g: &RenethackGame, want: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    match ui.dialogs.pick_reply() {
        Some(r) if r == want => Ok(true),
        Some(r) if !r.is_empty() && ui.dialogs.pick_list().is_some() => {
            Err(format!("the picker would send {r:?}, not {want:?}"))
        }
        _ => Ok(false),
    }
}

fn type_into_picker(g: &mut RenethackGame, text: &str) -> Result<(), String> {
    let ui = g.ui.as_mut().ok_or("no UI")?;
    if ui.dialogs.type_pick(text) {
        Ok(())
    } else {
        Err("no picker is open".into())
    }
}

/// The inventory as debug mode's ^I shows it, everything identified: the
/// check holds while it is open; then closed.
fn identified(what: &'static str, check: Check) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('i')),
        Step::Request("the inventory, identified", |p| {
            matches!(p, Prompt::Menu { .. })
        }),
        Step::Wait(what, check),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Request("a command", command),
    ]
}

/// An entry of the open menu has `what`.
fn listed(g: &RenethackGame, what: &str) -> Result<bool, String> {
    let entries = open_menu(g).ok_or("no menu is open")?;
    if entries.iter().any(|e| unmarked(&e.text).contains(what)) {
        return Ok(true);
    }
    let texts: Vec<String> = entries.iter().map(|e| unmarked(&e.text)).collect();
    Err(format!("no {what:?} in {texts:?}"))
}

fn wishing(p: &Prompt) -> bool {
    matches!(p, Prompt::Text { query, .. } if query.starts_with("For what do you wish"))
}

/// A wish by the picker: `typed` into its search, `english` sent.
fn wish(typed: fn(&mut RenethackGame) -> Result<(), String>, sent: super::Check) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", wishing),
        Step::Wait("the picker of objects", picker_open),
        Step::Call("the wish typed in Russian", typed),
        Step::Wait("the picker has the wish", sent),
    ]
}

pub(super) fn pickers() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode for this game, in Russian",
        |g: &mut RenethackGame| {
            g.debug_mode = true;
            if i18n::lang() == Lang::En {
                g.push_ui(UiEvent::SetLanguage(Lang::Ru));
            }
            Ok(())
        },
    )];
    steps.extend(start());
    steps.extend(wish(
        |g| type_into_picker(g, "благословенный +2 длинный меч"),
        |g| picks(g, "blessed +2 long sword"),
    ));
    steps.extend([
        Step::Shot("picker-wish"),
        Step::Dialog(DialogEvent::PickConfirm),
        Step::Request("a command after the wish", command),
    ]);
    steps.extend(identified("a blessed +2 long sword in the pack", |g| {
        listed(g, "blessed +2 long sword")
    }));
    steps.extend(wish(
        |g| type_into_picker(g, "непроклятый свиток геноцида"),
        |g| picks(g, "uncursed scroll of genocide"),
    ));
    steps.extend([
        Step::Dialog(DialogEvent::PickConfirm),
        Step::Request("a command after the wish", command),
    ]);
    steps.extend(identified(
        "an uncursed scroll of genocide in the pack",
        |g| listed(g, "uncursed scroll of genocide"),
    ));
    steps.extend([
        key('r'),
        Step::Request("what to read", |p| !matches!(p, Prompt::Command)),
        // wished for, it is still unidentified: "a scroll labeled ..."
        Step::KeyFrom("the scroll of genocide", |g| {
            let c = letter_of(g, "scroll").ok_or("no scroll")?;
            Ok(KeyInput::plain(Key::Char(c)))
        }),
        Step::Request(
            "the genocide's question",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("genocide")),
        ),
        Step::Wait("the picker of monsters", picker_open),
        Step::Call("\"кобольды\" typed", |g| {
            type_into_picker(g, "кобольды")
        }),
        Step::Wait("the kobold first", |g| picks(g, "kobold")),
        Step::Shot("picker-genocide"),
        Step::Dialog(DialogEvent::PickConfirm),
        Step::AnswerUntil('n', "the kobolds wiped out", |g| {
            fail_on_error_screen(g)?;
            Ok(logged(g, "Wiped out all kobolds"))
        }),
        Step::Request("a command after the genocide", command),
    ]);
    // a blessed scroll: a class of monsters
    steps.extend(wish(
        |g| type_into_picker(g, "благословенный свиток геноцида"),
        |g| picks(g, "blessed scroll of genocide"),
    ));
    steps.extend([
        Step::Dialog(DialogEvent::PickConfirm),
        Step::Request("a command after the wish", command),
        key('r'),
        Step::Request("what to read", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the scroll of genocide", |g| {
            let c = letter_of(g, "scroll of genocide").ok_or("no scroll of genocide")?;
            Ok(KeyInput::plain(Key::Char(c)))
        }),
        Step::Request(
            "the class genocide's question",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("class of monsters")),
        ),
        Step::Wait("the picker of classes", picker_open),
        Step::Call("\"гномы\" typed", |g| type_into_picker(g, "гномы")),
        Step::Wait("the gnomes' class first", |g| picks(g, "G")),
        Step::Shot("picker-class"),
        Step::Dialog(DialogEvent::PickConfirm),
        Step::AnswerUntil('n', "the gnomes wiped out", |g| {
            fail_on_error_screen(g)?;
            Ok(logged(g, "Wiped out all gnome"))
        }),
        Step::Request("a command after the genocide", command),
        // a wish by a gamepad: blessed, +2, the weapons, the first of them
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", wishing),
        Step::Wait("the picker of objects", picker_open),
        tap(PadButton::X),
        tap(PadButton::Y),
        tap(PadButton::Y),
        tap(PadButton::Rb),
        tap(PadButton::Down),
        Step::Wait("a blessed +2 weapon to wish for", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .dialogs
                .pick_reply()
                .is_some_and(|r| r.starts_with("blessed +2 ")))
        }),
        Step::Shot("picker-pad"),
        tap(PadButton::A),
        // a wish in English, the picker given up
        Step::Request("a command after the wish", command),
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", wishing),
        Step::Wait("the picker of objects", picker_open),
        Step::Dialog(DialogEvent::PickManual),
        Step::Wait("the plain question in its place", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.kind_name() == Some("text"))
        }),
        Step::Dialog(DialogEvent::TextSubmitted("2 uncursed food rations".into())),
        Step::Request("a command after the wish", command),
        // the starting ration is known uncursed: the two join it
        Step::Wait("two uncursed food rations more", |g| {
            Ok(logged(g, "2 uncursed food rations"))
        }),
        // an engraving in Cyrillic goes in Latin letters
        key('E'),
        Step::Request(
            "what to write with",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("write with")),
        ),
        key('-'),
        Step::Request(
            "what to write in the dust",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("write in the dust")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted("Элберет".into())),
        Step::Request("a command after engraving", command),
        key(':'),
        // read back as Elbereth (a fight in the dust may smudge a letter)
        Step::AnswerUntil('n', "the engraving read back as Elbereth", |g| {
            fail_on_error_screen(g)?;
            let read = g.world.log.iter().rev().find_map(|m| {
                let text = m.text.strip_prefix("You read: \"")?;
                Some(text.trim_end_matches(['.', '"']).to_string())
            });
            let waits = g.pending.as_ref().is_some_and(|(_, p)| command(p));
            match read {
                Some(r) if waits && smudged("Elbereth", &r) => Ok(true),
                Some(r) if waits => Err(format!("read back {r:?}")),
                _ => Ok(false),
            }
        }),
    ]);
    steps.extend(quit());
    steps
}
