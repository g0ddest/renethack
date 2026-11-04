//! A gamepad's words, never the keyboard's, while it plays (`pad-words`):
//! the achievements' foot; "Your quest" (its foot, the bar's name, moving
//! and fighting); a hint card (its words and how it goes); the game's
//! menu; a menu of the engine's ("What do you want to do?"); travel's
//! prompt line; a yes/no question (its answers
//! without their letters); the prologue's note. A key pressed, the
//! keyboard's words come back (the game's menu says Esc again).

use godot::global::{JoyAxis, Key as GKey};
use nh_world::{Guide, Key, KeyInput, Prompt};

use super::{PadEv, Step, command, ctrl_key, fail_on_error_screen, screen, smoke_choice, tap};
use crate::game::RenethackGame;
use crate::gamepad::{PadButton, PadCtx};
use crate::guide_view::QuestStep;
use crate::tr;
use crate::ui_events::UiEvent;

/// What only the keyboard or the mouse has: keys, clicks, letters to type.
const KEY_WORDS: [&str; 26] = [
    "Esc",
    "ESC",
    "Enter",
    "PgUp",
    "PgDn",
    "Tab",
    "Space",
    "Пробел",
    "Home",
    "Backspace",
    "Shift",
    "click",
    "Click",
    "щелч",
    "щёлк",
    "Щелч",
    "клавиш",
    "Клавиш",
    "any key",
    "буква",
    "letter",
    "(y)",
    "(n)",
    "(q)",
    "(a)",
    "(k)",
];

/// None of `words` names a key or a click.
fn no_keys(what: &str, words: &[String]) -> Result<(), String> {
    for w in words {
        if let Some(k) = KEY_WORDS.iter().find(|k| w.contains(**k)) {
            return Err(format!("{what} says {k:?} with a gamepad: {w:?}"));
        }
    }
    Ok(())
}

/// The strip shows this screen's buttons.
fn strip_on(g: &RenethackGame, ctx: PadCtx) -> bool {
    g.ui.as_ref()
        .is_some_and(|ui| ui.pad.shown().is_some_and(|(_, c)| c == ctx))
}

fn the_a() -> String {
    crate::gamepad::PadKind::default()
        .label(PadButton::A)
        .to_string()
}

