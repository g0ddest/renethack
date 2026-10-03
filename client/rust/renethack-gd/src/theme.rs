//! NetHack colours, the fonts, the colour tokens and the dark-fantasy
//! theme (ui-design §6): forged frames, parchment text, gold trim.

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter};
use godot::classes::image::Format;
use godot::classes::text_server::SpacingType;
use godot::classes::window::{ContentScaleAspect, ContentScaleMode};
use godot::classes::{
    Button, Control, DisplayServer, Font, FontFile, FontVariation, Image, ImageTexture, Label, Os,
    PanelContainer, ProjectSettings, Shader, ShaderMaterial, StyleBox, StyleBoxEmpty, StyleBoxFlat,
    StyleBoxTexture, SystemFont, TextServerManager, Theme, Window,
};
use godot::prelude::*;

use crate::ui_events::{UiEvent, UiQueue, push};

/// NetHack's CLR_* colours 0..15; NO_COLOR (8) shows as gray. Black is
/// lifted so it stays visible on the dark background.
const PALETTE: [(u8, u8, u8); 16] = [
    (85, 85, 110),   // black
    (205, 40, 40),   // red
    (40, 180, 40),   // green
    (170, 110, 30),  // brown
    (60, 90, 230),   // blue
    (190, 60, 190),  // magenta
    (30, 175, 185),  // cyan
    (190, 190, 190), // gray
    (190, 190, 190), // no colour
    (255, 140, 30),  // orange
    (90, 255, 90),   // bright green
    (255, 240, 60),  // yellow
    (110, 130, 255), // bright blue
    (255, 110, 255), // bright magenta
    (100, 255, 255), // bright cyan
    (255, 255, 255), // white
];

/// A NetHack colour (attribute bits above the low byte are ignored).
pub fn nh_color(c: i32) -> Color {
    let (r, g, b) = usize::try_from(c & 0xff)
        .ok()
        .and_then(|i| PALETTE.get(i).copied())
        .unwrap_or(PALETTE[8]);
    Color::from_rgba8(r, g, b, 255)
}

/// A colour from its "#rrggbb" hex value.
const fn rgb(hex: u32) -> Color {
    Color::from_rgb(
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    )
}

const fn alpha(c: Color, a: f32) -> Color {
    Color::from_rgba(c.r, c.g, c.b, a)
}

// ---- colour tokens (ui-design §6.2) ----

/// Screen-space backgrounds, letter pills.
pub const INK: Color = rgb(0x0b0908);
pub const PANEL_TOP: Color = rgb(0x1b140f);
pub const PANEL_BOTTOM: Color = rgb(0x0e0b09);
/// Cells and slots.
pub const SOCKET: Color = rgb(0x0d0a08);
/// The outer frame line.
pub const IRON_DARK: Color = rgb(0x070504);
/// Frames and cell borders.
pub const IRON: Color = rgb(0x3a2f25);
/// Top-left bevel highlight.
pub const BEVEL_LIGHT: Color = rgb(0x6b5638);
/// Idle trim.
pub const GOLD_DIM: Color = rgb(0x6e5328);
/// Hover, trim.
pub const GOLD: Color = rgb(0xb8893b);
/// Selected, focus, titles.
pub const GOLD_BRIGHT: Color = rgb(0xe7c27a);
/// Parchment body text.
pub const TEXT: Color = rgb(0xe8dcc4);
pub const TEXT_DIM: Color = rgb(0x9c8f7a);
/// Disabled.
pub const TEXT_OFF: Color = rgb(0x5b5247);
pub const HP_DEEP: Color = rgb(0x8a0f12);
pub const HP: Color = rgb(0xd8261e);
pub const PW_DEEP: Color = rgb(0x12307a);
pub const PW: Color = rgb(0x3f7fe0);
pub const XP: Color = rgb(0xd9a441);
pub const GOOD: Color = rgb(0x7fb069);
pub const WARN: Color = rgb(0xe39a2e);
pub const DANGER: Color = rgb(0xff3b24);
/// The world dimmed behind panels.
pub const SHADE: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.55);

/// Behind the title and other screens.
pub const BG: Color = INK;
/// What stands out and can be acted on (letters, titles, the default answer).
pub const ACCENT: Color = GOLD_BRIGHT;
/// Dark outlines under text drawn over the world.
pub const OUTLINE: Color = Color::from_rgba(0.02, 0.015, 0.01, 0.92);

