//! The hero's own look on the map: the gear the pack says they carry (read
//! again only when the pack changes), what their lamp does to the light
//! over their head, and the effects of the items they use.

use std::collections::HashMap;

use godot::classes::base_material_3d::{
    BillboardMode, BlendMode, CullMode, Flags, ShadingMode, TextureParam, Transparency,
};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::particle_process_material::{EmissionShape, Parameter as ProcessParam};
use godot::classes::{
    CylinderMesh, GpuParticles3D, Gradient, GradientTexture1D, MeshInstance3D, OmniLight3D,
    ParticleProcessMaterial, QuadMesh, SphereMesh, StandardMaterial3D, Texture2D,
};
use godot::prelude::*;
use nh_art::{ArtManifest, Gear};
use nh_protocol::{Catalog, InvItem, ObjectTile};
use nh_world::{ItemUse, Pack, UseKind};

/// The hero's gear, as of the last pack it was read from.
#[derive(Default)]
pub struct HeroGear {
    items: Vec<InvItem>,
    twoweap: bool,
    read: bool,
    gear: Gear,
    /// Whether the light over the hero is the lamp's helper now.
    lamp: bool,
}

impl HeroGear {
    /// The gear of `pack`: read again only when it changed.
    pub fn update(&mut self, pack: &Pack, catalog: &Catalog, art: &ArtManifest) -> &Gear {
        if !self.read || pack.items() != self.items.as_slice() || pack.twoweap() != self.twoweap {
            self.items = pack.items().to_vec();
            self.twoweap = pack.twoweap();
            self.gear = art.gear(pack, catalog);
            self.read = true;
        }
        &self.gear
    }

    pub fn gear(&self) -> &Gear {
        &self.gear
    }

    /// A lit lamp in hand (or not) from now on: true when that changed.
    pub fn set_lamp(&mut self, lit: bool) -> bool {
        std::mem::replace(&mut self.lamp, lit) != lit
    }

    /// Read the pack again next time (a new game).
    pub fn reset(&mut self) {
        *self = HeroGear::default();
    }
}

/// What using an item shows around the hero (spec §3.9): a drink's
/// sparkle in the potion's colour, runes rising off a page, a wand's beam,
/// a spell's glow, the thrown thing flying off, dust at a kick. Each
/// effect is a few nodes freed when it ends. The colour of an effect never
/// tells more than the appearance (a ruby potion sparkles red; a wand's
/// beam is its wood or metal's colour, else arcane violet).
pub struct HeroFx {
    root: Gd<Node3D>,
    pending: Vec<(f32, Fx)>,
    live: Vec<Live>,
    textures: HashMap<&'static str, Option<Gd<Texture2D>>>,
    /// Effects started since the start, by name (self-tests).
    started: Vec<&'static str>,
}

/// An effect, as plain data.
#[derive(Debug, Clone)]
pub enum Fx {
    /// Particles from a point: texture, colour, how many, rising or not.
    Burst {
        name: &'static str,
        at: Vector3,
        color: Color,
        texture: &'static str,
        amount: i32,
        rise: f32,
    },
    Beam {
        from: Vector3,
        to: Vector3,
        color: Color,
    },
    /// A thrown thing's model flying from one point to another.
    Throw {
        model: Option<Gd<Node3D>>,
        from: Vector3,
        to: Vector3,
    },
}

struct Live {
    node: Gd<Node3D>,
    age: f32,
    life: f32,
    /// A light that flashes and fades, with its peak energy.
    light: Option<(Gd<OmniLight3D>, f32)>,
    /// A flight: from, to.
    flight: Option<(Vector3, Vector3)>,
    beam: Option<Gd<StandardMaterial3D>>,
}

/// Seconds a thrown thing flies per cell, and how high it arcs.
const FLIGHT_SECS: f32 = 0.07;
const FLIGHT_ARC: f32 = 0.35;
const BURST_SECS: f32 = 1.1;
const BEAM_SECS: f32 = 0.45;

impl HeroFx {
    pub fn new(root: Gd<Node3D>) -> HeroFx {
        HeroFx {
            root,
            pending: Vec::new(),
            live: Vec::new(),
            textures: HashMap::new(),
            started: Vec::new(),
        }
    }

