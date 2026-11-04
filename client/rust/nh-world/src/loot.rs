//! A container's looting (#loot on a box, apply on a bag) driven for a
//! panel of two columns, the pack and the container: NetHack's own menus
//! in their order, the ones the panel stands for answered by it.
//!
//! pickup.c, with menustyle full: "Do what with <the container>?" (take
//! out, put in, both...), then for taking out "Take out what type of
//! objects?" (skipped by the engine when there is one class) and "Take
//! out what?", the container's things; for putting in, the same two for
//! the pack. The flow answers "both; take out, then put in" (or the one
//! the container allows) and "All types" itself; the first list of
//! things opens the panel, the player's picks in it answer that list,
//! and the pack's picks the put-in list after it. Every move is the
//! engine's, by its own menus: the panel only chooses in them.

use nh_protocol::{MenuItem, PickHow, Reply};

use crate::Prompt;

/// What the flow does with a request.
#[derive(Debug, Clone, PartialEq)]
pub enum LootStep {
    /// Answer it so.
    Reply(Reply),
    /// The panel, for this list: the container's things (`take`, the menu
    /// to answer) or, nothing to take out, the pack's to put in.
    Open(LootView),
    /// Not the flow's: as usual (a message, a question of the engine's).
    Pass,
    /// Back at the command prompt: the flow is over.
    Done,
}

/// What the panel shows.
#[derive(Debug, Clone, PartialEq)]
pub struct LootView {
    /// The container as the engine names it ("the large box", "your bag").
    pub container: String,
    /// The container's things, the "Take out what?" menu's entries (none
    /// when it is empty, or the request is the put-in list).
    pub take: Vec<MenuItem>,
    /// The request is "Put in what?" (no list to take out came first).
    pub putting: bool,
}

/// The looting in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct LootFlow {
    container: String,
    /// The pack's picks, made in the panel while it answered the list to
    /// take out: they answer the put-in list (None: none made yet).
    put_in: Option<Vec<(char, Option<u32>)>>,
    /// The panel answered a list (a put-in list after it is the flow's).
    answered: bool,
}

const DO_WHAT: &str = "Do what with ";
const EMPTY_DO_WHAT: &str = " is empty.  Do what with it?";
const TAKE_TYPES: &str = "Take out what type of objects?";
const TAKE: &str = "Take out what?";
const PUT_TYPES: &str = "Put in what type of objects?";
const PUT: &str = "Put in what?";

/// The container a "Do what with …?" title is about.
fn container_of(title: &str) -> Option<String> {
    if let Some(rest) = title.strip_prefix(DO_WHAT) {
        return rest.strip_suffix('?').map(str::to_string);
    }
    title.strip_suffix(EMPTY_DO_WHAT).map(str::to_string)
}

fn text(i: &MenuItem) -> &str {
    i.str.as_deref().unwrap_or("")
}

/// The selectable entry whose text starts with `start`.
fn entry<'a>(items: &'a [MenuItem], start: &str) -> Option<&'a MenuItem> {
    items
        .iter()
        .find(|i| i.selectable && text(i).starts_with(start))
}

fn pick(i: &MenuItem) -> Option<Reply> {
    Some(Reply::Menu(vec![(u32::try_from(i.idx).ok()?, -1)]))
}

impl LootFlow {
    /// A "Do what with the container?" menu: the flow begun, with its
    /// answer (both ways when it may, else the one way it may). None: not
    /// that menu, or nothing in it to take out or put in.
    pub fn start(prompt: &Prompt) -> Option<(LootFlow, Reply)> {
        let Prompt::Menu {
            how: PickHow::One,
            title: Some(title),
            items,
            ..
        } = prompt
        else {
            return None;
        };
        let container = container_of(title)?;
        let way = ["both; take out", "take something out", "put something in"]
            .iter()
            .find_map(|w| entry(items, w))?;
        let flow = LootFlow {
            container,
            put_in: None,
            answered: false,
        };
        Some((flow, pick(way)?))
    }

    /// The next request while looting.
    pub fn on_prompt(&mut self, prompt: &Prompt) -> LootStep {
        let (title, items) = match prompt {
            Prompt::Command => return LootStep::Done,
            Prompt::Menu {
                title: Some(t),
                items,
                how: PickHow::Any,
                ..
            } => (t.as_str(), items),
            _ => return LootStep::Pass,
        };
        match title {
            // every type: the list itself is the choice
            TAKE_TYPES | PUT_TYPES => match entry(items, "All types").and_then(pick) {
                Some(r) => LootStep::Reply(r),
                None => LootStep::Pass,
            },
            TAKE => LootStep::Open(LootView {
                container: self.container.clone(),
                take: items.clone(),
                putting: false,
            }),
            PUT if self.answered => {
                let picks = self.put_in.take().unwrap_or_default();
                let chosen: Vec<(u32, i64)> = picks
                    .iter()
                    .filter_map(|(letter, count)| {
                        let i = items
                            .iter()
                            .find(|i| i.selectable && i.ch == *letter as i32)?;
                        Some((u32::try_from(i.idx).ok()?, count.map_or(-1, i64::from)))
                    })
                    .collect();
                LootStep::Reply(if chosen.is_empty() {
                    Reply::Cancel
                } else {
                    Reply::Menu(chosen)
                })
            }
            PUT => LootStep::Open(LootView {
                container: self.container.clone(),
                take: Vec::new(),
                putting: true,
            }),
            _ => LootStep::Pass,
        }
    }

