//! The prologue's scenes (`prologue` tells its beats): each a small level
//! of its own the map draws as it draws the title's chamber (a world of
//! its own, while no game shows), a set built round it of what a level
//! cannot hold (the Sigil's Vault dungeon kit's vaults, pillars and walls,
//! props, the gods' figures, light pouring down through a haze), and a
//! camera of its own moving slowly through it:
//! - the Book: a tome on the god's altar in a candlelit sanctum, books and
//!   scrolls about it, a chandelier over it;
//! - the rebellion: a vaulted temple of pillars, Marduk the god-king in a
//!   shaft of light, dust drifting in it; Moloch twice his height behind,
//!   the furnace in his chest breathing, half lost in a black fog against
//!   smoke the fires behind him light, embers rising; braziers between;
//! - the Amulet: a cavern of Gehennom under a smouldering sky, its lava in
//!   a ring of rock, the Amulet sinking into it, its stone glowing;
//! - the god: their altar in a vaulted shrine, light pouring down on it,
//!   braziers either side;
//! - the hero: the title's chamber, the hero in their kit;
//! - the descent: the hero walking to the stairs down, under a vault.
//!
//! A beat's set is built a few pieces a frame while the beat before is
//! read (`prepare_prologue`), and the rehearsal draws one of each under
//! the start-up veil (`warm_prologue`).
//!
//! The gods and the Amulet are generated models (the owner's, their maps
//! cut to 2K: `art/owner/prologue/`), drawn by `prologue_figure.gdshader`
//! with a life no rig gives them (breath, cloth swaying, a glow pulsing).

use std::time::{Duration, Instant};

use godot::classes::base_material_3d::{Flags, ShadingMode, TextureParam};
use godot::classes::fast_noise_lite::NoiseType;
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::image::Format;
use godot::classes::light_3d::Param;
use godot::classes::particle_process_material::{EmissionShape, Parameter};
use godot::classes::rendering_server::FogVolumeShape;
use godot::classes::{
    CameraAttributesPractical, FastNoiseLite, FogMaterial, FogVolume, GpuParticles3D, Gradient,
    GradientTexture1D, GradientTexture2D, Image, ImageTexture, Material, Mesh, MeshInstance3D,
    Node3D, NoiseTexture3D, OmniLight3D, PackedScene, ParticleProcessMaterial, QuadMesh, Resource,
    Shader, ShaderMaterial, SphereMesh, SpotLight3D, StandardMaterial3D, Texture2D, Texture3D,
};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, LevelNotice, mg};
use nh_world::{Pack, World};

use super::{MapView, flicker, role_monster};
use crate::art::{Finish, no_shadow};
use crate::meshes::{cuboid, plane};
use crate::prologue::Beat;
use crate::rehearsal::glyph;
use crate::title_scene::{Fire, TitleScene};

/// Who the prologue is told for: the hero's role (its name or code), a
/// woman or not, their alignment (-1, 0, 1) and what they carry.
#[derive(Debug, Clone, Default)]
pub struct PrologueHero {
    pub role: String,
    pub female: bool,
    pub align: i32,
    pub inventory: Pack,
}

/// Where a scene stands on the map: the cell everything is laid round.
const MID: (i32, i32) = (40, 9);
/// The light pouring down, and how thick the black fog over Moloch's legs
/// grows.
const HOLY: Color = Color::from_rgb(1.0, 0.93, 0.72);
const FOG_DENSITY: f32 = 2.4;
/// The candles' light over the Book, and how high the Book lies on the
/// altar (its top, the cloth on it).
const CANDLE: Color = Color::from_rgb(1.0, 0.68, 0.36);
const BOOK_ON: f32 = 0.612;
/// Gehennom's heat: its sky at the skyline (and overhead), the glow of the
/// air over its lava.
const EMBERS: Color = Color::from_rgb(0.2, 0.045, 0.015);
const SKY_TOP: Color = Color::from_rgb(0.012, 0.004, 0.003);
/// The Amulet: where it starts above the lava and where it is gone under,
/// how tall it is drawn (its chain to the foot of its disc, bigger than
/// life), the lava's light on it from below, its stone's red light, and a
/// beat of the stone's glow every so many seconds.
const AMULET_FROM: f32 = 3.6;
const AMULET_TO: f32 = -0.7;
const AMULET_TALL: f32 = 1.25;
const LAVA_LIGHT: Color = Color::from_rgb(1.0, 0.42, 0.14);
const STONE_LIGHT: Color = Color::from_rgb(1.0, 0.08, 0.04);
const STONE_BEAT: f32 = 2.4;
/// How high over the lava its own lights hang in the Amulet's cavity, and
/// how bright they are there.
const LAVA_LOW: f32 = 0.12;
const LAVA_DIM: f32 = 0.45;
/// The haze the light pours through while the prologue plays (the air's
/// own: none, the scenes' haze where it shows), and how brightly the light
/// pouring down lights it.
const PROLOGUE_AIR: f32 = 0.0;
const BEAM_HAZE: f32 = 8.0;
/// How thick the column of haze down the light's way is.
const SHAFT_DENSITY: f32 = 0.25;
/// How brightly the braziers on the stage light the haze (the light
/// pouring down shows the brighter).
const FIRE_HAZE: f32 = 0.4;
/// A dark set keeps this share of the level's ambient light.
const DARK_AMBIENT: f32 = 0.0;
/// How far the sky over Gehennom is.
const SKY: f32 = 40.0;
/// The close beats' depth of field (the Book, the Amulet): what lies this
/// far past what the camera looks at begins to blur, sharp to soft over
/// this far, and at most this much.
const FOCUS_BEHIND: f32 = 0.8;
const FOCUS_SOFT: f32 = 2.5;
const FOCUS_BLUR: f32 = 0.05;
/// Time a frame may spend standing the next beat's set pieces.
const PIECES_BUDGET: Duration = Duration::from_millis(3);
/// The sets' pieces: the Sigil's Vault dungeon kit's (CC0) vaults,
/// pillars, walls and arch; Poly Haven's (CC0) candles and boulders;
/// Quaternius's (CC0) books and scrolls.
const VAULT: &str = "res://art/cc0/sigilsvault/dungeon/pieces/vault_ceiling_large_01.glb";
const BARREL: &str = "res://art/cc0/sigilsvault/dungeon/pieces/barrel_ceiling_small_01.glb";
const PILLAR: &str = "res://art/cc0/sigilsvault/dungeon/pieces/pillar_large_01.glb";
const WALL: &str = "res://art/cc0/sigilsvault/dungeon/pieces/wall_large_01.glb";
const ARCH: &str = "res://art/cc0/sigilsvault/dungeon/pieces/door_arched_01_001.glb";
const CANDLES: &str = "res://art/cc0/polyhaven/models/brass_candleholders/brass_candleholders.gltf";
const BOULDER: &str = "res://art/cc0/polyhaven/models/boulder_01/boulder_01.gltf";
const BOOK: &str = "res://art/cc0/quaternius/props/Book_5.gltf";
const SCROLL: &str = "res://art/cc0/quaternius/props/Scroll_1.gltf";
pub(super) const KIT: [&str; 9] = [
    VAULT, BARREL, PILLAR, WALL, ARCH, CANDLES, BOULDER, BOOK, SCROLL,
];
/// The gods of the rebellion: Marduk the god-king, and Moloch twice his
/// height, outlined by the fires behind him, his furnace's light on his
/// chains.
const MARDUK_TALL: f32 = 3.2;
const MOLOCH_TALL: f32 = 6.4;
/// Moloch stands this high on the dark behind Marduk (his feet in the fog).
const MOLOCH_UP: f32 = 0.35;
const MOLOCH_RIM: Color = Color::from_rgb(0.95, 0.3, 0.1);
/// The shaft Marduk stands in, a paler, cooler light than the god's (his
/// gold its warmth against Moloch's red), and the warm light before him,
/// on his face and chest.
const MARDUK_LIGHT: Color = Color::from_rgb(0.92, 0.92, 0.88);
const WITHIN: Color = Color::from_rgb(1.0, 0.84, 0.58);
const FURNACE: Color = Color::from_rgb(1.0, 0.45, 0.16);
/// The smoke the fires behind Moloch light, how it smoulders, and how
/// brightly the embers' glow low in his black fog lights it.
const SMOULDER: Color = Color::from_rgb(0.28, 0.06, 0.02);
const UNDER_HAZE: f32 = 3.0;
/// How brightly the fire behind Moloch lights the smoke about him (more
/// veiled him in red).
const BEHIND_HAZE: f32 = 1.4;
/// The smoke's shader (`prologue_smoke.gdshader`).
const SMOKE_SHADER: &str = "res://shaders/prologue_smoke.gdshader";
/// The figures' looks (`prologue_figure.gdshader`): the generated models,
/// their maps beside them.
const FIGURE_SHADER: &str = "res://shaders/prologue_figure.gdshader";
const MARDUK: Figure = Figure {
    key: "marduk",
    glb: "res://art/owner/prologue/marduk.glb",
    maps: "res://art/owner/prologue/marduk",
    glow: false,
    uniforms: &[
        // ivory, and dull gold on the crown, the bands and the rod; a light
        // from within him
        ("tint", U::V3(0.95, 0.88, 0.74)),
        // (less of a mirror: a gold that mirrors a dark hall is black)
        ("gold_tint", U::V3(1.0, 0.7, 0.3)),
        ("gold", U::F(1.0)),
        ("metal", U::F(0.6)),
        ("inside", U::F(0.5)),
        ("rough", U::F(0.05)),
        ("breath_secs", U::F(MARDUK_BREATH)),
        ("breath_depth", U::F(0.006)),
        ("breath_lift", U::F(0.004)),
        ("chest", U::V2(0.62, 0.8)),
        ("sway", U::F(0.012)),
        ("sway_secs", U::F(7.0)),
        ("hang", U::V2(0.04, 0.6)),
        ("radiance", U::F(0.26)),
        ("rim_color", U::V3(1.0, 0.82, 0.55)),
        ("rim_energy", U::F(1.2)),
        ("rim_power", U::F(3.0)),
    ],
};
const MOLOCH: Figure = Figure {
    key: "moloch",
    glb: "res://art/owner/prologue/moloch.glb",
    maps: "res://art/owner/prologue/moloch",
    glow: true,
    uniforms: &[
        ("breath_secs", U::F(MOLOCH_BREATH)),
        ("breath_depth", U::F(0.009)),
        ("breath_lift", U::F(0.006)),
        ("chest", U::V2(0.55, 0.78)),
        // (his cape, behind him)
        ("sway", U::F(0.03)),
        ("sway_secs", U::F(6.0)),
        ("hang", U::V2(0.15, 0.75)),
        ("behind", U::F(1.0)),
        ("inside", U::F(0.3)),
        ("glow_color", U::V3(1.0, 0.3, 0.06)),
        ("glow_energy", U::F(1.0)),
        ("glow_pulse", U::F(0.5)),
        (
            "heart",
            U::V3(MOLOCH_HEART.0, MOLOCH_HEART.1, MOLOCH_HEART.2),
        ),
        ("heart_radius", U::F(0.17)),
        ("heart_energy", U::F(2.2)),
        ("rim_color", U::V3(0.9, 0.3, 0.1)),
        ("rim_energy", U::F(0.25)),
        ("rim_power", U::F(4.0)),
    ],
};
const AMULET: Figure = Figure {
    key: "amulet",
    glb: "res://art/owner/prologue/amulet.glb",
    maps: "res://art/owner/prologue/amulet",
    glow: true,
    uniforms: &[
        // old gold, darkened; the stone's glow beating
        ("tint", U::V3(0.5, 0.42, 0.3)),
        ("rough", U::F(0.1)),
        // (a thin hot edge: the lava's light from below on its rings)
        ("rim_color", U::V3(1.0, 0.42, 0.12)),
        ("rim_energy", U::F(0.9)),
        ("rim_power", U::F(4.0)),
        ("breath_secs", U::F(STONE_BEAT)),
        // (a dark garnet, its light deep in it)
        ("glow_color", U::V3(0.85, 0.02, 0.03)),
        ("glow_energy", U::F(0.4)),
        ("glow_dark", U::F(0.35)),
        (
            "heart",
            U::V3(AMULET_STONE.0, AMULET_STONE.1, AMULET_STONE.2),
        ),
        ("heart_radius", U::F(0.09)),
        ("heart_energy", U::F(2.0)),
        ("glow_pulse", U::F(0.7)),
    ],
};
/// A breath of each god every so many seconds; Moloch's furnace (in his
/// model's own space), and where the Amulet's stone is in its.
const MARDUK_BREATH: f32 = 5.5;
const MOLOCH_BREATH: f32 = 4.0;
const MOLOCH_HEART: (f32, f32, f32) = (0.10, 0.29, 0.24);
const AMULET_STONE: (f32, f32, f32) = (0.0, -0.38, 0.12);
/// What the prologue's figures are, loaded ahead in the background.
pub(super) const FIGURES: [&str; 3] = [MARDUK.glb, MOLOCH.glb, AMULET.glb];
/// The descent: the cells the hero walks to the stairs on, a step every
/// this many seconds.
const WALK: [(i32, i32); 4] = [(40, 10), (40, 9), (40, 8), (40, 7)];
pub const STAIRS: (i32, i32) = (40, 6);
const STEP_SECS: f64 = 0.62;
/// On the stairs from this far into the descent, the hero goes down them,
/// this deep by its end.
const SINK_AT: f64 = 2.3;
const SINK_DEPTH: f32 = 1.5;

