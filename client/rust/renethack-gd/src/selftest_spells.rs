//! The spells' panel (`spells`): a Wizard's `+` laid out from the engine's
//! own menu, a spell a row with its school, level, failure and memory, in
//! the language the test runs in; no turn passes. A pick there casts the
//! spell: `+`'s menu is closed and `Z` picks it (the direction's question
//! then comes; let go, the energy is spent all the same, and the casts
//! after it want for energy). `Z`'s panel casts a pick at once. Tab
//! reorders: a pick
//! begins the engine's swap, the swap's panel picks the other spell, and
//! `+`'s comes back with the two exchanged. `+` closes its own panel; a
//! gamepad's A casts the first spell and B closes. Every spell's row shows
//! what the engine's menu says of it. In Russian the HUD's title and
//! subtitle have the Wizard's rank and role in the heroine's gender.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use godot::classes::{Engine, Input, InputEventMouseButton, InputEventMouseMotion};
use godot::global::{MouseButton, MouseButtonMask};
use godot::prelude::*;
use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Prompt, SlotBinding};

use super::{CharacterChoice, DialogEvent, PadEv, Step, fail_on_error_screen, key, quit, start_as};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::spells::{self, Purpose, Spellbook};
use crate::spells_view;
use crate::ui_events::UiEvent;

/// The turn the panel first opened on.
static TURN: AtomicI64 = AtomicI64::new(-1);
/// The spells' names by letter as `+` first listed them, and the first
/// row's spell as the gamepad found it.
static FIRST: Mutex<Vec<(char, String)>> = Mutex::new(Vec::new());
static PAD_FIRST: Mutex<String> = Mutex::new(String::new());

fn wizard() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "wizard".into(),
        race: "human".into(),
        gender: "female".into(),
        align: "neutral".into(),
        profile: KeyProfile::Classic,
    }
}

/// The spells' menu the engine waits on, as it wrote it.
fn book(g: &RenethackGame) -> Option<Spellbook> {
    match g.pending.as_ref() {
        Some((_, Prompt::Menu { title, items, .. })) => spells::spellbook(title.as_deref(), items),
        _ => None,
    }
}

/// The spells' panel is open for `purpose`, each spell's row showing what
/// the engine's menu says of it.
fn shown(g: &RenethackGame, purpose: Purpose, reorder: bool) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.spells_open() != Some((purpose, reorder)) {
        return Ok(false);
    }
    let b = book(g).ok_or("no spells' menu under the panel")?;
    let texts = ui.dialogs.panel_texts();
    for s in &b.spells {
        let want = [
            s.letter.to_string(),
            spells_view::spell_name(&s.name),
            format!("{}%", s.fail),
        ];
        if let Some(missing) = want.iter().find(|w| !texts.contains(w)) {
            return Err(format!("{}'s row shows no {missing:?}: {texts:?}", s.name));
        }
        let school = spells_view::school_name(s.school);
        if !texts.iter().any(|t| t.contains(&school)) {
            return Err(format!("{}'s row names no {school:?}: {texts:?}", s.name));
        }
    }
    Ok(true)
}

/// The engine asks for a direction (the spell is being cast).
fn asked_direction(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(matches!(
        g.pending,
        Some((
            _,
            Prompt::FreeKey {
                directions: true,
                ..
            }
        ))
    ))
}

/// The spell `Z`'s menu was answered with since the answers were cleared.
fn z_picked(g: &RenethackGame) -> Option<String> {
    g.sent.iter().find_map(|(p, r)| match (p, r) {
        (
            Prompt::Menu {
                title: Some(t),
                items,
                ..
            },
            Reply::Menu(picks),
        ) if t == "Choose which spell to cast" => {
            let b = spells::spellbook(Some(t), items)?;
            let idx = picks.first().map(|(i, _)| *i as i32)?;
            b.spell(idx).map(|s| s.name.clone())
        }
        _ => None,
    })
}

