//! What the hero's body shows of the conditions on the status line (the
//! player reads them there: showing them leaks nothing). Turning to stone
//! grey stone creeps over the hero in patches, turning to slime green
//! slime does, wet and faintly glowing, more of them each turn of the
//! countdown (five for stone, ten for slime), all of the body at its end.
//! Levitating or flying they float above the floor, bobbing slowly, and
//! glide from cell to cell, their legs still. Stunned or confused they
//! sway where they stand and the camera drifts a little; hallucinating, a
//! slow hue drifts over the view; blind, the view dims and darkens at its
//! edges. Fewer screen effects (the settings) leaves out the sway, the
//! drift and the hue; the dimming stays. Burdened, up to Overloaded, they
//! lean into the load and walk slower, never running; weak from hunger or
//! fainting, they slump where they stand; strangled, they clutch their
//! throat with both hands. A child of map_view: it lays its
//! skin over the hero's own model (the rider's while they ride), under the
//! see-through look that shows them behind a wall.

use godot::classes::base_material_3d::{Feature, Flags, TextureParam, Transparency};
use godot::classes::fast_noise_lite::NoiseType;
use godot::classes::{FastNoiseLite, Gradient, NoiseTexture2D, StandardMaterial3D};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::World;

use crate::bearing::{Bearing, Throat};

use super::{MapView, set_overlay};

/// A condition the hero's body shows, as the status line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shown {
    Stone,
    Slime,
}

/// A floating hero's height over the floor, the bob's height and its
/// seconds, and the seconds they take to rise or come down; the shadow
/// on the floor under them (from the camera above a lift reads by it),
/// its size and darkness.
const FLOAT_HEIGHT: f32 = 0.32;
const UNDER_SIZE: f32 = 0.85;
const UNDER_DARK: f32 = 0.6;

/// A stunned hero's sway (a confused one's is gentler): degrees at its
/// widest, the seconds of its two waves, the metres the camera's aim
/// wanders, and the seconds it takes to come and go.
const SWAY_DEG: f32 = 7.0;
const CONFUSED_SWAY: f32 = 0.55;
const SWAY_SECS: (f32, f32) = (1.9, 3.1);
const DRIFT: f32 = 0.22;
const EFFECT_EASE: f32 = 0.8;
/// A hallucination's tint over the view, the most it lifts a channel,
/// the hue it starts at (a sea green: none of the dungeon's warm light)
/// and the seconds of a turn round the hues; blindness's darkened edges,
/// and how far the view dims.
const HUE_ALPHA: f32 = 0.25;
const HUE_MOST: f32 = 2.0;
const HUE_START: f32 = 0.45;
const HUE_SECS: f32 = 9.0;
const BLIND_EDGE: f32 = 0.85;
const BLIND_DIM: (f32, f32) = (0.62, 0.45);
/// The screen's quad sits this far before the eye.
const SCREEN_AT: f32 = 0.3;

/// A burdened hero's lean (degrees) at each burden the status line names,
/// Burdened to Overloaded, while walking (standing, a share of it), the
/// share of it their head lifts by to see ahead, and how much longer a
/// step takes at each.
const LOAD_LEAN: [f32; 5] = [10.0, 15.0, 20.0, 25.0, 30.0];
const LOAD_STANDING: f32 = 0.7;
const LOAD_HEAD_UP: f32 = 0.6;
const LOAD_SLOWER: f32 = 0.15;
/// A hungry hero's slump, Weak and Fainting: the bow of their shoulders
/// and neck and the hang of their head (degrees) standing (walking, a
/// share of it), and their heavy breath: its seconds and its depth.
const SLUMP_BOW: [f32; 2] = [14.0, 22.0];
const SLUMP_NOD: [f32; 2] = [12.0, 18.0];
const SLUMP_WALKING: f32 = 0.5;
const BREATH_SECS: f32 = 3.4;
const BREATH: f32 = 0.15;
/// Degrees a second the bearing changes by; the name of its modifier on
/// the hero's skeleton.
const BEARING_RATE: f32 = 24.0;
const BEARING: &str = "Bearing";
/// Seconds the hands take to go to the throat and to come away; the name
/// of their modifier; the bones whose attachments (a weapon in the hand,
/// a shield on the arm) are put by while they clutch it, or they would
/// wave about the head.
const CLUTCH_SECS: f32 = 0.35;
const THROAT: &str = "Throat";
const ARM_BONES: [&str; 6] = [
    "hand_l",
    "hand_r",
    "lowerarm_l",
    "lowerarm_r",
    "upperarm_l",
    "upperarm_r",
];

/// The quad over the view: a hue tinting it, its edges darkened. It
/// multiplies what is drawn, so the dark stays dark and nothing is read
/// back from the screen.
const SCREEN_SHADER: &str = r#"shader_type spatial;
render_mode unshaded, blend_mul, depth_test_disabled, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;
uniform vec3 tint = vec3(1.0);
uniform float edge = 0.0;
void fragment() {
	vec2 c = (SCREEN_UV - 0.5) * vec2(1.5, 1.0);
	float dark = edge * smoothstep(0.22, 0.78, length(c));
	ALBEDO = tint * (1.0 - dark);
}
"#;

