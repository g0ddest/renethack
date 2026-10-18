//! The prologue's scenes (`prologue` tells its beats): each a small level
//! of its own the map draws as it draws the title's chamber (a world of
//! its own, while no game shows), with what a level cannot hold beside it,
//! and a camera of its own moving slowly through it:
//! - the Book: a tome on the god's altar in the dark, the altar's candles
//!   its only light;
//! - the rebellion: a temple of pillars, light pouring down on one side, a
//!   black fog on the other, a light that takes light away in it;
//! - the Amulet: Gehennom's lava, the Amulet sinking into it, glowing;
//! - the god: their altar in a shrine, light pouring down on it;
//! - the hero: the title's chamber, the hero in their kit;
//! - the descent: the hero walking to the stairs down.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::rendering_server::FogVolumeShape;
use godot::classes::{
    FogMaterial, FogVolume, MeshInstance3D, Node3D, OmniLight3D, PackedScene, Shader,
    ShaderMaterial,
};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, LevelNotice, mg};
use nh_world::{Pack, World};

use super::{MapView, ModelLook, flicker, object_tile, role_monster, tint_color};
use crate::art::{Model, Pose};
use crate::meshes::cylinder;
use crate::prologue::Beat;
use crate::rehearsal::glyph;
use crate::title_scene::TitleScene;

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
/// The light pouring down and the black fog: their colours, how tall the
/// light's column is, how thick the fog grows.
const HOLY: Color = Color::from_rgb(1.0, 0.93, 0.72);
const DARK: Color = Color::from_rgb(0.05, 0.02, 0.08);
const SHAFT_TALL: f32 = 7.0;
const FOG_DENSITY: f32 = 2.4;
/// The candles' light over the Book.
const CANDLE: Color = Color::from_rgb(1.0, 0.68, 0.36);
/// The Amulet's own glow, where it starts above the lava and where it is
/// gone under.
const AMULET_GLOW: Color = Color::from_rgb(1.0, 0.8, 0.42);
const AMULET_FROM: f32 = 3.6;
const AMULET_TO: f32 = -0.7;
/// The temple's pillars (the Sigil's Vault dungeon kit), how tall they
/// stand, and where.
const PILLAR: &str = "res://art/cc0/sigilsvault/dungeon/pieces/pillar_large_01.glb";
const PILLAR_TALL: f32 = 3.4;
const PILLARS: [(i32, i32); 6] = [(34, 8), (34, 11), (37, 7), (43, 7), (46, 8), (46, 11)];
/// The descent: the cells the hero walks to the stairs on, a step every
/// this many seconds.
const WALK: [(i32, i32); 4] = [(40, 10), (40, 9), (40, 8), (40, 7)];
pub const STAIRS: (i32, i32) = (40, 6);
const STEP_SECS: f64 = 0.62;
/// On the stairs from this far into the descent, the hero goes down them,
/// this deep by its end.
const SINK_AT: f64 = 2.3;
const SINK_DEPTH: f32 = 1.5;

/// A beat's scene as drawn: what stands beside its level, and what moves.
pub(super) struct Stage {
    beat: Beat,
    root: Gd<Node3D>,
    candle: Option<Gd<OmniLight3D>>,
    /// The light pouring down: its shader's strength, the most of it.
    veils: Vec<(Gd<ShaderMaterial>, f32)>,
    fog: Option<Gd<FogMaterial>>,
    shade: Option<Gd<OmniLight3D>>,
    amulet: Option<(Model, Gd<OmniLight3D>)>,
    /// The descent: the cell the hero stands on now.
    walked: usize,
}