/// Body text size in design pixels.
pub const FONT_SIZE: i32 = 17;
/// Raw engine text (menus, text windows) in PT Mono.
pub const MONO_SIZE: i32 = 16;

// ---- fonts (ui-design §6.3) ----

/// The UI's typefaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Cormorant SC SemiBold: titles, panel headers.
    Title,
    /// Cormorant SC Bold: the big words (the mode banner).
    TitleBold,
    /// Alegreya Sans Medium: body, log, tooltips.
    Body,
    /// Alegreya Sans Bold: buttons, emphasis.
    BodyBold,
    /// Alegreya Sans Black: the numbers on the orbs.
    Black,
    /// Alegreya Sans SC Bold: key labels, small caps.
    Caps,
    /// PT Mono: raw engine text.
    Mono,
    MonoBold,
    MonoItalic,
}

impl Face {
    fn file(self) -> &'static str {
        match self {
            Face::Title => "cormorant-sc/CormorantSC-SemiBold.ttf",
            Face::TitleBold => "cormorant-sc/CormorantSC-Bold.ttf",
            Face::Body => "alegreya-sans/AlegreyaSans-Medium.ttf",
            Face::BodyBold => "alegreya-sans/AlegreyaSans-Bold.ttf",
            Face::Black => "alegreya-sans/AlegreyaSans-Black.ttf",
            Face::Caps => "alegreya-sans-sc/AlegreyaSansSC-Bold.ttf",
            Face::Mono | Face::MonoBold | Face::MonoItalic => "pt-mono/PTM55FT.ttf",
        }
    }
}

const SANS_FONTS: [&str; 5] = [
    "PT Sans",
    "Noto Sans",
    "Helvetica Neue",
    "Arial",
    "sans-serif",
];

fn system_font(names: &[&str]) -> Gd<SystemFont> {
    let mut font = SystemFont::new_gd();
    let names: Vec<GString> = names.iter().map(|n| GString::from(*n)).collect();
    font.set_font_names(&PackedStringArray::from(names.as_slice()));
    font
}

thread_local! {
    /// The faces made so far: each new font has its own glyph cache, so a
    /// label given a fresh one shapes and rasterises its text from scratch
    /// (milliseconds a label).
    static FONTS: std::cell::RefCell<std::collections::HashMap<Face, Gd<Font>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Let the fonts go while Godot still runs (as `icons::clear`).
pub fn clear_fonts() {
    FONTS.with(|f| f.borrow_mut().clear());
}

/// A face of the UI; a system font when the file cannot be loaded, and
/// system fonts behind it for characters it lacks. Made once a face.
pub fn font(face: Face) -> Gd<Font> {
    if let Some(f) = FONTS.with(|f| f.borrow().get(&face).cloned()) {
        return f;
    }
    let f = make_font(face);
    FONTS.with(|fs| fs.borrow_mut().insert(face, f.clone()));
    f
}

fn make_font(face: Face) -> Gd<Font> {
    let path = format!("res://fonts/{}", face.file());
    let base: Option<Gd<Font>> = godot::tools::try_load::<FontFile>(&path)
        .ok()
        .map(|f| f.upcast());
    let mono = matches!(face, Face::Mono | Face::MonoBold | Face::MonoItalic);
    let fallback: Gd<Font> = if mono {
        mono_font().upcast()
    } else {
        system_font(&SANS_FONTS).upcast()
    };
    let Some(base) = base else {
        return fallback;
    };
    let mut v = FontVariation::new_gd();
    v.set_base_font(&base);
    match face {
        // tabular figures: numbers that change keep their place
        Face::Body | Face::BodyBold | Face::Black | Face::Caps => {
            if let Some(ts) = TextServerManager::singleton().get_primary_interface() {
                let mut features = VarDictionary::new();
                features.set(ts.name_to_tag("tnum"), 1);
                v.set_opentype_features(&features);
            }
        }
        Face::Title | Face::TitleBold => v.set_spacing(SpacingType::GLYPH, 1),
        Face::MonoBold => v.set_variation_embolden(0.7),
        Face::MonoItalic => v.set_variation_transform(Transform2D::from_cols(
            Vector2::new(1.0, 0.0),
            Vector2::new(0.2, 1.0),
            Vector2::ZERO,
        )),
        Face::Mono => {}
    }
    let mut fallbacks: Array<Gd<Font>> = Array::new();
    fallbacks.push(&fallback);
    v.set_fallbacks(&fallbacks);
    v.upcast()
}

const MONO_FONTS: [&str; 5] = [
    "DejaVu Sans Mono",
    "Menlo",
    "Consolas",
    "Liberation Mono",
    "monospace",
];

/// A monospace system font (first one installed from the list): the map's
/// 3D letters.
pub fn mono_font() -> Gd<SystemFont> {
    system_font(&MONO_FONTS)
}

/// The same font in bold.
pub fn mono_bold() -> Gd<SystemFont> {
    let mut font = mono_font();
    font.set_font_weight(700);
    font
}

/// The advance of one character of PT Mono at `MONO_SIZE`, and its line
/// height, in pixels (typical values when the font cannot tell).
pub fn mono_metrics() -> (f32, f32) {
    let font = font(Face::Mono);
    let w = font.get_char_size('M' as u32, MONO_SIZE).x;
    let h = font.get_height_ex().font_size(MONO_SIZE).done();
    let w = if w > 1.0 { w } else { MONO_SIZE as f32 * 0.6 };
    let h = if h > 1.0 { h } else { MONO_SIZE as f32 * 1.2 };
    (w, h)
}

// ---- frames (ui-design §6.4) ----

/// Kinds of forged frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// Dialogs, the message history: opaque, gold trim.
    Panel,
    /// HUD blocks over the world: a little translucent, gold trim.
    Hud,
    /// The prompt banner and the mode badge: dark, gold trim, small.
    Banner,
    /// Slots and the minimap's well: dark, no trim.
    Socket,
    /// Tooltips.
    Tooltip,
}

