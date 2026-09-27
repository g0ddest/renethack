//! The 3D map: a floor tile per known cell, raised blocks for walls and
//! closed doors, a billboard glyph for everything that is not plain floor,
//! and a perspective camera following the hero. Cell (x, y) is the point
//! (x, 0, y); one cell is one metre.

use std::collections::HashMap;

use godot::classes::base_material_3d::{BillboardMode, ShadingMode, Transparency};
use godot::classes::environment::{AmbientSource, BgMode};
use godot::classes::{
    BoxMesh, Camera3D, DirectionalLight3D, Environment, Label3D, Mesh, MeshInstance3D, Node3D,
    PlaneMesh, StandardMaterial3D, SystemFont, WorldEnvironment,
};
use godot::prelude::*;
use nh_protocol::{Catalog, Glyph, GlyphKind, mg};
use nh_world::{COLNO, Cell, ROWNO, Terrain, World, cell_terrain, in_field, terrain_of};

use crate::theme::{self, nh_color};

const PITCH_DEG: f32 = 55.0;
const DISTANCE: f32 = 14.0;
const MIN_DISTANCE: f32 = 6.0;
const MAX_DISTANCE: f32 = 30.0;
const FOLLOW_RATE: f32 = 8.0;
/// A glyph label's centre and top above its tile.
const GLYPH_LIFT: f32 = 0.45;
const GLYPH_TOP: f32 = 0.8;
/// Descent per step when a pointer ray is walked through the raised geometry.
const PICK_STEP: f32 = 0.05;

/// How a cell looks; nodes are touched only when it changes.
#[derive(Debug, Clone, PartialEq)]
struct Look {
    tile: Option<(Shape, Color)>,
    glyph: Option<(char, i32)>,
    hero: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    Floor,
    Narrow,
    Block,
    Low,
}

struct CellNodes {
    look: Look,
    tile: Option<Gd<MeshInstance3D>>,
    label: Option<Gd<Label3D>>,
}

pub struct MapView {
    root: Gd<Node3D>,
    cells_root: Gd<Node3D>,
    camera: Gd<Camera3D>,
    cells: HashMap<(i32, i32), CellNodes>,
    generation: Option<u64>,
    meshes: HashMap<Shape, Gd<Mesh>>,
    materials: HashMap<(u32, bool), Gd<StandardMaterial3D>>,
    font: Gd<SystemFont>,
    hover: Gd<MeshInstance3D>,
    cursor: Gd<MeshInstance3D>,
    hero_mark: Gd<MeshInstance3D>,
    target: Vector3,
    focus: Vector3,
    distance: f32,
    snap: bool,
}

fn color_key(c: Color) -> u32 {
    c.to_u32(godot::builtin::ColorChannelOrder::RGBA)
}

fn plane(size: f32) -> Gd<Mesh> {
    let mut m = PlaneMesh::new_gd();
    m.set_size(Vector2::new(size, size));
    m.upcast()
}

fn block(w: f32, h: f32) -> Gd<Mesh> {
    let mut m = BoxMesh::new_gd();
    m.set_size(Vector3::new(w, h, w));
    m.upcast()
}

fn shape_height(shape: Shape) -> f32 {
    match shape {
        Shape::Block => 0.8,
        Shape::Low => 0.5,
        Shape::Floor | Shape::Narrow => 0.0,
    }
}

fn marker(root: &mut Gd<Node3D>, color: Color, size: f32) -> Gd<MeshInstance3D> {
    let mut mat = StandardMaterial3D::new_gd();
    mat.set_shading_mode(ShadingMode::UNSHADED);
    mat.set_transparency(Transparency::ALPHA);
    mat.set_albedo(color);
    let mut mi = MeshInstance3D::new_alloc();
    mi.set_mesh(&plane(size));
    mi.set_material_override(&mat);
    mi.set_visible(false);
    root.add_child(&mi);
    mi
}

