//! The command wheel (`command-wheel`): Tab held opens it at the mouse on
//! the sections' ring, → points at the common ones, letting go opens
//! their ring (eleven commands and "More…"); the mouse on #search and a
//! click search. The middle button held and let go on "All commands…":
//! the list, for the same request; held and let go in the middle:
//! nothing. A tap of Tab leaves it open: → Enter the common ones, ←
//! Enter their "More…", the list with that section alone open, and the
//! next `#` with the player's folds again. Esc cancels. With the wheel
//! chosen in the settings `#` opens it, and the HUD's Commands button
//! by the button (a right click cancels). Under a pad: LT's "Commands",
//! the stick at the common ones, A, at #pray, A: the prayer's question;
//! B cancels.

use std::sync::Mutex;
use std::time::Instant;

use godot::classes::{Input, InputEventKey, InputEventMouseButton, InputEventMouseMotion};
use godot::global::{JoyAxis, Key as GKey, MouseButton};
use godot::prelude::*;
use nh_world::Prompt;

use super::{PadEv, Step, command, quit, radial, start, tap};
use crate::command_menu::Section;
use crate::command_wheel::{MOST, Ring, WheelItem};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::tr;

fn ui(g: &RenethackGame) -> Result<&crate::game::Ui, String> {
    g.ui.as_ref().ok_or_else(|| "no UI".to_string())
}

/// A key pressed or let go, through Godot's input as the player's.
fn key(code: GKey, pressed: bool) {
    let mut ev = InputEventKey::new_gd();
    ev.set_keycode(code);
    ev.set_physical_keycode(code);
    ev.set_pressed(pressed);
    Input::singleton().parse_input_event(&ev);
}

/// The canvas point `at` in the window's pixels.
fn window_at(g: &RenethackGame, at: Vector2) -> Result<Vector2, String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    Ok(vp.get_screen_transform() * at)
}

/// The mouse to `at` on the canvas.
fn mouse_to(g: &RenethackGame, at: Vector2) -> Result<(), String> {
    let at = window_at(g, at)?;
    let mut ev = InputEventMouseMotion::new_gd();
    ev.set_position(at);
    ev.set_global_position(at);
    Input::singleton().parse_input_event(&ev);
    Ok(())
}

/// A mouse button pressed or let go at `at` on the canvas.
fn button(g: &RenethackGame, b: MouseButton, pressed: bool, at: Vector2) -> Result<(), String> {
    let at = window_at(g, at)?;
    let mut ev = InputEventMouseButton::new_gd();
    ev.set_button_index(b);
    ev.set_position(at);
    ev.set_global_position(at);
    ev.set_pressed(pressed);
    Input::singleton().parse_input_event(&ev);
    Ok(())
}

/// Where the mouse opens the wheels: off the screen's middle, clear of
/// its edges.
fn spot(g: &RenethackGame) -> Result<Vector2, String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let size = vp.get_visible_rect().size;
    Ok(Vector2::new(size.x * 0.5 + 80.0, size.y * 0.35))
}

/// The wheel up: its ring, its words, the wedge pointed at, its middle,
/// whether it is held.
type Shown = crate::dialogs::WheelShown;

fn wheel(g: &RenethackGame) -> Result<Option<Shown>, String> {
    Ok(ui(g)?.dialogs.wheel_shown())
}

/// The wheel on `ring`, `held` or not.
fn on_ring(g: &RenethackGame, ring: Ring, held: bool) -> Result<bool, String> {
    Ok(wheel(g)?.is_some_and(|(r, _, _, _, h)| r == ring && h == held))
}

fn pointed(g: &RenethackGame, i: usize) -> Result<bool, String> {
    Ok(wheel(g)?.is_some_and(|(_, _, hover, _, _)| hover == Some(i)))
}

/// The mouse on wedge `i`'s caption.
fn mouse_on(g: &RenethackGame, i: usize) -> Result<(), String> {
    let r = (ui(g)?.dialogs.wheel_wedge_rect(i)).ok_or_else(|| format!("no wedge {i}"))?;
    mouse_to(g, r.center())
}

