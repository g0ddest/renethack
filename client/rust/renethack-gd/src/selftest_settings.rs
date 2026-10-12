//! The settings page: `settings` opens it over the title on its display
//! section, the window as it opened; the arrows move on it and change a
//! row, each change applied at once and kept in the profile (a frame-rate
//! cap is Godot's own, vsync off and on again with Enter); the window's
//! size and mode are asked of the window only without a display (a run in
//! a window keeps its own for its screenshots), the size's row dimmed
//! while the screen is full. Tab turns to the interface, whose language
//! draws the page again in it where it was; the start-up reads back what
//! was kept. A gamepad's bumper turns the section, its hints show, the
//! d-pad and A change a row, B goes back to the title. Over a game the
//! page opens as the HUD's gear opens it, and Esc goes back to the game.
//! Every screen fits the canvas, every value its place.

use std::sync::atomic::{AtomicU8, Ordering};

use godot::classes::display_server::VSyncMode;
use godot::classes::{DisplayServer, Engine, Input, InputEventKey};
use godot::global::Key as GKey;
use godot::prelude::*;
use nh_world::{Profile, WindowMode, step_window_size};

use super::{PadEv, Step, fail_on_error_screen, quit, screen, start};
use crate::game::{GameState, RenethackGame, parse_args, startup_settings};
use crate::gamepad::PadButton;
use crate::i18n::{self, Lang};
use crate::settings_page::{Field, Section, SettingsPage};
use crate::tr;
use crate::ui_events::UiEvent;

/// The language the run began in (`--lang`), to go back to.
static BEGAN_IN: AtomicU8 = AtomicU8::new(0);

fn lang_code(l: Lang) -> u8 {
    match l {
        Lang::En => 0,
        Lang::Ru => 1,
        Lang::Pseudo => 2,
    }
}

fn lang_of(code: u8) -> Lang {
    match code {
        1 => Lang::Ru,
        2 => Lang::Pseudo,
        _ => Lang::En,
    }
}

fn page(g: &RenethackGame) -> Result<&SettingsPage, String> {
    g.settings_page
        .as_ref()
        .ok_or_else(|| "the settings page is not open".to_string())
}

/// The profile as it was written.
fn profile(g: &RenethackGame) -> Result<Profile, String> {
    let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
    Ok(nh_link::read_profile(&pg)
        .map(|t| Profile::from_json(&t))
        .unwrap_or_default())
}

fn headless() -> bool {
    DisplayServer::singleton().get_name() == "headless"
}

