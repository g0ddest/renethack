//! The pack as NetHack's `i` lists it (`pack-list`): a debug game set up
//! in English wishes for scrolls of three labels, a potion, a ring and a
//! wand, cursed, uncursed and blessed. In the language asked for, the
//! inventory opens on the grid; the switch shows the list: a heading per
//! class in NetHack's order, then a line an item, its letter and its
//! doname in the language now, every item once. The profile keeps the
//! choice; Ctrl+L and a gamepad's Back/View switch back and forth; the
//! arrows and the d-pad walk the lines, the detail following; a closed
//! and opened panel keeps the list, and so does a question (quaff: the
//! potions suggested). Every picture's words fit the
//! panel, measured in its own pixels whatever the interface's scale.

use std::sync::Mutex;

use nh_world::{Key, KeyInput, Prompt};

use super::{
    DialogEvent, Step, ctrl_key, fail_on_error_screen, key, panel, panel_fits as fits, start, tap,
};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, EngineKind, Lang};
use crate::inventory_panel::{InvInput, VIEW_KEY, class_heading};
use crate::ui_events::UiEvent;

/// The language the test was started in (the game is set up in English:
/// the Russian picker would answer the debug mode's questions).
static ASKED: Mutex<Option<Lang>> = Mutex::new(None);
/// The item selected before a step down.
static BEFORE: Mutex<Option<char>> = Mutex::new(None);

fn asked() -> Lang {
    ASKED.lock().ok().and_then(|l| *l).unwrap_or(Lang::En)
}

fn asking(p: &Prompt, what: &str) -> bool {
    matches!(p, Prompt::Text { query, .. } if query.contains(what))
}

fn wish(what: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", |p| asking(p, "For what do you wish")),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::AnswerUntil(' ', "a command after the wish", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]
}

/// The profile's choice, as the playground's files keep it.
fn kept(g: &RenethackGame) -> bool {
    crate::game::load_profile(&g.playground())
        .pack_list
        .unwrap_or(false)
}

/// The panel shows the list, every item once in the grid's order under
/// its class's heading, each line the item's doname in the language now.
fn list_right(g: &RenethackGame) -> Result<bool, String> {
    let p = panel(g)?;
    if !p.list_view() || p.mode_name().is_none() {
        return Ok(false);
    }
    let lines = p.list_lines();
    // what the grid would show under the filter now (a question's
    // suggestions, or everything)
    let order: Vec<char> = p.shown().iter().map(|(c, _)| *c).collect();
    let listed: Vec<char> = lines.iter().filter_map(|(l, _)| *l).collect();
    if order.is_empty() || listed.len() != order.len() {
        // the list is drawn at the frame's end
        return Ok(false);
    }
    if listed != order {
        return Err(format!("the list's order {listed:?}, the grid's {order:?}"));
    }
    let mut class = None;
    for (i, (letter, words)) in lines.iter().enumerate() {
        match letter {
            None => {
                let next = lines
                    .get(i + 1)
                    .and_then(|(l, _)| *l)
                    .and_then(|l| g.world.inventory.by_letter(l))
                    .ok_or_else(|| format!("the heading {words:?} heads nothing"))?;
                if *words != class_heading(next.class) {
                    return Err(format!("{words:?} heads {:?}", next.text));
                }
                class = Some(next.class);
            }
            Some(l) => {
                let item = g
                    .world
                    .inventory
                    .by_letter(*l)
                    .ok_or_else(|| format!("no item {l}"))?;
                if class != Some(item.class) {
                    return Err(format!("{:?} is not under its class's heading", item.text));
                }
                let said = i18n::engine(EngineKind::Name, &item.text).into_owned();
                if *words != said {
                    return Err(format!("line {l}: {words:?}, not {said:?}"));
                }
            }
        }
    }
    Ok(true)
}

fn selected(g: &RenethackGame) -> Option<char> {
    panel(g).ok().and_then(|p| p.selected())
}

fn mark_selected() -> Step {
    Step::Call("the line before", |g| {
        *BEFORE.lock().map_err(|e| e.to_string())? = selected(g);
        Ok(())
    })
}