/// The sections' ring, held, its middle at the mouse (or as near as the
/// screen lets it), its words the sections' and "All commands…".
fn sections_at_mouse(g: &RenethackGame) -> Result<bool, String> {
    let Some((ring, words, hover, centre, held)) = wheel(g)? else {
        return Ok(false);
    };
    let want: Vec<String> = crate::command_wheel::RING_SECTIONS
        .iter()
        .map(|s| crate::i18n::tr(s.title_id()))
        .chain(std::iter::once(tr!("wheel-all")))
        .collect();
    if ring != Ring::Sections || words != want || hover.is_some() || !held {
        return Err(format!(
            "the wheel opened on {ring:?} {words:?}, {hover:?} pointed at, held {held}"
        ));
    }
    let at = spot(g)?;
    if centre.distance_to(at) > 2.0 {
        return Err(format!("its middle at {centre:?}, the mouse at {at:?}"));
    }
    Ok(true)
}

/// The common ring: eleven commands (#pray first) and "More…".
fn common_ring(g: &RenethackGame) -> Result<bool, String> {
    if !on_ring(g, Ring::Of(Section::Common), false)? {
        return Ok(false);
    }
    let items = ui(g)?.dialogs.wheel_items().unwrap_or_default();
    let words = wheel(g)?.map(|w| w.1).unwrap_or_default();
    if items.len() != MOST
        || items.last() != Some(&WheelItem::More(Section::Common))
        || words.first() != Some(&tr!("wheel-cmd", cmd = "pray"))
    {
        return Err(format!("the common ring: {words:?}"));
    }
    Ok(true)
}

/// Back at the command prompt, no wheel up.
fn back(g: &RenethackGame) -> Result<bool, String> {
    Ok(matches!(g.pending, Some((_, Prompt::Command))) && wheel(g)?.is_none())
}

/// When the wheel held last opened (its let go waits past a tap).
static HELD_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// The stick pushed toward wedge `i` of `n`, then let go.
fn aim(i: usize, n: usize) -> [Step; 2] {
    let a = i as f32 * std::f32::consts::TAU / n as f32;
    [
        Step::Pad(vec![
            PadEv::Axis(JoyAxis::LEFT_X, a.sin()),
            PadEv::Axis(JoyAxis::LEFT_Y, -a.cos()),
        ]),
        Step::Pad(vec![
            PadEv::Axis(JoyAxis::LEFT_X, 0.0),
            PadEv::Axis(JoyAxis::LEFT_Y, 0.0),
        ]),
    ]
}

/// Under a pad, the game saved through the wheel: LT's "Commands", the
/// stick at the game's section, A, at #save (its first), A: "Really
/// save?" (the `gamepad` scenario answers it).
pub(super) fn pad_save() -> Vec<Step> {
    let mut steps = radial(7);
    steps.push(Step::Wait("LT's Commands: the wheel", |g| {
        on_ring(g, Ring::Sections, false)
    }));
    steps.extend(aim(4, 6));
    steps.push(Step::Wait("the stick at the game's", |g| pointed(g, 4)));
    steps.push(tap(PadButton::A));
    steps.push(Step::Wait("A: the game's ring", |g| {
        on_ring(g, Ring::Of(Section::Game), false)
    }));
    steps.extend(aim(0, MOST));
    steps.push(Step::Wait("the stick at #save", |g| pointed(g, 0)));
    steps.push(tap(PadButton::A));
    steps
}

