//! The skills' panel (`skills`): a debug-mode Valkyrie's #enhance laid out
//! from the engine's own window, a skill a row under its group with its
//! level, in the language the test runs in; no turn passes. Without
//! practice ("Advance skills without practice?" n) nothing can advance and
//! the panel only shows; with it (y) each skill is a pick: its letter
//! advances it, the engine shows the skills again with it a level up, and
//! a gamepad's d-pad and A advance another. #enhance's own key (M-e)
//! opens the panel and closes it; B and Esc close it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use nh_world::{Key, KeyInput, Mods, Prompt};

use super::{DialogEvent, PadEv, Step, fail_on_error_screen, key, quit, start};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::skills::{self, Level, Mark, Row, Skills};
use crate::skills_view;

/// The turn #enhance first opened on.
static TURN: AtomicI64 = AtomicI64::new(-1);
/// The skill advanced by its letter, and its level before.
static ADVANCED: Mutex<Option<(String, Level)>> = Mutex::new(None);

/// #enhance's window the engine waits on, as it wrote it.
fn window(g: &RenethackGame) -> Option<Skills> {
    match g.pending.as_ref()? {
        (_, Prompt::Menu { title, items, .. }) => {
            let lines: Vec<(String, bool, Option<i32>)> = items
                .iter()
                .map(|i| {
                    let heading = !i.selectable && i.attr & 0x0f != 0;
                    let text = i.str.clone().unwrap_or_default();
                    (text, heading, i.selectable.then_some(i.idx))
                })
                .collect();
            skills::skills(title.as_deref(), &lines)
        }
        (_, Prompt::Show { title, lines }) => {
            let lines: Vec<(String, bool, Option<i32>)> = lines
                .iter()
                .map(|l| (l.text.clone(), l.attr & 0x0f != 0, None))
                .collect();
            skills::skills(title.as_deref(), &lines)
        }
        _ => None,
    }
}

fn all(s: &Skills) -> Vec<&skills::Skill> {
    s.rows
        .iter()
        .filter_map(|r| match r {
            Row::Skill(k) => Some(k),
            _ => None,
        })
        .collect()
}

/// The skills' panel is open on #enhance's window (`advance`: its skills
/// picks), each skill's row with its name and level.
fn shown(g: &RenethackGame, advance: bool) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some(s) = window(g) else {
        return Ok(false);
    };
    if s.advance != advance || ui.dialogs.kind_name() != Some("menu") {
        return Ok(false);
    }
    let texts = ui.dialogs.panel_texts();
    for k in all(&s) {
        for want in [
            skills_view::skill_name(&k.name),
            skills_view::level_name(k.level),
        ] {
            if !texts.iter().any(|t| t.starts_with(&want)) {
                return Err(format!("{}'s row shows no {want:?}: {texts:?}", k.name));
            }
        }
        if advance && k.mark != Mark::Advance {
            return Err(format!("{} is no pick", k.name));
        }
    }
    for g in [skills::Group::Fighting, skills::Group::Weapon] {
        let name = skills_view::group_name(g, "");
        if !texts.contains(&name) {
            return Err(format!("no group {name:?}: {texts:?}"));
        }
    }
    // the Valkyrie's polearms in Russian: a mass noun, as one weapon
    if crate::i18n::lang() == crate::i18n::Lang::Ru
        && all(&s).iter().any(|k| k.name == "polearms")
        && !texts.iter().any(|t| t == "Древковое оружие")
    {
        return Err(format!("no Древковое оружие: {texts:?}"));
    }
    Ok(true)
}

fn command(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && matches!(g.pending, Some((_, Prompt::Command))))
}

fn alt_e() -> KeyInput {
    KeyInput {
        mods: Mods {
            alt: true,
            ..Mods::default()
        },
        ..KeyInput::plain(Key::Char('e'))
    }
}

/// #enhance from the palette, then debug mode's question answered `yes`.
fn enhance(yes: char) -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("enhance".into())),
        Step::Request(
            "advance without practice?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("without practice")),
        ),
        key(yes),
    ]
}

pub(super) fn skills() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode", |g| {
        g.debug_mode = true;
        g.seed = Some(7);
        Ok(())
    })];
    steps.extend(start());
    steps.push(Step::Call("the turn", |g| {
        TURN.store(g.world.turn().unwrap_or(-1), Ordering::Relaxed);
        Ok(())
    }));
    // with no practice: the skills as they are
    steps.extend(enhance('n'));
    steps.extend([
        Step::Wait("the skills' panel, nothing to advance", |g| shown(g, false)),
        Step::Shot("skills"),
        Step::Key(KeyInput::plain(Key::Escape)),
        Step::Wait("closed, no turn passed", |g| {
            let was = TURN.load(Ordering::Relaxed);
            if command(g)? && g.world.turn().unwrap_or(-1) != was {
                return Err(format!("the turn was {was}, now {:?}", g.world.turn()));
            }
            command(g)
        }),
        // its own key opens it and closes it
        Step::Key(alt_e()),
        Step::Request(
            "advance without practice? (M-e)",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("without practice")),
        ),
        key('n'),
        Step::Wait("the panel on M-e", |g| shown(g, false)),
        Step::Key(alt_e()),
        Step::Wait("closed on M-e", command),
    ]);
    // without practice: every skill a pick
    steps.extend(enhance('y'));
    steps.extend([
        Step::Wait("the skills' panel, each a pick", |g| shown(g, true)),
        Step::Shot("skills-advance"),
        Step::KeyFrom("the first skill's letter", |g| {
            let s = window(g).ok_or("no skills")?;
            let first = all(&s).into_iter().next().ok_or("no skill")?.clone();
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let letter = ui
                .dialogs
                .menu_letter(first.idx.ok_or("no pick")?)
                .ok_or("no letter")?;
            *ADVANCED.lock().map_err(|e| e.to_string())? = Some((first.name, first.level));
            Ok(KeyInput::plain(Key::Char(letter)))
        }),
        Step::Wait("the skills again, that one a level up", |g| {
            if !shown(g, true)? {
                return Ok(false);
            }
            let s = window(g).ok_or("no skills")?;
            let (name, before) = ADVANCED
                .lock()
                .map_err(|e| e.to_string())?
                .clone()
                .ok_or("nothing advanced")?;
            let now = all(&s).into_iter().find(|k| k.name == name).map(|k| k.level);
            match now {
                Some(l) if l > before => Ok(true),
                other => Err(format!("{name} was {before:?}, now {other:?}")),
            }
        }),
        // a gamepad: the d-pad to a pick, A advances it; B closes
        Step::Pad(vec![
            PadEv::Button(PadButton::Down, true),
            PadEv::Button(PadButton::Down, false),
        ]),
        Step::Pad(vec![
            PadEv::Button(PadButton::A, true),
            PadEv::Button(PadButton::A, false),
        ]),
        Step::Wait("advanced by A: the skills again", |g| {
            Ok(g.ui
                .as_ref()
                .is_some_and(|ui| ui.dialogs.open_req().is_some())
                && g.sent.iter().rev().take(2).any(|(p, r)| {
                    matches!(p, Prompt::Menu { title: Some(t), .. } if t.starts_with("Pick a skill"))
                        && matches!(r, nh_protocol::Reply::Menu(m) if !m.is_empty())
                })
                && shown(g, true)?)
        }),
        Step::Pad(vec![
            PadEv::Button(PadButton::B, true),
            PadEv::Button(PadButton::B, false),
        ]),
        Step::Wait("B: closed", command),
    ]);
    steps.extend(quit());
    steps
}