    /// Start `fx` in `delay` seconds.
    pub fn after(&mut self, delay: f32, fx: Fx) {
        self.pending.push((delay, fx));
    }

    /// Effects started so far, by name (self-tests).
    pub fn started(&self) -> &[&'static str] {
        &self.started
    }

    /// Effects under way or still to come.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.live.is_empty()
    }

    /// End everything at once (a new level, a new game).
    pub fn clear(&mut self) {
        for (_, fx) in self.pending.drain(..) {
            if let Fx::Throw {
                model: Some(mut m), ..
            } = fx
            {
                m.queue_free();
            }
        }
        for mut l in self.live.drain(..) {
            if l.node.is_instance_valid() {
                l.node.queue_free();
            }
        }
    }

    pub fn advance(&mut self, delta: f32) {
        let mut due = Vec::new();
        for (left, fx) in std::mem::take(&mut self.pending) {
            if left - delta <= 0.0 {
                due.push(fx);
            } else {
                self.pending.push((left - delta, fx));
            }
        }
        for fx in due {
            self.spawn(fx);
        }
        let mut i = 0;
        while i < self.live.len() {
            let l = &mut self.live[i];
            l.age += delta;
            let t = (l.age / l.life).clamp(0.0, 1.0);
            if let Some((light, peak)) = l.light.as_mut()
                && light.is_instance_valid()
            {
                // up at once, then out
                light.set_param(Param::ENERGY, *peak * (1.0 - t) * (1.0 - t));
            }
            if let Some((from, to)) = l.flight
                && l.node.is_instance_valid()
            {
                let arc = FLIGHT_ARC * (std::f32::consts::PI * t).sin();
                let p = from.lerp(to, t) + Vector3::new(0.0, arc, 0.0);
                l.node.set_position(p);
                l.node.rotate_object_local(Vector3::RIGHT, delta * 14.0);
            }
            if let Some(m) = l.beam.as_mut() {
                let c = m.get_albedo();
                m.set_albedo(c.with_alpha(1.0 - t));
            }
            if l.age >= l.life {
                let mut l = self.live.swap_remove(i);
                if l.node.is_instance_valid() {
                    l.node.queue_free();
                }
            } else {
                i += 1;
            }
        }
    }

    fn texture(&mut self, name: &'static str) -> Option<Gd<Texture2D>> {
        self.textures
            .entry(name)
            .or_insert_with(|| {
                godot::tools::try_load::<Texture2D>(&format!(
                    "res://art/cc0/kenney/particles/{name}.png"
                ))
                .ok()
            })
            .clone()
    }

    fn glow_material(
        &mut self,
        color: Color,
        texture: Option<&'static str>,
    ) -> Gd<StandardMaterial3D> {
        let mut m = StandardMaterial3D::new_gd();
        m.set_shading_mode(ShadingMode::UNSHADED);
        m.set_transparency(Transparency::ALPHA);
        m.set_blend_mode(BlendMode::ADD);
        m.set_cull_mode(CullMode::DISABLED);
        m.set_albedo(color);
        if let Some(t) = texture.and_then(|t| self.texture(t)) {
            m.set_texture(TextureParam::ALBEDO, &t);
            m.set_billboard_mode(BillboardMode::PARTICLES);
            m.set_flag(Flags::ALBEDO_FROM_VERTEX_COLOR, true);
        }
        m
    }

    fn flash(
        &mut self,
        node: &mut Gd<Node3D>,
        color: Color,
        energy: f32,
        range: f32,
    ) -> Gd<OmniLight3D> {
        let mut light = OmniLight3D::new_alloc();
        light.set_color(color);
        light.set_param(Param::ENERGY, energy);
        light.set_param(Param::RANGE, range);
        light.set_param(Param::ATTENUATION, 1.2);
        node.add_child(&light);
        light
    }

    fn spawn(&mut self, fx: Fx) {
        match fx {
            Fx::Burst {
                name,
                at,
                color,
                texture,
                amount,
                rise,
            } => {
                self.started.push(name);
                let mut node = Node3D::new_alloc();
                node.set_name(&format!("Fx_{name}"));
                node.set_position(at);
                let mut p = GpuParticles3D::new_alloc();
                p.set_amount(amount);
                p.set_lifetime(f64::from(BURST_SECS * 0.8));
                p.set_one_shot(true);
                p.set_explosiveness_ratio(0.7);
                p.set_randomness_ratio(0.5);
                let mut pm = ParticleProcessMaterial::new_gd();
                pm.set_emission_shape(EmissionShape::SPHERE);
                pm.set_emission_sphere_radius(0.22);
                pm.set_direction(Vector3::UP);
                pm.set_spread(70.0);
                pm.set_param_min(ProcessParam::INITIAL_LINEAR_VELOCITY, 0.2);
                pm.set_param_max(ProcessParam::INITIAL_LINEAR_VELOCITY, 0.2 + rise);
                pm.set_gravity(Vector3::new(0.0, rise * 0.6 - 0.4, 0.0));
                pm.set_param_min(ProcessParam::SCALE, 0.5);
                pm.set_param_max(ProcessParam::SCALE, 1.2);
                let mut ramp = Gradient::new_gd();
                ramp.set_color(0, Color::WHITE);
                ramp.set_color(1, Color::from_rgba(1.0, 1.0, 1.0, 0.0));
                let mut rt = GradientTexture1D::new_gd();
                rt.set_gradient(&ramp);
                pm.set_color_ramp(&rt);
                p.set_process_material(&pm);
                let mut quad = QuadMesh::new_gd();
                quad.set_size(Vector2::new(0.12, 0.12));
                let mat = self.glow_material(color, Some(texture));
                quad.set_material(&mat);
                p.set_draw_pass_mesh(0, &quad);
                p.set_emitting(true);
                node.add_child(&p);
                let light = self.flash(&mut node, color, 2.2, 2.6);
                self.root.add_child(&node);
                self.live.push(Live {
                    node,
                    age: 0.0,
                    life: BURST_SECS,
                    light: Some((light, 2.2)),
                    flight: None,
                    beam: None,
                });
            }
            Fx::Beam { from, to, color } => {
                self.started.push("beam");
                let mut node = Node3D::new_alloc();
                node.set_name("Fx_beam");
                let len = from.distance_to(to).max(0.1);
                let mut mesh = CylinderMesh::new_gd();
                mesh.set_top_radius(0.035);
                mesh.set_bottom_radius(0.035);
                mesh.set_height(len);
                let mat = self.glow_material(color, None);
                let mut mi = MeshInstance3D::new_alloc();
                mi.set_mesh(&mesh);
                mi.set_material_override(&mat);
                mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
                // the cylinder stands along y: lay it from `from` to `to`
                let dir = (to - from).normalized();
                let side = dir.cross(Vector3::UP);
                let x = if side.length() > 0.01 {
                    side.normalized()
                } else {
                    Vector3::RIGHT
                };
                let basis = Basis::from_cols(x, dir, x.cross(dir));
                node.set_transform(Transform3D::new(basis, (from + to) * 0.5));
                node.add_child(&mi);
                let light = self.flash(&mut node, color, 3.0, 4.0);
                self.root.add_child(&node);
                self.live.push(Live {
                    node,
                    age: 0.0,
                    life: BEAM_SECS,
                    light: Some((light, 3.0)),
                    flight: None,
                    beam: Some(mat),
                });
            }
            Fx::Throw { model, from, to } => {
                self.started.push("throw");
                let mut node = Node3D::new_alloc();
                node.set_name("Fx_throw");
                node.set_position(from);
                match model {
                    Some(m) => node.add_child(&m),
                    None => {
                        let mut mi = MeshInstance3D::new_alloc();
                        let mut s = SphereMesh::new_gd();
                        s.set_radius(0.04);
                        s.set_height(0.08);
                        mi.set_mesh(&s);
                        node.add_child(&mi);
                    }
                }
                self.root.add_child(&node);
                let cells = from.distance_to(to).max(1.0);
                self.live.push(Live {
                    node,
                    age: 0.0,
                    life: FLIGHT_SECS * cells + 0.1,
                    light: None,
                    flight: Some((from, to)),
                    beam: None,
                });
            }
        }
    }
}