/// The answers sent since `+`'s panel picked: its menu closed, `Z`, the
/// cast menu's pick of `name`.
fn cast_through_z(g: &RenethackGame, name: &str) -> Result<bool, String> {
    let sent: Vec<&(Prompt, Reply)> = g.sent.iter().collect();
    let closed = sent.iter().position(|(p, r)| {
        matches!(p, Prompt::Menu { title: Some(t), .. } if t == "Currently known spells")
            && *r == Reply::Cancel
    });
    let Some(at) = closed else {
        return Err(format!("+'s menu was not closed: {sent:?}"));
    };
    let z = sent[at..]
        .iter()
        .position(|(p, r)| *p == Prompt::Command && *r == Reply::Key('Z' as i32));
    let Some(z) = z else {
        return Err(format!("no Z after +'s menu: {sent:?}"));
    };
    let picked = sent[at + z..].iter().find_map(|(p, r)| match (p, r) {
        (
            Prompt::Menu {
                title: Some(t),
                items,
                ..
            },
            Reply::Menu(picks),
        ) if t == "Choose which spell to cast" => {
            let b = spells::spellbook(Some(t), items)?;
            let idx = picks.first().map(|(i, _)| *i as i32)?;
            b.spell(idx).map(|s| s.name.clone())
        }
        _ => None,
    });
    match picked {
        Some(n) if n == name => Ok(true),
        Some(n) => Err(format!("Z's menu picked {n}, not {name}")),
        None => Err(format!("Z's menu picked nothing: {sent:?}")),
    }
}

fn command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && !g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
}

fn esc() -> Step {
    Step::Key(KeyInput::plain(Key::Escape))
}

/// The first spell's name as `+` first listed it.
fn first_spell() -> String {
    FIRST
        .lock()
        .ok()
        .and_then(|f| f.first().map(|(_, n)| n.clone()))
        .unwrap_or_default()
}

/// Every card says what its spell does, costs and asks; the full card
/// shows `want` (else the first spell).
fn cards_say(g: &RenethackGame, want: Option<usize>) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let b = book(g).ok_or("no spells' menu under the panel")?;
    let texts = ui.dialogs.panel_texts();
    for s in &b.spells {
        let says = spells_view::description(&s.name).ok_or(format!("{}: no line", s.name))?;
        let costs = crate::spells::cost(s.level).to_string();
        for w in [&says, &costs] {
            if !texts.contains(w) {
                return Err(format!("{}'s card shows no {w:?}: {texts:?}", s.name));
            }
        }
        if let Some(tag) = spells_view::aim_tag(spells_view::aim_of(&s.name))
            && !texts.iter().any(|t| t.contains(&tag))
        {
            return Err(format!("{}'s card says not that it asks {tag:?}", s.name));
        }
    }
    let shown = b.spells.get(want.unwrap_or(0)).map(|s| s.name.clone());
    Ok(ui.dialogs.spells_card() == shown)
}

/// The full card shows the spell of the list's `i`th card.
fn card_of(g: &RenethackGame, i: usize) -> Result<bool, String> {
    let b = book(g).ok_or("no spells")?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(ui.dialogs.spells_card() == b.spells.get(i).map(|s| s.name.clone()))
}

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

static INPUT_AT: AtomicI64 = AtomicI64::new(0);

/// A pointer's event at `at` on the canvas, as the player's mouse gives
/// it: a move (with the left button down if `held`), or the left button
/// pressed or let go.
fn mouse(g: &RenethackGame, at: Vector2, press: Option<bool>, held: bool) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    match press {
        Some(down) => {
            let mut ev = InputEventMouseButton::new_gd();
            ev.set_button_index(MouseButton::LEFT);
            ev.set_pressed(down);
            ev.set_position(at);
            ev.set_global_position(at);
            if down {
                ev.set_button_mask(MouseButtonMask::LEFT);
            }
            Input::singleton().parse_input_event(&ev);
        }
        None => {
            let mut ev = InputEventMouseMotion::new_gd();
            ev.set_position(at);
            ev.set_global_position(at);
            if held {
                ev.set_button_mask(MouseButtonMask::LEFT);
            }
            Input::singleton().parse_input_event(&ev);
        }
    }
    INPUT_AT.store(frames(), Ordering::Relaxed);
    Ok(())
}

/// A few frames since the last pointer's event: the GUI has had it.
fn input_taken(_: &RenethackGame) -> Result<bool, String> {
    Ok(frames() > INPUT_AT.load(Ordering::Relaxed) + 2)
}

