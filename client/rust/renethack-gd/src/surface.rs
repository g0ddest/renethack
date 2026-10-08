//! The map's surfaces: one shader (`shaders/map_surface.gdshader`) for all
//! the stone, earth and wood of the level, set up per manifest material and
//! role, and the fog of war it reads: a texture of the level's cells that
//! says what the hero sees now, what they remember and what lies near
//! anything known. What is seen now is worked out here from the glyphs
//! alone (the engine sends no vision): a lit floor is in view, a dark one
//! is remembered, and the rest takes the floor beside it.

use std::collections::HashMap;

use godot::classes::image::Format;
use godot::classes::{Image, ImageTexture, Material, Shader, ShaderMaterial, Texture2D};
use godot::prelude::*;
use nh_art::ArtManifest;
use nh_world::{COLNO, ROWNO};

const SHADER: &str = "res://shaders/map_surface.gdshader";
const ART_ROOT: &str = "res://art/";
/// The fine normal laid over every surface, close up.
const DETAIL: &str = "rock";
/// A cell fades in or out of view over this many seconds.
const REVEAL_SECS: f32 = 0.25;
/// Cells from anything known over which the darkness closes in.
const FADE_CELLS: f32 = 3.0;

/// How the shader treats a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Features, doors and the like.
    Prop,
    /// Floors: wet patches and grime at the foot of walls.
    Floor,
    /// Wall bodies: masonry sides under a rock top; they dither away
    /// between the eye and the hero.
    Wall,
    /// Door leaves and frames: they dither too.
    Door,
    /// Caps and plinths.
    Trim,
    /// A wall cut down in front of open ground: masonry with a broken top.
    Ruin,
    /// The rock the level is cut into: broken up, cut in facets, dithers.
    Rock,
    /// The ground under the level: rock fading into darkness.
    Void,
}

/// (manifest material, brightness %, role).
type Key = (usize, u8, Role);

pub struct Surfaces {
    shader: Option<Gd<Shader>>,
    fow: Gd<ImageTexture>,
    noise: Gd<Texture2D>,
    detail: Option<Gd<Texture2D>>,
    textures: HashMap<String, Option<Gd<Texture2D>>>,
    materials: HashMap<Key, Gd<Material>>,
    top: Option<usize>,
}

impl Surfaces {
    pub fn new(fow: &Fow, manifest: &ArtManifest) -> Surfaces {
        let shader = godot::tools::try_load::<Shader>(SHADER).ok();
        if shader.is_none() {
            godot_warn!("renethack: {SHADER} is missing; the map is drawn with plain materials");
        }
        let fow = ImageTexture::create_from_image(&fow.image).unwrap_or_else(ImageTexture::new_gd);
        let mut s = Surfaces {
            shader,
            fow,
            noise: noise_texture(),
            detail: None,
            textures: HashMap::new(),
            materials: HashMap::new(),
            top: manifest.material("bedrock"),
        };
        let detail = manifest
            .material(DETAIL)
            .and_then(|m| manifest.material_at(m).1.normal_path());
        s.detail = detail.and_then(|p| s.texture(&p));
        s
    }

    /// Every texture the manifest's materials read (all the branches'),
    /// as the map loads them.
    pub fn texture_paths(manifest: &ArtManifest) -> Vec<String> {
        let mut paths: Vec<String> = manifest
            .materials()
            .flat_map(|(_, _, spec)| [spec.albedo_path(), spec.normal_path(), spec.arm_path()])
            .flatten()
            .map(|p| format!("{ART_ROOT}{p}"))
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }

    fn texture(&mut self, path: &str) -> Option<Gd<Texture2D>> {
        if let Some(t) = self.textures.get(path) {
            return t.clone();
        }
        let t = godot::tools::try_load::<Texture2D>(&format!("{ART_ROOT}{path}")).ok();
        self.textures.insert(path.to_string(), t.clone());
        t
    }