impl MapView {
    pub fn new(mut root: Gd<Node3D>) -> MapView {
        let mut env = Environment::new_gd();
        env.set_background(BgMode::COLOR);
        env.set_bg_color(theme::BG);
        env.set_ambient_source(AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.75, 0.75, 0.85));
        env.set_ambient_light_energy(0.8);
        let mut world_env = WorldEnvironment::new_alloc();
        world_env.set_environment(&env);
        root.add_child(&world_env);

        let mut sun = DirectionalLight3D::new_alloc();
        sun.set_rotation_degrees(Vector3::new(-60.0, 30.0, 0.0));
        sun.set_param(godot::classes::light_3d::Param::ENERGY, 0.9);
        root.add_child(&sun);

        let mut camera = Camera3D::new_alloc();
        camera.set_fov(50.0);
        camera.set_current(true);
        root.add_child(&camera);

        let mut cells_root = Node3D::new_alloc();
        root.add_child(&cells_root);
        cells_root.set_name("Cells");

        let meshes = HashMap::from([
            (Shape::Floor, plane(0.96)),
            (Shape::Narrow, plane(0.8)),
            (Shape::Block, block(1.0, shape_height(Shape::Block))),
            (Shape::Low, block(0.9, shape_height(Shape::Low))),
        ]);
        let hover = marker(&mut root, Color::from_rgba(1.0, 1.0, 1.0, 0.18), 1.0);
        let cursor = marker(&mut root, Color::from_rgba(0.3, 0.9, 1.0, 0.45), 1.0);
        let hero_mark = marker(&mut root, Color::from_rgba(1.0, 0.85, 0.3, 0.35), 0.9);
        let center = Vector3::new(COLNO as f32 / 2.0, 0.0, ROWNO as f32 / 2.0);
        let mut view = MapView {
            root,
            cells_root,
            camera,
            cells: HashMap::new(),
            generation: None,
            meshes,
            materials: HashMap::new(),
            font: theme::mono_bold(),
            hover,
            cursor,
            hero_mark,
            target: center,
            focus: center,
            distance: DISTANCE,
            snap: true,
        };
        view.place_camera();
        view
    }

    /// Rebuild on a new generation, else apply dirty cells; camera target =
    /// view_center, else hero; cursor marker when World.cursor differs from
    /// the hero.
    pub fn sync(&mut self, world: &mut World, catalog: &Catalog, delta: f64) {
        let generation = world.map.generation();
        if self.generation != Some(generation) {
            self.generation = Some(generation);
            world.map.take_dirty();
            for y in 0..ROWNO {
                for x in 1..COLNO {
                    self.update_cell(x, y, world.map.cell(x, y), catalog);
                }
            }
            self.snap = true;
        } else {
            for (x, y) in world.map.take_dirty() {
                self.update_cell(x, y, world.map.cell(x, y), catalog);
            }
        }
        let hero = world.map.hero();
        if let Some((x, y)) = world.view_center.or(hero).or(world.cursor) {
            self.target = Vector3::new(x as f32, 0.0, y as f32);
        }
        match hero {
            Some((x, y)) => {
                self.hero_mark
                    .set_position(Vector3::new(x as f32, 0.02, y as f32));
                self.hero_mark.set_visible(true);
            }
            None => self.hero_mark.set_visible(false),
        }
        match world.cursor.filter(|c| hero.is_some() && Some(*c) != hero) {
            Some((x, y)) => {
                self.cursor
                    .set_position(Vector3::new(x as f32, 0.03, y as f32));
                self.cursor.set_visible(true);
            }
            None => self.cursor.set_visible(false),
        }
        if self.snap {
            self.focus = self.target;
            self.snap = false;
        } else {
            let t = (FOLLOW_RATE * delta as f32).clamp(0.0, 1.0);
            self.focus = self.focus.lerp(self.target, t);
        }
        self.place_camera();
    }

    /// The map cell under a screen position: the first wall, door or glyph
    /// the camera ray passes through, else where it meets the ground.
    pub fn cell_at(&self, screen_pos: Vector2) -> Option<(i32, i32)> {
        if !self.camera.is_inside_tree() {
            return None;
        }
        let origin = self.camera.project_ray_origin(screen_pos);
        let dir = self.camera.project_ray_normal(screen_pos);
        pick_cell(origin, dir, |x, y| {
            self.cells
                .get(&(x, y))
                .map_or(0.0, |n| pick_height(&n.look))
        })
    }

    pub fn set_hover(&mut self, cell: Option<(i32, i32)>) {
        match cell {
            Some((x, y)) => {
                self.hover
                    .set_position(Vector3::new(x as f32, 0.04, y as f32));
                self.hover.set_visible(true);
            }
            None => self.hover.set_visible(false),
        }
    }

    /// Wheel steps: positive moves the camera away.
    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance + steps * 1.5).clamp(MIN_DISTANCE, MAX_DISTANCE);
        self.place_camera();
    }

    /// Forget every cell (a new game).
    pub fn clear(&mut self) {
        for (_, nodes) in self.cells.drain() {
            for mut n in [
                nodes.tile.map(|t| t.upcast::<Node>()),
                nodes.label.map(|l| l.upcast()),
            ]
            .into_iter()
            .flatten()
            {
                n.queue_free();
            }
        }
        self.generation = None;
        self.hover.set_visible(false);
        self.cursor.set_visible(false);
        self.hero_mark.set_visible(false);
        self.snap = true;
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
    }

    /// Cells with a tile or a glyph (self-tests check the map is drawn).
    pub fn drawn_cells(&self) -> usize {
        self.cells
            .values()
            .filter(|c| c.look.tile.is_some() || c.look.glyph.is_some())
            .count()
    }

    fn place_camera(&mut self) {
        let pitch = PITCH_DEG.to_radians();
        let offset = Vector3::new(0.0, pitch.sin(), pitch.cos()) * self.distance;
        // aim a little north of the hero: the log covers the bottom of the screen
        let aim = self.focus + Vector3::new(0.0, 0.0, 1.5);
        self.camera.look_at_from_position(aim + offset, aim);
    }

    fn material(&mut self, color: Color, unshaded: bool) -> Gd<StandardMaterial3D> {
        self.materials
            .entry((color_key(color), unshaded))
            .or_insert_with(|| {
                let mut m = StandardMaterial3D::new_gd();
                m.set_albedo(color);
                m.set_roughness(0.9);
                if unshaded {
                    m.set_shading_mode(ShadingMode::UNSHADED);
                }
                m
            })
            .clone()
    }

    fn update_cell(&mut self, x: i32, y: i32, cell: Option<&Cell>, catalog: &Catalog) {
        let look = cell.map(|c| look_of(c, catalog)).unwrap_or(Look {
            tile: None,
            glyph: None,
            hero: false,
        });
        if self.cells.get(&(x, y)).is_some_and(|n| n.look == look) {
            return;
        }
        let mut nodes = self.cells.remove(&(x, y)).unwrap_or(CellNodes {
            look: look.clone(),
            tile: None,
            label: None,
        });
        match look.tile {
            Some((shape, color)) => {
                let mesh = self.meshes[&shape].clone();
                let mat = self.material(color, false);
                let tile = nodes.tile.get_or_insert_with(|| {
                    let mi = MeshInstance3D::new_alloc();
                    self.cells_root.add_child(&mi);
                    mi
                });
                tile.set_mesh(&mesh);
                tile.set_material_override(&mat);
                tile.set_position(Vector3::new(x as f32, shape_height(shape) / 2.0, y as f32));
                tile.set_visible(true);
            }
            None => {
                if let Some(t) = nodes.tile.as_mut() {
                    t.set_visible(false);
                }
            }
        }
        match look.glyph {
            Some((ch, color)) => {
                let font = self.font.clone();
                let label = nodes.label.get_or_insert_with(|| {
                    let mut l = Label3D::new_alloc();
                    l.set_billboard_mode(BillboardMode::ENABLED);
                    l.set_font(&font);
                    l.set_font_size(96);
                    l.set_pixel_size(0.008);
                    l.set_outline_size(18);
                    l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.85));
                    self.cells_root.add_child(&l);
                    l
                });
                let lift = look.tile.map_or(0.0, |(s, _)| shape_height(s));
                label.set_text(&ch.to_string());
                label.set_modulate(if look.hero {
                    Color::from_rgb(1.0, 1.0, 1.0)
                } else {
                    nh_color(color)
                });
                label.set_position(Vector3::new(x as f32, lift + GLYPH_LIFT, y as f32));
                label.set_visible(true);
            }
            None => {
                if let Some(l) = nodes.label.as_mut() {
                    l.set_visible(false);
                }
            }
        }
        nodes.look = look;
        self.cells.insert((x, y), nodes);
    }
}

