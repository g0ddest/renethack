//! Macros: the short scripts of keys behind a button, a drag or a bar
//! slot ("wield b" is `w`, then `b` at the getobj question). A macro only
//! answers the prompts it expects, each checked by its kind and text; the
//! first prompt it does not expect goes to the player and ends it. It
//! never types ahead: each key answers a request that is waiting.

use std::collections::VecDeque;

use nh_protocol::{InvItem, MenuItem, Reply};

use crate::{ItemActionKind, Prompt, World, count_keys, item_question, worn_accessory, worn_armor};

/// Questions about the floor that come before the getobj question of
/// some commands (eat, quaff, dip, offer, tip); a macro about an item in
/// the pack answers them "no" so that item is used.
const FLOOR_GUARDS: [&str; 7] = [
    // "There is a lichen corpse here; eat it?", "There are 2 food
    // rations here; eat one?", sacrifice, open (a tin)
    " here; ",
    // "There is a bear trap here (...); eat it?"
    "; eat it?",
    // "There is a large box here, tip it?"
    " here, tip it?",
    "Drink from the fountain?",
    "Drink from the sink?",
    " into the fountain?",
    " into the sink?",
];

/// The commands whose macros answer floor questions.
fn guarded(key: char) -> bool {
    matches!(key, 'e' | 'q')
}

fn guarded_ext(name: &str) -> bool {
    matches!(name, "dip" | "offer" | "tip")
}

