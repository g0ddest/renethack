//! The prologue's words over its scenes (`prologue::Run` plays it): each
//! beat's text in the title's typeface, in lines centred low on the
//! screen over a soft dark band, and a darker spot right behind them, as
//! dark over all the lines and fading out past them (the words read on
//! Gehennom's lava as on the Book's dark), coming as the run shows its
//! characters (the Book's as if written, letter by letter; the rest line
//! by line); the black between the beats; a quiet note of the keys in a
//! corner.

use godot::builtin::Side;
use godot::classes::control::MouseFilter;
use godot::classes::gradient_texture_2d::Fill;
use godot::classes::{
    CanvasLayer, ColorRect, Control, Gradient, GradientTexture2D, Label, NinePatchRect,
    TextureRect, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use crate::prologue::{Beat, Run};
use crate::theme::{self, Face};
use crate::tr;

/// The words' size in design pixels, the share of the screen's width a
/// line may take, and how far above the bottom the words end.
const SIZE: i32 = 34;
const WIDTH: f32 = 0.66;
const BOTTOM: f32 = 0.1;
/// The parchment the words are written in.
const INK: Color = Color::from_rgb(0.95, 0.89, 0.76);
/// The dark behind the words: how dark it is over them, how far past
/// them it stays that dark (design pixels, across and up and down), and
/// how far beyond that it fades out.
const PLATE_DARK: f32 = 0.6;
const PLATE_PAD: (f32, f32) = (24.0, 8.0);
const PLATE_FADE: i32 = 120;

pub struct PrologueView {
    root: Gd<Control>,
    shade: Gd<ColorRect>,
    words: Gd<VBoxContainer>,
    /// The lines laid out now: each label and the range of the text's
    /// characters it holds.
    lines: Vec<(Gd<Label>, usize, usize)>,
    hint: Gd<Label>,
    /// The dark right behind the words, and their widest line's width.
    plate: Gd<NinePatchRect>,
    widest: f32,
    /// What the lines are laid out for: the beat, its text, the width.
    laid: Option<(Beat, String, i32)>,
}

impl PrologueView {
    pub fn new(mut layer: Gd<CanvasLayer>) -> PrologueView {
        let mut root = Control::new_alloc();
        root.set_name("Prologue");
        theme::place(&root, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        // it takes the clicks while it plays: none reaches the game
        root.set_mouse_filter(MouseFilter::STOP);
        // a soft dark band under the words, rising from the bottom
        let mut gradient = Gradient::new_gd();
        gradient.set_color(0, Color::from_rgba(0.0, 0.0, 0.0, 0.0));
        gradient.set_color(1, Color::from_rgba(0.0, 0.0, 0.0, 0.72));
        let mut tex = GradientTexture2D::new_gd();
        tex.set_gradient(&gradient);
        tex.set_fill_from(Vector2::new(0.0, 0.0));
        tex.set_fill_to(Vector2::new(0.0, 1.0));
        let mut band = TextureRect::new_alloc();
        band.set_texture(&tex);
        band.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        band.set_mouse_filter(MouseFilter::IGNORE);
        theme::place(&band, [0.0, 0.5, 1.0, 1.0], [0.0; 4]);
        root.add_child(&band);
        // and right behind the words, as dark over all of them and fading
        // out past them: strong enough over the brightest scene, lost in
        // the dark ones
        let plate = plate();
        root.add_child(&plate);
        let mut words = VBoxContainer::new_alloc();
        words.set_mouse_filter(MouseFilter::IGNORE);
        words.set_alignment(godot::classes::box_container::AlignmentMode::END);
        words.add_theme_constant_override("separation", 6);
        theme::place(&words, [0.0, 0.3, 1.0, 1.0 - BOTTOM], [0.0; 4]);
        root.add_child(&words);
        let mut hint = theme::styled_label(&tr!("prologue-hint"), Face::Body, 17, theme::TEXT_DIM);
        hint.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        theme::outline(&hint, 4);
        theme::place(&hint, [0.5, 1.0, 1.0, 1.0], [0.0, -40.0, -28.0, -12.0]);
        root.add_child(&hint);
        let mut shade = ColorRect::new_alloc();
        shade.set_color(Color::BLACK);
        shade.set_mouse_filter(MouseFilter::IGNORE);
        theme::place(&shade, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        root.add_child(&shade);
        root.set_visible(false);
        layer.add_child(&root);
        PrologueView {
            root,
            shade,
            words,
            lines: Vec::new(),
            hint,
            plate,
            widest: 0.0,
            laid: None,
        }
    }

    pub fn set_visible(&mut self, on: bool) {
        self.root.set_visible(on);
        if !on {
            self.laid = None;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.root.is_visible()
    }

    /// The black alone over everything (the game coming up out of it
    /// after the prologue): `alpha` 1 in black.
    pub fn set_black(&mut self, alpha: f32) {
        self.shade.set_color(Color::from_rgba(0.0, 0.0, 0.0, alpha));
        self.words.set_visible(false);
        self.hint.set_visible(false);
        self.plate.set_visible(false);
    }

    /// The text in the language now (after a switch).
    pub fn relang(&mut self) {
        self.hint.set_text(&tr!("prologue-hint"));
        self.laid = None;
    }

    /// Show the run as it is now: its beat's words as far as they have
    /// come, the black.
    pub fn update(&mut self, run: &Run) {
        self.words.set_visible(true);
        self.hint.set_visible(true);
        self.shade
            .set_color(Color::from_rgba(0.0, 0.0, 0.0, run.shade()));
        let width = (self.root.get_viewport_rect().size.x * WIDTH) as i32;
        let key = (run.beat(), run.text().to_string(), width);
        if self.laid.as_ref() != Some(&key) {
            self.lay(run.text(), width);
            self.laid = Some(key);
        }
        let shown = run.shown_chars();
        let written = run.beat() == Beat::Book;
        for (label, start, len) in &mut self.lines {
            let into = shown.saturating_sub(*start).min(*len);
            if written {
                // as if written: letter by letter
                label.set_visible_characters(into as i32);
                label.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, 1.0));
            } else {
                // a line comes in as the words reach it
                let a = (into as f32 / (*len).max(1) as f32 * 1.6).min(1.0);
                label.set_visible_characters(-1);
                label.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, a));
            }
        }
        self.words
            .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, run.words_alpha()));
        self.place_plate(run.words_alpha());
    }

    /// The dark behind the words where they are now: as dark from their
    /// widest line's ends and their top and bottom out to `PLATE_PAD`,
    /// fading out `PLATE_FADE` beyond; it fades with them.
    fn place_plate(&mut self, alpha: f32) {
        let (Some(first), Some(last)) = (self.lines.first(), self.lines.last()) else {
            self.plate.set_visible(false);
            return;
        };
        let (top, bottom) = (
            first.0.get_global_rect().position.y,
            last.0.get_global_rect().end().y,
        );
        let view = self.root.get_viewport_rect().size;
        let fade = PLATE_FADE as f32;
        let half = self.widest / 2.0 + PLATE_PAD.0 + fade;
        let over = PLATE_PAD.1 + fade;
        self.plate
            .set_global_position(Vector2::new(view.x / 2.0 - half, top - over));
        self.plate
            .set_size(Vector2::new(2.0 * half, bottom - top + 2.0 * over));
        self.plate
            .set_modulate(Color::from_rgba(1.0, 1.0, 1.0, alpha));
        self.plate.set_visible(true);
    }

    /// The beat's text laid out in lines of at most `width` pixels.
    fn lay(&mut self, text: &str, width: i32) {
        for (mut label, _, _) in self.lines.drain(..) {
            label.queue_free();
        }
        let font = theme::font(Face::Title);
        let measure = |s: &str| font.get_string_size_ex(s).font_size(SIZE).done().x;
        let mut start = 0;
        let lines = wrap(text, |s| measure(s) <= width as f32);
        self.widest = lines.iter().map(|l| measure(l)).fold(0.0, f32::max);
        for line in lines {
            let len = line.chars().count();
            let mut label = theme::styled_label(&line, Face::Title, SIZE, INK);
            label.set_horizontal_alignment(HorizontalAlignment::CENTER);
            theme::outline(&label, 6);
            crate::i18n::verbatim(&label);
            self.words.add_child(&label);
            self.lines.push((label, start, len));
            // (the space between this line and the next is the next's)
            start += len + 1;
        }
    }

    /// Where the lines laid out now are on the canvas (self-tests).
    pub fn line_rects(&self) -> Vec<Rect2> {
        self.lines
            .iter()
            .map(|(l, _, _)| l.get_global_rect())
            .collect()
    }

    /// The lines laid out now (self-tests).
    pub fn lines(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|(l, _, _)| l.get_text().to_string())
            .collect()
    }
}