struct FrameLook {
    top: Color,
    bottom: Color,
    border: Color,
    border_px: f32,
    trim: Option<Color>,
    inner_shadow: f32,
    cut: f32,
    shadow: f32,
    margin: f32,
}

impl Frame {
    fn look(self) -> FrameLook {
        match self {
            Frame::Panel => FrameLook {
                top: alpha(PANEL_TOP, 0.985),
                bottom: alpha(PANEL_BOTTOM, 0.985),
                border: IRON,
                border_px: 2.0,
                trim: Some(alpha(GOLD_DIM, 0.85)),
                inner_shadow: 14.0,
                cut: 6.0,
                shadow: 14.0,
                margin: 20.0,
            },
            Frame::Hud => FrameLook {
                top: alpha(PANEL_TOP, 0.9),
                bottom: alpha(PANEL_BOTTOM, 0.9),
                border: IRON,
                border_px: 2.0,
                trim: Some(alpha(GOLD_DIM, 0.8)),
                inner_shadow: 12.0,
                cut: 5.0,
                shadow: 10.0,
                margin: 12.0,
            },
            Frame::Banner => FrameLook {
                top: alpha(rgb(0x1f1711), 0.95),
                bottom: alpha(rgb(0x100c09), 0.95),
                border: IRON,
                border_px: 1.0,
                trim: Some(alpha(GOLD, 0.7)),
                inner_shadow: 8.0,
                cut: 4.0,
                shadow: 8.0,
                margin: 10.0,
            },
            Frame::Socket => FrameLook {
                top: alpha(SOCKET, 0.92),
                bottom: alpha(rgb(0x14100c), 0.92),
                border: IRON,
                border_px: 1.0,
                trim: None,
                inner_shadow: 10.0,
                cut: 3.0,
                shadow: 0.0,
                margin: 4.0,
            },
            Frame::Tooltip => FrameLook {
                top: alpha(rgb(0x15100c), 0.96),
                bottom: alpha(rgb(0x0f0b08), 0.96),
                border: GOLD_DIM,
                border_px: 1.0,
                trim: None,
                inner_shadow: 6.0,
                cut: 3.0,
                shadow: 8.0,
                margin: 8.0,
            },
        }
    }
}

/// The frame as a StyleBoxFlat: the fallback look (chamfered corners, a
/// border, a drop shadow), and what the shader draws on.
pub fn frame_style(kind: Frame) -> Gd<StyleBoxFlat> {
    let l = kind.look();
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(l.bottom.lerp(l.top, 0.5));
    sb.set_border_width_all(l.border_px as i32);
    sb.set_border_color(l.border);
    sb.set_corner_radius_all(l.cut as i32);
    // one segment per corner: a straight cut, like forged metal
    sb.set_corner_detail(1);
    sb.set_shadow_size(l.shadow as i32);
    sb.set_shadow_color(Color::from_rgba(0.0, 0.0, 0.0, 0.45));
    sb.set_content_margin_all(l.margin);
    sb
}