/// The colour of a use's effect: the appearance's colour, else the kind's.
pub fn use_color(u: &ItemUse, tile: Option<&ObjectTile>, art: &ArtManifest) -> Color {
    let own = tile.and_then(|t| art.appearance_color(t));
    let fallback = match u.kind {
        UseKind::Read => [1.0, 0.82, 0.4],
        UseKind::Cast => [0.5, 0.7, 1.0],
        UseKind::Eat => [0.8, 0.62, 0.4],
        UseKind::Kick => [0.62, 0.56, 0.5],
        UseKind::Apply => [1.0, 0.86, 0.66],
        _ => [0.66, 0.5, 1.0],
    };
    let c = match (u.kind, own) {
        // a page's runes are gold whatever the cover
        (UseKind::Read | UseKind::Cast | UseKind::Kick | UseKind::Apply, _) | (_, None) => fallback,
        (_, Some(c)) => c,
    };
    Color::from_rgb(c[0], c[1], c[2])
}

/// The name of a use in the manifest's `held.uses`.
pub fn use_name(kind: UseKind) -> &'static str {
    match kind {
        UseKind::Quaff => "quaff",
        UseKind::Read => "read",
        UseKind::Zap => "zap",
        UseKind::Cast => "cast",
        UseKind::Eat => "eat",
        UseKind::Apply => "apply",
        UseKind::Throw => "throw",
        UseKind::Fire => "fire",
        UseKind::Wear => "wear",
        UseKind::PutOn => "put_on",
        UseKind::TakeOff => "take_off",
        UseKind::Remove => "remove",
        UseKind::Wield => "wield",
        UseKind::PickUp => "pick_up",
        UseKind::Kick => "kick",
    }
}

