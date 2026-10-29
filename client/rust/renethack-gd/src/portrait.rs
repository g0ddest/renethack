//! The HUD's portrait: the hero's head and shoulders, drawn by a camera
//! of its own in the map's world, as the inventory's doll is (it sees the
//! hero's layer and the doll's lamps, on a clear background), into a
//! texture the medallion shows in its face. It renders once each time
//! the hero's look changes (another model: a polymorph, a new game; their
//! gear), when the new look has held still a moment, and not otherwise:
//! a still picture, cheap on the Deck.

use godot::classes::sub_viewport::UpdateMode;
use godot::classes::{Camera3D, Control, Environment, Node3D, SubViewport, Texture2D};
use godot::prelude::*;
use nh_art::Gear;

use crate::map_view::DOLL_LAYER;

/// The layer the hero's model is drawn on (the doll's camera sees it).
const HERO_LAYER: u32 = 1 << 1;
/// The render's side, in pixels: twice the medallion's face at the HUD's
/// design size, for the larger UI scales.
const PORTRAIT_PX: i32 = 256;
/// The camera's angle of view.
const FOV: f32 = 20.0;
/// How much of the hero's height the portrait frames (head and
/// shoulders), and how far round from straight ahead it looks at them,
/// and from how far above. The hero in their own form is as tall as
/// every role's model (a tall hat or a plume aside); a form, as its
/// model stands.
const FRAMED: f32 = 0.4;
const HERO_TALL: f32 = 1.1;
const TURN_DEG: f32 = 20.0;
const PITCH_DEG: f32 = 6.0;
/// A form with no head to frame is framed whole, this much round it.
const FORM_MARGIN: f32 = 1.1;
/// A new look renders once it has held this long (its gear on the model,
/// a polymorph's model grown in) and the hero stands still: how far the
/// head may move between frames.
const SETTLE_SECS: f64 = 0.35;
const STILL: f32 = 0.02;

/// What the portrait shows: the hero's model, their gear, polymorphed.
#[derive(Debug, Clone, PartialEq)]
struct Look {
    model: InstanceId,
    gear: Gear,
    form: bool,
}

pub struct PortraitView {
    viewport: Gd<SubViewport>,
    camera: Gd<Camera3D>,
    /// The look rendered, the one wanted and since when, where the
    /// camera aimed last frame.
    shown: Option<Look>,
    wanted: Option<Look>,
    since: f64,
    aimed: Option<Vector3>,
    /// The model's bounds in its own frame, measured once a model.
    bounds: Option<(InstanceId, Aabb)>,
    /// Renders made, and whether one was made last frame (self-tests).
    renders: u32,
}

impl PortraitView {
    pub fn new(parent: &mut Gd<Control>) -> PortraitView {
        let mut viewport = SubViewport::new_alloc();
        viewport.set_size(Vector2i::new(PORTRAIT_PX, PORTRAIT_PX));
        viewport.set_transparent_background(true);
        viewport.set_update_mode(UpdateMode::DISABLED);
        let mut camera = Camera3D::new_alloc();
        // the hero, and the doll's lamps (they hang on the doll's camera,
        // which always looks at the hero from before them)
        camera.set_cull_mask(HERO_LAYER | DOLL_LAYER);
        camera.set_fov(FOV);
        camera.set_near(0.05);
        let mut env = Environment::new_gd();
        env.set_background(godot::classes::environment::BgMode::CLEAR_COLOR);
        env.set_ambient_source(godot::classes::environment::AmbientSource::COLOR);
        env.set_ambient_light_color(Color::from_rgb(0.52, 0.52, 0.56));
        env.set_ambient_light_energy(0.4);
        env.set_tonemapper(godot::classes::environment::ToneMapper::AGX);
        env.set_tonemap_exposure(1.25);
        camera.set_environment(&env);
        viewport.add_child(&camera);
        parent.add_child(&viewport);
        PortraitView {
            viewport,
            camera,
            shown: None,
            wanted: None,
            since: 0.0,
            aimed: None,
            bounds: None,
            renders: 0,
        }
    }

    pub fn texture(&self) -> Gd<Texture2D> {
        self.viewport
            .get_texture()
            .expect("a viewport has a texture")
            .upcast()
    }

