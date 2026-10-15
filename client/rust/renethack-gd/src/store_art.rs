//! Draft store art for Steam (docs/steam.md): the store's capsules, its
//! page background and the library's images at Valve's sizes, rendered
//! from the title's chamber with a hero of a role in the kit NetHack starts
//! them with, the game's name over them in the title's own face. They are
//! drafts the owner may replace. The `steam-art` self-test makes them into
//! `steam/store/` (`make steam-art`): the same shots every run, though
//! the flames, the torches' flicker and the hero's breath move on, and
//! about one pixel in a hundred differs from one run to the next.

use std::path::PathBuf;

use godot::classes::control::MouseFilter;
use godot::classes::image::{Format, Interpolation};
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::viewport::Msaa;
use godot::classes::{
    Camera3D, Control, Environment, Gradient, GradientTexture2D, Image, Label, LabelSettings, Node,
    ProjectSettings, SubViewport, TextureRect, World3D,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;

use crate::map_view::MapView;
use crate::theme::{self, Face};

/// Where the files go (at the repository's root).
const OUT_DIR: &str = "res://../../steam/store";
/// The game's name, as the title shows it.
const NAME: &str = "renethack";
/// Frames a shot is drawn before it is taken: the scene settled, the
/// volumetric fog and the temporal effects come to rest.
const SETTLE_FRAMES: u32 = 40;
/// The scene is drawn this many times larger than the file, then scaled
/// down: smooth edges (not the library's hero, already 3840 wide).
const SUPERSAMPLE: i32 = 2;

/// Where the camera stands: it aims at `aim` from `distance` away, `pitch`
/// degrees above the floor and `yaw` degrees round from the south (the
/// chamber's open side; positive: from the east), seeing `fov` degrees from
/// top to bottom.
#[derive(Clone, Copy)]
struct View {
    aim: (f32, f32, f32),
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov: f32,
}

/// Where the name stands: its middle at (x, y), parts of the picture, its
/// letters `height` of the picture's height, and never wider than `width`
/// of its width.
#[derive(Clone, Copy)]
struct Name {
    at: (f32, f32),
    height: f32,
    width: f32,
}

/// The dark laid over the scene: from the left, from the bottom (and a
/// little from the top), from the top, or over all of it (the page's
/// background, quiet behind the page).
#[derive(Clone, Copy)]
enum Shade {
    None,
    Left,
    Bottom,
    Top,
    All(f32),
}

struct Shot {
    /// The files it is saved as (the library's header is the store's).
    files: &'static [&'static str],
    size: (i32, i32),
    /// The hero in the chamber: a role and a woman's look.
    hero: (&'static str, bool),
    view: View,
    name: Option<Name>,
    shade: Shade,
    supersample: i32,
}

/// The shots, in the title's chamber (title_scene): the hero at (40, 10),
/// the altar between its braziers at (40, 7) under the far wall, the
/// fountain to the west, the fire pit at the hero's east.
const SHOTS: &[Shot] = &[
    Shot {
        files: &["header_capsule.png", "library_header.png"],
        size: (920, 430),
        hero: ("valkyrie", true),
        view: View {
            aim: (39.0, 0.7, 9.6),
            yaw: 4.0,
            pitch: 10.0,
            distance: 4.0,
            fov: 34.0,
        },
        name: Some(Name {
            at: (0.28, 0.5),
            height: 0.17,
            width: 0.48,
        }),
        shade: Shade::Left,
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["small_capsule.png"],
        size: (462, 174),
        hero: ("valkyrie", true),
        view: View {
            aim: (40.0, 1.3, 9.0),
            yaw: 0.0,
            pitch: 16.0,
            distance: 7.5,
            fov: 30.0,
        },
        name: Some(Name {
            at: (0.5, 0.47),
            height: 0.32,
            width: 0.9,
        }),
        shade: Shade::All(0.55),
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["main_capsule.png"],
        size: (1232, 706),
        hero: ("knight", false),
        view: View {
            aim: (39.0, 0.75, 9.6),
            yaw: 4.0,
            pitch: 10.0,
            distance: 4.4,
            fov: 34.0,
        },
        name: Some(Name {
            at: (0.29, 0.48),
            height: 0.14,
            width: 0.48,
        }),
        shade: Shade::Left,
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["vertical_capsule.png"],
        size: (748, 896),
        hero: ("wizard", false),
        view: View {
            aim: (40.0, 1.35, 9.6),
            yaw: 10.0,
            pitch: 12.0,
            distance: 5.6,
            fov: 38.0,
        },
        name: Some(Name {
            at: (0.5, 0.13),
            height: 0.09,
            width: 0.86,
        }),
        shade: Shade::Top,
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["library_capsule.png"],
        size: (600, 900),
        hero: ("samurai", false),
        view: View {
            aim: (40.0, 1.2, 9.7),
            yaw: -8.0,
            pitch: 10.0,
            distance: 5.4,
            fov: 38.0,
        },
        name: Some(Name {
            at: (0.5, 0.87),
            height: 0.08,
            width: 0.86,
        }),
        shade: Shade::Bottom,
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["page_background.png"],
        size: (1438, 810),
        hero: ("valkyrie", true),
        view: View {
            aim: (40.0, 1.4, 8.6),
            yaw: 6.0,
            pitch: 20.0,
            distance: 9.0,
            fov: 36.0,
        },
        name: None,
        shade: Shade::All(0.45),
        supersample: SUPERSAMPLE,
    },
    Shot {
        files: &["library_hero.png"],
        size: (3840, 1240),
        hero: ("valkyrie", true),
        view: View {
            aim: (40.0, 0.9, 9.4),
            yaw: 18.0,
            pitch: 13.0,
            distance: 8.0,
            fov: 24.0,
        },
        name: None,
        shade: Shade::None,
        supersample: 1,
    },
];

/// The library's logo: the name alone on clear, this wide.
const LOGO_FILE: &str = "library_logo.png";
const LOGO_WIDTH: i32 = 1280;

/// The self-test's rounds: one a shot, one for the logo.
pub const ROUNDS: usize = SHOTS.len() + 1;

/// What the bake does next.
enum Phase {
    /// The next shot's hero to stand in the chamber.
    Hero,
    /// The shot set up, being drawn this many frames.
    Settle(u32),
    /// Every shot taken: the logo, drawn this many frames.
    Logo(u32),
    Done,
}

pub struct Bake {
    viewport: Gd<SubViewport>,
    camera: Gd<Camera3D>,
    env: Gd<Environment>,
    overlay: Gd<Control>,
    shade: Gd<TextureRect>,
    name: Gd<Label>,
    next: usize,
    phase: Phase,
    out: PathBuf,
    pub written: Vec<String>,
}

impl Bake {
    /// A stage beside the game: a SubViewport that draws the game's own
    /// world (the map's chamber, its torches and fog) through a camera of
    /// its own, the name and its shade over it.
    pub fn new(root: &mut Gd<Node>, world: Gd<World3D>) -> Result<Bake, String> {
        let out = PathBuf::from(
            ProjectSettings::singleton()
                .globalize_path(OUT_DIR)
                .to_string(),
        );
        std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        // the map's environment, its fog moved to each camera's distance
        let env = world
            .get_environment()
            .ok_or("the world has no environment")?
            .duplicate_resource();
        let mut viewport = SubViewport::new_alloc();
        viewport.set_name("StoreArt");
        viewport.set_world_3d(&world);
        viewport.set_update_mode(UpdateMode::ALWAYS);
        viewport.set_msaa_3d(Msaa::MSAA_4X);
        viewport.set_use_debanding(true);
        viewport.set_positional_shadow_atlas_size(4096);
        root.add_child(&viewport);
        let mut camera = Camera3D::new_alloc();
        camera.set_environment(&env);
        viewport.add_child(&camera);
        camera.set_current(true);

        let mut overlay = Control::new_alloc();
        overlay.set_mouse_filter(MouseFilter::IGNORE);
        viewport.add_child(&overlay);
        let mut shade = TextureRect::new_alloc();
        shade.set_mouse_filter(MouseFilter::IGNORE);
        overlay.add_child(&shade);
        let mut name = Label::new_alloc();
        name.set_text(NAME);
        name.set_horizontal_alignment(HorizontalAlignment::CENTER);
        name.set_vertical_alignment(VerticalAlignment::CENTER);
        overlay.add_child(&name);
        Ok(Bake {
            viewport,
            camera,
            env,
            overlay,
            shade,
            name,
            next: 0,
            phase: Phase::Hero,
            out,
            written: Vec::new(),
        })
    }

    /// The hero the next shot stands in the chamber, None once every shot
    /// is taken.
    pub fn hero(&self) -> Option<(&'static str, bool)> {
        SHOTS.get(self.next).map(|s| s.hero)
    }

    /// One frame of work. Ok(true): the shot (or the logo) at hand is
    /// done; the self-test then stands the next hero in the chamber.
    pub fn tick(&mut self, map: &MapView) -> Result<bool, String> {
        match self.phase {
            Phase::Hero => {
                let Some(shot) = SHOTS.get(self.next) else {
                    self.phase = Phase::Logo(0);
                    self.set_logo();
                    return Ok(false);
                };
                let (role, female) = shot.hero;
                if map.title_hero() != (role, female) || !map.title_drawn() {
                    return Ok(false);
                }
                self.set_up(shot);
                self.phase = Phase::Settle(0);
                Ok(false)
            }
            Phase::Settle(n) if n < SETTLE_FRAMES => {
                self.phase = Phase::Settle(n + 1);
                Ok(false)
            }
            Phase::Settle(_) => {
                let shot = &SHOTS[self.next];
                let mut image = self.take()?;
                image
                    .resize_ex(shot.size.0, shot.size.1)
                    .interpolation(Interpolation::LANCZOS)
                    .done();
                image.convert(Format::RGB8);
                for f in shot.files {
                    self.save(&image, f)?;
                }
                self.next += 1;
                self.phase = Phase::Hero;
                Ok(true)
            }
            Phase::Logo(n) if n < SETTLE_FRAMES => {
                self.phase = Phase::Logo(n + 1);
                Ok(false)
            }
            Phase::Logo(_) => {
                let mut image = self.take()?;
                // the name alone, as tight as it is: Steam places it
                if let Some(used) = alpha_bounds(&image) {
                    image = image.get_region(used).ok_or("no region")?;
                }
                let (w, h) = (image.get_width(), image.get_height());
                let height = (h as f32 * LOGO_WIDTH as f32 / w as f32).round() as i32;
                image
                    .resize_ex(LOGO_WIDTH, height.max(1))
                    .interpolation(Interpolation::LANCZOS)
                    .done();
                self.save(&image, LOGO_FILE)?;
                self.phase = Phase::Done;
                Ok(true)
            }
            Phase::Done => Ok(true),
        }
    }

    pub fn done(&self) -> bool {
        matches!(self.phase, Phase::Done)
    }

    fn set_up(&mut self, shot: &Shot) {
        let (w, h) = (
            shot.size.0 * shot.supersample,
            shot.size.1 * shot.supersample,
        );
        self.viewport.set_size(Vector2i::new(w, h));
        self.viewport.set_transparent_background(false);
        let v = shot.view;
        let (yaw, pitch) = (v.yaw.to_radians(), v.pitch.to_radians());
        let aim = Vector3::new(v.aim.0, v.aim.1, v.aim.2);
        let eye = aim
            + Vector3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            ) * v.distance;
        self.camera.look_at_from_position(eye, aim);
        self.camera.set_fov(v.fov);
        self.camera.set_current(true);
        // the dark closes in past the far wall, as on the title
        self.env.set_fog_depth_begin(v.distance + 4.3);
        self.env.set_fog_depth_end(v.distance + 7.3);
        // the air's glow only as deep as the far wall: past it, over the
        // walls, the long rays would gather it into a grey sky
        self.env.set_volumetric_fog_length(v.distance + 6.0);
        self.lay_out((w, h), shot.shade, shot.name);
    }

    /// The name and its shade over a picture of `size`.
    fn lay_out(&mut self, size: (i32, i32), shade: Shade, name: Option<Name>) {
        let (w, h) = (size.0 as f32, size.1 as f32);
        self.overlay.set_position(Vector2::ZERO);
        self.overlay.set_size(Vector2::new(w, h));
        self.shade.set_position(Vector2::ZERO);
        self.shade.set_size(Vector2::new(w, h));
        self.shade.set_texture(&shade_texture(shade));
        self.shade.set_visible(!matches!(shade, Shade::None));
        let Some(n) = name else {
            self.name.set_visible(false);
            return;
        };
        let font = theme::font(Face::Title);
        // the letters' height at size 1000, then the size the shot asks
        let probe = font.get_string_size_ex(NAME).font_size(1000).done();
        let mut size = (1000.0 * n.height * h / (probe.y * 0.62)).round();
        let wide = font
            .get_string_size_ex(NAME)
            .font_size(size as i32)
            .done()
            .x;
        if wide > n.width * w {
            size = (size * n.width * w / wide).floor();
        }
        let size = size.max(8.0) as i32;
        let mut settings = LabelSettings::new_gd();
        settings.set_font(&font);
        settings.set_font_size(size);
        settings.set_font_color(theme::GOLD_BRIGHT);
        settings.set_outline_size((size / 14).max(1));
        settings.set_outline_color(theme::OUTLINE);
        settings.set_shadow_size((size / 10).max(1));
        settings.set_shadow_color(Color::from_rgba(0.0, 0.0, 0.0, 0.7));
        settings.set_shadow_offset(Vector2::new(0.0, size as f32 * 0.04));
        self.name.set_label_settings(&settings);
        self.name.set_visible(true);
        let box_w = w;
        let box_h = size as f32 * 1.6;
        self.name.set_size(Vector2::new(box_w, box_h));
        self.name.set_position(Vector2::new(
            n.at.0 * w - box_w / 2.0,
            n.at.1 * h - box_h / 2.0,
        ));
    }

    /// The name alone on clear, large: the library's logo.
    fn set_logo(&mut self) {
        let (w, h) = (LOGO_WIDTH * 2, LOGO_WIDTH);
        self.viewport.set_size(Vector2i::new(w, h));
        self.viewport.set_transparent_background(true);
        // no camera: the world is not drawn
        self.camera.set_current(false);
        self.lay_out(
            (w, h),
            Shade::None,
            Some(Name {
                at: (0.5, 0.5),
                height: 0.3,
                width: 0.9,
            }),
        );
    }

    fn take(&self) -> Result<Gd<Image>, String> {
        self.viewport
            .get_texture()
            .and_then(|t| t.get_image())
            .ok_or_else(|| "the store art's viewport drew nothing".to_string())
    }

    /// A picture into its file, a PNG (what Steamworks asks for the
    /// library's images; its store takes them too).
    fn save(&mut self, image: &Gd<Image>, file: &str) -> Result<(), String> {
        let path = self.out.join(file);
        let err = image.save_png(&path.to_string_lossy().to_string());
        if err != godot::global::Error::OK {
            return Err(format!("{}: {err:?}", path.display()));
        }
        self.written.push(file.to_string());
        Ok(())
    }
}