/// `text` names a keyboard's key ("Esc", "Tab", "Enter"), not a word
/// that begins as one (the Escape school).
fn names_a_key(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "Esc" | "Tab" | "Enter" | "Shift" | "Ctrl"))
}

/// The `i`th card's middle on the canvas.
fn card_at(g: &RenethackGame, i: usize) -> Result<Vector2, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let cards = ui.dialogs.spells_cards();
    let (_, r) = cards.get(i).ok_or(format!("no card {i}: {cards:?}"))?;
    Ok(r.position + r.size / 2.0)
}

/// The bar's slot of the spell `name`, if it is on the bar.
fn on_bar(g: &RenethackGame, name: &str) -> Option<usize> {
    let want = SlotBinding::Spell { name: name.into() };
    (0..nh_world::BAR_SLOTS).find(|&i| g.ui_state.bar.get(i) == Some(&want))
}

/// The slot a card is dropped on (the fifth: a command's, replaced).
const DROP_SLOT: usize = 4;

pub(super) fn spells() -> Vec<Step> {
    let mut steps = start_as(wizard());
    steps.extend([
        Step::Call("the turn", |g| {
            TURN.store(g.world.turn().unwrap_or(-1), Ordering::Relaxed);
            Ok(())
        }),
        key('+'),
        Step::Wait("+'s panel, as its menu says", |g| {
            shown(g, Purpose::View, false)
        }),
        Step::Call("the spells as + listed them", |g| {
            let b = book(g).ok_or("no spells")?;
            if b.spells.len() < 2 {
                return Err(format!("a Wizard knows {} spells", b.spells.len()));
            }
            *FIRST.lock().map_err(|e| e.to_string())? = b
                .spells
                .iter()
                .map(|s| (s.letter, s.name.clone()))
                .collect();
            Ok(())
        }),
        Step::Wait("its hint naming +", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .dialogs
                .panel_texts()
                .iter()
                .any(|t| t.contains("+:") || t.contains(", +")))
        }),
        Step::Wait("the HUD's title and role in the heroine's gender", |g| {
            // in Russian a female Wizard is Призывательница, Волшебница
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (title, role) = (ui.hud.title_shown(), ui.hud.subtitle_shown());
            if crate::i18n::lang() == crate::i18n::Lang::Ru
                && (!title.contains("Призывательница") || !role.starts_with("Волшебница"))
            {
                return Err(format!("the HUD says {title:?}, {role:?}"));
            }
            Ok(true)
        }),
        Step::Wait("every card says what its spell does, costs and asks", |g| {
            cards_say(g, None)
        }),
        Step::Shot("spells"),
        // the full card follows the keyboard's focus, and the pointer
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("the focus on the first card, its full card", |g| {
            card_of(g, 0)
        }),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("the focus on the second card, its full card", |g| {
            card_of(g, 1)
        }),
        Step::Shot("spells-focus"),
        Step::Call("the pointer on the first card", |g| {
            let at = card_at(g, 0)?;
            mouse(g, at, None, false)
        }),
        Step::Wait("the full card of the card under the pointer", |g| {
            Ok(input_taken(g)? && card_of(g, 0)?)
        }),
        // + again closes it; no turn passed
        key('+'),
        Step::Wait("closed on +, no turn passed", |g| {
            let was = TURN.load(Ordering::Relaxed);
            if command(g)? && g.world.turn().unwrap_or(-1) != was {
                return Err(format!("the turn was {was}, now {:?}", g.world.turn()));
            }
            command(g)
        }),
        // a pick casts it: + closed, Z, the spell picked, the direction
        key('+'),
        Step::Wait("+'s panel again", |g| shown(g, Purpose::View, false)),
        Step::Call("the answers so far forgotten", |g| {
            g.sent.clear();
            Ok(())
        }),
        key('a'),
        Step::Wait("a's spell cast through Z: the direction asked", |g| {
            Ok(asked_direction(g)? && cast_through_z(g, &first_spell())?)
        }),
        Step::Wait(
            "the question names the spell, its aim round the hero",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let line = ui.hud.prompt_line().unwrap_or_default();
                let name = spells_view::spell_name(&first_spell());
                if !line.contains(&name) {
                    return Err(format!("the prompt line says {line:?}"));
                }
                Ok(ui.aim.shown())
            },
        ),
        Step::Shot("spells-aim"),
        esc(),
        Step::Wait("no direction: the command again", command),
        // Z's panel casts a pick at once (the energy is short now)
        key('Z'),
        Step::Wait("Z's panel", |g| shown(g, Purpose::Cast, false)),
        Step::Wait("Z's panel greys the spells the Pw left cannot pay", |g| {
            let b = book(g).ok_or("no spells")?;
            let pw = g.world.status.number("energy").ok_or("no Pw")?;
            let short = b
                .spells
                .iter()
                .filter(|s| crate::spells::cost(s.level) > pw)
                .count();
            if short == 0 {
                return Err(format!("{pw} Pw pays for every spell"));
            }
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let said = crate::tr!("spells-cost-short");
            let greyed = ui
                .dialogs
                .panel_texts()
                .iter()
                .filter(|t| **t == said)
                .count();
            Ok(greyed == short)
        }),
        Step::Call("the bar greys the spell the Pw left cannot pay", |g| {
            let slot = on_bar(g, &first_spell()).ok_or("the first spell is on no slot")?;
            let pw = g.world.status.number("energy").ok_or("no Pw")?;
            let ui = g.ui.as_mut().ok_or("no UI")?;
            let (filled, enabled) = ui.hud.action_bar().slot_states()[slot];
            if pw < 5 && (!filled || enabled) {
                return Err(format!(
                    "slot {slot} at {pw} Pw: filled {filled}, enabled {enabled}"
                ));
            }
            Ok(())
        }),
        Step::Shot("spells-cast"),
        // the full card's button: the second spell on the bar's first
        // free slot
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Key(KeyInput::plain(Key::Down)),
        Step::Wait("the second card's full card", |g| card_of(g, 1)),
        Step::Call("its button: onto the bar", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            if !ui.dialogs.press_spells_to_bar() {
                return Err("no full card".into());
            }
            Ok(())
        }),
        Step::Wait("the second spell on the bar's first free slot", |g| {
            let b = book(g).ok_or("no spells")?;
            let s = b.spells.get(1).ok_or("one spell")?;
            Ok(on_bar(g, &s.name).is_some())
        }),
        // the first card dragged onto a slot: bound there, not cast
        Step::Wait("the bar's slots known", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.dialogs.bar_slots().len() > DROP_SLOT)
        }),
        Step::Call("the first card pressed", |g| {
            g.sent.clear();
            let at = card_at(g, 0)?;
            mouse(g, at, Some(true), true)
        }),
        Step::Wait("pressed", input_taken),
        Step::Call("dragged onto the bar", |g| {
            let at = card_at(g, 0)? + Vector2::new(0.0, 40.0);
            mouse(g, at, None, true)
        }),
        Step::Wait("moved", input_taken),
        Step::Call("over a slot", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let r = ui.dialogs.bar_slots()[DROP_SLOT];
            mouse(g, r.position + r.size / 2.0, None, true)
        }),
        Step::Wait("moved over the slot", input_taken),
        Step::Call("let go on the slot", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let r = ui.dialogs.bar_slots()[DROP_SLOT];
            mouse(g, r.position + r.size / 2.0, Some(false), false)
        }),
        Step::Wait("the first spell on the slot, Z's panel still up", |g| {
            if !g.sent.is_empty() {
                return Err(format!("the drag answered the engine: {:?}", g.sent));
            }
            let want = SlotBinding::Spell {
                name: first_spell(),
            };
            Ok(input_taken(g)?
                && g.ui_state.bar.get(DROP_SLOT) == Some(&want)
                && shown(g, Purpose::Cast, false)?)
        }),
        Step::Shot("spells-dragged"),
        Step::Call("the answers so far forgotten", |g| {
            g.sent.clear();
            Ok(())
        }),
        Step::Dialog(DialogEvent::MenuClick(1)),
        Step::Wait(
            "the first spell picked in Z's menu, the command again",
            |g| {
                let picked = z_picked(g);
                if picked.is_some() && picked != Some(first_spell()) {
                    return Err(format!("Z's menu picked {picked:?}"));
                }
                Ok(picked.is_some() && command(g)?)
            },
        ),
        // Tab reorders: a, then b, swaps them
        key('+'),
        Step::Wait("+'s panel for the reorder", |g| {
            shown(g, Purpose::View, false)
        }),
        Step::Key(KeyInput::plain(Key::Tab)),
        Step::Wait("a pick reorders now", |g| shown(g, Purpose::View, true)),
        key('a'),
        Step::Wait("the swap's panel for a", |g| {
            shown(g, Purpose::Swap('a'), false)
        }),
        Step::Shot("spells-swap"),
        key('b'),
        Step::Wait("+'s panel again, a and b exchanged", |g| {
            if !shown(g, Purpose::View, false)? {
                return Ok(false);
            }
            let b = book(g).ok_or("no spells")?;
            let first = FIRST.lock().map_err(|e| e.to_string())?.clone();
            let now = |c: char| {
                b.spells
                    .iter()
                    .find(|s| s.letter == c)
                    .map(|s| s.name.clone())
            };
            let was = |c: char| first.iter().find(|(l, _)| *l == c).map(|(_, n)| n.clone());
            if now('a') != was('b') || now('b') != was('a') {
                return Err(format!("a is {:?}, b {:?}", now('a'), now('b')));
            }
            Ok(true)
        }),
        // a gamepad: the d-pad and A cast the first spell; B closes
        esc(),
        Step::Wait("closed", command),
        Step::Push(UiEvent::GameKey(KeyInput::plain(Key::Char('+')))),
        Step::Wait("+'s panel for a gamepad", |g| {
            shown(g, Purpose::View, false)
        }),
        Step::Call("its first spell, the answers so far forgotten", |g| {
            let b = book(g).ok_or("no spells")?;
            let first = b.spells.first().ok_or("no spell")?.name.clone();
            *PAD_FIRST.lock().map_err(|e| e.to_string())? = first;
            g.sent.clear();
            Ok(())
        }),
        Step::Pad(vec![
            PadEv::Button(PadButton::Down, true),
            PadEv::Button(PadButton::Down, false),
        ]),
        Step::Wait(
            "under a pad: no card shows a key, the full card follows",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let b = book(g).ok_or("no spells")?;
                let words = ui.dialogs.panel_words();
                for s in &b.spells {
                    if words.contains(&s.letter.to_string()) {
                        return Err(format!("{}'s letter shows: {words:?}", s.name));
                    }
                }
                if words.iter().any(|w| *w == "+" || names_a_key(w)) {
                    return Err(format!("a key shows: {words:?}"));
                }
                card_of(g, 0)
            },
        ),
        Step::Shot("spells-pad"),
        Step::Pad(vec![
            PadEv::Button(PadButton::A, true),
            PadEv::Button(PadButton::A, false),
        ]),
        Step::Wait("the first row's spell cast by A through Z", |g| {
            let first = PAD_FIRST.lock().map_err(|e| e.to_string())?.clone();
            Ok(z_picked(g).is_some() && cast_through_z(g, &first)?)
        }),
        Step::Wait("the pad's aim: the spell named, its buttons, no key", |g| {
            if !asked_direction(g)? {
                // a spell that asks no direction
                return Ok(true);
            }
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let line = ui.hud.prompt_line().unwrap_or_default();
            let first = PAD_FIRST.lock().map_err(|e| e.to_string())?.clone();
            if names_a_key(&line) || !line.contains(&spells_view::spell_name(&first)) {
                return Err(format!("the prompt line says {line:?}"));
            }
            Ok(ui.aim.shown())
        }),
        // whatever the cast asks (a direction, where to jump), let go
        Step::KeysUntil("the cast's questions let go", |g| {
            fail_on_error_screen(g)?;
            if command(g)? {
                return Ok(None);
            }
            Ok(Some(KeyInput::plain(Key::Escape)))
        }),
        Step::Push(UiEvent::GameKey(KeyInput::plain(Key::Char('+')))),
        Step::Wait("+'s panel again", |g| shown(g, Purpose::View, false)),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("B: closed", command),
    ]);
    steps.extend(quit());
    steps
}