/// How high a cell's drawn geometry reaches: its tile, and its glyph on top.
fn pick_height(look: &Look) -> f32 {
    let tile = look.tile.map_or(0.0, |(s, _)| shape_height(s));
    if look.glyph.is_some() {
        tile + GLYPH_TOP
    } else {
        tile
    }
}

/// Walk a ray down from the tallest geometry (a glyph on a wall) in small
/// steps; the first cell whose column (0..height) holds the point is hit.
/// A ray that hits nothing raised picks the cell where it meets y = 0.
fn pick_cell(
    origin: Vector3,
    dir: Vector3,
    height: impl Fn(i32, i32) -> f32,
) -> Option<(i32, i32)> {
    if dir.y > -1e-4 {
        return None;
    }
    let ground = -origin.y / dir.y;
    if ground <= 0.0 {
        return None;
    }
    let cell = |t: f32| {
        let p = origin + dir * t;
        (p.x.round() as i32, p.z.round() as i32, p.y)
    };
    let top = shape_height(Shape::Block) + GLYPH_TOP;
    let step = PICK_STEP / -dir.y;
    let mut t = ((top - origin.y) / dir.y).max(0.0);
    while t < ground {
        let (x, y, h) = cell(t);
        if in_field(x, y) && height(x, y) >= h {
            return Some((x, y));
        }
        t += step;
    }
    let (x, y, _) = cell(ground);
    in_field(x, y).then_some((x, y))
}

