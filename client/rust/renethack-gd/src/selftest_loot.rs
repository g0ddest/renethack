//! A container's things beside the pack (`loot`): a debug game set up
//! in English wishes for a sack and arrows. Applied empty, the sack's
//! panel lists the pack to put in: the food ration marked, Enter puts it
//! in, through the engine's own menus. Applied again, its panel has the
//! ration in the sack's column: marked to take out, with 2 of the 5
//! arrows marked to put in, one Enter does both. Esc leaves everything
//! as it was. Under a pad: the d-pad, LB/RB, A and Start.

use std::sync::Mutex;

use godot::prelude::*;

use nh_world::{Key, KeyInput, Prompt};

use super::{DialogEvent, Step, ctrl_key, fail_on_error_screen, key, letter_of, start, tap};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::i18n::{self, Lang};
use crate::loot_view::{CONTAINER, PACK};
use crate::ui_events::UiEvent;

static ASKED: Mutex<Option<Lang>> = Mutex::new(None);

fn asked() -> Lang {
    ASKED.lock().ok().and_then(|l| *l).unwrap_or(Lang::En)
}

fn wish(what: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('w')),
        Step::Request(
            "the wish",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("For what do you wish")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::AnswerUntil(' ', "a command after the wish", command_now),
    ]
}

fn command_now(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    Ok(matches!(g.pending, Some((_, Prompt::Command))))
}

/// `a`, then the sack's letter at the getobj question.
fn apply_sack() -> Vec<Step> {
    vec![
        Step::Wait("the player's turn", command_now),
        key('a'),
        Step::Request("what to apply", |p| !matches!(p, Prompt::Command)),
        Step::KeyFrom("the sack's letter", |g| {
            let l = letter_of(g, "bag").ok_or("no bag")?;
            Ok(KeyInput::plain(Key::Char(l)))
        }),
    ]
}

/// The sack's panel open, laid out: `pack` rows and `inside` rows.
fn panel_with(g: &RenethackGame, pack: Option<usize>, inside: usize) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((lens, _, _)) = ui.dialogs.loot_state() else {
        return Ok(false);
    };
    if lens[CONTAINER] != inside || pack.is_some_and(|n| lens[PACK] != n) {
        return Err(format!("the panel's columns: {lens:?}"));
    }
    // laid out: its first row has a place
    let first = ui
        .dialogs
        .loot_row_rect(if lens[PACK] > 0 { PACK } else { CONTAINER }, 0);
    Ok(first.is_some_and(|r| r.size.y > 0.0 && r.position.y > 0.0))
}

/// The pack's row of the item named so (its place in the pack's column).
fn pack_row(g: &RenethackGame, what: &str) -> Result<usize, String> {
    let l = letter_of(g, what).ok_or_else(|| format!("no {what}"))?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let rows = ui.dialogs.loot_pack_letters().ok_or("no panel")?;
    rows.iter()
        .position(|c| *c == l)
        .ok_or_else(|| format!("{what} not in the pack's column"))
}

/// The request is the put-in list.
fn putting(g: &RenethackGame) -> bool {
    matches!(&g.pending, Some((_, Prompt::Menu { title: Some(t), .. })) if t == "Put in what?")
}

/// The pending request's id, for a dialog event pushed by hand.
fn req(g: &RenethackGame) -> Result<u64, String> {
    g.pending
        .as_ref()
        .map(|(id, _)| *id)
        .ok_or("no request".into())
}

/// A row marked through the panel's own event, as a click gives it.
fn mark(g: &mut RenethackGame, col: usize, row: usize) -> Result<(), String> {
    let req = req(g)?;
    let ev = DialogEvent::LootToggle { column: col, row };
    g.push_ui(UiEvent::Dialog { req, ev });
    Ok(())
}

/// A menu of NetHack's drawn as the dialogs draw it (no request: a
/// picture of how it read before the panel).
fn stage(
    g: &mut RenethackGame,
    title: &str,
    rows: &[&str],
    how: nh_protocol::PickHow,
) -> Result<(), String> {
    let items = rows
        .iter()
        .enumerate()
        .map(|(i, t)| nh_protocol::MenuItem {
            win: 3,
            idx: i as i32 + 1,
            glyph: None,
            selectable: true,
            ch: (b'a' + i as u8) as i32,
            gch: 0,
            attr: 0,
            clr: 8,
            str: Some((*t).into()),
            preselected: false,
            skipinvert: false,
        })
        .collect();
    let menu = Prompt::Menu {
        win: 3,
        how,
        title: Some(title.into()),
        items,
    };
    let cat = g.catalog.clone();
    let ui = g.ui.as_mut().ok_or("no UI")?;
    ui.dialogs.open(0, &menu, cat.as_deref());
    Ok(())
}