/// A hallucination's tint at `hue` (a turn round the wheel from 0 to 1),
/// `k` of the way from none: in linear light and of even brightness, so
/// that no hue darkens the view or lights it up.
fn hue_tint(hue: f32, k: f32) -> Vector3 {
    // the hue at 0.65 saturation and full value, from sRGB to linear
    let channel = |n: f32| {
        let at = (n + hue * 6.0) % 6.0;
        let v = 1.0 - 0.65 * at.min(4.0 - at).clamp(0.0, 1.0);
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let rgb = Vector3::new(channel(5.0), channel(3.0), channel(1.0));
    let even = rgb / rgb.dot(Vector3::new(0.2126, 0.7152, 0.0722)).max(0.05);
    let even = Vector3::new(
        even.x.min(HUE_MOST),
        even.y.min(HUE_MOST),
        even.z.min(HUE_MOST),
    );
    Vector3::ONE + (even - Vector3::ONE) * k
}
const BOB: f32 = 0.05;
const BOB_SECS: f32 = 2.6;
const FLOAT_EASE: f32 = 0.6;

impl Shown {
    const ALL: [Shown; 2] = [Shown::Stone, Shown::Slime];

    fn name(self) -> &'static str {
        match self {
            Shown::Stone => "Stone",
            Shown::Slime => "Slime",
        }
    }

    /// Turns from the condition's start to its end (stoned.c, slime's
    /// countdown in timeout.c).
    fn turns(self) -> f32 {
        match self {
            Shown::Stone => 5.0,
            Shown::Slime => 10.0,
        }
    }
}

/// How far along a countdown is shown at its first turn.
const FIRST_STEP: f32 = 0.2;
/// Stone's creep is of the statues' own grey (the art's `statue`
/// material, which a stoned hero wears whole at the end), as dark as the
/// light on the hero is bright: under it they read as the statues round
/// them, not as white marble.
const STONE: Color = Color::from_rgb(0.21, 0.225, 0.245);
const STONE_SHADE: u8 = 50;
const SLIME: Color = Color::from_rgb(0.3, 0.75, 0.2);
const SLIME_GLOW: Color = Color::from_rgb(0.06, 0.22, 0.03);

/// The screen's effects now (self-tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effects {
    /// The hero's sway (1 stunned, gentler confused), the camera's drift.
    pub sway: f32,
    pub drift: Vector3,
    /// The hue's strength and the dimming's (0..=1).
    pub hue: f32,
    pub dim: f32,
    /// The environment's brightness.
    pub bright: f32,
    /// The quad over the view shows.
    pub screen: bool,
    /// Every effect at the strength it eases to.
    pub settled: bool,
}

/// The state shown now.
pub(super) struct HeroLook {
    /// The catalog's masks of the conditions shown.
    masks: Vec<(u64, Shown)>,
    /// The turn each condition under way began.
    since: Vec<(Shown, i64)>,
    stone: Gd<StandardMaterial3D>,
    slime: Gd<StandardMaterial3D>,
    /// The tint on the hero's model now: which model, which tint, and
    /// whether it is at its end.
    tinted: Option<(InstanceId, Option<(Shown, bool)>)>,
    /// The statues' own stone (fetched the first time a hero is stoned
    /// through).
    statue: Option<Gd<godot::classes::Material>>,
    /// The masks of Lev and Fly; how far up a float is (0..=1, easing),
    /// and the seconds the bob has run.
    float_masks: u64,
    float: f32,
    bob: f32,
    /// The shadow under a floating hero (made the first time they float).
    under: Option<Gd<godot::classes::Decal>>,
    /// Stun's and Conf's, Hallu's, Blind's masks.
    stun: u64,
    conf: u64,
    hallu: u64,
    blind: u64,
    /// Strngl's mask; how far the hands are at the throat (0..=1, easing),
    /// and their modifier on the hero's skeleton with the model's inner
    /// node.
    strangled: u64,
    clutch: f32,
    throat: Option<(InstanceId, Gd<Throat>)>,
    /// What the hands held and the arms wore, hidden while they clutch the
    /// throat.
    put_by: Vec<Gd<Node3D>>,
    /// The effects' strengths (0..=1, easing), whether each is where it
    /// eases to, and the seconds they ran.
    sway: f32,
    hue: f32,
    dim: f32,
    settled: bool,
    clock: f32,
    /// The hero's model swayed: its inner node and its own basis.
    swayed: Option<(Gd<Node3D>, Basis)>,
    /// Where the camera's aim wanders to now (map_view's `place_camera`).
    pub(super) drift: Vector3,
    /// The quad over the view, its material, and whether it shows.
    screen: Option<(
        Gd<godot::classes::MeshInstance3D>,
        Gd<godot::classes::ShaderMaterial>,
    )>,
    screened: bool,
    /// The settings' fewer screen effects.
    pub(super) reduced: bool,
    /// The environment dimmed now (its grade is given back after).
    dimmed: bool,
    /// The status line's burden (0: none, 1 Burdened to 5 Overloaded) and
    /// hunger (0: none, 1 Weak, 2 Fainting or Fainted).
    load: usize,
    hunger: usize,
    /// The bearing's lean, bow and nod now (degrees, easing), whether they
    /// are where they ease to, the seconds a slump has breathed, and its
    /// modifier on the hero's skeleton with the model's inner node.
    lean: f32,
    bow: f32,
    nod: f32,
    bent: bool,
    breath: f32,
    bearing: Option<(InstanceId, Gd<Bearing>)>,
    /// The hero's last step: its seconds, and whether it was a run.
    last_step: Option<(f32, bool)>,
}

