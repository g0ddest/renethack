//! The client's own settings, the same for every character and kept in the
//! profile (`Profile`): how the window covers the screen, how much the 3D
//! view draws, the interface's scale, the screen's effects, a new
//! character's keys, the commands' keys moved, the NetHack options a new
//! game starts with. Plain data and the rules between its parts (a graphics
//! preset and the controls it sets, a step through a setting's values);
//! the client applies them to Godot and to the engine.

use serde::{Deserialize, Serialize};

use crate::{Gameplay, KeyProfile, Keymaps};

/// The client's settings as the profile keeps them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub display: Display,
    /// None: the device's own preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphics: Option<Graphics>,
    /// The interface's scale, percent (`UI_SCALES`); None: by the screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<u32>,
    /// Fewer screen effects: the hero's sway, the camera's drift and a
    /// hallucination's hue off (blindness still dims the view).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduce_effects: bool,
    /// The keys a new character starts with; None: Modern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<KeyProfile>,
    /// The commands' keys the player moved, each profile its own.
    #[serde(default, skip_serializing_if = "Keymaps::is_empty")]
    pub keymaps: Keymaps,
    /// The Gameplay section's NetHack options, for a new game (a saved
    /// one keeps its own).
    #[serde(default, skip_serializing_if = "Gameplay::is_default")]
    pub gameplay: Gameplay,
}

/// One step from `now` through `all` (`dir` -1 back, +1 on); `wrap`: past
/// either end to the other, else it stays at the end. A value not among
/// them steps from the first.
pub fn step_through<T: Copy + PartialEq>(all: &[T], now: T, dir: i32, wrap: bool) -> T {
    let n = all.len() as i32;
    if n == 0 {
        return now;
    }
    let i = all.iter().position(|&v| v == now).unwrap_or(0) as i32 + dir.signum();
    let i = if wrap {
        i.rem_euclid(n)
    } else {
        i.clamp(0, n - 1)
    };
    all[i as usize]
}

/// How the game's window covers the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    #[default]
    Windowed,
    /// As large as the screen, the display left as it is.
    Borderless,
    /// The display given to the game alone.
    Exclusive,
}

impl WindowMode {
    pub const ALL: [WindowMode; 3] = [
        WindowMode::Windowed,
        WindowMode::Borderless,
        WindowMode::Exclusive,
    ];
}

/// The frame-rate caps offered (0: none).
pub const FPS_CAPS: [u32; 5] = [0, 30, 60, 120, 144];

/// The window sizes offered when windowed, smallest first.
pub const WINDOW_SIZES: [(u32, u32); 7] = [
    (1280, 720),
    (1280, 800),
    (1366, 768),
    (1600, 900),
    (1920, 1080),
    (2560, 1440),
    (3840, 2160),
];

/// The window sizes that fit a screen of `screen` (the smallest always).
pub fn window_sizes(screen: (u32, u32)) -> Vec<(u32, u32)> {
    let fit: Vec<_> = WINDOW_SIZES
        .iter()
        .copied()
        .filter(|&(w, h)| w <= screen.0 && h <= screen.1)
        .collect();
    if fit.is_empty() {
        WINDOW_SIZES[..1].to_vec()
    } else {
        fit
    }
}

/// The window size a step from `now` among those that fit `screen`: the
/// next larger or smaller (a size not offered steps to its nearest).
pub fn step_window_size(now: (u32, u32), screen: (u32, u32), dir: i32, wrap: bool) -> (u32, u32) {
    let sizes = window_sizes(screen);
    if sizes.contains(&now) {
        return step_through(&sizes, now, dir, wrap);
    }
    let area = |(w, h): (u32, u32)| u64::from(w) * u64::from(h);
    let next = if dir > 0 {
        sizes.iter().copied().find(|&s| area(s) > area(now))
    } else {
        sizes.iter().rev().copied().find(|&s| area(s) < area(now))
    };
    next.unwrap_or(if dir > 0 {
        sizes[sizes.len() - 1]
    } else {
        sizes[0]
    })
}

/// The window and how often it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Display {
    /// None: the device's own (`Display::window_mode`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<WindowMode>,
    /// The window's size when windowed; None: as the game opens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<(u32, u32)>,
    #[serde(default = "yes")]
    pub vsync: bool,
    /// Frames a second at most; 0: no cap.
    #[serde(default)]
    pub fps_cap: u32,
}

fn yes() -> bool {
    true
}

