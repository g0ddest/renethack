//! The character creation form: the name (typed, or rolled by the die
//! beside it), the roles as cards, a mark on those that forgive a first
//! game, the chosen one told in a line beside the hero (the title scene
//! shows them in their kit), and race, gender, alignment, the keys and
//! the language as compact choices under it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, Control, GridContainer, HBoxContainer, HSeparator, Label, LineEdit, OptionButton,
    PanelContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat, TextureRect, Theme, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_protocol::Catalog;
use nh_world::KeyProfile;

use crate::i18n::{self, EngineKind};
use crate::icons::{self, Glyph};
use crate::names;
use crate::theme::{self, Face};
use crate::title_scene::Preview;
use crate::tr;
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, push};

const RANDOM: &str = "random";

/// The roles that forgive a newcomer most (NetHack's own advice): a mark
/// on their cards.
const FIRST_GAME: [&str; 3] = ["Bar", "Sam", "Val"];

/// The role cards' columns, and the height of a card.
pub const CARD_COLUMNS: usize = 2;
const CARD_HEIGHT: f32 = 40.0;
/// The panels' widths: the name and the cards on the left, the role's
/// words and the other choices on the right.
const LEFT_WIDTH: f32 = 420.0;
const RIGHT_WIDTH: f32 = 440.0;

/// Whether a saved game has a name (the dice pass over those).
pub type Taken = Rc<dyn Fn(&str) -> bool>;

/// Item ids are catalog indices plus one: 0 is "Random" (Godot reads an
/// id of -1 as "use the item's position").
const RANDOM_ID: i32 = 0;

/// The catalog index of the selected item; None for "Random".
fn selected(ob: &Gd<OptionButton>) -> Option<i32> {
    Some(ob.get_selected_id() - 1).filter(|&idx| idx >= 0)
}

/// Select catalog index `idx` (None: Random), if offered.
fn select_index(ob: &mut Gd<OptionButton>, idx: Option<i32>) {
    let id = idx.map_or(RANDOM_ID, |i| i + 1);
    let index = ob.get_item_index(id).max(0);
    ob.select(index);
}

/// Fill `ob` with Random plus `items` (catalog index, text), keeping the
/// selection if still offered.
fn fill_options(ob: &mut Gd<OptionButton>, items: &[(i32, String)]) {
    let keep = selected(ob);
    ob.clear();
    ob.add_item_ex(&tr!("creation-random")).id(RANDOM_ID).done();
    for (idx, text) in items {
        ob.add_item_ex(text).id(idx + 1).done();
    }
    select_index(ob, keep);
}