/// The hero's bearing now (self-tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Posture {
    /// Degrees: the back's lean, the shoulders' bow, the head's nod.
    pub lean: f32,
    pub bow: f32,
    pub nod: f32,
    /// Each where it eases to.
    pub settled: bool,
    /// The modifier bends the skeleton, and how far it took the head last
    /// frame: forward, and up (the skeleton's units).
    pub on: bool,
    pub head: (f32, f32),
    /// The last step's seconds, and whether it was a run.
    pub step: Option<(f32, bool)>,
    /// How far the hands are at the throat (0..=1), and how far a palm
    /// stopped short of the neck last frame (metres).
    pub clutch: f32,
    pub miss: f32,
}

/// Its share of the way, `elapsed` turns into a countdown of `turns`.
pub fn progress(elapsed: i64, turns: f32) -> f32 {
    (FIRST_STEP + elapsed.max(0) as f32 / turns).clamp(FIRST_STEP, 1.0)
}

/// A skin creeping over a body in patches: `color` mottled by a noise's
/// cells, mapped from every side (any model, whatever its UVs), shown
/// where the noise passes a threshold that falls as the countdown runs
/// (alpha scissor: no blending).
fn creeping(color: Color, seed: i32, noise_type: NoiseType) -> Gd<StandardMaterial3D> {
    let mut noise = FastNoiseLite::new_gd();
    noise.set_seed(seed);
    noise.set_noise_type(noise_type);
    noise.set_frequency(0.02);
    let mut ramp = Gradient::new_gd();
    ramp.set_color(0, Color::from_rgba(0.68, 0.68, 0.68, 0.0));
    ramp.set_color(1, Color::from_rgba(1.0, 1.0, 1.0, 1.0));
    let mut texture = NoiseTexture2D::new_gd();
    texture.set_width(256);
    texture.set_height(256);
    texture.set_seamless(true);
    texture.set_noise(&noise);
    texture.set_color_ramp(&ramp);
    let mut m = StandardMaterial3D::new_gd();
    m.set_albedo(color);
    m.set_texture(TextureParam::ALBEDO, &texture);
    m.set_flag(Flags::UV1_USE_TRIPLANAR, true);
    // a patch a hand or two across on a body of metres
    m.set_uv1_scale(Vector3::new(1.2, 1.2, 1.2));
    m.set_transparency(Transparency::ALPHA_SCISSOR);
    m.set_alpha_scissor_threshold(1.0);
    m
}

impl HeroLook {
    pub(super) fn new() -> HeroLook {
        let mut stone = creeping(STONE, 7, NoiseType::CELLULAR);
        stone.set_roughness(0.95);
        let mut slime = creeping(SLIME, 11, NoiseType::SIMPLEX_SMOOTH);
        slime.set_roughness(0.15);
        slime.set_feature(Feature::EMISSION, true);
        slime.set_emission(SLIME_GLOW);
        HeroLook {
            masks: Vec::new(),
            since: Vec::new(),
            stone,
            slime,
            tinted: None,
            statue: None,
            float_masks: 0,
            float: 0.0,
            bob: 0.0,
            under: None,
            stun: 0,
            conf: 0,
            hallu: 0,
            blind: 0,
            sway: 0.0,
            hue: 0.0,
            dim: 0.0,
            clock: 0.0,
            swayed: None,
            drift: Vector3::ZERO,
            screen: None,
            reduced: false,
            dimmed: false,
            settled: true,
            screened: false,
            load: 0,
            hunger: 0,
            lean: 0.0,
            bow: 0.0,
            nod: 0.0,
            bent: true,
            breath: 0.0,
            bearing: None,
            last_step: None,
            strangled: 0,
            clutch: 0.0,
            throat: None,
            put_by: Vec::new(),
        }
    }