pub(super) fn command_wheel() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the player's turn", back),
        Step::Call("the mouse over the map", |g| mouse_to(g, spot(g)?)),
        Step::Wait("the mouse there", |g| Ok(g.mouse_at() == Some(spot(g)?))),
        // Tab held: the wheel at the mouse
        Step::Call("Tab held", |_| {
            key(GKey::TAB, true);
            Ok(())
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the sections' ring at the mouse, held", sections_at_mouse),
        Step::Shot("wheel-sections"),
        Step::Call("→ while held", |_| {
            key(GKey::RIGHT, true);
            Ok(())
        }),
        Step::Wait("the common ones pointed at", |g| pointed(g, 0)),
        Step::Call("Tab let go", |_| {
            key(GKey::TAB, false);
            Ok(())
        }),
        Step::Wait("let go: the common ring, open", common_ring),
        Step::Shot("wheel-common"),
        // the mouse on #search, a click
        Step::Call("the mouse on #search", |g| mouse_on(g, 3)),
        Step::Wait("#search pointed at", |g| pointed(g, 3)),
        Step::Call("a click", |g| {
            let r = ui(g)?.dialogs.wheel_wedge_rect(3).ok_or("no wedge")?;
            button(g, MouseButton::LEFT, true, r.center())?;
            button(g, MouseButton::LEFT, false, r.center())
        }),
        Step::Request("a command after the search", command),
        // the middle button held, let go on "All commands…": the list
        Step::Wait("the player's turn", back),
        Step::Call("the middle button held", |g| {
            button(g, MouseButton::MIDDLE, true, spot(g)?)
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the wheel at the mouse, held", sections_at_mouse),
        Step::Call("the mouse on \"All commands…\"", |g| mouse_on(g, 5)),
        Step::Wait("\"All commands…\" pointed at", |g| pointed(g, 5)),
        Step::Call("the middle button let go", |g| {
            let r = ui(g)?.dialogs.wheel_wedge_rect(5).ok_or("no wedge")?;
            button(g, MouseButton::MIDDLE, false, r.center())
        }),
        Step::Wait("the list for the same request, every section", |g| {
            let d = &ui(g)?.dialogs;
            Ok(d.kind_name() == Some("extcmd")
                && d.palette_sections().is_some_and(|s| s.len() == 6))
        }),
        Step::Shot("wheel-all"),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after the list", command),
        // held and let go in the middle: nothing
        Step::Wait("the player's turn", back),
        Step::Call("the middle button held", |g| {
            button(g, MouseButton::MIDDLE, true, spot(g)?)
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the wheel held", sections_at_mouse),
        Step::Call("", |_| {
            *HELD_AT.lock().map_err(|e| e.to_string())? = Some(Instant::now());
            Ok(())
        }),
        Step::Wait("held past a tap", |_| {
            let at = *HELD_AT.lock().map_err(|e| e.to_string())?;
            Ok(at.is_some_and(|t| t.elapsed().as_secs_f32() > 0.5))
        }),
        Step::Call("let go in the middle", |g| {
            button(g, MouseButton::MIDDLE, false, spot(g)?)
        }),
        Step::Request("nothing: a command again", command),
        // a tap leaves it open; → Enter, ← Enter: the common ones' More
        Step::Wait("the player's turn", back),
        Step::Call("Tab tapped", |_| {
            key(GKey::TAB, true);
            key(GKey::TAB, false);
            Ok(())
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the wheel open, not held", |g| {
            on_ring(g, Ring::Sections, false)
        }),
        Step::Press(GKey::RIGHT, '\0', false),
        Step::Press(GKey::ENTER, '\0', false),
        Step::Wait("→ Enter: the common ring", common_ring),
        Step::Press(GKey::LEFT, '\0', false),
        Step::Wait("←: More…", |g| pointed(g, MOST - 1)),
        Step::Shot("wheel-more"),
        Step::Press(GKey::ENTER, '\0', false),
        Step::Wait("Enter: the list, the common ones alone open", |g| {
            let d = &ui(g)?.dialogs;
            let Some(s) = d.palette_sections() else {
                return Ok(false);
            };
            let folded: Vec<bool> = s.iter().map(|(_, f, _)| *f).collect();
            Ok(folded == [false, true, true, true, true, true]
                && d.palette_highlight() == Some(tr!("cmd-section-common")))
        }),
        Step::Shot("wheel-more-list"),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after the list", command),
        // the next # : the list with the player's folds
        Step::Press(GKey::KEY_3, '#', true),
        Step::Request("the list from #", |p| *p == Prompt::ExtCmd),
        Step::Wait("the player's folds again", |g| {
            let s = ui(g)?.dialogs.palette_sections().unwrap_or_default();
            let folded: Vec<bool> = s.iter().map(|(_, f, _)| *f).collect();
            Ok(folded == [false, false, false, false, false, true])
        }),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after the list", command),
        // Esc cancels
        Step::Wait("the player's turn", back),
        Step::Call("Tab tapped", |_| {
            key(GKey::TAB, true);
            key(GKey::TAB, false);
            Ok(())
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the wheel open", |g| on_ring(g, Ring::Sections, false)),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("Esc: a command again", command),
        Step::Wait("no wheel", back),
        // the settings' wheel: # opens it, and so does the Commands button
        Step::Call("the wheel chosen in the settings", |g| {
            let mut s = g.settings.clone();
            s.command_wheel = true;
            g.set_settings(s);
            Ok(())
        }),
        Step::Press(GKey::KEY_3, '#', true),
        Step::Request("the command request from #", |p| *p == Prompt::ExtCmd),
        Step::Wait("# : the wheel, open", |g| on_ring(g, Ring::Sections, false)),
        Step::Shot("wheel-setting"),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after Esc", command),
        Step::Wait("the player's turn", back),
        Step::Call("the Commands button clicked", |g| {
            let r = ui(g)?.hud.commands_rect().ok_or("no Commands button")?;
            button(g, MouseButton::LEFT, true, r.center())?;
            button(g, MouseButton::LEFT, false, r.center())
        }),
        Step::Request("the command request", |p| *p == Prompt::ExtCmd),
        Step::Wait("the button's wheel, by it", |g| {
            let Some((ring, _, _, centre, _)) = wheel(g)? else {
                return Ok(false);
            };
            let b = ui(g)?.hud.commands_rect().ok_or("no button")?;
            let near = centre.distance_to(b.center()) < crate::command_wheel::RING;
            if ring != Ring::Sections || !near {
                return Err(format!("the wheel at {centre:?}, the button at {b:?}"));
            }
            Ok(true)
        }),
        Step::Shot("wheel-button"),
        Step::Call("a right click", |g| {
            let at = spot(g)?;
            button(g, MouseButton::RIGHT, true, at)?;
            button(g, MouseButton::RIGHT, false, at)
        }),
        Step::Request("a right click: a command again", command),
        Step::Call("the list again in the settings", |g| {
            let mut s = g.settings.clone();
            s.command_wheel = false;
            g.set_settings(s);
            Ok(())
        }),
        // a pad: LT's Commands, the common ones, #pray: its question
        Step::Wait("the player's turn", back),
    ]);
    steps.extend(radial(7));
    steps.push(Step::Wait("LT's Commands: the wheel in the middle", |g| {
        on_ring(g, Ring::Sections, false)
    }));
    steps.extend(aim(0, 6));
    steps.extend([
        Step::Wait("the stick at the common ones", |g| pointed(g, 0)),
        Step::Shot("wheel-pad"),
        tap(PadButton::A),
        Step::Wait("A: the common ring", common_ring),
    ]);
    steps.extend(aim(0, MOST));
    steps.extend([
        Step::Wait("the stick at #pray", |g| pointed(g, 0)),
        tap(PadButton::A),
        Step::Request(
            "A: are you sure you want to pray?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("pray")),
        ),
        tap(PadButton::B),
        Step::Request("B: not praying", command),
    ]);
    // B cancels
    steps.extend(radial(7));
    steps.extend([
        Step::Wait("the wheel again", |g| on_ring(g, Ring::Sections, false)),
        tap(PadButton::B),
        Step::Request("B: a command again", command),
    ]);
    steps.extend(quit());
    steps
}