fn glyph_char(g: &Glyph) -> Option<char> {
    u32::try_from(g.ch)
        .ok()
        .and_then(char::from_u32)
        .filter(|c| !c.is_control() && *c != ' ')
}

/// Terrain drawn as geometry, without a glyph on top.
fn plain(t: Terrain) -> bool {
    matches!(
        t,
        Terrain::Floor
            | Terrain::DarkFloor
            | Terrain::Corridor
            | Terrain::Wall
            | Terrain::Doorway
            | Terrain::Stone
    )
}

fn tile_of(t: Terrain, glyph_color: i32) -> Option<(Shape, Color)> {
    let rgb = Color::from_rgb;
    Some(match t {
        Terrain::Stone | Terrain::Effect | Terrain::Unknown => return None,
        Terrain::Wall => (Shape::Block, rgb(0.42, 0.42, 0.46)),
        Terrain::Floor => (Shape::Floor, rgb(0.3, 0.3, 0.32)),
        Terrain::DarkFloor => (Shape::Floor, rgb(0.14, 0.14, 0.16)),
        Terrain::Corridor => (Shape::Narrow, rgb(0.2, 0.18, 0.16)),
        Terrain::Doorway | Terrain::OpenDoor | Terrain::BrokenDoor => {
            (Shape::Floor, rgb(0.36, 0.25, 0.14))
        }
        Terrain::ClosedDoor | Terrain::DrawbridgeUp => (Shape::Low, rgb(0.5, 0.32, 0.15)),
        Terrain::Tree => (Shape::Floor, rgb(0.1, 0.3, 0.1)),
        Terrain::Pool | Terrain::Water => (Shape::Floor, rgb(0.1, 0.2, 0.6)),
        Terrain::Lava | Terrain::LavaWall => (Shape::Floor, rgb(0.7, 0.15, 0.05)),
        Terrain::Ice => (Shape::Floor, rgb(0.55, 0.75, 0.85)),
        Terrain::Air | Terrain::Cloud => (Shape::Floor, rgb(0.5, 0.55, 0.65)),
        Terrain::IronBars => (Shape::Floor, nh_color(glyph_color).darkened(0.5)),
        _ => (Shape::Floor, rgb(0.3, 0.3, 0.32)),
    })
}