    /// A new game: nothing under way.
    pub(super) fn forget(&mut self) {
        self.since.clear();
        self.tinted = None;
        self.float = 0.0;
        if let Some(u) = self.under.as_mut() {
            u.set_visible(false);
        }
        self.sway = 0.0;
        self.hue = 0.0;
        self.dim = 0.0;
        self.drift = Vector3::ZERO;
        self.unsway();
        if let Some((q, _)) = self.screen.as_mut() {
            q.set_visible(false);
        }
        self.screened = false;
        // the back straight and the hands down
        (self.lean, self.bow, self.nod, self.clutch) = (0.0, 0.0, 0.0, 0.0);
        if let Some((_, b)) = self.bearing.as_mut()
            && b.is_instance_valid()
        {
            b.bind_mut().set_angles(0.0, 0.0, 0.0);
            b.clone()
                .upcast::<godot::classes::SkeletonModifier3D>()
                .set_active(false);
        }
        if let Some((_, t)) = self.throat.as_ref()
            && t.is_instance_valid()
        {
            let mut m: Gd<godot::classes::SkeletonModifier3D> = t.clone().upcast();
            m.set_influence(0.0);
            m.set_active(false);
        }
        self.take_out();
    }

    /// The swayed model back in its own stance.
    fn unsway(&mut self) {
        if let Some((mut inner, basis)) = self.swayed.take()
            && inner.is_instance_valid()
        {
            inner.set_basis(basis);
        }
    }

    /// The effects now (self-tests), with the environment's brightness.
    pub(super) fn effects(&self, bright: f32) -> Effects {
        Effects {
            sway: self.sway,
            drift: self.drift,
            hue: self.hue,
            dim: self.dim,
            bright,
            screen: self.screened,
            settled: self.settled,
        }
    }

    /// A step's seconds and whether it runs, as the hero's load lets them:
    /// slower the heavier it is, and never a run (kept as the last step).
    pub(super) fn pace(&mut self, secs: f32, run: bool) -> (f32, bool) {
        let paced = match self.load {
            0 => (secs, run),
            // hurried, a quick walk
            _ if run => (secs, false),
            load => (secs * (1.0 + LOAD_SLOWER * load as f32), false),
        };
        self.last_step = Some(paced);
        paced
    }

    /// The bearing now (self-tests), `on`: its modifier bends the skeleton.
    pub(super) fn posture(&self) -> Posture {
        let bearing = self
            .bearing
            .as_ref()
            .map(|(_, b)| b)
            .filter(|b| b.is_instance_valid());
        let on = bearing.is_some_and(|b| {
            b.clone()
                .upcast::<godot::classes::SkeletonModifier3D>()
                .is_active()
        });
        let head = bearing.map_or((0.0, 0.0), |b| b.bind().shift());
        Posture {
            lean: self.lean,
            bow: self.bow,
            nod: self.nod,
            settled: self.bent,
            on,
            head,
            step: self.last_step,
            clutch: self.clutch,
            miss: self
                .throat
                .as_ref()
                .filter(|(_, t)| t.is_instance_valid())
                .map_or(0.0, |(_, t)| t.bind().miss()),
        }
    }

    /// What was put by while the hands clutched the throat, shown again.
    fn take_out(&mut self) {
        for mut n in self.put_by.drain(..) {
            if n.is_instance_valid() {
                n.set_visible(true);
            }
        }
    }

    /// The statues' own stone, once fetched.
    pub(super) fn statue(&self) -> Option<Gd<godot::classes::Material>> {
        self.statue.clone()
    }

    /// The hero floats (or is rising or coming down).
    pub(super) fn floating(&self) -> bool {
        self.float > 0.0
    }

    /// The tint to lay over the hero now (the stronger countdown's, stone
    /// before slime), None: none.
    pub(super) fn overlay(&self) -> Option<Gd<godot::classes::Material>> {
        self.tinted.and_then(|(_, s)| s).map(|s| match s {
            // stoned through: the statues' own cold stone, whatever
            // lights it
            (Shown::Stone, true) => match &self.statue {
                Some(m) => m.clone(),
                None => self.stone.clone().upcast(),
            },
            (Shown::Stone, false) => self.stone.clone().upcast(),
            (Shown::Slime, _) => self.slime.clone().upcast(),
        })
    }

    /// The conditions shown, each with its share of the way (self-tests).
    pub fn shown(&self, world: &World) -> Vec<(Shown, f32)> {
        let turn = world.turn().unwrap_or(0);
        self.since
            .iter()
            .map(|&(s, since)| (s, progress(turn - since, s.turns())))
            .collect()
    }
}

