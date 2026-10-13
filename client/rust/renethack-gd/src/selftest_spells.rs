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
//! what the engine's menu says of it. In Russian the HUD's title has the
//! Wizard's rank in the heroine's gender.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Prompt};

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
        Step::Wait(
            "the HUD's title with the rank in the heroine's gender",
            |g| {
                // in Russian a female Wizard is Призывательница
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let title = ui.hud.title_shown();
                if crate::i18n::lang() == crate::i18n::Lang::Ru
                    && !title.contains("Призывательница")
                {
                    return Err(format!("the HUD's title is {title:?}"));
                }
                Ok(true)
            },
        ),
        Step::Shot("spells"),
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
        esc(),
        Step::Wait("no direction: the command again", command),
        // Z's panel casts a pick at once (the energy is short now)
        key('Z'),
        Step::Wait("Z's panel", |g| shown(g, Purpose::Cast, false)),
        Step::Shot("spells-cast"),
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
        Step::Pad(vec![
            PadEv::Button(PadButton::A, true),
            PadEv::Button(PadButton::A, false),
        ]),
        Step::Wait("the first row's spell cast by A through Z", |g| {
            let first = PAD_FIRST.lock().map_err(|e| e.to_string())?.clone();
            Ok(z_picked(g).is_some() && cast_through_z(g, &first)?)
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
