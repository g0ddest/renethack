//! The creatures drawn in code that a game meets most (the sightings of
//! a soak: the kitten first), put together from the smooth shapes of
//! `organic.rs` instead of stacked primitives: a shape for each material
//! of each moving part, so a creature is a handful of nodes. Like the rest
//! of the kit, a body is one unit tall, stands on y = 0, faces +z and
//! names the pivots its animations move (Head, Tail, the legs).

use super::*;
use crate::organic::felines::{self, Coat, Part};
use crate::organic::more;
use crate::organic::weather;
use crate::organic::{self, Organic};
use godot::classes::particle_process_material::{EmissionShape, Parameter};
use godot::classes::{
    GpuParticles3D, Gradient, GradientTexture1D, ParticleProcessMaterial, QuadMesh,
};

const ONE: [f32; 3] = [1.0; 3];
const FLAT: [f32; 3] = [0.0; 3];

/// A creature's legs by their pivots: front left, front right, back left,
/// back right (the walk swings them in pairs).
pub(super) const LEGS: [&str; 4] = ["LegFL", "LegFR", "LegBL", "LegBR"];

/// A six-legged creature's middle legs, left and right.
const MIDDLE_LEGS: [&str; 2] = ["LegML", "LegMR"];

/// The shader a weather's funnel, puffs and wind are drawn with, and the
/// torches' flame shader a fire elemental's flames are.
const WIND_SHADER: &str = "res://shaders/vortex.gdshader";
const FLAME_SHADER: &str = "res://shaders/flame.gdshader";

/// A fire elemental's tongues of flame: where each stands (its foot), how
/// wide and how tall it is; its legs, its body, its arms, its head (a
/// flame's picture is brightest at its foot: each stands in the one under
/// it, and where they overlap it burns hotter).
const TONGUES: [([f32; 3], f32, f32); 6] = [
    ([-0.11, 0.0, 0.03], 0.36, 0.55),
    ([0.11, 0.0, -0.03], 0.36, 0.55),
    ([0.0, 0.16, 0.0], 0.56, 0.85),
    ([-0.3, 0.36, 0.0], 0.28, 0.44),
    ([0.3, 0.36, 0.0], 0.28, 0.44),
    ([0.0, 0.52, 0.02], 0.34, 0.48),
];

/// How a weather's funnel, puffs or wind are drawn (the weather shader's
/// parameters): its colour, its own light, the bands round it, the turns
/// they make from its foot to its top, how fast they flow, how dense it
/// is, whether it is a cloud, soft at its rims, and the turns a second it
/// spins.
#[derive(Clone, Copy)]
struct Wind {
    color: Color,
    glow: f32,
    bands: f32,
    twist: f32,
    speed: f32,
    density: f32,
    soft: bool,
    spin: f32,
}

/// What a vortex or an elemental is made of, by its model's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Weather {
    Dust,
    Fog,
    Ice,
    Energy,
    Steam,
    Fire,
    /// An air elemental.
    Air,
    /// A fire elemental.
    Flame,
}

impl Weather {
    pub(super) fn of(shape: &str) -> Option<Weather> {
        Some(match shape {
            "dust" => Weather::Dust,
            "fog" => Weather::Fog,
            "ice" => Weather::Ice,
            "energy" => Weather::Energy,
            "steam" => Weather::Steam,
            "fire" => Weather::Fire,
            "air" => Weather::Air,
            "flame" => Weather::Flame,
            _ => return None,
        })
    }

    /// Whether it gives light of its own (drawn unshaded, its motes
    /// added to what is behind them).
    fn glows(self) -> bool {
        matches!(self, Weather::Energy | Weather::Fire | Weather::Flame)
    }

    /// How its motes drift: up for heat and steam, down for ice.
    fn rise(self) -> f32 {
        match self {
            Weather::Fire | Weather::Flame | Weather::Steam | Weather::Energy => 0.35,
            Weather::Ice => -0.25,
            Weather::Dust | Weather::Fog | Weather::Air => 0.05,
        }
    }
}

/// The big cats, by their model's shape in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BigCat {
    Jaguar,
    Lynx,
    Panther,
    Tiger,
    Displacer,
}

impl BigCat {
    pub(super) fn of(shape: &str) -> Option<BigCat> {
        Some(match shape {
            "jaguar" => BigCat::Jaguar,
            "lynx" => BigCat::Lynx,
            "panther" => BigCat::Panther,
            "tiger" => BigCat::Tiger,
            "displacer" => BigCat::Displacer,
            _ => return None,
        })
    }