/// Whether the item is shown in the hand while it is used, and for how
/// long of the clip (throws let go early).
pub fn in_hand(kind: UseKind) -> Option<f32> {
    match kind {
        UseKind::Quaff | UseKind::Eat | UseKind::Read | UseKind::Zap | UseKind::Apply => Some(1.6),
        UseKind::Throw | UseKind::Fire => Some(THROW_AT),
        _ => None,
    }
}

/// When a throw lets go (seconds into the clip).
pub const THROW_AT: f32 = 0.45;

/// The effects of a use, after their delays: `hand` is where the item
/// is, `ahead` the far end of its way (a beam's, a flight's).
pub fn use_effects(
    u: &ItemUse,
    color: Color,
    hand: Vector3,
    ahead: Option<Vector3>,
) -> Vec<(f32, Fx)> {
    let burst = |name, at, texture, amount, rise| Fx::Burst {
        name,
        at,
        color,
        texture,
        amount,
        rise,
    };
    let up = Vector3::new(0.0, 0.35, 0.0);
    match u.kind {
        UseKind::Quaff => vec![(0.8, burst("quaff", hand + up, "star_06", 28, 1.2))],
        UseKind::Eat => vec![(0.6, burst("eat", hand, "dirt_02", 14, 0.2))],
        UseKind::Read => vec![(0.5, burst("read", hand, "magic_02", 26, 1.4))],
        UseKind::Zap => {
            let mut v = vec![(0.35, burst("zap", hand, "spark_05", 18, 0.6))];
            if let Some(to) = ahead {
                v.push((
                    0.4,
                    Fx::Beam {
                        from: hand,
                        to,
                        color,
                    },
                ));
            }
            v
        }
        UseKind::Cast => {
            let mut v = vec![(0.3, burst("cast", hand, "magic_04", 30, 0.8))];
            if let Some(to) = ahead {
                v.push((
                    0.4,
                    Fx::Beam {
                        from: hand,
                        to,
                        color,
                    },
                ));
            }
            v
        }
        UseKind::Apply => vec![(0.4, burst("apply", hand, "spark_02", 10, 0.3))],
        UseKind::Kick => vec![(
            0.3,
            burst("kick", ahead.unwrap_or(hand), "smoke_03", 12, 0.2),
        )],
        _ => Vec::new(),
    }
}