fn moved_down(g: &RenethackGame) -> Result<bool, String> {
    let before = *BEFORE.lock().map_err(|e| e.to_string())?;
    let p = panel(g)?;
    let order: Vec<char> = p.shown().iter().map(|(c, _)| *c).collect();
    let want = match before {
        Some(b) => order
            .iter()
            .position(|c| *c == b)
            .and_then(|i| order.get(i + 1))
            .copied(),
        None => order.first().copied(),
    };
    Ok(want.is_some() && selected(g) == want)
}

pub(super) fn pack_list() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, set up in English", |g| {
        *ASKED.lock().map_err(|e| e.to_string())? = Some(i18n::lang());
        g.debug_mode = true;
        g.seed = Some(7);
        g.push_ui(UiEvent::SetLanguage(Lang::En));
        Ok(())
    })];
    steps.extend(start());
    for w in [
        "2 cursed scrolls of teleportation",
        "uncursed scroll of light",
        "3 blessed scrolls of remove curse",
        "blessed potion of full healing",
        "uncursed ring of slow digestion",
        "wand of digging",
    ] {
        steps.extend(wish(w));
    }
    steps.extend([
        Step::Call("the language asked for", |g| {
            g.push_ui(UiEvent::SetLanguage(asked()));
            Ok(())
        }),
        // nothing chosen: Classic keys (the scenarios' own) show the list
        Step::Wait("the list at first, for Classic keys, none chosen", |g| {
            let none = crate::game::load_profile(&g.playground())
                .pack_list
                .is_none();
            Ok(panel(g)?.list_view() && none)
        }),
        // the grid chosen, and kept
        Step::Push(UiEvent::Inventory(InvInput::View(false))),
        Step::Wait("the grid chosen, the profile told", |g| {
            let chosen = crate::game::load_profile(&g.playground()).pack_list;
            Ok(!panel(g)?.list_view() && chosen == Some(false))
        }),
        key('i'),
        Step::Wait("the panel in browse mode on the grid", |g| {
            let p = panel(g)?;
            Ok(p.mode_name() == Some("browse")
                && p.shown().len() == g.world.inventory.items().len())
        }),
        Step::Shot("pack-grid"),
        fits("the grid's words fit"),
        // the switch: the list
        Step::Push(UiEvent::Inventory(InvInput::View(true))),
        Step::Wait("the list, in the pack's order, under its classes", |g| {
            Ok(list_right(g)? && panel(g)?.shown().len() == g.world.inventory.items().len())
        }),
        Step::Wait("the profile keeps the list", |g| Ok(kept(g))),
        Step::Shot("pack-list"),
        fits("the list's words fit"),
        // the arrows walk the lines, the detail follows
        mark_selected(),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("↓: the next line", moved_down),
        // Ctrl+L: the grid, and the list again
        Step::Key(VIEW_KEY),
        Step::Wait("Ctrl+L: the grid, the profile told", |g| {
            Ok(!panel(g)?.list_view() && !kept(g))
        }),
        Step::Key(VIEW_KEY),
        Step::Wait("Ctrl+L again: the list", |g| {
            Ok(panel(g)?.list_view() && kept(g))
        }),
        // a gamepad: the d-pad walks, Back/View switches
        mark_selected(),
        tap(PadButton::Down),
        Step::Wait("the d-pad: the next line", moved_down),
        tap(PadButton::Back),
        Step::Wait("Back: the grid", |g| Ok(!panel(g)?.list_view())),
        Step::Shot("pack-grid-pad"),
        tap(PadButton::Back),
        Step::Wait("Back again: the list", |g| {
            Ok(panel(g)?.list_view() && kept(g))
        }),
        Step::Shot("pack-list-pad"),
        fits("the list's words fit under a pad"),
        // closed and opened: still the list
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed", |g| Ok(!panel(g)?.is_open())),
        Step::Wait("at the command prompt", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        key('i'),
        Step::Wait("opened again on the list", list_right),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("the panel closed again", |g| Ok(!panel(g)?.is_open())),
        Step::Wait("at the command prompt", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // a question: the list in selection mode
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::Wait("the question in the list", |g| {
            let p = panel(g)?;
            Ok(p.mode_name() == Some("select") && list_right(g)?)
        }),
        Step::Shot("pack-list-select"),
        fits("the question's words fit"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::AnswerUntil(' ', "a command after no drink", |g| {
            fail_on_error_screen(g)?;
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]);
    steps
}