pub(super) fn pad_words() -> Vec<Step> {
    vec![
        Step::Call(
            "the guide on, no card but the one staged, debug mode",
            |g| {
                g.guide_in_test = true;
                g.debug_mode = true;
                g.hint_only = Some(Guide::Altar);
                Ok(())
            },
        ),
        Step::Wait("the title, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        // the achievements, from the title with the pad
        tap(PadButton::Down),
        Step::Wait("down to the achievements", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.screens.nav_focused() == Some(tr!("title-achievements")))
        }),
        tap(PadButton::A),
        Step::Wait("the achievements, a gamepad's foot", |g| {
            if screen(g) != Some("achievements") {
                return Ok(false);
            }
            let page = g.achievement_page.as_ref().ok_or("no page")?;
            Ok(page.pad().is_some())
        }),
        Step::Shot("pad-words-achievements"),
        tap(PadButton::B),
        Step::Wait("the title again", |g| Ok(screen(g) == Some("title"))),
        Step::Push(UiEvent::StartCharacter(smoke_choice())),
        Step::Request("the first command", command),
        // "Your quest", made for the gamepad from its first step
        Step::Wait("Your quest, a gamepad's foot", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Goal) {
                return Ok(false);
            }
            let words = ui.quest.shown_words();
            no_keys("Your quest", &words)?;
            Ok(words.iter().any(|w| *w == the_a()))
        }),
        Step::Shot("pad-words-quest"),
        tap(PadButton::A),
        Step::Wait("the screen: the bar's chords", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Screen) {
                return Ok(false);
            }
            let words = ui.quest.shown_words();
            no_keys("the screen's names", &words)?;
            let bar = g
                .quest_words(QuestStep::Screen)
                .tags
                .into_iter()
                .find(|(b, _)| *b == "action bar")
                .map(|(_, w)| w)
                .ok_or("no bar")?;
            if !bar.contains("LB") {
                return Err(format!("the bar named {bar:?}"));
            }
            // the whole level by the stick
            let l3 = crate::gamepad::PadKind::default().label(PadButton::L3);
            if !words.iter().any(|w| w.contains(l3)) {
                return Err(format!("the whole level's button unsaid: {words:?}"));
            }
            Ok(words.contains(&bar))
        }),
        tap(PadButton::A),
        Step::Wait("moving and fighting, with the gamepad", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Moves) {
                return Ok(false);
            }
            let (_, lines) = ui.quest.words();
            no_keys("moving and fighting", &lines)?;
            no_keys("Your quest", &ui.quest.shown_words())?;
            Ok(lines.first() == Some(&tr!("quest-moves-pad-walk")))
        }),
        Step::Shot("pad-words-moves"),
        tap(PadButton::A),
        Step::Wait("the game", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.quest.is_open())
        }),
        Step::Wait("the prologue's note names the buttons", |g| {
            let note = g.ui.as_ref().ok_or("no UI")?.prologue.hint_words();
            no_keys("the prologue's note", std::slice::from_ref(&note))?;
            Ok(note.contains(&the_a()))
        }),
        // a hint card: the stairs mapped
        Step::Call("stage the stairs' card", |g| {
            g.hint_only = Some(Guide::StairsDown);
            Ok(())
        }),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the mapping", command),
        Step::Wait("the stairs' card, a gamepad's words", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.hint.shown() != Some(Guide::StairsDown) || !ui.hint.in_full() {
                return Ok(false);
            }
            // the gamepad's words, not "press >"
            let (_, body) = ui.hint.words();
            if body != g.guide_words(Guide::StairsDown).1 || body.contains('>') {
                return Err(format!("the card says {body:?}"));
            }
            let words = ui.hint.shown_words();
            no_keys("the card", &words)?;
            Ok(words.contains(&tr!("hint-close")))
        }),
        Step::Shot("pad-words-card"),
        tap(PadButton::B),
        Step::Wait("put away with B", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.hint.idle())
        }),
        // the game's menu
        tap(PadButton::B),
        Step::Wait("the game's menu, a gamepad's foot", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.game_menu.is_open() {
                return Ok(false);
            }
            let words = ui.game_menu.shown_words();
            no_keys("the game's menu", &words)?;
            Ok(words.contains(&tr!("hint-choose")) && ui.pad.shown().is_none())
        }),
        Step::Shot("pad-words-game-menu"),
        tap(PadButton::B),
        Step::Wait("back in the game", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.game_menu.is_open())
        }),
        // A on the hero: what can be done here, a menu of the engine's
        tap(PadButton::A),
        Step::Request("What do you want to do?", |p| {
            matches!(p, Prompt::Menu { .. })
        }),
        Step::Wait("the menu, the strip's buttons", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.dialogs.is_open() {
                return Ok(false);
            }
            no_keys("the menu", &ui.dialogs.shown_words())?;
            Ok(strip_on(g, PadCtx::Pick))
        }),
        Step::Shot("pad-words-menu"),
        tap(PadButton::B),
        Step::Request("a command after the menu", command),
        // travel from the ring: its line, the gamepad's (NetHack's own tip
        // window is off: the guide has its cards)
        Step::Pad(vec![PadEv::Axis(JoyAxis::TRIGGER_LEFT, 1.0)]),
        Step::Pad(vec![PadEv::Axis(JoyAxis::LEFT_X, -1.0)]),
        Step::Pad(vec![PadEv::Axis(JoyAxis::TRIGGER_LEFT, 0.0)]),
        Step::Pad(vec![PadEv::Axis(JoyAxis::LEFT_X, 0.0)]),
        Step::Wait("travel's line, the gamepad's", |g| {
            fail_on_error_screen(g)?;
            if !g.world.getpos || !matches!(g.pending, Some((_, Prompt::Command))) {
                return Ok(false);
            }
            let line =
                g.ui.as_ref()
                    .and_then(|ui| ui.hud.prompt_line())
                    .ok_or("no prompt line")?;
            no_keys("travel's line", std::slice::from_ref(&line))?;
            Ok(line.contains(&the_a()) && strip_on(g, PadCtx::Getpos))
        }),
        Step::Shot("pad-words-travel"),
        tap(PadButton::B),
        Step::Wait("travel let go", |g| {
            Ok(!g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // a yes/no question: giving up, asked
        tap(PadButton::B),
        Step::Wait("the game's menu", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.game_menu.is_open()))
        }),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::Down),
        tap(PadButton::A),
        Step::Request(
            "Really quit?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.starts_with("Really quit")),
        ),
        Step::Wait("the answers without their letters", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.dialogs.is_open() {
                return Ok(false);
            }
            let words = ui.dialogs.shown_words();
            no_keys("the question", &words)?;
            Ok(words.contains(&tr!("dlg-no-pad")) && strip_on(g, PadCtx::Choice))
        }),
        Step::Shot("pad-words-choice"),
        tap(PadButton::B),
        Step::Request("a command after the question", command),
        // a key: the keyboard's words again
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Wait("the game's menu with the keys", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !ui.game_menu.is_open() {
                return Ok(false);
            }
            let words = ui.game_menu.shown_words();
            let keys = crate::i18n::whole_parts(&tr!("game-menu-hint"));
            Ok(!g.pad.active && words.contains(&keys))
        }),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("back in the game", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.game_menu.is_open())
        }),
    ]
}
