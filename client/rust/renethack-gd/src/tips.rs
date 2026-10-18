//! The game's tips, one style for all (the HUD's blocks, the panels'
//! buttons, the pages'): a gold-edged plate in the body face at the
//! HUD's size, the words wrapped at `MAX_W` (design pixels, 320 at
//! 1080p), shown a moment after the mouse rests on a block, above it
//! (under it when there is no room above), kept on the screen and off the
//! map's middle where the hero stands; drawn by `TipHost` on a layer of
//! its own over the rest. A block's tip is its metadata `META`, not
//! Godot's `tooltip_text`: Godot's own tooltip is a window of its own,
//! with neither a width nor a place of the game's.

use godot::classes::control::MouseFilter;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{CanvasLayer, Control, Label, PanelContainer};
use godot::prelude::*;

use crate::theme::{self, Face, Frame};

/// The metadata a block's tip is kept in.
pub const META: &str = "hud_tip";
/// The metadata of a root whose blocks' tips keep off the map's middle
/// (the HUD's: the map shows round it; a panel covers it).
const OFF_HERO: &str = "tips_off_hero";
/// The widest a tip grows before it wraps, in design pixels.
pub const MAX_W: f32 = 320.0;
/// The words' size: the log's, the HUD's body text.
pub const SIZE: i32 = 16;
/// Seconds the mouse rests on a block before its tip shows.
pub const DELAY: f64 = 0.35;
/// The gap between a block and its tip.
const GAP: f32 = 8.0;
/// The map's middle the tips keep off (the hero), a half side.
const HERO: f32 = 110.0;

/// `c`'s tip (empty: none).
pub fn set<T: Inherits<Control>>(c: &Gd<T>, text: &str) {
    let mut c = c.clone().upcast::<Control>();
    if c.get_tooltip_text().is_empty() && tip_of(&c).as_deref() == Some(text) {
        return;
    }
    c.set_tooltip_text("");
    c.set_meta(META, &text.to_variant());
}

/// The tips of the blocks under `root` keep off the map's middle, where
/// the hero stands.
pub fn keep_off_hero<T: Inherits<Control>>(root: &Gd<T>) {
    root.clone()
        .upcast::<Control>()
        .set_meta(OFF_HERO, &true.to_variant());
}

/// `c`'s tip from the catalog's `key`, kept in the language.
pub fn bind<T: Inherits<Control>>(c: &Gd<T>, key: &'static str) {
    let c = c.clone().upcast::<Control>();
    crate::i18n::bind(&c, "metadata/hud_tip", key);
}

/// The tip `c` has, if any.
pub fn tip_of(c: &Gd<Control>) -> Option<String> {
    c.has_meta(META)
        .then(|| c.get_meta(META).to::<String>())
        .filter(|t| !t.is_empty())
}

/// The block under the mouse that has a tip: `hovered` or the nearest of
/// its parents (up to its layer); and whether its tip keeps off the map's
/// middle (a root above it says so).
pub fn owner(hovered: Gd<Control>) -> Option<(Gd<Control>, String, bool)> {
    let mut at = Some(hovered);
    let mut found: Option<(Gd<Control>, String)> = None;
    while let Some(c) = at {
        if found.is_none()
            && let Some(t) = tip_of(&c)
        {
            found = Some((c.clone(), t));
        }
        if found.is_some() && c.has_meta(OFF_HERO) {
            return found.map(|(b, t)| (b, t, true));
        }
        at = c.get_parent().and_then(|p| p.try_cast::<Control>().ok());
    }
    found.map(|(b, t)| (b, t, false))
}

/// The controls under `node` with a tip, shown, where they are
/// (self-tests).
pub fn tipped(node: &Gd<Node>) -> Vec<(String, Rect2)> {
    fn walk(node: &Gd<Node>, out: &mut Vec<(String, Rect2)>) {
        if let Ok(c) = node.clone().try_cast::<Control>() {
            if !c.is_visible_in_tree() {
                return;
            }
            if let Some(tip) = tip_of(&c) {
                out.push((tip, c.get_global_rect()));
            }
        }
        for child in node.get_children().iter_shared() {
            walk(&child, out);
        }
    }
    let mut out = Vec::new();
    walk(node, &mut out);
    out
}

/// The one tip on screen: the block the mouse rests on, since when, and
/// the tip shown for it.
pub struct TipHost {
    panel: Gd<PanelContainer>,
    label: Gd<Label>,
    owner: Option<InstanceId>,
    since: f64,
    shown: Option<(InstanceId, String)>,
}

impl TipHost {
    pub fn new(mut layer: Gd<CanvasLayer>) -> TipHost {
        let root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        layer.add_child(&root);
        let (panel, label) = view();
        let mut root = root;
        root.add_child(&panel);
        TipHost {
            panel,
            label,
            owner: None,
            since: 0.0,
            shown: None,
        }
    }

