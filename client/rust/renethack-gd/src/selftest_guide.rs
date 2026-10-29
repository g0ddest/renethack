//! The first-steps guide's hint cards: `guide` plays a debug game with the
//! guide on, staging each card's occasion in turn (the others passed by,
//! `RenethackGame::hint_only`). The pet's card comes at the start, under
//! the portrait block, clear of the HUD and the hero; Esc, a click and the
//! gamepad's B put a card away. The level mapped (^F) shows stairs down and
//! a door; a monster made (^G) a creature; a dagger dropped and looked at
//! an item here; a wished scroll an unknown item; a potion of gain level a
//! new level; a fountain, a trap and an altar drawn beside the hero (the
//! map's own cells; the hero's hides what is under them), a shopkeeper's
//! welcome and a skill's line (as weapon.c says it: the log shows it in
//! the language now, the card goes by the engine's English), hunger,
//! wounds and few hit points (the status) theirs. A card waits
//! while a question is up (the engine's O); the guide turned off shows
//! none, turned on again the one waiting. Each card once: staged again,
//! none. The help's Guide lists every card met.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use godot::classes::{Engine, Input, InputEventMouseButton};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_protocol::{FmtArg, WinCall};
use nh_world::{Guide, Key, KeyInput, Prompt};

use super::audio_tests::wish;
use super::{PadEv, Step, command, ctrl_key, key, letter_of, quit, set_status, start};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::guide_view::QuestStep;
use crate::i18n::Lang;
use crate::tr;
use crate::ui_events::{DialogEvent, UiEvent};

/// The card staged now (its place in `Guide::CARDS`), and a frame to wait
/// from.
static WANT: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicI64 = AtomicI64::new(0);
/// "Your quest"'s layouts counted at the screen's step.
static LAYOUTS: AtomicUsize = AtomicUsize::new(0);

fn wanted() -> Guide {
    Guide::CARDS[WANT.load(Ordering::Relaxed)]
}

/// Stage `card`: its occasion alone is looked for.
macro_rules! stage {
    ($card:expr) => {
        Step::Call(concat!("stage ", stringify!($card)), |g| {
            let card = $card;
            let i = Guide::CARDS
                .iter()
                .position(|&c| c == card)
                .ok_or("not a card")?;
            WANT.store(i, Ordering::Relaxed);
            g.hint_only = Some(card);
            Ok(())
        })
    };
}

/// A left click through Godot's input at `at` on the canvas, as the
/// player's mouse would.
fn click(g: &RenethackGame, at: Vector2) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    for pressed in [true, false] {
        let mut ev = InputEventMouseButton::new_gd();
        ev.set_button_index(MouseButton::LEFT);
        ev.set_position(at);
        ev.set_global_position(at);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
    Ok(())
}

/// The card staged is shown, with its title, and kept as met.
fn card(g: &RenethackGame) -> Result<bool, String> {
    let want = wanted();
    let ui = g.ui.as_ref().ok_or("no UI")?;
    match ui.hint.shown() {
        Some(c) if c == want => {
            let (title, body) = ui.hint.words();
            let (t, b) = g.guide_words(want);
            if title != t || body != b {
                return Err(format!("the card says {title:?} {body:?}"));
            }
            Ok(g.guide_seen.contains(&want))
        }
        Some(other) => Err(format!("{other:?} shown, not {want:?}")),
        None => Ok(false),
    }
}

/// No card, not even one going.
fn gone(g: &RenethackGame) -> Result<bool, String> {
    Ok(g.ui.as_ref().ok_or("no UI")?.hint.idle())
}

fn mark_frame(_: &mut RenethackGame) -> Result<(), String> {
    FRAME.store(
        Engine::singleton().get_process_frames() as i64,
        Ordering::Relaxed,
    );
    Ok(())
}

/// A dozen frames after the mark, still no card.
fn none_for_a_while(g: &RenethackGame) -> Result<bool, String> {
    if let Some(c) = g.ui.as_ref().ok_or("no UI")?.hint.shown() {
        return Err(format!("{c:?} shown"));
    }
    let frame = Engine::singleton().get_process_frames() as i64;
    Ok(frame >= FRAME.load(Ordering::Relaxed) + 12)
}

/// A message as the engine says it.
fn say(g: &mut RenethackGame, text: &str) {
    say_from(g, text, None, Vec::new());
}

/// A message as the engine's host sends one: its words, and the format
/// and arguments it was made of (the key of its translation).
fn say_from(g: &mut RenethackGame, text: &str, fmt: Option<&str>, args: Vec<FmtArg>) {
    let win = g.world.message_win.unwrap_or(1);
    g.world.apply(&WinCall::Putstr {
        win,
        attr: 0,
        text: text.to_string(),
        fmt: fmt.map(str::to_string),
        args,
    });
}

