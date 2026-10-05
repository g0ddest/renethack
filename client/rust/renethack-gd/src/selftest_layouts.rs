//! The engine's pictures and tables the client lays out from their English
//! (`layouts`): `layouts` plays a debug-mode game that wishes for two
//! scrolls of genocide, makes two jackals and a newt and wipes both kinds
//! out. In the language the test was started in, #vanquished then lists
//! the jackals and the newts, #genocided the two species, and #overview the
//! level teleported to with the player's note on it. Last, #wizkill on the
//! hero's own square ends the game: the end's overview has where the hero
//! lies, and the end screen the drawn tombstone.

use std::sync::Mutex;

use nh_world::Prompt;

use super::{
    DialogEvent, Step, command, ctrl_key, fail_on_error_screen, key, letter_of, logged, screen,
    start,
};
use crate::game::RenethackGame;
use crate::i18n::{self, Lang};
use crate::layouts;
use crate::tr;
use crate::ui_events::UiEvent;

/// The language the test was started in: the game is set up in English
/// (the Russian pickers would answer the debug mode's questions), and the
/// views are seen in it.
static ASKED: Mutex<Option<Lang>> = Mutex::new(None);

/// The player's note on the level teleported to.
const NOTE: &str = "jackal den";

fn asked() -> Lang {
    ASKED.lock().ok().and_then(|l| *l).unwrap_or(Lang::En)
}

fn asking(p: &Prompt, what: &str) -> bool {
    matches!(p, Prompt::Text { query, .. } if query.contains(what))
}

/// An extended command chosen in the palette.
fn extended(name: &'static str) -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
    ]
}

/// A monster of debug mode's ^G.
fn create(name: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('g')),
        Step::Request("the kind of monster to create", |p| {
            asking(p, "Create what kind of monster")
        }),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
        Step::Request("a command after the monster", command),
    ]
}

/// A scroll of genocide read for `name`; `wiped` holds once the engine
/// says they are gone.
fn genocide(name: &'static str, what: &'static str, wiped: super::Check) -> Vec<Step> {
    vec![
        key('r'),
        Step::Request("what to read", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the scroll", |g| {
            let c = letter_of(g, "scroll").ok_or("no scroll")?;
            Ok(nh_world::KeyInput::plain(nh_world::Key::Char(c)))
        }),
        Step::Request("the genocide's question", |p| asking(p, "genocide")),
        Step::Dialog(DialogEvent::TextSubmitted(name.into())),
        Step::AnswerUntil('n', what, wiped),
        Step::Request("a command after the genocide", command),
    ]
}

/// The lines of the text window the engine waits on, as it wrote them.
fn shown_lines(g: &RenethackGame) -> Option<Vec<String>> {
    match g.pending.as_ref() {
        Some((_, Prompt::Show { lines, .. })) => {
            Some(lines.iter().map(|l| l.text.clone()).collect())
        }
        _ => None,
    }
}

/// The open dialog lays out `layout` and shows `text` (as its label).
fn laid_out(g: &RenethackGame, layout: &str, text: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.layout_name() != Some(layout) {
        return Ok(false);
    }
    let texts = ui.dialogs.panel_texts();
    if texts.iter().any(|t| t == text) {
        Ok(true)
    } else {
        Err(format!("no {text:?} in the {layout}: {texts:?}"))
    }
}