    /// The panel answered its list; `put_in`, the pack's picks, for the
    /// put-in list to come (when the list it answered was the one to take
    /// out).
    pub fn answered(&mut self, put_in: Vec<(char, Option<u32>)>) {
        self.answered = true;
        self.put_in = Some(put_in);
    }

    /// The container as the engine names it.
    pub fn container(&self) -> &str {
        &self.container
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(idx: i32, ch: char, text: &str) -> MenuItem {
        MenuItem {
            win: 3,
            idx,
            glyph: None,
            selectable: !text.is_empty(),
            ch: ch as i32,
            gch: 0,
            attr: 0,
            clr: 8,
            str: Some(text.into()),
            preselected: false,
            skipinvert: false,
        }
    }

    fn menu(how: PickHow, title: &str, items: Vec<MenuItem>) -> Prompt {
        Prompt::Menu {
            win: 3,
            how,
            title: Some(title.into()),
            items,
        }
    }

    fn do_what(both: bool) -> Prompt {
        let mut items = vec![item(1, ':', "Look inside the large box")];
        items.push(item(2, 'o', "take something out"));
        if both {
            items.push(item(3, 'i', "put something in"));
            items.push(item(4, 'b', "both; take out, then put in"));
            items.push(item(5, 'r', "both reversed; put in, then take out"));
        }
        items.push(item(0, '\0', ""));
        items.push(item(8, 'q', "do nothing"));
        menu(PickHow::One, "Do what with the large box?", items)
    }

    #[test]
    fn a_box_is_looted_both_ways_through_the_engine_s_menus() {
        let (mut flow, r) = LootFlow::start(&do_what(true)).expect("the flow");
        assert_eq!(r, Reply::Menu(vec![(4, -1)]));
        assert_eq!(flow.container(), "the large box");
        // all types, then the panel for the box's things
        let types = menu(
            PickHow::Any,
            TAKE_TYPES,
            vec![item(1, 'a', "All types"), item(2, 'b', "Weapons")],
        );
        assert_eq!(
            flow.on_prompt(&types),
            LootStep::Reply(Reply::Menu(vec![(1, -1)]))
        );
        let things = vec![item(1, 'a', "a dagger"), item(2, 'b', "3 food rations")];
        let take = menu(PickHow::Any, TAKE, things.clone());
        assert_eq!(
            flow.on_prompt(&take),
            LootStep::Open(LootView {
                container: "the large box".into(),
                take: things,
                putting: false,
            })
        );
        // the panel took the dagger and puts the pack's c and 2 of f in
        flow.answered(vec![('c', None), ('f', Some(2)), ('z', None)]);
        let put = menu(
            PickHow::Any,
            PUT,
            vec![item(10, 'c', "a spear"), item(11, 'f', "5 arrows")],
        );
        assert_eq!(
            flow.on_prompt(&put),
            LootStep::Reply(Reply::Menu(vec![(10, -1), (11, 2)]))
        );
        assert_eq!(flow.on_prompt(&Prompt::Command), LootStep::Done);
    }

    #[test]
    fn nothing_put_in_cancels_the_put_in_list() {
        let (mut flow, _) = LootFlow::start(&do_what(true)).unwrap();
        flow.answered(Vec::new());
        let put = menu(PickHow::Any, PUT, vec![item(10, 'c', "a spear")]);
        assert_eq!(flow.on_prompt(&put), LootStep::Reply(Reply::Cancel));
    }

    #[test]
    fn an_empty_box_opens_the_panel_on_the_put_in_list() {
        let empty = menu(
            PickHow::One,
            "The large box is empty.  Do what with it?",
            vec![
                item(1, ':', "Look inside the large box"),
                item(3, 'i', "put something in"),
                item(8, 'q', "do nothing"),
            ],
        );
        let (mut flow, r) = LootFlow::start(&empty).expect("the flow");
        assert_eq!(r, Reply::Menu(vec![(3, -1)]));
        assert_eq!(flow.container(), "The large box");
        let put = menu(PickHow::Any, PUT, vec![item(10, 'c', "a spear")]);
        assert!(matches!(
            flow.on_prompt(&put),
            LootStep::Open(LootView { putting: true, .. })
        ));
    }

    #[test]
    fn only_out_when_nothing_can_go_in_and_other_menus_pass() {
        let (mut flow, r) = LootFlow::start(&do_what(false)).unwrap();
        assert_eq!(r, Reply::Menu(vec![(2, -1)]));
        // a message, another question: not the flow's
        let other = menu(PickHow::One, "Something else?", vec![item(1, 'a', "x")]);
        assert_eq!(flow.on_prompt(&other), LootStep::Pass);
        assert!(LootFlow::start(&other).is_none());
        // a menu that is not a container's
        let none = menu(
            PickHow::One,
            "Do what with the large box?",
            vec![item(8, 'q', "do nothing")],
        );
        assert!(LootFlow::start(&none).is_none());
    }
}