    /// The tiling noise the map's shaders read.
    pub fn noise(&self) -> Gd<Texture2D> {
        self.noise.clone()
    }

    /// The fog of war's texture, for shaders of the map's own.
    pub fn fow_texture(&self) -> Gd<ImageTexture> {
        self.fow.clone()
    }

    /// The fog of war changed: the shader sees the new texture.
    pub fn show_fow(&mut self, fow: &Fow) {
        self.fow.update(&fow.image);
    }

    /// The material of a manifest material in a role (None without the
    /// shader: the caller falls back to the art library's).
    pub fn material(
        &mut self,
        manifest: &ArtManifest,
        m: usize,
        shade: u8,
        role: Role,
    ) -> Option<Gd<Material>> {
        let shader = self.shader.clone()?;
        let key = (m, shade, role);
        if let Some(mat) = self.materials.get(&key) {
            return Some(mat.clone());
        }
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        let k = f32::from(shade) / 100.0;
        let side = manifest.material_at(m).1.clone();
        // a wall's top is the rock it is cut from
        let top = match (role, self.top) {
            (Role::Wall, Some(t)) => manifest.material_at(t).1.clone(),
            _ => side.clone(),
        };
        for (prefix, spec) in [("side", &side), ("top", &top)] {
            let c = spec.albedo();
            let color = Color::from_rgb(c[0] * k, c[1] * k, c[2] * k);
            mat.set_shader_parameter(&format!("{prefix}_color"), &color.to_variant());
            mat.set_shader_parameter(&format!("{prefix}_scale"), &spec.uv_scale.to_variant());
            let maps = [
                ("albedo", spec.albedo_path()),
                ("normal", spec.normal_path()),
                ("orm", spec.arm_path()),
            ];
            for (name, path) in maps {
                if let Some(t) = path.and_then(|p| self.texture(&p)) {
                    mat.set_shader_parameter(&format!("{prefix}_{name}"), &t.to_variant());
                }
            }
        }
        let textured = side.textures.is_some();
        let set = |mat: &mut Gd<ShaderMaterial>, name: &str, v: f32| {
            mat.set_shader_parameter(name, &v.to_variant());
        };
        set(&mut mat, "normal_strength", side.normal_scale);
        if !textured {
            set(&mut mat, "roughness", side.roughness.unwrap_or(0.8));
            set(&mut mat, "macro", 0.3);
        } else if let Some(r) = side.roughness {
            set(&mut mat, "roughness_bias", r - 0.8);
        }
        set(&mut mat, "metallic", side.metallic.unwrap_or(0.0));
        if let Some(d) = &self.detail {
            mat.set_shader_parameter("detail_normal", &d.to_variant());
        }
        mat.set_shader_parameter("fow_tex", &self.fow.to_variant());
        mat.set_shader_parameter("noise_tex", &self.noise.to_variant());
        // the rock the level is cut from is rock, never blocks
        if Some(m) == self.top {
            set(&mut mat, "natural", 1.0);
        }
        // whatever stands between the eye and the hero thins out around
        // them; floors and the ground below never do
        if !matches!(role, Role::Floor | Role::Void) {
            set(&mut mat, "occlude", 1.0);
            set(&mut mat, "solid_top", 10.0);
        }
        // nothing on the level is pure black: a faint cold light of its own
        let lift = match role {
            Role::Void => Color::from_rgb(0.0, 0.0, 0.0),
            Role::Rock => Color::from_rgb(0.007, 0.007, 0.0085),
            _ => Color::from_rgb(0.004, 0.004, 0.005),
        };
        mat.set_shader_parameter("lift", &lift.to_variant());
        match role {
            Role::Prop | Role::Trim => set(&mut mat, "glow", 0.02),
            Role::Ruin => {
                set(&mut mat, "glow", 0.02);
                set(&mut mat, "displace", 0.03);
            }
            Role::Floor => {
                set(&mut mat, "grime", 1.0);
                set(&mut mat, "wet", 0.8);
            }
            Role::Wall | Role::Door => {
                set(
                    &mut mat,
                    "natural_top",
                    if role == Role::Wall { 1.0 } else { 0.0 },
                );
                set(&mut mat, "glow", 0.03);
            }
            Role::Rock => {
                set(&mut mat, "chunks", 0.12);
                set(&mut mat, "facets", 1.0);
                // broken in chunks along its joints, lit as it is cut;
                // its faces are mottled rock in the relief of the
                // texture's own cracks; its tops are plain rock and lie
                // in the dark over them, a tall block's all but black, a
                // ledge cut down in front of open ground dim (a ledge at
                // the hero's feet, not a slab as lit as the floor); and
                // it mirrors next to nothing: under a light held this
                // close a sheen makes cloth of it
                set(&mut mat, "natural", 0.7);
                set(&mut mat, "natural_relief", 0.25);
                set(&mut mat, "natural_top", 1.0);
                set(&mut mat, "top_shade", 0.12);
                set(&mut mat, "ledge_shade", 0.6);
                set(&mut mat, "specular", 0.12);
                set(&mut mat, "detail_strength", 0.4);
                set(&mut mat, "detail_scale", 3.0);
                set(&mut mat, "glow", 0.05);
                set(&mut mat, "memory_glow", 0.04);
            }
            Role::Void => {
                set(&mut mat, "void_fade", 1.0);
                // no sheen on it either: by its own colour alone, dark
                set(&mut mat, "specular", 0.0);
                set(&mut mat, "memory", 0.0);
                set(&mut mat, "glow", 0.02);
                set(&mut mat, "macro", 1.4);
                // rock in any branch (the main dungeon's is the manifest's
                // own bedrock), cracked where the branch's floors are
                set(&mut mat, "natural", 1.0);
                set(&mut mat, "fissures", 0.6);
            }
        }
        let mat = mat.upcast::<Material>();
        self.materials.insert(key, mat.clone());
        Some(mat)
    }
}