impl Default for Display {
    fn default() -> Display {
        Display {
            mode: None,
            size: None,
            vsync: true,
            fps_cap: 0,
        }
    }
}

impl Display {
    /// The window's mode: the player's, else the device's. A Steam Deck's
    /// (`deck`) is the whole screen at its own size: gamescope shows the
    /// project's 1600×900 window letterboxed at 1280×720, resampled, and
    /// the Deck draws 40 % more pixels than its screen has.
    pub fn window_mode(&self, deck: bool) -> WindowMode {
        self.mode.unwrap_or(if deck {
            WindowMode::Borderless
        } else {
            WindowMode::Windowed
        })
    }
}

/// A graphics preset; Custom once a control differs from every preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    Medium,
    High,
    Ultra,
    Custom,
}

impl Quality {
    pub const PRESETS: [Quality; 4] =
        [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];
}

/// How much of an effect the 3D view draws: none, or Godot's quality
/// steps for it (ambient occlusion, indirect light, volumetric fog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Off,
    Low,
    Medium,
    High,
}

impl Level {
    pub const ALL: [Level; 4] = [Level::Off, Level::Low, Level::Medium, Level::High];
}

/// Shadows: their atlas's size and how soft their edges are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shadows {
    Low,
    Medium,
    High,
    Ultra,
}

impl Shadows {
    pub const ALL: [Shadows; 4] = [Shadows::Low, Shadows::Medium, Shadows::High, Shadows::Ultra];

    /// The positional shadows' atlas, pixels a side.
    pub fn atlas(self) -> u32 {
        match self {
            Shadows::Low => 1024,
            Shadows::Medium => 2048,
            Shadows::High => 4096,
            Shadows::Ultra => 8192,
        }
    }

    /// The torches nearest the hero that cast shadows (the hero's own
    /// light always does).
    pub fn torches(self) -> usize {
        match self {
            Shadows::Low => 1,
            Shadows::Medium => 2,
            Shadows::High => 3,
            Shadows::Ultra => 4,
        }
    }

    /// Godot's soft shadow filter quality (0 hard ... 5 ultra).
    pub fn filter(self) -> u8 {
        match self {
            Shadows::Low => 0,
            Shadows::Medium => 1,
            Shadows::High => 3,
            Shadows::Ultra => 5,
        }
    }
}

/// Multisample anti-aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Msaa {
    Off,
    X2,
    X4,
    X8,
}

impl Msaa {
    pub const ALL: [Msaa; 4] = [Msaa::Off, Msaa::X2, Msaa::X4, Msaa::X8];
}

/// How the 3D view, drawn at `Graphics::scale`, is brought up to the
/// window's resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Upscale {
    Bilinear,
    /// AMD FidelityFX Super Resolution 1 (spatial).
    Fsr1,
    /// AMD FidelityFX Super Resolution 2 (temporal).
    Fsr2,
}

impl Upscale {
    pub const ALL: [Upscale; 3] = [Upscale::Bilinear, Upscale::Fsr1, Upscale::Fsr2];
}

/// The 3D view's resolutions offered, percent of the window's.
pub const RENDER_SCALES: [u32; 5] = [50, 67, 75, 85, 100];

/// What the 3D view draws, and at what resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Graphics {
    /// Screen-space ambient occlusion: the dark at the foot of walls.
    pub ssao: Level,
    /// Screen-space indirect light: torchlight thrown back off the walls.
    pub ssil: Level,
    /// The haze that lights up around flames.
    pub fog: Level,
    pub shadows: Shadows,
    pub msaa: Msaa,
    /// Percent of the window's resolution (`RENDER_SCALES`).
    pub scale: u32,
    pub upscale: Upscale,
}

impl Graphics {
    /// A preset's controls. High is the look the game was made with.
    pub fn preset(q: Quality) -> Graphics {
        match q {
            Quality::Low => Graphics {
                ssao: Level::Off,
                ssil: Level::Off,
                fog: Level::Off,
                shadows: Shadows::Low,
                msaa: Msaa::Off,
                scale: 75,
                upscale: Upscale::Fsr1,
            },
            // a Steam Deck's: the torches' shadows, the dearest part of a
            // frame there, from the nearest one alone
            Quality::Medium => Graphics {
                ssao: Level::Low,
                ssil: Level::Off,
                fog: Level::Low,
                shadows: Shadows::Low,
                msaa: Msaa::X2,
                scale: 100,
                upscale: Upscale::Bilinear,
            },
            Quality::High | Quality::Custom => Graphics {
                ssao: Level::Medium,
                ssil: Level::Medium,
                fog: Level::Medium,
                shadows: Shadows::High,
                msaa: Msaa::X4,
                scale: 100,
                upscale: Upscale::Bilinear,
            },
            Quality::Ultra => Graphics {
                ssao: Level::High,
                ssil: Level::High,
                fog: Level::High,
                shadows: Shadows::Ultra,
                msaa: Msaa::X8,
                scale: 100,
                upscale: Upscale::Bilinear,
            },
        }
    }

