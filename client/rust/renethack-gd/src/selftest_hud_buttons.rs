//! The HUD's micro-buttons (the inventory, the spells, the character…
//! over the bar's right end), measured at every UI scale the window
//! offers (`hud-buttons`): each one's hit target in the screen's pixels,
//! at least 32 (44, a fingertip's, at a Steam Deck's 1280×800), none on
//! another block, each with its tip naming its panel; a picture at each
//! scale, one under the sheet (the buttons in one row off it, a tip on
//! the first), one with a tip shown and one under a pad (the Menu
//! button's medallion by them, on no other block and under none of the
//! strip's hints, and no tip), an Xbox's and a PlayStation's.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use godot::classes::{Engine, Input, InputEventMouseMotion};
use godot::obj::Singleton;
use godot::prelude::*;

use super::{Step, start};
use crate::game::RenethackGame;

/// The scales tried, in percent.
const SCALES: [u32; 7] = [80, 90, 100, 110, 120, 130, 140];

/// The scale being tried (an index into `SCALES`, then one past).
static AT: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicI64 = AtomicI64::new(0);
/// What fell short, told at the end.
static SHORT: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// The window's pixels to a canvas pixel (a headless run at no set size:
/// those of the window the design is for, as the HUD counts them).
fn screen_per_canvas(g: &RenethackGame) -> f32 {
    crate::theme::window_scale(g.base().get_viewport())
}

/// The window is a Steam Deck's screen.
fn deck(g: &RenethackGame) -> bool {
    g.base()
        .get_tree()
        .get_root()
        .is_some_and(|w| w.get_size() == Vector2i::new(1280, 800))
}

/// The next scale set (one the window offers: none under 100 % on a
/// Deck's screen).
fn next_scale(g: &mut RenethackGame) -> Result<(), String> {
    let i = AT.load(Ordering::Relaxed);
    if let Some(&p) = SCALES.get(i)
        && (p >= crate::theme::DECK_SCALE_FLOOR || !deck(g))
    {
        let mut s = g.settings.clone();
        s.ui_scale = Some(p);
        g.set_settings(s);
    }
    FRAME.store(frames(), Ordering::Relaxed);
    Ok(())
}

/// The least hit target in the window's pixels.
fn least(g: &RenethackGame) -> f32 {
    if deck(g) {
        crate::hud::MICRO_TOUCH
    } else {
        crate::hud::MICRO_LEAST
    }
}

fn settled(_: &RenethackGame) -> Result<bool, String> {
    Ok(frames() > FRAME.load(Ordering::Relaxed) + 8)
}

/// This scale's buttons measured, what falls short kept.
fn measure(g: &mut RenethackGame) -> Result<(), String> {
    let i = AT.load(Ordering::Relaxed);
    let Some(&p) = SCALES.get(i) else {
        return Ok(());
    };
    if p < crate::theme::DECK_SCALE_FLOOR && deck(g) {
        return Ok(());
    }
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let k = screen_per_canvas(g);
    let buttons = ui.hud.micro_buttons();
    let least = least(g);
    let mut short = SHORT.lock().map_err(|e| e.to_string())?;
    let (mut w, mut h) = (f32::MAX, f32::MAX);
    for (r, tip) in &buttons {
        w = w.min(r.size.x * k);
        h = h.min(r.size.y * k);
        if tip.trim().is_empty() {
            short.push(format!("{p}%: a button with no tip"));
        }
    }
    let first = buttons
        .first()
        .map_or(Vector2::ZERO, |(r, _)| (r.position + r.size / 2.0) * k);
    godot_print!(
        "selftest: micro-buttons at {p}%: {} of them, the least {:.1}×{:.1} px (wanted {least}), \
         the first at ({:.0}, {:.0})",
        buttons.len(),
        w,
        h,
        first.x,
        first.y
    );
    if p == 100 {
        let tips: Vec<&str> = buttons.iter().map(|(_, t)| t.as_str()).collect();
        godot_print!("selftest: the micro-buttons' tips: {}", tips.join(" · "));
    }
    if w < least || h < least {
        short.push(format!("{p}%: {w:.1}×{h:.1} px, under {least}"));
    }
    // none on another block
    for (name, b) in ui.hud.blocks() {
        if name == "micro-buttons" {
            continue;
        }
        if let Some((r, tip)) = buttons.iter().find(|(r, _)| r.intersects(b)) {
            let _ = r;
            short.push(format!("{p}%: {tip:?} lies on the {name}"));
        }
    }
    Ok(())
}