/// Races allowed for `role` (None: any role).
fn races_for(cat: &Catalog, role: Option<i32>) -> Vec<i32> {
    let mut v: Vec<i32> = cat
        .roles
        .iter()
        .filter(|r| role.is_none_or(|x| r.idx == x))
        .flat_map(|r| r.combos.iter().map(|c| c.race))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Genders (`aligns` false) or alignments allowed for role and race.
fn options_for(cat: &Catalog, role: Option<i32>, race: Option<i32>, aligns: bool) -> Vec<i32> {
    let mut v: Vec<i32> = cat
        .roles
        .iter()
        .filter(|r| role.is_none_or(|x| r.idx == x))
        .flat_map(|r| r.combos.iter())
        .filter(|c| race.is_none_or(|x| c.race == x))
        .flat_map(|c| {
            if aligns {
                c.aligns.clone()
            } else {
                c.genders.clone()
            }
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A role's, race's, gender's or alignment's name from the catalog, as the
/// player reads it.
fn engine_name(english: &str) -> String {
    capitalize(&i18n::engine(EngineKind::Name, english))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The words on how a role plays, in a line (its catalog code; None:
/// Random).
fn about(code: Option<&str>) -> Option<&'static str> {
    Some(match code {
        None => "creation-about-random",
        Some("Arc") => "creation-about-arc",
        Some("Bar") => "creation-about-bar",
        Some("Cav") => "creation-about-cav",
        Some("Hea") => "creation-about-hea",
        Some("Kni") => "creation-about-kni",
        Some("Mon") => "creation-about-mon",
        Some("Pri") => "creation-about-pri",
        Some("Rog") => "creation-about-rog",
        Some("Ran") => "creation-about-ran",
        Some("Sam") => "creation-about-sam",
        Some("Tou") => "creation-about-tou",
        Some("Val") => "creation-about-val",
        Some("Wiz") => "creation-about-wiz",
        Some(_) => return None,
    })
}

fn flat(bg: Color, border: Color) -> Gd<StyleBoxFlat> {
    let mut sb = StyleBoxFlat::new_gd();
    sb.set_bg_color(bg);
    sb.set_border_width_all(1);
    sb.set_border_color(border);
    sb.set_corner_radius_all(3);
    sb.set_content_margin(Side::LEFT, 12.0);
    sb.set_content_margin(Side::RIGHT, 10.0);
    sb.set_content_margin(Side::TOP, 6.0);
    sb.set_content_margin(Side::BOTTOM, 6.0);
    sb
}

/// The cards' looks: dark plates, the chosen one warm and gold-edged.
fn card_theme() -> Gd<Theme> {
    let mut t = Theme::new_gd();
    let plate = flat(Color::from_rgba(0.06, 0.047, 0.037, 0.94), theme::IRON);
    let lit = flat(Color::from_rgba(0.11, 0.085, 0.06, 0.96), theme::GOLD_DIM);
    let chosen = flat(Color::from_rgba(0.24, 0.17, 0.07, 1.0), theme::GOLD);
    let looks: [(&str, &Gd<StyleBoxFlat>); 5] = [
        ("normal", &plate),
        ("hover", &lit),
        ("pressed", &chosen),
        ("hover_pressed", &chosen),
        ("disabled", &plate),
    ];
    for (state, look) in looks {
        t.set_stylebox(state, "Button", look);
    }
    t.set_stylebox("focus", "Button", &StyleBoxEmpty::new_gd());
    t
}

/// The look of the card the keys and a gamepad are on: the focus's gold
/// edge and glow, over the card's own margins.
pub fn card_focus() -> Gd<StyleBox> {
    let mut sb = theme::focus_style();
    sb.set_content_margin(Side::LEFT, 12.0);
    sb.set_content_margin(Side::RIGHT, 10.0);
    sb.upcast()
}

fn card(icon: Option<Glyph>, color: Color) -> Gd<Button> {
    let mut b = Button::new_alloc();
    b.set_toggle_mode(true);
    b.set_focus_mode(FocusMode::NONE);
    b.set_text_alignment(HorizontalAlignment::LEFT);
    b.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
    b.set_clip_text(true);
    b.set_h_size_flags(SizeFlags::EXPAND_FILL);
    b.set_custom_minimum_size(Vector2::new(0.0, CARD_HEIGHT));
    if let Some(g) = icon {
        b.set_button_icon(&icons::glyph_icon(g));
        b.set_icon_alignment(HorizontalAlignment::RIGHT);
        b.add_theme_constant_override("icon_max_width", 18);
        for state in [
            "icon_normal_color",
            "icon_focus_color",
            "icon_hover_color",
            "icon_pressed_color",
            "icon_hover_pressed_color",
        ] {
            b.add_theme_color_override(state, color);
        }
    }
    b
}

/// The first-game mark, a star, and `text` beside it.
fn starred(text: &str, size: i32, color: Color) -> Gd<HBoxContainer> {
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 6);
    let mut star = TextureRect::new_alloc();
    star.set_texture(&icons::glyph_icon(Glyph::Star));
    star.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    star.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    star.set_custom_minimum_size(Vector2::new(18.0, 18.0));
    star.set_modulate(theme::GOOD);
    row.add_child(&star);
    row.add_child(&theme::styled_label(text, Face::BodyBold, size, color));
    row
}

/// The keys' choice told in a line: what walks, the rest in its tip.
fn keys_line(profile: KeyProfile) -> String {
    match profile {
        KeyProfile::Modern => tr!("creation-keys-modern-line"),
        KeyProfile::Classic => tr!("creation-keys-classic-line"),
    }
}

fn caption(text: &str) -> Gd<Label> {
    theme::styled_label(text, Face::Caps, 15, theme::TEXT_DIM)
}

/// The form's controls; the closures share them to keep the choices
/// valid together.
#[derive(Clone)]
pub struct Form {
    pub catalog: Rc<Catalog>,
    queue: UiQueue,
    pub name: Gd<LineEdit>,
    /// Rolls a name (`names`).
    pub dice: Gd<Button>,
    /// The name the dice put in the field, a heroine's or not: while the
    /// field holds it, it follows the gender chosen.
    pub rolled: Rc<RefCell<Option<(String, bool)>>>,
    pub taken: Taken,
    /// The role cards in their order (None: Random), the role chosen.
    pub cards: Vec<(Option<i32>, Gd<Button>)>,
    role: Rc<Cell<Option<i32>>>,
    race: Gd<OptionButton>,
    gender: Gd<OptionButton>,
    align: Gd<OptionButton>,
    /// Modern (item 0) or Classic keys, and what they walk with.
    keys: Gd<OptionButton>,
    keys_told: Gd<Label>,
    lang: Gd<OptionButton>,
    /// The chosen role's name, its first-game mark and how it plays.
    role_title: Gd<Label>,
    role_mark: Gd<HBoxContainer>,
    role_line: Gd<Label>,
    pub start: Gd<Button>,
    pub continue_instead: Gd<Button>,
    back: Gd<Button>,
    notice: Gd<Label>,
    /// The two panels, and the room between them where the hero stands.
    pub left: Gd<PanelContainer>,
    pub right: Gd<PanelContainer>,
    pub room: Gd<Control>,
}

impl Form {
    /// The form's controls, laid out in its two panels (`left`, `right`,
    /// the `room` between them for the page to place); `lang` is the
    /// language's picker, `back` the button that leaves.
    pub fn new(
        catalog: Rc<Catalog>,
        keys_first: KeyProfile,
        taken: Taken,
        queue: &UiQueue,
        lang: Gd<OptionButton>,
        back: Gd<Button>,
    ) -> Form {
        // ---- left: the title, the roles ----
        let mut left = theme::framed(theme::Frame::Panel);
        left.set_custom_minimum_size(Vector2::new(LEFT_WIDTH, 0.0));
        left.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        let mut title = theme::styled_label(&tr!("creation-title"), Face::Title, 32, theme::ACCENT);
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        col.add_child(&title);
        let mut name = LineEdit::new_alloc();
        name.set_placeholder(&tr!("creation-name-placeholder"));
        // a name: the same in every language
        i18n::verbatim(&name);
        name.set_max_length(names::MAX_BYTES as i32);
        name.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut dice = Button::new_alloc();
        dice.set_focus_mode(FocusMode::NONE);
        dice.set_button_icon(&icons::glyph_icon(Glyph::Die));
        dice.set_icon_alignment(HorizontalAlignment::CENTER);
        dice.add_theme_constant_override("icon_max_width", 24);
        for (state, color) in [
            ("icon_normal_color", theme::GOLD),
            ("icon_focus_color", theme::GOLD),
            ("icon_hover_color", theme::GOLD_BRIGHT),
            ("icon_pressed_color", theme::GOLD_BRIGHT),
            ("icon_hover_pressed_color", theme::GOLD_BRIGHT),
        ] {
            dice.add_theme_color_override(state, color);
        }
        dice.set_custom_minimum_size(Vector2::new(46.0, 0.0));
        crate::tips::set(&dice, &tr!("creation-name-roll-tip"));
        let mut name_row = HBoxContainer::new_alloc();
        name_row.add_theme_constant_override("separation", 8);
        name_row.add_child(&name);
        name_row.add_child(&dice);
        col.add_child(&caption(&tr!("creation-role")));
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(CARD_COLUMNS as i32);
        grid.add_theme_constant_override("h_separation", 8);
        grid.add_theme_constant_override("v_separation", 6);
        grid.set_theme(&card_theme());
        let mut cards = vec![(None, card(Some(Glyph::Die), theme::GOLD))];
        for r in &catalog.roles {
            let first = FIRST_GAME.contains(&r.code.as_str());
            let icon = first.then_some(Glyph::Star);
            cards.push((Some(r.idx), card(icon, theme::GOOD)));
        }
        for (_, c) in &cards {
            grid.add_child(c);
        }
        col.add_child(&grid);
        // what the star on a card says
        col.add_child(&starred(
            &tr!("creation-first-game-key"),
            15,
            theme::TEXT_DIM,
        ));
        left.add_child(&col);

        // ---- right: the role chosen, the name and the other choices,
        // start ----
        let mut right = theme::framed(theme::Frame::Panel);
        right.set_custom_minimum_size(Vector2::new(RIGHT_WIDTH, 0.0));
        right.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 10);
        let role_title = theme::styled_label("", Face::Title, 30, theme::ACCENT);
        col.add_child(&role_title);
        let role_mark = starred(&tr!("creation-first-game"), 16, theme::GOOD);
        col.add_child(&role_mark);
        let mut role_line = theme::styled_label("", Face::Body, 17, theme::TEXT);
        role_line.set_autowrap_mode(AutowrapMode::WORD_SMART);
        role_line.set_custom_minimum_size(Vector2::new(RIGHT_WIDTH - 40.0, 0.0));
        col.add_child(&role_line);
        col.add_child(&HSeparator::new_alloc());
        let mut choices = GridContainer::new_alloc();
        choices.set_columns(2);
        choices.add_theme_constant_override("h_separation", 14);
        choices.add_theme_constant_override("v_separation", 8);
        choices.add_child(&theme::label(&tr!("creation-name")));
        name_row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        choices.add_child(&name_row);
        let option = |label: &str, grid: &mut Gd<GridContainer>| {
            grid.add_child(&theme::label(label));
            let mut ob = OptionButton::new_alloc();
            ob.set_focus_mode(FocusMode::NONE);
            ob.set_h_size_flags(SizeFlags::EXPAND_FILL);
            // as wide as the panel, its longest choice cut short
            ob.set_fit_to_longest_item(false);
            ob.set_clip_text(true);
            ob.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
            grid.add_child(&ob);
            ob
        };
        let race = option(&tr!("creation-race"), &mut choices);
        let gender = option(&tr!("creation-gender"), &mut choices);
        let align = option(&tr!("creation-alignment"), &mut choices);
        let mut keys = option(&tr!("creation-keys"), &mut choices);
        keys.add_item(&tr!("creation-keys-modern-name"));
        keys.add_item(&tr!("creation-keys-classic-name"));
        keys.select(i32::from(keys_first == KeyProfile::Classic));
        crate::tips::set(&keys, &tr!("creation-keys-tip"));
        // under the keys' short name, what they walk with
        choices.add_child(&Control::new_alloc());
        let mut keys_told =
            theme::styled_label(&keys_line(keys_first), Face::Body, 15, theme::TEXT_DIM);
        keys_told.set_autowrap_mode(AutowrapMode::WORD_SMART);
        crate::tips::set(&keys_told, &tr!("creation-keys-tip"));
        choices.add_child(&keys_told);
        choices.add_child(&theme::label(&tr!("creation-language")));
        let mut lang = lang;
        lang.set_h_size_flags(SizeFlags::EXPAND_FILL);
        lang.set_fit_to_longest_item(false);
        choices.add_child(&lang);
        col.add_child(&choices);
        let mut notice = theme::label("");
        notice.add_theme_color_override("font_color", theme::WARN);
        notice.set_autowrap_mode(AutowrapMode::WORD_SMART);
        notice.set_custom_minimum_size(Vector2::new(RIGHT_WIDTH - 40.0, 0.0));
        col.add_child(&notice);
        let mut buttons = HBoxContainer::new_alloc();
        buttons.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
        buttons.add_theme_constant_override("separation", 12);
        let mut start = Button::new_alloc();
        start.set_text(&tr!("creation-start"));
        start.set_focus_mode(FocusMode::NONE);
        start.set_custom_minimum_size(Vector2::new(150.0, 44.0));
        start.add_theme_stylebox_override("normal", &theme::default_button_style());
        let mut continue_instead = Button::new_alloc();
        continue_instead.set_focus_mode(FocusMode::NONE);
        continue_instead.set_visible(false);
        let mut back = back;
        back.set_custom_minimum_size(Vector2::new(110.0, 44.0));
        buttons.add_child(&start);
        buttons.add_child(&continue_instead);
        buttons.add_child(&back);
        col.add_child(&buttons);
        right.add_child(&col);

        let mut room = Control::new_alloc();
        room.set_mouse_filter(MouseFilter::IGNORE);
        room.set_h_size_flags(SizeFlags::EXPAND_FILL);

        let mut form = Form {
            catalog,
            queue: queue.clone(),
            name,
            dice,
            rolled: Rc::new(RefCell::new(None)),
            taken,
            cards,
            role: Rc::new(Cell::new(None)),
            race,
            gender,
            align,
            keys,
            keys_told,
            lang,
            role_title,
            role_mark,
            role_line,
            start,
            continue_instead,
            back,
            notice,
            left,
            right,
            room,
        };
        form.select_role(None);
        form.connect();
        form
    }

    /// The closures: they touch only the form's own widgets and the queue.
    fn connect(&mut self) {
        for (role, c) in self.cards.clone() {
            let f = self.clone();
            c.signals()
                .pressed()
                .connect(move || f.clone().select_role(role));
        }
        let f = self.clone();
        self.race.signals().item_selected().connect(move |_i: i64| {
            let mut f = f.clone();
            f.restrict(false);
            f.agree();
            f.show_role();
        });
        let f = self.clone();
        self.gender
            .signals()
            .item_selected()
            .connect(move |_i: i64| {
                let mut f = f.clone();
                f.agree();
                f.show_role();
            });
        let f = self.clone();
        self.dice
            .signals()
            .pressed()
            .connect(move || f.clone().roll());
        let f = self.clone();
        self.keys
            .signals()
            .item_selected()
            .connect(move |_i: i64| f.clone().show_keys());
        let (mut field, q) = (self.name.clone(), self.queue.clone());
        self.name
            .signals()
            .text_changed()
            .connect(move |t: GString| {
                // no more than NetHack keeps (Cyrillic: 15 letters)
                let text = t.to_string();
                let kept = names::clip(&text);
                if kept.len() < text.len() {
                    field.set_text(kept);
                    field.set_caret_column(kept.chars().count() as i32);
                }
                push(&q, UiEvent::NameEdited(kept.to_string()));
            });
        let (f, q) = (self.clone(), self.queue.clone());
        self.name
            .signals()
            .text_submitted()
            .connect(move |_t: GString| {
                if !f.start.is_disabled() {
                    push(&q, UiEvent::StartCharacter(f.chosen()));
                }
            });
        let (f, q) = (self.clone(), self.queue.clone());
        self.start
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::StartCharacter(f.chosen())));
        let (f, q) = (self.clone(), self.queue.clone());
        self.continue_instead
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::ContinueGame(f.typed_name())));
    }

    /// The controls the keys and a gamepad move among, in order (the
    /// cards first), where the cards are among them, and Start.
    pub fn nav_items(&self) -> (Vec<Gd<Control>>, std::ops::Range<usize>, usize) {
        let mut items: Vec<Gd<Control>> = Vec::new();
        items.extend(self.cards.iter().map(|(_, c)| c.clone().upcast()));
        let cards = 0..items.len();
        items.extend([
            self.name.clone().upcast(),
            self.dice.clone().upcast(),
            self.race.clone().upcast(),
            self.gender.clone().upcast(),
            self.align.clone().upcast(),
            self.keys.clone().upcast(),
            self.lang.clone().upcast(),
        ]);
        let start = items.len();
        items.extend([
            self.start.clone().upcast(),
            self.continue_instead.clone().upcast(),
            self.back.clone().upcast(),
        ]);
        (items, cards, start)
    }

    /// The role chosen (its catalog index; None: Random).
    fn role_now(&self) -> Option<i32> {
        self.role.get()
    }

    /// `role` chosen: its card lit, the choices it allows, its words.
    pub fn select_role(&mut self, role: Option<i32>) {
        self.role.set(role);
        for (r, c) in &mut self.cards {
            c.set_pressed_no_signal(*r == role);
        }
        self.restrict(true);
        self.agree();
        self.show_role();
    }

    /// Offer only races, genders and alignments valid with what is chosen.
    fn restrict(&mut self, from_role: bool) {
        let cat = self.catalog.clone();
        let role = self.role_now();
        if from_role {
            let races: Vec<(i32, String)> = races_for(&cat, role)
                .into_iter()
                .filter_map(|i| cat.races.get(i as usize).map(|r| (i, engine_name(&r.noun))))
                .collect();
            fill_options(&mut self.race, &races);
        }
        let race = selected(&self.race);
        let genders: Vec<(i32, String)> = options_for(&cat, role, race, false)
            .into_iter()
            .filter_map(|i| {
                cat.genders
                    .get(i as usize)
                    .map(|g| (i, engine_name(&g.adj)))
            })
            .collect();
        fill_options(&mut self.gender, &genders);
        let aligns: Vec<(i32, String)> = options_for(&cat, role, race, true)
            .into_iter()
            .filter_map(|i| cat.aligns.get(i as usize).map(|a| (i, engine_name(&a.adj))))
            .collect();
        fill_options(&mut self.align, &aligns);
    }

    /// Whether the hero is a heroine: chosen, or the only gender the
    /// role and race allow; None: left to chance.
    fn female(&self) -> Option<bool> {
        let cat = &self.catalog;
        if let Some(g) = selected(&self.gender) {
            return cat.genders.get(g as usize).map(|g| g.adj == "female");
        }
        match options_for(cat, self.role_now(), selected(&self.race), false)[..] {
            [only] => cat.genders.get(only as usize).map(|g| g.adj == "female"),
            _ => None,
        }
    }

    /// The cards' names (a heroine's where the role has one, a heroine
    /// chosen) and the chosen role's words (a heroine's name for her).
    fn show_role(&mut self) {
        let cat = self.catalog.clone();
        let name_of = |r: &nh_protocol::RoleInfo, female: bool| {
            let english = r.name_female.as_ref().filter(|_| female).unwrap_or(&r.name);
            engine_name(english)
        };
        let chosen = selected(&self.gender)
            .and_then(|g| cat.genders.get(g as usize))
            .is_some_and(|g| g.adj == "female");
        for (r, c) in &mut self.cards {
            let text = match r.and_then(|i| cat.roles.get(i as usize)) {
                Some(info) => name_of(info, chosen),
                None => tr!("creation-random"),
            };
            c.set_text(&text);
        }
        let info = self.role_now().and_then(|i| cat.roles.get(i as usize));
        let female = self.female() == Some(true);
        self.role_title.set_text(&match info {
            Some(r) => name_of(r, female),
            None => tr!("creation-random"),
        });
        let code = info.map(|r| r.code.as_str());
        self.role_mark
            .set_visible(code.is_some_and(|c| FIRST_GAME.contains(&c)));
        self.role_line
            .set_text(&about(code).map(i18n::tr).unwrap_or_default());
    }

    /// The line under the keys for the ones chosen.
    fn show_keys(&mut self) {
        let profile = self.choice().profile;
        self.keys_told.set_text(&keys_line(profile));
    }

    pub fn typed_name(&self) -> String {
        self.name.get_text().to_string().trim().to_string()
    }

    /// Whether a heroine's name, when the field holds the dice's.
    fn rolled_female(&self) -> Option<bool> {
        let name = self.typed_name();
        self.rolled
            .borrow()
            .as_ref()
            .filter(|(rolled, _)| *rolled == name)
            .map(|&(_, female)| female)
    }

    /// The dice: a name that agrees with the gender chosen, a gender
    /// left to chance chosen first (and shown).
    pub fn roll(&mut self) {
        let was = self.choice();
        let rolled = names::named(
            &self.catalog,
            was.clone(),
            names::roll(),
            true,
            &*self.taken,
        );
        if rolled.gender != was.gender {
            let genders: Vec<(i32, String, String)> = self
                .catalog
                .genders
                .iter()
                .map(|g| (g.idx, g.code.clone(), g.adj.clone()))
                .collect();
            Form::preset_option(&mut self.gender, &rolled.gender, &genders);
        }
        self.show_rolled(&rolled);
        self.show_role();
    }

    /// The dice's name in the field, again if the gender chosen now (or
    /// the role's and race's) does not agree with it.
    fn agree(&mut self) {
        let Some(female) = self.rolled_female() else {
            return;
        };
        let was = self.choice();
        let now = names::agreed(
            &self.catalog,
            was.clone(),
            female,
            names::roll(),
            &*self.taken,
        );
        if now.name != was.name {
            self.show_rolled(&now);
        }
    }

    fn show_rolled(&mut self, choice: &CharacterChoice) {
        let female = names::is_female(&self.catalog, &choice.gender);
        *self.rolled.borrow_mut() = Some((choice.name.clone(), female));
        self.name.set_text(&choice.name);
        self.name
            .set_caret_column(choice.name.chars().count() as i32);
        push(&self.queue, UiEvent::NameEdited(choice.name.clone()));
    }

    /// The choice the game starts with: an empty name rolled, the dice's
    /// name with the gender it agrees with.
    pub fn chosen(&self) -> CharacterChoice {
        let (cat, choice, roll) = (&self.catalog, self.choice(), names::roll());
        match self.rolled_female() {
            Some(female) => names::agreed(cat, choice, female, roll, &*self.taken),
            None => names::named(cat, choice, roll, false, &*self.taken),
        }
    }

    pub fn choice(&self) -> CharacterChoice {
        let cat = &self.catalog;
        let code = |ob: &Gd<OptionButton>, codes: &dyn Fn(usize) -> Option<String>| {
            selected(ob)
                .and_then(|i| codes(i as usize))
                .unwrap_or_else(|| RANDOM.to_string())
        };
        CharacterChoice {
            name: self.typed_name(),
            role: self
                .role_now()
                .and_then(|i| cat.roles.get(i as usize))
                .map_or_else(|| RANDOM.to_string(), |r| r.code.clone()),
            race: code(&self.race, &|i| cat.races.get(i).map(|r| r.code.clone())),
            gender: code(&self.gender, &|i| {
                cat.genders.get(i).map(|g| g.code.clone())
            }),
            align: code(&self.align, &|i| cat.aligns.get(i).map(|a| a.code.clone())),
            profile: if self.keys.get_selected() == 1 {
                KeyProfile::Classic
            } else {
                KeyProfile::Modern
            },
        }
    }

    /// The hero the title scene shows for the choice now, standing at
    /// `at` (a share of the screen's width).
    pub fn preview(&self, at: f32) -> Preview {
        let cat = &self.catalog;
        Preview {
            role: self
                .role_now()
                .and_then(|i| cat.roles.get(i as usize))
                .map(|r| r.code.clone()),
            female: self.female(),
            at,
        }
    }

    /// Select the item whose catalog code or name matches `word`.
    fn preset_option(ob: &mut Gd<OptionButton>, word: &str, names: &[(i32, String, String)]) {
        let w = word.to_lowercase();
        let idx = names
            .iter()
            .find(|(_, code, name)| code.to_lowercase() == w || name.to_lowercase() == w)
            .map(|(idx, _, _)| *idx);
        select_index(ob, idx);
    }

    /// Fill the form as `choice` has it.
    pub fn preset(&mut self, choice: &CharacterChoice) {
        self.name.set_text(&choice.name);
        let cat = self.catalog.clone();
        let w = choice.role.to_lowercase();
        let role = cat
            .roles
            .iter()
            .find(|r| r.code.to_lowercase() == w || r.name.to_lowercase() == w)
            .map(|r| r.idx);
        self.select_role(role);
        let races: Vec<(i32, String, String)> = cat
            .races
            .iter()
            .map(|r| (r.idx, r.code.clone(), r.noun.clone()))
            .collect();
        Form::preset_option(&mut self.race, &choice.race, &races);
        self.restrict(false);
        let genders: Vec<(i32, String, String)> = cat
            .genders
            .iter()
            .map(|g| (g.idx, g.code.clone(), g.adj.clone()))
            .collect();
        Form::preset_option(&mut self.gender, &choice.gender, &genders);
        let aligns: Vec<(i32, String, String)> = cat
            .aligns
            .iter()
            .map(|a| (a.idx, a.code.clone(), a.adj.clone()))
            .collect();
        Form::preset_option(&mut self.align, &choice.align, &aligns);
        self.keys
            .select(i32::from(choice.profile == KeyProfile::Classic));
        self.show_keys();
        self.show_role();
    }

    /// The name would restore a saved game: offer to continue it instead.
    pub fn set_name_taken(&mut self, taken: bool) {
        let name = self.typed_name();
        self.start.set_disabled(taken);
        self.continue_instead.set_visible(taken);
        self.continue_instead
            .set_text(&tr!("creation-continue-instead", name = name));
        self.notice.set_text(&if taken {
            tr!("creation-name-taken")
        } else {
            String::new()
        });
    }

    /// Why the name cannot be used.
    pub fn set_name_error(&mut self, error: &str) {
        self.start.set_disabled(true);
        self.notice.set_text(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_is_told_and_the_first_games_are_roles() {
        let line = include_str!("../../nh-world/tests/data/catalog.jsonl");
        let cat = match nh_protocol::parse_line(line).unwrap() {
            nh_protocol::EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        };
        for lang in [i18n::Lang::En, i18n::Lang::Ru] {
            let told = |code: Option<&str>| about(code).and_then(|k| i18n::tr_in(lang, k));
            for r in &cat.roles {
                assert!(told(Some(&r.code)).is_some(), "{lang:?} {}", r.code);
            }
            assert!(told(None).is_some());
        }
        for code in FIRST_GAME {
            assert!(cat.roles.iter().any(|r| r.code == code), "{code}");
        }
    }
}