/// A left click at `at` on the canvas.
fn click(g: &RenethackGame, at: godot::prelude::Vector2) -> Result<(), String> {
    use godot::classes::{Input, InputEventMouseButton};

    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    for pressed in [true, false] {
        let mut ev = InputEventMouseButton::new_gd();
        ev.set_button_index(godot::global::MouseButton::LEFT);
        ev.set_position(at);
        ev.set_global_position(at);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
    Ok(())
}

fn quan(g: &RenethackGame, what: &str) -> i64 {
    g.world
        .inventory
        .items()
        .iter()
        .find(|i| i.text.contains(what))
        .map_or(0, |i| i.quan)
}

pub(super) fn loot() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, set up in English", |g| {
        *ASKED.lock().map_err(|e| e.to_string())? = Some(i18n::lang());
        g.debug_mode = true;
        g.seed = Some(7);
        g.push_ui(UiEvent::SetLanguage(Lang::En));
        Ok(())
    })];
    steps.extend(start());
    steps.extend(wish("uncursed sack"));
    steps.extend(wish("5 uncursed +0 arrows"));
    steps.push(Step::Call("the language asked for", |g| {
        g.push_ui(UiEvent::SetLanguage(asked()));
        Ok(())
    }));
    // how NetHack's own menus for it read as dialogs (the pictures of
    // before the panel): drawn, pictured, put away
    steps.extend([
        Step::Call("NetHack's «Do what?», as a dialog", |g| {
            stage(
                g,
                "Do what with your bag?",
                &[
                    "Look inside the bag",
                    "take something out",
                    "put something in",
                    "both; take out, then put in",
                    "both reversed; put in, then take out",
                    "stash one item into the bag",
                    "do nothing",
                ],
                nh_protocol::PickHow::One,
            )
        }),
        Step::Shot("loot-before-what"),
        Step::Call("NetHack's «Take out what?», as a dialog", |g| {
            stage(
                g,
                "Take out what?",
                &["an uncursed food ration", "2 arrows"],
                nh_protocol::PickHow::Any,
            )
        }),
        Step::Shot("loot-before-take"),
        Step::Call("put away", |g| {
            g.ui.as_mut().ok_or("no UI")?.dialogs.close();
            Ok(())
        }),
    ]);
    // the empty sack: the pack to put in, nothing inside
    steps.extend(apply_sack());
    steps.extend([
        Step::Wait("the sack's panel: the pack to put in", |g| {
            Ok(putting(g) && panel_with(g, None, 0)?)
        }),
        Step::Shot("loot-put"),
        Step::Call("the food ration marked", |g| {
            let row = pack_row(g, "food ration")?;
            mark(g, PACK, row)
        }),
        Step::Wait("marked", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .dialogs
                .loot_state()
                .is_some_and(|(_, m, _)| m[PACK].len() == 1))
        }),
        Step::Press(godot::global::Key::ENTER, '\0', false),
        Step::AnswerUntil(' ', "Enter: put in, a command again", command_now),
        Step::Wait(
            "the ration in the sack",
            |g| Ok(quan(g, "food ration") == 0),
        ),
    ]);
    // the sack again: its ration to take out, 2 of 5 arrows to put in
    steps.extend(apply_sack());
    steps.extend([
        Step::Wait("the sack's panel: the ration inside", |g| {
            Ok(!putting(g) && panel_with(g, None, 1)?)
        }),
        Step::Shot("loot-both"),
        Step::Call("the ration marked to take out", |g| mark(g, CONTAINER, 0)),
        Step::Call("the arrows marked to put in", |g| {
            let row = pack_row(g, "arrow")?;
            mark(g, PACK, row)
        }),
        Step::Wait("both marked", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .dialogs
                .loot_state()
                .is_some_and(|(_, m, _)| m[PACK].len() == 1 && m[CONTAINER].len() == 1))
        }),
        // ← three times: 2 of the 5 (the keys' row is the arrows', the
        // last marked)
        Step::Press(godot::global::Key::LEFT, '\0', false),
        Step::Press(godot::global::Key::LEFT, '\0', false),
        Step::Press(godot::global::Key::LEFT, '\0', false),
        Step::Shot("loot-count"),
        Step::Press(godot::global::Key::ENTER, '\0', false),
        Step::AnswerUntil(' ', "Enter: out and in, a command again", command_now),
        Step::Wait("the ration back, 3 arrows left", |g| {
            Ok(quan(g, "food ration") == 1 && quan(g, "arrow") == 3)
        }),
    ]);
    // Esc: nothing moves
    steps.extend(apply_sack());
    steps.extend([
        Step::Wait("the sack's panel", |g| panel_with(g, None, 1)),
        Step::Call("the arrows marked", |g| mark(g, CONTAINER, 0)),
        Step::Press(godot::global::Key::ESCAPE, '\0', false),
        Step::AnswerUntil(' ', "Esc: a command again", command_now),
        Step::Wait("still 3 arrows, 2 in the sack", |g| {
            Ok(quan(g, "arrow") == 3)
        }),
    ]);
    // a pad: ↓ to the sack's row, A marks, Start takes it out
    steps.extend(apply_sack());
    steps.extend([
        Step::Wait("the sack's panel", |g| panel_with(g, None, 1)),
        tap(PadButton::Down),
        Step::Wait("a pad's: the sack's column, its first row", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(g.pad_ctx() == crate::gamepad::PadCtx::Loot
                && ui
                    .dialogs
                    .loot_state()
                    .is_some_and(|(_, _, f)| f == (CONTAINER, Some(0))))
        }),
        tap(PadButton::A),
        Step::Shot("loot-pad"),
        tap(PadButton::Start),
        Step::AnswerUntil(' ', "Start: taken out, a command again", command_now),
        Step::Wait("5 arrows again", |g| Ok(quan(g, "arrow") == 5)),
    ]);
    // a shop's goods not paid for: «Заплатить» on the HUD, NetHack's `p`
    // (staged: the client's own view of the pack says a ware is unpaid;
    // the engine, with no shopkeeper here, says so)
    steps.extend([
        Step::Wait("the player's turn", command_now),
        Step::Call("an unpaid ware, staged", |g| {
            let mut items = g.world.inventory.items().to_vec();
            let ration = items
                .iter_mut()
                .find(|i| i.text.contains("food ration"))
                .ok_or("no ration")?;
            ration.text = format!("{} (unpaid, 45 zorkmids)", ration.text);
            g.world.inventory.replace(&nh_protocol::Inventory {
                items,
                twoweap: false,
            });
            Ok(())
        }),
        Step::Wait(
            "«Заплатить» on the HUD, on no other block",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let Some(pay) = ui.hud.pay_rect() else {
                    return Ok(false);
                };
                if let Some((name, _)) = (ui.hud.blocks().into_iter())
                    .find(|(n, r)| *n != "pay button" && r.intersects(pay))
                {
                    return Err(format!("«Заплатить» lies on the {name}"));
                }
                Ok(pay.size.x > 0.0)
            },
        ),
        Step::Shot("pay-button"),
        Step::Call("«Заплатить» clicked", |g| {
            let at =
                g.ui.as_ref()
                    .ok_or("no UI")?
                    .hud
                    .pay_rect()
                    .ok_or("no button")?;
            click(g, at.center())
        }),
        Step::Wait("p: the engine's answer, a command again", |g| {
            fail_on_error_screen(g)?;
            let said = g.world.log.iter().any(|m| m.text.contains("shopkeeper"));
            Ok(said && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // the staging undone: nothing unpaid, no button
        Step::Call("the ware as the engine has it", |g| {
            let mut items = g.world.inventory.items().to_vec();
            for i in items.iter_mut() {
                i.text = i.text.replace(" (unpaid, 45 zorkmids)", "");
            }
            g.world.inventory.replace(&nh_protocol::Inventory {
                items,
                twoweap: false,
            });
            Ok(())
        }),
        Step::Wait("nothing unpaid: the button away", |g| {
            Ok(g.ui.as_ref().ok_or("no UI")?.hud.pay_rect().is_none())
        }),
    ]);
    steps
}