/// The books a Wizard learns for a long list: long names, every school.
const BOOKS: [&str; 10] = [
    "blessed spellbook of remove curse",
    "blessed spellbook of create familiar",
    "blessed spellbook of cure blindness",
    "blessed spellbook of detect unseen",
    "blessed spellbook of magic missile",
    "blessed spellbook of cone of cold",
    "blessed spellbook of stone to flesh",
    "blessed spellbook of restore ability",
    "blessed spellbook of confuse monster",
    "blessed spellbook of teleport away",
];

/// The pack's letters before the wishes, then the books' letters.
static WISHED: Mutex<Vec<char>> = Mutex::new(Vec::new());
/// The book being read (its place in `BOOKS`).
static READING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// What is on screen for the spells' menu the engine waits on: the panel,
/// or the menu as it came when it is not one.
fn spells_or_why(g: &RenethackGame, purpose: Purpose) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if ui.dialogs.spells_open() == Some((purpose, false)) {
        return Ok(true);
    }
    match g.pending.as_ref() {
        Some((_, Prompt::Menu { title, items, .. })) if ui.dialogs.is_open() => {
            let lines: Vec<String> = items
                .iter()
                .map(|i| format!("{} {:?}", i.selectable, i.str))
                .collect();
            Err(format!("{title:?} came as a plain menu: {lines:#?}"))
        }
        _ => Ok(false),
    }
}