    /// The preset these controls are, else Custom.
    pub fn quality(&self) -> Quality {
        Quality::PRESETS
            .into_iter()
            .find(|&q| Graphics::preset(q) == *self)
            .unwrap_or(Quality::Custom)
    }

    /// How much the controls draw, summed: the presets rise by it.
    fn rank(&self) -> usize {
        let at = |all: &[u32], v: u32| all.iter().position(|&x| x == v).unwrap_or(all.len() - 1);
        self.ssao as usize
            + self.ssil as usize
            + self.fog as usize
            + self.shadows as usize
            + self.msaa as usize
            + at(&RENDER_SCALES, self.scale)
    }

    /// The preset a step of the quality's selector goes to: the next or
    /// the one before; from Custom, the nearest preset on that side (or
    /// as much as it).
    pub fn step_quality(&self, dir: i32, wrap: bool) -> Quality {
        match self.quality() {
            Quality::Custom => {
                let r = self.rank();
                let presets = Quality::PRESETS.map(|q| (q, Graphics::preset(q).rank()));
                let found = if dir > 0 {
                    presets.iter().find(|(_, pr)| *pr >= r)
                } else {
                    presets.iter().rev().find(|(_, pr)| *pr <= r)
                };
                match found {
                    Some(&(q, _)) => q,
                    None if dir > 0 => Quality::Ultra,
                    None => Quality::Low,
                }
            }
            q => step_through(&Quality::PRESETS, q, dir, wrap),
        }
    }
}

impl Default for Graphics {
    fn default() -> Graphics {
        Graphics::preset(Quality::High)
    }
}