impl MapView {
    /// Draw `beat`'s scene for `hero` (None: none, and whatever the map
    /// draws without it after): its level, and its stage (built ahead by
    /// `prepare_prologue` while the beat before was read, else now).
    pub fn show_prologue(&mut self, beat: Option<Beat>, hero: &PrologueHero, catalog: &Catalog) {
        let began = std::time::Instant::now();
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
        let built_ahead = ready.as_ref().is_some_and(|s| s.beat == beat);
        let mut stage = match ready {
            Some(stage) if stage.beat == beat => stage,
            other => {
                if let Some(stage) = other {
                    self.free_stage(stage);
                }
                self.build_stage(beat, catalog)
            }
        };
        stage.root.set_visible(true);
        if let Some((m, _)) = stage.amulet.as_mut() {
            m.node.set_visible(true);
        }
        self.stage = Some(stage);
        self.stage_camera = None;
        if self.stats_window.is_some() {
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
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

    /// Build `beat`'s stage ahead, hidden till its beat: the frames of the
    /// beat before have room for it, and the change under the black has
    /// the less to do.
    pub fn prepare_prologue(&mut self, beat: Beat, catalog: &Catalog) {
        if self.next_stage.as_ref().is_some_and(|s| s.beat == beat) {
            return;
        }
        let began = std::time::Instant::now();
        if let Some(stage) = self.next_stage.take() {
            self.free_stage(stage);
        }
        let mut stage = self.build_stage(beat, catalog);
        stage.root.set_visible(false);
        if let Some((m, _)) = stage.amulet.as_mut() {
            m.node.set_visible(false);
        }
        self.next_stage = Some(stage);
        if self.stats_window.is_some() {
            godot_print!(
                "map: the prologue's {beat:?} built ahead in {:.1} ms",
                began.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    /// What `beat`'s scene holds beside its level: its lights, the temple's
    /// pillars, the light pouring down, the black fog, the Amulet.
    fn build_stage(&mut self, beat: Beat, catalog: &Catalog) -> Stage {
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueStage");
        self.root.add_child(&root);
        let mut stage = Stage {
            beat,
            root,
            candle: None,
            veils: Vec::new(),
            fog: None,
            shade: None,
            amulet: None,
            walked: 0,
        };
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        match beat {
            Beat::Book => {
                // the altar's candles over the Book
                stage.candle = Some(light(
                    &mut stage.root,
                    CANDLE,
                    mid + Vector3::new(0.0, 1.15, -0.1),
                    3.2,
                ));
            }
            Beat::Rebellion => {
                if let Ok(pillar) = godot::tools::try_load::<PackedScene>(PILLAR) {
                    for &(x, y) in &PILLARS {
                        stand(
                            &mut stage.root,
                            &pillar,
                            Vector3::new(x as f32, 0.0, y as f32),
                            PILLAR_TALL,
                        );
                    }
                }
                // Marduk's light, Moloch's dark
                let light_at = Vector3::new(37.6, 0.0, 9.0);
                self.stage_shaft(&mut stage, light_at);
                let mut l = light(
                    &mut stage.root,
                    HOLY,
                    light_at + Vector3::new(0.0, 2.6, 0.0),
                    6.0,
                );
                l.set_param(Param::ENERGY, 0.0);
                stage.candle = Some(l);
                let dark_at = Vector3::new(42.4, 1.2, 9.0);
                stage.fog = Some(black_fog(&mut stage.root, dark_at));
                // and a black whirl of it rising, as a vortex's
                if let Some(whirl) = dark_whirl(Vector3::new(dark_at.x, 0.0, dark_at.z)) {
                    stage.root.add_child(&whirl);
                }
                let mut shade = light(&mut stage.root, DARK, dark_at, 4.5);
                shade.set_negative(true);
                shade.set_param(Param::ENERGY, 0.0);
                stage.shade = Some(shade);
            }
            Beat::Amulet => {
                stage.amulet = self.take_amulet(catalog, &mut stage.root, mid);
            }
            Beat::God => {
                let altar = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32 - 2.0);
                self.stage_shaft(&mut stage, altar);
                // softer over the altar: it shows through its light
                for (_, most) in &mut stage.veils {
                    *most *= 0.5;
                }
                let mut l = light(
                    &mut stage.root,
                    HOLY,
                    altar + Vector3::new(0.0, 2.8, 0.4),
                    5.0,
                );
                l.set_param(Param::ENERGY, 0.0);
                stage.candle = Some(l);
            }
            Beat::Hero | Beat::Descent => {}
        }
        stage
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
                _ => 2.6 * rise,
            };
            c.set_param(Param::ENERGY, energy);
        }
        for (mat, most) in &mut stage.veils {
            mat.set_shader_parameter("strength", &(rise * *most).to_variant());
        }
        if let Some(fog) = stage.fog.as_mut() {
            fog.set_density(FOG_DENSITY * rise);
        }
        if let Some(shade) = stage.shade.as_mut() {
            shade.set_param(Param::ENERGY, 3.0 * rise);
        }
        let mut amulet_at = None;
        if let Some((m, _)) = stage.amulet.as_mut().filter(|(m, _)| m.is_pending()) {
            self.art.complete(m);
        }
        let Some(stage) = self.stage.as_mut() else {
            return;
        };
        if let Some((m, glow)) = stage.amulet.as_mut() {
            // it sinks slowly, turning, and is gone under the lava
            let k = smooth((t / 13.0).min(1.0));
            let y = AMULET_FROM + (AMULET_TO - AMULET_FROM) * k;
            let at = mid + Vector3::new(0.0, y, 0.0);
            m.node.set_position(at);
            m.node.set_rotation(Vector3::new(0.35, t * 0.6, 0.0));
            glow.set_position(at + Vector3::new(0.0, 0.25, 0.0));
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

    /// What the prologue's scenes hold beside their levels, all at once
    /// in the rehearsal's room (its shaders and models drawn under the
    /// start-up veil): the temple's pillars, the light and the black whirl,
    /// the Amulet. Gone as the rehearsal clears its level.
    pub fn warm_prologue(&mut self) {
        self.end_stage();
        let mut root = Node3D::new_alloc();
        root.set_name("PrologueWarm");
        self.root.add_child(&root);
        let mut stage = Stage {
            beat: Beat::Rebellion,
            root,
            candle: None,
            veils: Vec::new(),
            fog: None,
            shade: None,
            amulet: None,
            walked: 0,
        };
        let mid = Vector3::new(MID.0 as f32, 0.0, MID.1 as f32);
        if let Ok(pillar) = godot::tools::try_load::<PackedScene>(PILLAR) {
            stand(
                &mut stage.root,
                &pillar,
                mid + Vector3::new(-3.0, 0.0, 0.0),
                PILLAR_TALL,
            );
        }
        self.stage_shaft(&mut stage, mid + Vector3::new(-1.5, 0.0, 0.0));
        for (mat, most) in &mut stage.veils {
            mat.set_shader_parameter("strength", &most.to_variant());
        }
        // the fog as thick as it grows, the beats' lights lit (a fog at
        // nothing, a light at nothing, would not draw what they draw)
        let mut fog = black_fog(&mut stage.root, mid + Vector3::new(1.5, 1.2, 0.0));
        fog.set_density(FOG_DENSITY);
        stage.fog = Some(fog);
        let mut holy = light(
            &mut stage.root,
            HOLY,
            mid + Vector3::new(-1.5, 2.6, 0.0),
            6.0,
        );
        holy.set_param(Param::ENERGY, 2.6);
        let mut shade = light(
            &mut stage.root,
            DARK,
            mid + Vector3::new(1.5, 1.2, 0.0),
            4.5,
        );
        shade.set_negative(true);
        shade.set_param(Param::ENERGY, 3.0);
        if let Some(whirl) = dark_whirl(mid + Vector3::new(1.5, 0.0, 0.0)) {
            stage.root.add_child(&whirl);
        }
        if let Some(catalog) = self.title_catalog.clone() {
            stage.amulet = self.take_amulet(&catalog, &mut stage.root, mid);
        }
        self.stage = Some(stage);
    }

    /// Which beat's scene the map draws (self-tests).
    pub fn prologue_beat(&self) -> Option<Beat> {
        self.stage.as_ref().map(|s| s.beat)
    }

    /// Let the beat's scene go: what stood beside its level, the Amulet;
    /// and the next beat's, if it was built ahead.
    pub(super) fn end_stage(&mut self) {
        for stage in [self.stage.take(), self.next_stage.take()]
            .into_iter()
            .flatten()
        {
            self.free_stage(stage);
        }
    }

    fn free_stage(&mut self, stage: Stage) {
        if let Some((m, _)) = stage.amulet {
            self.art.give(m);
        }
        let mut root = stage.root;
        if root.is_instance_valid() {
            root.queue_free();
        }
    }

    /// A column of light pouring down at `at` (on the floor), soft at its
    /// sides and fading up (the shader a pleased god's answer is drawn
    /// with), a brighter core in it.
    fn stage_shaft(&mut self, stage: &mut Stage, at: Vector3) {
        let Ok(shader) = godot::tools::try_load::<Shader>("res://shaders/holy_light.gdshader")
        else {
            return;
        };
        for (top, bottom, weight) in [(0.5, 0.7, 0.55), (0.2, 0.28, 0.35)] {
            let mut mat = ShaderMaterial::new_gd();
            mat.set_shader(&shader);
            mat.set_shader_parameter("color", &HOLY.to_variant());
            mat.set_shader_parameter("height", &SHAFT_TALL.to_variant());
            mat.set_shader_parameter("strength", &0.0f32.to_variant());
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_mesh(&self.art.mesh(cylinder(top, bottom, SHAFT_TALL)));
            mi.set_material_override(&mat);
            mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            mi.set_position(at + Vector3::new(0.0, SHAFT_TALL / 2.0, 0.0));
            stage.root.add_child(&mi);
            stage.veils.push((mat, weight));
        }
    }

    /// The Amulet of Yendor's model, glowing, at `at`.
    fn take_amulet(
        &mut self,
        catalog: &Catalog,
        root: &mut Gd<Node3D>,
        at: Vector3,
    ) -> Option<(Model, Gd<OmniLight3D>)> {
        let tile = catalog
            .object_tiles
            .iter()
            .find(|t| t.appearance == "Amulet of Yendor")?;
        let tile = object_tile(catalog, tile.tile)?;
        let r = self.art.manifest().object(tile);
        let tint = tint_color(r.tint, tile.color.unwrap_or(6));
        let look = ModelLook {
            art: r,
            tint,
            pose: Pose::Ware,
        };
        let mut m = self.art.take(&look);
        // (a stand-in past the frame's budget: built in a frame after)
        self.art.complete(&mut m);
        m.node.set_scale(Vector3::new(3.4, 3.4, 3.4));
        m.node
            .set_position(at + Vector3::new(0.0, AMULET_FROM, 0.0));
        let glow = light(
            root,
            AMULET_GLOW,
            at + Vector3::new(0.0, AMULET_FROM, 0.0),
            3.5,
        );
        Some((m, glow))
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
        // the temple, the light and the dark face to face, moving in
        Beat::Rebellion => {
            let k = smooth(t / 11.0);
            let aim = mid + v(0.0, 1.5, -0.2);
            (
                v(mid.x + 0.6 - 1.2 * k, 3.0 - 0.6 * k, mid.z + 6.6 - 1.6 * k),
                aim,
            )
        }
        // down with the Amulet into the lava
        Beat::Amulet => {
            let aim = amulet.unwrap_or(mid + v(0.0, 1.5, 0.0));
            (v(mid.x + 0.8, aim.y.max(0.2) + 2.7, mid.z + 4.6), aim)
        }
        // up from the floor of the shrine to the altar in its light
        Beat::God => {
            let k = smooth(t / 10.0);
            let aim = mid + v(0.0, 1.0 + 0.6 * k, -2.0);
            (mid + v(0.0, 0.9 + 2.1 * k, 4.2 - 0.8 * k), aim)
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
            if let Some(book) = object(cat, "leathery") {
                let floor = cmap(cat, "S_altar");
                map.print(mx, my, &book, floor.as_ref());
            }
        }
        Beat::Rebellion => {
            // a temple: a long hall, lit, its altar at the far end
            room(map, cat, (33, 6), (47, 12), true);
            put(map, mx, 7, "S_altar", Some(1));
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
            // a shrine, the god's altar in its light
            room(map, cat, (mx - 3, my - 3), (mx + 3, my + 2), true);
            put(map, mx, my - 2, "S_altar", Some(altar));
        }
        Beat::Hero => {}
        Beat::Descent => {
            // a short hall to the stairs down, the hero at its foot
            room(
                map,
                cat,
                (mx - 2, STAIRS.1 - 1),
                (mx + 2, WALK[0].1 + 1),
                true,
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

/// A room of floor from `a` to `b`, walled on its far (north) side and
/// its two others, open to the camera; `lit`: a lit room's floor.
fn room(map: &mut nh_world::MapState, cat: &Catalog, a: (i32, i32), b: (i32, i32), lit: bool) {
    let floor = if lit { "S_room" } else { "S_darkroom" };
    for y in a.1..=b.1 {
        for x in a.0..=b.0 {
            let sym = match (x, y) {
                (x, y) if x == a.0 && y == a.1 => "S_tlcorn",
                (x, y) if x == b.0 && y == a.1 => "S_trcorn",
                (_, y) if y == a.1 => "S_hwall",
                (x, _) if x == a.0 || x == b.0 => "S_vwall",
                _ => floor,
            };
            if let Some(g) = cmap(cat, sym) {
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

fn object(cat: &Catalog, appearance: &str) -> Option<Glyph> {
    let t = cat
        .object_tiles
        .iter()
        .find(|t| t.appearance == appearance)?;
    let ch = t.class.chars().next().map_or('*' as i32, |c| c as i32);
    Some(Glyph {
        tile: t.tile,
        color: t.color.unwrap_or(7),
        ..glyph(GlyphKind::Obj, ch)
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

/// A black whirl standing on the floor at `at`, slowly turning (the
/// engulfed view's funnel, in Moloch's dark).
fn dark_whirl(at: Vector3) -> Option<Gd<MeshInstance3D>> {
    let shader = godot::tools::try_load::<Shader>(crate::engulf::WIND_SHADER).ok()?;
    let mut mat = ShaderMaterial::new_gd();
    mat.set_shader(&shader);
    for (name, value) in [
        ("color", Color::from_rgb(0.05, 0.02, 0.07).to_variant()),
        ("core", Color::from_rgb(0.32, 0.12, 0.4).to_variant()),
        ("glow", 0.0f32.to_variant()),
        ("bands", 3.0f32.to_variant()),
        ("twist", 1.3f32.to_variant()),
        ("speed", 0.45f32.to_variant()),
        ("density", 0.95f32.to_variant()),
        ("soft", 0.0f32.to_variant()),
        ("streaks", 0.0f32.to_variant()),
        ("specks", 2i32.to_variant()),
    ] {
        mat.set_shader_parameter(name, &value);
    }
    let mut mi = MeshInstance3D::new_alloc();
    mi.set_mesh(&crate::engulf::funnel_mesh((0.6, 1.9, 3.6)));
    mi.set_material_override(&mat);
    mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    mi.set_position(at);
    Some(mi)
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

/// A model of `scene` stood on the floor at `at`, `tall` high.
fn stand(root: &mut Gd<Node3D>, scene: &Gd<PackedScene>, at: Vector3, tall: f32) {
    let Some(node) = scene.instantiate() else {
        return;
    };
    let Ok(mut node) = node.try_cast::<Node3D>() else {
        return;
    };
    root.add_child(&node);
    let height = super::world_box(&node).map_or(1.0, |b| b.size.y.max(0.01));
    let s = tall / height;
    node.set_scale(Vector3::new(s, s, s));
    node.set_position(at);
}