/// A tiling fractal noise, 256 px, about eight features across: the
/// surface shader's noise in its fragments.
fn noise_texture() -> Gd<Texture2D> {
    use godot::classes::fast_noise_lite::{FractalType, NoiseType};
    use godot::classes::{FastNoiseLite, NoiseTexture2D};
    let mut noise = FastNoiseLite::new_gd();
    noise.set_noise_type(NoiseType::PERLIN);
    noise.set_frequency(1.0 / 32.0);
    noise.set_fractal_type(FractalType::FBM);
    noise.set_fractal_octaves(5);
    let mut tex = NoiseTexture2D::new_gd();
    tex.set_width(256);
    tex.set_height(256);
    tex.set_seamless(true);
    tex.set_generate_mipmaps(true);
    tex.set_normalize(true);
    tex.set_noise(&noise);
    tex.upcast()
}

/// What the hero knows of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sight {
    #[default]
    Unknown,
    Remembered,
    Seen,
}

/// A cell as the fog of war takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FowCell {
    pub sight: Sight,
    pub wall: bool,
}

/// The fog of war's texture: the target per cell and the value shown, which
/// moves towards it (a cell comes into view over `REVEAL_SECS`).
pub struct Fow {
    target: Vec<[f32; 4]>,
    shown: Vec<[f32; 4]>,
    image: Gd<Image>,
    moving: bool,
}

fn index(x: i32, y: i32) -> Option<usize> {
    ((0..COLNO).contains(&x) && (0..ROWNO).contains(&y)).then(|| (y * COLNO + x) as usize)
}

impl Fow {
    pub fn new() -> Fow {
        let n = (COLNO * ROWNO) as usize;
        let image =
            Image::create_empty(COLNO, ROWNO, false, Format::RGBA8).unwrap_or_else(Image::new_gd);
        Fow {
            target: vec![[0.0; 4]; n],
            shown: vec![[0.0; 4]; n],
            image,
            moving: true,
        }
    }

