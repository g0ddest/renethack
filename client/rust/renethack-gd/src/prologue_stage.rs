//! The prologue's scenes (`prologue` tells its beats): each a small level
//! of its own the map draws as it draws the title's chamber (a world of
//! its own, while no game shows), a set built round it of what a level
//! cannot hold (the Sigil's Vault dungeon kit's vaults, pillars and walls,
//! props, the gods' figures, light pouring down through a haze), and a
//! camera of its own moving slowly through it:
//! - the Book: a tome on the god's altar in a candlelit sanctum, books and
//!   scrolls about it, a chandelier over it;
//! - the rebellion: a vaulted temple of pillars, Marduk a figure of light
//!   in a shaft of it, Moloch a dark one in a black fog, braziers between;
//! - the Amulet: a cavern of Gehennom under a smouldering sky, its lava in
//!   a ring of rock, the Amulet sinking into it, glowing;
//! - the god: their altar in a vaulted shrine, light pouring down on it,
//!   braziers either side;
//! - the hero: the title's chamber, the hero in their kit;
//! - the descent: the hero walking to the stairs down, under a vault.
//!
//! A beat's set is built a few pieces a frame while the beat before is
//! read (`prepare_prologue`), and the rehearsal draws one of each under
//! the start-up veil (`warm_prologue`).

use std::time::{Duration, Instant};

use godot::classes::base_material_3d::{Feature, Flags, ShadingMode, TextureParam};
use godot::classes::image::Format;
use godot::classes::light_3d::Param;
use godot::classes::rendering_server::FogVolumeShape;
use godot::classes::{
    FogMaterial, FogVolume, Gradient, GradientTexture2D, Image, ImageTexture, Material, Mesh,
    MeshInstance3D, Node3D, OmniLight3D, PackedScene, Resource, SphereMesh, SpotLight3D,
    StandardMaterial3D,
};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, LevelNotice, mg};
use nh_world::{Pack, World};

use super::{MapView, ModelLook, flicker, role_monster};
use crate::art::{Finish, Model, Pose, no_shadow};
use crate::meshes::{cuboid, cylinder, plane, sphere, torus};
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
/// The light pouring down and the black fog: their colours, how thick the
/// fog grows.
const HOLY: Color = Color::from_rgb(1.0, 0.93, 0.72);
const DARK: Color = Color::from_rgb(0.05, 0.02, 0.08);
const FOG_DENSITY: f32 = 2.4;
/// The candles' light over the Book, and how high the Book lies on the
/// altar (its top, the cloth on it).
const CANDLE: Color = Color::from_rgb(1.0, 0.68, 0.36);
const BOOK_ON: f32 = 0.612;
/// Gehennom's heat: its sky at the skyline (and overhead), the glow of the
/// air over its lava.
const EMBERS: Color = Color::from_rgb(0.2, 0.045, 0.015);
const SKY_TOP: Color = Color::from_rgb(0.012, 0.004, 0.003);
/// The Amulet's own glow, where it starts above the lava and where it is
/// gone under, and how much bigger than life it is drawn.
const AMULET_GLOW: Color = Color::from_rgb(1.0, 0.8, 0.42);
const AMULET_FROM: f32 = 3.6;
const AMULET_TO: f32 = -0.7;
const AMULET_SIZE: f32 = 2.6;
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
/// How far the sky over Gehennom is.
const SKY: f32 = 40.0;
/// The Amulet's chain: how many links in its loop.
const CHAIN_LINKS: usize = 30;
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
/// The gods of the rebellion, as the bestiary draws them: Marduk a being
/// of light, Moloch a horned dark one (outlined by the fires behind him),
/// each this tall.
const MARDUK: &str = "Angel";
const MOLOCH: &str = "pit fiend";
const MOLOCH_RIM: Color = Color::from_rgb(1.0, 0.36, 0.12);
const GOD_TALL: f32 = 3.6;
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