impl MapView {
    /// Every frame with the hero on the map: their body as their
    /// conditions show.
    pub(super) fn look_hero(&mut self, world: &World, catalog: &Catalog, delta: f32) {
        let look = &mut self.hero_look;
        if look.masks.is_empty() {
            look.masks = Shown::ALL
                .iter()
                .filter_map(|&s| {
                    let c = catalog.conditions.iter().find(|c| c.name == s.name())?;
                    Some((c.mask, s))
                })
                .collect();
            look.float_masks = catalog
                .conditions
                .iter()
                .filter(|c| c.name == "Lev" || c.name == "Fly")
                .fold(0, |m, c| m | c.mask);
            let mask = |name: &str| {
                catalog
                    .conditions
                    .iter()
                    .find(|c| c.name == name)
                    .map_or(0, |c| c.mask)
            };
            (look.stun, look.conf) = (mask("Stun"), mask("Conf"));
            (look.hallu, look.blind) = (mask("Hallu"), mask("Blind"));
            look.strangled = mask("Strngl");
        }
        let conds = world.status.conditions();
        // floating, up off the floor and bobbing (a rider keeps their seat)
        let floats = conds & look.float_masks != 0 && self.rider.is_none();
        let to = if floats { 1.0 } else { 0.0 };
        let step = delta / FLOAT_EASE;
        look.float = if look.float < to {
            (look.float + step).min(to)
        } else {
            (look.float - step).max(to)
        };
        look.bob = if look.float > 0.0 {
            (look.bob + delta) % BOB_SECS
        } else {
            0.0
        };
        let bob = (look.bob / BOB_SECS * std::f32::consts::TAU).sin();
        let ease = look.float * look.float * (3.0 - 2.0 * look.float);
        self.hero_pose.lift = ease * (FLOAT_HEIGHT + BOB * bob);
        // their shadow stays on the floor, under the ring
        if ease > 0.0 && look.under.is_none() {
            use godot::classes::decal::DecalTexture;
            let mut d = godot::classes::Decal::new_alloc();
            d.set_name("FloatShadow");
            d.set_size(Vector3::new(UNDER_SIZE, 0.6, UNDER_SIZE));
            d.set_texture(DecalTexture::ALBEDO, &self.soft_tex);
            d.set_albedo_mix(1.0);
            d.set_cull_mask(super::TERRAIN_LAYER);
            d.set_normal_fade(0.6);
            self.root.add_child(&d);
            self.hero_look.under = Some(d);
        }
        self.screen_effects(world.status.conditions(), delta);
        self.bear(world, delta);
        self.clutch(world.status.conditions(), delta);
        let ring = self.hero_ring.get_position();
        let look = &mut self.hero_look;
        if let Some(u) = look.under.as_mut() {
            u.set_visible(ease > 0.0);
            if ease > 0.0 {
                // darker as they rise, a little smaller at the bob's top
                u.set_position(ring);
                u.set_modulate(Color::from_rgba(0.0, 0.0, 0.0, UNDER_DARK * ease));
                let k = 1.0 - 0.1 * bob;
                u.set_size(Vector3::new(UNDER_SIZE * k, 0.6, UNDER_SIZE * k));
            }
        }
        let turn = world.turn().unwrap_or(0);
        for &(mask, s) in &look.masks {
            let on = conds & mask != 0;
            let i = look.since.iter().position(|&(k, _)| k == s);
            match (on, i) {
                (true, None) => look.since.push((s, turn)),
                (false, Some(i)) => {
                    look.since.swap_remove(i);
                }
                _ => {}
            }
        }
        // the countdown furthest along shows (stone over slime)
        let shown = look
            .since
            .iter()
            .map(|&(s, since)| (s, progress(turn - since, s.turns())))
            .max_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then((a.0 == Shown::Stone).cmp(&(b.0 == Shown::Stone)))
            });
        if let Some((s, p)) = shown {
            let m = match s {
                Shown::Stone => &mut look.stone,
                Shown::Slime => &mut look.slime,
            };
            // more of the body each step, all of it at the end (a
            // noise's values crowd its middle: the threshold falls
            // through them more slowly)
            let threshold = if p >= 1.0 {
                0.0
            } else {
                0.1 + 0.75 * (1.0 - p)
            };
            if m.get_alpha_scissor_threshold() != threshold {
                m.set_alpha_scissor_threshold(threshold);
            }
        }
        let whole = shown.is_some_and(|(s, p)| s == Shown::Stone && p >= 1.0);
        if whole && self.hero_look.statue.is_none() {
            let statue = self.art.manifest().material("statue");
            self.hero_look.statue =
                statue.map(|i| self.art.surface(i, STONE_SHADE, false, Color::WHITE));
        }
        let want = shown.map(|(s, _)| (s, whole));
        let model = self.rim_model.clone().filter(|n| n.is_instance_valid());
        let now = model.as_ref().map(|n| (n.instance_id(), want));
        if now == self.hero_look.tinted {
            return;
        }
        self.hero_look.tinted = now;
        // under the see-through look the tint waits (`show_through` lays
        // it back)
        if let Some(n) = model.filter(|_| !self.xray_on) {
            set_overlay(&n, self.hero_look.overlay().as_ref());
        }
    }
}

/// `now` a step toward `to` at `rate` a second.
fn ease_to(now: f32, to: f32, rate: f32) -> f32 {
    if now < to {
        (now + rate).min(to)
    } else {
        (now - rate).max(to)
    }
}