    /// The marks of its coat (a panther's and a displacer beast's are
    /// plain: their colour is their material).
    fn coat(self) -> Option<Coat> {
        match self {
            BigCat::Jaguar => Some(Coat::Rosettes),
            BigCat::Tiger => Some(Coat::Stripes),
            BigCat::Lynx => Some(Coat::Lynx),
            BigCat::Panther | BigCat::Displacer => None,
        }
    }
}

fn shape(o: Organic) -> MeshKey {
    MeshKey::Organic(o)
}

impl Kit<'_> {
    /// A cat: a lean body, a round head with pointed ears and eyes that
    /// catch the light, slim legs with a hock behind, a long tail carried
    /// high.
    pub(super) fn cat_body(&mut self, root: &mut Gd<Node3D>) {
        // (the pivots are the root's own: the animations name them so)
        self.skin(root, shape(Organic::CatBody), FLAT, FLAT, ONE);
        let mut head = self.pivot(root, "Head", [0.0, 0.75, 0.44]);
        // a kitten's big head: it reads as a cat from the camera's height
        head.set_scale(Vector3::splat(1.3));
        self.skin(&mut head, shape(Organic::CatHead), FLAT, FLAT, ONE);
        self.eyes(&mut head, [0.0, 0.028, 0.104], 0.052, 0.02, true);
        let nose = self
            .art
            .flat(Color::from_rgb(0.62, 0.36, 0.36), Finish::Matte);
        self.part(
            &mut head,
            sphere(0.014),
            &nose,
            [0.0, -0.03, 0.163],
            FLAT,
            [1.3, 0.8, 1.0],
        );
        let mut tail = self.pivot(root, "Tail", [0.0, 0.58, -0.42]);
        self.skin(&mut tail, shape(Organic::CatTail), FLAT, FLAT, ONE);
        for (i, name) in LEGS.into_iter().enumerate() {
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let (leg, at) = if i < 2 {
                (Organic::CatForeleg, organic::CAT_SHOULDER)
            } else {
                (Organic::CatHindleg, organic::CAT_HIP)
            };
            let mut pivot = self.pivot(root, name, [side * at[0], at[1], at[2]]);
            self.skin(&mut pivot, shape(leg), FLAT, FLAT, ONE);
        }
    }

    /// A big cat: heavier than a housecat, its head broad, its legs thick
    /// on broad paws, the marks of its coat on it (a tiger's stripes, a
    /// jaguar's rosettes, a lynx's tufts and ruff); a displacer beast with
    /// six legs and a tentacle from each shoulder.
    pub(super) fn big_cat(&mut self, root: &mut Gd<Node3D>, cat: BigCat) {
        let lynx = cat == BigCat::Lynx;
        let coat = cat.coat();
        self.skin(root, shape(Organic::BigCatBody), FLAT, FLAT, ONE);
        self.marks(root, coat, Part::Body);
        let mut head = self.pivot(root, "Head", felines::HEAD_AT);
        self.skin(
            &mut head,
            shape(Organic::BigCatHead { lynx }),
            FLAT,
            FLAT,
            ONE,
        );
        self.marks(&mut head, coat, Part::Head);
        self.eyes(&mut head, [0.0, 0.025, 0.12], 0.07, 0.024, true);
        let nose = self
            .art
            .flat(Color::from_rgb(0.2, 0.13, 0.12), Finish::Matte);
        self.part(
            &mut head,
            sphere(0.022),
            &nose,
            [0.0, -0.03, 0.225],
            FLAT,
            [1.4, 0.8, 1.0],
        );
        let mut tail = self.pivot(root, "Tail", felines::TAIL_AT);
        self.skin(
            &mut tail,
            shape(Organic::BigCatTail { lynx }),
            FLAT,
            FLAT,
            ONE,
        );
        self.marks(&mut tail, coat, Part::Tail);
        let mut legs: Vec<(&str, bool, [f32; 3])> = LEGS
            .iter()
            .enumerate()
            .map(|(i, &name)| {
                if i < 2 {
                    (name, false, felines::SHOULDER)
                } else {
                    (name, true, felines::HIP)
                }
            })
            .collect();
        if cat == BigCat::Displacer {
            legs.extend(
                MIDDLE_LEGS
                    .iter()
                    .map(|&name| (name, false, felines::MIDDLE)),
            );
        }
        for (name, hind, at) in legs {
            let side = if name.ends_with('L') { -1.0 } else { 1.0 };
            let mut leg = self.pivot(root, name, [side * at[0], at[1], at[2]]);
            self.skin(
                &mut leg,
                shape(Organic::BigCatLeg { hind }),
                FLAT,
                FLAT,
                ONE,
            );
        }
        if cat == BigCat::Displacer {
            for (right, name) in [(false, "TentL"), (true, "TentR")] {
                let [x, y, z] = felines::TENTACLE_AT;
                let x = if right { x } else { -x };
                let mut tentacle = self.pivot(root, name, [x, y, z]);
                self.skin(
                    &mut tentacle,
                    shape(Organic::Tentacle { right }),
                    FLAT,
                    FLAT,
                    ONE,
                );
            }
        }
    }

    /// A cockatrice: a yellow rooster, red-combed, on scaled legs, with a
    /// serpent's tail; or its chick, round, with a short one.
    pub(super) fn cockatrice(&mut self, root: &mut Gd<Node3D>, chick: bool) {
        use more::Part::*;
        let red = self
            .art
            .flat(Color::from_rgb(0.75, 0.08, 0.06), Finish::Glossy);
        let horn = self
            .art
            .flat(Color::from_rgb(0.95, 0.68, 0.18), Finish::Glossy);
        let scales = self
            .art
            .flat(Color::from_rgb(0.42, 0.45, 0.14), Finish::Glossy);
        let (body, head, leg, tail) = if chick {
            (ChickBody, ChickHead, ChickLeg, ChickTail)
        } else {
            (CockBody, CockHead, CockLeg, CockTail)
        };
        let (head_at, tail_at, hip) = if chick {
            (more::CHICK_HEAD, more::CHICK_TAIL, more::CHICK_HIP)
        } else {
            (more::COCK_HEAD, more::COCK_TAIL, more::COCK_HIP)
        };
        self.skin(root, shape(Organic::More(body)), FLAT, FLAT, ONE);
        let mut pivot = self.pivot(root, "Head", head_at);
        self.skin(&mut pivot, shape(Organic::More(head)), FLAT, FLAT, ONE);
        if chick {
            self.eyes(&mut pivot, [0.0, 0.03, 0.095], 0.055, 0.022, false);
        } else {
            self.part(
                &mut pivot,
                shape(Organic::More(CockComb)),
                &red,
                FLAT,
                FLAT,
                ONE,
            );
            self.part(
                &mut pivot,
                shape(Organic::More(CockBeak)),
                &horn,
                FLAT,
                FLAT,
                ONE,
            );
            self.eyes(&mut pivot, [0.0, 0.02, 0.05], 0.045, 0.016, false);
        }
        let mut pivot = self.pivot(root, "Tail", tail_at);
        self.part(
            &mut pivot,
            shape(Organic::More(tail)),
            &scales,
            FLAT,
            FLAT,
            ONE,
        );
        for (name, side) in [("LegL", -1.0), ("LegR", 1.0)] {
            let mut pivot = self.pivot(root, name, [side * hip[0], hip[1], hip[2]]);
            self.part(
                &mut pivot,
                shape(Organic::More(leg)),
                &horn,
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    /// A shrieker: a stalk flaring to a funnel, dark down its throat.
    pub(super) fn shrieker(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(
            &mut body,
            shape(Organic::More(more::Part::Shrieker)),
            FLAT,
            FLAT,
            ONE,
        );
        self.skin(
            &mut body,
            shape(Organic::More(more::Part::ShriekerBell)),
            FLAT,
            FLAT,
            ONE,
        );
        let throat = self
            .art
            .flat(Color::from_rgb(0.08, 0.02, 0.1), Finish::Matte);
        self.part(
            &mut body,
            shape(Organic::More(more::Part::ShriekerThroat)),
            &throat,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A violet fungus: a cluster of caps, tendrils curling out over the
    /// floor to sticky ends.
    pub(super) fn violet_fungus(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(
            &mut body,
            shape(Organic::More(more::Part::Violet)),
            FLAT,
            FLAT,
            ONE,
        );
        let tips = self.dark();
        self.part(
            &mut body,
            shape(Organic::More(more::Part::VioletTips)),
            &tips,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A gelatinous cube: a quivering cube of clear jelly, the bones and
    /// the metal of what it has engulfed hanging in it.
    pub(super) fn cube(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        let bone = self.bone();
        self.part(
            &mut body,
            shape(Organic::More(more::Part::CubeBones)),
            &bone,
            FLAT,
            FLAT,
            ONE,
        );
        let metal = self
            .art
            .flat(Color::from_rgb(0.74, 0.66, 0.44), Finish::Glossy);
        self.part(
            &mut body,
            shape(Organic::More(more::Part::CubeLoot)),
            &metal,
            FLAT,
            FLAT,
            ONE,
        );
        self.skin(
            &mut body,
            shape(Organic::More(more::Part::Cube)),
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A vortex or an elemental: weather turning about its middle, its
    /// funnel, puffs or wind seen through and banded by the weather shader
    /// (which spins it, the bands flowing round and up it), what it
    /// carries and motes of its stuff whirled round with it (the Body
    /// pivot spins); a fire elemental is a figure of the torches' flames
    /// instead, standing still while its embers whirl.
    pub(super) fn weather(&mut self, root: &mut Gd<Node3D>, w: Weather) {
        use weather::Part::*;
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        let shape_of = |p| MeshKey::Organic(Organic::Weather(p));
        if w == Weather::Flame {
            let mut figure = self.pivot(root, "Figure", [0.0; 3]);
            self.flames(&mut figure);
            self.motes(&mut body, w);
            return;
        }
        let tint = self.tint;
        let pale = |k: f32| {
            Color::from_rgb(
                tint.r + (1.0 - tint.r) * k,
                tint.g + (1.0 - tint.g) * k,
                tint.b + (1.0 - tint.b) * k,
            )
        };
        let wind = |color, glow, bands, speed| Wind {
            color,
            glow,
            bands,
            twist: 1.5,
            speed,
            density: 0.6,
            soft: false,
            spin: 1.0 / WAVE_PERIOD,
        };
        let look = match w {
            Weather::Fire => wind(Color::from_rgb(1.0, 0.5, 0.12), 2.0, 3.0, 0.7),
            Weather::Energy => wind(pale(0.2), 2.2, 4.0, 0.8),
            Weather::Dust => Wind {
                density: 0.75,
                ..wind(tint, 0.0, 3.0, 0.5)
            },
            Weather::Ice => wind(pale(0.3), 0.25, 3.0, 0.5),
            Weather::Steam => Wind {
                density: 0.5,
                ..wind(pale(0.55), 0.0, 2.0, 0.35)
            },
            Weather::Fog => Wind {
                density: 0.3,
                soft: true,
                spin: 0.1,
                ..wind(pale(0.2), 0.0, 0.0, 0.2)
            },
            Weather::Air | Weather::Flame => wind(pale(0.2), 0.0, 3.0, 0.6),
        };
        let main = match w {
            Weather::Fog => Fog,
            Weather::Air => Gale,
            _ => Twister,
        };
        // the wind spins in its shader, as the Body does, its bounds still
        // (as the footprint self-test measures them, whatever the turn)
        let mut still = self.pivot(root, "Wind", [0.0; 3]);
        let mesh = self.art.mesh(shape_of(main));
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&mesh);
        mi.set_material_override(&self.wind(look));
        mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        // (its swaying axis turns out past them)
        mi.set_extra_cull_margin(0.3);
        still.add_child(&mi);
        // stones in dust, shards in ice, a bright rope of energy
        let carried = match w {
            Weather::Dust => Some((Debris, self.dark())),
            Weather::Ice => Some((Debris, self.art.flat(pale(0.75), Finish::Glossy))),
            Weather::Energy => Some((
                Core,
                self.art
                    .flat(Color::from_rgba(0.85, 0.94, 1.0, 0.85), Finish::Flat),
            )),
            _ => None,
        };
        if let Some((part, mat)) = carried {
            self.part(&mut body, shape_of(part), &mat, FLAT, FLAT, ONE);
        }
        self.motes(&mut body, w);
    }

    /// The material of a weather's funnel, puffs or wind: the weather
    /// shader's (a ghost's see-through finish without it, still).
    fn wind(&mut self, look: Wind) -> Gd<Material> {
        let Ok(shader) = godot::tools::try_load::<Shader>(WIND_SHADER) else {
            return self.art.flat(look.color, Finish::Ghost);
        };
        let mut m = ShaderMaterial::new_gd();
        m.set_shader(&shader);
        m.set_shader_parameter("color", &look.color.to_variant());
        m.set_shader_parameter("glow", &look.glow.to_variant());
        m.set_shader_parameter("bands", &look.bands.to_variant());
        m.set_shader_parameter("twist", &look.twist.to_variant());
        m.set_shader_parameter("speed", &look.speed.to_variant());
        m.set_shader_parameter("density", &look.density.to_variant());
        m.set_shader_parameter("soft", &f32::from(u8::from(look.soft)).to_variant());
        m.set_shader_parameter("spin", &look.spin.to_variant());
        m.upcast()
    }

    /// A fire elemental's flames, the torches' own (their shader and the
    /// pictures of their flame): a tongue for each leg, its body, each arm
    /// and its head, each flickering in its own time.
    fn flames(&mut self, figure: &mut Gd<Node3D>) {
        let Ok(shader) = godot::tools::try_load::<Shader>(FLAME_SHADER) else {
            return;
        };
        let mut m = ShaderMaterial::new_gd();
        m.set_shader(&shader);
        if let Ok(book) = godot::tools::try_load::<Texture2D>(crate::map_view::FLAME_BOOK) {
            m.set_shader_parameter("flipbook", &book.to_variant());
            m.set_shader_parameter("has_flipbook", &true.to_variant());
        }
        m.set_shader_parameter("energy", &0.75f32.to_variant());
        // (the picture's flame is cut off flat at its foot)
        m.set_shader_parameter("foot", &0.3f32.to_variant());
        for (i, &(at, w, h)) in TONGUES.iter().enumerate() {
            let mut quad = QuadMesh::new_gd();
            quad.set_size(Vector2::new(w, h));
            // the picture's flame rises from just above its foot
            quad.set_center_offset(Vector3::new(0.0, h * 0.42, 0.0));
            let mut mi = MeshInstance3D::new_alloc();
            mi.set_mesh(&quad);
            mi.set_material_override(&m);
            mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            mi.set_instance_shader_parameter("phase", &(i as f32 * 0.37).to_variant());
            mi.set_position(Vector3::from_array(at));
            figure.add_child(&mi);
        }
    }

    /// Motes of a weather's stuff, whirled round it: emitted where it
    /// turns (in its own space, so they turn with it) and drifting up or
    /// down, as an ember of a torch rises (the torches' particles, so no
    /// new shader is made for them).
    fn motes(&mut self, body: &mut Gd<Node3D>, w: Weather) {
        let mut m = ParticleProcessMaterial::new_gd();
        m.set_emission_shape(EmissionShape::SPHERE);
        m.set_emission_sphere_radius(0.32);
        m.set_direction(Vector3::UP);
        m.set_spread(30.0);
        m.set_param_min(Parameter::INITIAL_LINEAR_VELOCITY, 0.15);
        m.set_param_max(Parameter::INITIAL_LINEAR_VELOCITY, 0.5);
        m.set_gravity(Vector3::new(0.0, w.rise(), 0.0));
        m.set_param_min(Parameter::SCALE, 0.6);
        m.set_param_max(Parameter::SCALE, 1.4);
        m.set_turbulence_enabled(true);
        m.set_turbulence_noise_strength(0.6);
        m.set_turbulence_noise_scale(1.5);
        m.set_color_ramp(&fading(self.tint, if w.glows() { 3.0 } else { 1.0 }));
        let mut p = GpuParticles3D::new_alloc();
        p.set_name("Motes");
        p.set_process_material(&m);
        p.set_amount(if w == Weather::Fog { 14 } else { 32 });
        p.set_lifetime(1.6);
        p.set_randomness_ratio(0.5);
        p.set_use_local_coordinates(true);
        let mut quad = QuadMesh::new_gd();
        let size = if matches!(w, Weather::Fog | Weather::Steam) {
            0.18
        } else {
            0.08
        };
        quad.set_size(Vector2::new(size, size));
        p.set_draw_pass_mesh(0, &quad);
        p.set_material_override(&mote_material(w.glows()));
        p.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        p.set_visibility_aabb(Aabb::new(
            Vector3::new(-0.8, -0.2, -0.8),
            Vector3::new(1.6, 1.8, 1.6),
        ));
        p.set_position(Vector3::new(0.0, 0.25, 0.0));
        body.add_child(&p);
    }

    /// The dark marks of a coat on a part of a big cat (none without one).
    fn marks(&mut self, parent: &mut Gd<Node3D>, coat: Option<Coat>, part: Part) {
        let Some(coat) = coat else {
            return;
        };
        let ink = if coat.black_on(part) {
            self.art
                .flat(Color::from_rgb(0.05, 0.04, 0.035), Finish::Matte)
        } else {
            self.dark()
        };
        self.part(
            parent,
            shape(Organic::Coat { coat, part }),
            &ink,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A newt, a gecko, a crocodile: a low flat body slung between legs
    /// out to the sides and bent at the elbow, a wedge head, a long tail
    /// that drags.
    pub(super) fn lizard_body(&mut self, root: &mut Gd<Node3D>) {
        self.skin(root, shape(Organic::LizardBody), FLAT, FLAT, ONE);
        let mut head = self.pivot(root, "Head", [0.0, 0.155, 0.44]);
        self.skin(&mut head, shape(Organic::LizardHead), FLAT, FLAT, ONE);
        self.eyes(&mut head, [0.0, 0.045, 0.06], 0.062, 0.022, false);
        let mut tail = self.pivot(root, "Tail", [0.0, 0.13, -0.38]);
        self.skin(&mut tail, shape(Organic::LizardTail), FLAT, FLAT, ONE);
        for (i, name) in LEGS.into_iter().enumerate() {
            let (front, right) = (i < 2, i % 2 == 1);
            let x = if right { 0.14 } else { -0.14 };
            let z = if front { 0.26 } else { -0.24 };
            let mut leg = self.pivot(root, name, [x, organic::LIZARD_HIP, z]);
            self.skin(
                &mut leg,
                shape(Organic::LizardLeg { front, right }),
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    /// An ant: head, humped middle, a waist of two knots, the great
    /// gaster; six jointed legs from the middle, elbowed feelers, jaws.
    pub(super) fn ant_body(&mut self, root: &mut Gd<Node3D>, grid: bool) {
        let y = organic::ANT_Y;
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::AntBody), FLAT, FLAT, ONE);
        self.eyes(&mut body, [0.0, y + 0.07, 0.29], 0.085, 0.03, grid);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::AntFeelers),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
        self.six_legs(root, false);
    }

    /// Six legs, three a side, in the groups the walk swings (LegsL,
    /// LegsR).
    fn six_legs(&mut self, root: &mut Gd<Node3D>, beetle: bool) {
        let dark = self.dark();
        for (right, name) in [(false, "LegsL"), (true, "LegsR")] {
            let mut group = self.pivot(root, name, [0.0, organic::legs_y(beetle), 0.0]);
            self.part(
                &mut group,
                shape(Organic::Legs { beetle, right }),
                &dark,
                FLAT,
                FLAT,
                ONE,
            );
        }
    }

    /// A beetle: domed wing cases with their seam, a small head with
    /// jaws, six legs under the shell.
    pub(super) fn beetle_body(&mut self, root: &mut Gd<Node3D>) {
        let y = organic::BEETLE_Y;
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::BeetleBody), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::BeetleJaws),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
        self.eyes(&mut body, [0.0, y + 0.04, 0.53], 0.075, 0.025, false);
        self.six_legs(root, true);
    }

    /// A long worm: one thick ringed body raised at the front, its round
    /// mouth set with teeth.
    pub(super) fn worm_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::WormBody), FLAT, FLAT, ONE);
        let dark = self.dark();
        let (y, z, r) = organic::WORM_MOUTH;
        self.part(
            &mut body,
            cylinder(r, r, 0.035),
            &dark,
            [0.0, y, z],
            [90.0, 0.0, 0.0],
            ONE,
        );
        let bone = self.bone();
        self.part(&mut body, shape(Organic::WormTeeth), &bone, FLAT, FLAT, ONE);
    }

    /// A lichen: a flat rosette of crinkled lobes on the floor, little cups
    /// among them.
    pub(super) fn lichen(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::LichenLobes), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::LichenCups),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A mold: a soft mound of furry tufts, spores like dust on them.
    pub(super) fn mold(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::MoldMound), FLAT, FLAT, ONE);
        let glint = self.glint();
        self.part(
            &mut body,
            shape(Organic::MoldSpecks),
            &glint,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A blob, a jelly, a pudding: a sagging dome of slime, a darker
    /// heart inside, bubbles caught in it.
    pub(super) fn blob_body(&mut self, root: &mut Gd<Node3D>) {
        if self.shape.as_deref() == Some("cube") {
            self.cube(root);
            return;
        }
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::Blob), FLAT, FLAT, ONE);
        let heart = self.dark();
        self.part(
            &mut body,
            sphere(0.22),
            &heart,
            [0.05, 0.35, 0.05],
            FLAT,
            [1.0, 0.8, 1.0],
        );
        let glass = self
            .art
            .flat(Color::from_rgba(1.0, 1.0, 1.0, 0.5), Finish::Glass);
        self.part(
            &mut body,
            shape(Organic::BlobBubbles),
            &glass,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A floating eye: a great eyeball turned up to look at whoever looks
    /// down at it (the camera sees its iris whichever way it faces), the
    /// iris wide and its colour, red veins on the white, three tendrils
    /// hanging below.
    pub(super) fn eye_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        let mut ball = self.pivot(&mut body, "Ball", [0.0, 0.5, 0.0]);
        ball.set_rotation_degrees(Vector3::new(-55.0, 0.0, 0.0));
        let white = self
            .art
            .flat(Color::from_rgb(0.86, 0.8, 0.74), Finish::Glossy);
        self.part(&mut ball, sphere(organic::EYE_R), &white, FLAT, FLAT, ONE);
        // the iris rises from the white like a cornea, the pupil from it
        let iris = self.art.flat(self.tint, Finish::Glossy);
        self.part(
            &mut ball,
            dome(0.3),
            &iris,
            [0.0, 0.0, 0.294],
            [90.0, 0.0, 0.0],
            [1.0, 0.47, 1.0],
        );
        let pupil = self.eye();
        self.part(
            &mut ball,
            dome(0.13),
            &pupil,
            [0.0, 0.0, 0.405],
            [90.0, 0.0, 0.0],
            [1.0, 0.31, 1.0],
        );
        let glint = self.art.flat(Color::from_rgb(1.0, 1.0, 1.0), Finish::Glow);
        self.part(
            &mut ball,
            sphere(0.03),
            &glint,
            [0.09, 0.1, 0.43],
            FLAT,
            ONE,
        );
        let vein = self
            .art
            .flat(Color::from_rgb(0.62, 0.1, 0.08), Finish::Matte);
        self.part(&mut ball, shape(Organic::EyeVeins), &vein, FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::EyeTendrils),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }

    /// A gas spore: a grey puffball floating, covered in knobs, thin
    /// threads trailing.
    pub(super) fn spore_body(&mut self, root: &mut Gd<Node3D>) {
        let mut body = self.pivot(root, "Body", [0.0; 3]);
        self.skin(&mut body, shape(Organic::SporeBall), FLAT, FLAT, ONE);
        let dark = self.dark();
        self.part(
            &mut body,
            shape(Organic::SporeThreads),
            &dark,
            FLAT,
            FLAT,
            ONE,
        );
    }
}

/// What a body built here moves when idle besides what `proc_waves` moves
/// for its kind: a displacer beast's tentacles weave.
pub(super) fn idle_waves(kind: Proc, shape: Option<&str>) -> Vec<Wave> {
    match (kind, shape.and_then(BigCat::of)) {
        (Proc::Beast, Some(BigCat::Displacer)) => vec![
            Wave::Rock("TentL", Vector3::RIGHT, 14.0, 0.0),
            Wave::Rock("TentR", Vector3::RIGHT, 14.0, 0.5),
            Wave::Rock("TentL", Vector3::BACK, 8.0, 0.25),
            Wave::Rock("TentR", Vector3::BACK, 8.0, 0.75),
        ],
        _ => Vec::new(),
    }
}

/// A mote's colour over its life: `c`, brighter by `hdr` for what glows,
/// fading out in its own colour.
fn fading(c: Color, hdr: f32) -> Gd<GradientTexture1D> {
    let mut g = Gradient::new_gd();
    let hot = Color::from_rgba(c.r * hdr, c.g * hdr, c.b * hdr, 0.9);
    let mid = Color::from_rgba(c.r * hdr * 0.7, c.g * hdr * 0.7, c.b * hdr * 0.7, 0.6);
    let end = Color::from_rgba(c.r * 0.4, c.g * 0.4, c.b * 0.4, 0.0);
    g.set_offsets(&PackedFloat32Array::from(&[0.0, 0.5, 1.0][..]));
    g.set_colors(&PackedColorArray::from(&[hot, mid, end][..]));
    let mut t = GradientTexture1D::new_gd();
    t.set_gradient(&g);
    t.set_use_hdr(true);
    t
}

/// The material a mote is drawn with, as the map's own particles are: the
/// soft disc facing the camera in the mote's colour, added to what is
/// behind it if it glows, else shaded like the dust in the air.
fn mote_material(glows: bool) -> Gd<Material> {
    use godot::classes::base_material_3d::{
        BillboardMode, BlendMode, Flags, ShadingMode, TextureParam, Transparency,
    };
    // (a texture of its own for each model built: few are, being pooled,
    // and none may outlive the engine in a static)
    let tex = crate::vfx::soft_texture();
    let mut m = StandardMaterial3D::new_gd();
    m.set_billboard_mode(BillboardMode::PARTICLES);
    m.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
    m.set_texture(TextureParam::ALBEDO, &tex);
    m.set_transparency(Transparency::ALPHA);
    if glows {
        m.set_blend_mode(BlendMode::ADD);
        m.set_shading_mode(ShadingMode::UNSHADED);
    }
    m.set_flag(Flags::DISABLE_FOG, glows);
    m.upcast()
}

/// Seconds a walking body takes for one step of each leg.
pub(super) const WALK_PERIOD: f32 = 0.6;

/// The walk of a body built here: its legs swung in pairs, the tail
/// swaying; none for the bodies that hop (or have no legs).
pub(super) fn walk_waves(kind: Proc, shape: Option<&str>) -> Vec<Wave> {
    let x = Vector3::RIGHT;
    let y = Vector3::UP;
    let cat = shape.and_then(BigCat::of);
    match (kind, shape) {
        // a trot: the legs swing fore and aft, a diagonal pair together
        // (a displacer beast's middle legs a quarter step behind the
        // front ones of their side, a wave down each flank)
        (Proc::Beast, Some(s)) if s == "cat" || cat.is_some() => {
            let mut waves = vec![
                Wave::Rock(LEGS[0], x, 26.0, 0.0),
                Wave::Rock(LEGS[3], x, 26.0, 0.0),
                Wave::Rock(LEGS[1], x, 26.0, 0.5),
                Wave::Rock(LEGS[2], x, 26.0, 0.5),
                Wave::Rock("Tail", y, 8.0, 0.25),
            ];
            if cat == Some(BigCat::Displacer) {
                waves.extend([
                    Wave::Rock(MIDDLE_LEGS[0], x, 26.0, 0.25),
                    Wave::Rock(MIDDLE_LEGS[1], x, 26.0, 0.75),
                ]);
                waves.extend(idle_waves(kind, shape));
            }
            waves
        }
        // a sprawled leg swings round its hip, its foot along the floor;
        // turned the same way, a left foot goes forward as a right one
        // goes back, so a diagonal pair turns opposite ways
        (Proc::Lizard, _) => vec![
            Wave::Rock(LEGS[0], y, 28.0, 0.0),
            Wave::Rock(LEGS[1], y, 28.0, 0.0),
            Wave::Rock(LEGS[2], y, 28.0, 0.5),
            Wave::Rock(LEGS[3], y, 28.0, 0.5),
            Wave::Rock("Head", y, 6.0, 0.5),
            Wave::Rock("Tail", y, 16.0, 0.25),
        ],
        // a bird's stride, its head bobbing, a cockatrice's tail swaying
        (Proc::Bird, Some("cockatrice" | "chickatrice")) => vec![
            Wave::Rock("LegL", x, 30.0, 0.0),
            Wave::Rock("LegR", x, 30.0, 0.5),
            Wave::Rock("Head", x, 12.0, 0.25),
            Wave::Rock("Tail", y, 14.0, 0.25),
        ],
        (Proc::Bug, _) => vec![
            Wave::Rock("LegsL", y, 16.0, 0.0),
            Wave::Rock("LegsR", y, 16.0, 0.5),
            Wave::Bob("Body", Vector3::ZERO, 0.012, 0.0),
        ],
        (Proc::Worm, _) => vec![Wave::Pulse("Body", 0.05, 0.0)],
        (Proc::Blob, _) => vec![Wave::Pulse("Body", 0.09, 0.0)],
        _ => Vec::new(),
    }
}