/// A piece of a beat's set: a scene of the kit at `at`, turned `yaw`
/// degrees about the vertical, `scale` times its own size.
#[derive(Debug, Clone, Copy)]
struct Piece {
    scene: &'static str,
    at: Vector3,
    yaw: f32,
    scale: f32,
}

const fn piece(scene: &'static str, (x, y, z): (f32, f32, f32), yaw: f32, scale: f32) -> Piece {
    Piece {
        scene,
        at: Vector3::new(x, y, z),
        yaw,
        scale,
    }
}

/// A uniform of a figure's shader: a number, a pair, a triple.
#[derive(Debug, Clone, Copy)]
enum U {
    F(f32),
    V2(f32, f32),
    V3(f32, f32, f32),
}

impl U {
    fn variant(self) -> Variant {
        match self {
            U::F(x) => x.to_variant(),
            U::V2(x, y) => Vector2::new(x, y).to_variant(),
            U::V3(x, y, z) => Vector3::new(x, y, z).to_variant(),
        }
    }
}

/// A figure of the prologue's: its model, its maps (`<maps>_albedo.jpg`,
/// `_orm`, `_normal`, and `_glow` if it glows), and how its shader draws it.
#[derive(Debug, Clone, Copy)]
struct Figure {
    key: &'static str,
    glb: &'static str,
    maps: &'static str,
    glow: bool,
    uniforms: &'static [(&'static str, U)],
}

/// Smoke's looks (`prologue_smoke.gdshader`): how thick at its thickest,
/// the share of that everywhere, its colour lit and its own glow, how fast
/// it rises (metres a second).
#[derive(Debug, Clone, Copy)]
struct Smoke {
    density: f32,
    least: f32,
    albedo: Color,
    emission: Color,
    climb: f32,
}

/// The Amulet as drawn: its figure, its material, the lava's light on it
/// and its stone's light.
pub(super) struct Amulet {
    /// A pivot at its stone, the figure in it.
    node: Gd<Node3D>,
    mat: Option<Gd<ShaderMaterial>>,
    lava: Gd<OmniLight3D>,
    stone: Gd<OmniLight3D>,
}

/// A beat's scene as drawn: what stands beside its level, and what moves.
pub(super) struct Stage {
    beat: Beat,
    root: Gd<Node3D>,
    candle: Option<Gd<OmniLight3D>>,
    /// The light pouring down, the most of its energy.
    beams: Vec<(Gd<SpotLight3D>, f32)>,
    /// The braziers' lights, each at its brightness and its own phase.
    fires: Vec<(Gd<OmniLight3D>, f32, f64)>,
    /// The gods' materials (their clock and their life coming up).
    gods: Vec<Gd<ShaderMaterial>>,
    /// Moloch's furnace's light: it breathes with him.
    furnace: Option<Gd<OmniLight3D>>,
    /// Smoke rising, and how thick it grows.
    smoke: Vec<(Gd<ShaderMaterial>, f32)>,
    amulet: Option<Amulet>,
    /// The gods still to stand (each its own frame's work ahead: its
    /// model, its material's maps, its smoke and its light), then the set
    /// pieces.
    gods_due: Vec<God>,
    todo: Vec<Piece>,
    /// The descent: the cell the hero stands on now.
    walked: usize,
}

/// A god to stand on a stage: where, turned how far, how tall.
#[derive(Debug, Clone, Copy)]
enum God {
    Marduk(Vector3, f32, f32),
    Moloch(Vector3, f32, f32),
}

impl Stage {
    fn new(beat: Beat, root: Gd<Node3D>) -> Stage {
        Stage {
            beat,
            root,
            candle: None,
            beams: Vec::new(),
            fires: Vec::new(),
            gods: Vec::new(),
            furnace: None,
            smoke: Vec::new(),
            amulet: None,
            gods_due: Vec::new(),
            todo: Vec::new(),
            walked: 0,
        }
    }

    /// Everything of it stood.
    fn built(&self) -> bool {
        self.gods_due.is_empty() && self.todo.is_empty()
    }
}

impl MapView {
    /// Draw `beat`'s scene for `hero` (None: none, and whatever the map
    /// draws without it after): its level, and its set (built ahead by
    /// `prepare_prologue` while the beat before was read, else now).
    pub fn show_prologue(&mut self, beat: Option<Beat>, hero: &PrologueHero, catalog: &Catalog) {
        let began = Instant::now();
        let ready = self.next_stage.take();
        self.end_stage();
        let Some(beat) = beat else {
            if let Some(stage) = ready {
                self.free_stage(stage);
            }
            self.clear();
            self.stage_camera = None;
            self.show_parts();
            return;
        };
        self.clear();
        let cleared = began.elapsed();
        let scene = match beat {
            Beat::Hero => {
                let mut t = TitleScene::new(catalog, &hero.role, hero.female);
                // what they carry (the kit the role starts with till the
                // game's own pack is known)
                if !hero.inventory.items().is_empty() {
                    t.parts().0.inventory = hero.inventory.clone();
                }
                t
            }
            _ => TitleScene::of_world(level(beat, hero, catalog), catalog),
        };
        self.title = Some(scene);
        self.titled = true;
        let levelled = began.elapsed();
        if beat == Beat::Hero {
            self.light_title_fires();
        }
        self.show_parts();
        // black past what is lit
        self.env.set_bg_color(Color::BLACK);
        self.env.set_fog_light_color(Color::BLACK);
        let built_ahead = ready.as_ref().is_some_and(|s| s.beat == beat && s.built());
        let mut stage = match ready {
            Some(stage) if stage.beat == beat => stage,
            other => {
                if let Some(stage) = other {
                    self.free_stage(stage);
                }
                self.build_stage(beat)
            }
        };
        self.build_pieces(&mut stage, None);
        stage.root.set_visible(true);
        self.stage = Some(stage);
        self.stage_camera = None;
        self.keep_prologue_air();
        if self.stats_window.is_some() {
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            godot_print!(
                "map: the prologue's {beat:?} laid in {:.1} ms (the last one cleared {:.1}, the level {:.1}, the stage {:.1}{})",
                ms(began.elapsed()),
                ms(cleared),
                ms(levelled - cleared),
                ms(began.elapsed() - levelled),
                if built_ahead { ", built ahead" } else { "" }
            );
        }
    }