fn shader(path: &str) -> Option<Gd<Shader>> {
    godot::tools::try_load::<Shader>(path).ok()
}

/// A material with `res://ui/<name>.gdshader`, if it loads.
pub fn ui_material(name: &str) -> Option<Gd<ShaderMaterial>> {
    let shader = shader(&format!("res://ui/{name}.gdshader"))?;
    let mut m = ShaderMaterial::new_gd();
    m.set_shader(&shader);
    Some(m)
}

/// Draw `panel`'s background as a forged frame of `kind`: the frame
/// shader when it loads, the StyleBoxFlat look otherwise.
pub fn apply_frame<T: Inherits<Control>>(panel: &Gd<T>, kind: Frame) {
    let mut c = panel.clone().upcast::<Control>();
    let l = kind.look();
    let mut sb = frame_style(kind);
    let Some(mut m) = ui_material("ui_frame") else {
        c.add_theme_stylebox_override("panel", &sb);
        return;
    };
    // the shader draws the shadow itself, in the expanded rect
    sb.set_shadow_size(0);
    sb.set_corner_radius_all(0);
    sb.set_border_width_all(0);
    sb.set_anti_aliased(false);
    sb.set_bg_color(Color::WHITE);
    sb.set_expand_margin_all(l.shadow);
    c.add_theme_stylebox_override("panel", &sb);
    let trim = l.trim.unwrap_or(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    for (name, v) in [
        ("top_color", l.top.to_variant()),
        ("bottom_color", l.bottom.to_variant()),
        ("outer_color", IRON_DARK.to_variant()),
        ("border_color", l.border.to_variant()),
        ("border_px", l.border_px.to_variant()),
        ("bevel_light", alpha(BEVEL_LIGHT, 0.9).to_variant()),
        ("inner_shadow_px", l.inner_shadow.to_variant()),
        ("corner_cut_px", l.cut.to_variant()),
        ("trim_color", trim.to_variant()),
        (
            "trim_px",
            (if l.trim.is_some() { 1.0f32 } else { 0.0 }).to_variant(),
        ),
        ("trim_inset", (l.border_px + 4.0).to_variant()),
        ("shadow_px", l.shadow.to_variant()),
        ("size", c.get_size().to_variant()),
    ] {
        m.set_shader_parameter(name, &v);
    }
    c.set_material(&m);
    // a size set before the panel entered the tree emits no `resized`:
    // every redraw brings the shader up to date
    let this = c.clone();
    c.signals().draw().connect(move || {
        let size = this.get_size().to_variant();
        if m.get_shader_parameter("size") != size {
            m.set_shader_parameter("size", &size);
        }
    });
}

/// A PanelContainer drawn as a frame of `kind`.
pub fn framed(kind: Frame) -> Gd<PanelContainer> {
    let p = PanelContainer::new_alloc();
    apply_frame(&p, kind);
    p
}

// ---- buttons: generated bevel textures ----

/// A w×h RGBA image of a bevelled plate: vertical gradient `top`→`bottom`,
/// an outer dark line, a `border`, a light line under the top edge and
/// chamfered corners.
fn bevel_image(
    w: i32,
    h: i32,
    top: Color,
    bottom: Color,
    border: Color,
    light: Color,
) -> Gd<Image> {
    let cut = 3;
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x.min(w - 1 - x), y.min(h - 1 - y));
            // distance from the edge along the chamfer
            let depth = dx.min(dy).min(dx + dy - cut);
            let c = if depth < 0 {
                Color::from_rgba(0.0, 0.0, 0.0, 0.0)
            } else if depth == 0 {
                IRON_DARK
            } else if depth == 1 {
                border
            } else if depth == 2 && y < h / 2 {
                light
            } else if depth == 2 {
                bottom.darkened(0.35)
            } else {
                let t = y as f32 / (h - 1).max(1) as f32;
                top.lerp(bottom, t as f64)
            };
            data.extend_from_slice(&[
                (c.r * 255.0).round() as u8,
                (c.g * 255.0).round() as u8,
                (c.b * 255.0).round() as u8,
                (c.a * 255.0).round() as u8,
            ]);
        }
    }
    Image::create_from_data(w, h, false, Format::RGBA8, &PackedByteArray::from(data))
        .expect("a bevel image")
}