/// The scale after this one.
fn advance(_: &mut RenethackGame) -> Result<(), String> {
    AT.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// The mouse on the first micro-button (its tip comes after a rest).
fn hover_first(g: &RenethackGame) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((r, _)) = ui.hud.micro_buttons().first().cloned() else {
        return Err("no micro-buttons".into());
    };
    move_mouse(g, r.position + r.size / 2.0)
}

/// The mouse to `at` on the canvas, as the player's would move.
fn move_mouse(g: &RenethackGame, at: Vector2) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    let mut ev = InputEventMouseMotion::new_gd();
    ev.set_position(at);
    ev.set_global_position(at);
    Input::singleton().parse_input_event(&ev);
    Ok(())
}

/// The mouse kept on the first micro-button, its tip shown: its words, by
/// the button where it is now (above it or under it).
fn tip_by_first(g: &RenethackGame) -> Result<bool, String> {
    hover_first(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((r, want)) = ui.hud.micro_buttons().first().cloned() else {
        return Err("no micro-buttons".into());
    };
    Ok(ui.tips.shown().is_some_and(|(words, tip)| {
        let near = Rect2::new(
            r.position - Vector2::new(0.0, 80.0),
            r.size + Vector2::new(0.0, 160.0),
        );
        words == want && tip.intersects(near)
    }))
}

/// Under a pad: its medallion on no block and no button, the strip's
/// hints on no button.
fn pad_clear(g: &mut RenethackGame) -> Result<(), String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let medal = ui.hud.micro_pad_rect().ok_or("no medallion")?;
    let row = ui.hud.micro_rect().ok_or("no micro-buttons")?;
    let mut short = SHORT.lock().map_err(|e| e.to_string())?;
    for (name, b) in ui.hud.blocks() {
        if name != "micro-buttons" && medal.intersects(b) {
            short.push(format!("the pad's medallion lies on the {name}"));
        }
    }
    for (r, tip) in ui.hud.micro_buttons() {
        if medal.intersects(r) {
            short.push(format!("the pad's medallion lies on {tip:?}"));
        }
    }
    if let Some((strip, _)) = ui.pad.strip_rect()
        && strip.intersects(row)
    {
        short.push("the pad's hints lie on the micro-buttons".into());
    }
    godot_print!(
        "selftest: under a {:?} pad: the medallion at {:?} {:?}, the buttons' {:?}",
        g.pad.kind,
        medal.position,
        medal.size,
        row
    );
    Ok(())
}

