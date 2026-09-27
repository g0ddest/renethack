//! Full-screen pages outside a game: title, character creation, the end of
//! a game and errors.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use godot::classes::control::{FocusMode, MouseFilter, SizeFlags};
use godot::classes::{
    Button, CanvasLayer, CenterContainer, ColorRect, Control, GridContainer, HBoxContainer, Label,
    LineEdit, OptionButton, PanelContainer, RichTextLabel, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;
use nh_link::SavedGame;
use nh_protocol::Catalog;

use crate::theme::{self, bbcode_escape, hex};
use crate::ui_events::{CharacterChoice, UiEvent, UiQueue, push};

/// What the end screen shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndSummary {
    pub last_messages: Vec<String>,
    /// The summary and tombstone window (empty if the player skipped it).
    pub text: Vec<String>,
    /// The top ten list.
    pub scores: Vec<String>,
}

/// Name used when the field is left empty.
pub const DEFAULT_NAME: &str = "Adventurer";
const RANDOM: &str = "random";

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
    ob.add_item_ex("Random").id(RANDOM_ID).done();
    for (idx, text) in items {
        ob.add_item_ex(text).id(idx + 1).done();
    }
    select_index(ob, keep);
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The creation form; closures share it to keep combinations valid.
#[derive(Clone)]
struct Form {
    catalog: Rc<Catalog>,
    name: Gd<LineEdit>,
    role: Gd<OptionButton>,
    race: Gd<OptionButton>,
    gender: Gd<OptionButton>,
    align: Gd<OptionButton>,
    start: Gd<Button>,
    continue_instead: Gd<Button>,
    notice: Gd<Label>,
}

impl Form {
    /// Offer only races, genders and alignments valid with what is chosen.
    fn restrict(&mut self, from_role: bool) {
        let cat = self.catalog.clone();
        let role = selected(&self.role);
        if from_role {
            let races: Vec<(i32, String)> = races_for(&cat, role)
                .into_iter()
                .filter_map(|i| cat.races.get(i as usize).map(|r| (i, capitalize(&r.noun))))
                .collect();
            fill_options(&mut self.race, &races);
        }
        let race = selected(&self.race);
        let genders: Vec<(i32, String)> = options_for(&cat, role, race, false)
            .into_iter()
            .filter_map(|i| cat.genders.get(i as usize).map(|g| (i, capitalize(&g.adj))))
            .collect();
        fill_options(&mut self.gender, &genders);
        let aligns: Vec<(i32, String)> = options_for(&cat, role, race, true)
            .into_iter()
            .filter_map(|i| cat.aligns.get(i as usize).map(|a| (i, capitalize(&a.adj))))
            .collect();
        fill_options(&mut self.align, &aligns);
    }

    fn typed_name(&self) -> String {
        let name = self.name.get_text().to_string();
        let name = name.trim();
        if name.is_empty() {
            DEFAULT_NAME.to_string()
        } else {
            name.to_string()
        }
    }

    fn choice(&self) -> CharacterChoice {
        let cat = &self.catalog;
        let code = |ob: &Gd<OptionButton>, codes: &dyn Fn(usize) -> Option<String>| {
            selected(ob)
                .and_then(|i| codes(i as usize))
                .unwrap_or_else(|| RANDOM.to_string())
        };
        CharacterChoice {
            name: self.typed_name(),
            role: code(&self.role, &|i| cat.roles.get(i).map(|r| r.code.clone())),
            race: code(&self.race, &|i| cat.races.get(i).map(|r| r.code.clone())),
            gender: code(&self.gender, &|i| {
                cat.genders.get(i).map(|g| g.code.clone())
            }),
            align: code(&self.align, &|i| cat.aligns.get(i).map(|a| a.code.clone())),
        }
    }

    /// Select the item whose catalog code or name matches `word`.
    fn preset(ob: &mut Gd<OptionButton>, word: &str, names: &[(i32, String, String)]) {
        let w = word.to_lowercase();
        let idx = names
            .iter()
            .find(|(_, code, name)| code.to_lowercase() == w || name.to_lowercase() == w)
            .map(|(idx, _, _)| *idx);
        select_index(ob, idx);
    }
}

pub struct Screens {
    root: Gd<Control>,
    content: Option<Gd<Control>>,
    queue: UiQueue,
    current: Option<&'static str>,
    form: Option<Form>,
    end: Option<EndSummary>,
}