fn bevel_style(
    top: Color,
    bottom: Color,
    border: Color,
    light: Color,
    press: bool,
) -> Gd<StyleBox> {
    let Some(tex) =
        ImageTexture::create_from_image(&bevel_image(64, 32, top, bottom, border, light))
    else {
        let mut sb = StyleBoxFlat::new_gd();
        sb.set_bg_color(top.lerp(bottom, 0.5));
        sb.set_border_width_all(1);
        sb.set_border_color(border);
        return sb.upcast();
    };
    let mut sb = StyleBoxTexture::new_gd();
    sb.set_texture(&tex);
    sb.set_texture_margin_all(6.0);
    sb.set_content_margin(Side::LEFT, 16.0);
    sb.set_content_margin(Side::RIGHT, 16.0);
    sb.set_content_margin(Side::TOP, if press { 6.0 } else { 5.0 });
    sb.set_content_margin(Side::BOTTOM, if press { 4.0 } else { 5.0 });
    sb.upcast()
}

/// Button plates: (normal, hover, pressed, disabled).
fn button_styles(default: bool) -> [Gd<StyleBox>; 4] {
    let (top, bottom) = (rgb(0x33261a), rgb(0x1c140e));
    let idle = if default { GOLD } else { GOLD_DIM };
    let hover_edge = if default { GOLD_BRIGHT } else { GOLD };
    let light = alpha(BEVEL_LIGHT, 0.9);
    [
        bevel_style(top, bottom, idle, light, false),
        bevel_style(
            top.lightened(0.08),
            bottom.lightened(0.05),
            hover_edge,
            light,
            false,
        ),
        bevel_style(
            rgb(0x140f0b),
            rgb(0x1e160f),
            hover_edge,
            alpha(IRON_DARK, 1.0),
            true,
        ),
        bevel_style(rgb(0x1a1510), rgb(0x120e0b), IRON, alpha(IRON, 1.0), false),
    ]
}

/// A button that stands out: the answer Enter gives.
pub fn default_button_style() -> Gd<StyleBox> {
    let [normal, ..] = button_styles(true);
    normal
}

/// A socket: cells, slots, input fields.
pub fn socket_style() -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(SOCKET);
    sb.set_border_width_all(1);
    sb.set_border_color(IRON);
    sb.set_corner_radius_all(2);
    sb.set_corner_detail(1);
    sb.set_content_margin_all(4.0);
    sb.set_content_margin(Side::LEFT, 8.0);
    sb.set_content_margin(Side::RIGHT, 8.0);
    sb
}

fn flat(bg: Color, border: Color, width: i32) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(width);
    sb.set_border_color(border);
    sb.set_corner_radius_all(2);
    sb.set_corner_detail(1);
    sb
}

/// The theme every layer's root Control uses: body text in Alegreya Sans.
pub fn dark_theme() -> Gd<Theme> {
    theme_with(font(Face::Body), FONT_SIZE)
}

/// The dialogs' theme: engine text (menu rows, text windows, answers
/// typed) in PT Mono, so the engine's columns line up; buttons and titles
/// as everywhere else.
pub fn dialog_theme() -> Gd<Theme> {
    let mut theme = theme_with(font(Face::Mono), MONO_SIZE);
    theme.set_font("normal_font", "RichTextLabel", &font(Face::Mono));
    theme.set_font("bold_font", "RichTextLabel", &font(Face::MonoBold));
    theme.set_font("italics_font", "RichTextLabel", &font(Face::MonoItalic));
    theme
}