impl Drop for Bake {
    fn drop(&mut self) {
        if self.viewport.is_instance_valid() {
            self.viewport.queue_free();
        }
    }
}

/// The dark over the picture: the title's ink, fading away from its side.
fn shade_texture(shade: Shade) -> Gd<GradientTexture2D> {
    let ink = |a: f32| theme::BG.with_alpha(a);
    let (from, to, stops): (Vector2, Vector2, &[(f32, f32)]) = match shade {
        Shade::None => (Vector2::ZERO, Vector2::RIGHT, &[(0.0, 0.0), (1.0, 0.0)]),
        Shade::Left => (
            Vector2::ZERO,
            Vector2::RIGHT,
            &[(0.0, 0.88), (0.38, 0.62), (0.62, 0.0), (1.0, 0.0)],
        ),
        Shade::Bottom => (
            Vector2::ZERO,
            Vector2::DOWN,
            &[
                (0.0, 0.85),
                (0.18, 0.6),
                (0.28, 0.0),
                (0.45, 0.0),
                (0.7, 0.55),
                (1.0, 0.92),
            ],
        ),
        Shade::Top => (
            Vector2::ZERO,
            Vector2::DOWN,
            &[(0.0, 0.9), (0.24, 0.55), (0.45, 0.0), (1.0, 0.0)],
        ),
        Shade::All(a) => (Vector2::ZERO, Vector2::DOWN, &[(0.0, a), (1.0, a)]),
    };
    let offsets: Vec<f32> = stops.iter().map(|s| s.0).collect();
    let colors: Vec<Color> = stops.iter().map(|s| ink(s.1)).collect();
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(&offsets[..]));
    g.set_colors(&PackedColorArray::from(&colors[..]));
    let mut t = GradientTexture2D::new_gd();
    t.set_gradient(&g);
    t.set_width(256);
    t.set_height(256);
    t.set_fill_from(from);
    t.set_fill_to(to);
    t
}

/// The smallest rectangle holding every pixel not clear.
fn alpha_bounds(image: &Gd<Image>) -> Option<Rect2i> {
    let r = image.get_used_rect();
    (r.size.x > 0 && r.size.y > 0).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_written_once() {
        let mut files: Vec<&str> = SHOTS.iter().flat_map(|s| s.files.iter().copied()).collect();
        files.push(LOGO_FILE);
        let all = files.len();
        files.sort_unstable();
        files.dedup();
        assert_eq!(files.len(), all);
    }

    #[test]
    fn the_name_stays_inside_its_picture() {
        for s in SHOTS {
            let Some(n) = s.name else { continue };
            assert!(n.at.0 - n.width / 2.0 >= 0.0 && n.at.0 + n.width / 2.0 <= 1.0);
            assert!(n.at.1 - n.height >= 0.0 && n.at.1 + n.height <= 1.0);
        }
    }

    #[test]
    fn no_text_on_the_library_hero_or_the_page_background() {
        for s in SHOTS {
            if s.files
                .iter()
                .any(|f| *f == "library_hero.png" || *f == "page_background.png")
            {
                assert!(s.name.is_none(), "{:?}", s.files);
            }
        }
    }
}
