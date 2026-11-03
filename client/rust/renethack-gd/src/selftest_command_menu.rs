//! The command menu (`palette`'s second half): opened from the HUD's
//! Commands button it hangs above the button, its search waiting
//! unfocused; it lists every command once, in sections, the moves
//! folded; a click on a row runs that command, and so do the arrows and
//! Enter (← and → fold and unfold a section); the wheel scrolls it; a
//! letter typed goes to its search, which lists the matches alone, and
//! cleared brings the sections back; the button again puts it away.

use godot::classes::{Input, InputEventMouseButton};
use godot::global::{Key as GKey, MouseButton};
use godot::prelude::*;
use nh_world::Prompt;

use super::{Step, command, key, press};
use crate::game::RenethackGame;
use crate::tr;

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

/// The wheel turned down a notch over `at`.
fn wheel(g: &RenethackGame, at: Vector2) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    for pressed in [true, false] {
        let mut ev = InputEventMouseButton::new_gd();
        ev.set_button_index(MouseButton::WHEEL_DOWN);
        ev.set_factor(1.0);
        ev.set_position(at);
        ev.set_global_position(at);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
    Ok(())
}

fn ui(g: &RenethackGame) -> Result<&crate::game::Ui, String> {
    g.ui.as_ref().ok_or_else(|| "no UI".to_string())
}

/// The HUD's Commands button clicked.
fn click_button(g: &mut RenethackGame) -> Result<(), String> {
    let r = ui(g)?.hud.commands_rect().ok_or("no Commands button")?;
    click(g, r.center())
}

/// Command `name`'s row clicked, or the heading with these words.
fn click_row(g: &RenethackGame, name: &str) -> Result<(), String> {
    let r = (ui(g)?.dialogs.palette_row_rect(name))
        .ok_or_else(|| format!("no row for {name} in sight"))?;
    click(g, r.center())
}

