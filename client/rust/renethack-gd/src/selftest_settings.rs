//! The settings page: `settings` opens it over the title on its display
//! section, the window as it opened; the arrows move on it and change a
//! row, each change applied at once and kept in the profile (a frame-rate
//! cap is Godot's own, vsync off and on again with Enter); the window's
//! size and mode are asked of the window only without a display (a run in
//! a window keeps its own for its screenshots), the size's row dimmed
//! while the screen is full; the start-up reads back what was kept. Tab
//! turns to the graphics, the device's High: Medium and Low step the
//! renderer and the map down (their effects, anti-aliasing, resolution,
//! shadows), a control changed by hand makes them Custom, and → from it
//! climbs back through the presets. The interface's language draws the
//! page again in it where it was; fewer screen effects are kept. A
//! gamepad's bumpers turn the section, its hints show, the d-pad and A
//! change a row, B goes back to the title. The controls list the Classic
//! keys a new character starts with, then the gamepad's (the Gameplay
//! section between them has a scenario of its own, `gameplay`). Over a game the
//! page opens as the HUD's gear opens it, and Esc goes back to the game.
//! Every screen fits the canvas, every value its place.

use std::sync::atomic::{AtomicU8, Ordering};

use godot::classes::display_server::VSyncMode;
use godot::classes::viewport::{Msaa as ViewportMsaa, Scaling3DMode};
use godot::classes::{DisplayServer, Engine, Input, InputEventKey};
use godot::global::Key as GKey;
use godot::prelude::*;
use nh_world::{
    Graphics, Key, KeyInput, KeyProfile, Level, Msaa, Profile, Quality, Upscale, WindowMode,
    step_window_size,
};

use super::{PadEv, Step, fail_on_error_screen, quit, screen, start};
use crate::audio::Bus;
use crate::bindings::Shown;
use crate::game::{
    GameState, RenethackGame, load_profile, parse_args, startup_lang, startup_settings,
};
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

pub(super) fn page(g: &RenethackGame) -> Result<&SettingsPage, String> {
    g.settings_page
        .as_ref()
        .ok_or_else(|| "the settings page is not open".to_string())
}

/// The profile as it was written.
pub(super) fn profile(g: &RenethackGame) -> Result<Profile, String> {
    let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
    Ok(crate::game::load_profile(&pg))
}

fn headless() -> bool {
    DisplayServer::singleton().get_name() == "headless"
}