/// A beat's scene as drawn: what stands beside its level, and what moves.
pub(super) struct Stage {
    beat: Beat,
    root: Gd<Node3D>,
    candle: Option<Gd<OmniLight3D>>,
    /// The light pouring down, the most of its energy.
    beams: Vec<(Gd<SpotLight3D>, f32)>,
    fog: Option<Gd<FogMaterial>>,
    shade: Option<Gd<OmniLight3D>>,
    /// The braziers' lights, each at its brightness and its own phase.
    fires: Vec<(Gd<OmniLight3D>, f32, f64)>,
    /// The gods' figures (the bestiary's models, given back after).
    figures: Vec<Model>,
    amulet: Option<(Gd<Node3D>, Gd<OmniLight3D>)>,
    /// Set pieces still to stand.
    todo: Vec<Piece>,
    /// The descent: the cell the hero stands on now.
    walked: usize,
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
        let built_ahead = ready
            .as_ref()
            .is_some_and(|s| s.beat == beat && s.todo.is_empty());
        let mut stage = match ready {
            Some(stage) if stage.beat == beat => stage,
            other => {
                if let Some(stage) = other {
                    self.free_stage(stage);
                }
                self.build_stage(beat, catalog)
            }
        };
        self.build_pieces(&mut stage, None);
        stage.root.set_visible(true);
        for m in &mut stage.figures {
            m.node.set_visible(true);
        }
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
    pub fn prepare_prologue(&mut self, beat: Beat, catalog: &Catalog) {
        if self
            .next_stage
            .as_ref()
            .is_some_and(|s| s.beat == beat && s.todo.is_empty())
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
                let mut stage = self.build_stage(beat, catalog);
                stage.root.set_visible(false);
                for m in &mut stage.figures {
                    m.node.set_visible(false);
                }
                stage
            }
        };
        self.build_pieces(&mut stage, Some(PIECES_BUDGET));
        if self.stats_window.is_some() {
            godot_print!(
                "map: the prologue's {beat:?} built ahead for {:.1} ms, {} pieces to go",
                began.elapsed().as_secs_f64() * 1000.0,
                stage.todo.len()
            );
        }
        self.next_stage = Some(stage);
    }

    /// `beat`'s set but its pieces (`build_pieces` stands them): its lights,
    /// the haze the light pours through, the braziers, the gods, the
    /// Amulet, the sky.
    fn build_stage(&mut self, beat: Beat, catalog: &Catalog) -> Stage {
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueStage");
        self.root.add_child(&root);
        let mut stage = Stage {
            beat,
            root,
            candle: None,
            beams: Vec::new(),
            fog: None,
            shade: None,
            fires: Vec::new(),
            figures: Vec::new(),
            amulet: None,
            todo: set(beat),
            walked: 0,
        };
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
                // Marduk in the light pouring down on him, Moloch in his
                // black fog, a light that takes light away in it
                let marduk = v(37.7, 0.0, 10.0);
                let moloch = v(42.3, 0.0, 10.0);
                let tall = GOD_TALL;
                stage.figures.extend(self.figure(
                    catalog,
                    MARDUK,
                    Color::WHITE,
                    marduk,
                    55.0,
                    tall,
                ));
                let shadow = Color::from_rgb(0.22, 0.16, 0.2);
                stage
                    .figures
                    .extend(self.figure(catalog, MOLOCH, shadow, moloch, -55.0, tall));
                let from = marduk + v(-1.5, 8.0, -2.5);
                stage
                    .beams
                    .push((beam(&mut stage.root, marduk, from, HOLY, 9.0), 4.5));
                let mut l = light(&mut stage.root, HOLY, marduk + v(0.8, 2.4, 1.6), 5.0);
                l.set_param(Param::ENERGY, 0.0);
                l.set_param(Param::VOLUMETRIC_FOG_ENERGY, 0.0);
                stage.candle = Some(l);
                // Moloch's outline, the fires behind him
                let rim = light(&mut stage.root, MOLOCH_RIM, moloch + v(0.6, 2.6, -1.6), 4.5);
                stage.fires.push((rim, 2.4, 0.7));
                let dark_at = moloch + v(0.0, 1.2, 0.0);
                stage.fog = Some(black_fog(&mut stage.root, dark_at));
                let mut shade = light(&mut stage.root, DARK, dark_at, 4.5);
                shade.set_negative(true);
                shade.set_param(Param::ENERGY, 0.0);
                stage.shade = Some(shade);
                haze(
                    &mut stage.root,
                    v(40.0, 3.5, 9.5),
                    v(14.0, 7.0, 10.0),
                    0.03,
                    None,
                );
                for (k, x) in [37.6, 42.4].into_iter().enumerate() {
                    self.stage_brazier(&mut stage, v(x, 0.0, 6.4), k as f64 * 2.1);
                }
            }
            Beat::Amulet => {
                stage.amulet = Some(self.stage_amulet(&mut stage.root, mid));
                self.dome(&mut stage.root, mid);
                // the lava's heat glowing in the air over it
                let heat = Color::from_rgb(0.5, 0.13, 0.03);
                haze(
                    &mut stage.root,
                    mid + v(0.0, 1.0, 0.0),
                    v(14.0, 2.6, 9.0),
                    0.03,
                    Some(heat),
                );
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

    /// Stand the stage's pieces still to stand, for at most `budget` (all
    /// of them: None).
    fn build_pieces(&mut self, stage: &mut Stage, budget: Option<Duration>) {
        let began = Instant::now();
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

    /// A monster of the bestiary (the catalog's `name`) stood as a figure
    /// of a scene: `tall` high at `at`, turned `yaw` degrees, `tint`ed.
    fn figure(
        &mut self,
        catalog: &Catalog,
        name: &str,
        tint: Color,
        at: Vector3,
        yaw: f32,
        tall: f32,
    ) -> Option<Model> {
        let info = catalog.monsters.iter().find(|m| m.name == name)?;
        let art = self.art.manifest().monster(info, 0);
        let height = art.height.max(0.1);
        let mut m = self.art.take(&ModelLook {
            art,
            tint,
            pose: Pose::Alive,
        });
        self.art.complete(&mut m);
        let k = tall / height;
        m.node.set_scale(Vector3::new(k, k, k));
        m.node.set_position(at);
        m.node.set_rotation_degrees(Vector3::new(0.0, yaw, 0.0));
        Some(m)
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

    /// The Amulet of Yendor at `at`, high over it, and its glow.
    fn stage_amulet(
        &mut self,
        root: &mut Gd<Node3D>,
        at: Vector3,
    ) -> (Gd<Node3D>, Gd<OmniLight3D>) {
        let mut amulet = self.amulet_model();
        let k = AMULET_SIZE / 10.0;
        amulet.set_scale(Vector3::new(k, k, k));
        amulet.set_position(at + Vector3::new(0.0, AMULET_FROM, 0.0));
        root.add_child(&amulet);
        let glow = light(
            root,
            AMULET_GLOW,
            at + Vector3::new(0.0, AMULET_FROM, 0.0),
            3.5,
        );
        (amulet, glow)
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
        let hero = self.hero_stands().map(|(p, _)| p);
        let Some(stage) = self.stage.as_mut() else {
            return;
        };
        if let Some(c) = stage.candle.as_mut() {
            let energy = match beat {
                Beat::Book => 1.7 * flicker(secs * 1.3),
                Beat::Rebellion => 1.3 * rise,
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
        if let Some(fog) = stage.fog.as_mut() {
            fog.set_density(FOG_DENSITY * rise);
        }
        if let Some(shade) = stage.shade.as_mut() {
            shade.set_param(Param::ENERGY, 3.0 * rise);
        }
        let mut amulet_at = None;
        if let Some((amulet, glow)) = stage.amulet.as_mut() {
            // it sinks slowly, turning, and is gone under the lava
            let k = smooth((t / 13.0).min(1.0));
            let y = AMULET_FROM + (AMULET_TO - AMULET_FROM) * k;
            let at = mid + Vector3::new(0.0, y, 0.0);
            amulet.set_position(at);
            amulet.set_rotation(Vector3::new(0.2, t * 0.6, 0.0));
            glow.set_position(at + Vector3::new(0.4, 0.6, 1.2));
            glow.set_param(
                Param::ENERGY,
                2.2 * (1.0 - smooth(((y - 0.2) / -0.9).clamp(0.0, 1.0))),
            );
            amulet_at = Some(at);
        }
        let (eye, aim) = camera(beat, t, descent.map(|d| d as f32), mid, hero, amulet_at);
        self.stage_camera = Some((eye, aim));
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
    /// the gods, a brazier, light pouring down through a haze, the black
    /// fog, the beats' lights lit (a fog at nothing, a light at nothing,
    /// would not draw what they draw), the Amulet, the sky. Gone as the
    /// rehearsal clears its level.
    pub fn warm_prologue(&mut self) {
        self.end_stage();
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueWarm");
        self.root.add_child(&root);
        let mut stage = Stage {
            beat: Beat::Rebellion,
            root,
            candle: None,
            beams: Vec::new(),
            fog: None,
            shade: None,
            fires: Vec::new(),
            figures: Vec::new(),
            amulet: None,
            todo: Vec::new(),
            walked: 0,
        };
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        let v = Vector3::new;
        for (i, &path) in KIT.iter().enumerate() {
            let at = (mid.x - 5.5 + i as f32, 0.0, mid.z - 1.0);
            stage.todo.push(piece(path, at, 0.0, 0.25));
        }
        self.build_pieces(&mut stage, None);
        let at = mid + v(-1.5, 0.0, 0.0);
        let from = at + v(-1.0, 4.0, -1.0);
        stage
            .beams
            .push((beam(&mut stage.root, at, from, HOLY, 9.0), 6.0));
        haze(
            &mut stage.root,
            mid + v(0.0, 1.5, 0.0),
            v(6.0, 3.0, 4.0),
            0.03,
            Some(EMBERS),
        );
        let mut fog = black_fog(&mut stage.root, mid + v(1.5, 1.2, 0.0));
        fog.set_density(FOG_DENSITY);
        stage.fog = Some(fog);
        let mut holy = light(&mut stage.root, HOLY, mid + v(-1.5, 2.6, 0.0), 6.0);
        holy.set_param(Param::ENERGY, 2.6);
        let mut shade = light(&mut stage.root, DARK, mid + v(1.5, 1.2, 0.0), 4.5);
        shade.set_negative(true);
        shade.set_param(Param::ENERGY, 3.0);
        if let Some(catalog) = self.title_catalog.clone() {
            let shadow = Color::from_rgb(0.22, 0.16, 0.2);
            for (name, tint, x) in [(MARDUK, Color::WHITE, -2.0), (MOLOCH, shadow, 2.0)] {
                let at = mid + v(x, 0.0, 0.5);
                stage
                    .figures
                    .extend(self.figure(&catalog, name, tint, at, 0.0, 1.2));
            }
        }
        stage.amulet = Some(self.stage_amulet(&mut stage.root, mid));
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
        let (density, glow) = match beat {
            None => (look.fog_density, super::HAZE * look.haze),
            Some(Beat::Hero) => (look.fog_density, 0.0),
            Some(_) => (PROLOGUE_AIR, 0.0),
        };
        self.env.set_volumetric_fog_density(density);
        self.env.set_volumetric_fog_emission_energy(glow);
    }

    /// Which beat's scene the map draws (self-tests).
    pub fn prologue_beat(&self) -> Option<Beat> {
        self.stage.as_ref().map(|s| s.beat)
    }

    /// Let the beat's scene go: its set, its gods, the Amulet; and the next
    /// beat's, if it was built ahead.
    pub(super) fn end_stage(&mut self) {
        for stage in [self.stage.take(), self.next_stage.take()]
            .into_iter()
            .flatten()
        {
            self.free_stage(stage);
        }
    }

    fn free_stage(&mut self, stage: Stage) {
        for m in stage.figures {
            self.art.give(m);
        }
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
        // to face, moving in
        Beat::Rebellion => {
            let k = smooth(t / 12.0);
            let aim = v(mid.x, 2.7 + 0.3 * k, mid.z);
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
            // the floor of a temple (its walls the set's), lit, its altar
            // before the arch at the far end
            hall(map, cat, (31, 5), (49, 16));
            put(map, mx, 6, "S_altar", Some(1));
        }
        Beat::Amulet => {
            // Gehennom's lava, a lake of it in the dark rock
            for y in my - 4..=my + 4 {
                for x in mx - 6..=mx + 6 {
                    let (dx, dy) = ((x - mx) as f32 / 4.5, (y - my) as f32 / 2.8);
                    let sym = if dx * dx + dy * dy <= 1.0 {
                        "S_lava"
                    } else {
                        "S_room"
                    };
                    put(map, x, y, sym, None);
                }
            }
        }
        Beat::God => {
            // a shrine's floor (its walls the set's), the god's altar in
            // its light
            hall(map, cat, (mx - 3, my - 3), (mx + 3, my + 4));
            put(map, mx, my - 2, "S_altar", Some(altar));
        }
        Beat::Hero => {}
        Beat::Descent => {
            // a short hall to the stairs down (its walls the set's), the
            // hero at its foot
            hall(map, cat, (mx - 1, STAIRS.1), (mx + 1, WALK[0].1 + 3));
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

/// A lit floor from `a` to `b`, without walls.
fn hall(map: &mut nh_world::MapState, cat: &Catalog, a: (i32, i32), b: (i32, i32)) {
    if let Some(g) = cmap(cat, "S_room") {
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

/// A black fog round `at`.
fn black_fog(root: &mut Gd<Node3D>, at: Vector3) -> Gd<FogMaterial> {
    let mut mat = FogMaterial::new_gd();
    mat.set_density(0.0);
    mat.set_albedo(DARK);
    mat.set_edge_fade(0.8);
    let mut fog = FogVolume::new_alloc();
    fog.set_shape(FogVolumeShape::ELLIPSOID);
    fog.set_size(Vector3::new(4.0, 3.2, 3.6));
    fog.set_position(at);
    fog.set_material(&mat);
    root.add_child(&fog);
    mat
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
                    s.push(piece(PILLAR, (x, 0.0, z), 0.0, 1.0));
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

    /// The Amulet of Yendor, ten times its size (the meshes are cached to
    /// the centimetre): a disc of gold, a ring of studs round a red stone
    /// cut and glowing, its bail and a loop of its chain above.
    fn amulet_model(&mut self) -> Gd<Node3D> {
        let mut amulet = Node3D::new_alloc();
        amulet.set_name("Amulet");
        let gold = self
            .kept("gold", || {
                let mut m = StandardMaterial3D::new_gd();
                m.set_albedo(Color::from_rgb(1.0, 0.78, 0.36));
                m.set_metallic(0.6);
                m.set_roughness(0.32);
                m.set_specular(0.8);
                Some(m.upcast::<Material>())
            })
            .unwrap_or_else(|| self.art.flat(AMULET_GLOW, Finish::Glossy));
        let ruby = self
            .kept("ruby", || {
                let mut m = StandardMaterial3D::new_gd();
                m.set_albedo(Color::from_rgb(0.55, 0.02, 0.02));
                m.set_roughness(0.08);
                m.set_specular(1.0);
                m.set_feature(Feature::EMISSION, true);
                m.set_emission(Color::from_rgb(0.9, 0.05, 0.02));
                m.set_emission_energy_multiplier(1.3);
                m.set_feature(Feature::RIM, true);
                m.set_rim(0.3);
                m.set_rim_tint(0.9);
                Some(m.upcast::<Material>())
            })
            .unwrap_or_else(|| self.art.flat(Color::from_rgb(0.9, 0.1, 0.06), Finish::Gem));
        // the disc stands up, its face to +z
        let face = |z: f32| Vector3::new(0.0, 0.0, z);
        let up = Vector3::new(90.0, 0.0, 0.0);
        let parts = [
            (cylinder(1.2, 1.2, 0.26), face(0.0)),
            (torus(1.12, 1.34), face(0.0)),
        ];
        for (key, at) in parts {
            let mesh = self.art.mesh(key);
            let mut mi = add_mesh(&mut amulet, &mesh, &gold, at);
            mi.set_rotation_degrees(up);
        }
        // either face alike (it turns as it sinks): a ring of studs round
        // a ring, the stone cut in it, its pavilion in the disc
        let ring = self.art.mesh(torus(0.64, 0.78));
        let stud = self.art.mesh(sphere(0.11));
        let pavilion = self.art.mesh(cylinder(0.55, 0.0, 0.35));
        let crown = self.art.mesh(cylinder(0.34, 0.55, 0.22));
        for side in [1.0f32, -1.0] {
            let mut mi = add_mesh(&mut amulet, &ring, &gold, face(0.14 * side));
            mi.set_rotation_degrees(up);
            for i in 0..12 {
                let a = i as f32 * std::f32::consts::TAU / 12.0;
                let at = Vector3::new(0.95 * a.cos(), 0.95 * a.sin(), 0.14 * side);
                add_mesh(&mut amulet, &stud, &gold, at);
            }
            for (mesh, z) in [(&pavilion, 0.025), (&crown, 0.31)] {
                let mut mi = add_mesh(&mut amulet, mesh, &ruby, face(z * side));
                mi.set_rotation_degrees(Vector3::new(90.0 * side, 0.0, 0.0));
            }
        }
        // the bail over it, across the disc, and the chain from it
        let bail = self.art.mesh(torus(0.16, 0.28));
        let mut b = add_mesh(&mut amulet, &bail, &gold, Vector3::new(0.0, 1.5, 0.0));
        b.set_rotation_degrees(Vector3::new(0.0, 0.0, 90.0));
        let link = self.art.mesh(torus(0.07, 0.14));
        let (c, r) = (Vector3::new(0.0, 3.7, 0.0), 2.0);
        for i in 0..CHAIN_LINKS {
            let a = -std::f32::consts::FRAC_PI_2
                + (i as f32 + 0.5) * std::f32::consts::TAU / CHAIN_LINKS as f32;
            let at = c + Vector3::new(r * a.cos(), r * a.sin(), 0.0);
            let along = Vector3::new(-a.sin(), a.cos(), 0.0);
            // each link across the one before
            let axis = if i % 2 == 0 {
                Vector3::BACK
            } else {
                Vector3::new(a.cos(), a.sin(), 0.0)
            };
            let side = along.cross(axis);
            let mut mi = add_mesh(&mut amulet, &link, &gold, at);
            mi.set_transform(Transform3D::new(
                Basis::from_cols(along * 1.5, axis, side),
                at,
            ));
        }
        amulet
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