fn theme_with(default: Gd<Font>, size: i32) -> Gd<Theme> {
    let mut theme = Theme::new_gd();
    theme.set_default_font(&default);
    theme.set_default_font_size(size);
    let panel = frame_style(Frame::Panel);
    for class in ["PanelContainer", "Panel"] {
        theme.set_stylebox("panel", class, &panel);
    }
    let body_bold = font(Face::BodyBold);
    for (class, default) in [("Button", false), ("OptionButton", false)] {
        let [normal, hover, pressed, disabled] = button_styles(default);
        theme.set_stylebox("normal", class, &normal);
        theme.set_stylebox("hover", class, &hover);
        theme.set_stylebox("pressed", class, &pressed);
        theme.set_stylebox("hover_pressed", class, &pressed);
        theme.set_stylebox("disabled", class, &disabled);
        theme.set_stylebox("focus", class, &StyleBoxEmpty::new_gd());
        theme.set_font("font", class, &body_bold);
        theme.set_font_size("font_size", class, 16);
        theme.set_color("font_color", class, TEXT);
        theme.set_color("font_hover_color", class, GOLD_BRIGHT);
        theme.set_color("font_pressed_color", class, GOLD_BRIGHT);
        theme.set_color("font_hover_pressed_color", class, GOLD_BRIGHT);
        theme.set_color("font_focus_color", class, TEXT);
        theme.set_color("font_disabled_color", class, TEXT_OFF);
    }
    for class in ["Label", "LineEdit", "ItemList"] {
        theme.set_color("font_color", class, TEXT);
    }
    theme.set_color("default_color", "RichTextLabel", TEXT);
    theme.set_font("bold_font", "RichTextLabel", &font(Face::BodyBold));
    let mut edit = socket_style();
    edit.set_content_margin(Side::TOP, 6.0);
    edit.set_content_margin(Side::BOTTOM, 6.0);
    theme.set_stylebox("normal", "LineEdit", &edit);
    let mut focus = flat(Color::from_rgba(0.0, 0.0, 0.0, 0.0), GOLD, 1);
    focus.set_content_margin_all(0.0);
    theme.set_stylebox("focus", "LineEdit", &focus);
    theme.set_color("caret_color", "LineEdit", GOLD_BRIGHT);
    theme.set_color("selection_color", "LineEdit", alpha(GOLD, 0.35));
    theme.set_color("font_placeholder_color", "LineEdit", TEXT_OFF);
    theme.set_stylebox("panel", "ItemList", &socket_style());
    // dropdowns (the creation screen) and tooltips
    let mut popup = frame_style(Frame::Tooltip);
    popup.set_content_margin_all(6.0);
    theme.set_stylebox("panel", "PopupMenu", &popup);
    theme.set_stylebox("hover", "PopupMenu", &flat(alpha(GOLD, 0.22), GOLD, 0));
    theme.set_color("font_color", "PopupMenu", TEXT);
    theme.set_color("font_hover_color", "PopupMenu", GOLD_BRIGHT);
    theme.set_font("font", "PopupMenu", &font(Face::Body));
    theme.set_stylebox("panel", "TooltipPanel", &frame_style(Frame::Tooltip));
    theme.set_color("font_color", "TooltipLabel", TEXT);
    theme.set_font("font", "TooltipLabel", &font(Face::Body));
    theme.set_font_size("font_size", "TooltipLabel", 15);
    // thin iron scroll bars with a gold grabber
    for class in ["VScrollBar", "HScrollBar"] {
        let mut track = flat(alpha(INK, 0.6), IRON, 1);
        track.set_content_margin_all(2.0);
        theme.set_stylebox("scroll", class, &track);
        theme.set_stylebox("scroll_focus", class, &track);
        for (state, c) in [
            ("grabber", GOLD_DIM),
            ("grabber_highlight", GOLD),
            ("grabber_pressed", GOLD_BRIGHT),
        ] {
            theme.set_stylebox(state, class, &flat(c, IRON_DARK, 1));
        }
    }
    theme
}

// ---- scale (ui-design §1.6) ----

/// The design resolution of the HUD.
pub const DESIGN: Vector2 = Vector2::new(1920.0, 1080.0);

/// The player's UI scale: `RENETHACK_UI_SCALE` (a percent, 80-140) or the
/// project setting `renethack/ui_scale`; 120 % on a Steam Deck or a
/// 1280×800 screen, else 100 %.
pub fn user_scale(window: &Gd<Window>) -> f32 {
    let percent = std::env::var("RENETHACK_UI_SCALE")
        .ok()
        .and_then(|v| v.trim().trim_end_matches('%').parse::<f32>().ok())
        .or_else(|| {
            let key = "renethack/ui_scale";
            let ps = ProjectSettings::singleton();
            ps.has_setting(key)
                .then(|| ps.get_setting(key).try_to::<f32>().ok())
                .flatten()
        });
    if let Some(p) = percent {
        return (p / 100.0).clamp(0.8, 1.4);
    }
    let deck_size = Vector2i::new(1280, 800);
    let ds = DisplayServer::singleton();
    let screen = if ds.get_name() == "headless" {
        Vector2i::ZERO
    } else {
        ds.screen_get_size()
    };
    if Os::singleton().has_feature("steamdeck")
        || screen == deck_size
        || window.get_size() == deck_size
    {
        1.2
    } else {
        1.0
    }
}