fn look_of(cell: &Cell, catalog: &Catalog) -> Look {
    let terrain = cell_terrain(cell, catalog);
    let terrain_color = cell.terrain.as_ref().map_or(7, |g| g.color);
    let top = cell
        .glyph
        .as_ref()
        .filter(|g| !matches!(g.kind, GlyphKind::Unexplored | GlyphKind::Nothing));
    // something stands where the floor was never seen (print_glyph sends an
    // unexplored background under monsters and objects): it is walkable
    let tile = match terrain {
        Some(t) => tile_of(t, terrain_color),
        None if cell.entity().is_some() => Some((Shape::Floor, Color::from_rgb(0.2, 0.2, 0.22))),
        None => None,
    };
    let glyph = top.and_then(|g| {
        if g.kind == GlyphKind::Cmap {
            let t = g
                .cmap
                .and_then(|i| catalog.cmap.get(usize::try_from(i).ok()?))
                .map(|c| terrain_of(&c.sym));
            if t.is_some_and(plain) {
                return None;
            }
        }
        glyph_char(g).map(|c| (c, g.color))
    });
    Look {
        tile,
        glyph,
        hero: top.is_some_and(|g| g.flags & mg::HERO != 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The camera's ray, 55 degrees down and looking north, meeting the
    /// ground at the centre of cell (10, 9).
    fn ray() -> (Vector3, Vector3) {
        let pitch = PITCH_DEG.to_radians();
        let back = Vector3::new(0.0, pitch.sin(), pitch.cos());
        (Vector3::new(10.0, 0.0, 9.0) + back * DISTANCE, -back)
    }

    #[test]
    fn a_ray_over_flat_ground_picks_where_it_lands() {
        let (origin, dir) = ray();
        assert_eq!(pick_cell(origin, dir, |_, _| 0.0), Some((10, 9)));
    }

    #[test]
    fn a_wall_in_front_takes_the_ray() {
        let (origin, dir) = ray();
        let wall = shape_height(Shape::Block);
        // the south wall of the room hides the floor cell north of it
        let h = |x, y| if (x, y) == (10, 10) { wall } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 10)));
        // a glyph standing south of the floor cell covers it too
        let h = |x, y| if (x, y) == (10, 10) { GLYPH_TOP } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 10)));
        // a raised cell behind the landing point is never reached
        let h = |x, y| if (x, y) == (10, 8) { wall } else { 0.0 };
        assert_eq!(pick_cell(origin, dir, h), Some((10, 9)));
    }

    #[test]
    fn a_ray_away_from_the_ground_picks_nothing() {
        let (origin, dir) = ray();
        assert_eq!(pick_cell(origin, -dir, |_, _| 0.0), None);
        let off_map = Vector3::new(-50.0, 0.0, 9.0) + (origin - Vector3::new(10.0, 0.0, 9.0));
        assert_eq!(pick_cell(off_map, dir, |_, _| 0.0), None);
    }
}