/// A feature drawn on the map beside the hero (`dx` cells east), as the
/// map shows one seen: the hero's own cell hides what is under them.
fn draw_beside(g: &mut RenethackGame, sym: &str, dx: i32) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    let c = cat
        .cmap
        .iter()
        .find(|c| c.sym == sym)
        .ok_or_else(|| format!("no {sym} in the catalog"))?;
    let (x, y) = g.world.map.hero().ok_or("no hero")?;
    let glyph = nh_protocol::Glyph {
        glyph: Some(c.idx),
        ch: c.ch,
        color: 0,
        flags: 0,
        tile: 0,
        kind: nh_protocol::GlyphKind::Cmap,
        mon: None,
        cmap: Some(c.idx),
        level: None,
    };
    g.world.map.print(x + dx, y, &glyph, None);
    Ok(())
}

fn esc() -> Step {
    Step::Key(KeyInput::plain(Key::Escape))
}

/// Wait for the card staged, then Esc.
fn shown_then_esc(what: &'static str) -> [Step; 3] {
    [Step::Wait(what, card), esc(), Step::Wait("put away", gone)]
}

pub(super) fn guide() -> Vec<Step> {
    let mut steps = vec![
        Step::Call("the guide on, debug mode", |g| {
            g.guide_in_test = true;
            g.debug_mode = true;
            Ok(())
        }),
        stage!(Guide::Pet),
    ];
    steps.extend(start());
    // "Your quest" first: the goal, the screen, moving and fighting; a
    // click on the page goes on, any key too (Enter, the gamepad's A, a
    // letter), ← and Backspace back, Esc skips
    steps.extend([
        Step::Wait("Your quest, its goal", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Goal) {
                return Ok(false);
            }
            let (title, lines) = ui.quest.words();
            Ok(title == tr!("guide-quest-title")
                && lines.first() == Some(&tr!("guide-quest"))
                && g.guide_seen.contains(&Guide::Quest)
                && ui.hint.shown().is_none())
        }),
        Step::Wait("the welcome agreeing with the hero", super::welcome_agrees),
        Step::Shot("guide-quest-goal"),
        Step::Call("a click on the page's words", |g| {
            let r = g.ui.as_ref().ok_or("no UI")?.quest.rect().ok_or("no page")?;
            click(g, r.position + Vector2::new(r.size.x / 2.0, 30.0))
        }),
        Step::Wait("the screen, each block named beside it", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Screen) {
                return Ok(false);
            }
            let tags = ui.quest.tags();
            let blocks = ui.hud.blocks();
            for name in [
                "portrait",
                "mode badge",
                "HP orb",
                "Pw orb",
                "minimap",
                "micro-buttons",
                "action bar",
                "log",
            ] {
                if !blocks.iter().any(|(b, _)| *b == name) {
                    return Err(format!("no {name} on the HUD"));
                }
            }
            // the fight's banner and badge, the whole level's key as bound,
            // and the tips the mouse brings
            let (_, lines) = ui.quest.words();
            let said = |w: &str| lines.iter().any(|l| l.contains(w));
            let overview = crate::rebind::bound_hint(g.keymap(), "F8");
            for w in [tr!("mode-combat-badge"), overview, tr!("quest-screen-tips")] {
                if !said(&w) {
                    return Err(format!("the screen's words lack {w:?}: {lines:?}"));
                }
            }
            // each name clear of every block, at every size and language
            // (once laid out for the blocks where they are now)
            if ui.quest.moved(&blocks) {
                return Ok(false);
            }
            let canvas = Rect2::new(Vector2::ZERO, g.canvas_size()).grow(1.0);
            super::quest_names_clear(ui, canvas)?;
            Ok(tags.len() == 8
                && [
                    tr!("quest-tour-log"),
                    tr!("quest-tour-mode"),
                    tr!("quest-tour-buttons"),
                ]
                .iter()
                .all(|w| tags.iter().any(|(t, _)| t == w)))
        }),
        Step::Call("", |g| {
            let n = g.ui.as_ref().ok_or("no UI")?.quest.layouts();
            LAYOUTS.store(n, Ordering::Relaxed);
            mark_frame(g)
        }),
        Step::Wait("laid out once, not every frame", |g| {
            let n = g.ui.as_ref().ok_or("no UI")?.quest.layouts();
            if n > LAYOUTS.load(Ordering::Relaxed) + 1 {
                return Err(format!("laid out {n} times"));
            }
            Ok(Engine::singleton().get_process_frames() as i64 > FRAME.load(Ordering::Relaxed) + 12)
        }),
        Step::Shot("guide-quest-screen"),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Wait("moving and fighting, the keys as bound", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if ui.quest.step() != Some(QuestStep::Moves) {
                return Ok(false);
            }
            let (_, lines) = ui.quest.words();
            let steps = tr!("quest-steps-classic");
            Ok(lines.first().is_some_and(|l| l.contains(&steps)) && lines.len() == 5)
        }),
        Step::Shot("guide-quest-moves"),
        Step::Key(KeyInput::plain(Key::Left)),
        Step::Wait("back to the screen", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.quest.step() == Some(QuestStep::Screen))
        }),
        Step::Pad(vec![
            PadEv::Button(PadButton::A, true),
            PadEv::Button(PadButton::A, false),
        ]),
        Step::Wait("on again with A; no card while the page is up", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if !g.hint_log.is_empty() {
                return Err(format!("cards under the page: {:?}", g.hint_log));
            }
            Ok(ui.quest.step() == Some(QuestStep::Moves))
        }),
        Step::Key(KeyInput::plain(Key::Backspace)),
        Step::Wait("back with Backspace", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.quest.step() == Some(QuestStep::Screen))
        }),
        key('j'),
        Step::Wait("on with any key", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.quest.step() == Some(QuestStep::Moves))
        }),
        esc(),
        Step::Wait("skipped: the game", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.quest.is_open())
        }),
        Step::Wait("the pet's card", card),
        Step::Wait("the card come in", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.hint.in_full())
        }),
        Step::Shot("guide-pet"),
        esc(),
        Step::Wait("put away with Esc", gone),
        // the level mapped: stairs down (put away with a click), a door
        // (with the gamepad's B)
        stage!(Guide::StairsDown),
        Step::Key(ctrl_key('f')),
        Step::Request("a command after the mapping", command),
        Step::Wait("the stairs' card", card),
        Step::Call("a click on it", |g| {
            let r = g.ui.as_ref().ok_or("no UI")?.hint.rect().ok_or("no card")?;
            click(g, r.position + r.size / 2.0)
        }),
        Step::Wait("put away with a click", gone),
        stage!(Guide::Door),
        Step::Call("looked again", |g| {
            g.check_hints();
            Ok(())
        }),
        Step::Wait("the door's card", card),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("put away with B", gone),
        // a creature
        stage!(Guide::Monster),
        Step::Key(ctrl_key('g')),
        Step::Request("the kind of monster to make", |p| {
            matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster"))
        }),
        Step::Dialog(DialogEvent::TextSubmitted("newt".into())),
        Step::Request("a command after the monster", command),
    ]);
    steps.extend(shown_then_esc("the creature's card"));
    // a dagger dropped and looked at
    steps.extend([
        stage!(Guide::ItemHere),
        key('d'),
        Step::Request("what to drop", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the dagger", |g| {
            letter_of(g, "dagger")
                .map(|c| KeyInput::plain(Key::Char(c)))
                .ok_or_else(|| "no dagger".to_string())
        }),
        Step::Request("a command after the drop", command),
        key(':'),
        Step::Request("a command after the look", command),
    ]);
    steps.extend(shown_then_esc("the card of a thing here"));
    // a scroll whose kind the hero does not know
    steps.push(stage!(Guide::Unidentified));
    steps.extend(wish("uncursed scroll"));
    steps.extend(shown_then_esc("the unknown thing's card"));
    // a new level
    steps.push(stage!(Guide::LevelUp));
    steps.extend(wish("uncursed potion of gain level"));
    steps.extend([
        key('q'),
        Step::Request("what to drink", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the potion", |g| {
            letter_of(g, "potion")
                .map(|c| KeyInput::plain(Key::Char(c)))
                .ok_or_else(|| "no potion".to_string())
        }),
        Step::Request("a command after the drink", command),
    ]);
    steps.extend(shown_then_esc("the new level's card"));
    // a fountain beside the hero
    steps.extend([
        stage!(Guide::Fountain),
        Step::Call("a fountain drawn beside the hero", |g| {
            draw_beside(g, "S_fountain", -1)?;
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the fountain's card"));
    // a trap beside the hero, as the map would show a seen one
    steps.extend([
        stage!(Guide::Trap),
        Step::Call("a trap drawn beside the hero", |g| {
            draw_beside(g, "S_arrow_trap", 1)?;
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the trap's card"));
    // a shopkeeper's welcome
    steps.extend([
        stage!(Guide::Shop),
        Step::Call("a shopkeeper's welcome", |g| {
            say(g, "Hello, Hero!  Welcome to Asidonhopo's general store!");
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the shop's card"));
    // the status: wounds, then few hit points, then hunger
    steps.extend([
        stage!(Guide::Wounded),
        Step::Call("wounded", |g| {
            set_status(g, "hp", "10", None);
            set_status(g, "hpmax", "16", None);
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the wound's card"));
    steps.extend([
        stage!(Guide::Pray),
        Step::Call("few hit points left", |g| {
            set_status(g, "hp", "2", None);
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the prayer's card"));
    steps.extend([
        stage!(Guide::Hungry),
        Step::Call("hungry", |g| {
            set_status(g, "hunger", "Hungry", None);
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the hunger's card"));
    // a question up: the card waits hidden until it is answered
    steps.extend([
        stage!(Guide::Enhance),
        key('O'),
        Step::Request("the engine's options", |p| matches!(p, Prompt::Menu { .. })),
        Step::Call("a skill can improve, the options up", |g| {
            // as weapon.c's give_may_advance_msg() says it
            say_from(
                g,
                "You feel more confident in your weapon skills.",
                Some("You feel more confident in your %sskills."),
                vec![FmtArg::Str("weapon ".into())],
            );
            g.check_hints();
            mark_frame(g)
        }),
        Step::Wait("no card while the question is up", none_for_a_while),
        Step::Dialog(DialogEvent::MenuCancel),
        Step::Request("a command", command),
        // the log shows the line in the language now; the card goes by
        // the engine's English all the same
        Step::Wait("the skill's line in the language shown", |g| {
            let shown = g.ui.as_ref().ok_or("no UI")?.hud.log_shown();
            Ok(match crate::i18n::lang() {
                Lang::Ru => {
                    shown.contains("Вы чувствуете себя увереннее в обращении с оружием.")
                        && !shown.contains("more confident")
                }
                Lang::En | Lang::Pseudo => shown.contains("more confident in your weapon skills."),
            })
        }),
    ]);
    steps.extend(shown_then_esc("the skill's card, after the question"));
    // the guide off: no card; on again: the one waiting
    steps.extend([
        stage!(Guide::Altar),
        Step::Call("the guide off", |g| {
            let mut s = g.settings.clone();
            s.hints_off = true;
            g.set_settings(s);
            Ok(())
        }),
    ]);
    steps.extend([
        Step::Call("an altar drawn beside the hero", |g| {
            draw_beside(g, "S_altar", 2)?;
            g.check_hints();
            mark_frame(g)
        }),
        Step::Wait("no card with the guide off", none_for_a_while),
        Step::Call("the guide on again", |g| {
            let mut s = g.settings.clone();
            s.hints_off = false;
            g.set_settings(s);
            g.check_hints();
            Ok(())
        }),
    ]);
    steps.extend(shown_then_esc("the altar's card"));
    // each card once: a creature again, and its card no more
    steps.extend([
        stage!(Guide::Monster),
        Step::Key(ctrl_key('g')),
        Step::Request("the kind of monster to make", |p| {
            matches!(p, Prompt::Text { query, .. } if query.contains("Create what kind of monster"))
        }),
        Step::Dialog(DialogEvent::TextSubmitted("newt".into())),
        Step::Request("a command after the monster", command),
        Step::Call("", mark_frame),
        Step::Wait("no card again", none_for_a_while),
        Step::Call("every card once", |g| {
            let mut seen = g.hint_log.clone();
            seen.sort();
            let n = seen.len();
            seen.dedup();
            if seen.len() != n {
                return Err(format!("a card twice: {:?}", g.hint_log));
            }
            if n != Guide::CARDS.len() {
                return Err(format!("{n} cards shown, not {}: {:?}", Guide::CARDS.len(), g.hint_log));
            }
            Ok(())
        }),
        // the help's Guide: every card met, to read again
        Step::Push(UiEvent::ToggleHelp),
        Step::Wait("the Guide first in the help", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let Some((titles, _, _)) = ui.help.view() else {
                return Ok(false);
            };
            if titles.first() != Some(&tr!("help-guide")) {
                return Ok(false);
            }
            for card in Guide::CARDS {
                let (title, _) = g.guide_words(card);
                if !titles.contains(&title) {
                    return Err(format!("the Guide lacks {title:?}"));
                }
            }
            if titles.get(1) != Some(&tr!("guide-quest-title")) {
                return Err(format!("the Guide's first page is {:?}", titles.get(1)));
            }
            Ok(true)
        }),
        Step::Shot("guide-help"),
        // "Your quest" whole: the goal, the screen's names, the moves
        Step::Push(UiEvent::Help(crate::help_panel::HelpInput::Row(1))),
        Step::Wait("Your quest to read again", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let text = ui.help.shown_text();
            // moving and fighting for the input the player last used: the
            // gamepad put the door's card away
            let moves = if g.pad.active {
                tr!("quest-moves-pad-walk")
            } else {
                tr!("quest-moves-click")
            };
            Ok([
                tr!("guide-quest"),
                tr!("quest-screen-title"),
                tr!("quest-tour-log"),
                moves,
            ]
            .iter()
            .all(|w| text.contains(w.as_str())))
        }),
        Step::Shot("guide-help-quest"),
        Step::Push(UiEvent::ToggleHelp),
        Step::Wait("the help closed", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.help.is_open())
        }),
    ]);
    steps.extend(quit());
    steps
}