    /// Build `beat`'s set ahead, hidden till its beat, a few pieces a frame
    /// (`PIECES_BUDGET`): the frames of the beat before have room for it,
    /// and the change under the black has the less to do.
    pub fn prepare_prologue(&mut self, beat: Beat, _catalog: &Catalog) {
        if self
            .next_stage
            .as_ref()
            .is_some_and(|s| s.beat == beat && s.built())
        {
            return;
        }
        let began = Instant::now();
        let mut stage = match self.next_stage.take() {
            Some(stage) if stage.beat == beat => stage,
            other => {
                if let Some(stage) = other {
                    self.free_stage(stage);
                }
                let mut stage = self.build_stage(beat);
                stage.root.set_visible(false);
                stage
            }
        };
        self.build_pieces(&mut stage, Some(PIECES_BUDGET));
        if self.stats_window.is_some() {
            godot_print!(
                "map: the prologue's {beat:?} built ahead for {:.1} ms, {} pieces to go",
                began.elapsed().as_secs_f64() * 1000.0,
                stage.gods_due.len() + stage.todo.len()
            );
        }
        self.next_stage = Some(stage);
    }

    /// `beat`'s set but its pieces (`build_pieces` stands them): its lights,
    /// the haze the light pours through, the braziers, the gods, the
    /// Amulet, the sky.
    fn build_stage(&mut self, beat: Beat) -> Stage {
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueStage");
        self.root.add_child(&root);
        let mut stage = Stage::new(beat, root);
        stage.todo = set(beat);
        // the pieces stand in the order listed
        stage.todo.reverse();
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        let v = Vector3::new;
        match beat {
            Beat::Book => {
                // the Book open on the altar, the altar's candles over it,
                // more burning either side
                let mut book = self.book_model();
                book.set_position(mid + v(0.0, BOOK_ON, 0.1));
                book.set_rotation_degrees(v(0.0, 3.0, 0.0));
                stage.root.add_child(&book);
                stage.candle = Some(light(
                    &mut stage.root,
                    CANDLE,
                    mid + v(0.0, 1.15, -0.1),
                    3.2,
                ));
                for (k, x) in [38.7f32, 41.3].into_iter().enumerate() {
                    let l = light(&mut stage.root, CANDLE, v(x, 1.0, 8.5), 2.6);
                    stage.fires.push((l, 0.9, 1.3 + k as f64 * 2.9));
                }
                haze(
                    &mut stage.root,
                    mid + v(0.0, 1.5, -0.6),
                    v(6.0, 3.0, 4.0),
                    0.035,
                    None,
                );
            }
            Beat::Rebellion => {
                // Marduk in the light pouring down on him; Moloch behind,
                // twice his height, in his black fog and his smoke
                // (stood in this order)
                stage.gods_due = vec![
                    God::Moloch(v(42.6, MOLOCH_UP, 6.1), -30.0, MOLOCH_TALL),
                    God::Marduk(v(37.6, 0.0, 10.2), 50.0, MARDUK_TALL),
                ];
                haze(
                    &mut stage.root,
                    v(40.0, 3.5, 9.5),
                    v(14.0, 7.0, 10.0),
                    0.03,
                    None,
                );
                // (one between the gods, before the altar)
                for (k, (x, z)) in [(37.6, 6.4), (40.4, 8.0)].into_iter().enumerate() {
                    self.stage_brazier(&mut stage, v(x, 0.0, z), k as f64 * 2.1);
                }
            }
            Beat::Amulet => {
                // a cavity of Gehennom: its lava's light low over it, smoke
                // drifting in the dark above, embers rising
                stage.amulet = Some(self.stage_amulet(&mut stage.root, mid, AMULET_TALL));
                self.dome(&mut stage.root, mid);
                let heat = Color::from_rgb(0.3, 0.07, 0.015);
                haze(
                    &mut stage.root,
                    mid + v(0.0, 0.5, 0.0),
                    v(13.0, 1.2, 8.0),
                    0.02,
                    Some(heat),
                );
                let above = Smoke {
                    density: 0.35,
                    least: 0.0,
                    albedo: Color::from_rgb(0.12, 0.1, 0.09),
                    emission: Color::from_rgb(0.03, 0.008, 0.002),
                    climb: 0.25,
                };
                let turn = Basis::IDENTITY;
                let size = v(16.0, 5.0, 9.0);
                if let Some(m) =
                    self.smoke(&mut stage.root, mid + v(0.0, 4.0, -2.0), turn, size, above)
                {
                    stage.smoke.push((m, above.density));
                }
                let mut embers = self.rising_embers(v(3.5, 0.2, 2.5), 60, 0.04);
                embers.set_position(mid + v(0.0, 0.2, 0.6));
                stage.root.add_child(&embers);
            }
            Beat::God => {
                let altar = v(MID.0 as f32, 0.0, MID.1 as f32 - 2.0);
                let from = altar + v(0.0, 6.0, 1.2);
                stage
                    .beams
                    .push((beam(&mut stage.root, altar, from, HOLY, 7.0), 7.0));
                let mut l = light(&mut stage.root, HOLY, altar + v(0.0, 2.2, 0.6), 5.0);
                l.set_param(Param::ENERGY, 0.0);
                l.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
                stage.candle = Some(l);
                haze(
                    &mut stage.root,
                    mid + v(0.0, 1.5, -0.4),
                    v(6.0, 3.2, 5.0),
                    0.045,
                    None,
                );
                let mut high = light(&mut stage.root, CANDLE, mid + v(0.0, 4.6, -0.5), 6.0);
                high.set_param(Param::ENERGY, 0.35);
                // dust drifting in the light, lit as it passes through it
                let mut dust = self.motes(
                    v(0.9, 2.2, 0.9),
                    80,
                    0.04,
                    Color::from_rgba(0.95, 0.9, 0.78, 0.55),
                );
                dust.set_position(altar + v(0.0, 2.4, 0.6));
                stage.root.add_child(&dust);
                for (k, x) in [38.4, 41.6].into_iter().enumerate() {
                    self.stage_brazier(&mut stage, v(x, 0.0, 8.4), k as f64 * 1.7);
                }
            }
            Beat::Descent => {
                // fires either side of the way down
                for (k, x) in [38.8, 41.2].into_iter().enumerate() {
                    self.stage_brazier(&mut stage, v(x, 0.0, 8.6), k as f64 * 1.9);
                }
            }
            Beat::Hero => {}
        }
        stage
    }

    /// Stand the stage's gods and pieces still to stand, for at most
    /// `budget` (all of them: None).
    fn build_pieces(&mut self, stage: &mut Stage, budget: Option<Duration>) {
        let began = Instant::now();
        while let Some(god) = stage.gods_due.pop() {
            match god {
                God::Marduk(at, yaw, tall) => self.stand_marduk(stage, at, yaw, tall),
                God::Moloch(at, yaw, tall) => self.stand_moloch(stage, at, yaw, tall),
            }
            if budget.is_some_and(|b| began.elapsed() >= b) {
                return;
            }
        }
        while let Some(p) = stage.todo.pop() {
            if let Some(scene) = self.kit(p.scene) {
                stand_piece(&mut stage.root, &scene, p);
            }
            if budget.is_some_and(|b| began.elapsed() >= b) {
                break;
            }
        }
    }

    /// A set piece's scene, loaded once and kept.
    fn kit(&mut self, path: &'static str) -> Option<Gd<PackedScene>> {
        self.kept(path, || godot::tools::try_load::<PackedScene>(path).ok())
    }

    /// A figure of the prologue's stood on the stage: `tall` high, its
    /// soles at `at`, turned `yaw` degrees, drawn in its own material (made
    /// once and kept: its maps and its shader's looks). The figure, and its
    /// scale.
    fn stand_figure(
        &mut self,
        root: &mut Gd<Node3D>,
        figure: &Figure,
        at: Vector3,
        yaw: f32,
        tall: f32,
    ) -> Option<(Gd<Node3D>, Gd<ShaderMaterial>, f32)> {
        let scene = self.kit(figure.glb)?;
        let mut node = scene.instantiate()?.try_cast::<Node3D>().ok()?;
        let mat = self.figure_material(figure)?;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for n in node
            .find_children_ex("*")
            .type_("MeshInstance3D")
            .owned(false)
            .done()
            .iter_shared()
        {
            if let Ok(mut mi) = n.try_cast::<MeshInstance3D>() {
                let b = mi.get_aabb();
                lo = lo.min(b.position.y);
                hi = hi.max(b.position.y + b.size.y);
                mi.set_material_override(&mat);
                no_shadow(&mut mi);
            }
        }
        if hi <= lo {
            node.queue_free();
            return None;
        }
        let k = tall / (hi - lo);
        node.set_scale(Vector3::new(k, k, k));
        node.set_rotation_degrees(Vector3::new(0.0, yaw, 0.0));
        node.set_position(at - Vector3::new(0.0, lo * k, 0.0));
        root.add_child(&node);
        Some((node, mat, k))
    }

    /// A figure's material: its shader, its maps, its looks.
    fn figure_material(&mut self, figure: &Figure) -> Option<Gd<ShaderMaterial>> {
        let shader = self.kept("figure shader", || {
            godot::tools::try_load::<Shader>(FIGURE_SHADER).ok()
        })?;
        let f = *figure;
        self.kept(f.key, move || {
            let mut m = ShaderMaterial::new_gd();
            m.set_shader(&shader);
            let maps = [
                ("albedo_map", "albedo"),
                ("orm_map", "orm"),
                ("normal_map", "normal"),
                ("glow_map", "glow"),
            ];
            for (uniform, map) in maps {
                if map == "glow" && !f.glow {
                    continue;
                }
                let tex =
                    godot::tools::try_load::<Texture2D>(&format!("{}_{map}.jpg", f.maps)).ok()?;
                m.set_shader_parameter(uniform, &tex.to_variant());
            }
            for &(name, u) in f.uniforms {
                m.set_shader_parameter(name, &u.variant());
            }
            Some(m)
        })
    }

