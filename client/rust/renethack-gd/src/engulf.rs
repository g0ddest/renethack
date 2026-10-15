//! The inside of what engulfed the hero, drawn round them while they are
//! swallowed (the engine shows nothing else then): a vortex's funnel of
//! its stuff whirling round them, an air elemental's gale, a purple worm's
//! wet gullet, the fold of a trapper's or a lurker's flesh, a jelly's goo.
//! Told by the engulfer the swallow glyphs name. One mesh and a light of
//! its own (a fire vortex's flames, an energy vortex's arcs flicker), so it
//! costs about nothing. What lies towards the camera, or far round them,
//! fades out.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    Mesh, MeshInstance3D, Node3D, OmniLight3D, Shader, ShaderMaterial, SphereMesh, SurfaceTool,
};
use godot::prelude::*;
use std::f32::consts::TAU;

const WIND_SHADER: &str = "res://shaders/engulf_wind.gdshader";
const MAW_SHADER: &str = "res://shaders/engulf_maw.gdshader";

/// What the inside of an engulfer looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inside {
    /// A vortex's funnel, of what it whirls.
    Funnel(Stuff),
    /// An air elemental's gale.
    Gale,
    /// A gullet or a fold of flesh, or goo.
    Maw(Flesh),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stuff {
    Fog,
    Dust,
    Ice,
    Energy,
    Steam,
    Fire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flesh {
    /// A purple worm's gullet, rings passing down it.
    Gullet,
    /// A trapper's or a lurker above's hide, folded round its prey.
    Hide,
    /// An ochre jelly's goo.
    Jelly,
    /// Juiblex's slime.
    Slime,
}

impl Inside {
    /// The inside of the monster named, one of NetHack's engulfers; None
    /// for any other (the map draws a fold of hide for that).
    pub fn of(name: &str) -> Option<Inside> {
        Some(match name {
            "fog cloud" => Inside::Funnel(Stuff::Fog),
            "dust vortex" => Inside::Funnel(Stuff::Dust),
            "ice vortex" => Inside::Funnel(Stuff::Ice),
            "energy vortex" => Inside::Funnel(Stuff::Energy),
            "steam vortex" => Inside::Funnel(Stuff::Steam),
            "fire vortex" => Inside::Funnel(Stuff::Fire),
            "air elemental" => Inside::Gale,
            "purple worm" => Inside::Maw(Flesh::Gullet),
            "trapper" | "lurker above" => Inside::Maw(Flesh::Hide),
            "ochre jelly" => Inside::Maw(Flesh::Jelly),
            "Juiblex" => Inside::Maw(Flesh::Slime),
            _ => return None,
        })
    }

    /// Its light: colour, energy, how much it flickers.
    fn light(self) -> (Color, f32, f32) {
        let rgb = Color::from_rgb;
        match self {
            Inside::Funnel(Stuff::Fire) => (rgb(1.0, 0.52, 0.18), 2.2, 0.35),
            Inside::Funnel(Stuff::Energy) => (rgb(0.45, 0.62, 1.0), 1.8, 0.6),
            Inside::Funnel(Stuff::Ice) => (rgb(0.62, 0.82, 1.0), 0.8, 0.0),
            Inside::Funnel(Stuff::Dust) => (rgb(0.85, 0.66, 0.45), 0.4, 0.0),
            Inside::Funnel(Stuff::Steam) => (rgb(0.92, 0.94, 0.97), 0.6, 0.0),
            Inside::Funnel(Stuff::Fog) => (rgb(0.75, 0.78, 0.82), 0.4, 0.0),
            Inside::Gale => (rgb(0.82, 0.9, 1.0), 0.6, 0.0),
            Inside::Maw(Flesh::Gullet) => (rgb(1.0, 0.62, 0.72), 2.2, 0.0),
            Inside::Maw(Flesh::Hide) => (rgb(1.0, 0.8, 0.62), 2.0, 0.0),
            Inside::Maw(Flesh::Jelly) => (rgb(1.0, 0.82, 0.45), 1.8, 0.0),
            Inside::Maw(Flesh::Slime) => (rgb(0.75, 1.0, 0.6), 2.0, 0.0),
        }
    }
}

/// The wind shader's parameters for a funnel or the gale: colour, core,
/// glow, bands, twist, speed, density, soft, streaks, specks.
struct Wind {
    color: Color,
    core: Color,
    glow: f32,
    bands: f32,
    twist: f32,
    speed: f32,
    density: f32,
    soft: f32,
    streaks: f32,
    specks: i32,
}

fn wind(inside: Inside) -> Wind {
    let rgb = Color::from_rgb;
    let base = Wind {
        color: rgb(0.6, 0.6, 0.6),
        core: rgb(1.0, 1.0, 1.0),
        glow: 0.0,
        bands: 3.0,
        twist: 1.4,
        speed: 0.6,
        density: 0.65,
        soft: 0.0,
        streaks: 0.0,
        specks: 0,
    };
    match inside {
        Inside::Funnel(Stuff::Fire) => Wind {
            color: rgb(1.0, 0.4, 0.08),
            core: rgb(1.0, 0.86, 0.4),
            glow: 1.6,
            speed: 0.8,
            density: 0.95,
            specks: 4,
            ..base
        },
        Inside::Funnel(Stuff::Energy) => Wind {
            color: rgb(0.22, 0.42, 1.0),
            core: rgb(0.85, 0.95, 1.0),
            glow: 1.3,
            bands: 4.0,
            speed: 1.0,
            density: 0.8,
            specks: 3,
            ..base
        },
        Inside::Funnel(Stuff::Ice) => Wind {
            color: rgb(0.6, 0.8, 0.95),
            glow: 0.2,
            density: 0.8,
            specks: 2,
            ..base
        },
        Inside::Funnel(Stuff::Dust) => Wind {
            color: rgb(0.52, 0.4, 0.25),
            density: 0.95,
            specks: 1,
            ..base
        },
        Inside::Funnel(Stuff::Steam) => Wind {
            color: rgb(0.86, 0.88, 0.91),
            glow: 0.1,
            bands: 2.0,
            speed: 0.45,
            density: 0.7,
            ..base
        },
        Inside::Funnel(Stuff::Fog) => Wind {
            color: rgb(0.55, 0.58, 0.62),
            bands: 0.0,
            speed: 0.2,
            density: 0.6,
            soft: 1.0,
            ..base
        },
        Inside::Gale | Inside::Maw(_) => Wind {
            color: rgb(0.8, 0.87, 0.95),
            speed: 1.2,
            density: 0.6,
            streaks: 1.0,
            ..base
        },
    }
}

/// The maw shader's parameters: flesh, ridge, deep, rings, folds, goo
/// (and a hide's ribs, `RIBS`).
fn maw(flesh: Flesh) -> (Color, Color, Color, f32, f32, f32) {
    let rgb = Color::from_rgb;
    match flesh {
        Flesh::Gullet => (
            rgb(0.45, 0.12, 0.3),
            rgb(0.8, 0.36, 0.55),
            rgb(0.12, 0.02, 0.07),
            1.0,
            0.35,
            0.0,
        ),
        Flesh::Hide => (
            rgb(0.55, 0.25, 0.18),
            rgb(0.88, 0.52, 0.4),
            rgb(0.14, 0.05, 0.04),
            0.0,
            1.0,
            0.0,
        ),
        Flesh::Jelly => (
            rgb(0.65, 0.44, 0.1),
            rgb(0.95, 0.72, 0.28),
            rgb(0.28, 0.15, 0.02),
            0.0,
            0.5,
            1.0,
        ),
        Flesh::Slime => (
            rgb(0.24, 0.42, 0.1),
            rgb(0.58, 0.78, 0.26),
            rgb(0.05, 0.12, 0.02),
            0.3,
            0.6,
            0.7,
        ),
    }
}

/// The funnel round the hero: across at its foot and its top, how tall.
const FUNNEL: (f32, f32, f32) = (0.85, 2.1, 2.8);
/// The gale's, broader.
const GALE: (f32, f32, f32) = (1.0, 2.5, 2.6);
/// The floor of its stuff under them, out to here (whole under them, over
/// the cell the level drew there).
const GROUND: f32 = 2.0;
/// The bands of muscle round the inside of a hide wrapped round the hero.
const RIBS: f32 = 14.0;
/// A maw's floor lies just over the level's.
const MAW_FLOOR: f32 = 0.02;
/// The fold of flesh or the blob round the hero: across, and how tall.
const MAW: (f32, f32) = (3.0, 3.6);
/// A purple worm's gullet: a tube lying north to south through the hero
/// (the camera looks down it), half as wide and half as high as it is
/// round, and how long.
const GULLET: (f32, f32, f32) = (2.4, 2.0, 16.0);

/// A mesh built of grids of points, each quad wound to face the way its
/// points' normals do (Godot's front faces wind clockwise).
#[derive(Default)]
struct Grids {
    pos: Vec<Vector3>,
    nrm: Vec<Vector3>,
    uv: Vec<Vector2>,
    idx: Vec<i32>,
}

impl Grids {
    /// `rows` rings of `sides` + 1 points (the last one the first again,
    /// for the texture), `at(t, u)` giving each: its place, its normal,
    /// its texture's place (t and u from 0 to 1, u round the ring).
    fn grid(
        &mut self,
        rows: usize,
        sides: usize,
        at: impl Fn(f32, f32) -> (Vector3, Vector3, Vector2),
    ) {
        let base = self.pos.len() as i32;
        for i in 0..rows {
            for j in 0..=sides {
                let (p, n, uv) = at(i as f32 / (rows - 1) as f32, j as f32 / sides as f32);
                self.pos.push(p);
                self.nrm.push(n);
                self.uv.push(uv);
            }
        }
        let w = sides as i32 + 1;
        for i in 0..rows as i32 - 1 {
            for j in 0..sides as i32 {
                let a = base + i * w + j;
                self.tri(a, a + 1, a + w);
                self.tri(a + 1, a + w + 1, a + w);
            }
        }
    }

    fn tri(&mut self, a: i32, b: i32, c: i32) {
        let p = |i: i32| self.pos[i as usize];
        let n = self.nrm[a as usize] + self.nrm[b as usize] + self.nrm[c as usize];
        if (p(b) - p(a)).cross(p(c) - p(a)).dot(n) > 0.0 {
            self.idx.extend([a, c, b]);
        } else {
            self.idx.extend([a, b, c]);
        }
    }

    fn mesh(self) -> Gd<Mesh> {
        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        for k in 0..self.pos.len() {
            st.set_normal(self.nrm[k]);
            st.set_uv(self.uv[k]);
            st.add_vertex(self.pos[k]);
        }
        for i in self.idx {
            st.add_index(i);
        }
        st.commit()
            .map_or_else(|| SphereMesh::new_gd().upcast(), |m| m.upcast())
    }
}

/// A funnel round y (`low` across at the floor, `high` at its top `tall`
/// up, open at both ends) and a disc of floor under it out to `GROUND`,
/// the disc's u past 2 and its v from its middle out (the wind shader
/// tells them apart by it, and the near wall, seen from outside, from the
/// far one seen from inside by the way it faces).
fn funnel_mesh((low, high, tall): (f32, f32, f32)) -> Gd<Mesh> {
    let mut g = Grids::default();
    g.grid(16, 48, |t, u| {
        let a = u * TAU;
        let r = low + (high - low) * t.powf(1.3);
        let slope = (high - low) * 1.3 * t.max(0.01).powf(0.3) / tall;
        let p = Vector3::new(r * a.cos(), tall * t, r * a.sin());
        let n = Vector3::new(a.cos(), -slope, a.sin()).normalized();
        (p, n, Vector2::new(u, t))
    });
    g.grid(8, 48, |t, u| {
        let a = u * TAU;
        let r = GROUND * t;
        let p = Vector3::new(r * a.cos(), 0.02, r * a.sin());
        (p, Vector3::UP, Vector2::new(2.0 + u, t))
    });
    g.mesh()
}

/// A tube of an oval `wide` and `high` (half sizes) lying along z,
/// `length` long, its middle at the origin: its u runs round it from its
/// floor, its v along it from the north.
fn tube_mesh((wide, high, length): (f32, f32, f32)) -> Gd<Mesh> {
    let mut g = Grids::default();
    g.grid(64, 40, |t, u| {
        let a = u * TAU;
        let (s, c) = a.sin_cos();
        let p = Vector3::new(wide * s, -high * c, length * (t - 0.5));
        let n = Vector3::new(s / wide, -c / high, 0.0).normalized();
        (p, n, Vector2::new(u, t))
    });
    g.mesh()
}

/// A maw's mesh and how it lies: its middle's height over the floor, the
/// rings a turn of its texture's v holds, how far round the hero and how
/// high it shows before the dark.
struct MawShape {
    lift: f32,
    ring_k: f32,
    reach: f32,
    tall: f32,
}

const ROUND_MAW: MawShape = MawShape {
    lift: MAW.1 / 2.0,
    ring_k: 30.0,
    reach: 3.0,
    tall: 3.2,
};
const TUBE_MAW: MawShape = MawShape {
    lift: GULLET.1,
    ring_k: 110.0,
    reach: 5.5,
    tall: 3.2,
};

/// The engulfed view's nodes: the funnel (vortices) and the gale, the
/// fold of flesh or blob and the gullet, and their light.
pub struct Engulfed {
    funnel: Gd<MeshInstance3D>,
    gale: Gd<MeshInstance3D>,
    maw: Gd<MeshInstance3D>,
    gullet: Gd<MeshInstance3D>,
    wind_mat: Option<Gd<ShaderMaterial>>,
    maw_mat: Option<Gd<ShaderMaterial>>,
    light: Gd<OmniLight3D>,
    shown: Option<Inside>,
    clock: f64,
}

impl Engulfed {
    pub fn new(root: &mut Gd<Node3D>) -> Engulfed {
        let material = |path: &str| {
            godot::tools::try_load::<Shader>(path).ok().map(|shader| {
                let mut m = ShaderMaterial::new_gd();
                m.set_shader(&shader);
                m
            })
        };
        let (wind_mat, maw_mat) = (material(WIND_SHADER), material(MAW_SHADER));
        let mut part = |mesh: Gd<Mesh>, mat: &Option<Gd<ShaderMaterial>>| {
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_mesh(&mesh);
            if let Some(m) = mat {
                mi.set_material_override(m);
            }
            mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            mi.set_visible(false);
            root.add_child(&mi);
            mi
        };
        let funnel = part(funnel_mesh(FUNNEL), &wind_mat);
        let gale = part(funnel_mesh(GALE), &wind_mat);
        let mut sphere = SphereMesh::new_gd();
        sphere.set_radius(MAW.0);
        sphere.set_height(MAW.1);
        sphere.set_radial_segments(48);
        sphere.set_rings(24);
        let maw = part(sphere.upcast(), &maw_mat);
        let gullet = part(tube_mesh(GULLET), &maw_mat);
        let mut light = OmniLight3D::new_alloc();
        light.set_param(Param::RANGE, 6.0);
        light.set_param(Param::ATTENUATION, 0.9);
        light.set_shadow(false);
        light.set_visible(false);
        root.add_child(&light);
        Engulfed {
            funnel,
            gale,
            maw,
            gullet,
            wind_mat,
            maw_mat,
            light,
            shown: None,
            clock: 0.0,
        }
    }

    /// Drawn round the hero standing at `at` (on the floor) while `inside`
    /// engulfs them; none of it otherwise.
    pub fn show(&mut self, inside: Option<Inside>, at: Vector3, delta: f64) {
        if inside != self.shown {
            self.dress(inside);
            self.shown = inside;
        }
        let Some(inside) = inside else {
            return;
        };
        self.clock += delta;
        self.funnel.set_position(at);
        self.gale.set_position(at);
        self.maw
            .set_position(at + Vector3::new(0.0, MAW_FLOOR + ROUND_MAW.lift, 0.0));
        self.gullet
            .set_position(at + Vector3::new(0.0, MAW_FLOOR + TUBE_MAW.lift, 0.0));
        let (_, energy, flicker) = inside.light();
        let t = self.clock as f32;
        let wobble = (t * 13.0).sin() * 0.5 + (t * 23.7 + 1.3).sin() * 0.3 + (t * 7.1).sin() * 0.2;
        self.light
            .set_param(Param::ENERGY, energy * (1.0 + flicker * wobble));
        self.light.set_position(at + Vector3::new(0.0, 1.4, 0.0));
    }

    /// What it draws now (self-tests).
    pub fn shown(&self) -> Option<Inside> {
        self.shown
    }

    fn dress(&mut self, inside: Option<Inside>) {
        let gullet = inside == Some(Inside::Maw(Flesh::Gullet));
        self.funnel
            .set_visible(matches!(inside, Some(Inside::Funnel(_))));
        self.gale.set_visible(inside == Some(Inside::Gale));
        self.maw
            .set_visible(matches!(inside, Some(Inside::Maw(_))) && !gullet);
        self.gullet.set_visible(gullet);
        self.light.set_visible(inside.is_some());
        let Some(inside) = inside else {
            return;
        };
        let (color, _, _) = inside.light();
        self.light.set_color(color);
        match inside {
            Inside::Maw(flesh) => {
                if let Some(m) = self.maw_mat.as_mut() {
                    let (f, r, d, rings, folds, goo) = maw(flesh);
                    let shape = if gullet { &TUBE_MAW } else { &ROUND_MAW };
                    m.set_shader_parameter("flesh", &f.to_variant());
                    m.set_shader_parameter("ridge", &r.to_variant());
                    m.set_shader_parameter("deep", &d.to_variant());
                    m.set_shader_parameter("rings", &rings.to_variant());
                    m.set_shader_parameter("folds", &folds.to_variant());
                    m.set_shader_parameter("goo", &goo.to_variant());
                    let ribs = if flesh == Flesh::Hide { RIBS } else { 0.0 };
                    m.set_shader_parameter("ribs", &ribs.to_variant());
                    m.set_shader_parameter("floor_y", &(-shape.lift).to_variant());
                    m.set_shader_parameter("ring_k", &shape.ring_k.to_variant());
                    m.set_shader_parameter("reach", &shape.reach.to_variant());
                    m.set_shader_parameter("tall", &shape.tall.to_variant());
                }
            }
            _ => {
                if let Some(m) = self.wind_mat.as_mut() {
                    let w = wind(inside);
                    m.set_shader_parameter("color", &w.color.to_variant());
                    m.set_shader_parameter("core", &w.core.to_variant());
                    m.set_shader_parameter("glow", &w.glow.to_variant());
                    m.set_shader_parameter("bands", &w.bands.to_variant());
                    m.set_shader_parameter("twist", &w.twist.to_variant());
                    m.set_shader_parameter("speed", &w.speed.to_variant());
                    m.set_shader_parameter("density", &w.density.to_variant());
                    m.set_shader_parameter("soft", &w.soft.to_variant());
                    m.set_shader_parameter("streaks", &w.streaks.to_variant());
                    m.set_shader_parameter("specks", &w.specks.to_variant());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engulfer_has_an_inside_of_its_own_kind() {
        for (name, inside) in [
            ("fire vortex", Inside::Funnel(Stuff::Fire)),
            ("fog cloud", Inside::Funnel(Stuff::Fog)),
            ("air elemental", Inside::Gale),
            ("purple worm", Inside::Maw(Flesh::Gullet)),
            ("lurker above", Inside::Maw(Flesh::Hide)),
            ("trapper", Inside::Maw(Flesh::Hide)),
            ("ochre jelly", Inside::Maw(Flesh::Jelly)),
            ("Juiblex", Inside::Maw(Flesh::Slime)),
        ] {
            assert_eq!(Inside::of(name), Some(inside), "{name}");
        }
        assert_eq!(Inside::of("newt"), None);
    }
}