/// The UI's scales offered, percent.
pub const UI_SCALES: [u32; 7] = [80, 90, 100, 110, 120, 130, 140];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_is_its_own_quality_and_a_change_is_custom() {
        for q in Quality::PRESETS {
            assert_eq!(Graphics::preset(q).quality(), q, "{q:?}");
        }
        let mut g = Graphics::preset(Quality::High);
        g.ssil = Level::Off;
        assert_eq!(g.quality(), Quality::Custom);
        // Custom starts from High's controls
        assert_eq!(Graphics::preset(Quality::Custom), Graphics::default());
    }

    #[test]
    fn high_is_the_look_the_project_was_made_with() {
        // project.godot: ssao and ssil quality 2 at half size, fog volume
        // 64, positional atlas 4096 with filter quality 3, MSAA 4x
        let g = Graphics::preset(Quality::High);
        assert_eq!(
            (g.ssao, g.ssil, g.fog),
            (Level::Medium, Level::Medium, Level::Medium)
        );
        assert_eq!((g.shadows.atlas(), g.shadows.filter()), (4096, 3));
        // and the map's three torches nearest the hero with shadows
        assert_eq!(g.shadows.torches(), 3);
        assert_eq!((g.msaa, g.scale), (Msaa::X4, 100));
    }

    #[test]
    fn the_presets_rise_from_low_to_ultra() {
        let p = Quality::PRESETS.map(Graphics::preset);
        for w in p.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(a.ssao <= b.ssao && a.ssil <= b.ssil && a.fog <= b.fog);
            assert!(a.shadows <= b.shadows && a.msaa <= b.msaa && a.scale <= b.scale);
        }
    }

    #[test]
    fn the_window_sizes_fit_the_screen() {
        assert_eq!(window_sizes((1280, 800)), vec![(1280, 720), (1280, 800)]);
        assert_eq!(window_sizes((1920, 1080)).last(), Some(&(1920, 1080)));
        assert!(!window_sizes((1920, 1080)).contains(&(2560, 1440)));
        // a screen smaller than every size still has the smallest
        assert_eq!(window_sizes((1024, 600)), vec![(1280, 720)]);
    }

    #[test]
    fn a_step_goes_through_the_values_and_stops_or_wraps_at_the_ends() {
        assert_eq!(step_through(&FPS_CAPS, 0, 1, false), 30);
        assert_eq!(step_through(&FPS_CAPS, 144, 1, false), 144);
        assert_eq!(step_through(&FPS_CAPS, 144, 1, true), 0);
        assert_eq!(step_through(&FPS_CAPS, 0, -1, true), 144);
        // a value from a hand-edited profile steps from the first
        assert_eq!(step_through(&FPS_CAPS, 75, 1, false), 30);
    }

    #[test]
    fn a_window_size_steps_among_those_that_fit() {
        let screen = (1920, 1080);
        assert_eq!(
            step_window_size((1600, 900), screen, 1, false),
            (1920, 1080)
        );
        assert_eq!(
            step_window_size((1920, 1080), screen, 1, false),
            (1920, 1080)
        );
        assert_eq!(
            step_window_size((1280, 720), screen, -1, true),
            (1920, 1080)
        );
        // a window dragged to its own size: the nearest on that side
        assert_eq!(step_window_size((1500, 850), screen, 1, false), (1600, 900));
        assert_eq!(
            step_window_size((1500, 850), screen, -1, false),
            (1366, 768)
        );
        assert_eq!(
            step_window_size((5000, 3000), screen, 1, false),
            (1920, 1080)
        );
    }

    #[test]
    fn the_quality_steps_through_the_presets_and_from_custom_to_the_nearest() {
        let high = Graphics::preset(Quality::High);
        assert_eq!(high.step_quality(1, false), Quality::Ultra);
        assert_eq!(high.step_quality(-1, false), Quality::Medium);
        assert_eq!(
            Graphics::preset(Quality::Ultra).step_quality(1, false),
            Quality::Ultra
        );
        assert_eq!(
            Graphics::preset(Quality::Ultra).step_quality(1, true),
            Quality::Low
        );
        // High less its indirect light: between Medium and High
        let custom = Graphics {
            ssil: Level::Off,
            ..high
        };
        assert_eq!(custom.quality(), Quality::Custom);
        assert_eq!(custom.step_quality(1, false), Quality::High);
        assert_eq!(custom.step_quality(-1, false), Quality::Medium);
        // Ultra's controls with another upscaler: Ultra on either side
        let most = Graphics {
            upscale: Upscale::Fsr2,
            ..Graphics::preset(Quality::Ultra)
        };
        assert_eq!(most.step_quality(1, false), Quality::Ultra);
        assert_eq!(most.step_quality(-1, false), Quality::Ultra);
    }

    #[test]
    fn the_settings_read_from_an_older_profile_and_keep_only_what_was_chosen() {
        let none: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(none, Settings::default());
        let s = Settings {
            graphics: Some(Graphics::preset(Quality::Low)),
            ui_scale: Some(120),
            reduce_effects: true,
            keys: Some(KeyProfile::Classic),
            ..Settings::default()
        };
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&text).unwrap(), s);
        let plain = serde_json::to_string(&Settings::default()).unwrap();
        assert!(
            !plain.contains("graphics")
                && !plain.contains("ui_scale")
                && !plain.contains("reduce_effects")
                && !plain.contains("gameplay"),
            "{plain}"
        );
    }

    #[test]
    fn the_display_s_defaults_leave_the_window_as_it_opens() {
        let d = Display::default();
        assert_eq!((d.mode, d.size, d.vsync, d.fps_cap), (None, None, true, 0));
        let read: Display = serde_json::from_str("{}").unwrap();
        assert_eq!(read, d, "an older profile has none of it");
    }

    #[test]
    fn a_deck_covers_its_screen_until_the_player_chooses() {
        let d = Display::default();
        assert_eq!(d.window_mode(false), WindowMode::Windowed);
        assert_eq!(d.window_mode(true), WindowMode::Borderless);
        let chosen = Display {
            mode: Some(WindowMode::Windowed),
            ..d
        };
        assert_eq!(chosen.window_mode(true), WindowMode::Windowed);
        // a mode not chosen is not kept
        let text = serde_json::to_string(&d).unwrap();
        assert!(!text.contains("mode"), "{text}");
        assert_eq!(
            serde_json::from_str::<Display>(r#"{"mode":"exclusive"}"#)
                .unwrap()
                .mode,
            Some(WindowMode::Exclusive)
        );
    }
}