    /// Marduk the Creator at `at` (`tall` high, turned `yaw`): in the light
    /// pouring down on him, dust drifting in it, a warm light before him.
    fn stand_marduk(&mut self, stage: &mut Stage, at: Vector3, yaw: f32, tall: f32) {
        let v = Vector3::new;
        let k = tall / MARDUK_TALL;
        if let Some((_, mat, _)) = self.stand_figure(&mut stage.root, &MARDUK, at, yaw, tall) {
            stage.gods.push(mat);
        }
        // the shaft down on his crown and shoulders from behind him
        let down = at + v(0.0, 1.4, 0.0) * k;
        let from = at + v(-1.2, 8.5, -1.9) * k;
        stage
            .beams
            .push((beam(&mut stage.root, down, from, MARDUK_LIGHT, 11.0), 4.0));
        let mut l = light(&mut stage.root, WITHIN, at + v(0.6, 2.9, 1.4) * k, 4.0 * k);
        l.set_param(Param::ENERGY, 0.0);
        l.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        stage.candle = Some(l);
        // dust in the light, lit as it drifts through it
        let mut dust = self.motes(
            v(1.1, 2.0, 1.1) * k,
            90,
            0.045 * k.sqrt(),
            Color::from_rgba(0.95, 0.9, 0.78, 0.55),
        );
        dust.set_position(at + v(0.0, 2.2, 0.0) * k);
        stage.root.add_child(&dust);
    }