    /// The cells as they are now; `snap` shows them at once (a new level).
    pub fn set(&mut self, cells: &[FowCell], snap: bool) {
        self.target = targets(cells);
        if snap {
            self.shown.clone_from(&self.target);
        }
        self.moving = true;
    }

    /// Move what is shown towards the target; true when the texture changed.
    pub fn advance(&mut self, delta: f32) -> bool {
        if !self.moving {
            return false;
        }
        let step = (delta / REVEAL_SECS).max(0.0);
        let mut moving = false;
        for (s, t) in self.shown.iter_mut().zip(&self.target) {
            for c in 0..4 {
                let d = t[c] - s[c];
                s[c] = if d.abs() <= step {
                    t[c]
                } else {
                    s[c] + step * d.signum()
                };
                moving |= s[c] != t[c];
            }
        }
        let mut bytes = PackedByteArray::new();
        bytes.resize(self.shown.len() * 4);
        let out = bytes.as_mut_slice();
        for (i, s) in self.shown.iter().enumerate() {
            for c in 0..4 {
                out[i * 4 + c] = (s[c].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        self.image
            .set_data(COLNO, ROWNO, false, Format::RGBA8, &bytes);
        self.moving = moving;
        true
    }
}

impl Default for Fow {
    fn default() -> Fow {
        Fow::new()
    }
}

/// Per cell: (seen, known, wall, near something known), each 0..1.
fn targets(cells: &[FowCell]) -> Vec<[f32; 4]> {
    let known: Vec<bool> = cells.iter().map(|c| c.sight != Sight::Unknown).collect();
    let near = nearness(&known);
    cells
        .iter()
        .zip(near)
        .map(|(c, near)| {
            let (seen, known) = match c.sight {
                Sight::Seen => (1.0, 1.0),
                Sight::Remembered => (0.0, 1.0),
                Sight::Unknown => (0.0, 0.0),
            };
            [seen, known, if c.wall { 1.0 } else { 0.0 }, near]
        })
        .collect()
}

/// 1 on a known cell, falling to 0 `FADE_CELLS` away from any.
fn nearness(known: &[bool]) -> Vec<f32> {
    let reach = FADE_CELLS.ceil() as i32 + 1;
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .map(|(x, y)| {
            let mut best = f32::MAX;
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    if index(x + dx, y + dy).is_some_and(|i| known[i]) {
                        best = best.min(((dx * dx + dy * dy) as f32).sqrt());
                    }
                }
            }
            (1.0 - best / FADE_CELLS).clamp(0.0, 1.0)
        })
        .collect()
}

/// A cell of the map as the sight rules take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Seen {
    /// Nothing known.
    #[default]
    Nothing,
    /// The rock around the known level (drawn, never seen as such).
    Rock,
    /// A lit room floor: in view.
    LitFloor,
    /// A floor out of view (a dark room, remembered).
    DarkFloor,
    /// A lit corridor: in view.
    LitCorridor,
    /// Anything else known: in view when beside what is.
    Other,
    /// A monster in view.
    Creature,
}