    /// Every frame: the hero's model (None: none to show), the gear it
    /// shows, whether they are polymorphed. True while there is a render
    /// of the hero to show.
    pub fn sync(&mut self, hero: Option<Gd<Node3D>>, gear: &Gear, form: bool, now: f64) -> bool {
        let hero = hero.filter(|h| h.is_instance_valid() && h.is_inside_tree());
        let look = hero.as_ref().map(|h| Look {
            model: h.instance_id(),
            gear: gear.clone(),
            form,
        });
        if look != self.wanted {
            self.wanted = look;
            self.since = now;
        }
        // none a moment (a level arriving, a new game): the last render
        // stays until the next is made
        let Some(hero) = hero else {
            self.aimed = None;
            return self.shown.is_some();
        };
        if self.wanted != self.shown {
            // aimed every frame while a render is due: a camera reaches the
            // renderer a frame after it moves
            let was = self.aimed;
            self.aimed = self.aim(&hero, form);
            let still =
                matches!((was, self.aimed), (Some(a), Some(b)) if a.distance_to(b) <= STILL);
            if still && now - self.since >= SETTLE_SECS {
                self.render();
                self.shown = self.wanted.clone();
            }
        }
        self.shown.is_some()
    }

    fn render(&mut self) {
        // in the 3D view's anti-aliasing, as the doll: Godot builds the
        // pipelines for one, and the map's hero has built these
        if self.viewport.is_inside_tree()
            && let Some(window) = self.viewport.get_tree().get_root()
        {
            let msaa = window.get_msaa_3d();
            if self.viewport.get_msaa_3d() != msaa {
                self.viewport.set_msaa_3d(msaa);
            }
        }
        self.viewport.set_update_mode(UpdateMode::ONCE);
        self.renders += 1;
    }

    /// Aim at the hero's head and shoulders, a little round from straight
    /// ahead and above (a form without a head: at all of it); where it
    /// aims, None when there is nothing to aim at.
    fn aim(&mut self, hero: &Gd<Node3D>, form: bool) -> Option<Vector3> {
        // the model's own node, under the one the map places: its yaw
        let inner = hero.get_child(0)?.try_cast::<Node3D>().ok()?;
        let to = inner.get_global_transform();
        let id = inner.instance_id();
        let bounds = match self.bounds {
            Some((was, b)) if was == id => Some(b),
            _ => {
                let b = crate::art::posed_bounds(&inner)?;
                self.bounds = Some((id, b));
                Some(b)
            }
        }
        .map(|b| to * b);
        let height = match bounds {
            Some(b) if form => b.size.y.max(0.05),
            _ => HERO_TALL,
        };
        let face = to.basis.col_c().normalized();
        let turn = Basis::from_axis_angle(Vector3::UP, TURN_DEG.to_radians());
        let pitch = PITCH_DEG.to_radians();
        let back = (turn * face) * pitch.cos() + Vector3::UP * pitch.sin();
        let half = (FOV.to_radians() / 2.0).tan();
        let head = crate::art::bone_at(&inner, "Head");
        let (target, span) = match (head, bounds) {
            // the head and the shoulders under it: the head's bone is at the
            // neck, the frame's middle
            (Some(head), _) => (head, height * FRAMED),
            (None, Some(b)) => (
                b.position + b.size / 2.0,
                b.size.y.max(b.size.x).max(b.size.z) * FORM_MARGIN,
            ),
            _ => return None,
        };
        let distance = span / 2.0 / half;
        let eye = target + back * distance;
        self.camera
            .set_global_transform(Transform3D::new(Basis::IDENTITY, eye).looking_at(target));
        Some(target)
    }

    /// Renders made since the start (self-tests: it renders on a change,
    /// not every frame).
    pub fn renders(&self) -> u32 {
        self.renders
    }

    /// The box round what the render draws, in shares of its side from
    /// its top left corner; None without a renderer, or with nothing
    /// drawn (self-tests).
    pub fn drawn_box(&self) -> Option<Rect2> {
        let image = self.viewport.get_texture()?.get_image()?;
        let (w, h) = (image.get_width(), image.get_height());
        let (mut lo, mut hi) = (Vector2i::new(w, h), Vector2i::new(-1, -1));
        for y in (0..h).step_by(2) {
            for x in (0..w).step_by(2) {
                if image.get_pixel(x, y).a > 0.5 {
                    lo = Vector2i::new(lo.x.min(x), lo.y.min(y));
                    hi = Vector2i::new(hi.x.max(x), hi.y.max(y));
                }
            }
        }
        (hi.x >= lo.x).then(|| {
            let (w, h) = (w as f32, h as f32);
            Rect2::new(
                Vector2::new(lo.x as f32 / w, lo.y as f32 / h),
                Vector2::new((hi.x - lo.x) as f32 / w, (hi.y - lo.y) as f32 / h),
            )
        })
    }
}