    /// Moloch at `at` (`tall` high, turned `yaw`): the furnace in his chest
    /// lighting his chains, the fires behind him outlining him against the
    /// smoke they light, a black fog over his legs glowing low, embers
    /// rising out of it.
    fn stand_moloch(&mut self, stage: &mut Stage, at: Vector3, yaw: f32, tall: f32) {
        let v = Vector3::new;
        let k = tall / MOLOCH_TALL;
        let turn = Basis::from_euler(EulerOrder::YXZ, v(0.0, yaw.to_radians(), 0.0));
        if let Some((node, mat, _)) = self.stand_figure(&mut stage.root, &MOLOCH, at, yaw, tall) {
            stage.gods.push(mat);
            // the furnace's light a little before it
            let (hx, hy, hz) = MOLOCH_HEART;
            let heart = node.get_transform() * v(hx, hy, hz + 0.25);
            let mut f = light(&mut stage.root, FURNACE, heart, 3.2 * k);
            f.set_param(Param::ENERGY, 0.0);
            f.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.2);
            stage.furnace = Some(f);
        }
        // a fire before him, low and to his left, his body shaped by it
        // from below (the set is dark: nothing else lights it)
        // (no glow in the air between him and the eye: it veiled him red)
        let mut before = light(
            &mut stage.root,
            FURNACE,
            at + turn * v(1.6, 1.2, 2.6) * k,
            7.0 * k,
        );
        before.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
        stage.fires.push((before, 1.8, 3.1));
        // the fires behind him: low, behind his legs, a red-brown glow on
        // the wall and in the smoke he rises from; and high, a dim edge on
        // his horns and shoulders
        let low = light(
            &mut stage.root,
            MOLOCH_RIM,
            at + turn * v(0.0, 0.9, -1.9) * k,
            6.5 * k,
        );
        let mut low = low;
        low.set_param(Param::VOLUMETRIC_FOG_ENERGY, BEHIND_HAZE);
        stage.fires.push((low, 2.2, 0.7));
        for (x, phase) in [(-1.6, 2.3), (1.6, 4.1)] {
            let mut high = light(
                &mut stage.root,
                MOLOCH_RIM,
                at + turn * v(x, 6.2, -1.6) * k,
                3.2 * k,
            );
            high.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.3);
            stage.fires.push((high, 0.9, phase));
        }
        // the smoke about and behind him they light, smouldering, rising
        let back = Smoke {
            density: 0.22,
            least: 0.15,
            albedo: Color::from_rgb(0.3, 0.26, 0.24),
            emission: SMOULDER,
            climb: 0.35,
        };
        let at_back = at + turn * v(0.0, 3.8, -1.9) * k;
        let size = v(11.0, 9.0, 3.6) * k;
        if let Some(m) = self.smoke(&mut stage.root, at_back, turn, size, back) {
            stage.smoke.push((m, back.density));
        }
        // the black fog to his knees, rising slowly, the embers' glow low
        // in it; wisps of it rising past his waist
        let dark_at = at + turn * v(0.0, 1.0, 0.9) * k;
        let black = Smoke {
            density: FOG_DENSITY,
            least: 0.45,
            albedo: Color::from_rgb(0.09, 0.07, 0.065),
            emission: Color::BLACK,
            climb: 0.15,
        };
        let size = v(8.0, 3.4, 5.5) * k;
        if let Some(m) = self.smoke(&mut stage.root, dark_at, turn, size, black) {
            stage.smoke.push((m, black.density));
        }
        let wisps = Smoke {
            density: 0.4,
            least: 0.0,
            climb: 0.3,
            ..black
        };
        let size = v(6.0, 4.0, 2.6) * k;
        if let Some(m) = self.smoke(
            &mut stage.root,
            at + turn * v(0.0, 3.4, -0.2) * k,
            turn,
            size,
            wisps,
        ) {
            stage.smoke.push((m, wisps.density));
        }
        let mut under = light(
            &mut stage.root,
            FURNACE,
            at + turn * v(0.0, 0.4, 1.2) * k,
            4.5 * k,
        );
        under.set_param(Param::VOLUMETRIC_FOG_ENERGY, UNDER_HAZE);
        stage.fires.push((under, 1.2, 1.7));
        // embers rising out of it
        let mut embers = self.rising_embers(v(2.6, 0.6, 1.8) * k, 90, 0.09 * k.sqrt());
        embers.set_position(at + turn * v(0.0, 0.8, 0.7) * k);
        stage.root.add_child(&embers);
    }

    /// Smoke `size` round `at` turned `turn` (an ellipsoid), its looks
    /// `look`: its material (the beat's clock and its thickness set as it
    /// plays; thin till then).
    fn smoke(
        &mut self,
        root: &mut Gd<Node3D>,
        at: Vector3,
        turn: Basis,
        size: Vector3,
        look: Smoke,
    ) -> Option<Gd<ShaderMaterial>> {
        let shader = self.kept("smoke shader", || {
            godot::tools::try_load::<Shader>(SMOKE_SHADER).ok()
        })?;
        let noise = self.smoke_noise()?;
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        mat.set_shader_parameter("noise", &noise.to_variant());
        mat.set_shader_parameter("density", &0.0f32.to_variant());
        mat.set_shader_parameter("least", &look.least.to_variant());
        let rgb = |c: Color| Vector3::new(c.r, c.g, c.b).to_variant();
        mat.set_shader_parameter("albedo", &rgb(look.albedo));
        mat.set_shader_parameter("emission", &rgb(look.emission));
        mat.set_shader_parameter("climb", &look.climb.to_variant());
        let mut fog = FogVolume::new_alloc();
        fog.set_shape(FogVolumeShape::ELLIPSOID);
        fog.set_size(size);
        fog.set_transform(Transform3D::new(turn, at));
        fog.set_material(&mat);
        root.add_child(&fog);
        Some(mat)
    }

    /// Motes drifting slowly in a box `extents` round where they are put
    /// (the dungeon's dust's looks, `amount` of them `size` across,
    /// `color`).
    fn motes(
        &mut self,
        extents: Vector3,
        amount: i32,
        size: f32,
        color: Color,
    ) -> Gd<GpuParticles3D> {
        let mut m = ParticleProcessMaterial::new_gd();
        m.set_emission_shape(EmissionShape::BOX);
        m.set_emission_box_extents(extents);
        m.set_direction(Vector3::UP);
        m.set_spread(180.0);
        m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.02);
        m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.08);
        m.set_gravity(Vector3::new(0.0, 0.01, 0.0));
        m.set_param_min(Parameter::SCALE, 0.6);
        m.set_param_max(Parameter::SCALE, 1.3);
        m.set_turbulence_enabled(true);
        m.set_turbulence_noise_strength(0.35);
        m.set_turbulence_noise_scale(4.0);
        m.set_color_ramp(&fading(color));
        let mut p = GpuParticles3D::new_alloc();
        p.set_process_material(&m);
        p.set_amount(amount);
        p.set_lifetime(7.0);
        p.set_pre_process_time(7.0);
        p.set_draw_pass_mesh(0, &quad_mesh(size));
        p.set_material_override(&crate::vfx::particle_material(
            &crate::vfx::soft_texture(),
            false,
            true,
            None,
        ));
        p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        p.set_visibility_aabb(Aabb::new(-extents * 2.0, extents * 4.0));
        p
    }

    /// Embers flung up and drifting from a box `extents` round where they
    /// are put, `amount` of them `size` across (a torch's embers' looks).
    fn rising_embers(&mut self, extents: Vector3, amount: i32, size: f32) -> Gd<GpuParticles3D> {
        let mut m = ParticleProcessMaterial::new_gd();
        m.set_emission_shape(EmissionShape::BOX);
        m.set_emission_box_extents(extents);
        m.set_direction(Vector3::UP);
        m.set_spread(20.0);
        m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.5);
        m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 1.1);
        m.set_gravity(Vector3::new(0.0, 0.06, 0.0));
        m.set_param_min(Parameter::SCALE, 0.6);
        m.set_param_max(Parameter::SCALE, 1.4);
        m.set_turbulence_enabled(true);
        m.set_turbulence_noise_strength(0.5);
        m.set_turbulence_noise_scale(2.0);
        m.set_color_ramp(&crate::vfx::ramp(Color::from_rgb(1.0, 0.42, 0.12), 4.0));
        let mut p = GpuParticles3D::new_alloc();
        p.set_process_material(&m);
        p.set_amount(amount);
        p.set_lifetime(4.5);
        p.set_pre_process_time(4.5);
        p.set_randomness_ratio(0.5);
        p.set_draw_pass_mesh(0, &quad_mesh(size));
        p.set_material_override(&crate::vfx::particle_material(
            &crate::vfx::soft_texture(),
            true,
            false,
            None,
        ));
        p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        p.set_visibility_aabb(Aabb::new(
            -extents - Vector3::new(1.0, 1.0, 1.0),
            extents * 2.0 + Vector3::new(2.0, 6.0, 2.0),
        ));
        p
    }

    /// A noise for smoke's thickness, made once and kept.
    fn smoke_noise(&mut self) -> Option<Gd<Texture3D>> {
        self.kept("smoke noise", || {
            let mut noise = FastNoiseLite::new_gd();
            noise.set_noise_type(NoiseType::SIMPLEX_SMOOTH);
            noise.set_frequency(0.06);
            let mut t = NoiseTexture3D::new_gd();
            t.set_width(48);
            t.set_height(48);
            t.set_depth(48);
            t.set_seamless(true);
            t.set_noise(&noise);
            Some(t.upcast::<Texture3D>())
        })
    }

    /// A brazier of the title's chamber burning at `at` on the stage.
    fn stage_brazier(&mut self, stage: &mut Stage, at: Vector3, phase: f64) {
        let (mut node, mut light, _, _) = self.title_fire(Fire::Brazier, at, phase);
        node.reparent(&stage.root);
        light.reparent(&stage.root);
        light.set_param(Param::VOLUMETRIC_FOG_ENERGY, FIRE_HAZE);
        let energy = light.get_param(Param::ENERGY);
        stage.fires.push((light, energy, phase));
    }

    /// The Amulet of Yendor high over `at`, `tall` from its chain to its
    /// foot, turning about its stone; the lava's light on it from below and
    /// its stone's red light.
    fn stage_amulet(&mut self, root: &mut Gd<Node3D>, at: Vector3, tall: f32) -> Amulet {
        let high = at + Vector3::new(0.0, AMULET_FROM, 0.0);
        let mut node = Node3D::new_alloc();
        node.set_name("Amulet");
        node.set_position(high);
        root.add_child(&node);
        let mut mat = None;
        if let Some((mut figure, m, k)) = self.stand_figure(root, &AMULET, high, 0.0, tall) {
            // (the figure in a pivot at its stone)
            let (sx, sy, _) = AMULET_STONE;
            figure.reparent(&node);
            figure.set_position(-Vector3::new(sx, sy, 0.0) * k);
            mat = Some(m);
        }
        let front = AMULET_STONE.2 * tall / 1.9 + 0.12;
        let mut stone = light(
            &mut node,
            STONE_LIGHT,
            Vector3::new(0.0, 0.0, front),
            0.9 * tall,
        );
        stone.set_param(Param::ENERGY, 0.0);
        // (it lights the claws and the gold round the stone, not the stone)
        stone.set_param(Param::SPECULAR, 0.0);
        let mut lava = light(root, LAVA_LIGHT, high, 3.0);
        lava.set_param(Param::ENERGY, 0.0);
        Amulet {
            node,
            mat,
            lava,
            stone,
        }
    }

    /// The scene `secs` into its beat (`descent`: seconds into the last
    /// beat's walk down, once it begins): its camera, its lights, what
    /// moves in it.
    pub fn prologue_frame(&mut self, secs: f64, descent: Option<f64>) {
        let Some(beat) = self.stage.as_ref().map(|s| s.beat) else {
            return;
        };
        let t = secs as f32;
        let rise = smooth((t / 2.5).min(1.0));
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        // the hero walks on, a cell a step
        if let Some(d) = descent {
            let due = ((d / STEP_SECS) as usize + 1).min(WALK.len());
            let walked = self.stage.as_ref().map_or(0, |s| s.walked);
            if due > walked {
                self.walk_stage_hero(walked, due);
                if let Some(s) = self.stage.as_mut() {
                    s.walked = due;
                }
            }
        }
        // down the stairs, out of sight
        if let Some(d) = descent.filter(|&d| d > SINK_AT) {
            let k = smooth(((d - SINK_AT) / 1.3) as f32);
            if let Some(mut node) = self.hero_model().map(|m| m.node.clone()) {
                let p = node.get_position();
                node.set_position(Vector3::new(p.x, -SINK_DEPTH * k * k, p.z));
            }
        }
        // the lava's own lights low over it in the Amulet's cavity: its rocks
        // lit at their feet, dark above (the game's are higher; its next
        // level lays them again)
        if beat == Beat::Amulet {
            for l in &mut self.lava_lights {
                let p = l.get_position();
                if p.y > LAVA_LOW {
                    l.set_position(Vector3::new(p.x, LAVA_LOW, p.z));
                    l.set_param(Param::ENERGY, super::LAVA_ENERGY * LAVA_DIM);
                }
            }
            // (and its cells' lamps over its banks, which lit its rocks
            // all one orange)
            for n in self.cells.values_mut() {
                if let Some(l) = n.lamp.as_mut().filter(|l| l.kind == super::Lamp::Lava) {
                    l.light
                        .set_param(Param::ENERGY, super::LAVA_LAMP_ENERGY * LAVA_DIM);
                }
            }
        }
        let hero = self.hero_stands().map(|(p, _)| p);
        let Some(stage) = self.stage.as_mut() else {
            return;
        };
        if let Some(c) = stage.candle.as_mut() {
            let energy = match beat {
                Beat::Book => 1.7 * flicker(secs * 1.3),
                Beat::Rebellion => 2.2 * rise,
                _ => 2.6 * rise,
            };
            c.set_param(Param::ENERGY, energy);
        }
        for (b, most) in &mut stage.beams {
            b.set_param(Param::ENERGY, rise * *most);
        }
        for (l, energy, phase) in &mut stage.fires {
            l.set_param(Param::ENERGY, *energy * flicker(secs * 1.1 + *phase));
        }
        // the gods breathe, their cloth and the smoke about them stir
        for m in &mut stage.gods {
            m.set_shader_parameter("clock", &t.to_variant());
            m.set_shader_parameter("rise", &rise.to_variant());
        }
        if let Some(f) = stage.furnace.as_mut() {
            f.set_param(
                Param::ENERGY,
                2.8 * rise * (0.35 + 0.65 * inhale(t, MOLOCH_BREATH)),
            );
        }
        for (m, density) in &mut stage.smoke {
            m.set_shader_parameter("clock", &t.to_variant());
            m.set_shader_parameter("density", &(*density * rise).to_variant());
        }
        let mut amulet_at = None;
        if let Some(a) = stage.amulet.as_mut() {
            // it sinks slowly, swaying on its chain (its back is plain: it
            // never turns it to us), and is gone under the lava; the lava's
            // light on it the stronger the nearer, its stone beating
            let k = smooth((t / 13.0).min(1.0));
            let y = AMULET_FROM + (AMULET_TO - AMULET_FROM) * k;
            let at = mid + Vector3::new(0.0, y, 0.0);
            a.node.set_position(at);
            a.node.set_rotation(Vector3::new(
                0.08 * (t * 0.7).sin(),
                0.45 * (t * 0.45).sin(),
                0.05 * (t * 0.9 + 1.0).sin(),
            ));
            let gone = smooth(((0.2 - y) / 0.9).clamp(0.0, 1.0));
            let near = smooth(((AMULET_FROM - y) / (AMULET_FROM - 0.3)).clamp(0.0, 1.0));
            a.lava.set_position(Vector3::new(at.x, 0.35, at.z + 0.5));
            a.lava
                .set_param(Param::ENERGY, (0.5 + 1.6 * near) * (1.0 - gone));
            a.stone.set_param(
                Param::ENERGY,
                0.9 * rise * (0.3 + 0.7 * inhale(t, STONE_BEAT)) * (1.0 - gone),
            );
            if let Some(m) = a.mat.as_mut() {
                m.set_shader_parameter("clock", &t.to_variant());
                m.set_shader_parameter("rise", &rise.to_variant());
            }
            amulet_at = Some(at);
        }
        let (eye, aim) = camera(beat, t, descent.map(|d| d as f32), mid, hero, amulet_at);
        self.stage_camera = Some((eye, aim));
        let close = matches!(beat, Beat::Book | Beat::Amulet);
        self.focus(close.then(|| eye.distance_to(aim)));
        if std::env::var("QA_STAGE").is_ok() && (t * 10.0) as i32 % 20 == 0 {
            godot_print!(
                "QA stage: {beat:?} t {t:.1} shown {} titled {} title {} cells {} drawn {} eye {eye:?} aim {aim:?} cam {:?} root vis {}",
                self.shown,
                self.titled,
                self.title.is_some(),
                self.cells.len(),
                self.drawn_cells(),
                self.camera.get_global_position(),
                self.root.is_visible()
            );
        }
    }

    /// What the prologue's sets hold beside their levels, all at once in
    /// the rehearsal's room, their shaders, models and textures drawn under
    /// the start-up veil (the kit's scenes loaded and kept): every piece,
    /// the gods with their light, dust, smoke and embers, a brazier, light
    /// pouring down through a haze, the beats' lights lit (a fog at
    /// nothing, a light at nothing, would not draw what they draw), the
    /// Amulet, the sky. Gone as the rehearsal clears its level.
    pub fn warm_prologue(&mut self) {
        self.end_stage();
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueWarm");
        self.root.add_child(&root);
        let mut stage = Stage::new(Beat::Rebellion, root);
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        let v = Vector3::new;
        for (i, &path) in KIT.iter().enumerate() {
            let at = (mid.x - 5.5 + i as f32, 0.0, mid.z - 1.0);
            stage.todo.push(piece(path, at, 0.0, 0.25));
        }
        self.build_pieces(&mut stage, None);
        haze(
            &mut stage.root,
            mid + v(0.0, 1.5, 0.0),
            v(6.0, 3.0, 4.0),
            0.03,
            Some(EMBERS),
        );
        self.stand_marduk(&mut stage, mid + v(-2.0, 0.0, 0.5), 30.0, 1.2);
        self.stand_moloch(&mut stage, mid + v(2.0, 0.0, 0.5), -30.0, 1.4);
        stage.amulet = Some(self.stage_amulet(&mut stage.root, mid, 0.6));
        for (b, most) in &mut stage.beams {
            b.set_param(Param::ENERGY, *most);
        }
        let lit = [stage.candle.as_mut(), stage.furnace.as_mut()];
        for l in lit.into_iter().flatten() {
            l.set_param(Param::ENERGY, 2.6);
        }
        for (m, density) in &mut stage.smoke {
            m.set_shader_parameter("density", &density.to_variant());
        }
        if let Some(a) = stage.amulet.as_mut() {
            a.lava.set_param(Param::ENERGY, 2.0);
            a.stone.set_param(Param::ENERGY, 1.0);
        }
        let mut book = self.book_model();
        book.set_position(mid + v(0.0, 0.0, 1.0));
        stage.root.add_child(&book);
        self.dome(&mut stage.root, mid);
        self.stage_brazier(&mut stage, mid + v(0.0, 0.0, 1.5), 0.0);
        self.stage = Some(stage);
    }

    /// The air while a beat's scene is drawn: it glows nowhere (a dark
    /// past the walls, never a grey sky), and is clear but for the set's
    /// haze (the title's chamber keeps its own). Nothing drawn (or only
    /// the rehearsal's warm-up): the branch's, as it was.
    pub(super) fn keep_prologue_air(&mut self) {
        let look = self.branch_look;
        let beat = self.stage.as_ref().filter(|_| self.titled).map(|s| s.beat);
        // (the game's own: thinner the farther the camera, none in the
        // overview: `air_near`)
        let near = self.air_near;
        let (density, glow) = match beat {
            None => (look.fog_density * near, super::HAZE * look.haze),
            Some(Beat::Hero) => (look.fog_density, 0.0),
            Some(_) => (PROLOGUE_AIR, 0.0),
        };
        self.env.set_volumetric_fog_density(density);
        self.env.set_volumetric_fog_emission_energy(glow);
        // a dark set: lit only by its own lights (a shaft, fires), none of
        // the level's ambient or its light from above; the game's back as
        // it was when it is over
        let dark = beat.is_some_and(dark_set);
        let ambient = if self.showcase {
            super::GALLERY_AMBIENT
        } else {
            look.ambient_energy
        };
        if dark {
            // (every time: a beat in another branch sets the branch's
            // ambient again as its level comes)
            self.moon.set_visible(false);
            self.back.set_visible(false);
            self.env.set_ambient_light_energy(ambient * DARK_AMBIENT);
        } else if !self.moon.is_visible() {
            self.moon.set_visible(true);
            self.back.set_visible(true);
            self.env.set_ambient_light_energy(ambient);
            // (the lava's lights as the game has them: the Amulet's cavity
            // dims them)
            for l in &mut self.lava_lights {
                l.set_param(Param::ENERGY, super::LAVA_ENERGY);
            }
            let lamps = self
                .cells
                .values_mut()
                .filter_map(|n| n.lamp.as_mut())
                .chain(self.spare_lamps.iter_mut())
                .filter(|l| l.kind == super::Lamp::Lava);
            for l in lamps {
                l.light.set_param(Param::ENERGY, super::LAVA_LAMP_ENERGY);
            }
        }
        // (and as the game's thins with the distance the hero is followed
        // from, what its glow gave made up by the exposure: never in the
        // overview's clear air)
        let k = self.overview_k * self.overview_k * (3.0 - 2.0 * self.overview_k);
        let far = (super::DISTANCE / self.distance.max(1.0)).clamp(super::AIR_LEAST, 1.0);
        let exposure = match beat {
            None => 1.0 + super::FAR_EXPOSURE * 2.0 * (1.0 - far) * (1.0 - k),
            Some(_) => 1.0,
        };
        self.env.set_tonemap_exposure(exposure);
    }

    /// Which beat's scene the map draws (self-tests).
    pub fn prologue_beat(&self) -> Option<Beat> {
        self.stage.as_ref().map(|s| s.beat)
    }

    /// The camera sharp to `near` metres and a little past, blurring
    /// beyond (a close beat's), or sharp throughout (None).
    fn focus(&mut self, near: Option<f32>) {
        let Some(near) = near else {
            if self.camera.get_attributes().is_some() {
                self.camera.set_attributes(Gd::null_arg());
            }
            return;
        };
        let Some(mut attrs) = self.kept("focus", || {
            let mut a = CameraAttributesPractical::new_gd();
            a.set_dof_blur_far_enabled(true);
            a.set_dof_blur_far_transition(FOCUS_SOFT);
            a.set_dof_blur_amount(FOCUS_BLUR);
            Some(a)
        }) else {
            return;
        };
        attrs.set_dof_blur_far_distance(near + FOCUS_BEHIND);
        if self.camera.get_attributes().is_none() {
            self.camera.set_attributes(&attrs);
        }
    }

    /// Let the beat's scene go: its set, its gods, the Amulet; and the next
    /// beat's, if it was built ahead.
    pub(super) fn end_stage(&mut self) {
        self.focus(None);
        for stage in [self.stage.take(), self.next_stage.take()]
            .into_iter()
            .flatten()
        {
            self.free_stage(stage);
        }
    }

    fn free_stage(&mut self, stage: Stage) {
        let mut root = stage.root;
        if root.is_instance_valid() {
            root.queue_free();
        }
    }

    /// The descent's hero steps on, from the cell `from` of `WALK` to
    /// `to`: the level's hero printed there (the map walks the model).
    fn walk_stage_hero(&mut self, from: usize, to: usize) {
        let Some(title) = self.title.as_mut() else {
            return;
        };
        let (world, catalog) = title.parts();
        let floor = cmap(catalog, "S_room");
        let Some(hero) = WALK
            .get(from.saturating_sub(1).min(WALK.len() - 1))
            .and_then(|&(x, y)| world.map.cell(x, y))
            .and_then(|c| c.glyph.clone())
            .filter(|g| g.flags & mg::HERO != 0)
        else {
            return;
        };
        let start = WALK[from.saturating_sub(1).min(WALK.len() - 1)];
        let end = WALK[(to - 1).min(WALK.len() - 1)];
        // the last step onto the stairs
        let end = if to >= WALK.len() { STAIRS } else { end };
        if let Some(f) = floor.as_ref() {
            world.map.print(start.0, start.1, f, None);
        }
        world.map.print(end.0, end.1, &hero, floor.as_ref());
    }
}