impl MapView {
    /// Stun's and Conf's sway and drift, Hallu's hue, Blind's dimming.
    fn screen_effects(&mut self, conds: u64, delta: f32) {
        let look = &mut self.hero_look;
        let gentle = !look.reduced;
        let sway_to = if conds & look.stun != 0 && gentle {
            1.0
        } else if conds & look.conf != 0 && gentle {
            CONFUSED_SWAY
        } else {
            0.0
        };
        let hue_to = if conds & look.hallu != 0 && gentle {
            1.0
        } else {
            0.0
        };
        let dim_to = if conds & look.blind != 0 { 1.0 } else { 0.0 };
        let rate = delta / EFFECT_EASE;
        look.sway = ease_to(look.sway, sway_to, rate);
        look.hue = ease_to(look.hue, hue_to, rate);
        look.dim = ease_to(look.dim, dim_to, rate);
        look.settled = look.sway == sway_to && look.hue == hue_to && look.dim == dim_to;
        let busy = look.sway > 0.0 || look.hue > 0.0 || look.dim > 0.0;
        look.clock = if busy { look.clock + delta } else { 0.0 };
        // none on and none left to undo: nothing to do
        if !busy && look.swayed.is_none() && !look.screened && !look.dimmed {
            look.drift = Vector3::ZERO;
            return;
        }
        let t = look.clock;
        let wave = |secs: f32, at: f32| (t / secs * std::f32::consts::TAU + at).sin();
        // the sway: two waves out of step, about the feet
        let (roll, pitch) = (
            wave(SWAY_SECS.0, 0.0) + 0.4 * wave(SWAY_SECS.1, 1.3),
            0.6 * wave(SWAY_SECS.1, 0.0) + 0.3 * wave(SWAY_SECS.0, 2.1),
        );
        look.drift = Vector3::new(
            wave(SWAY_SECS.1 * 1.7, 0.4),
            0.0,
            wave(SWAY_SECS.0 * 2.3, 1.9),
        ) * (DRIFT * look.sway);
        let sway = look.sway;
        let inner = self
            .hero_model()
            .and_then(|m| m.node.get_child(0))
            .and_then(|n| n.try_cast::<Node3D>().ok());
        let look = &mut self.hero_look;
        if look
            .swayed
            .as_ref()
            .is_some_and(|(n, _)| inner.as_ref().map(|i| i.instance_id()) != Some(n.instance_id()))
        {
            look.unsway();
        }
        match inner {
            Some(mut n) if sway > 0.0 => {
                let base = match &look.swayed {
                    Some((_, b)) => *b,
                    None => {
                        let b = n.get_basis();
                        look.swayed = Some((n.clone(), b));
                        b
                    }
                };
                let deg = SWAY_DEG * sway;
                let lean = Basis::from_euler(
                    EulerOrder::XYZ,
                    Vector3::new((deg * pitch).to_radians(), 0.0, (deg * roll).to_radians()),
                );
                n.set_basis(lean * base);
            }
            _ => look.unsway(),
        }
        // the hue and the edges: a quad before the eye
        let look = &mut self.hero_look;
        let shows = look.hue > 0.0 || look.dim > 0.0;
        if shows && look.screen.is_none() {
            let mut shader = godot::classes::Shader::new_gd();
            shader.set_code(SCREEN_SHADER);
            let mut mat = godot::classes::ShaderMaterial::new_gd();
            mat.set_shader(&shader);
            let mut quad = godot::classes::QuadMesh::new_gd();
            // wider than any screen at that distance
            let h = 2.0 * SCREEN_AT * (super::FOV_DEG.to_radians() / 2.0).tan() * 1.2;
            quad.set_size(Vector2::new(h * 4.0, h));
            let mut mi = godot::classes::MeshInstance3D::new_alloc();
            mi.set_name("ScreenEffects");
            mi.set_mesh(&quad);
            mi.set_material_override(&mat);
            mi.set_cast_shadows_setting(
                godot::classes::geometry_instance_3d::ShadowCastingSetting::OFF,
            );
            mi.set_position(Vector3::new(0.0, 0.0, -SCREEN_AT));
            self.camera.add_child(&mi);
            self.hero_look.screen = Some((mi, mat));
        }
        let look = &mut self.hero_look;
        if let Some((q, mat)) = look.screen.as_mut() {
            if look.screened != shows {
                q.set_visible(shows);
                look.screened = shows;
            }
            if shows {
                let tint = hue_tint((HUE_START + t / HUE_SECS).fract(), HUE_ALPHA * look.hue);
                mat.set_shader_parameter("tint", &tint.to_variant());
                mat.set_shader_parameter("edge", &(BLIND_EDGE * look.dim).to_variant());
            }
        }
        // blind: the view dims and greys, the branch's grade given back
        // after (set when it changes, or when a new level's grade came)
        let dim = look.dim;
        if dim > 0.0 || look.dimmed {
            let g = self.branch_look.grade;
            let bright = g.brightness * (1.0 - (1.0 - BLIND_DIM.0) * dim);
            let saturation = g.saturation * (1.0 - (1.0 - BLIND_DIM.1) * dim);
            if self.env.get_adjustment_brightness() != bright
                || self.env.get_adjustment_saturation() != saturation
            {
                self.env.set_adjustment_brightness(bright);
                self.env.set_adjustment_saturation(saturation);
            }
            self.hero_look.dimmed = dim > 0.0;
        }
    }
}