pub(super) fn layouts() -> Vec<Step> {
    let mut steps = vec![Step::Call(
        "debug mode, set up in English",
        |g: &mut RenethackGame| {
            *ASKED.lock().map_err(|e| e.to_string())? = Some(i18n::lang());
            g.debug_mode = true;
            g.seed = Some(7);
            g.push_ui(UiEvent::SetLanguage(Lang::En));
            Ok(())
        },
    )];
    steps.extend(start());
    steps.extend([
        Step::Key(ctrl_key('w')),
        Step::Request("the wish", |p| asking(p, "For what do you wish")),
        Step::Dialog(DialogEvent::TextSubmitted(
            "2 uncursed scrolls of genocide".into(),
        )),
        Step::Request("a command after the wish", command),
    ]);
    for name in ["jackal", "jackal", "newt"] {
        steps.extend(create(name));
    }
    steps.extend(genocide("jackal", "the jackals wiped out", |g| {
        fail_on_error_screen(g)?;
        Ok(logged(g, "Wiped out all jackals"))
    }));
    steps.extend(genocide("newt", "the newts wiped out", |g| {
        fail_on_error_screen(g)?;
        Ok(logged(g, "Wiped out all newts"))
    }));
    steps.push(Step::Call("the language asked for", |g| {
        g.push_ui(UiEvent::SetLanguage(asked()));
        Ok(())
    }));
    steps.extend(extended("vanquished"));
    steps.extend([
        Step::Request("the vanquished", |p| matches!(p, Prompt::Show { .. })),
        Step::Wait("the jackals and the newts laid out", |g| {
            let total = shown_lines(g)
                .and_then(|l| layouts::vanquished(&l, &|_| None))
                .and_then(|v| v.total)
                .ok_or("no total of the vanquished")?;
            Ok(laid_out(g, "vanquished", &layouts::name("jackal"))?
                && laid_out(g, "vanquished", &layouts::name("newt"))?
                && laid_out(g, "vanquished", &tr!("vanquished-total", count = total))?)
        }),
        Step::Shot("vanquished"),
        Step::Dialog(DialogEvent::Close),
        Step::Request("a command after the vanquished", command),
    ]);
    steps.extend(extended("genocided"));
    steps.extend([
        Step::Request("the genocided", |p| matches!(p, Prompt::Show { .. })),
        Step::Wait("the two species laid out", |g| {
            Ok(laid_out(g, "genocided", &layouts::name("jackals"))?
                && laid_out(g, "genocided", &tr!("genocided-total", count = 2))?)
        }),
        Step::Shot("genocided"),
        Step::Dialog(DialogEvent::Close),
        Step::Request("a command after the genocided", command),
        // to the third level, seen whole
        Step::Key(ctrl_key('v')),
        Step::Request("where to teleport", |p| asking(p, "teleport")),
        Step::Dialog(DialogEvent::TextSubmitted("3".into())),
        Step::AnswerUntil('n', "the hero on level 3", |g| {
            fail_on_error_screen(g)?;
            let on_3 = g.world.status.number("leveldesc") == Some(3);
            Ok(on_3 && g.pending.as_ref().is_some_and(|(_, p)| command(p)))
        }),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the map", command),
    ]);
    steps.extend(extended("annotate"));
    steps.extend([
        Step::Request("the level's note", |p| matches!(p, Prompt::Text { .. })),
        Step::Dialog(DialogEvent::TextSubmitted(NOTE.into())),
        Step::Request("a command after the note", command),
    ]);
    steps.extend(extended("overview"));
    steps.extend([
        // a menu of no choice: the host sends it as a text window
        Step::Request("the overview", |p| {
            matches!(p, Prompt::Show { .. } | Prompt::Menu { .. })
        }),
        Step::Wait("the levels and the note laid out", |g| {
            Ok(laid_out(g, "overview", &layouts::note(NOTE))?
                && laid_out(g, "overview", &layouts::here(layouts::Here::Are))?)
        }),
        Step::Shot("overview"),
        Step::Dialog(DialogEvent::Close),
        Step::Request("a command after the overview", command),
    ]);
    // the hero slain by their own player: the tombstone (the palette
    // lists no command of debug mode)
    steps.extend([
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        // the game's first getpos shows its tip
        Step::Request("the getpos tip", |p| matches!(p, Prompt::Show { .. })),
        Step::Dialog(DialogEvent::Close),
        Step::Request("the monster to slay", command),
        key('.'),
        Step::Request("the suicide's question", |p| asking(p, "suicide")),
        Step::Dialog(DialogEvent::TextSubmitted("yes".into())),
        Step::Request(
            "Die?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Die?")),
        ),
        key('y'),
        // the end's overview: where the hero was, and lies
        Step::AnswerUntil('n', "the question of the overview", |g| {
            fail_on_error_screen(g)?;
            Ok(g.pending.as_ref().is_some_and(|(_, p)| {
                matches!(p, Prompt::Choice { query, .. } if query.contains("dungeon overview"))
            }))
        }),
        key('y'),
        Step::Request("the overview at the end", |p| {
            matches!(p, Prompt::Show { .. } | Prompt::Menu { .. })
        }),
        Step::Wait("the hero's resting place laid out", |g| {
            Ok(
                laid_out(g, "overview", &layouts::here(layouts::Here::Were))?
                    && laid_out(g, "overview", &tr!("overview-resting"))?,
            )
        }),
        Step::Shot("overview-end"),
        Step::Dialog(DialogEvent::Close),
        Step::AnswerUntil('n', "the end screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("end"))
        }),
        Step::Wait("the tombstone drawn", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let summary = ui.screens.end_summary().ok_or("no summary")?;
            let (stone, _) =
                layouts::tombstone(&summary.text).ok_or("no tombstone in the end window")?;
            // debug mode names every hero "wizard"
            if stone.name.is_empty() || !stone.death.contains("own player") {
                return Err(format!("the stone says {stone:?}"));
            }
            let carved = [tr!("rip-rest-in-peace"), layouts::typed(&stone.name)];
            Ok(carved.iter().all(|t| ui.screens.has_label(t)))
        }),
        Step::Shot("tombstone"),
    ]);
    steps
}
