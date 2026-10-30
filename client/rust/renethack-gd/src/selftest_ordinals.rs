//! `ordinals`: a number keeps its Russian ending on its line ("1-й"
//! broke as "1-" at a line's end and "й уровень" at the next one's
//! start). Laid out by Godot's own text server in the body face at a width
//! where the line must break by the number: with the word joiner the
//! translator puts after the hyphen (`engine_text::glue_ordinals`) the
//! break comes before "1-й", without it, after "1-".

use godot::classes::TextParagraph;
use godot::prelude::*;

use super::Step;
use crate::theme::{self, Face};

/// Where Godot breaks `text` laid out `width` pixels wide: the index
/// (in chars) each line after the first starts at.
fn breaks(text: &str, width: f32) -> Vec<i32> {
    let mut p = TextParagraph::new_gd();
    p.add_string(text, &theme::font(Face::Body), 17);
    p.set_width(width);
    (1..p.get_line_count())
        .map(|i| p.get_line_range(i).x)
        .collect()
}

pub(super) fn ordinals() -> Vec<Step> {
    vec![Step::Call("the number keeps its ending", |_| {
        let plain = "у вас был 1-й уровень";
        let glued = crate::engine_text::glue_ordinals(plain);
        // as wide as "у вас был 1-": a break must come by the number
        let font = theme::font(Face::Body);
        let upto = font
            .get_string_size_ex("у вас был 1-")
            .font_size(17)
            .done()
            .x;
        let width = upto + 2.0;
        let ending = plain.chars().position(|c| c == 'й').ok_or("no ending")? as i32;
        let loose = breaks(plain, width);
        if !loose.contains(&ending) {
            return Err(format!("unglued, no break at the ending: {loose:?}"));
        }
        let held = breaks(&glued, width);
        let ending = glued.chars().position(|c| c == 'й').ok_or("no ending")? as i32;
        if held.iter().any(|&b| b == ending || b == ending - 1) {
            return Err(format!("glued, still broken by the ending: {held:?}"));
        }
        godot_print!("selftest: ordinals: breaks {loose:?} unglued, {held:?} glued");
        Ok(())
    })]
}