impl MapView {
    /// A load's lean and hunger's slump on the hero's back, by the status
    /// line. None wanted and none left: nothing is done.
    fn bear(&mut self, world: &World, delta: f32) {
        let load = match world.status.get("cap").unwrap_or("") {
            "Burdened" => 1,
            "Stressed" => 2,
            "Strained" => 3,
            "Overtaxed" => 4,
            "Overloaded" => 5,
            _ => 0,
        };
        let hunger = match world.status.get("hunger").unwrap_or("") {
            "Weak" => 1,
            "Fainting" | "Fainted" => 2,
            _ => 0,
        };
        let look = &mut self.hero_look;
        (look.load, look.hunger) = (load, hunger);
        if load == 0 && hunger == 0 && look.lean == 0.0 && look.bow == 0.0 && look.nod == 0.0 {
            look.bent = true;
            if let Some((_, b)) = look.bearing.as_ref() {
                let mut m: Gd<godot::classes::SkeletonModifier3D> = b.clone().upcast();
                if m.is_instance_valid() && m.is_active() {
                    m.set_active(false);
                }
            }
            return;
        }
        // a pose of its own (a prayer, a spell of helplessness, a trap's
        // hold) and the saddle take the body as it is
        let posed = self.hero_helpless().is_some()
            || self.hero_prayer().is_some()
            || self.hero_held().2.is_some()
            || self.rider.is_some();
        let walking = self.motions.iter().any(|m| m.hero);
        let (lean_to, bow_to, nod_to) = if posed {
            (0.0, 0.0, 0.0)
        } else {
            let lean = match load {
                0 => 0.0,
                l => LOAD_LEAN[l - 1] * if walking { 1.0 } else { LOAD_STANDING },
            };
            let k = if walking { SLUMP_WALKING } else { 1.0 };
            let (bow, nod) = match hunger {
                0 => (0.0, 0.0),
                h => (SLUMP_BOW[h - 1] * k, SLUMP_NOD[h - 1] * k),
            };
            (lean, bow, nod - LOAD_HEAD_UP * lean)
        };
        let look = &mut self.hero_look;
        let rate = BEARING_RATE * delta;
        look.lean = ease_to(look.lean, lean_to, rate);
        look.bow = ease_to(look.bow, bow_to, rate);
        look.nod = ease_to(look.nod, nod_to, rate);
        look.bent = look.lean == lean_to && look.bow == bow_to && look.nod == nod_to;
        look.breath = if look.bow > 0.0 {
            (look.breath + delta) % BREATH_SECS
        } else {
            0.0
        };
        let breathe = 1.0 + BREATH * (look.breath / BREATH_SECS * std::f32::consts::TAU).sin();
        let angles = (look.lean, look.bow * breathe, look.nod);
        // the modifier on the skeleton of the model the hero is now
        let Some((body, inner)) = self
            .hero_model()
            .and_then(|m| Some((m.node.clone(), m.node.get_child(0)?)))
        else {
            return;
        };
        let id = inner.instance_id();
        if self.hero_look.bearing.as_ref().map(|(i, _)| *i) != Some(id) {
            // off a model the hero no longer is
            if let Some((_, mut old)) = self.hero_look.bearing.take()
                && old.is_instance_valid()
            {
                old.bind_mut().set_angles(0.0, 0.0, 0.0);
                old.upcast::<godot::classes::SkeletonModifier3D>()
                    .set_active(false);
            }
            let skeleton = inner
                .find_children_ex("*")
                .type_("Skeleton3D")
                .owned(false)
                .done()
                .iter_shared()
                .next()
                .and_then(|n| n.try_cast::<godot::classes::Skeleton3D>().ok());
            let Some(mut sk) = skeleton.filter(|s| s.find_bone("spine_01") >= 0) else {
                return;
            };
            let own = sk
                .get_node_or_null(BEARING)
                .and_then(|n| n.try_cast::<Bearing>().ok());
            let bearing = match own {
                Some(b) => b,
                None => {
                    let b = Bearing::new_alloc();
                    let mut m: Gd<godot::classes::SkeletonModifier3D> = b.clone().upcast();
                    m.set_name(BEARING);
                    sk.add_child(&m);
                    // before the off hand's, which holds the haft where
                    // the bent arms have it
                    sk.move_child(&m, 0);
                    b
                }
            };
            self.hero_look.bearing = Some((id, bearing));
        }
        if let Some((_, b)) = self.hero_look.bearing.as_mut() {
            b.bind_mut().set_body(body);
            b.bind_mut().set_angles(angles.0, angles.1, angles.2);
            let on = angles != (0.0, 0.0, 0.0);
            let mut m: Gd<godot::classes::SkeletonModifier3D> = b.clone().upcast();
            if m.is_active() != on {
                m.set_active(on);
            }
        }
    }
}