/// Who sees what: a lit floor and a lit corridor are in view, and so is
/// what the hero stands next to and a monster shown in sight; a wall,
/// door or feature is in view when a floor beside it is; the rock around
/// the level takes the most of its neighbours. `walls` marks the cells of
/// walls (grime at their foot).
pub fn sight(cells: &[Seen], walls: &[bool], hero: Option<(i32, i32)>) -> Vec<FowCell> {
    let at = |x: i32, y: i32| index(x, y).map_or(Seen::Nothing, |i| cells[i]);
    let near_hero =
        |x: i32, y: i32| hero.is_some_and(|(hx, hy)| (hx - x).abs() <= 1 && (hy - y).abs() <= 1);
    let around =
        |x: i32, y: i32| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
    let base: Vec<Sight> = (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .map(|(x, y)| match at(x, y) {
            Seen::Nothing | Seen::Rock => Sight::Unknown,
            Seen::LitFloor | Seen::LitCorridor | Seen::Creature => Sight::Seen,
            _ if near_hero(x, y) => Sight::Seen,
            Seen::DarkFloor => Sight::Remembered,
            Seen::Other => {
                let lit = around(x, y).any(|(a, b)| at(a, b) == Seen::LitFloor);
                if lit { Sight::Seen } else { Sight::Remembered }
            }
        })
        .collect();
    (0..ROWNO)
        .flat_map(|y| (0..COLNO).map(move |x| (x, y)))
        .enumerate()
        .map(|(i, (x, y))| {
            let sight = if cells[i] == Seen::Rock {
                // rock is as seen as what it borders
                around(x, y)
                    .filter_map(|(a, b)| index(a, b))
                    .map(|j| base[j])
                    .max_by_key(|s| match s {
                        Sight::Unknown => 0,
                        Sight::Remembered => 1,
                        Sight::Seen => 2,
                    })
                    .unwrap_or_default()
            } else {
                base[i]
            };
            FowCell {
                sight,
                wall: walls[i],
            }
        })
        .collect()
}

/// The index of cell (x, y) in the per-cell arrays above.
pub fn cell_index(x: i32, y: i32) -> Option<usize> {
    index(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> (Vec<Seen>, Vec<bool>) {
        let n = (COLNO * ROWNO) as usize;
        (vec![Seen::Nothing; n], vec![false; n])
    }

    fn put(cells: &mut [Seen], x: i32, y: i32, s: Seen) {
        cells[cell_index(x, y).unwrap()] = s;
    }

    fn get(fow: &[FowCell], x: i32, y: i32) -> Sight {
        fow[cell_index(x, y).unwrap()].sight
    }

    #[test]
    fn a_lit_room_is_in_view_with_its_walls_and_a_dark_one_is_remembered() {
        let (mut cells, walls) = field();
        // a lit room 10..12 x 5..6 with walls round it, and a dark one
        for x in 9..=13 {
            for y in 4..=7 {
                let inside = (10..=12).contains(&x) && (5..=6).contains(&y);
                put(
                    &mut cells,
                    x,
                    y,
                    if inside { Seen::LitFloor } else { Seen::Other },
                );
                put(
                    &mut cells,
                    x + 20,
                    y,
                    if inside { Seen::DarkFloor } else { Seen::Other },
                );
            }
        }
        put(&mut cells, 8, 5, Seen::Rock);
        let fow = sight(&cells, &walls, Some((11, 5)));
        assert_eq!(get(&fow, 11, 6), Sight::Seen);
        assert_eq!(get(&fow, 9, 4), Sight::Seen, "a corner beside a lit floor");
        assert_eq!(get(&fow, 31, 5), Sight::Remembered);
        assert_eq!(get(&fow, 29, 4), Sight::Remembered);
        assert_eq!(get(&fow, 8, 5), Sight::Seen, "rock by a wall in view");
        assert_eq!(get(&fow, 50, 5), Sight::Unknown);
        // in a dark room the hero sees what is next to them
        let fow = sight(&cells, &walls, Some((31, 5)));
        assert_eq!(get(&fow, 30, 5), Sight::Seen);
        assert_eq!(get(&fow, 32, 6), Sight::Seen);
        assert_eq!(get(&fow, 29, 7), Sight::Remembered);
    }

    #[test]
    fn darkness_closes_in_away_from_what_is_known() {
        let n = (COLNO * ROWNO) as usize;
        let mut cells = vec![FowCell::default(); n];
        cells[cell_index(40, 10).unwrap()].sight = Sight::Remembered;
        let t = targets(&cells);
        let near = |x, y| t[cell_index(x, y).unwrap()][3];
        assert_eq!(near(40, 10), 1.0);
        assert!(near(41, 10) > near(42, 10) && near(42, 10) > 0.0);
        assert_eq!(near(44, 10), 0.0);
        assert_eq!(t[cell_index(40, 10).unwrap()][..2], [0.0, 1.0]);
    }
}
