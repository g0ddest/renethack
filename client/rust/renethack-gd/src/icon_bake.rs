//! The icon bake (spec decision 6, ui-design §6.5): every object
//! appearance tile of the catalog drawn by the same art as on the map (its
//! appearance only, never its identity), lit like a painted icon in a
//! 256² SubViewport of its own world, outlined, shadowed and saved as
//! `art/icons/items/<tile>.png` at 128². The `icons` self-test runs it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use godot::classes::camera_3d::ProjectionType;
use godot::classes::environment::{AmbientSource, BgMode, ToneMapper};
use godot::classes::image::{Format, Interpolation};
use godot::classes::light_3d::Param as LightParam;
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::viewport::Msaa;
use godot::classes::{
    Camera3D, DirectionalLight3D, Environment, Image, MeshInstance3D, Node, Node3D, OmniLight3D,
    ProjectSettings, SubViewport, WorldEnvironment,
};
use godot::prelude::*;
use nh_art::{Level, Tint};
use nh_protocol::{Catalog, ObjectTile};

use crate::art::{Art, Model, ModelLook, Pose};
use crate::theme::nh_color;

/// Where the icons go, and the side of the render and of the icon.
pub const OUT_DIR: &str = "res://art/icons/items";
const RENDER: i32 = 256;
const ICON: i32 = 128;
/// The part of the icon the object fills.
const FILL: f32 = 0.8;
/// Corpses on the map are darkened this much (map_view's CORPSE_DARKEN).
const CORPSE_DARKEN: f32 = 0.35;

/// How many icons came from which level of the art's fallback chain.
#[derive(Debug, Default)]
pub struct Report {
    pub written: usize,
    pub levels: BTreeMap<String, usize>,
    pub bytes: u64,
}

impl Report {
    /// Tiles whose icon is not the generic object.
    pub fn specific(&self) -> usize {
        self.written - self.levels.get("Generic").copied().unwrap_or(0)
    }
}

/// The stage: a transparent SubViewport with its own world, lights and an
/// orthographic camera, and an art library whose models live in it.
struct Stage {
    viewport: Gd<SubViewport>,
    camera: Gd<Camera3D>,
    art: Art,
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// The colour a look multiplies its model by (map_view's `tint_color`),
/// from the colour the catalog gives the appearance.
fn tint_color(tint: Tint, color: i32) -> Color {
    match tint {
        Tint::None => Color::WHITE,
        Tint::Glyph(s) => mix(Color::WHITE, nh_color(color), s),
        Tint::Rgb(c, s) => mix(Color::WHITE, Color::from_rgb(c[0], c[1], c[2]), s),
    }
}

/// The view: from the front, a little to the right and above.
const YAW: f32 = 28.0;
const PITCH: f32 = -32.0;

fn view_basis() -> Basis {
    Basis::from_euler(
        EulerOrder::YXZ,
        Vector3::new(PITCH.to_radians(), YAW.to_radians(), 0.0),
    )
}

impl Stage {
    fn new(parent: &mut Gd<Node>) -> Stage {
        let mut viewport = SubViewport::new_alloc();
        viewport.set_name("IconBake");
        viewport.set_size(Vector2i::new(RENDER, RENDER));
        viewport.set_use_own_world_3d(true);
        viewport.set_transparent_background(true);
        viewport.set_msaa_3d(Msaa::MSAA_8X);
        viewport.set_update_mode(UpdateMode::ALWAYS);
        parent.add_child(&viewport);

        let mut env = Environment::new_gd();
        env.set_background(BgMode::CLEAR_COLOR);
        env.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        env.set_ambient_source(AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.42, 0.38, 0.40));
        env.set_ambient_light_energy(0.55);
        env.set_tonemapper(ToneMapper::FILMIC);
        let mut world_env = WorldEnvironment::new_alloc();
        world_env.set_environment(&env);
        viewport.add_child(&world_env);