impl MapView {
    /// Strangled, both hands at the throat. None, and none left to take
    /// away: nothing is done.
    fn clutch(&mut self, conds: u64, delta: f32) {
        let look = &self.hero_look;
        let on = conds & look.strangled != 0 && look.strangled != 0;
        if !on && look.clutch == 0.0 {
            return;
        }
        // a spell of helplessness or a trap's hold takes the body as it is
        let posed = self.hero_helpless().is_some() || self.hero_held().2.is_some();
        let to = if on && !posed { 1.0 } else { 0.0 };
        let look = &mut self.hero_look;
        let was = look.clutch;
        look.clutch = ease_to(look.clutch, to, delta / CLUTCH_SECS);
        let k = look.clutch;
        let Some((body, inner)) = self
            .hero_model()
            .and_then(|m| Some((m.node.clone(), m.node.get_child(0)?)))
        else {
            return;
        };
        let id = inner.instance_id();
        if self.hero_look.throat.as_ref().map(|(i, _)| *i) != Some(id) {
            // off a model the hero no longer is
            self.hero_look.take_out();
            if let Some((_, old)) = self.hero_look.throat.take()
                && old.is_instance_valid()
            {
                let mut m: Gd<godot::classes::SkeletonModifier3D> = old.upcast();
                m.set_influence(0.0);
                m.set_active(false);
            }
            let skeleton = inner
                .find_children_ex("*")
                .type_("Skeleton3D")
                .owned(false)
                .done()
                .iter_shared()
                .next()
                .and_then(|n| n.try_cast::<godot::classes::Skeleton3D>().ok());
            let Some(mut sk) = skeleton.filter(|s| s.find_bone("upperarm_l") >= 0) else {
                return;
            };
            let own = sk
                .get_node_or_null(THROAT)
                .and_then(|n| n.try_cast::<Throat>().ok());
            let throat = match own {
                Some(t) => t,
                None => {
                    let t = Throat::new_alloc();
                    let mut m: Gd<godot::classes::SkeletonModifier3D> = t.clone().upcast();
                    m.set_name(THROAT);
                    m.set_influence(0.0);
                    m.set_active(false);
                    sk.add_child(&m);
                    t
                }
            };
            self.hero_look.throat = Some((id, throat));
        }
        // the palms as the manifest holds things in the hands
        let slot = |name: &str| {
            self.art
                .manifest()
                .held_slot(name)
                .map_or(Transform3D::IDENTITY, |s| {
                    let rot =
                        Vector3::new(s.rot[0], s.rot[1], s.rot[2]) * (std::f32::consts::PI / 180.0);
                    Transform3D::new(
                        Basis::from_euler(EulerOrder::YXZ, rot),
                        Vector3::new(s.pos[0], s.pos[1], s.pos[2]),
                    )
                })
        };
        let palms = [slot("hand_l"), slot("hand_r")];
        let Some((_, t)) = self.hero_look.throat.as_mut() else {
            return;
        };
        t.bind_mut().set_body(body, palms);
        let mut m: Gd<godot::classes::SkeletonModifier3D> = t.clone().upcast();
        if was == 0.0 && k > 0.0 {
            // after every other modifier (the off hand's too, made since)
            if let Some(mut sk) = m.get_parent() {
                let last = sk.get_child_count() - 1;
                sk.move_child(&m, last);
            }
        }
        m.set_influence(k * k * (3.0 - 2.0 * k));
        if m.is_active() != (k > 0.0) {
            m.set_active(k > 0.0);
        }
        // the hands empty while at the throat
        let clutching = k > 0.5;
        if clutching && self.hero_look.put_by.is_empty() {
            let sk = m.get_parent();
            self.hero_look.put_by = sk
                .map(|sk| {
                    sk.get_children()
                        .iter_shared()
                        .filter_map(|n| n.try_cast::<godot::classes::BoneAttachment3D>().ok())
                        .filter(|b| {
                            b.is_visible()
                                && ARM_BONES.contains(&b.get_bone_name().to_string().as_str())
                        })
                        .map(|mut b| {
                            b.set_visible(false);
                            b.upcast()
                        })
                        .collect()
                })
                .unwrap_or_default();
        } else if !clutching {
            self.hero_look.take_out();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_countdown_shows_a_step_further_each_turn() {
        assert_eq!(progress(0, 5.0), 0.2);
        assert!((progress(2, 5.0) - 0.6).abs() < 1e-6);
        assert_eq!(progress(4, 5.0), 1.0);
        assert_eq!(
            progress(9, 5.0),
            1.0,
            "a debug-mode countdown longer than the game's"
        );
        assert!(progress(5, 10.0) < 1.0, "slime takes longer");
        assert_eq!(progress(-3, 5.0), 0.2);
    }

    #[test]
    fn a_hallucinations_tint_turns_the_hue_not_the_brightness() {
        let lum = |v: Vector3| v.dot(Vector3::new(0.2126, 0.7152, 0.0722));
        for i in 0..36 {
            let tint = hue_tint(i as f32 / 36.0, HUE_ALPHA);
            assert!((lum(tint) - 1.0).abs() < 0.12, "{i}: {tint:?}");
            let spread = tint.x.max(tint.y).max(tint.z) - tint.x.min(tint.y).min(tint.z);
            assert!(spread > 0.15, "{i}: {tint:?}");
        }
        assert_eq!(hue_tint(0.3, 0.0), Vector3::ONE);
    }
}