/// The dark behind the words: a round soft spot, its middle stretched
/// over the lines and its rim `PLATE_FADE` wide, falling off as a
/// smoothstep (no edge where the fade starts).
fn plate() -> Gd<NinePatchRect> {
    let stops = [
        (0.0, 1.0),
        (0.25, 0.84),
        (0.5, 0.5),
        (0.75, 0.16),
        (1.0, 0.0),
    ];
    let offsets: Vec<f32> = stops.iter().map(|s| s.0).collect();
    let colors: Vec<Color> = stops
        .iter()
        .map(|s| Color::from_rgba(0.0, 0.0, 0.0, PLATE_DARK * s.1))
        .collect();
    let mut g = Gradient::new_gd();
    g.set_offsets(&PackedFloat32Array::from(&offsets[..]));
    g.set_colors(&PackedColorArray::from(&colors[..]));
    let mut t = GradientTexture2D::new_gd();
    t.set_gradient(&g);
    // the rims and one texel of middle between them, to stretch
    t.set_width(2 * PLATE_FADE + 1);
    t.set_height(2 * PLATE_FADE + 1);
    t.set_fill(Fill::RADIAL);
    t.set_fill_from(Vector2::new(0.5, 0.5));
    t.set_fill_to(Vector2::new(1.0, 0.5));
    let mut plate = NinePatchRect::new_alloc();
    plate.set_texture(&t);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        plate.set_patch_margin(side, PLATE_FADE);
    }
    plate.set_mouse_filter(MouseFilter::IGNORE);
    plate
}

/// `text` in lines of whole words, each as long as `fits` lets it be (a
/// word longer than that alone on its line).
fn wrap(text: &str, fits: impl Fn(&str) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut now = String::new();
    for word in text.split_whitespace() {
        if now.is_empty() {
            now.push_str(word);
            continue;
        }
        let longer = format!("{now} {word}");
        if fits(&longer) {
            now = longer;
        } else {
            lines.push(std::mem::replace(&mut now, word.to_string()));
        }
    }
    if !now.is_empty() {
        lines.push(now);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_wraps_by_whole_words_and_keeps_them_all() {
        let text = "It is written in the Book of Tyr: and more words than fit";
        let lines = wrap(text, |s| s.chars().count() <= 20);
        assert!(lines.iter().all(|l| l.chars().count() <= 20), "{lines:?}");
        assert_eq!(lines.join(" "), text);
        // a word too long for a line stands alone
        assert_eq!(
            wrap("a Brobdingnagian b", |s| s.len() <= 4),
            vec!["a", "Brobdingnagian", "b"]
        );
    }
}