        let view = view_basis();
        // warm key from the top left, in front
        let mut key = DirectionalLight3D::new_alloc();
        key.set_color(Color::from_rgb(1.0, 0.84, 0.62));
        key.set_param(LightParam::ENERGY, 2.4);
        key.set_basis(
            view * Basis::from_euler(
                EulerOrder::YXZ,
                Vector3::new((-40f32).to_radians(), (-40f32).to_radians(), 0.0),
            ),
        );
        key.set_shadow(true);
        viewport.add_child(&key);
        // cool rim from behind on the right
        let mut rim = DirectionalLight3D::new_alloc();
        rim.set_color(Color::from_rgb(0.55, 0.72, 1.0));
        rim.set_param(LightParam::ENERGY, 2.2);
        rim.set_basis(
            view * Basis::from_euler(
                EulerOrder::YXZ,
                Vector3::new((-15f32).to_radians(), (150f32).to_radians(), 0.0),
            ),
        );
        viewport.add_child(&rim);
        // a dim warm fill from below left keeps the shadows from going black
        let mut fill = OmniLight3D::new_alloc();
        fill.set_color(Color::from_rgb(0.9, 0.6, 0.4));
        fill.set_param(LightParam::ENERGY, 0.6);
        fill.set_param(LightParam::RANGE, 40.0);
        fill.set_position(view * Vector3::new(-6.0, -3.0, 8.0));
        viewport.add_child(&fill);

        let mut camera = Camera3D::new_alloc();
        camera.set_projection(ProjectionType::ORTHOGONAL);
        camera.set_far(100.0);
        viewport.add_child(&camera);
        camera.set_current(true);

        let mut root = Node3D::new_alloc();
        root.set_name("Models");
        viewport.add_child(&root);
        Stage {
            viewport,
            camera,
            art: Art::new(root),
        }
    }

    /// The camera framing `aabb` (in world space) to fill FILL of the
    /// frame, looking along the view.
    fn frame(&mut self, center: Vector3, extent: f32) {
        let view = view_basis();
        let pos = center + view * Vector3::new(0.0, 0.0, 20.0);
        self.camera.set_transform(Transform3D::new(view, pos));
        self.camera.set_size(extent.max(0.01) / FILL);
    }

    fn grab(&self) -> Option<Gd<Image>> {
        let mut image = self.viewport.get_texture()?.get_image()?;
        image.convert(Format::RGBA8);
        Some(image)
    }
}

/// Every mesh of a model, in world space: the corners of their boxes.
fn corners(node: &Gd<Node3D>) -> Vec<Vector3> {
    let mut out = Vec::new();
    for n in node
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        let Ok(mi) = n.try_cast::<MeshInstance3D>() else {
            continue;
        };
        if !mi.is_visible_in_tree() {
            continue;
        }
        let t = mi.get_global_transform();
        out.extend(mi.get_aabb().corners().iter().map(|c| t * *c));
    }
    out
}

/// The box of `points` in the holder's own space.
fn local_box(holder: &Gd<Node3D>, points: &[Vector3]) -> Option<Aabb> {
    let inv = holder.get_global_transform().affine_inverse();
    let mut it = points.iter().map(|p| inv * *p);
    let first = it.next()?;
    Some(it.fold(Aabb::new(first, Vector3::ZERO), |b, p| b.expand(p)))
}

/// Turn a long thing (a sword, a wand, a polearm) to lie across the icon
/// from the bottom left to the top right, its flat side to the viewer;
/// anything else is seen from the front at three quarters.
fn present(holder: &mut Gd<Node3D>, upright: bool) {
    let Some(b) = local_box(holder, &corners(holder)) else {
        return;
    };
    let s = b.size;
    if upright {
        // a shield lying face up: stood on its point, its face to the
        // viewer, turned a little (three quarters)
        let local = Basis::from_cols(Vector3::LEFT, Vector3::BACK, Vector3::UP);
        let turn = Basis::from_axis_angle(Vector3::UP, (-14f32).to_radians());
        let basis = view_basis() * turn * local;
        holder.set_transform(Transform3D::new(basis, -(basis * b.center())));
        return;
    }
    let (long, short) = if s.x >= s.z { (s.x, s.z) } else { (s.z, s.x) };
    if long < 1.8 * short.max(s.y) {
        return;
    }
    // the model's long axis to screen x, its up to the viewer
    let local = if s.x >= s.z {
        Basis::from_cols(Vector3::RIGHT, Vector3::BACK, Vector3::DOWN)
    } else {
        Basis::from_cols(Vector3::UP, Vector3::BACK, Vector3::RIGHT)
    };
    let diagonal = Basis::from_axis_angle(Vector3::BACK, 45f32.to_radians())
        * Basis::from_axis_angle(Vector3::RIGHT, (-24f32).to_radians());
    let centre = b.center();
    let basis = view_basis() * diagonal * local;
    holder.set_transform(Transform3D::new(basis, -(basis * centre)));
}