fn title_label(text: &str, size: i32) -> Gd<Label> {
    let mut l = theme::label(text);
    l.add_theme_font_size_override("font_size", size);
    l.add_theme_color_override("font_color", theme::ACCENT);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l
}

fn wide(mut b: Gd<Button>) -> Gd<Button> {
    b.set_custom_minimum_size(Vector2::new(460.0, 44.0));
    b
}

fn column() -> Gd<VBoxContainer> {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 12);
    col
}

fn row() -> Gd<HBoxContainer> {
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 12);
    row
}

/// Unix time as "YYYY-MM-DD HH:MM" (UTC).
pub fn format_time(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // civil_from_days (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

impl Screens {
    pub fn new(mut layer: Gd<CanvasLayer>, queue: UiQueue) -> Screens {
        let mut root = Control::new_alloc();
        theme::full_rect_ignore(&root);
        root.set_theme(&theme::dark_theme());
        let mut bg = ColorRect::new_alloc();
        bg.set_color(theme::BG);
        theme::place(&bg, [0.0, 0.0, 1.0, 1.0], [0.0; 4]);
        // a real page: clicks must not reach the map behind it
        bg.set_mouse_filter(MouseFilter::STOP);
        root.add_child(&bg);
        root.set_visible(false);
        layer.add_child(&root);
        Screens {
            root,
            content: None,
            queue,
            current: None,
            form: None,
            end: None,
        }
    }

    /// Replace the page with a centred `inner`.
    fn page(&mut self, name: &'static str, inner: &Gd<Control>) {
        self.hide();
        let mut center = CenterContainer::new_alloc();
        theme::full_rect_ignore(&center);
        center.add_child(inner);
        self.root.add_child(&center);
        self.content = Some(center.upcast());
        self.current = Some(name);
        self.root.set_visible(true);
    }

    pub fn show_title(&mut self, saves: &[SavedGame], notice: Option<&str>) {
        let mut col = column();
        col.add_child(&title_label("renethack", 72));
        let mut sub = theme::label("NetHack 5.0");
        sub.set_horizontal_alignment(HorizontalAlignment::CENTER);
        sub.add_theme_color_override("font_color", theme::TEXT_DIM);
        col.add_child(&sub);
        let mut gap = Control::new_alloc();
        gap.set_custom_minimum_size(Vector2::new(0.0, 24.0));
        col.add_child(&gap);
        col.add_child(&wide(theme::button(
            "New game",
            &self.queue,
            UiEvent::NewGame,
        )));
        for s in saves {
            let text = format!("Continue: {} ({})", s.display_name, format_time(s.modified));
            let ev = UiEvent::ContinueGame(s.name.clone());
            col.add_child(&wide(theme::button(&text, &self.queue, ev)));
        }
        col.add_child(&wide(theme::button("Quit", &self.queue, UiEvent::QuitApp)));
        if let Some(n) = notice {
            let mut l = theme::label(n);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            l.add_theme_color_override("font_color", theme::ACCENT);
            l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
            l.set_custom_minimum_size(Vector2::new(460.0, 0.0));
            col.add_child(&l);
        }
        self.form = None;
        self.page("title", &col.upcast());
    }

    pub fn show_creation(&mut self, catalog: Rc<Catalog>) {
        let mut panel = PanelContainer::new_alloc();
        let mut col = column();
        col.add_child(&title_label("New character", 32));
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(2);
        grid.add_theme_constant_override("h_separation", 16);
        grid.add_theme_constant_override("v_separation", 10);
        let mut name = LineEdit::new_alloc();
        name.set_placeholder(DEFAULT_NAME);
        name.set_max_length(31);
        name.set_custom_minimum_size(Vector2::new(360.0, 0.0));
        let option = |label: &str, grid: &mut Gd<GridContainer>| {
            grid.add_child(&theme::label(label));
            let mut ob = OptionButton::new_alloc();
            ob.set_focus_mode(FocusMode::NONE);
            ob.set_h_size_flags(SizeFlags::EXPAND_FILL);
            grid.add_child(&ob);
            ob
        };
        grid.add_child(&theme::label("Name"));
        grid.add_child(&name);
        let mut role = option("Role", &mut grid);
        let race = option("Race", &mut grid);
        let gender = option("Gender", &mut grid);
        let align = option("Alignment", &mut grid);
        col.add_child(&grid);
        let roles: Vec<(i32, String)> = catalog
            .roles
            .iter()
            .map(|r| (r.idx, r.name.clone()))
            .collect();
        fill_options(&mut role, &roles);
        let mut notice = theme::label("");
        notice.add_theme_color_override("font_color", theme::WARN);
        notice.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        notice.set_custom_minimum_size(Vector2::new(480.0, 0.0));
        col.add_child(&notice);
        let mut buttons = row();
        let mut start = Button::new_alloc();
        start.set_text("Start");
        start.set_focus_mode(FocusMode::NONE);
        let mut continue_instead = Button::new_alloc();
        continue_instead.set_focus_mode(FocusMode::NONE);
        continue_instead.set_visible(false);
        buttons.add_child(&start);
        buttons.add_child(&continue_instead);
        buttons.add_child(&theme::button("Back", &self.queue, UiEvent::BackToTitle));
        col.add_child(&buttons);
        panel.add_child(&col);

        let mut form = Form {
            catalog,
            name,
            role,
            race,
            gender,
            align,
            start,
            continue_instead,
            notice,
        };
        form.restrict(true);
        // the closures below touch only the form's own widgets and the queue
        let f = form.clone();
        form.role
            .signals()
            .item_selected()
            .connect(move |_i: i64| f.clone().restrict(true));
        let f = form.clone();
        form.race
            .signals()
            .item_selected()
            .connect(move |_i: i64| f.clone().restrict(false));
        let q = self.queue.clone();
        form.name
            .signals()
            .text_changed()
            .connect(move |t: GString| push(&q, UiEvent::NameEdited(t.to_string())));
        let (f, q) = (form.clone(), self.queue.clone());
        form.name
            .signals()
            .text_submitted()
            .connect(move |_t: GString| {
                if !f.start.is_disabled() {
                    push(&q, UiEvent::StartCharacter(f.choice()));
                }
            });
        let (f, q) = (form.clone(), self.queue.clone());
        form.start
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::StartCharacter(f.choice())));
        let (f, q) = (form.clone(), self.queue.clone());
        form.continue_instead
            .signals()
            .pressed()
            .connect(move || push(&q, UiEvent::ContinueGame(f.typed_name())));
        let mut name = form.name.clone();
        self.page("creation", &panel.upcast());
        self.form = Some(form);
        name.call_deferred("grab_focus", &[]);
    }

    /// Fill the form (self-tests).
    pub fn preset_creation(&mut self, choice: &CharacterChoice) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        form.name.set_text(&choice.name);
        let cat = form.catalog.clone();
        let roles: Vec<(i32, String, String)> = cat
            .roles
            .iter()
            .map(|r| (r.idx, r.code.clone(), r.name.clone()))
            .collect();
        Form::preset(&mut form.role, &choice.role, &roles);
        form.restrict(true);
        let races: Vec<(i32, String, String)> = cat
            .races
            .iter()
            .map(|r| (r.idx, r.code.clone(), r.noun.clone()))
            .collect();
        Form::preset(&mut form.race, &choice.race, &races);
        form.restrict(false);
        let genders: Vec<(i32, String, String)> = cat
            .genders
            .iter()
            .map(|g| (g.idx, g.code.clone(), g.adj.clone()))
            .collect();
        Form::preset(&mut form.gender, &choice.gender, &genders);
        let aligns: Vec<(i32, String, String)> = cat
            .aligns
            .iter()
            .map(|a| (a.idx, a.code.clone(), a.adj.clone()))
            .collect();
        Form::preset(&mut form.align, &choice.align, &aligns);
    }

    /// The name would restore a saved game: offer to continue it instead.
    pub fn set_name_taken(&mut self, taken: bool) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let name = form.typed_name();
        form.start.set_disabled(taken);
        form.continue_instead.set_visible(taken);
        form.continue_instead
            .set_text(&format!("Continue {name} instead"));
        form.notice.set_text(if taken {
            "A saved game has this name: starting would continue it."
        } else {
            ""
        });
    }

    /// Why the name cannot be used (None: it can).
    pub fn set_name_error(&mut self, error: Option<&str>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if let Some(e) = error {
            form.start.set_disabled(true);
            form.notice.set_text(e);
        }
    }

    pub fn show_end(&mut self, summary: &EndSummary) {
        let mut panel = PanelContainer::new_alloc();
        let mut col = column();
        col.add_child(&title_label("The game is over", 32));
        let mut text = RichTextLabel::new_alloc();
        text.set_use_bbcode(true);
        text.set_focus_mode(FocusMode::NONE);
        text.set_custom_minimum_size(Vector2::new(980.0, 600.0));
        let mut parts: Vec<String> = Vec::new();
        if !summary.last_messages.is_empty() {
            let msgs: Vec<String> = summary
                .last_messages
                .iter()
                .map(|m| bbcode_escape(m))
                .collect();
            parts.push(format!(
                "[color={}]{}[/color]",
                hex(theme::TEXT_DIM),
                msgs.join("\n")
            ));
        }
        if !summary.text.is_empty() {
            let lines: Vec<String> = summary.text.iter().map(|l| bbcode_escape(l)).collect();
            parts.push(lines.join("\n"));
        }
        if !summary.scores.is_empty() {
            let lines: Vec<String> = summary.scores.iter().map(|l| bbcode_escape(l)).collect();
            parts.push(format!(
                "[color={}]{}[/color]",
                hex(theme::ACCENT),
                lines.join("\n")
            ));
        }
        text.set_text(&parts.join("\n\n"));
        col.add_child(&text);
        let mut buttons = row();
        buttons.add_child(&theme::button("New game", &self.queue, UiEvent::NewGame));
        buttons.add_child(&theme::button("Title", &self.queue, UiEvent::BackToTitle));
        col.add_child(&buttons);
        panel.add_child(&col);
        self.end = Some(summary.clone());
        self.form = None;
        self.page("end", &panel.upcast());
    }

    /// `can_continue`: the name of a save to offer.
    pub fn show_error(&mut self, what: &str, details: &str, can_continue: Option<&str>) {
        let mut panel = PanelContainer::new_alloc();
        let mut col = column();
        col.add_child(&title_label("Something went wrong", 32));
        let mut l = theme::label(what);
        l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        l.set_custom_minimum_size(Vector2::new(900.0, 0.0));
        col.add_child(&l);
        if !details.trim().is_empty() {
            let mut text = RichTextLabel::new_alloc();
            text.set_focus_mode(FocusMode::NONE);
            text.set_custom_minimum_size(Vector2::new(900.0, 360.0));
            text.add_theme_color_override("default_color", theme::TEXT_DIM);
            text.set_text(details);
            col.add_child(&text);
        }
        let mut buttons = row();
        if let Some(name) = can_continue {
            let ev = UiEvent::ContinueGame(name.to_string());
            buttons.add_child(&theme::button(&format!("Continue {name}"), &self.queue, ev));
        }
        buttons.add_child(&theme::button("Title", &self.queue, UiEvent::BackToTitle));
        buttons.add_child(&theme::button("Quit", &self.queue, UiEvent::QuitApp));
        col.add_child(&buttons);
        panel.add_child(&col);
        self.form = None;
        self.page("error", &panel.upcast());
    }

    pub fn hide(&mut self) {
        if let Some(mut c) = self.content.take() {
            c.queue_free();
        }
        self.current = None;
        self.form = None;
        self.root.set_visible(false);
    }

    /// "title", "creation", "end", "error" or None.
    pub fn current(&self) -> Option<&'static str> {
        self.current
    }

    /// What the end screen shows.
    pub fn end_summary(&self) -> Option<&EndSummary> {
        self.end.as_ref().filter(|_| self.current == Some("end"))
    }

    /// Is there a button with this text on the page? (self-tests)
    pub fn has_button(&self, text: &str) -> bool {
        fn walk(node: &Gd<Node>, text: &str) -> bool {
            if let Ok(b) = node.clone().try_cast::<Button>()
                && b.get_text().to_string().starts_with(text)
            {
                return true;
            }
            node.get_children().iter_shared().any(|c| walk(&c, text))
        }
        self.content
            .as_ref()
            .is_some_and(|c| walk(&c.clone().upcast(), text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn times_format_as_utc_dates() {
        let t = UNIX_EPOCH + Duration::from_secs(1_768_694_400);
        assert_eq!(format_time(t), "2026-01-18 00:00");
        assert_eq!(format_time(UNIX_EPOCH), "1970-01-01 00:00");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400 + 3_661);
        assert_eq!(format_time(leap), "2000-02-29 01:01");
    }
}