/// How far into a breath of `period` seconds `t` is: 0 out, 1 in (as
/// `prologue_figure.gdshader` breathes).
fn inhale(t: f32, period: f32) -> f32 {
    0.5 + 0.5 * (std::f32::consts::TAU * t / period.max(0.1)).sin()
}

/// A particle's colour over its life: `color` faded in and out.
fn fading(color: Color) -> Gd<GradientTexture1D> {
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.2, 0.8, 1.0][..]));
    g.set_colors(&PackedColorArray::from(
        &[color.with_alpha(0.0), color, color, color.with_alpha(0.0)][..],
    ));
    let mut ramp = GradientTexture1D::new_gd();
    ramp.set_gradient(&g);
    ramp
}

/// A particle's quad, `size` across.
fn quad_mesh(size: f32) -> Gd<QuadMesh> {
    let mut q = QuadMesh::new_gd();
    q.set_size(Vector2::new(size, size));
    q
}

/// A beat whose set only its own lights light (a shaft, fires): the rest
/// falls to black.
fn dark_set(beat: Beat) -> bool {
    matches!(beat, Beat::Rebellion | Beat::Amulet)
}

/// `t` eased in and out (0..1).
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The camera of a beat's scene `t` seconds into it: where it is and what
/// it looks at (the scene laid round `mid`; the hero and the Amulet where
/// they are now).
fn camera(
    beat: Beat,
    t: f32,
    descent: Option<f32>,
    mid: Vector3,
    hero: Option<Vector3>,
    amulet: Option<Vector3>,
) -> (Vector3, Vector3) {
    let v = Vector3::new;
    match beat {
        // close over the Book, moving in
        Beat::Book => {
            let aim = mid + v(0.0, 0.66, 0.05);
            let d = 2.3 - 0.75 * smooth(t / 12.0);
            (aim + v(0.0, 0.55, 0.85).normalized() * d, aim)
        }
        // the temple from low down its nave, the light and the dark face
        // to face, moving in (looking up the more, Moloch's horns kept)
        Beat::Rebellion => {
            let k = smooth(t / 12.0);
            let aim = v(mid.x, 3.0 + 0.6 * k, mid.z);
            (
                v(mid.x + 0.9 - 1.4 * k, 1.5 + 0.2 * k, mid.z + 8.5 - 1.5 * k),
                aim,
            )
        }
        // down with the Amulet into the lava
        Beat::Amulet => {
            let aim = amulet.unwrap_or(mid + v(0.0, 1.5, 0.0));
            (v(mid.x + 0.6, aim.y.max(0.2) + 1.5, mid.z + 2.9), aim)
        }
        // up from the floor of the shrine to the altar in its light
        Beat::God => {
            let k = smooth(t / 10.0);
            let aim = mid + v(0.0, 1.0 + 0.6 * k, -2.0);
            (mid + v(0.0, 0.9 + 1.5 * k, 4.2 - 0.8 * k), aim)
        }
        // round the hero, close
        Beat::Hero => {
            let hero = hero.unwrap_or(v(
                crate::title_scene::HERO.0 as f32,
                0.0,
                crate::title_scene::HERO.1 as f32,
            ));
            let aim = hero + v(0.0, 1.05, 0.0);
            let yaw = (-24.0 + 48.0 * smooth(t / 16.0)).to_radians();
            let pitch = 12f32.to_radians();
            let d = 3.4;
            let off = v(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            ) * d;
            (aim + off, aim)
        }
        // behind the hero as they go to the stairs, then down into them
        Beat::Descent => {
            let hero = hero.unwrap_or(v(40.0, 0.0, 10.0));
            let down = descent.map_or(0.0, |d| smooth((d - 1.6) / 1.4));
            let aim = hero + v(0.0, 0.9 - 0.9 * down, -1.6);
            let eye = hero + v(0.0, 2.4 - 0.8 * down, 3.4 - 1.4 * down);
            (eye, aim)
        }
    }
}

