//! The HUD's tooltips, one style for all: a gold-edged plate in the body
//! face at the HUD's size, the words wrapped at `MAX_W` (design pixels,
//! 320 at 1080p), shown a moment after the mouse rests on a block, above
//! it (under one at the top), kept on the screen and off the map's middle
//! where the hero stands. A block's tip is its metadata `META`, not
//! Godot's `tooltip_text`: Godot's own tooltip has neither a width nor a
//! place of the HUD's.

use godot::classes::control::MouseFilter;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Control, Label, PanelContainer};
use godot::prelude::*;

use crate::theme::{self, Face, Frame};

/// The metadata a block's tip is kept in.
pub const META: &str = "hud_tip";
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
/// its parents, up to `root`.
pub fn owner(hovered: Gd<Control>, root: &Gd<Control>) -> Option<(Gd<Control>, String)> {
    let mut at = Some(hovered);
    while let Some(c) = at {
        if let Some(t) = tip_of(&c) {
            return Some((c, t));
        }
        if c == *root {
            return None;
        }
        at = c.get_parent().and_then(|p| p.try_cast::<Control>().ok());
    }
    None
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
/// block's, inside the screen, off the map's middle.
pub fn place_for(size: Vector2, block: Rect2, view: Vector2) -> Vector2 {
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
            && !middle.intersects(Rect2::new(Vector2::new(x, y), size))
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
        let at = place_for(size, slot, view);
        assert_eq!(at, Vector2::new(486.0, 924.0));
        // at the screen's right edge: pulled in
        let orb = Rect2::new(Vector2::new(1850.0, 920.0), Vector2::new(60.0, 148.0));
        assert_eq!(place_for(size, orb, view).x, 1920.0 - 300.0 - 8.0);
        // a block at the top: under it
        let portrait = Rect2::new(Vector2::new(24.0, 4.0), Vector2::new(520.0, 176.0));
        assert_eq!(place_for(size, portrait, view).y, 188.0);
        // no room above, and under it the hero: kept at the top instead
        let tall = Rect2::new(Vector2::new(900.0, 20.0), Vector2::new(120.0, 380.0));
        let at = place_for(size, tall, view);
        assert_eq!(at.y, 8.0, "{at:?}");
    }
}
