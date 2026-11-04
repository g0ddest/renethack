//! `identify`: the identification helpers. A Valkyrie in debug mode wishes
//! for a potion, whose kind she does not know: the pack shows it by its
//! look. Its card offers «Назвать вид…» (#call) at the top, under its
//! default action. A press there runs #name's "the type of an object in
//! inventory" for it, and the name typed comes back in the pack ("called
//! fizzy") and on the card. `\`'s discoveries then show the type by its
//! icon, its look and the player's own name, marked as theirs.

use std::sync::atomic::{AtomicU32, Ordering};

use nh_world::{ItemActionKind, KeyProfile, Prompt};

use super::{CharacterChoice, DialogEvent, Step, key, panel, quit, start_as};
use crate::codex;
use crate::game::RenethackGame;
use crate::inventory_panel::{InvInput, InvTarget};

/// The name the player gives the potion's type.
pub(super) const NAME: &str = "fizzy";

/// The potion's letter once wished for.
static POTION: AtomicU32 = AtomicU32::new(0);

fn valkyrie() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "valkyrie".into(),
        race: "human".into(),
        gender: "female".into(),
        align: "neutral".into(),
        profile: KeyProfile::Classic,
    }
}

fn potion() -> char {
    char::from_u32(POTION.load(Ordering::Relaxed)).unwrap_or('?')
}

/// The potion as the pack names it now.
fn potion_text(g: &RenethackGame) -> Option<String> {
    g.world
        .inventory
        .by_letter(potion())
        .map(|i| i.text.clone())
}

/// At the command prompt, no dialog up.
fn at_command(g: &RenethackGame) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.dialogs.is_open() && matches!(g.pending, Some((_, Prompt::Command))))
}

// ---- what the features under test show ----

/// The card offers «Назвать вид…» right under the default action.
fn card_calls(g: &RenethackGame) -> Result<bool, String> {
    let p = panel(g)?;
    let call = crate::inventory_panel::label("item.call");
    let words = p.detail_words();
    Ok(p.card_call_on_top() && words.iter().any(|w| w.starts_with(&call)))
}

/// The card tells the player's own name for the type.
fn card_says_called(g: &RenethackGame) -> Result<bool, String> {
    let said = crate::tr!("fact-called", name = NAME);
    Ok(panel(g)?.detail_words().iter().any(|w| w.contains(&said)))
}

/// The discoveries show the type under «Ваши названия», its card marked
/// with the player's own name, and by its icon.
fn codex_marks(g: &RenethackGame, found: &codex::Discovery) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let texts = ui.dialogs.panel_texts();
    let mark = crate::tr!("codex-called", name = NAME);
    let yours = crate::tr!("codex-yours");
    let marks = texts.iter().filter(|t| t.contains(&mark)).count();
    // the card twice: first among the player's names, then in its class
    if !texts.contains(&yours) || marks != 2 {
        return Err(format!(
            "{found:?}: {marks} {mark:?}, {yours:?} among {texts:?}"
        ));
    }
    Ok(!ui.dialogs.panel_textures().is_empty())
}

pub(super) fn identify() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode", |g| {
        g.debug_mode = true;
        Ok(())
    })];
    steps.extend(start_as(valkyrie()));
    steps.extend(super::audio_tests::wish("uncursed potion of gain level"));
    steps.extend([
        Step::Call("the potion's letter", |g| {
            let item = g
                .world
                .inventory
                .items()
                .iter()
                .find(|i| i.class == '!')
                .ok_or("no potion wished for")?;
            if item.text.contains(" of ") {
                return Err(format!("{:?}: its kind is known", item.text));
            }
            POTION.store(u32::from(item.letter), Ordering::Relaxed);
            Ok(())
        }),
        key('i'),
        Step::Wait("the pack in browse mode", |g| {
            Ok(panel(g)?.mode_name() == Some("browse"))
        }),
        Step::Inv("the potion picked", |_| {
            Some(InvInput::Click {
                target: InvTarget::Cell(potion()),
                button: 1,
                shift: false,
                double: false,
            })
        }),
        Step::Wait("its card offers to call its type, at the top", |g| {
            let p = panel(g)?;
            Ok(p.selected() == Some(potion()) && card_calls(g)?)
        }),
        Step::Shot("identify-card"),
        Step::Inv("«Назвать вид…» pressed", |_| {
            Some(InvInput::Action {
                letter: potion(),
                kind: ItemActionKind::Call,
            })
        }),
    ]);
    steps.extend([
        Step::Request(
            "what to call it",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Call")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(NAME.into())),
        Step::Wait("the pack names the type as the player called it", |g| {
            Ok(at_command(g)?
                && potion_text(g).is_some_and(|t| t.contains(&format!("called {NAME}"))))
        }),
        // the pack as the naming left it, or opened again
        Step::KeyIf("the pack again", |g| {
            Ok((!panel(g)?.is_open()).then(|| nh_world::KeyInput::plain(nh_world::Key::Char('i'))))
        }),
        Step::Wait("the pack again", |g| {
            Ok(panel(g)?.mode_name() == Some("browse"))
        }),
        Step::Inv("the potion picked again", |_| {
            Some(InvInput::Click {
                target: InvTarget::Cell(potion()),
                button: 1,
                shift: false,
                double: false,
            })
        }),
        Step::Wait("its card says the player's name for the type", |g| {
            Ok(panel(g)?.selected() == Some(potion()) && card_says_called(g)?)
        }),
        Step::Shot("identify-called"),
        Step::Key(nh_world::KeyInput::plain(nh_world::Key::Escape)),
        Step::Wait("the pack closed", |g| {
            Ok(!panel(g)?.is_open() && at_command(g)?)
        }),
        key('\\'),
        Step::Wait(
            "the discoveries: the type by its look and the player's name",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                if ui.dialogs.layout_name() != Some("codex") {
                    return Ok(false);
                }
                let Some((_, Prompt::Show { lines, .. })) = g.pending.as_ref() else {
                    return Ok(false);
                };
                let headed: Vec<(String, bool)> = lines
                    .iter()
                    .map(|l| (l.text.clone(), l.attr & 0x0f != 0))
                    .collect();
                let d = codex::discoveries(&headed).ok_or("no discoveries")?;
                let found = d
                    .sections
                    .iter()
                    .flat_map(|s| &s.entries)
                    .find(|e| e.called.as_deref() == Some(NAME))
                    .ok_or(format!("no type called {NAME}: {d:?}"))?;
                codex_marks(g, found)
            },
        ),
        Step::Shot("identify-discoveries"),
        Step::Key(nh_world::KeyInput::plain(nh_world::Key::Escape)),
        Step::Wait("closed", at_command),
    ]);
    steps.extend(quit());
    steps
}