/// A beat's level: the cells of its scene.
fn level(beat: Beat, hero: &PrologueHero, cat: &Catalog) -> World {
    let mut world = World::new();
    world.set_catalog(cat);
    let dungeon = if beat == Beat::Amulet {
        "Gehennom"
    } else {
        "The Dungeons of Doom"
    };
    world.level = Some(LevelNotice {
        dungeon: dungeon.to_string(),
        depth: 1,
        plane: None,
        lair: None,
    });
    let altar = altar_color(hero.align);
    let (mx, my) = MID;
    let map = &mut world.map;
    let put = |map: &mut nh_world::MapState, x: i32, y: i32, sym: &str, color: Option<i32>| {
        if let Some(g) = cmap(cat, sym) {
            let g = Glyph {
                color: color.unwrap_or(g.color),
                ..g
            };
            map.print(x, y, &g, None);
        }
    };
    match beat {
        Beat::Book => {
            // a nook in the dark, the god's altar in it, the Book on it
            for y in my - 1..=my + 1 {
                for x in mx - 2..=mx + 2 {
                    put(map, x, y, "S_darkroom", None);
                }
            }
            put(map, mx, my, "S_altar", Some(altar));
        }
        Beat::Rebellion => {
            // the floor of a temple (its walls the set's), dark (a lit
            // hall's fill lights lit its vault evenly grey), its altar before
            // the arch at the far end
            hall(map, cat, (31, 5), (49, 16), dark_set(beat));
            put(map, mx, 6, "S_altar", Some(1));
        }
        Beat::Amulet => {
            // Gehennom's lava, a lake of it in the dark rock
            for y in my - 4..=my + 4 {
                for x in mx - 6..=mx + 6 {
                    let (dx, dy) = ((x - mx) as f32 / 4.5, (y - my) as f32 / 2.8);
                    // (its rock unlit: a lit floor's fill lights lit the rocks
                    // all one orange)
                    let sym = if dx * dx + dy * dy <= 1.0 {
                        "S_lava"
                    } else {
                        "S_darkroom"
                    };
                    put(map, x, y, sym, None);
                }
            }
        }
        Beat::God => {
            // a shrine's floor (its walls the set's), the god's altar in
            // its light
            hall(map, cat, (mx - 3, my - 3), (mx + 3, my + 4), dark_set(beat));
            put(map, mx, my - 2, "S_altar", Some(altar));
        }
        Beat::Hero => {}
        Beat::Descent => {
            // a short hall to the stairs down (its walls the set's), the
            // hero at its foot
            hall(
                map,
                cat,
                (mx - 1, STAIRS.1),
                (mx + 1, WALK[0].1 + 3),
                dark_set(beat),
            );
            put(map, STAIRS.0, STAIRS.1, "S_dnstair", None);
            if let Some(g) = hero_glyph(cat, &hero.role, hero.female) {
                let floor = cmap(cat, "S_room");
                map.print(WALK[0].0, WALK[0].1, &g, floor.as_ref());
            }
        }
    }
    if matches!(beat, Beat::Descent) {
        world.inventory = hero.inventory.clone();
    }
    world
}

/// A floor from `a` to `b`, without walls: lit (the map lights it, its
/// fill lights over it), or `dark` (only the set's own lights light it).
fn hall(map: &mut nh_world::MapState, cat: &Catalog, a: (i32, i32), b: (i32, i32), dark: bool) {
    if let Some(g) = cmap(cat, if dark { "S_darkroom" } else { "S_room" }) {
        for y in a.1..=b.1 {
            for x in a.0..=b.0 {
                map.print(x, y, &g, None);
            }
        }
    }
}

/// The cloth on the god's altar, by their alignment: a lawful god's gold,
/// a neutral one's blue, a chaotic one's red (NetHack's colours).
fn altar_color(align: i32) -> i32 {
    match align.signum() {
        1 => 11,
        0 => 12,
        _ => 1,
    }
}

fn cmap(cat: &Catalog, sym: &str) -> Option<Glyph> {
    let info = cat.cmap.iter().find(|c| c.sym == sym)?;
    Some(Glyph {
        cmap: Some(info.idx),
        color: info.color,
        ..glyph(GlyphKind::Cmap, info.ch)
    })
}

/// The hero's glyph for a role (its name or code), a woman's or a man's.
fn hero_glyph(cat: &Catalog, role: &str, female: bool) -> Option<Glyph> {
    let role = cat
        .roles
        .iter()
        .find(|r| {
            r.name.eq_ignore_ascii_case(role)
                || r.code.eq_ignore_ascii_case(role)
                || r.name_female
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(role))
        })
        .map_or_else(|| role.to_lowercase(), |r| r.name.to_lowercase());
    let m = cat
        .monsters
        .iter()
        .find(|m| m.name == role_monster(&role))?;
    let gender = if female { mg::FEMALE } else { mg::MALE };
    Some(Glyph {
        mon: Some(m.idx),
        color: m.color,
        flags: mg::HERO | gender,
        ..glyph(GlyphKind::Mon, '@' as i32)
    })
}

/// A light of `color` at `at`, reaching `range`.
fn light(root: &mut Gd<Node3D>, color: Color, at: Vector3, range: f32) -> Gd<OmniLight3D> {
    let mut l = OmniLight3D::new_alloc();
    l.set_color(color);
    l.set_param(Param::RANGE, range);
    l.set_param(Param::ATTENUATION, 1.1);
    l.set_shadow(false);
    l.set_position(at);
    root.add_child(&l);
    l
}

/// A beat's set: the pieces stood round its level, in the order they
/// stand.
fn set(beat: Beat) -> Vec<Piece> {
    let mut s = Vec::new();
    match beat {
        Beat::Book => {
            // the sanctum's far wall behind the altar, an arch in it;
            // candles either side of the altar, books and a scroll by it
            for (scene, x) in [(WALL, 34.0), (ARCH, 38.0), (WALL, 42.0)] {
                s.push(piece(scene, (x, 0.0, 7.0), 0.0, 1.0));
            }
            s.push(piece(CANDLES, (38.7, 0.0, 8.5), 30.0, 1.3));
            s.push(piece(CANDLES, (41.3, 0.0, 8.5), 150.0, 1.3));
            s.push(piece(BOOK, (38.95, 0.04, 9.25), 25.0, 1.3));
            s.push(piece(BOOK, (38.97, 0.12, 9.22), 5.0, 1.2));
            s.push(piece(SCROLL, (41.05, 0.0, 9.35), -30.0, 1.3));
        }
        Beat::Rebellion => {
            // a hall of the gods: its far wall three storeys high, a great
            // arch in it, its side walls two; down the nave two rows of
            // pillars, its vaults on them (hanging where they meet over
            // the nave), the aisles' on the side walls
            for y in [0.0, 3.0, 6.0] {
                for x in [30.0, 34.0, 38.0, 42.0, 46.0] {
                    let scene = if x == 38.0 && y == 0.0 { ARCH } else { WALL };
                    s.push(piece(scene, (x, y, 4.5), 0.0, 1.0));
                }
            }
            for y in [0.0, 3.0] {
                for z in [8.5, 12.5, 16.5] {
                    s.push(piece(WALL, (32.0, y, z), 90.0, 1.0));
                    s.push(piece(WALL, (48.0, y, z - 4.0), -90.0, 1.0));
                }
            }
            for z in [4.5, 8.5, 12.5] {
                for x in [36.0, 44.0] {
                    // (none behind Moloch: lit, it showed through his cape)
                    if (x, z) != (44.0, 4.5) {
                        s.push(piece(PILLAR, (x, 0.0, z), 0.0, 1.0));
                    }
                }
            }
            for z in [6.5, 10.5, 14.5] {
                for x in [34.0, 38.0, 42.0, 46.0] {
                    s.push(piece(VAULT, (x, 6.0, z), 0.0, 1.0));
                }
            }
        }
        Beat::Amulet => {
            // the rock ringing the lava
            for (i, (x, z, k)) in [
                (34.4, 7.2, 2.4),
                (35.2, 11.9, 1.8),
                (45.8, 7.6, 2.6),
                (45.1, 11.8, 1.6),
                (36.9, 4.9, 2.0),
                (43.2, 4.7, 2.2),
                (40.3, 4.3, 1.7),
            ]
            .into_iter()
            .enumerate()
            {
                s.push(piece(BOULDER, (x, -0.15, z), i as f32 * 67.0, k));
            }
        }
        Beat::God => {
            // a shrine: walls two storeys high round the altar, an arch
            // behind it, one great vault over all
            for y in [0.0, 3.0] {
                for x in [34.0, 38.0, 42.0] {
                    let scene = if x == 38.0 && y == 0.0 { ARCH } else { WALL };
                    s.push(piece(scene, (x, y, 5.5), 0.0, 1.0));
                }
                for z in [9.5, 13.5] {
                    s.push(piece(WALL, (36.0, y, z), 90.0, 1.0));
                    s.push(piece(WALL, (44.0, y, z - 4.0), -90.0, 1.0));
                }
            }
            s.push(piece(VAULT, (40.0, 3.0, 9.5), 0.0, 2.0));
        }
        Beat::Hero => {}
        Beat::Descent => {
            // a vaulted way to the stairs down, an arch over them
            s.push(piece(ARCH, (38.0, 0.0, STAIRS.1 as f32 - 0.6), 0.0, 1.0));
            s.push(piece(WALL, (38.0, 3.0, STAIRS.1 as f32 - 0.6), 0.0, 1.0));
            for z in [9.4, 13.4] {
                s.push(piece(WALL, (38.0, 0.0, z), 90.0, 1.0));
                s.push(piece(WALL, (42.0, 0.0, z - 4.0), -90.0, 1.0));
            }
            for z in [7.4, 11.4] {
                s.push(piece(BARREL, (40.0, 3.0, z), 0.0, 2.0));
            }
        }
    }
    s
}

/// A piece of a set stood on the stage.
fn stand_piece(root: &mut Gd<Node3D>, scene: &Gd<PackedScene>, p: Piece) {
    let Some(node) = scene.instantiate() else {
        return;
    };
    let Ok(mut node) = node.try_cast::<Node3D>() else {
        return;
    };
    node.set_scale(Vector3::new(p.scale, p.scale, p.scale));
    node.set_rotation_degrees(Vector3::new(0.0, p.yaw, 0.0));
    node.set_position(p.at);
    root.add_child(&node);
}