/// A key pressed and let go through Godot's input (`input()` has it next
/// frame).
pub(super) fn tap_key(k: GKey) {
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

pub(super) fn pad_tap(b: PadButton) -> Step {
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

/// The renderer and the map draw as `g` asks: the window's anti-aliasing,
/// resolution, upscaler and shadow atlas, the map's effects and its
/// torches with shadows.
fn drawn_as(game: &RenethackGame, g: &Graphics) -> Result<bool, String> {
    let w = game.base().get_tree().get_root().ok_or("no window")?;
    let msaa = match g.msaa {
        Msaa::Off => ViewportMsaa::DISABLED,
        Msaa::X2 => ViewportMsaa::MSAA_2X,
        Msaa::X4 => ViewportMsaa::MSAA_4X,
        Msaa::X8 => ViewportMsaa::MSAA_8X,
    };
    let mode = match g.upscale {
        Upscale::Bilinear => Scaling3DMode::BILINEAR,
        Upscale::Fsr1 => Scaling3DMode::FSR,
        Upscale::Fsr2 => Scaling3DMode::FSR2,
    };
    let ui = game.ui.as_ref().ok_or("no UI")?;
    let effects = (
        g.ssao != Level::Off,
        g.ssil != Level::Off,
        g.fog != Level::Off,
    );
    Ok(w.get_msaa_3d() == msaa
        && (w.get_scaling_3d_scale() - g.scale as f32 / 100.0).abs() < 1e-3
        && w.get_scaling_3d_mode() == mode
        && w.get_positional_shadow_atlas_size() == g.shadows.atlas() as i32
        && ui.map.effects() == effects
        && ui.map.torch_shadows() == g.shadows.torches())
}

/// The graphics are the preset `q`: drawn so, shown so, kept so.
fn preset_is(game: &RenethackGame, q: Quality) -> Result<bool, String> {
    let g = Graphics::preset(q);
    let name = match q {
        Quality::Low => tr!("settings-quality-low"),
        Quality::Medium => tr!("settings-quality-medium"),
        Quality::High => tr!("settings-quality-high"),
        Quality::Ultra => tr!("settings-quality-ultra"),
        Quality::Custom => tr!("settings-quality-custom"),
    };
    Ok(game.graphics() == g
        && drawn_as(game, &g)?
        && shown(game, Field::Quality)? == name
        && profile(game)?.settings.graphics == Some(g))
}

/// The interface's scale now.
fn content_scale(g: &RenethackGame) -> Result<f32, String> {
    let w = g.base().get_tree().get_root().ok_or("no window")?;
    Ok(w.get_content_scale_factor())
}

/// The value the page shows in the row of `field` (of the section shown).
pub(super) fn shown(g: &RenethackGame, field: Field) -> Result<String, String> {
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
            Ok(
                profile(g)?.settings.display.mode == Some(WindowMode::Borderless)
                    && shown(g, Field::WindowMode)? == tr!("settings-window-borderless"),
            )
        }),
        Step::Call("no size to choose in a full screen", |g| {
            if !headless() {
                return Ok(());
            }
            let before = g.settings.clone();
            g.settings_page
                .as_mut()
                .ok_or("no page")?
                .focus_field(Field::WindowSize);
            let k = nh_world::KeyInput::plain(nh_world::Key::Left);
            let asked = g.settings_page.as_mut().ok_or("no page")?.key(&k, false);
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
            Ok(profile(g)?.settings.display.mode == Some(WindowMode::Windowed))
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
        // Tab: the graphics, the device's preset (High on this machine)
        keys!("Tab", TAB),
        Step::Wait("the graphics' rows: the device's High, drawn so", |g| {
            let p = page(g)?;
            let high = Graphics::preset(Quality::High);
            Ok(p.section() == Section::Graphics
                && p.field() == Some(Field::Quality)
                && shown(g, Field::Quality)? == tr!("settings-quality-high")
                && g.graphics() == high
                && drawn_as(g, &high)?
                && profile(g)?.settings.graphics.is_none())
        }),
        Step::Shot("settings-graphics"),
        keys!("←", LEFT),
        Step::Wait("Medium, drawn and kept", |g| preset_is(g, Quality::Medium)),
        keys!("←", LEFT),
        Step::Wait("Low, drawn and kept", |g| preset_is(g, Quality::Low)),
        // a control by hand: Custom; → from it climbs to the nearest preset
        keys!("↓ →", DOWN, RIGHT),
        Step::Wait("ambient occlusion by hand: Custom", |g| {
            let gr = g.graphics();
            Ok(page(g)?.field() == Some(Field::Ssao)
                && gr.ssao == Level::Low
                && gr.quality() == Quality::Custom
                && drawn_as(g, &gr)?
                && shown(g, Field::Quality)? == tr!("settings-quality-custom")
                && profile(g)?.settings.graphics == Some(gr))
        }),
        Step::Shot("settings-graphics-custom"),
        keys!("↑ →", UP, RIGHT),
        Step::Wait("from Custom to Medium", |g| preset_is(g, Quality::Medium)),
        keys!("→", RIGHT),
        Step::Wait("High again", |g| preset_is(g, Quality::High)),
        // Tab: the sound; the music a step quieter with ←, then dragged
        // to 30 % as the mouse would
        keys!("Tab", TAB),
        Step::Wait("the sound's rows, the music at its default", |g| {
            let p = page(g)?;
            Ok(p.section() == Section::Sound
                && p.field() == Some(Field::Volume(Bus::Master))
                && (crate::audio::volume(Bus::Music) - Bus::Music.default_volume()).abs() < 0.01)
        }),
        keys!("↓ ←", DOWN, LEFT),
        Step::Wait("the music at 65 %, heard so and shown so", |g| {
            Ok(page(g)?.field() == Some(Field::Volume(Bus::Music))
                && (crate::audio::volume(Bus::Music) - 0.65).abs() < 0.01
                && shown(g, Field::Volume(Bus::Music))? == tr!("settings-percent", percent = 65))
        }),
        Step::Call("the music's slider dragged to 30 %", |g| {
            let mut s = page(g)?
                .slider(Bus::Music)
                .ok_or("the music has no slider")?;
            s.set_value(0.3);
            Ok(())
        }),
        Step::Wait(
            "the music at 30 %, the slider and the percent with it",
            |g| {
                let s = page(g)?.slider(Bus::Music).ok_or("no slider")?;
                Ok((crate::audio::volume(Bus::Music) - 0.3).abs() < 0.01
                    && (s.get_value() - 0.3).abs() < 0.01
                    && shown(g, Field::Volume(Bus::Music))?
                        == tr!("settings-percent", percent = 30))
            },
        ),
        Step::Shot("settings-sound"),
        // Tab: the interface; its scale from Auto up to 140 % (the page
        // still fits the canvas) and back; Classic keys for a new character
        keys!("Tab", TAB),
        Step::Wait("the interface's rows, the scale Auto", |g| {
            let p = page(g)?;
            let auto = tr!("settings-ui-scale-auto", percent = p.view().ui_auto);
            Ok(p.section() == Section::Interface
                && p.field() == Some(Field::UiScale)
                && shown(g, Field::UiScale)? == auto)
        }),
        keys!(
            "→ seven times",
            RIGHT,
            RIGHT,
            RIGHT,
            RIGHT,
            RIGHT,
            RIGHT,
            RIGHT
        ),
        Step::Wait("the interface at 140 %, kept", |g| {
            Ok((content_scale(g)? - 1.4).abs() < 1e-3
                && profile(g)?.settings.ui_scale == Some(140)
                && shown(g, Field::UiScale)? == tr!("settings-percent", percent = 140))
        }),
        Step::Shot("settings-interface-140"),
        keys!("← seven times", LEFT, LEFT, LEFT, LEFT, LEFT, LEFT, LEFT),
        Step::Wait("Auto again", |g| {
            let auto = page(g)?.view().ui_auto as f32 / 100.0;
            Ok((content_scale(g)? - auto).abs() < 1e-3 && profile(g)?.settings.ui_scale.is_none())
        }),
        keys!("↓ →", DOWN, RIGHT),
        Step::Wait("Classic keys for a new character, kept", |g| {
            Ok(page(g)?.field() == Some(Field::Keys)
                && profile(g)?.settings.keys == Some(KeyProfile::Classic)
                && shown(g, Field::Keys)? == tr!("settings-keys-classic"))
        }),
        // ↓ to the language; the other language draws the page again in
        // it, on the same row
        keys!("↓", DOWN),
        Step::Wait("the language has the focus", |g| {
            Ok(page(g)?.field() == Some(Field::Language))
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
            let row = (tr!("settings-language"), other.name().to_string());
            Ok(i18n::lang() == other
                && p.section() == Section::Interface
                && p.field() == Some(Field::Language)
                && p.shown_rows().contains(&row)
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
        // ↓ to fewer screen effects: → on, kept; ← off again
        keys!("↓ →", DOWN, RIGHT),
        Step::Wait("fewer screen effects, kept", |g| {
            Ok(page(g)?.field() == Some(Field::ReduceEffects)
                && profile(g)?.settings.reduce_effects
                && g.settings.reduce_effects
                && shown(g, Field::ReduceEffects)? == tr!("settings-on"))
        }),
        Step::Shot("settings-interface-effects"),
        keys!("←", LEFT),
        Step::Wait("the screen's effects again, kept", |g| {
            Ok(!profile(g)?.settings.reduce_effects
                && shown(g, Field::ReduceEffects)? == tr!("settings-off"))
        }),
        // Tab: the gameplay (its own scenario), Tab: the controls, a new
        // character's Classic keys first; → the gamepad's; ↓ through the list
        keys!("Tab", TAB),
        Step::Wait("the gameplay", |g| {
            Ok(page(g)?.section() == Section::Gameplay)
        }),
        keys!("Tab", TAB),
        Step::Wait("the controls: the Classic keys", |g| {
            let p = page(g)?;
            let list = p.control_texts();
            let west = KeyInput::plain(Key::Char('h'));
            let slot = KeyInput::plain(Key::Char('1'));
            let entry = |k| p.entry_of(k).and_then(|i| list.get(i));
            Ok(p.section() == Section::Controls
                && p.controls() == Shown::Keys(KeyProfile::Classic)
                && shown(g, Field::Controls)? == tr!("controls-shown-classic")
                && list.first() == Some(&tr!("controls-reset-all"))
                && entry(west).is_some_and(|e| e.starts_with("h "))
                && entry(slot).is_some_and(|e| e.ends_with(&tr!("controls-slot", n = 1))))
        }),
        Step::Shot("settings-controls"),
        keys!("→", RIGHT),
        Step::Wait("the gamepad's controls", |g| {
            let p = page(g)?;
            let list = p.control_texts();
            Ok(p.controls() == Shown::Pad
                && list
                    .first()
                    .is_some_and(|e| e.ends_with(&tr!("controls-pad-walk")))
                && list
                    .iter()
                    .any(|e| e.ends_with(&tr!("controls-pad-radial"))))
        }),
        keys!(
            "↓ ↓ ↓ ↓ ↓ ↓ ↓ ↓ ↓ ↓ ↓ ↓",
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN,
            DOWN
        ),
        Step::Wait("the focus down the list, the row no longer", |g| {
            Ok(page(g)?.field().is_none())
        }),
        Step::Shot("settings-controls-pad"),
        // a gamepad: LB turns back to the gameplay, its hints show, then
        // to the interface, the sound, the graphics and the display; ↓↓ to
        // vsync, A turns it off and on
        pad_tap(PadButton::Lb),
        Step::Wait("the gameplay again, a gamepad's hints", |g| {
            let p = page(g)?;
            Ok(p.section() == Section::Gameplay && p.pad().is_some())
        }),
        pad_tap(PadButton::Lb),
        Step::Wait("the interface again", |g| {
            Ok(page(g)?.section() == Section::Interface)
        }),
        pad_tap(PadButton::Lb),
        Step::Wait("the sound again", |g| {
            Ok(page(g)?.section() == Section::Sound)
        }),
        pad_tap(PadButton::Lb),
        Step::Wait("the graphics again", |g| {
            Ok(page(g)?.section() == Section::Graphics)
        }),
        pad_tap(PadButton::Lb),
        Step::Wait("the display again", |g| {
            Ok(page(g)?.section() == Section::Display)
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
        Step::Wait("B: the title again, the volumes kept", |g| {
            Ok(screen(g) == Some("title")
                && g.settings_page.is_none()
                && profile(g)?.volumes.get("Music") == Some(&30))
        }),
        // a new character is offered the keys chosen
        Step::Push(UiEvent::NewGame),
        Step::Wait("the creation form offers Classic keys", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("creation")
                && ui
                    .screens
                    .creation_choice()
                    .is_some_and(|c| c.profile == KeyProfile::Classic))
        }),
        Step::Push(UiEvent::BackToTitle),
    ];
    // over a game: from the HUD's gear, Esc goes back to it
    steps.extend(start());
    steps.extend([
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings page over the game", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("settings")
                && ui.screens.settings_in_game()
                && g.state == GameState::Playing
                && page(g)?.view().playing_keys == Some(KeyProfile::Classic)
                // the controls: the gamepad's while it is in hand, else
                // the character's own keys
                && page(g)?.controls()
                    == if g.pad.active {
                        Shown::Pad
                    } else {
                        Shown::Keys(KeyProfile::Classic)
                    })
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

/// `deck`, run as a Steam Deck (`SteamDeck=1`): while the player has not
/// chosen a window, the game covers the Deck's screen at its own size, no
/// window size set (gamescope would show the project's 1600×900 window
/// letterboxed at 1280×720, resampled, the Deck drawing 40 % more pixels
/// than its screen has): the window is the screen's size (without a
/// display the run's 1280×800 stands for it), the canvas has its shape
/// (no bars), the page's window row says so and has no size to choose,
/// the interface at the Deck's 120 % and the graphics at its preset. A
/// window chosen on the page is asked for and kept, and the next start
/// keeps the player's choice.
pub(super) fn deck() -> Vec<Step> {
    vec![
        Step::Wait("the title screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("title"))
        }),
        Step::Call(
            "a Deck, its window not chosen: asked to cover the screen",
            |g| {
                if !crate::video::is_deck() {
                    return Err("not a Steam Deck: run with SteamDeck=1".into());
                }
                if g.settings.display.mode.is_some() {
                    return Err(format!("a window chosen: {:?}", g.settings.display.mode));
                }
                match (crate::video::asked_mode(), crate::video::asked_size()) {
                    (Some(WindowMode::Borderless), None) => Ok(()),
                    other => Err(format!("the window was asked for {other:?}")),
                }
            },
        ),
        Step::Wait("the screen's own size, nothing letterboxed", |g| {
            // without a display, the run's window stands for the Deck's
            // screen (--size=1280x800)
            let native = if headless() {
                crate::video::DECK_SCREEN
            } else {
                crate::video::screen_size()
            };
            let (w, h) = window(g);
            let root = g.base().get_tree().get_root().ok_or("no window")?;
            // the canvas has the window's shape: no bars above or beside it
            let seen = root.get_visible_rect().size;
            let shaped = (seen.x / seen.y - w as f32 / h as f32).abs() < 0.01;
            Ok((w, h) == native && shaped)
        }),
        Step::Wait("the Deck's interface scale and graphics", |g| {
            Ok((content_scale(g)? - 1.2).abs() < 1e-3
                && g.graphics() == Graphics::preset(crate::video::DECK_QUALITY))
        }),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the page: the whole screen, no size to choose", |g| {
            let p = page(g)?;
            Ok(p.field() == Some(Field::WindowMode)
                && shown(g, Field::WindowMode)? == tr!("settings-window-borderless")
                && !p.row_live(Field::WindowSize)
                && profile(g)?.settings.display.mode.is_none())
        }),
        Step::Shot("deck-settings"),
        keys!("← a window", LEFT),
        Step::Wait("a window chosen: asked for and kept", |g| {
            Ok(
                profile(g)?.settings.display.mode == Some(WindowMode::Windowed)
                    && crate::video::asked_mode() == Some(WindowMode::Windowed)
                    && shown(g, Field::WindowMode)? == tr!("settings-window-windowed")
                    && page(g)?.row_live(Field::WindowSize),
            )
        }),
        Step::Call("the next start keeps the player's window", |g| {
            let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
            let args = parse_args(&[format!("--playground={}", pg.display())]);
            let mode = startup_settings(&args).display.window_mode(true);
            if mode != WindowMode::Windowed {
                return Err(format!("the next start asks for {mode:?}"));
            }
            Ok(())
        }),
        keys!("Esc", ESCAPE),
        Step::Wait("the title again", |g| {
            Ok(screen(g) == Some("title") && g.settings_page.is_none())
        }),
    ]
}

/// A profile with every value set, none at its default, as an older
/// version kept it in one file.
fn old_profile() -> Profile {
    let mut keymaps = nh_world::Keymaps::default();
    keymaps.get_mut(KeyProfile::Classic).rebind(
        KeyInput::plain(Key::Char('s')),
        KeyInput::plain(Key::Char('g')),
        false,
    );
    Profile {
        lang: Some("ru".into()),
        last_role: Some("Wiz".into()),
        last_female: Some(true),
        settings: nh_world::Settings {
            display: nh_world::Display {
                mode: Some(WindowMode::Exclusive),
                size: Some((1600, 900)),
                vsync: false,
                fps_cap: 60,
            },
            graphics: Some(Graphics::preset(Quality::Low)),
            ui_scale: Some(130),
            reduce_effects: true,
            keys: Some(KeyProfile::Classic),
            keymaps,
            gameplay: nh_world::Gameplay {
                autopickup: true,
                pickup_types: "$".into(),
                paranoid_pray: false,
                ..nh_world::Gameplay::default()
            },
        },
        volumes: [("Music".to_string(), 40), ("SFX".to_string(), 100)].into(),
    }
}

/// `profile-files`: an older version's profile, every value set, is split
/// as a start splits it into the player's file (Steam Cloud syncs it) and
/// the device's (one machine's window, graphics and interface scale): every
/// value kept, each in its own file, the old file gone; a start reads them
/// all back, and the game writing its settings keeps them.
pub(super) fn profile_files() -> Vec<Step> {
    vec![
        Step::Wait("the title screen", |g| {
            fail_on_error_screen(g)?;
            Ok(screen(g) == Some("title"))
        }),
        Step::Call("an older version's profile split, every value kept", |g| {
            let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
            let old = old_profile();
            std::fs::write(pg.join("profile.json"), old.to_json()).map_err(|e| e.to_string())?;
            let read = load_profile(&pg);
            if read != old {
                return Err(format!("read {read:?}, kept {old:?}"));
            }
            if pg.join("profile.json").exists() {
                return Err("the old file is still there".into());
            }
            // each part in its own file
            let (player, device) = nh_link::read_profile(&pg);
            let (player, device) = (player.unwrap_or_default(), device.unwrap_or_default());
            for k in nh_world::DEVICE_KEYS {
                let key = format!("\"{k}\"");
                if !device.contains(&key) || player.contains(&key) {
                    return Err(format!("{k}: player {player}, device {device}"));
                }
            }
            for k in ["lang", "keymaps", "gameplay", "volumes", "last_role"] {
                let key = format!("\"{k}\"");
                if !player.contains(&key) || device.contains(&key) {
                    return Err(format!("{k}: player {player}, device {device}"));
                }
            }
            // a start reads them
            let args = parse_args(&[format!("--playground={}", pg.display())]);
            if startup_settings(&args) != old.settings || startup_lang(&args) != Lang::Ru {
                return Err("a start reads other settings".into());
            }
            Ok(())
        }),
        Step::Call(
            "the game writes its settings: both files keep theirs",
            |g| {
                let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
                let old = old_profile();
                g.set_settings(old.settings.clone());
                let read = load_profile(&pg);
                if read != old {
                    return Err(format!("read {read:?}, kept {old:?}"));
                }
                Ok(())
            },
        ),
    ]
}

/// What `presets` times, in turn: each preset, and Medium drawn at 75 %
/// through FSR 1 (a Steam Deck's other candidate); or the list in
/// `RENETHACK_BENCH`: entries split by `;`, each a preset's name and the
/// controls changed from it (`medium msaa=off ssao=off scale=75
/// upscale=fsr1`, the profile's words).
fn bench_graphics() -> Vec<(String, Graphics)> {
    if let Ok(list) = std::env::var("RENETHACK_BENCH")
        && !list.trim().is_empty()
    {
        return list
            .split(';')
            .filter(|e| !e.trim().is_empty())
            .map(|e| (e.trim().to_string(), bench_entry(e)))
            .collect();
    }
    let medium = Graphics::preset(Quality::Medium);
    vec![
        ("Low".into(), Graphics::preset(Quality::Low)),
        ("Medium".into(), medium),
        ("High".into(), Graphics::preset(Quality::High)),
        ("Ultra".into(), Graphics::preset(Quality::Ultra)),
        (
            "Medium at 75% through FSR 1".into(),
            Graphics {
                scale: 75,
                upscale: Upscale::Fsr1,
                ..medium
            },
        ),
    ]
}

/// An entry of `RENETHACK_BENCH` (a word it does not know is passed by).
fn bench_entry(entry: &str) -> Graphics {
    fn word<T: serde::de::DeserializeOwned>(w: &str) -> Option<T> {
        serde_json::from_str(&format!("\"{w}\"")).ok()
    }
    let mut words = entry.split_whitespace();
    let mut g = Graphics::preset(words.next().and_then(word).unwrap_or(Quality::High));
    for w in words {
        let Some((k, v)) = w.split_once('=') else {
            continue;
        };
        match k {
            "ssao" => g.ssao = word(v).unwrap_or(g.ssao),
            "ssil" => g.ssil = word(v).unwrap_or(g.ssil),
            "fog" => g.fog = word(v).unwrap_or(g.fog),
            "shadows" => g.shadows = word(v).unwrap_or(g.shadows),
            "msaa" => g.msaa = word(v).unwrap_or(g.msaa),
            "scale" => g.scale = v.parse().unwrap_or(g.scale),
            "upscale" => g.upscale = word(v).unwrap_or(g.upscale),
            _ => {}
        }
    }
    g
}

/// Frames a preset is drawn before it is timed (its pipelines compiled,
/// its buffers made), and frames timed.
const BENCH_WARM: u32 = 120;
const BENCH_TIMED: usize = 360;

/// Where `presets` is: the graphics timed, the frames drawn in them, the
/// times of the frames past the warm-up, the last frame's start.
#[derive(Default)]
struct Bench {
    at: usize,
    frames: u32,
    times: Vec<f64>,
    last: Option<std::time::Instant>,
    /// Godot's count of the frames it drew, when the timing began: a
    /// window the screen does not show is not drawn, and its frames time
    /// nothing.
    drawn_from: Option<i32>,
}

thread_local! {
    static BENCH: std::cell::RefCell<Bench> = std::cell::RefCell::new(Bench::default());
}

/// The next graphics to time, drawn from now.
fn bench_next(g: &mut RenethackGame) -> Result<(), String> {
    let at = BENCH.with(|b| b.borrow().at);
    let (_, graphics) = bench_graphics()
        .into_iter()
        .nth(at)
        .ok_or("no graphics left to time")?;
    let s = nh_world::Settings {
        graphics: Some(graphics),
        ..g.settings.clone()
    };
    g.set_settings(s);
    Ok(())
}

/// A frame of the graphics timed: after the warm-up its time by the wall
/// clock; once enough are timed, the mean, the median, the 95th
/// percentile and the worst in the log.
fn bench_frame(g: &RenethackGame) -> Result<bool, String> {
    let now = std::time::Instant::now();
    let done = BENCH.with(|b| {
        let mut b = b.borrow_mut();
        if let Some(last) = b.last
            && b.frames > BENCH_WARM
        {
            b.times
                .push(now.duration_since(last).as_secs_f64() * 1000.0);
        }
        if b.frames == BENCH_WARM {
            b.drawn_from = Some(Engine::singleton().get_frames_drawn());
        }
        b.last = Some(now);
        b.frames += 1;
        if b.times.len() < BENCH_TIMED {
            return None;
        }
        let mut t = std::mem::take(&mut b.times);
        t.sort_by(f64::total_cmp);
        let n = t.len();
        let mean = t.iter().sum::<f64>() / n as f64;
        let drawn = Engine::singleton().get_frames_drawn() - b.drawn_from.unwrap_or(0);
        let at = b.at;
        *b = Bench {
            at: at + 1,
            ..Bench::default()
        };
        Some((at, mean, t[n / 2], t[n * 95 / 100], t[n - 1], drawn))
    });
    let Some((at, mean, median, p95, worst, drawn)) = done else {
        return Ok(false);
    };
    // the window hidden (another window over it, the screen off): Godot
    // draws nothing, and the frames time only the waits between them
    let hidden = (drawn as f64) < BENCH_TIMED as f64 * 0.9;
    let (w, h) = window(g);
    let name = bench_graphics()
        .into_iter()
        .nth(at)
        .map(|(n, _)| n)
        .unwrap_or_default();
    godot_print!(
        "selftest: presets: {name} at {w}×{h}: mean {mean:.2} ms ({:.0} fps), median {median:.2}, p95 {p95:.2}, worst {worst:.2}{}",
        1000.0 / mean,
        if hidden {
            format!("; only {drawn} frames drawn: the window was hidden, nothing measured")
        } else {
            String::new()
        }
    );
    Ok(true)
}

/// `presets`: the graphics' frame times on a game's first level, the hero
/// standing still, vsync off and no cap: each preset in turn, warmed up,
/// then timed by the wall clock. A measurement, not a test: run it in
/// release, in a window, at the size it is for (`--size=1280x800` for a
/// Steam Deck).
pub(super) fn presets() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Wait("the first level drawn, the camera still", |g| {
            Ok(g.world.map.hero().is_some() && super::camera_settled(g)?)
        }),
        Step::Call("vsync off, no cap", |g| {
            let mut s = g.settings.clone();
            s.display.vsync = false;
            s.display.fps_cap = 0;
            g.set_settings(s);
            Ok(())
        }),
    ]);
    for _ in bench_graphics() {
        steps.push(Step::Call("the next graphics", bench_next));
        steps.push(Step::Wait("its frames timed", bench_frame));
    }
    steps.extend(quit());
    steps
}
