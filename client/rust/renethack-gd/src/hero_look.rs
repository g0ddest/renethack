//! What the hero's body shows of the conditions on the status line (the
//! player reads them there: showing them leaks nothing). Turning to stone
//! grey stone creeps over the hero in patches, turning to slime green
//! slime does, wet and faintly glowing, more of them each turn of the
//! countdown (five for stone, ten for slime), all of the body at its end.
//! A child of map_view: it lays its skin over the hero's own model (the
//! rider's while they ride), under the see-through look that shows them
//! behind a wall.

use godot::classes::base_material_3d::{Feature, Flags, TextureParam, Transparency};
use godot::classes::fast_noise_lite::NoiseType;
use godot::classes::{FastNoiseLite, Gradient, NoiseTexture2D, StandardMaterial3D};
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::World;

use super::{MapView, set_overlay};

/// A condition the hero's body shows, as the status line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shown {
    Stone,
    Slime,
}

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
const STONE: Color = Color::from_rgb(0.4, 0.4, 0.39);
const SLIME: Color = Color::from_rgb(0.3, 0.75, 0.2);
const SLIME_GLOW: Color = Color::from_rgb(0.06, 0.22, 0.03);

/// The state shown now.
pub(super) struct HeroLook {
    /// The catalog's masks of the conditions shown.
    masks: Vec<(u64, Shown)>,
    /// The turn each condition under way began.
    since: Vec<(Shown, i64)>,
    stone: Gd<StandardMaterial3D>,
    slime: Gd<StandardMaterial3D>,
    /// The tint on the hero's model now: which model, which tint.
    tinted: Option<(InstanceId, Option<Shown>)>,
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
        stone.set_roughness(1.0);
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
        }
    }

    /// A new game: nothing under way.
    pub(super) fn forget(&mut self) {
        self.since.clear();
        self.tinted = None;
    }

    /// The tint to lay over the hero now (the stronger countdown's, stone
    /// before slime), None: none.
    pub(super) fn overlay(&self) -> Option<Gd<godot::classes::Material>> {
        self.tinted.and_then(|(_, s)| s).map(|s| match s {
            Shown::Stone => self.stone.clone().upcast(),
            Shown::Slime => self.slime.clone().upcast(),
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
    pub(super) fn look_hero(&mut self, world: &World, catalog: &Catalog) {
        let look = &mut self.hero_look;
        if look.masks.is_empty() {
            look.masks = Shown::ALL
                .iter()
                .filter_map(|&s| {
                    let c = catalog.conditions.iter().find(|c| c.name == s.name())?;
                    Some((c.mask, s))
                })
                .collect();
        }
        let conds = world.status.conditions();
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
        let want = shown.map(|(s, _)| s);
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
}
