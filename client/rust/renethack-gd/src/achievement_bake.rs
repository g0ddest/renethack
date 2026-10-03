//! The achievement icons (spec 2026-10-03-steam-achievements-design.md,
//! phase S4): each achievement's medallion, a gold rim lit from the top
//! left round a dark field with its subject on it: an object or a creature
//! drawn by its map art, a role's hero, a numeral, or an embossed glyph,
//! struck through for a "never did" conduct. The game's are 256 px PNGs in
//! `art/icons/achievements/` (`<id>.png`, `<id>_locked.png`: darkened,
//! desaturated); Steam's are 64 px JPGs in `steam/achievements/` by API
//! name, with `steam/achievements.vdf`, the names and descriptions to
//! enter on the Steamworks site. The `achievement-icons` self-test runs it
//! (`make achievement-icons`).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use godot::classes::base_material_3d::BillboardMode;
use godot::classes::image::{Format, Interpolation};
use godot::classes::{Image, Label3D, Node, ProjectSettings};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::achievements::{Achievement, Achievements, Subject};

use crate::art::{Model, ModelLook, Pose};
use crate::icon_bake::{self, FILL, RENDER, STILL_FRAMES, Stage};
use crate::icons::{self, Glyph};
use crate::map_view::role_monster;
use crate::theme::{self, Face};

/// The game's icons, and Steam's files (at the repository's root).
pub const GAME_DIR: &str = "res://art/icons/achievements";
const STEAM_DIR: &str = "res://../../steam";
/// The game's medallion and Steam's icon.
const SIDE: usize = 256;
const STEAM_SIDE: i32 = 64;
/// The side glyphs are painted at; every subject is then fitted to the
/// field (`fit`).
const SUBJECT: i32 = 200;
/// The part of the field's diameter a subject's diagonal spans.
const SUBJECT_SPAN: f32 = 0.9;
/// The part of a hero's figure (from the top) a portrait shows.
const PORTRAIT: f32 = 0.52;

/// The glyph an emblem is drawn as.
fn emblem_glyph(name: &str) -> Option<Glyph> {
    Some(match name {
        "gehennom" => Glyph::Flames,
        "invocation" => Glyph::Pentagram,
        "planes" => Glyph::Elements,
        "astral" => Glyph::Sunburst,
        "ascension" => Glyph::Ascension,
        "big room" => Glyph::Hall,
        "crown" => Glyph::Crown,
        "vibrating square" => Glyph::Ripples,
        "drawbridge" => Glyph::Drawbridge,
        "tune" => Glyph::Notes,
        _ => return None,
    })
}

/// The pictogram of a glyph or an emblem subject.
fn subject_glyph(subject: Subject) -> Option<Glyph> {
    match subject {
        Subject::Glyph(name) => Glyph::named(name),
        Subject::Emblem(name) => emblem_glyph(name),
        _ => None,
    }
}

// ---- the medallion (RGBA8, straight alpha) ----

fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// `col` at alpha `ca` over `(dst, da)`.
fn over(dst: [f32; 3], da: f32, col: [f32; 3], ca: f32) -> ([f32; 3], f32) {
    let a = ca + da * (1.0 - ca);
    if a <= 0.0 {
        return ([0.0; 3], 0.0);
    }
    let f = |i: usize| (col[i] * ca + dst[i] * da * (1.0 - ca)) / a;
    ([f(0), f(1), f(2)], a)
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// A medallion of `side` pixels: a gold rim lit from the top left round a
/// dark field; `subject` (RGBA8, its width and height) centred on the
/// field, kept inside it; struck through with a red bar when `crossed`.
pub(crate) fn medallion(
    side: usize,
    subject: Option<(&[u8], usize, usize)>,
    crossed: bool,
) -> Vec<u8> {
    let n = side as f32;
    let c = n / 2.0;
    let outer = c - 1.5;
    let rim = n * 0.075;
    let inner = outer - rim;
    let (gold_dim, gold, gold_bright) = (rgb(0x6e5328), rgb(0xb8893b), rgb(0xf0d48c));
    let (field_mid, field_edge) = (rgb(0x33271c), rgb(0x0b0806));
    let dark = rgb(0x120c07);
    let crimson = rgb(0xb8261c);
    // the light comes from the top left
    let light = (-0.6f32, -0.8f32);
    let slash = |x: f32, y: f32| -> f32 {
        // a bar from the top left to the bottom right, as a distance
        let (ax, ay) = (c - inner * 0.62, c - inner * 0.62);
        let (bx, by) = (c + inner * 0.62, c + inner * 0.62);
        let (px, py, vx, vy) = (x - ax, y - ay, bx - ax, by - ay);
        let h = clamp01((px * vx + py * vy) / (vx * vx + vy * vy));
        ((px - vx * h).powi(2) + (py - vy * h).powi(2)).sqrt() - n * 0.034
    };
    let mut out = vec![0u8; side * side * 4];
    for y in 0..side {
        for x in 0..side {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (fx - c, fy - c);
            let d = (dx * dx + dy * dy).sqrt();
            let cover = clamp01(outer - d + 0.5);
            if cover <= 0.0 {
                continue;
            }
            let (nx, ny) = if d > 0.0 {
                (dx / d, dy / d)
            } else {
                (0.0, 0.0)
            };
            let facing = nx * light.0 + ny * light.1;
            // the field: darker toward its edge and in the rim's shadow
            let t = clamp01(d / inner);
            let mut field = mix3(field_mid, field_edge, t.powf(1.6));
            let shadow = clamp01((d - (inner - n * 0.07)) / (n * 0.07));
            field = field.map(|v| v * (1.0 - 0.55 * shadow * (0.6 - 0.4 * facing)));
            // the rim: a rounded ridge, its upper left catching the light
            let u = clamp01((d - inner) / rim);
            let ridge = 1.0 - (2.0 * u - 1.0).abs();
            let lit = clamp01(0.5 + 0.5 * facing * (1.0 - 2.0 * u));
            let mut metal = mix3(gold_dim, gold, clamp01(ridge * 1.2));
            metal = mix3(metal, gold_bright, lit * ridge * 0.85);
            let lines =
                clamp01(1.2 - (d - inner).abs()).max(clamp01(1.2 - (d - outer + 0.6).abs()));
            metal = mix3(metal, dark, lines * 0.85);
            let k = clamp01(d - inner + 0.5);
            let mut col = mix3(field, metal, k);
            let mut a = cover;
            // the subject, on the field only
            if let Some((px, w, h)) = subject {
                let (ox, oy) = (c - w as f32 / 2.0, c - h as f32 / 2.0);
                let (sx, sy) = ((fx - ox).floor() as isize, (fy - oy).floor() as isize);
                if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                    let i = (sy as usize * w + sx as usize) * 4;
                    let sa = f32::from(px[i + 3]) / 255.0 * clamp01(inner - 1.0 - d + 0.5);
                    let sc = [
                        f32::from(px[i]) / 255.0,
                        f32::from(px[i + 1]) / 255.0,
                        f32::from(px[i + 2]) / 255.0,
                    ];
                    (col, a) = over(col, a, sc, sa);
                }
            }
            if crossed {
                let s = slash(fx, fy);
                let inside = clamp01(inner - 2.0 - d + 0.5);
                let outline = clamp01(0.5 - (s - n * 0.012)) * inside;
                (col, a) = over(col, a, dark, outline);
                let bar = clamp01(0.5 - s) * inside;
                (col, a) = over(col, a, crimson, bar);
            }
            let i = (y * side + x) * 4;
            for k in 0..3 {
                out[i + k] = (clamp01(col[k]) * 255.0).round() as u8;
            }
            out[i + 3] = (clamp01(a) * 255.0).round() as u8;
        }
    }
    out
}