/// The outline and the drop shadow that keep an icon readable on a dark
/// slot, under the rendered object (RGBA8, straight alpha).
fn finish(image: &Gd<Image>) -> Option<Gd<Image>> {
    let (w, h) = (image.get_width() as usize, image.get_height() as usize);
    let src = image.get_data().to_vec();
    let alpha: Vec<f32> = src.chunks(4).map(|p| f32::from(p[3]) / 255.0).collect();
    let at = |x: isize, y: isize, a: &[f32]| -> f32 {
        if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
            0.0
        } else {
            a[y as usize * w + x as usize]
        }
    };
    // outline: the alpha grown by a disc of 3 px (1.5 px at 128)
    let r = 3isize;
    let mut outline = vec![0f32; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let mut m = 0f32;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy <= r * r + 1 {
                        m = m.max(at(x + dx, y + dy, &alpha));
                    }
                }
            }
            outline[y as usize * w + x as usize] = m;
        }
    }
    // shadow: the outline blurred (two box passes) and moved down right
    let blur = |a: &[f32], rad: isize| -> Vec<f32> {
        let mut tmp = vec![0f32; w * h];
        let mut out = vec![0f32; w * h];
        let n = (2 * rad + 1) as f32;
        for y in 0..h as isize {
            for x in 0..w as isize {
                let s: f32 = (-rad..=rad).map(|d| at(x + d, y, a)).sum();
                tmp[y as usize * w + x as usize] = s / n;
            }
        }
        for y in 0..h as isize {
            for x in 0..w as isize {
                let s: f32 = (-rad..=rad).map(|d| at(x, y + d, &tmp)).sum();
                out[y as usize * w + x as usize] = s / n;
            }
        }
        out
    };
    let soft = blur(&blur(&outline, 4), 4);
    let (sx, sy) = (5isize, 7isize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let shadow = at(x as isize - sx, y as isize - sy, &soft) * 0.6;
            // back to front: shadow (black), outline (dark brown), object
            let mut c = [0f32; 3];
            let mut a = shadow;
            let over = |c: &mut [f32; 3], a: &mut f32, col: [f32; 3], ca: f32| {
                let na = ca + *a * (1.0 - ca);
                if na > 0.0 {
                    for k in 0..3 {
                        c[k] = (col[k] * ca + c[k] * *a * (1.0 - ca)) / na;
                    }
                }
                *a = na;
            };
            over(&mut c, &mut a, [0.05, 0.035, 0.03], outline[i] * 0.92);
            let p = &src[i * 4..i * 4 + 4];
            let col = [
                f32::from(p[0]) / 255.0,
                f32::from(p[1]) / 255.0,
                f32::from(p[2]) / 255.0,
            ];
            over(&mut c, &mut a, col, alpha[i]);
            for k in 0..3 {
                out[i * 4 + k] = (c[k] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            out[i * 4 + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    let bytes = PackedByteArray::from(out.as_slice());
    Image::create_from_data(w as i32, h as i32, false, Format::RGBA8, &bytes)
}

/// The used part of an image's alpha: (min x, min y, max x, max y).
fn used(image: &Gd<Image>) -> Option<(f32, f32, f32, f32)> {
    let r = image.get_used_rect();
    (r.size.x > 0 && r.size.y > 0).then(|| {
        (
            r.position.x as f32,
            r.position.y as f32,
            (r.position.x + r.size.x) as f32,
            (r.position.y + r.size.y) as f32,
        )
    })
}

/// A corpse icon: a person lying dead, as the map lays a corpse.
fn corpse_look(art: &Art, cat: &Catalog) -> Option<ModelLook> {
    let m = cat.monsters.iter().find(|m| m.name == "human")?;
    let r = art.manifest().monster(m, 0);
    let tint = mix(tint_color(r.tint, m.color), Color::BLACK, CORPSE_DARKEN);
    Some(ModelLook {
        art: r,
        tint,
        pose: Pose::Corpse,
    })
}

/// The look of an object tile, as the map draws the object.
fn object_look(art: &Art, cat: &Catalog, tile: &ObjectTile) -> (ModelLook, Level) {
    if tile.class == "%"
        && tile.appearance == "corpse"
        && let Some(look) = corpse_look(art, cat)
    {
        return (look, Level::Appearance);
    }
    let r = art.manifest().object(tile);
    let look = ModelLook {
        art: r,
        tint: tint_color(r.tint, tile.color.unwrap_or(7)),
        pose: Pose::Alive,
    };
    (look, r.level)
}

/// Where the bake of one icon is. Godot moves nodes and cameras in the
/// renderer once a frame, so each view is drawn over frames: framed
/// loosely by the meshes' boxes, then tightly by the pixels drawn.
enum Phase {
    /// The next tile to take.
    Next,
    /// A view is set; the picture is taken once it holds still.
    Loose(Model, Vector3, f32, Option<PackedByteArray>, u32),
    Tight(Model, Option<PackedByteArray>, u32),
}

/// The whole bake, one frame at a time.
pub struct Bake {
    stage: Stage,
    catalog: std::rc::Rc<Catalog>,
    dir: PathBuf,
    todo: Vec<usize>,
    phase: Phase,
    pub report: Report,
    failed: Vec<String>,
}

/// Frames a picture may take to hold still (a new material's pipeline
/// compiles in the background, and meanwhile draws nothing).
const STILL_FRAMES: u32 = 240;

impl Bake {
    pub fn new(parent: &mut Gd<Node>, catalog: std::rc::Rc<Catalog>) -> Result<Bake, String> {
        let dir: PathBuf = ProjectSettings::singleton()
            .globalize_path(OUT_DIR)
            .to_string()
            .into();
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // RENETHACK_ICONS_ONLY=tile,tile...: bake only these (while tuning)
        let only: Vec<i32> = std::env::var("RENETHACK_ICONS_ONLY")
            .unwrap_or_default()
            .split(',')
            .filter_map(|t| t.trim().parse().ok())
            .collect();
        let mut todo: Vec<usize> = (0..catalog.object_tiles.len())
            .filter(|i| only.is_empty() || only.contains(&catalog.object_tiles[*i].tile))
            .collect();
        todo.reverse();
        Ok(Bake {
            stage: Stage::new(parent),
            catalog,
            dir,
            todo,
            phase: Phase::Next,
            report: Report::default(),
            failed: Vec::new(),
        })
    }

    /// Tiles still to bake.
    pub fn left(&self) -> usize {
        self.todo.len() + usize::from(!matches!(self.phase, Phase::Next))
    }

    pub fn failures(&self) -> &[String] {
        &self.failed
    }

    /// One frame of work; true when an icon was saved (or failed).
    pub fn tick(&mut self) -> Result<bool, String> {
        match std::mem::replace(&mut self.phase, Phase::Next) {
            Phase::Next => {
                let Some(&i) = self.todo.last() else {
                    return Ok(true);
                };
                let tile = self.catalog.object_tiles[i].clone();
                let (look, _) = object_look(&self.stage.art, &self.catalog, &tile);
                let mut model = self.stage.art.take(&look);
                model.node.set_transform(Transform3D::IDENTITY);
                let shield = tile.class == "[" && tile.appearance.contains("shield");
                present(&mut model.node, shield);
                let pts = corners(&model.node);
                if pts.is_empty() {
                    self.stage.art.give(model);
                    return Ok(self.fail("the model has no meshes"));
                }
                let (view, inv) = (view_basis(), view_basis().inverse());
                let (mut lo, mut hi) = (Vector3::splat(f32::MAX), Vector3::splat(f32::MIN));
                for p in &pts {
                    let v = inv * *p;
                    lo = lo.coord_min(v);
                    hi = hi.coord_max(v);
                }
                let mid = view * ((lo + hi) * 0.5);
                let loose = (hi.x - lo.x).max(hi.y - lo.y) * 1.5 + 0.02;
                self.stage.frame(mid, loose * FILL);
                self.phase = Phase::Loose(model, mid, loose, None, 0);
                Ok(false)
            }
            Phase::Loose(model, mid, loose, last, n) => {
                let image = self.stage.grab().ok_or("no image")?;
                let image = match still(image, last.as_ref(), n) {
                    Ok(image) => image,
                    Err(data) => {
                        return self.wait(Phase::Loose(model, mid, loose, Some(data), n + 1), n);
                    }
                };
                debug_save(&image, "loose");
                let Some((x0, y0, x1, y1)) = used(&image) else {
                    self.stage.art.give(model);
                    return Ok(self.fail("nothing drawn"));
                };
                let unit = loose / RENDER as f32;
                let half = RENDER as f32 * 0.5;
                let (cx, cy) = ((x0 + x1) * 0.5 - half, (y0 + y1) * 0.5 - half);
                let centre = mid + view_basis() * Vector3::new(cx * unit, -cy * unit, 0.0);
                let extent = (x1 - x0).max(y1 - y0) * unit;
                self.stage.frame(centre, extent);
                self.phase = Phase::Tight(model, None, 0);
                Ok(false)
            }
            Phase::Tight(model, last, n) => {
                let image = self.stage.grab().ok_or("no image")?;
                let image = match still(image, last.as_ref(), n) {
                    Ok(image) => image,
                    Err(data) => return self.wait(Phase::Tight(model, Some(data), n + 1), n),
                };
                debug_save(&image, "tight");
                self.stage.art.give(model);
                let mut icon = finish(&image).ok_or("cannot finish the icon")?;
                icon.resize_ex(ICON, ICON)
                    .interpolation(Interpolation::LANCZOS)
                    .done();
                let i = self.todo.pop().ok_or("no tile")?;
                let tile = &self.catalog.object_tiles[i];
                let (_, level) = object_look(&self.stage.art, &self.catalog, tile);
                let path = self.dir.join(format!("{}.png", tile.tile));
                save(&icon, &path)?;
                self.report.bytes += std::fs::metadata(&path).map_or(0, |m| m.len());
                self.report.written += 1;
                *self.report.levels.entry(format!("{level:?}")).or_default() += 1;
                Ok(true)
            }
        }
    }

    fn wait(&mut self, phase: Phase, n: u32) -> Result<bool, String> {
        if n >= STILL_FRAMES {
            let model = match phase {
                Phase::Loose(m, ..) | Phase::Tight(m, ..) => Some(m),
                Phase::Next => None,
            };
            if let Some(m) = model {
                self.stage.art.give(m);
            }
            return Ok(self.fail("the picture never held still"));
        }
        self.phase = phase;
        Ok(false)
    }

    fn fail(&mut self, why: &str) -> bool {
        if let Some(i) = self.todo.pop() {
            let t = &self.catalog.object_tiles[i];
            self.failed
                .push(format!("{} {} {}: {why}", t.tile, t.class, t.appearance));
        }
        true
    }
}

impl Drop for Bake {
    fn drop(&mut self) {
        self.stage.viewport.queue_free();
    }
}

/// The picture, once something is drawn and it is the same as the frame
/// before (after a few frames for the view to reach the renderer).
fn still(
    image: Gd<Image>,
    last: Option<&PackedByteArray>,
    n: u32,
) -> Result<Gd<Image>, PackedByteArray> {
    let data = image.get_data();
    let drawn = image.get_used_rect().size.x > 0;
    if n >= 2 && drawn && last == Some(&data) {
        Ok(image)
    } else {
        Err(data)
    }
}

fn save(image: &Gd<Image>, path: &Path) -> Result<(), String> {
    let err = image.save_png(&path.to_string_lossy().to_string());
    if err == godot::global::Error::OK {
        Ok(())
    } else {
        Err(format!("cannot save {}: {err:?}", path.display()))
    }
}

/// RENETHACK_ICONS_DEBUG=dir: keep the raw renders there.
fn debug_save(image: &Gd<Image>, what: &str) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    if let Ok(dir) = std::env::var("RENETHACK_ICONS_DEBUG") {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let _ = image.save_png(&format!("{dir}/{n:04}-{what}.png"));
    }
}