/// What a menu step picks.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pick {
    /// The first entry whose text contains this.
    Containing(&'static str),
    /// The entry whose text is this name, then spaces or a tab (the spell
    /// menu's columns); any case.
    Named(String),
    /// Entries by their letter, with a count (None: all of it).
    Letters(Vec<(char, Option<u32>)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// The command prompt: this key.
    Key(i32),
    /// The extended command prompt: this command.
    Ext(&'static str),
    /// getobj() with a verb starting with `verb`: `letter` (or '-'),
    /// after the count.
    Item {
        verb: &'static str,
        letter: char,
        count: Option<u32>,
        /// Skipped when the engine does not ask (`T` with one thing worn).
        optional: bool,
    },
    /// A yn_function whose query contains `pattern`: `answer`.
    Answer { pattern: &'static str, answer: char },
    /// A menu whose title contains `title`.
    Menu {
        title: &'static str,
        pick: Pick,
        optional: bool,
    },
}

impl Step {
    fn optional(&self) -> bool {
        matches!(
            self,
            Step::Item { optional: true, .. } | Step::Menu { optional: true, .. }
        )
    }
}

/// A script of expected prompts and their answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    steps: Vec<Step>,
    /// Answer floor questions "no".
    guards: bool,
    /// The item action and the letter it is about, for the UI (what the
    /// hero uses: decision 8).
    pub action: Option<(ItemActionKind, char)>,
}

impl Macro {
    fn new(steps: Vec<Step>) -> Macro {
        let guards = steps.iter().any(|s| match s {
            Step::Key(k) => u8::try_from(*k).is_ok_and(|c| guarded(c as char)),
            Step::Ext(name) => guarded_ext(name),
            _ => false,
        });
        Macro {
            steps,
            guards,
            action: None,
        }
    }

    fn about(mut self, kind: ItemActionKind, letter: char) -> Macro {
        self.action = Some((kind, letter));
        self
    }

    /// An item action (ui-design §5.2) on `item`; `count` for the ones
    /// that take part of a stack (drop, throw, quiver). None for the
    /// actions that need another choice first ([`Macro::dip`],
    /// [`Macro::adjust`]).
    pub fn item(kind: ItemActionKind, item: &InvItem, count: Option<u32>) -> Option<Macro> {
        use ItemActionKind::*;
        let l = item.letter;
        let key = |c: char| Step::Key(c as i32);
        let get = |verb, letter| Step::Item {
            verb,
            letter,
            count: None,
            optional: false,
        };
        let counted = |verb| Step::Item {
            verb,
            letter: l,
            count,
            optional: false,
        };
        let maybe = |verb| Step::Item {
            verb,
            letter: l,
            count: None,
            optional: true,
        };
        let ext = |name| [key('#'), Step::Ext(name)];
        let finger = |answer| Step::Answer {
            pattern: "Right or Left?",
            answer,
        };
        let steps: Vec<Step> = match kind {
            Wield => vec![key('w'), counted("wield")],
            Unwield => vec![key('w'), get("wield", '-')],
            SetAlternate => vec![key('w'), get("wield", l), key('x')],
            SwapWeapons => vec![key('x')],
            Quiver => vec![key('Q'), counted("ready")],
            EmptyQuiver => vec![key('Q'), get("ready", '-')],
            // the engine asks only when the quiver is empty
            Fire => vec![key('f'), maybe("fire")],
            Throw => vec![key('t'), counted("throw")],
            Apply | Break => vec![key('a'), get("use or apply", l)],
            Wear => vec![key('W'), get("wear", l)],
            TakeOff => vec![key('T'), maybe("take off")],
            PutOn => vec![key('P'), get("put on", l)],
            PutOnLeft => vec![key('P'), get("put on", l), finger('l')],
            PutOnRight => vec![key('P'), get("put on", l), finger('r')],
            Remove => vec![key('R'), maybe("remove")],
            Eat => vec![key('e'), get("eat", l)],
            Quaff => vec![key('q'), get("drink", l)],
            Read => vec![key('r'), get("read", l)],
            Zap => vec![key('z'), get("zap", l)],
            Engrave => vec![key('E'), get("write with", l)],
            Drop | DropSome => {
                let mut s = Vec::new();
                // a worn thing comes off first: two actions
                if worn_armor(item) {
                    s.extend([key('T'), maybe("take off")]);
                } else if worn_accessory(item) {
                    s.extend([key('R'), maybe("remove")]);
                }
                s.extend([key('d'), counted("drop")]);
                s
            }
            Name | Call => {
                let what = if kind == Name {
                    "a particular object in inventory"
                } else {
                    "the type of an object in inventory"
                };
                let mut s = ext("name").to_vec();
                s.push(Step::Menu {
                    title: "What do you want to name?",
                    pick: Pick::Containing(what),
                    optional: false,
                });
                s.push(get(if kind == Name { "name" } else { "call" }, l));
                s
            }
            TwoWeapon => ext("twoweapon").to_vec(),
            Force => ext("force").to_vec(),
            Rub => {
                let mut s = ext("rub").to_vec();
                s.push(get("rub", l));
                s
            }
            Tip => {
                let mut s = ext("tip").to_vec();
                s.push(get("tip", l));
                s
            }
            Dip | Adjust | Split => return None,
        };
        Some(Macro::new(steps).about(kind, l))
    }

    /// `#dip` the item into another one (`into`, chosen first in the UI).
    pub fn dip(item: &InvItem, into: char) -> Macro {
        Macro::new(vec![
            Step::Key('#' as i32),
            Step::Ext("dip"),
            Step::Item {
                verb: "dip",
                letter: item.letter,
                count: None,
                optional: false,
            },
            Step::Item {
                verb: "dip",
                letter: into,
                count: None,
                optional: false,
            },
        ])
        .about(ItemActionKind::Dip, item.letter)
    }

    /// `#adjust` the item to letter `to`; with a count, split that many
    /// off to it.
    pub fn adjust(item: &InvItem, to: char, count: Option<u32>) -> Macro {
        let kind = if count.is_some() {
            ItemActionKind::Split
        } else {
            ItemActionKind::Adjust
        };
        Macro::new(vec![
            Step::Key('#' as i32),
            Step::Ext("adjust"),
            Step::Item {
                verb: "adjust",
                letter: item.letter,
                count,
                optional: false,
            },
            Step::Answer {
                pattern: " to what [",
                answer: to,
            },
        ])
        .about(kind, item.letter)
    }

    /// `D`: drop these items (letter, count). "All types" in the first
    /// menu (it does not come unless menustyle is full), then the items;
    /// never "Auto-select every item".
    pub fn drop_many(items: &[(char, Option<u32>)]) -> Macro {
        Macro::new(vec![
            Step::Key('D' as i32),
            Step::Menu {
                title: "Drop what type of items?",
                pick: Pick::Containing("All types"),
                optional: true,
            },
            Step::Menu {
                title: "What would you like to drop?",
                pick: Pick::Letters(items.to_vec()),
                optional: false,
            },
        ])
    }

    /// `Z`, then the spell by its name in the menu (not by a letter: the
    /// letters change when spells are learned or reordered).
    pub fn cast(spell: &str) -> Macro {
        Macro::new(vec![
            Step::Key('Z' as i32),
            Step::Menu {
                title: "Choose which spell to cast",
                pick: Pick::Named(spell.to_string()),
                optional: false,
            },
        ])
    }

    /// A command key; with a count it goes first, as tty reads one (`n20`
    /// with number_pad). Repeated keys (search, rest, steps) had better
    /// be the client's Repeat order, which stops for a hostile.
    pub fn key(key: i32, count: Option<u32>, number_pad: bool) -> Macro {
        let mut steps: Vec<Step> = count
            .map(|n| count_keys(n, number_pad))
            .unwrap_or_default()
            .into_iter()
            .map(Step::Key)
            .collect();
        steps.push(Step::Key(key));
        Macro::new(steps)
    }

    /// An extended command by name (`#pray`): `#`, then the name.
    pub fn ext(name: &'static str) -> Macro {
        Macro::new(vec![Step::Key('#' as i32), Step::Ext(name)])
    }
}

/// What to do with a prompt while a macro runs.
#[derive(Debug, Clone, PartialEq)]
pub enum MacroStep {
    /// Answer it with this; the macro goes on.
    Reply(Reply),
    /// Not a question (a text window, a map pause): show it as usual; the
    /// macro goes on after it.
    Pass,
    /// The macro did not expect it and has ended: the player answers.
    HandToPlayer,
    /// The macro has ended at this command prompt, which is the player's.
    Done,
}

#[derive(Debug, Clone)]
struct Running {
    m: Macro,
    next: usize,
    /// The rest of a count and the letter, for getobj's get_count(): it
    /// reads them one key at a time as command-prompt keys.
    typing: VecDeque<i32>,
}

/// Runs one macro at a time; lives next to the TickDriver in the game.
#[derive(Debug, Clone, Default)]
pub struct MacroRunner {
    running: Option<Running>,
    /// The last macro answered every step it had.
    completed: bool,
}

impl MacroRunner {
    pub fn new() -> MacroRunner {
        MacroRunner::default()
    }

    pub fn is_active(&self) -> bool {
        self.running.is_some()
    }

    /// The running macro's item action, for the UI.
    pub fn action(&self) -> Option<(ItemActionKind, char)> {
        self.running.as_ref().and_then(|r| r.m.action)
    }

    /// Whether the last macro that ended got through all its steps (false
    /// when the engine cut it short: "You don't have anything to drink.").
    pub fn completed(&self) -> bool {
        self.completed
    }

    /// Start at a command prompt (after stopping any order): the reply
    /// to it. None when the macro does not begin with a command key.
    pub fn start(&mut self, m: Macro) -> Option<Reply> {
        let Some(&Step::Key(k)) = m.steps.first() else {
            return None;
        };
        self.completed = m.steps.len() == 1;
        self.running = Some(Running {
            m,
            next: 1,
            typing: VecDeque::new(),
        });
        Some(Reply::Key(k))
    }

    /// Stop without a word (Esc, a new game).
    pub fn cancel(&mut self) {
        self.running = None;
    }

    /// The next request, as the world sees it.
    pub fn on_prompt(&mut self, prompt: &Prompt, world: &World) -> MacroStep {
        let Some(run) = self.running.as_mut() else {
            return MacroStep::HandToPlayer;
        };
        if matches!(
            prompt,
            Prompt::Show { .. }
                | Prompt::MapPause
                | Prompt::AutoAck
                | Prompt::MessageMenu { pick: false, .. }
        ) {
            return MacroStep::Pass;
        }
        let command = *prompt == Prompt::Command && !world.getpos;
        if !run.typing.is_empty() {
            if command || *prompt == Prompt::Key {
                let k = run.typing.pop_front().unwrap_or(0);
                if run.typing.is_empty() {
                    self.after_step();
                }
                return MacroStep::Reply(Reply::Key(k));
            }
            return self.end(MacroStep::HandToPlayer);
        }
        if run.m.guards
            && let Prompt::Choice { query, allowed, .. } = prompt
            && allowed.contains(&'n')
            && FLOOR_GUARDS.iter().any(|g| query.contains(g))
        {
            return MacroStep::Reply(Reply::Char('n' as i32));
        }
        while let Some(step) = run.m.steps.get(run.next) {
            if let Some(reply) = answer(step, prompt, command, &mut run.typing) {
                run.next += 1;
                if run.typing.is_empty() {
                    self.after_step();
                }
                return MacroStep::Reply(reply);
            }
            if !step.optional() {
                break;
            }
            run.next += 1;
        }
        let done = run.next >= run.m.steps.len();
        if command {
            self.completed = done;
            self.end(MacroStep::Done)
        } else {
            self.completed = done;
            self.end(MacroStep::HandToPlayer)
        }
    }

    /// A step was answered: whether it was the last one.
    fn after_step(&mut self) {
        if let Some(run) = &self.running
            && run.next >= run.m.steps.len()
            && run.typing.is_empty()
        {
            self.completed = true;
        }
    }

    fn end(&mut self, step: MacroStep) -> MacroStep {
        self.running = None;
        step
    }
}

/// The answer `step` gives `prompt`, if it expects it.
fn answer(
    step: &Step,
    prompt: &Prompt,
    command: bool,
    typing: &mut VecDeque<i32>,
) -> Option<Reply> {
    match (step, prompt) {
        (Step::Key(k), _) if command => Some(Reply::Key(*k)),
        (Step::Ext(name), Prompt::ExtCmd) => Some(Reply::ExtCmd(Some(name.to_string()))),
        (
            Step::Item {
                verb,
                letter,
                count,
                ..
            },
            _,
        ) => {
            let q = item_question(prompt)?;
            if !q.verb.starts_with(verb) {
                return None;
            }
            match count {
                Some(n) if *n > 0 && q.takes_count() => {
                    // the first digit answers getobj, get_count reads the rest
                    let mut keys: VecDeque<i32> = n.to_string().bytes().map(i32::from).collect();
                    let first = keys.pop_front()?;
                    keys.push_back(*letter as i32);
                    *typing = keys;
                    Some(Reply::Char(first))
                }
                _ => Some(Reply::Char(*letter as i32)),
            }
        }
        (Step::Answer { pattern, answer }, Prompt::Choice { query, allowed, .. }) => {
            (query.contains(pattern) && allowed.contains(answer))
                .then_some(Reply::Char(*answer as i32))
        }
        (
            Step::Answer { pattern, answer },
            Prompt::FreeKey {
                query,
                directions: false,
            },
        ) => query
            .contains(pattern)
            .then_some(Reply::Char(*answer as i32)),
        (
            Step::Menu { title, pick, .. },
            Prompt::Menu {
                title: t, items, ..
            },
        ) => {
            if !t.as_deref().is_some_and(|t| t.contains(title)) {
                return None;
            }
            pick_entries(pick, items)
        }
        _ => None,
    }
}

fn pick_entries(pick: &Pick, items: &[MenuItem]) -> Option<Reply> {
    let text = |i: &MenuItem| i.str.clone().unwrap_or_default();
    let idx = |i: &MenuItem| u32::try_from(i.idx).ok();
    let picks: Vec<(u32, i64)> = match pick {
        Pick::Containing(s) => {
            let i = items.iter().find(|i| i.selectable && text(i).contains(s))?;
            vec![(idx(i)?, -1)]
        }
        Pick::Named(name) => {
            let name = name.to_lowercase();
            let i = items.iter().find(|i| {
                let t = text(i).to_lowercase();
                i.selectable
                    && t.strip_prefix(&name)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
            })?;
            vec![(idx(i)?, -1)]
        }
        Pick::Letters(want) => {
            let mut v = Vec::new();
            for (letter, count) in want {
                let i = items
                    .iter()
                    .find(|i| i.selectable && i.ch == *letter as i32)?;
                v.push((idx(i)?, count.map_or(-1, i64::from)));
            }
            v
        }
    };
    Some(Reply::Menu(picks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ItemActionKind::*;
    use nh_protocol::{PickHow, Slot};

    fn item(letter: char, class: char, slots: &[Slot], text: &str) -> InvItem {
        InvItem {
            letter,
            class,
            tile: 1,
            quan: 1,
            slots: slots.to_vec(),
            lit: false,
            text: text.into(),
        }
    }

    fn getobj(q: &str) -> Prompt {
        Prompt::FreeKey {
            query: q.into(),
            directions: false,
        }
    }

    fn yn(q: &str, allowed: &str) -> Prompt {
        Prompt::Choice {
            query: q.into(),
            visible: allowed.chars().collect(),
            allowed: allowed.chars().collect(),
            default: Some('n'),
        }
    }

    fn key(c: char) -> MacroStep {
        MacroStep::Reply(Reply::Key(c as i32))
    }

    fn ch(c: char) -> MacroStep {
        MacroStep::Reply(Reply::Char(c as i32))
    }

    fn run(m: Macro) -> (MacroRunner, World) {
        let mut r = MacroRunner::new();
        assert!(r.start(m).is_some());
        (r, World::new())
    }

    #[test]
    fn wield_answers_the_command_and_the_question() {
        let dagger = item('b', ')', &[Slot::Alternate], "a +0 dagger");
        let mut r = MacroRunner::new();
        let w = World::new();
        assert_eq!(
            r.start(Macro::item(Wield, &dagger, None).unwrap()),
            Some(Reply::Key('w' as i32))
        );
        assert!(r.is_active());
        assert_eq!(r.action(), Some((Wield, 'b')));
        assert_eq!(
            r.on_prompt(&getobj("What do you want to wield? [- ab or ?*]"), &w),
            ch('b')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        assert!(!r.is_active() && r.completed());
    }

    #[test]
    fn an_unexpected_question_goes_to_the_player() {
        let potion = item('f', '!', &[], "a bubbly potion");
        let (mut r, w) = run(Macro::item(Quaff, &potion, None).unwrap());
        assert_eq!(
            r.on_prompt(&yn("Really quaff it while levitating?", "yn"), &w),
            MacroStep::HandToPlayer
        );
        assert!(!r.is_active() && !r.completed());
        // nothing to drink: the engine says so and the command prompt comes
        let (mut r, w) = run(Macro::item(Quaff, &potion, None).unwrap());
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        assert!(!r.completed());
        // another verb is another question
        let (mut r, w) = run(Macro::item(Quaff, &potion, None).unwrap());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to eat? [d or ?*]"), &w),
            MacroStep::HandToPlayer
        );
    }

    #[test]
    fn floor_questions_are_answered_no_for_pack_items() {
        let food = item('d', '%', &[], "an uncursed food ration");
        let (mut r, w) = run(Macro::item(Eat, &food, None).unwrap());
        assert_eq!(
            r.on_prompt(&yn("There is a lichen corpse here; eat it?", "ynq"), &w),
            ch('n')
        );
        assert_eq!(
            r.on_prompt(&getobj("What do you want to eat? [d or ?*]"), &w),
            ch('d')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        let potion = item('f', '!', &[], "a bubbly potion");
        let (mut r, w) = run(Macro::item(Quaff, &potion, None).unwrap());
        assert_eq!(
            r.on_prompt(&yn("Drink from the fountain?", "yn"), &w),
            ch('n')
        );
        assert_eq!(
            r.on_prompt(&getobj("What do you want to drink? [f or ?*]"), &w),
            ch('f')
        );
        // put on takes no count: the letter
        let ring = item('g', '=', &[], "a jade ring");
        let (mut r, w) = run(Macro::item(PutOn, &ring, None).unwrap());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to put on? [g or ?*]"), &w),
            ch('g')
        );
        let (mut r, w) = run(Macro::item(Tip, &item('m', '(', &[], "a bag"), None).unwrap());
        r.on_prompt(&Prompt::ExtCmd, &w);
        assert_eq!(
            r.on_prompt(&yn("There is a large box here, tip it?", "ynq"), &w),
            ch('n')
        );
        // other macros leave floor questions to the player
        let (mut r, w) = run(Macro::item(Read, &item('g', '?', &[], "a scroll"), None).unwrap());
        assert_eq!(
            r.on_prompt(&yn("There is a scroll here; read it?", "yn"), &w),
            MacroStep::HandToPlayer
        );
    }

    #[test]
    fn a_count_goes_digit_by_digit() {
        let arrows = InvItem {
            quan: 30,
            ..item('c', ')', &[], "30 +0 arrows")
        };
        let (mut r, w) = run(Macro::item(Drop, &arrows, Some(12)).unwrap());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to drop? [$a-e or ?*]"), &w),
            ch('1')
        );
        // get_count() reads the rest at the command prompt
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('2'));
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('c'));
        assert!(r.completed());
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        // throwing some of a stack
        let (mut r, w) = run(Macro::item(Throw, &arrows, Some(3)).unwrap());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to throw? [$c or ?*]"), &w),
            ch('3')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('c'));
        // then the direction: the player's
        let dir = Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        };
        assert_eq!(r.on_prompt(&dir, &w), MacroStep::HandToPlayer);
        assert!(r.completed());
    }

    #[test]
    fn optional_questions_are_skipped_when_they_do_not_come() {
        let shield = item('c', '[', &[Slot::Shield], "a small shield (being worn)");
        // one thing worn: T takes it off at once
        let (mut r, w) = run(Macro::item(TakeOff, &shield, None).unwrap());
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        assert!(r.completed());
        // dropping it: T first, then d at the next command prompt
        let mut r = MacroRunner::new();
        assert_eq!(
            r.start(Macro::item(Drop, &shield, None).unwrap()),
            Some(Reply::Key('T' as i32))
        );
        assert_eq!(
            r.on_prompt(&getobj("What do you want to take off? [bc or ?*]"), &w),
            ch('c')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('d'));
        assert_eq!(
            r.on_prompt(&getobj("What do you want to drop? [$a-e or ?*]"), &w),
            ch('c')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
    }

    #[test]
    fn two_commands_for_the_alternate_and_rings_pick_a_hand() {
        let axe = item('f', ')', &[], "an axe");
        let (mut r, w) = run(Macro::item(SetAlternate, &axe, None).unwrap());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to wield? [- abf or ?*]"), &w),
            ch('f')
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('x'));
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        let ring = item('g', '=', &[], "a jade ring");
        let (mut r, w) = run(Macro::item(PutOnLeft, &ring, None).unwrap());
        r.on_prompt(&getobj("What do you want to put on? [g or ?*]"), &w);
        assert_eq!(
            r.on_prompt(&yn("Which ring-finger, Right or Left?", "rl"), &w),
            ch('l')
        );
        let unwield = Macro::item(Unwield, &axe, None).unwrap();
        let (mut r, w) = run(unwield);
        assert_eq!(
            r.on_prompt(&getobj("What do you want to wield? [- abf or ?*]"), &w),
            ch('-')
        );
    }

    #[test]
    fn text_windows_pass_through() {
        let lamp = item('e', '(', &[], "an oil lamp");
        let (mut r, w) = run(Macro::item(Apply, &lamp, None).unwrap());
        let show = Prompt::Show {
            title: None,
            lines: vec![],
        };
        assert_eq!(r.on_prompt(&show, &w), MacroStep::Pass);
        assert!(r.is_active());
        assert_eq!(
            r.on_prompt(&getobj("What do you want to use or apply? [e or ?*]"), &w),
            ch('e')
        );
    }

    fn menu(title: &str, entries: &[(char, &str)]) -> Prompt {
        Prompt::Menu {
            win: 5,
            how: PickHow::Any,
            title: Some(title.into()),
            items: entries
                .iter()
                .enumerate()
                .map(|(i, (ch, text))| MenuItem {
                    win: 5,
                    idx: i as i32,
                    glyph: None,
                    selectable: *ch != '\0',
                    ch: *ch as i32,
                    gch: 0,
                    attr: 0,
                    clr: 0,
                    str: Some(text.to_string()),
                    preselected: false,
                    skipinvert: false,
                })
                .collect(),
        }
    }

    #[test]
    fn menus_are_answered_by_text_and_letters() {
        let (mut r, w) = run(Macro::drop_many(&[('d', None), ('f', Some(2))]));
        let types = menu(
            "Drop what type of items?",
            &[
                ('a', "All types"),
                ('b', "Weapons"),
                ('A', "Auto-select every item"),
            ],
        );
        assert_eq!(
            r.on_prompt(&types, &w),
            MacroStep::Reply(Reply::Menu(vec![(0, -1)]))
        );
        let items = menu(
            "What would you like to drop?",
            &[
                ('\0', "Comestibles"),
                ('d', "an uncursed food ration"),
                ('f', "3 bubbly potions"),
            ],
        );
        assert_eq!(
            r.on_prompt(&items, &w),
            MacroStep::Reply(Reply::Menu(vec![(1, -1), (2, 2)]))
        );
        // without the first menu, straight to the items
        let (mut r, w) = run(Macro::drop_many(&[('d', None)]));
        assert_eq!(
            r.on_prompt(&items, &w),
            MacroStep::Reply(Reply::Menu(vec![(1, -1)]))
        );
        // a letter that is not there: the player's menu
        let (mut r, w) = run(Macro::drop_many(&[('z', None)]));
        assert_eq!(r.on_prompt(&items, &w), MacroStep::HandToPlayer);
    }

    #[test]
    fn spells_by_name_and_names_by_menu() {
        let spells = menu(
            "Choose which spell to cast",
            &[
                (
                    '\0',
                    "    Name                 Level Category     Fail Retention",
                ),
                ('a', "force bolt            1   attack         0%      100%"),
                ('b', "force bolt II         2   attack         0%      100%"),
                ('c', "sleep                 1   enchantment   10%      100%"),
            ],
        );
        let (mut r, w) = run(Macro::cast("Sleep"));
        assert_eq!(
            r.on_prompt(&spells, &w),
            MacroStep::Reply(Reply::Menu(vec![(3, -1)]))
        );
        let (mut r, w) = run(Macro::cast("force bolt"));
        assert_eq!(
            r.on_prompt(&spells, &w),
            MacroStep::Reply(Reply::Menu(vec![(1, -1)]))
        );
        let (mut r, w) = run(Macro::cast("magic missile"));
        assert_eq!(r.on_prompt(&spells, &w), MacroStep::HandToPlayer);

        let dagger = item('b', ')', &[], "a +0 dagger");
        let (mut r, w) = run(Macro::item(Call, &dagger, None).unwrap());
        assert_eq!(
            r.on_prompt(&Prompt::ExtCmd, &w),
            MacroStep::Reply(Reply::ExtCmd(Some("name".into())))
        );
        let what = menu(
            "What do you want to name?",
            &[
                ('m', "a monster"),
                ('i', "a particular object in inventory"),
                ('o', "the type of an object in inventory"),
            ],
        );
        assert_eq!(
            r.on_prompt(&what, &w),
            MacroStep::Reply(Reply::Menu(vec![(2, -1)]))
        );
        assert_eq!(
            r.on_prompt(&getobj("What do you want to call? [b or ?*]"), &w),
            ch('b')
        );
        let text = Prompt::Text {
            query: "Call a dagger:".into(),
            name: false,
        };
        assert_eq!(r.on_prompt(&text, &w), MacroStep::HandToPlayer);
    }

    #[test]
    fn adjust_dip_and_counted_commands() {
        let dagger = item('b', ')', &[], "a +0 dagger");
        let (mut r, w) = run(Macro::adjust(&dagger, 'q', None));
        r.on_prompt(&Prompt::ExtCmd, &w);
        assert_eq!(
            r.on_prompt(&getobj("What do you want to adjust? [$a-e or ?*]"), &w),
            ch('b')
        );
        let to = Prompt::FreeKey {
            query: "Adjust letter to what [c-zA-Z] (? see used letters)?".into(),
            directions: false,
        };
        assert_eq!(r.on_prompt(&to, &w), ch('q'));
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);

        let (mut r, w) = run(Macro::dip(&dagger, 'f'));
        r.on_prompt(&Prompt::ExtCmd, &w);
        assert_eq!(
            r.on_prompt(&yn("Dip the dagger into the fountain?", "yn"), &w),
            ch('n')
        );
        assert_eq!(
            r.on_prompt(&getobj("What do you want to dip? [a-f or ?*]"), &w),
            ch('b')
        );
        assert_eq!(
            r.on_prompt(
                &getobj("What do you want to dip the dagger into? [f or ?*]"),
                &w
            ),
            ch('f')
        );

        let mut r = MacroRunner::new();
        assert_eq!(
            r.start(Macro::key('o' as i32, Some(3), true)),
            Some(Reply::Key('n' as i32))
        );
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('3'));
        assert_eq!(r.on_prompt(&Prompt::Command, &w), key('o'));
        let dir = Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        };
        assert_eq!(r.on_prompt(&dir, &w), MacroStep::HandToPlayer);
        let mut r = MacroRunner::new();
        assert_eq!(r.start(Macro::ext("pray")), Some(Reply::Key('#' as i32)));
        assert_eq!(
            r.on_prompt(&Prompt::ExtCmd, &w),
            MacroStep::Reply(Reply::ExtCmd(Some("pray".into())))
        );
        // the confirmation is the player's
        assert_eq!(
            r.on_prompt(&yn("Are you sure you want to pray?", "yn"), &w),
            MacroStep::HandToPlayer
        );
        // one key and done
        let mut r = MacroRunner::new();
        r.start(Macro::key('s' as i32, None, false));
        assert!(r.completed());
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::Done);
        assert!(r.completed());
        assert_eq!(
            r.on_prompt(&Prompt::Command, &w),
            MacroStep::HandToPlayer,
            "not running"
        );
    }

    #[test]
    fn getpos_is_not_a_command_prompt() {
        let mut r = MacroRunner::new();
        r.start(Macro::key(';' as i32, None, false));
        let mut w = World::new();
        w.getpos = true;
        assert_eq!(r.on_prompt(&Prompt::Command, &w), MacroStep::HandToPlayer);
    }

    #[test]
    fn needs_a_choice_first() {
        let potion = item('f', '!', &[], "a bubbly potion");
        for k in [Dip, Adjust, Split] {
            assert!(Macro::item(k, &potion, None).is_none(), "{k:?}");
        }
    }
}