    /// The tip of the block under the mouse (`hovered`, as Godot has it),
    /// a moment after the mouse rests there; gone when it leaves. Every
    /// frame.
    pub fn tick(&mut self, hovered: Option<Gd<Control>>, now: f64) {
        let Some((block, text, off_hero)) = hovered.and_then(owner) else {
            self.owner = None;
            if self.shown.take().is_some() {
                self.panel.set_visible(false);
            }
            return;
        };
        let id = block.instance_id();
        if self.owner != Some(id) {
            // another block: its tip waits a moment
            self.owner = Some(id);
            self.since = now;
            if self.shown.take().is_some() {
                self.panel.set_visible(false);
            }
            return;
        }
        if now - self.since < DELAY || self.shown.as_ref() == Some(&(id, text.clone())) {
            return;
        }
        let size = fill(&mut self.panel, &mut self.label, &text);
        let view = self.panel.get_viewport_rect().size;
        let at = place_for(size, block.get_global_rect(), view, off_hero);
        self.panel.set_position(at);
        self.panel.set_visible(true);
        self.shown = Some((id, text));
    }

    /// The tip on screen: its words and where it is (self-tests).
    pub fn shown(&self) -> Option<(String, Rect2)> {
        self.shown
            .as_ref()
            .filter(|_| self.panel.is_visible())
            .map(|(_, t)| (t.clone(), self.panel.get_global_rect()))
    }
}

/// The plate and its words, hidden.
pub fn view() -> (Gd<PanelContainer>, Gd<Label>) {
    let mut panel = theme::framed(Frame::Tooltip);
    panel.set_mouse_filter(MouseFilter::IGNORE);
    let mut label = theme::styled_label("", Face::Body, SIZE, theme::TEXT);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label.set_autowrap_mode(AutowrapMode::WORD_SMART);
    panel.add_child(&label);
    panel.set_visible(false);
    (panel, label)
}

/// `text` in the plate, as wide as its longest line up to `MAX_W`, as tall
/// as its lines wrap; the plate's size. The label is in the tree.
pub fn fill(panel: &mut Gd<PanelContainer>, label: &mut Gd<Label>, text: &str) -> Vector2 {
    label.set_text(text);
    let font = theme::font(Face::Body);
    let widest = text
        .lines()
        .map(|l| font.get_string_size_ex(l).font_size(SIZE).done().x)
        .fold(0.0f32, f32::max);
    let w = widest.ceil().min(MAX_W);
    label.set_custom_minimum_size(Vector2::new(w, 0.0));
    label.set_size(Vector2::new(w, 4000.0));
    let lines = label.get_line_count().max(1) as f32;
    let spacing = label.get_theme_constant("line_spacing") as f32;
    let h = lines * label.get_line_height() as f32 + (lines - 1.0) * spacing;
    label.set_custom_minimum_size(Vector2::new(w, h.ceil()));
    panel.reset_size();
    panel.get_combined_minimum_size()
}

/// Where a tip of `size` goes for the block at `block` on a screen of
/// `view`: above it (under it when there is no room), its middle over the
/// block's, inside the screen; `off_hero`: off the map's middle too.
pub fn place_for(size: Vector2, block: Rect2, view: Vector2, off_hero: bool) -> Vector2 {
    let x = (block.position.x + block.size.x / 2.0 - size.x / 2.0)
        .clamp(GAP, (view.x - size.x - GAP).max(GAP));
    let above = block.position.y - GAP - size.y;
    let below = block.end().y + GAP;
    let middle = Rect2::new(
        view / 2.0 - Vector2::splat(HERO),
        Vector2::splat(2.0 * HERO),
    );
    let fits = |y: f32| {
        y >= GAP
            && y + size.y <= view.y - GAP
            && !(off_hero && middle.intersects(Rect2::new(Vector2::new(x, y), size)))
    };
    let y = [above, below]
        .into_iter()
        .find(|&y| fits(y))
        .unwrap_or_else(|| above.clamp(GAP, (view.y - size.y - GAP).max(GAP)));
    Vector2::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tip_goes_above_its_block_on_the_screen_off_the_middle() {
        let view = Vector2::new(1920.0, 1080.0);
        let size = Vector2::new(300.0, 60.0);
        // a slot at the foot: above it, centred
        let slot = Rect2::new(Vector2::new(604.0, 992.0), Vector2::new(64.0, 64.0));
        let at = place_for(size, slot, view, true);
        assert_eq!(at, Vector2::new(486.0, 924.0));
        // at the screen's right edge: pulled in
        let orb = Rect2::new(Vector2::new(1850.0, 920.0), Vector2::new(60.0, 148.0));
        assert_eq!(place_for(size, orb, view, true).x, 1920.0 - 300.0 - 8.0);
        // a block at the top: under it
        let portrait = Rect2::new(Vector2::new(24.0, 4.0), Vector2::new(520.0, 176.0));
        assert_eq!(place_for(size, portrait, view, true).y, 188.0);
        // no room above, and under it the hero: kept at the top instead
        let tall = Rect2::new(Vector2::new(900.0, 20.0), Vector2::new(120.0, 380.0));
        let at = place_for(size, tall, view, true);
        assert_eq!(at.y, 8.0, "{at:?}");
        // just under the middle: a HUD block's tip goes under it, a panel's
        // above (the panel covers the map, the hero with it)
        let tab = Rect2::new(Vector2::new(830.0, 660.0), Vector2::new(42.0, 36.0));
        assert_eq!(place_for(size, tab, view, false).y, 660.0 - 8.0 - 60.0);
        assert_eq!(place_for(size, tab, view, true).y, 660.0 + 36.0 + 8.0);
    }
}
