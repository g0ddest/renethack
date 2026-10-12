//! The settings page's display and graphics applied to Godot: the window's
//! mode and size, vsync and a frame-rate cap; what the 3D view draws (its
//! effects, its shadows, its anti-aliasing) and at what resolution. And
//! the device's own preset: High, or on a Steam Deck the one that keeps
//! its frame rate there.

use godot::classes::display_server::VSyncMode;
use godot::classes::rendering_server::{
    EnvironmentSsaoQuality, EnvironmentSsilQuality, ShadowQuality,
};
use godot::classes::viewport::{Msaa as ViewportMsaa, Scaling3DMode};
use godot::classes::window::Mode;
use godot::classes::{DisplayServer, Engine, Os, ProjectSettings, RenderingServer, Window};
use godot::prelude::*;
use nh_world::{Display, Graphics, Level, Msaa, Quality, Upscale, WindowMode, window_sizes};

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

/// The preset a device starts with: High, the look the game was made
/// with; on a Steam Deck, the one that holds its frame rate.
pub fn device_quality() -> Quality {
    if is_deck() {
        DECK_QUALITY
    } else {
        Quality::High
    }
}

/// The Steam Deck's preset.
pub const DECK_QUALITY: Quality = Quality::Medium;

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

/// A project setting's number (Godot's default when the project has none).
fn project_number(key: &str, default: f32) -> f32 {
    let v = ProjectSettings::singleton().get_setting(key);
    v.try_to::<f64>()
        .map(|x| x as f32)
        .or_else(|_| v.try_to::<i64>().map(|x| x as f32))
        .unwrap_or(default)
}

/// The renderer's side of `g` (the map's environment turns its effects on
/// and off, its torches cast shadows: `MapView::set_graphics`): the
/// effects' quality, the shadows' atlas and filter, the 3D view's
/// anti-aliasing and resolution.
pub fn apply_graphics(window: &mut Gd<Window>, g: &Graphics) {
    let mut rs = RenderingServer::singleton();
    // the effects' other parameters stay the project's
    let ssao = |key: &str, default: f32| {
        project_number(&format!("rendering/environment/ssao/{key}"), default)
    };
    let (quality, half) = match g.ssao {
        Level::Off | Level::Low => (EnvironmentSsaoQuality::VERY_LOW, true),
        Level::Medium => (EnvironmentSsaoQuality::MEDIUM, true),
        Level::High => (EnvironmentSsaoQuality::HIGH, false),
    };
    rs.environment_set_ssao_quality(
        quality,
        half,
        ssao("adaptive_target", 0.5),
        ssao("blur_passes", 2.0) as i32,
        ssao("fadeout_from", 50.0),
        ssao("fadeout_to", 300.0),
    );
    let ssil = |key: &str, default: f32| {
        project_number(&format!("rendering/environment/ssil/{key}"), default)
    };
    let (quality, half) = match g.ssil {
        Level::Off | Level::Low => (EnvironmentSsilQuality::VERY_LOW, true),
        Level::Medium => (EnvironmentSsilQuality::MEDIUM, true),
        Level::High => (EnvironmentSsilQuality::HIGH, false),
    };
    rs.environment_set_ssil_quality(
        quality,
        half,
        ssil("adaptive_target", 0.5),
        ssil("blur_passes", 4.0) as i32,
        ssil("fadeout_from", 50.0),
        ssil("fadeout_to", 300.0),
    );
    let (size, filter) = fog_volume(g.fog);
    rs.environment_set_volumetric_fog_volume_size(size, size);
    rs.environment_set_volumetric_fog_filter_active(filter);
    let filter = match g.shadows.filter() {
        0 => ShadowQuality::HARD,
        1 => ShadowQuality::SOFT_VERY_LOW,
        2 => ShadowQuality::SOFT_LOW,
        3 => ShadowQuality::SOFT_MEDIUM,
        4 => ShadowQuality::SOFT_HIGH,
        _ => ShadowQuality::SOFT_ULTRA,
    };
    rs.positional_soft_shadow_filter_set_quality(filter);
    rs.directional_soft_shadow_filter_set_quality(filter);
    window.set_positional_shadow_atlas_size(g.shadows.atlas() as i32);
    window.set_msaa_3d(match g.msaa {
        Msaa::Off => ViewportMsaa::DISABLED,
        Msaa::X2 => ViewportMsaa::MSAA_2X,
        Msaa::X4 => ViewportMsaa::MSAA_4X,
        Msaa::X8 => ViewportMsaa::MSAA_8X,
    });
    window.set_scaling_3d_mode(match g.upscale {
        Upscale::Bilinear => Scaling3DMode::BILINEAR,
        Upscale::Fsr1 => Scaling3DMode::FSR,
        Upscale::Fsr2 => Scaling3DMode::FSR2,
    });
    window.set_scaling_3d_scale(g.scale as f32 / 100.0);
}

/// The volumetric fog's froxels a side (and as deep), and whether they
/// are blurred: Godot's default is 64, blurred.
fn fog_volume(level: Level) -> (i32, bool) {
    match level {
        Level::Off | Level::Low => (48, false),
        Level::Medium => (64, true),
        Level::High => (96, true),
    }
}