/// Lay the canvas out for 1920×1080 design pixels (canvas_items, expand)
/// at the player's UI scale; the 3D view keeps the window's resolution.
pub fn apply_scaling(mut window: Gd<Window>) {
    let scale = user_scale(&window);
    window.set_content_scale_mode(ContentScaleMode::CANVAS_ITEMS);
    window.set_content_scale_aspect(ContentScaleAspect::EXPAND);
    window.set_content_scale_size(Vector2i::new(DESIGN.x as i32, DESIGN.y as i32));
    window.set_content_scale_factor(scale);
}

// ---- layout helpers ----

/// Anchors (left, top, right, bottom) and offsets in pixels.
pub fn place<T: Inherits<Control>>(node: &Gd<T>, anchors: [f32; 4], offsets: [f32; 4]) {
    let mut c = node.clone().upcast::<Control>();
    let sides = [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM];
    for (i, side) in sides.into_iter().enumerate() {
        c.set_anchor(side, anchors[i]);
        c.set_offset(side, offsets[i]);
    }
}

/// Fill the parent and let the mouse through (layout-only containers).
pub fn full_rect_ignore<T: Inherits<Control>>(node: &Gd<T>) {
    place(node, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
    node.clone()
        .upcast::<Control>()
        .set_mouse_filter(MouseFilter::IGNORE);
}

pub fn label(text: &str) -> Gd<Label> {
    let mut l = Label::new_alloc();
    l.set_text(text);
    l.set_mouse_filter(MouseFilter::IGNORE);
    l
}

/// A label in `face` at `size`, in `color`.
pub fn styled_label(text: &str, face: Face, size: i32, color: Color) -> Gd<Label> {
    let mut l = label(text);
    l.add_theme_font_override("font", &font(face));
    l.add_theme_font_size_override("font_size", size);
    l.add_theme_color_override("font_color", color);
    l
}

/// A dark outline under a label's text, so it reads over the world.
pub fn outline<T: Inherits<Control>>(node: &Gd<T>, px: i32) {
    let mut c = node.clone().upcast::<Control>();
    c.add_theme_color_override("font_outline_color", OUTLINE);
    c.add_theme_constant_override("outline_size", px);
}

/// A button that queues `ev` when pressed; it never takes keyboard focus,
/// so Space and Enter stay with the game.
pub fn button(text: &str, queue: &UiQueue, ev: UiEvent) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_text(text);
    b.set_focus_mode(FocusMode::NONE);
    let q = queue.clone();
    b.signals().pressed().connect(move || push(&q, ev.clone()));
    b
}

/// Text for a BBCode label, shown literally.
pub fn bbcode_escape(s: &str) -> String {
    s.replace('[', "[lb]")
}

/// "#rrggbb" for BBCode colours.
pub fn hex(c: Color) -> String {
    format!("#{}", c.to_html_without_alpha())
}

/// "#rrggbbaa" for BBCode colours with an opacity.
pub fn hex_alpha(c: Color) -> String {
    format!("#{}", c.to_html())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbcode_brackets_are_escaped() {
        assert_eq!(bbcode_escape("a [b] c"), "a [lb]b] c");
    }

    #[test]
    fn palette_maps_no_color_and_out_of_range_to_gray() {
        assert_eq!(nh_color(8), nh_color(7));
        assert_eq!(nh_color(99), nh_color(8));
        assert_eq!(nh_color(-1), nh_color(8));
        // attribute bits above the colour byte do not change it
        assert_eq!(nh_color(1 | (1 << 8)), nh_color(1));
        assert_ne!(nh_color(1), nh_color(9));
    }

    #[test]
    fn tokens_are_the_designed_hex_values() {
        let bytes = |c: Color| [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8);
        assert_eq!(bytes(GOLD_BRIGHT), [0xe7, 0xc2, 0x7a]);
        assert_eq!(bytes(TEXT), [0xe8, 0xdc, 0xc4]);
        assert_eq!(bytes(HP_DEEP), [0x8a, 0x0f, 0x12]);
    }
}