/// `spells-many`: a Wizard who has read ten more books (debug mode's
/// wishes), long names among them: `+`, `Z` and the swap each come as the
/// spells' panel, every spell a row. In the pack a wished book keeps its
/// cover until it is read; then it shows its spell's icon.
pub(super) fn spells_many() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start_as(wizard()));
    steps.push(Step::Call("the pack before the wishes", |g| {
        let had: Vec<char> = g.world.inventory.items().iter().map(|i| i.letter).collect();
        *WISHED.lock().map_err(|e| e.to_string())? = had;
        Ok(())
    }));
    for wish in BOOKS {
        steps.extend(super::audio_tests::wish(wish));
    }
    steps.push(Step::Call("the books wished for", |g| {
        let mut w = WISHED.lock().map_err(|e| e.to_string())?;
        let books: Vec<char> = g
            .world
            .inventory
            .items()
            .iter()
            .map(|i| i.letter)
            .filter(|c| !w.contains(c))
            .collect();
        if books.len() != BOOKS.len() {
            return Err(format!("{} books wished for: {books:?}", books.len()));
        }
        *w = books;
        Ok(())
    }));
    steps.extend(pack_of_books(false));
    steps.push(Step::Call("the first book", |_| {
        READING.store(0, Ordering::Relaxed);
        Ok(())
    }));
    for _ in BOOKS {
        steps.extend([
            key('r'),
            Step::Request("what to read", |p| !matches!(p, Prompt::Command)),
            Step::KeyFrom("the book", |_| {
                let books = WISHED.lock().map_err(|e| e.to_string())?;
                books
                    .get(READING.load(Ordering::Relaxed))
                    .map(|c| KeyInput::plain(Key::Char(*c)))
                    .ok_or_else(|| "no book left".to_string())
            }),
            Step::KeysUntil("read through", |g| {
                fail_on_error_screen(g)?;
                if command(g)? {
                    return Ok(None);
                }
                Ok(Some(KeyInput::plain(Key::Enter)))
            }),
            Step::Call("the next book", |_| {
                READING.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        ]);
    }
    steps.extend(pack_of_books(true));
    steps.extend(books_elsewhere());
    steps.extend([
        key('+'),
        Step::Wait("+'s panel for many spells", |g| {
            spells_or_why(g, Purpose::View)
        }),
        Step::Call("every spell learnt is a row", |g| {
            let b = book(g).ok_or("no spells")?;
            godot::prelude::godot_print!(
                "selftest: spells-many: {:?}",
                b.spells.iter().map(|s| s.name.as_str()).collect::<Vec<_>>()
            );
            if b.spells.len() < 8 {
                return Err(format!("{} spells known", b.spells.len()));
            }
            Ok(())
        }),
        Step::Shot("spells-many"),
        esc(),
        Step::Wait("closed", command),
        key('Z'),
        Step::Wait("Z's panel for many spells", |g| {
            spells_or_why(g, Purpose::Cast)
        }),
        Step::Shot("spells-many-cast"),
        esc(),
        Step::Wait("closed", command),
    ]);
    steps.extend(quit());
    steps
}

/// The wished books as the pack names them: each tells its spell (and
/// has its spell's icon) only once the hero knows its kind. Read, every
/// one does; unread, all but those of a kind the hero began with (a
/// Wizard's second book) keep their covers.
fn books_tell(g: &RenethackGame, read: bool) -> Result<bool, String> {
    let wished = WISHED.lock().map_err(|e| e.to_string())?.clone();
    let told: Vec<(String, bool)> = g
        .world
        .inventory
        .items()
        .iter()
        .filter(|i| wished.contains(&i.letter))
        .map(|i| (i.text.clone(), spells::book_spell(&i.text).is_some()))
        .collect();
    let n = told.iter().filter(|(_, t)| *t).count();
    let right = if read { n == told.len() } else { n <= 2 };
    if !right {
        return Err(format!("read {read}: {told:?}"));
    }
    Ok(true)
}

/// The pack open on the wished books, `read` or not yet, and closed.
fn pack_of_books(read: bool) -> Vec<Step> {
    vec![
        key('i'),
        if read {
            Step::Wait("the pack: each book read shows its spell", |g| {
                let open = g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open());
                Ok(open && books_tell(g, true)?)
            })
        } else {
            Step::Wait("the pack: the books unread keep their covers", |g| {
                let open = g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open());
                Ok(open && books_tell(g, false)?)
            })
        },
        Step::Shot(if read {
            "spells-books-known"
        } else {
            "spells-books-unknown"
        }),
        esc(),
        Step::Wait("the pack closed", |g| {
            let open = g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open());
            Ok(!open && command(g)?)
        }),
    ]
}