/// A key pressed and let go through Godot's input (`input()` has it next
/// frame).
fn tap_key(k: GKey) {
    for pressed in [true, false] {
        let mut ev = InputEventKey::new_gd();
        ev.set_keycode(k);
        ev.set_physical_keycode(k);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
}

macro_rules! keys {
    ($what:literal, $($k:ident),+) => {
        Step::Call($what, |_| {
            $( tap_key(GKey::$k); )+
            Ok(())
        })
    };
}

fn pad_tap(b: PadButton) -> Step {
    Step::Pad(vec![PadEv::Button(b, true), PadEv::Button(b, false)])
}

/// The window's size now.
fn window(g: &RenethackGame) -> (u32, u32) {
    let s = g
        .base()
        .get_tree()
        .get_root()
        .map(|w| w.get_size())
        .unwrap_or_default();
    (s.x.max(0) as u32, s.y.max(0) as u32)
}

/// The value the page shows in the row of `field` (of the section shown).
fn shown(g: &RenethackGame, field: Field) -> Result<String, String> {
    let p = page(g)?;
    let i = p
        .section()
        .fields()
        .iter()
        .position(|&f| f == field)
        .ok_or_else(|| format!("{field:?} is not on the section shown"))?;
    Ok(p.shown_rows()[i].1.clone())
}

pub(super) fn settings() -> Vec<Step> {
    let mut steps = vec![
        Step::Call("the language the run began in", |_| {
            BEGAN_IN.store(lang_code(i18n::lang()), Ordering::Relaxed);
            Ok(())
        }),
        Step::Wait("the title screen, its scene drawn", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let drawn = ui.map.preloaded() && ui.map.title_drawn();
            Ok(screen(g) == Some("title") && drawn && g.warmed_up())
        }),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page over the title, on its display", |g| {
            Ok(screen(g) == Some("settings")
                && g.state == GameState::Title
                && page(g)?.section() == Section::Display
                && page(g)?.field() == Some(Field::WindowMode))
        }),
        Step::Call("the display's rows: the window as it opened", |g| {
            let rows = page(g)?.shown_rows();
            let (w, h) = window(g);
            let want = [
                (tr!("settings-window-mode"), tr!("settings-window-windowed")),
                (
                    tr!("settings-window-size"),
                    tr!("settings-size", width = w, height = h),
                ),
                (tr!("settings-vsync"), tr!("settings-on")),
                (tr!("settings-fps-cap"), tr!("settings-fps-none")),
            ];
            if rows != want {
                return Err(format!("the rows are {rows:?}, not {want:?}"));
            }
            Ok(())
        }),
        Step::Shot("settings-display"),
        // ↓↓↓ to the cap; → 30, → → 120, ← ← 30 again
        keys!("↓↓↓ →", DOWN, DOWN, DOWN, RIGHT),
        Step::Wait("a cap of 30 frames, applied and kept", |g| {
            Ok(page(g)?.field() == Some(Field::FpsCap)
                && Engine::singleton().get_max_fps() == 30
                && profile(g)?.settings.display.fps_cap == 30)
        }),
        keys!("→ →", RIGHT, RIGHT),
        Step::Wait("a cap of 120", |g| {
            Ok(Engine::singleton().get_max_fps() == 120
                && shown(g, Field::FpsCap)? == tr!("settings-fps", fps = 120))
        }),
        keys!("← ←", LEFT, LEFT),
        Step::Wait("a cap of 30 again", |g| {
            Ok(Engine::singleton().get_max_fps() == 30
                && profile(g)?.settings.display.fps_cap == 30)
        }),
        // ↑ to vsync; ← off; Enter steps it on, round
        keys!("↑ ←", UP, LEFT),
        Step::Wait("vsync off, applied and kept", |g| {
            let ds = DisplayServer::singleton();
            Ok(page(g)?.field() == Some(Field::Vsync)
                && !profile(g)?.settings.display.vsync
                && shown(g, Field::Vsync)? == tr!("settings-off")
                && (headless() || ds.window_get_vsync_mode() == VSyncMode::DISABLED))
        }),
        keys!("Enter", ENTER),
        Step::Wait("vsync on again", |g| {
            let ds = DisplayServer::singleton();
            Ok(profile(g)?.settings.display.vsync
                && (headless() || ds.window_get_vsync_mode() == VSyncMode::ENABLED))
        }),
        // the window's size and mode: without a display only (a window
        // keeps the size its screenshots are taken at)
        Step::Call("↑ to the size, ← a smaller window", |_| {
            if headless() {
                tap_key(GKey::UP);
                tap_key(GKey::LEFT);
            }
            Ok(())
        }),
        Step::Wait("a smaller window, kept", |g| {
            if !headless() {
                return Ok(true);
            }
            // without a display the window is tiny and stands for the
            // screen: the size steps as the model says (its own tests)
            let p = page(g)?;
            let smaller = step_window_size(window(g), p.view().screen, -1, false);
            let kept = profile(g)?.settings.display.size;
            Ok(p.field() == Some(Field::WindowSize)
                && kept == Some(smaller)
                && shown(g, Field::WindowSize)?
                    == tr!("settings-size", width = smaller.0, height = smaller.1))
        }),
        Step::Call("↑ to the mode, → the full screen", |_| {
            if headless() {
                tap_key(GKey::UP);
                tap_key(GKey::RIGHT);
            }
            Ok(())
        }),
        Step::Wait("borderless, kept; the size's row dimmed", |g| {
            if !headless() {
                return Ok(true);
            }
            Ok(profile(g)?.settings.display.mode == WindowMode::Borderless
                && shown(g, Field::WindowMode)? == tr!("settings-window-borderless"))
        }),
        Step::Call("no size to choose in a full screen", |g| {
            if !headless() {
                return Ok(());
            }
            let before = g.settings;
            g.settings_page
                .as_mut()
                .ok_or("no page")?
                .focus_field(Field::WindowSize);
            let k = nh_world::KeyInput::plain(nh_world::Key::Left);
            let asked = g.settings_page.as_mut().ok_or("no page")?.key(&k);
            if asked
                != Some(crate::settings_page::PageKey::Step(
                    Field::WindowSize,
                    -1,
                    false,
                ))
            {
                return Err(format!("← on the size's row asks {asked:?}"));
            }
            let view = g.settings_page.as_ref().ok_or("no page")?.view().clone();
            if crate::settings_page::step(Field::WindowSize, &view, -1, false).is_some() {
                return Err("the size changes in a full screen".into());
            }
            if g.settings != before {
                return Err("the settings changed".into());
            }
            g.settings_page
                .as_mut()
                .ok_or("no page")?
                .focus_field(Field::WindowMode);
            Ok(())
        }),
        Step::Call("← back to a window", |_| {
            if headless() {
                tap_key(GKey::LEFT);
            }
            Ok(())
        }),
        Step::Wait("windowed again, kept", |g| {
            if !headless() {
                return Ok(true);
            }
            Ok(profile(g)?.settings.display.mode == WindowMode::Windowed)
        }),
        // what was kept is what a start reads
        Step::Call("the start-up reads the settings kept", |g| {
            let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
            let args = parse_args(&[format!("--playground={}", pg.display())]);
            let read = startup_settings(&args);
            if read != g.settings {
                return Err(format!(
                    "the start reads {read:?}, the page has {:?}",
                    g.settings
                ));
            }
            Ok(())
        }),
        // Tab: the interface, its language; the other language draws the
        // page again in it, on the same row
        keys!("Tab", TAB),
        Step::Wait("the interface's rows", |g| {
            Ok(page(g)?.section() == Section::Interface
                && page(g)?.field() == Some(Field::Language))
        }),
        Step::Shot("settings-interface"),
        Step::Call("the other language", |_| {
            // from English (or the pseudo-language) to Russian, from
            // Russian to English
            tap_key(if i18n::lang() == Lang::Ru {
                GKey::LEFT
            } else {
                GKey::RIGHT
            });
            Ok(())
        }),
        Step::Wait("the page again in the other language, where it was", |g| {
            let began = lang_of(BEGAN_IN.load(Ordering::Relaxed));
            let other = if began == Lang::Ru {
                Lang::En
            } else {
                Lang::Ru
            };
            let p = page(g)?;
            Ok(i18n::lang() == other
                && p.section() == Section::Interface
                && p.field() == Some(Field::Language)
                && p.shown_rows() == [(tr!("settings-language"), other.name().to_string())]
                && profile(g)?.lang.as_deref() == Some(other.code()))
        }),
        Step::Shot("settings-interface-other"),
        Step::Call("back to the language the run began in", |g| {
            let began = lang_of(BEGAN_IN.load(Ordering::Relaxed));
            g.push_ui(UiEvent::SetLanguage(began));
            Ok(())
        }),
        Step::Wait("the page in it again", |g| {
            let began = lang_of(BEGAN_IN.load(Ordering::Relaxed));
            Ok(i18n::lang() == began && page(g)?.section() == Section::Interface)
        }),
        // a gamepad: LB turns back to the display, its hints show; ↓↓ to
        // vsync, A turns it off and on
        pad_tap(PadButton::Lb),
        Step::Wait("the display again, a gamepad's hints", |g| {
            let p = page(g)?;
            Ok(p.section() == Section::Display && p.pad().is_some())
        }),
        Step::Shot("settings-pad"),
        pad_tap(PadButton::Down),
        pad_tap(PadButton::Down),
        Step::Wait("vsync has the focus", |g| {
            Ok(page(g)?.field() == Some(Field::Vsync))
        }),
        pad_tap(PadButton::A),
        Step::Wait("A turns vsync off", |g| {
            Ok(!profile(g)?.settings.display.vsync)
        }),
        pad_tap(PadButton::A),
        Step::Wait("and on", |g| Ok(profile(g)?.settings.display.vsync)),
        pad_tap(PadButton::B),
        Step::Wait("B: the title again", |g| {
            Ok(screen(g) == Some("title") && g.settings_page.is_none())
        }),
    ];
    // over a game: from the HUD's gear, Esc goes back to it
    steps.extend(start());
    steps.extend([
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page over the game", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("settings")
                && ui.screens.settings_in_game()
                && g.state == GameState::Playing)
        }),
        Step::Shot("settings-in-game"),
        keys!("Esc", ESCAPE),
        Step::Wait("the game again", |g| {
            Ok(screen(g).is_none() && g.settings_page.is_none() && g.state == GameState::Playing)
        }),
    ]);
    steps.extend(quit());
    steps
}