/// Light pouring down on `at` from `from`, `angle` degrees from its
/// middle to its edge: a spot light, and a column of haze along it that
/// shows its shaft. Dark till the beat lights it.
fn beam(
    root: &mut Gd<Node3D>,
    at: Vector3,
    from: Vector3,
    color: Color,
    angle: f32,
) -> Gd<SpotLight3D> {
    let mut l = SpotLight3D::new_alloc();
    l.set_color(color);
    l.set_param(Param::RANGE, from.distance_to(at) + 1.5);
    l.set_param(Param::SPOT_ANGLE, angle);
    l.set_param(Param::SPOT_ATTENUATION, 0.8);
    l.set_param(Param::ATTENUATION, 0.5);
    l.set_param(Param::ENERGY, 0.0);
    l.set_param(Param::VOLUMETRIC_FOG_ENERGY, BEAM_HAZE);
    l.set_shadow(false);
    // its -z along the way down
    let back = (from - at).normalized();
    let up = if back.cross(Vector3::UP).length() < 0.01 {
        Vector3::BACK
    } else {
        Vector3::UP
    };
    let x = up.cross(back).normalized();
    let y = back.cross(x);
    l.set_transform(Transform3D::new(Basis::from_cols(x, y, back), from));
    root.add_child(&l);
    // the column, its y along the light, as wide as the light at its foot
    let long = from.distance_to(at);
    let wide = 2.0 * long * angle.to_radians().tan();
    let mut mat = FogMaterial::new_gd();
    mat.set_density(SHAFT_DENSITY);
    mat.set_albedo(Color::from_rgb(0.85, 0.82, 0.78));
    mat.set_edge_fade(0.7);
    let mut shaft = FogVolume::new_alloc();
    shaft.set_shape(FogVolumeShape::CYLINDER);
    shaft.set_size(Vector3::new(wide, long, wide));
    shaft.set_material(&mat);
    let (sx, sz) = (y, x);
    shaft.set_transform(Transform3D::new(
        Basis::from_cols(sx, back, sz),
        (from + at) / 2.0,
    ));
    root.add_child(&shaft);
    l
}

/// A box of haze `size` round `at`, `density` thick, glowing `emission`
/// if it does: what light pouring through it shows.
fn haze(root: &mut Gd<Node3D>, at: Vector3, size: Vector3, density: f32, emission: Option<Color>) {
    let mut mat = FogMaterial::new_gd();
    mat.set_density(density);
    mat.set_albedo(Color::from_rgb(0.85, 0.82, 0.78));
    if let Some(e) = emission {
        mat.set_emission(e);
    }
    mat.set_edge_fade(0.4);
    let mut fog = FogVolume::new_alloc();
    fog.set_shape(FogVolumeShape::BOX);
    fog.set_size(size);
    fog.set_position(at);
    fog.set_material(&mat);
    root.add_child(&fog);
}

impl MapView {
    /// Gehennom's sky over the stage round `at`: smouldering at the
    /// skyline, dark overhead.
    fn dome(&mut self, root: &mut Gd<Node3D>, at: Vector3) {
        let Some(mat) = self.kept("sky", || {
            let (horizon, top) = (EMBERS, SKY_TOP);
            let mut g = Gradient::new_gd();
            let ground = horizon.lerp(Color::BLACK, 0.85);
            g.set_offsets(&PackedFloat32Array::from(
                &[0.0, 0.3, 0.47, 0.52, 0.6, 1.0][..],
            ));
            g.set_colors(&PackedColorArray::from(
                &[
                    top,
                    top.lerp(horizon, 0.35),
                    horizon,
                    horizon,
                    ground,
                    Color::BLACK,
                ][..],
            ));
            let mut t = GradientTexture2D::new_gd();
            t.set_gradient(&g);
            t.set_width(4);
            t.set_height(256);
            t.set_fill_from(Vector2::new(0.0, 0.0));
            t.set_fill_to(Vector2::new(0.0, 1.0));
            let mut m = StandardMaterial3D::new_gd();
            m.set_shading_mode(ShadingMode::UNSHADED);
            m.set_flag(Flags::DISABLE_FOG, true);
            m.set_texture(TextureParam::ALBEDO, &t);
            Some(m.upcast::<Material>())
        }) else {
            return;
        };
        let mut sphere = SphereMesh::new_gd();
        sphere.set_radius(SKY);
        sphere.set_height(2.0 * SKY);
        sphere.set_radial_segments(32);
        sphere.set_rings(16);
        sphere.set_flip_faces(true);
        let mut sky = MeshInstance3D::new_alloc();
        sky.set_mesh(&sphere);
        sky.set_material_override(&mat);
        no_shadow(&mut sky);
        sky.set_position(at);
        root.add_child(&sky);
    }

    /// A material, texture or scene of the sets, made (or loaded) once and
    /// kept: the next prologue's draws with what the start-up veil hid
    /// being built.
    fn kept<T>(&mut self, key: &'static str, make: impl FnOnce() -> Option<Gd<T>>) -> Option<Gd<T>>
    where
        T: Inherits<Resource>,
    {
        if let Some(r) = self.stage_kit.get(key) {
            return r.clone().try_cast::<T>().ok();
        }
        let made = make()?;
        self.stage_kit.insert(key, made.clone().upcast());
        Some(made)
    }

    /// The Book, open: leather covers, the pages in a dark hand, an
    /// initial in red.
    fn book_model(&mut self) -> Gd<Node3D> {
        let mut book = Node3D::new_alloc();
        book.set_name("Book");
        let leather = self
            .art
            .flat(Color::from_rgb(0.24, 0.09, 0.05), Finish::Matte);
        let edges = self
            .art
            .flat(Color::from_rgb(0.78, 0.7, 0.54), Finish::Matte);
        let cover = self.art.mesh(cuboid(0.6, 0.02, 0.41));
        add_mesh(&mut book, &cover, &leather, Vector3::new(0.0, 0.01, 0.0));
        let block = self.art.mesh(cuboid(0.27, 0.034, 0.38));
        let leaf = self.art.mesh(plane(0.255, 0.365));
        for (side, seed, initial) in [(-1.0f32, 7u32, true), (1.0, 23, false)] {
            let key = if initial { "page left" } else { "page right" };
            let page = self.kept(key, || {
                let mut m = StandardMaterial3D::new_gd();
                m.set_texture(TextureParam::ALBEDO, &page_texture(seed, initial)?);
                m.set_roughness(0.85);
                Some(m.upcast::<Material>())
            });
            let mut pages = Node3D::new_alloc();
            pages.set_position(Vector3::new(side * 0.142, 0.037, 0.0));
            // the leaves sloping down into the gutter
            pages.set_rotation_degrees(Vector3::new(0.0, 0.0, side * 4.0));
            add_mesh(&mut pages, &block, &edges, Vector3::ZERO);
            if let Some(page) = page {
                add_mesh(&mut pages, &leaf, &page, Vector3::new(0.0, 0.0175, 0.0));
            }
            book.add_child(&pages);
        }
        book
    }
}

/// A mesh of `mat` at `at` under `parent`.
fn add_mesh(
    parent: &mut Gd<Node3D>,
    mesh: &Gd<Mesh>,
    mat: &Gd<Material>,
    at: Vector3,
) -> Gd<MeshInstance3D> {
    let mut mi = MeshInstance3D::new_alloc();
    mi.set_mesh(mesh);
    mi.set_material_override(mat);
    mi.set_position(at);
    parent.add_child(&mi);
    mi
}

/// A page of the Book: lines of a dark hand on parchment browned at its
/// edges, an initial in red and gold at its head (`initial`).
fn page_texture(seed: u32, initial: bool) -> Option<Gd<ImageTexture>> {
    const W: usize = 256;
    const H: usize = 360;
    let mut px = vec![0u8; W * H * 3];
    let put = |px: &mut [u8], x: usize, y: usize, c: [u8; 3]| {
        if x < W && y < H {
            let i = (y * W + x) * 3;
            px[i..i + 3].copy_from_slice(&c);
        }
    };
    let mut state = seed.wrapping_mul(2_654_435_761) | 1;
    let mut rand = move |n: u32| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state % n.max(1)
    };
    for y in 0..H {
        for x in 0..W {
            let ex = (x as f32 / W as f32 - 0.5).abs() * 2.0;
            let ey = (y as f32 / H as f32 - 0.5).abs() * 2.0;
            let edge = ((ex.max(ey) - 0.72) / 0.28).max(0.0);
            let k = 1.0 - 0.45 * edge * edge;
            let c = [(226.0 * k) as u8, (204.0 * k) as u8, (156.0 * k) as u8];
            put(&mut px, x, y, c);
        }
    }
    let (left, right, top, bottom, line) = (24, W - 22, 30, H - 30, 14);
    if initial {
        // a red square, gilt round, three lines high
        for y in top - 2..top + 34 {
            for x in left..left + 34 {
                let rim = x < left + 3 || x >= left + 31 || y < top + 1 || y >= top + 31;
                put(
                    &mut px,
                    x,
                    y,
                    if rim { [196, 150, 52] } else { [150, 24, 18] },
                );
            }
        }
    }
    let mut y = top;
    let mut row = 0;
    while y + 6 < bottom {
        let mut x = if initial && row < 3 { left + 40 } else { left };
        if row % 9 == 8 {
            // a paragraph's end: half a line
            x = right - (right - x) / 2;
        }
        while x + 8 < right {
            let letters = 2 + rand(6) as usize;
            for _ in 0..letters {
                let w = 3 + rand(2) as usize;
                if x + w >= right {
                    break;
                }
                let ink = 40 + rand(30) as u8;
                let tall = rand(5) == 0;
                let deep = rand(7) == 0;
                for dx in 0..w {
                    let (from, to) = (
                        if tall && dx == 0 { y - 3 } else { y },
                        if deep && dx == w - 1 { y + 8 } else { y + 5 },
                    );
                    for yy in from..to {
                        put(&mut px, x + dx, yy, [ink + 18, ink + 4, ink / 2]);
                    }
                }
                x += w + 1;
            }
            x += 5 + rand(3) as usize;
        }
        y += line;
        row += 1;
    }
    let data = PackedByteArray::from(px);
    let mut image = Image::create_from_data(W as i32, H as i32, false, Format::RGB8, &data)?;
    image.generate_mipmaps();
    ImageTexture::create_from_image(&image)
}