/// The menu open, every command listed once, in sections as the keys
/// show them: the common ones first and open, the moves last and folded.
fn sections_as_keys_show(g: &RenethackGame) -> Result<bool, String> {
    let d = &ui(g)?.dialogs;
    let Some(sections) = d.palette_sections() else {
        return Ok(false);
    };
    let names = d.palette_names().unwrap_or_default();
    let cmds = crate::dialogs::palette_cmds(g.catalog.as_deref(), g.keymap());
    if names.len() != cmds.len() {
        return Err(format!(
            "the menu lists {} commands of {}",
            names.len(),
            cmds.len()
        ));
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    if sorted.len() != names.len() {
        return Err("a command listed twice".into());
    }
    let titles: Vec<&str> = sections.iter().map(|(t, _, _)| t.as_str()).collect();
    let want = [
        tr!("cmd-section-common"),
        tr!("cmd-section-items"),
        tr!("cmd-section-actions"),
        tr!("cmd-section-info"),
        tr!("cmd-section-game"),
        tr!("cmd-section-moves"),
    ];
    if titles != want {
        return Err(format!("the sections read {titles:?}, not {want:?}"));
    }
    let folded: Vec<bool> = sections.iter().map(|(_, f, _)| *f).collect();
    if folded != [false, false, false, false, false, true] {
        return Err(format!("folded: {folded:?} (the moves alone at first)"));
    }
    if sections[0].2.first().map(String::as_str) != Some("pray") {
        return Err(format!(
            "the common section starts {:?}",
            sections[0].2.first()
        ));
    }
    Ok(true)
}

/// The menu hangs above the button, inside the canvas, nothing typed and
/// its search not taking the keyboard.
fn hangs_from_button(g: &RenethackGame) -> Result<bool, String> {
    let ui = ui(g)?;
    if !ui.dialogs.palette_dropdown() {
        return Ok(false);
    }
    let panel = ui.dialogs.panel_rect().ok_or("no panel")?;
    let button = ui.hud.commands_rect().ok_or("no button")?;
    let canvas = g
        .base()
        .get_viewport()
        .map(|v| v.get_visible_rect())
        .ok_or("no viewport")?;
    if panel.size.y < 100.0 {
        // laid out at the frame's end
        return Ok(false);
    }
    if panel.end().y > button.position.y || !canvas.encloses(panel) {
        return Err(format!(
            "the menu at {:?} {:?}, the button at {:?}: not above it in the canvas",
            panel.position, panel.size, button.position
        ));
    }
    if (panel.end().x - button.end().x).abs() > 1.0 && panel.end().x < canvas.end().x - 13.0 {
        return Err(format!(
            "the menu's right edge at {:.0}, the button's at {:.0}",
            panel.end().x,
            button.end().x
        ));
    }
    if ui.dialogs.text_has_focus() || ui.dialogs.text().as_deref() != Some("") {
        return Err("the search takes the keyboard, or holds text, at first".into());
    }
    Ok(same_as_before(panel))
}

/// The panel where the last look saw it: laid out, no longer growing.
fn same_as_before(panel: Rect2) -> bool {
    let mut last = LAST_PANEL.lock().unwrap_or_else(|e| e.into_inner());
    let same = *last == Some(panel);
    *last = Some(panel);
    same
}

static LAST_PANEL: std::sync::Mutex<Option<Rect2>> = std::sync::Mutex::new(None);

/// The highlighted row reads `want` ("#pray", or a heading's words).
fn highlight(g: &RenethackGame, want: &str) -> Result<bool, String> {
    Ok(ui(g)?.dialogs.palette_highlight().as_deref() == Some(want))
}

/// The palette open with a list (its rows laid out).
fn palette_up(g: &RenethackGame) -> Result<bool, String> {
    let d = &ui(g)?.dialogs;
    Ok(d.kind_name() == Some("extcmd") && d.palette_view().is_some_and(|(r, _)| r.size.y > 0.0))
}

/// The steps: from a command prompt, back at one.
pub(super) fn menu_steps() -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the player's turn", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && !g.world.getpos)
        }),
        // the button: the menu hangs above it, its sections as the keys
        // show them
        Step::Call("the Commands button clicked", click_button),
        Step::Request("the command menu", |p| *p == Prompt::ExtCmd),
        Step::Wait(
            "hung above the button, the search waiting",
            hangs_from_button,
        ),
        Step::Wait("every command in its section", sections_as_keys_show),
        Step::Shot("command-menu-button"),
        // the wheel scrolls the list
        Step::Call("the wheel turned over the list", |g| {
            let (r, _) = ui(g)?.dialogs.palette_view().ok_or("no list")?;
            wheel(g, r.center())
        }),
        Step::Wait("the list scrolled", |g| {
            Ok(ui(g)?.dialogs.palette_top().is_some_and(|t| t > 0.0))
        }),
        // a heading clicked folds its section, again unfolds it
        Step::Call("the items' heading clicked", |g| {
            click_row(g, &tr!("cmd-section-items"))
        }),
        Step::Wait("the items folded", |g| {
            let s = ui(g)?.dialogs.palette_sections().unwrap_or_default();
            Ok(s.get(1).is_some_and(|(_, folded, _)| *folded))
        }),
        Step::Call("the heading clicked again", |g| {
            click_row(g, &tr!("cmd-section-items"))
        }),
        Step::Wait("the items open again", |g| {
            let s = ui(g)?.dialogs.palette_sections().unwrap_or_default();
            Ok(s.get(1).is_some_and(|(_, folded, _)| !*folded))
        }),
        // the button again: the menu goes
        Step::Call("the Commands button clicked again", click_button),
        Step::Request("a command after the menu put away", command),
        // a row clicked runs its command: a search
        Step::Call("the Commands button clicked", click_button),
        Step::Request("the command menu", |p| *p == Prompt::ExtCmd),
        Step::Wait("hung above the button", hangs_from_button),
        Step::Call("#search's row clicked", |g| click_row(g, "search")),
        Step::Request("a command after the search", command),
        // the keys: `#`, ↓ to #pray, ← folds the common ones (the heading
        // highlighted), → unfolds them, ↓ and Enter pray
        Step::Press(GKey::KEY_3, '#', true),
        Step::Request("the command menu from #", |p| *p == Prompt::ExtCmd),
        Step::Wait("the menu centred, laid out", |g| {
            let panel = ui(g)?
                .dialogs
                .panel_rect()
                .unwrap_or(Rect2::new(Vector2::ZERO, Vector2::ZERO));
            Ok(palette_up(g)? && !ui(g)?.dialogs.palette_dropdown() && same_as_before(panel))
        }),
        Step::Press(GKey::DOWN, '\0', false),
        Step::Press(GKey::DOWN, '\0', false),
        Step::Wait("↓ ↓ on #pray", |g| highlight(g, "#pray")),
        Step::Press(GKey::LEFT, '\0', false),
        Step::Wait(
            "← : the common ones folded, their heading highlighted",
            |g| {
                let s = ui(g)?.dialogs.palette_sections().unwrap_or_default();
                let folded = s.first().is_some_and(|(_, f, _)| *f);
                Ok(folded && highlight(g, &tr!("cmd-section-common"))?)
            },
        ),
        Step::Shot("command-menu-folded"),
        Step::Press(GKey::RIGHT, '\0', false),
        Step::Press(GKey::DOWN, '\0', false),
        Step::Wait("→ ↓ : unfolded, on #pray", |g| {
            let s = ui(g)?.dialogs.palette_sections().unwrap_or_default();
            let open = s.first().is_some_and(|(_, f, _)| !*f);
            Ok(open && highlight(g, "#pray")?)
        }),
        Step::Press(GKey::ENTER, '\0', false),
        Step::Request(
            "Enter: are you sure you want to pray?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("pray")),
        ),
        key('n'),
        Step::Request("a command after not praying", command),
        // typing: a letter goes to the search, which lists the matches
        // alone; cleared, the sections are back
        Step::Press(GKey::KEY_3, '#', true),
        Step::Request("the command menu again", |p| *p == Prompt::ExtCmd),
        Step::Wait("laid out", palette_up),
    ];
    steps.extend("lo".chars().map(press));
    steps.extend([
        Step::Wait("\"lo\" typed into the search: the matches alone", |g| {
            let d = &ui(g)?.dialogs;
            let names = d.palette_names().unwrap_or_default();
            Ok(d.text().as_deref() == Some("lo")
                && d.text_has_focus()
                && d.palette_sections().is_some_and(|s| s.is_empty())
                && names.first().map(String::as_str) == Some("look"))
        }),
        Step::Shot("command-menu-typed"),
        Step::Press(GKey::BACKSPACE, '\0', false),
        Step::Press(GKey::BACKSPACE, '\0', false),
        Step::Wait("the search cleared: the sections back", |g| {
            let d = &ui(g)?.dialogs;
            Ok(d.text().as_deref() == Some("") && sections_as_keys_show(g)?)
        }),
        Step::Press(GKey::ESCAPE, '\0', false),
        Step::Request("a command after Esc", command),
    ]);
    steps
}