/// The locked state: darkened and nearly grey.
pub(crate) fn locked(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks(4)
        .flat_map(|p| {
            let c = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            let luma = 0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2];
            let g = c.map(|v| (luma + (v - luma) * 0.12) * 0.55);
            let b = |v: f32| (clamp01(v) * 255.0).round() as u8;
            [b(g[0]), b(g[1]), b(g[2]), p[3]]
        })
        .collect()
}

/// `steam/achievements.vdf`: every achievement's API name, the names and
/// descriptions by language, whether Steam hides it and its icons, to
/// enter on the Steamworks site.
pub(crate) fn vdf(all: &[Achievement]) -> String {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = String::from(
        "// renethack's achievements for the Steamworks partner site (generated by\n\
         // `make achievement-icons` from client/achievements/achievements.toml):\n\
         // each one's API name, its name and description by language, whether\n\
         // it is hidden until earned, and its icons (achievements/, 64 px).\n\
         \"achievements\"\n{\n",
    );
    for a in all {
        out += &format!(
            "\t\"{steam}\"\n\t{{\n\
             \t\t\"id\"\t\"{id}\"\n\
             \t\t\"hidden\"\t\"{hidden}\"\n\
             \t\t\"icon\"\t\"achievements/{steam}.jpg\"\n\
             \t\t\"icon_gray\"\t\"achievements/{steam}_locked.jpg\"\n\
             \t\t\"name\"\n\t\t{{\n\t\t\t\"english\"\t\"{name}\"\n\t\t}}\n\
             \t\t\"desc\"\n\t\t{{\n\t\t\t\"english\"\t\"{desc}\"\n\t\t}}\n\
             \t}}\n",
            steam = a.steam,
            id = a.id,
            hidden = u8::from(a.hidden),
            name = q(&a.name_en),
            desc = q(&a.desc_en),
        );
    }
    out + "}\n"
}

// ---- the bake ----

/// What the stage shows of a subject.
enum Shown {
    /// A model; a portrait shows a figure's head and shoulders.
    Model(Model, bool),
    Numeral(Gd<Label3D>),
}

/// Where the picture of one subject is (as the item bake: framed loosely,
/// then tightly by the pixels drawn). After each picture the stage is
/// seen empty before the next subject comes, so that a late frame of the
/// last one is never taken for the next.
enum Phase {
    Next,
    Loose(Shown, Vector3, f32, Option<PackedByteArray>, u32),
    Tight(Shown, Option<PackedByteArray>, u32),
    Clear(u32),
}

/// The whole bake, one frame at a time.
pub struct Bake {
    stage: Stage,
    catalog: Rc<Catalog>,
    all: Vec<Achievement>,
    todo: Vec<usize>,
    phase: Phase,
    game_dir: PathBuf,
    steam_dir: PathBuf,
    pub written: usize,
    failed: Vec<String>,
}