/// The spell icon of each read book in `names` (their spells' English
/// names) is among the open dialog's icons.
fn shows_spell_icons(g: &RenethackGame, names: &[String]) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let shown = ui.dialogs.panel_textures();
    for n in names {
        let icon = crate::icons::spell_icon(n).ok_or(format!("no icon for {n}"))?;
        if !shown.contains(&icon) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The spells of the first two books wished for, once read.
fn first_books(g: &RenethackGame) -> Result<Vec<String>, String> {
    let wished = WISHED.lock().map_err(|e| e.to_string())?.clone();
    wished
        .iter()
        .take(2)
        .map(|c| {
            let item = g.world.inventory.by_letter(*c).ok_or(format!("no {c}"))?;
            spells::book_spell(&item.text)
                .map(|k| k.name.to_string())
                .ok_or(format!("{:?} tells no spell", item.text))
        })
        .collect()
}

static FLOOR: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// A read book shows its spell's icon outside the pack too: `\`'s
/// discoveries, and the pick-up menu of two books put down.
fn books_elsewhere() -> Vec<Step> {
    fn book(n: usize) -> Result<KeyInput, String> {
        let wished = WISHED.lock().map_err(|e| e.to_string())?;
        wished
            .get(n)
            .map(|c| KeyInput::plain(Key::Char(*c)))
            .ok_or_else(|| "no book".to_string())
    }
    let drop = |pick: fn(&RenethackGame) -> Result<KeyInput, String>| {
        [
            key('d'),
            Step::Request("what to drop", |p| !matches!(p, Prompt::Command)),
            Step::KeyFrom("a read book", pick),
            Step::Request("dropped", |p| matches!(p, Prompt::Command)),
        ]
    };
    let mut steps = vec![
        Step::Call("the first two books' spells", |g| {
            *FLOOR.lock().map_err(|e| e.to_string())? = first_books(g)?;
            Ok(())
        }),
        key('\\'),
        Step::Wait("the discoveries, a read book by its spell's icon", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let names = FLOOR.lock().map_err(|e| e.to_string())?.clone();
            Ok(ui.dialogs.layout_name() == Some("codex") && shows_spell_icons(g, &names)?)
        }),
        Step::Shot("spells-discoveries"),
        esc(),
        Step::Wait("closed", command),
    ];
    // the drop asks a count first for a stack; each book is one
    steps.extend(drop(|_| book(0)));
    steps.extend(drop(|_| book(1)));
    steps.extend([
        key(','),
        Step::Wait("the pick-up menu, each book by its spell's icon", |g| {
            let menu = matches!(g.pending, Some((_, Prompt::Menu { .. })));
            let names = FLOOR.lock().map_err(|e| e.to_string())?.clone();
            Ok(menu && shows_spell_icons(g, &names)?)
        }),
        Step::Shot("spells-pickup"),
        // both picked up again
        Step::Key(KeyInput::plain(Key::Char('.'))),
        Step::Key(KeyInput::plain(Key::Enter)),
        Step::Wait("picked up, the command again", command),
    ]);
    steps
}