pub(super) fn hud_buttons() -> Vec<Step> {
    let mut steps = vec![Step::Call("", |_| {
        AT.store(0, Ordering::Relaxed);
        SHORT.lock().map_err(|e| e.to_string())?.clear();
        Ok(())
    })];
    steps.extend(start());
    for name in [
        "hud-buttons-80",
        "hud-buttons-90",
        "hud-buttons-100",
        "hud-buttons-110",
        "hud-buttons-120",
        "hud-buttons-130",
        "hud-buttons-140",
    ] {
        steps.extend([
            Step::Call("the next scale", next_scale),
            Step::Wait("laid out", settled),
            Step::Call("the buttons measured", measure),
            Step::ShotIf(name, |g| {
                let p = SCALES[AT.load(Ordering::Relaxed).min(SCALES.len() - 1)];
                Ok(p >= crate::theme::DECK_SCALE_FLOOR || !deck(g))
            }),
            Step::Call("", advance),
        ]);
    }
    // the window's own scale again, and a tip on the first button
    steps.extend([
        Step::Call("Auto again", |g| {
            let mut s = g.settings.clone();
            s.ui_scale = None;
            g.set_settings(s);
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("laid out", settled),
        // the sheet over the cluster: the log docked there steps back, the
        // buttons take one row under the panel
        Step::Key(super::ctrl_key('x')),
        Step::Wait("the sheet, the buttons in one row off it", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (Some(panel), Some(row)) = (ui.dialogs.panel_rect(), ui.hud.micro_rect()) else {
                return Ok(false);
            };
            Ok(ui.dialogs.rpg_open() && row.size.y < 60.0 && !panel.intersects(row))
        }),
        Step::Call("the buttons under the sheet measured", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let k = screen_per_canvas(g);
            let least = least(g);
            let side = (ui.hud.micro_buttons().iter())
                .map(|(r, _)| r.size.x.min(r.size.y) * k)
                .fold(f32::MAX, f32::min);
            godot_print!("selftest: micro-buttons under the sheet: the least {side:.1} px");
            if side < least {
                let mut short = SHORT.lock().map_err(|e| e.to_string())?;
                short.push(format!("under the sheet: {side:.1} px, under {least}"));
            }
            Ok(())
        }),
        // the docked log's lines away with it let the mouse through
        Step::Call("the mouse on the inventory's button", |g| hover_first(g)),
        Step::Wait("its tip under the sheet, by it", tip_by_first),
        Step::Shot("hud-buttons-sheet"),
        Step::Key(super::ctrl_key('x')),
        Step::Wait("the sheet closed, the buttons as before", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let w = g
                .base()
                .get_viewport()
                .map_or(0.0, |v| v.get_visible_rect().size.x);
            let two = crate::hud::compact(w);
            let rows = ui.hud.micro_rect().map_or(0.0, |r| r.size.y);
            Ok(!ui.dialogs.rpg_open() && (rows > 60.0) == two)
        }),
        // laid out again, the mouse off to the map's middle (the tip under
        // the sheet goes), then on the button where it is now
        Step::Call("the mouse on the map", |g| {
            let vp = g.base().get_viewport().ok_or("no viewport")?;
            let at = vp.get_visible_rect().size / 2.0;
            move_mouse(g, at)?;
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("laid out, no tip", |g| {
            Ok(settled(g)? && g.ui.as_ref().is_some_and(|ui| ui.tips.shown().is_none()))
        }),
        Step::Call("the mouse on the inventory's button", |g| hover_first(g)),
        Step::Wait("its tip, by it", tip_by_first),
        Step::Shot("hud-buttons-tip"),
        // a pad: the Menu button's medallion beside them
        super::tap(crate::gamepad::PadButton::R3),
        Step::Wait("the pad's medallion by the buttons, no tip", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.hud.micro_pad_rect().is_some() && ui.tips.shown().is_none())
        }),
        Step::Shot("hud-buttons-pad"),
        Step::Call(
            "the medallion on no block, the strip on no button",
            pad_clear,
        ),
        // a PlayStation's, its Menu button the widest ("Options")
        Step::Call("a PlayStation pad", |g| {
            g.pad.kind = crate::gamepad::PadKind::PlayStation;
            FRAME.store(frames(), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("laid out", settled),
        Step::Shot("hud-buttons-pad-ps"),
        Step::Call(
            "its medallion on no block, the strip on no button",
            pad_clear,
        ),
        Step::Call("what fell short", |_| {
            let short = SHORT.lock().map_err(|e| e.to_string())?;
            match short.is_empty() {
                true => Ok(()),
                false => Err(short.join("; ")),
            }
        }),
    ]);
    steps
}