fn global(path: &str) -> PathBuf {
    let p: PathBuf = ProjectSettings::singleton()
        .globalize_path(path)
        .to_string()
        .into();
    // "client/godot/../../steam": the repository's own path
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

impl Bake {
    pub fn new(parent: &mut Gd<Node>, catalog: Rc<Catalog>) -> Result<Bake, String> {
        let game_dir = global(GAME_DIR);
        let steam_dir = global(STEAM_DIR);
        for d in [&game_dir, &steam_dir.join("achievements")] {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let all = Achievements::built_in().all().to_vec();
        // RENETHACK_ACHIEVEMENTS_ONLY=id,id...: bake only these (while tuning)
        let only: Vec<String> = std::env::var("RENETHACK_ACHIEVEMENTS_ONLY")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let mut todo: Vec<usize> = (0..all.len())
            .filter(|&i| only.is_empty() || only.contains(&all[i].id))
            .collect();
        todo.reverse();
        Ok(Bake {
            stage: Stage::new(parent),
            catalog,
            all,
            todo,
            phase: Phase::Next,
            game_dir,
            steam_dir,
            written: 0,
            failed: Vec::new(),
        })
    }

    /// Achievements still to bake.
    pub fn left(&self) -> usize {
        self.todo.len() + usize::from(matches!(self.phase, Phase::Loose(..) | Phase::Tight(..)))
    }

    /// The stage is idle: nothing is being drawn, nothing left over.
    pub fn idle(&self) -> bool {
        matches!(self.phase, Phase::Next)
    }

    pub fn failures(&self) -> &[String] {
        &self.failed
    }

    /// Steam's `achievements.vdf`, for every achievement.
    pub fn write_vdf(&self) -> Result<(), String> {
        let path = self.steam_dir.join("achievements.vdf");
        std::fs::write(&path, vdf(&self.all)).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// One frame of work; true when a medallion was saved (or failed).
    pub fn tick(&mut self) -> Result<bool, String> {
        match std::mem::replace(&mut self.phase, Phase::Next) {
            Phase::Next => {
                let Some(&i) = self.todo.last() else {
                    return Ok(true);
                };
                let icon = self.all[i].icon.clone();
                let Some(subject) = icon.subject() else {
                    return Ok(self.fail("no subject"));
                };
                if let Some(glyph) = subject_glyph(subject) {
                    let image = icons::glyph_image(glyph, SUBJECT as usize, true);
                    return self.write(image).map(|()| true);
                }
                let steel = self.all[i].when.deepest.is_some();
                let shown = match self.show(subject, steel) {
                    Ok(s) => s,
                    Err(e) => return Ok(self.fail(&e)),
                };
                let (mid, loose) = match &shown {
                    Shown::Model(m, _) => {
                        let pts = icon_bake::corners(&m.node);
                        let (view, inv) =
                            (icon_bake::view_basis(), icon_bake::view_basis().inverse());
                        let (mut lo, mut hi) = (Vector3::splat(f32::MAX), Vector3::splat(f32::MIN));
                        for p in &pts {
                            let v = inv * *p;
                            lo = lo.coord_min(v);
                            hi = hi.coord_max(v);
                        }
                        if pts.is_empty() {
                            self.release(shown);
                            return Ok(self.fail("the model has no meshes"));
                        }
                        (
                            view * ((lo + hi) * 0.5),
                            (hi.x - lo.x).max(hi.y - lo.y) * 1.5 + 0.02,
                        )
                    }
                    Shown::Numeral(_) => (Vector3::ZERO, 3.0),
                };
                self.stage.frame(mid, loose * FILL);
                self.phase = Phase::Loose(shown, mid, loose, None, 0);
                Ok(false)
            }
            Phase::Loose(shown, mid, loose, last, n) => {
                let image = self.stage.grab().ok_or("no image")?;
                let image = match icon_bake::still(image, last.as_ref(), n) {
                    Ok(image) => image,
                    Err(data) => {
                        return self.wait(Phase::Loose(shown, mid, loose, Some(data), n + 1), n);
                    }
                };
                icon_bake::debug_save(&image, "loose");
                let Some((x0, y0, x1, y1)) = icon_bake::used(&image) else {
                    self.release(shown);
                    return Ok(self.fail("nothing drawn"));
                };
                // a portrait: the head and the shoulders
                let y1 = if matches!(shown, Shown::Model(_, true)) {
                    y0 + (y1 - y0) * PORTRAIT
                } else {
                    y1
                };
                let unit = loose / RENDER as f32;
                let half = RENDER as f32 * 0.5;
                let (cx, cy) = ((x0 + x1) * 0.5 - half, (y0 + y1) * 0.5 - half);
                let centre =
                    mid + icon_bake::view_basis() * Vector3::new(cx * unit, -cy * unit, 0.0);
                let extent = (x1 - x0).max(y1 - y0) * unit;
                self.stage.frame(centre, extent);
                self.phase = Phase::Tight(shown, None, 0);
                Ok(false)
            }
            Phase::Tight(shown, last, n) => {
                let image = self.stage.grab().ok_or("no image")?;
                let image = match icon_bake::still(image, last.as_ref(), n) {
                    Ok(image) => image,
                    Err(data) => return self.wait(Phase::Tight(shown, Some(data), n + 1), n),
                };
                icon_bake::debug_save(&image, "tight");
                let portrait = matches!(shown, Shown::Model(_, true));
                self.release(shown);
                let mut subject = icon_bake::finish(&image).ok_or("cannot finish the picture")?;
                if portrait {
                    fade_bottom(&mut subject);
                }
                self.write(Some(subject))?;
                self.phase = Phase::Clear(0);
                Ok(true)
            }
            Phase::Clear(n) => {
                let image = self.stage.grab().ok_or("no image")?;
                if icon_bake::used(&image).is_some() && n < STILL_FRAMES {
                    self.phase = Phase::Clear(n + 1);
                }
                Ok(false)
            }
        }
    }

    /// Put the subject on the stage (a numeral of steel for a depth).
    fn show(&mut self, subject: Subject, steel: bool) -> Result<Shown, String> {
        let cat = self.catalog.clone();
        let look = match subject {
            Subject::Object(appearance) => {
                let tile = cat
                    .object_tiles
                    .iter()
                    .find(|t| t.appearance == appearance)
                    .ok_or(format!("no object looks like {appearance:?}"))?;
                icon_bake::object_look(&self.stage.art, &cat, tile).0
            }
            Subject::Creature(name) => self.creature(&cat, name)?,
            Subject::Role(code) => {
                let role = cat
                    .roles
                    .iter()
                    .find(|r| r.code == code)
                    .ok_or(format!("no role {code}"))?;
                self.creature(&cat, role_monster(&role.name.to_lowercase()))?
            }
            Subject::Numeral(n) => return Ok(Shown::Numeral(self.numeral(n, steel))),
            Subject::Glyph(_) | Subject::Emblem(_) => return Err("not a model".into()),
        };
        let mut model = self.stage.art.take(&look);
        model.node.set_transform(Transform3D::IDENTITY);
        icon_bake::present(&mut model.node, false);
        // a person (a figure on the characters' rig) is seen as a portrait
        let person = self
            .stage
            .art
            .manifest()
            .model_at(look.art.model)
            .1
            .rig
            .is_some();
        Ok(Shown::Model(model, person))
    }

    /// A monster as its map art draws it, held still in its idle pose.
    fn creature(&self, cat: &Catalog, name: &str) -> Result<ModelLook, String> {
        let info = cat
            .monsters
            .iter()
            .find(|m| m.name == name)
            .ok_or(format!("no monster {name:?}"))?;
        let art = self.stage.art.manifest().monster(info, 0);
        Ok(ModelLook {
            art,
            tint: icon_bake::tint_color(art.tint, info.color),
            pose: Pose::Statue,
        })
    }

    /// A numeral in the title face, gold with a dark edge, facing the view.
    /// A numeral in the title face with a dark edge, facing the view: gold
    /// for a rank, steel for a depth.
    fn numeral(&mut self, n: u32, steel: bool) -> Gd<Label3D> {
        let mut label = Label3D::new_alloc();
        label.set_text(&n.to_string());
        label.set_font(&theme::font(Face::TitleBold));
        label.set_font_size(160);
        label.set_outline_size(22);
        label.set_pixel_size(0.01);
        label.set_modulate(if steel {
            Color::from_rgb(0.78, 0.84, 0.92)
        } else {
            Color::from_rgb(0.95, 0.8, 0.48)
        });
        label.set_outline_modulate(Color::from_rgb(0.12, 0.07, 0.03));
        label.set_billboard_mode(BillboardMode::ENABLED);
        self.stage.viewport.add_child(&label);
        label
    }

    fn release(&mut self, shown: Shown) {
        match shown {
            Shown::Model(m, _) => self.stage.art.give(m),
            Shown::Numeral(mut l) => l.queue_free(),
        }
    }

    fn wait(&mut self, phase: Phase, n: u32) -> Result<bool, String> {
        if n >= STILL_FRAMES {
            if let Phase::Loose(s, ..) | Phase::Tight(s, ..) = phase {
                self.release(s);
            }
            self.phase = Phase::Clear(0);
            return Ok(self.fail("the picture never held still"));
        }
        self.phase = phase;
        Ok(false)
    }

    fn fail(&mut self, why: &str) -> bool {
        if let Some(i) = self.todo.pop() {
            self.failed.push(format!("{}: {why}", self.all[i].id));
        }
        true
    }

    /// The medallion of the next achievement with `subject` on it, in both
    /// states, for the game and for Steam.
    fn write(&mut self, subject: Option<Gd<Image>>) -> Result<(), String> {
        let i = self.todo.pop().ok_or("no achievement")?;
        let a = self.all[i].clone();
        let pixels = subject.as_ref().and_then(fit).map(|mut s| {
            s.convert(Format::RGBA8);
            (
                s.get_data().to_vec(),
                s.get_width() as usize,
                s.get_height() as usize,
            )
        });
        let done = medallion(
            SIDE,
            pixels.as_ref().map(|(p, w, h)| (p.as_slice(), *w, *h)),
            a.icon.crossed,
        );
        for (data, suffix) in [(done.clone(), ""), (locked(&done), "_locked")] {
            let image = rgba_image(&data, SIDE)?;
            icon_bake::save(&image, &self.game_dir.join(format!("{}{suffix}.png", a.id)))?;
            let path = self
                .steam_dir
                .join("achievements")
                .join(format!("{}{suffix}.jpg", a.steam));
            save_jpg(&steam_icon(&data)?, &path)?;
        }
        self.written += 1;
        Ok(())
    }
}

impl Drop for Bake {
    fn drop(&mut self) {
        self.stage.viewport.queue_free();
    }
}

/// A portrait fades out at its bottom (it is cut there).
fn fade_bottom(image: &mut Gd<Image>) {
    let (w, h) = (image.get_width(), image.get_height());
    let Some(used) = Some(image.get_used_rect()).filter(|r| r.size.y > 0) else {
        return;
    };
    let bottom = used.position.y + used.size.y;
    let band = (used.size.y as f32 * 0.22).max(1.0);
    for y in 0..h {
        let t = ((bottom - y) as f32 / band).clamp(0.0, 1.0);
        if t >= 1.0 {
            continue;
        }
        for x in 0..w {
            let mut c = image.get_pixel(x, y);
            c.a *= t * t;
            image.set_pixel(x, y, c);
        }
    }
}

/// The subject cropped to what it shows and scaled so that its diagonal
/// spans SUBJECT_SPAN of the medallion's field: a tall figure fills the
/// field's height, a round thing most of its width.
fn fit(image: &Gd<Image>) -> Option<Gd<Image>> {
    let used = image.get_used_rect();
    if used.size.x <= 0 || used.size.y <= 0 {
        return None;
    }
    let mut out = image.get_region(used)?;
    let (w, h) = (used.size.x as f32, used.size.y as f32);
    let field = SIDE as f32 * (1.0 - 2.0 * 0.075) - 3.0;
    let k = field * SUBJECT_SPAN / (w * w + h * h).sqrt();
    let (nw, nh) = ((w * k).round() as i32, (h * k).round() as i32);
    out.resize_ex(nw.max(1), nh.max(1))
        .interpolation(Interpolation::LANCZOS)
        .done();
    Some(out)
}

fn rgba_image(data: &[u8], side: usize) -> Result<Gd<Image>, String> {
    Image::create_from_data(
        side as i32,
        side as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(data),
    )
    .ok_or_else(|| "cannot make an image".to_string())
}

/// Steam's icon: the medallion on the UI's darkest ink (a JPG has no
/// alpha), at 64 px.
fn steam_icon(rgba: &[u8]) -> Result<Gd<Image>, String> {
    let ink = rgb(0x0b0908);
    let rgb8: Vec<u8> = rgba
        .chunks(4)
        .flat_map(|p| {
            let a = f32::from(p[3]) / 255.0;
            let c = [p[0], p[1], p[2]].map(|v| f32::from(v) / 255.0);
            let (col, _) = over(ink, 1.0, c, a);
            col.map(|v| (clamp01(v) * 255.0).round() as u8)
        })
        .collect();
    let mut image = Image::create_from_data(
        SIDE as i32,
        SIDE as i32,
        false,
        Format::RGB8,
        &PackedByteArray::from(rgb8.as_slice()),
    )
    .ok_or("cannot make Steam's image")?;
    image
        .resize_ex(STEAM_SIDE, STEAM_SIDE)
        .interpolation(Interpolation::LANCZOS)
        .done();
    Ok(image)
}

fn save_jpg(image: &Gd<Image>, path: &Path) -> Result<(), String> {
    let err = image
        .save_jpg_ex(&path.to_string_lossy().to_string())
        .quality(0.92)
        .done();
    if err == godot::global::Error::OK {
        Ok(())
    } else {
        Err(format!("cannot save {}: {err:?}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(px: &[u8], side: usize, x: usize, y: usize) -> u8 {
        px[(y * side + x) * 4 + 3]
    }

    #[test]
    fn every_glyph_and_emblem_subject_is_drawn() {
        for a in Achievements::built_in().all() {
            match a.icon.subject() {
                Some(s @ (Subject::Glyph(_) | Subject::Emblem(_))) => {
                    assert!(subject_glyph(s).is_some(), "{}: {s:?}", a.id);
                }
                Some(_) => {}
                None => panic!("{}: no subject", a.id),
            }
        }
    }

    #[test]
    fn a_medallion_is_round_and_gold_rimmed() {
        let side = 64;
        let px = medallion(side, None, false);
        // the corners are clear, the middle is the dark field
        assert_eq!(alpha(&px, side, 0, 0), 0);
        assert_eq!(alpha(&px, side, 32, 32), 255);
        let mid = &px[(32 * side + 32) * 4..][..3];
        assert!(mid.iter().all(|&v| v < 80), "{mid:?}");
        // the rim, at the top left: gold (red over blue)
        let (x, y) = (12, 12);
        let rim = &px[(y * side + x) * 4..][..4];
        assert!(rim[3] > 200 && rim[0] > rim[2] + 40, "{rim:?}");
    }

    #[test]
    fn the_subject_sits_in_the_middle_and_a_cross_strikes_it() {
        let side = 64;
        let white = vec![255u8; 20 * 20 * 4];
        let px = medallion(side, Some((&white, 20, 20)), false);
        assert_eq!(&px[(32 * side + 32) * 4..][..4], &[255, 255, 255, 255]);
        let crossed = medallion(side, Some((&white, 20, 20)), true);
        let p = &crossed[(32 * side + 32) * 4..][..3];
        assert!(p[0] > 120 && p[1] < 80, "{p:?}");
    }

    #[test]
    fn a_locked_medallion_is_dark_and_grey() {
        let px = medallion(64, None, false);
        let l = locked(&px);
        let rim = &l[(12 * 64 + 12) * 4..][..4];
        let gold = &px[(12 * 64 + 12) * 4..][..4];
        assert_eq!(rim[3], gold[3]);
        assert!(rim[0] < gold[0] / 2, "{rim:?} {gold:?}");
        assert!(
            (i32::from(rim[0]) - i32::from(rim[2])).abs() < 12,
            "{rim:?}"
        );
    }

    #[test]
    fn the_vdf_names_every_achievement_once() {
        let all = Achievements::built_in();
        let text = vdf(all.all());
        for a in all.all() {
            assert_eq!(
                text.matches(&format!("\t\"{}\"\n", a.steam)).count(),
                1,
                "{}",
                a.steam
            );
        }
        assert!(text.contains("\"icon_gray\"\t\"achievements/ACH_BELL_locked.jpg\""));
        assert!(text.contains("\"english\"\t\"Ring My Bell\""));
        // braces balance
        assert_eq!(text.matches('{').count(), text.matches('}').count());
        // quotes inside a string are escaped
        let mut one = all.all()[0].clone();
        one.name_en = "Say \"hi\"".into();
        assert!(vdf(&[one]).contains("\"Say \\\"hi\\\"\""));
    }
}