fn monk() -> CharacterChoice {
    CharacterChoice {
        role: "monk".into(),
        ..wizard()
    }
}

/// The single spell `+` listed (a Monk's).
static ONE: Mutex<String> = Mutex::new(String::new());

/// `spells-one`: a Monk knows one spell, so spell.c's `+` is a menu of no
/// choice, which comes as a text window: the spells' panel all the same,
/// its spell a row (no table of the engine's columns); a pick casts it
/// through `Z`, whose panel lists it too.
pub(super) fn spells_one() -> Vec<Step> {
    let mut steps = start_as(monk());
    steps.extend([
        key('+'),
        Step::Wait("+'s panel for the one spell", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            if let Some((_, Prompt::Show { title, lines })) = g.pending.as_ref()
                && ui.dialogs.is_open()
                && ui.dialogs.spells_open().is_none()
            {
                return Err(format!("{title:?} came as a text window: {lines:?}"));
            }
            Ok(ui.dialogs.spells_open() == Some((Purpose::View, false)))
        }),
        Step::Call("its spell, its row", |g| {
            let Some((_, Prompt::Show { title, lines })) = g.pending.as_ref() else {
                return Err("+'s list is not the text window of one spell".into());
            };
            let headed: Vec<(String, bool)> = lines
                .iter()
                .map(|l| (l.text.clone(), l.attr & 0x0f != 0))
                .collect();
            let (_, b) = spells::shown(title.as_deref(), &headed).ok_or("no spellbook")?;
            let [s] = b.spells.as_slice() else {
                return Err(format!("{} spells", b.spells.len()));
            };
            let texts = g.ui.as_ref().ok_or("no UI")?.dialogs.panel_texts();
            for want in [s.letter.to_string(), spells_view::spell_name(&s.name)] {
                if !texts.contains(&want) {
                    return Err(format!("{}'s row shows no {want:?}: {texts:?}", s.name));
                }
            }
            *ONE.lock().map_err(|e| e.to_string())? = s.name.clone();
            g.sent.clear();
            Ok(())
        }),
        Step::Shot("spells-one"),
        key('a'),
        Step::Wait(
            "cast through Z: its list let go, Z, the spell picked",
            |g| {
                fail_on_error_screen(g)?;
                let name = ONE.lock().map_err(|e| e.to_string())?.clone();
                let sent: Vec<&(Prompt, Reply)> = g.sent.iter().collect();
                let closed = sent.iter().position(|(p, r)| {
                matches!(p, Prompt::Show { title: Some(t), .. } if t == "Currently known spells")
                    && *r == Reply::Cancel
            });
                let Some(at) = closed else {
                    return Ok(false);
                };
                let picked = sent[at..].iter().find_map(|(p, r)| match (p, r) {
                    (
                        Prompt::Menu {
                            title: Some(t),
                            items,
                            ..
                        },
                        Reply::Menu(picks),
                    ) if t == "Choose which spell to cast" => {
                        let b = spells::spellbook(Some(t), items)?;
                        let idx = picks.first().map(|(i, _)| *i as i32)?;
                        b.spell(idx).map(|s| s.name.clone())
                    }
                    _ => None,
                });
                match picked {
                    Some(n) if n == name => Ok(true),
                    Some(n) => Err(format!("Z's menu picked {n}, not {name}")),
                    None => Ok(false),
                }
            },
        ),
        Step::KeysUntil("the cast's questions let go", |g| {
            fail_on_error_screen(g)?;
            if command(g)? {
                return Ok(None);
            }
            Ok(Some(KeyInput::plain(Key::Escape)))
        }),
        key('Z'),
        Step::Wait("Z's panel for the one spell", |g| {
            shown(g, Purpose::Cast, false)
        }),
        Step::Shot("spells-one-cast"),
        esc(),
        Step::Wait("closed", command),
    ]);
    steps.extend(quit());
    steps
}
