//! The settings page's display applied to Godot: the window's mode and
//! size, vsync and a frame-rate cap.

use godot::classes::display_server::VSyncMode;
use godot::classes::window::Mode;
use godot::classes::{DisplayServer, Engine, Os, Window};
use godot::prelude::*;
use nh_world::{Display, WindowMode, window_sizes};

/// A Steam Deck's screen.
pub const DECK_SCREEN: (u32, u32) = (1280, 800);

/// A Steam Deck: Steam says so (`SteamDeck=1`), or the screen is its own.
pub fn is_deck() -> bool {
    std::env::var("SteamDeck").is_ok_and(|v| v == "1")
        || Os::singleton().has_feature("steamdeck")
        || screen_size() == DECK_SCREEN
}

/// The size of the screen the window is on (zero without a display).
pub fn screen_size() -> (u32, u32) {
    let ds = DisplayServer::singleton();
    if ds.get_name() == "headless" {
        return (0, 0);
    }
    let s = ds.screen_get_size();
    (s.x.max(0) as u32, s.y.max(0) as u32)
}

/// The window as `d` asks. At start (`all` false) only what differs from
/// how the window opens: a self-test's size, and Godot's own flags on the
/// command line (`--disable-vsync`, `--max-fps`), stay as they are.
pub fn apply_display(window: &mut Gd<Window>, d: &Display, all: bool) {
    let mode = match d.mode {
        WindowMode::Windowed => Mode::WINDOWED,
        WindowMode::Borderless => Mode::FULLSCREEN,
        WindowMode::Exclusive => Mode::EXCLUSIVE_FULLSCREEN,
    };
    let headless = DisplayServer::singleton().get_name() == "headless";
    if !headless && (all || d.mode != WindowMode::Windowed) && window.get_mode() != mode {
        window.set_mode(mode);
    }
    if !headless
        && d.mode == WindowMode::Windowed
        && let Some(size) = d.size
    {
        // a screen smaller than the size chosen (another monitor): the
        // largest that fits it
        let screen = screen_size();
        let fits = window_sizes(screen);
        let (w, h) = if fits.contains(&size) || screen == (0, 0) {
            size
        } else {
            fits[fits.len() - 1]
        };
        let want = Vector2i::new(w as i32, h as i32);
        if window.get_size() != want {
            window.set_size(want);
            window.move_to_center();
        }
    }
    if all || !d.vsync {
        DisplayServer::singleton().window_set_vsync_mode(if d.vsync {
            VSyncMode::ENABLED
        } else {
            VSyncMode::DISABLED
        });
    }
    if all || d.fps_cap > 0 {
        Engine::singleton().set_max_fps(d.fps_cap as i32);
    }
}
