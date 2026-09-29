//! What the hero uses, told from what the client sent (spec decision 8):
//! the command key, the letter answered to the engine's getobj() question
//! and the direction answered to getdir(). Once the turn resolves (the next
//! command prompt) the use is complete and the client shows it: the item's
//! appearance tile is taken when its letter is answered, so a potion drunk
//! to the last drop is still known. Nothing here knows more than the
//! player: the tile is the appearance, the letter the player's own answer.

use nh_protocol::{ESC, Reply, Slot};

use crate::{Prompt, World, parse_item_question};

/// A use the client shows with an animation and an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UseKind {
    Quaff,
    Read,
    Zap,
    /// `Z`: a spell (no item).
    Cast,
    Eat,
    Apply,
    Throw,
    /// `f`: what the quiver holds.
    Fire,
    Wear,
    PutOn,
    TakeOff,
    Remove,
    Wield,
    /// `,` (no item: what lies here).
    PickUp,
    /// `k` with number_pad, `^D` either way (no item).
    Kick,
}

impl UseKind {
    /// The use a command key starts.
    pub fn of_key(key: i32, number_pad: bool) -> Option<UseKind> {
        use UseKind::*;
        if key == 4 {
            return Some(Kick);
        }
        let c = char::from(u8::try_from(key).ok()?);
        Some(match c {
            'q' => Quaff,
            'r' => Read,
            'z' => Zap,
            'Z' => Cast,
            'e' => Eat,
            'a' => Apply,
            't' => Throw,
            'f' => Fire,
            'W' => Wear,
            'P' => PutOn,
            'T' => TakeOff,
            'R' => Remove,
            'w' => Wield,
            ',' => PickUp,
            'k' if number_pad => Kick,
            _ => return None,
        })
    }

    /// The use an extended command starts (`#quaff`).
    pub fn of_ext(name: &str) -> Option<UseKind> {
        use UseKind::*;
        Some(match name {
            "quaff" => Quaff,
            "read" => Read,
            "zap" => Zap,
            "cast" => Cast,
            "eat" => Eat,
            "apply" => Apply,
            "throw" => Throw,
            "fire" => Fire,
            "wear" => Wear,
            "puton" => PutOn,
            "takeoff" => TakeOff,
            "remove" => Remove,
            "wield" => Wield,
            "pickup" => PickUp,
            "kick" => Kick,
            _ => return None,
        })
    }

    /// Needs an item's letter to be a use (the others need none).
    pub fn needs_item(self) -> bool {
        !matches!(self, UseKind::Cast | UseKind::PickUp | UseKind::Kick)
    }
}

/// A use of the hero's, complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemUse {
    pub kind: UseKind,
    /// The letter answered ('-': bare hands, fingers; None: no item).
    pub letter: Option<char>,
    /// The item's appearance tile and class when the letter was answered.
    pub tile: Option<i32>,
    pub class: Option<char>,
    /// The direction answered on the map ((0, 0): at oneself).
    pub dir: Option<(i32, i32)>,
    /// Up or down ('<', '>') instead of a direction on the map.
    pub vertical: Option<char>,
}

impl ItemUse {
    fn new(kind: UseKind) -> ItemUse {
        ItemUse {
            kind,
            letter: None,
            tile: None,
            class: None,
            dir: None,
            vertical: None,
        }
    }
}

/// The steps of NetHack's `xdir`/`ydir`, in the order of `dirchars`.
const STEPS: [(i32, i32); 8] = [
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
];

#[derive(Debug, Clone)]
struct Armed {
    u: ItemUse,
    /// A count typed into getobj: the letter follows as a key.
    counting: bool,
    /// `Z`: a spell was picked from the menu.
    chosen: bool,
}

impl Armed {
    fn ready(&self) -> bool {
        match self.u.kind {
            UseKind::Cast => self.chosen,
            UseKind::Kick => self.u.dir.is_some(),
            UseKind::PickUp => true,
            _ => self.u.letter.is_some(),
        }
    }
}

/// Fed with every reply the client sends and every prompt it gets.
#[derive(Debug, Clone, Default)]
pub struct UseTracker {
    armed: Option<Armed>,
}

impl UseTracker {
    pub fn new() -> UseTracker {
        UseTracker::default()
    }

    /// Forget a use under way (a new game).
    pub fn cancel(&mut self) {
        self.armed = None;
    }

    /// The client answered `prompt` with `reply`.
    pub fn on_reply(&mut self, prompt: &Prompt, reply: &Reply, world: &World) {
        if matches!(reply, Reply::Cancel)
            || matches!(reply, Reply::Key(ESC) | Reply::Char(ESC))
            || matches!(reply, Reply::ExtCmd(None))
        {
            self.armed = None;
            return;
        }
        match (prompt, reply) {
            (Prompt::Command | Prompt::Key, Reply::Key(k)) if !world.getpos => {
                self.on_key(*k, world, *prompt == Prompt::Command)
            }
            (Prompt::ExtCmd, Reply::ExtCmd(Some(name))) => {
                self.armed = UseKind::of_ext(name).map(|kind| arm(kind, world));
            }
            (Prompt::FreeKey { query, directions }, Reply::Char(c)) => {
                let Some(a) = self.armed.as_mut() else {
                    return;
                };
                let Some(c) = u32::try_from(*c).ok().and_then(char::from_u32) else {
                    return;
                };
                if *directions {
                    direction(&mut a.u, c, world);
                } else if parse_item_question(query).is_some() {
                    if c.is_ascii_digit() {
                        a.counting = true;
                    } else {
                        pick(&mut a.u, c, world);
                    }
                }
            }
            (Prompt::Menu { .. }, Reply::Menu(items)) => {
                if let Some(a) = self.armed.as_mut() {
                    a.chosen |= !items.is_empty();
                }
            }
            _ => {}
        }
    }

    fn on_key(&mut self, k: i32, world: &World, command: bool) {
        let c = u32::try_from(k).ok().and_then(char::from_u32);
        if let Some(a) = self.armed.as_mut().filter(|a| a.counting) {
            match c {
                Some(c) if c.is_ascii_digit() => {}
                Some(c) => {
                    a.counting = false;
                    pick(&mut a.u, c, world);
                }
                None => self.armed = None,
            }
            return;
        }
        if !command {
            return;
        }
        // a count before the command: `n20`, or digits without number_pad
        let count = match c {
            Some('n') => world.number_pad,
            Some(d) => d.is_ascii_digit() && !world.number_pad,
            None => false,
        };
        if count {
            return;
        }
        self.armed = UseKind::of_key(k, world.number_pad).map(|kind| arm(kind, world));
    }

    /// A prompt came: at the next command prompt the turn has resolved,
    /// and the use under way, if complete, is returned.
    pub fn on_prompt(&mut self, prompt: &Prompt, world: &World) -> Option<ItemUse> {
        if *prompt != Prompt::Command || world.getpos {
            return None;
        }
        let a = self.armed.take()?;
        if a.counting {
            self.armed = Some(a);
            return None;
        }
        a.ready().then_some(a.u)
    }
}

fn arm(kind: UseKind, world: &World) -> Armed {
    let mut u = ItemUse::new(kind);
    // `f` with something in the quiver asks only for the direction
    if kind == UseKind::Fire
        && let Some(q) = world.inventory.in_slot(&Slot::Quiver)
    {
        pick(&mut u, q.letter, world);
    }
    Armed {
        u,
        counting: false,
        chosen: false,
    }
}

fn pick(u: &mut ItemUse, c: char, world: &World) {
    if !(c.is_ascii_alphabetic() || c == '-' || c == '$' || c == '#') {
        return;
    }
    u.letter = Some(c);
    let item = world.inventory.by_letter(c);
    u.tile = item.map(|i| i.tile);
    u.class = item.map(|i| i.class);
}

fn direction(u: &mut ItemUse, c: char, world: &World) {
    match c {
        '<' | '>' => u.vertical = Some(c),
        '.' | 's' => u.dir = Some((0, 0)),
        _ => match world.dirchars.chars().position(|d| d == c) {
            Some(i) if i < STEPS.len() => u.dir = Some(STEPS[i]),
            Some(8) => u.vertical = Some('<'),
            Some(9) => u.vertical = Some('>'),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use nh_protocol::{InvItem, Inventory};

    use super::*;

    fn item(letter: char, class: char, tile: i32, slots: Vec<Slot>) -> InvItem {
        InvItem {
            letter,
            class,
            tile,
            quan: 1,
            slots,
            lit: false,
            text: String::new(),
        }
    }

    fn world() -> World {
        let mut w = World::new();
        w.inventory.replace(&Inventory {
            items: vec![
                item('a', ')', 10, vec![Slot::Weapon]),
                item('b', ')', 11, vec![Slot::Quiver]),
                item('f', '!', 200, vec![]),
                item('g', '?', 300, vec![]),
                item('h', '/', 400, vec![]),
                item('i', '(', 500, vec![]),
            ],
            twoweap: false,
        });
        w
    }

    fn getobj(verb: &str) -> Prompt {
        Prompt::FreeKey {
            query: format!("What do you want to {verb}? [fgh or ?*]"),
            directions: false,
        }
    }

    fn getdir() -> Prompt {
        Prompt::FreeKey {
            query: "In what direction?".into(),
            directions: true,
        }
    }

    /// Feed (prompt, reply) pairs; what the next command prompt yields.
    fn run(w: &World, steps: &[(Prompt, Reply)]) -> Option<ItemUse> {
        let mut t = UseTracker::new();
        for (p, r) in steps {
            assert_eq!(t.on_prompt(p, w), None, "{p:?} is not the end");
            t.on_reply(p, r, w);
        }
        t.on_prompt(&Prompt::Command, w)
    }

    fn key(c: char) -> Reply {
        Reply::Key(c as i32)
    }

    fn ch(c: char) -> Reply {
        Reply::Char(c as i32)
    }

    #[test]
    fn quaffing_is_told_by_the_letter_answered() {
        let w = world();
        let u = run(
            &w,
            &[(Prompt::Command, key('q')), (getobj("drink"), ch('f'))],
        )
        .unwrap();
        assert_eq!(u.kind, UseKind::Quaff);
        assert_eq!(
            (u.letter, u.tile, u.class),
            (Some('f'), Some(200), Some('!'))
        );
        assert_eq!(u.dir, None);
    }

    #[test]
    fn the_tile_is_taken_before_the_item_is_used_up() {
        let mut w = world();
        let mut t = UseTracker::new();
        t.on_reply(&Prompt::Command, &key('q'), &w);
        t.on_reply(&getobj("drink"), &ch('f'), &w);
        // the potion is gone from the next inventory notice
        w.inventory.replace(&Inventory {
            items: vec![item('a', ')', 10, vec![Slot::Weapon])],
            twoweap: false,
        });
        let u = t.on_prompt(&Prompt::Command, &w).unwrap();
        assert_eq!(u.tile, Some(200));
    }

    #[test]
    fn zapping_and_throwing_take_a_direction() {
        let w = world();
        let u = run(
            &w,
            &[
                (Prompt::Command, key('z')),
                (getobj("zap"), ch('h')),
                (getdir(), ch('l')),
            ],
        )
        .unwrap();
        assert_eq!(
            (u.kind, u.tile, u.dir),
            (UseKind::Zap, Some(400), Some((1, 0)))
        );
        let u = run(
            &w,
            &[
                (Prompt::Command, key('t')),
                (getobj("throw"), ch('a')),
                (getdir(), ch('y')),
            ],
        )
        .unwrap();
        assert_eq!((u.kind, u.dir), (UseKind::Throw, Some((-1, -1))));
        let u = run(
            &w,
            &[
                (Prompt::Command, key('z')),
                (getobj("zap"), ch('h')),
                (getdir(), ch('>')),
            ],
        )
        .unwrap();
        assert_eq!((u.dir, u.vertical), (None, Some('>')));
    }

    #[test]
    fn number_pad_directions_and_kicks() {
        let mut w = world();
        w.number_pad = true;
        w.dirchars = "47896321><".into();
        let u = run(&w, &[(Prompt::Command, key('k')), (getdir(), ch('2'))]).unwrap();
        assert_eq!(
            (u.kind, u.dir, u.letter),
            (UseKind::Kick, Some((0, 1)), None)
        );
        // without number_pad `k` is a step north, ^D kicks
        let w = world();
        assert_eq!(run(&w, &[(Prompt::Command, key('k'))]), None);
        let u = run(&w, &[(Prompt::Command, Reply::Key(4)), (getdir(), ch('j'))]).unwrap();
        assert_eq!(u.dir, Some((0, 1)));
    }

    #[test]
    fn firing_uses_the_quiver() {
        let w = world();
        let u = run(&w, &[(Prompt::Command, key('f')), (getdir(), ch('h'))]).unwrap();
        assert_eq!(
            (u.kind, u.letter, u.tile),
            (UseKind::Fire, Some('b'), Some(11))
        );
    }

    #[test]
    fn a_cancelled_or_refused_use_shows_nothing() {
        let w = world();
        // Esc at the item question
        let esc = Reply::Char(ESC);
        assert_eq!(
            run(
                &w,
                &[(Prompt::Command, key('q')), (getobj("drink"), esc.clone())]
            ),
            None
        );
        // Esc at the direction
        assert_eq!(
            run(
                &w,
                &[
                    (Prompt::Command, key('z')),
                    (getobj("zap"), ch('h')),
                    (getdir(), esc)
                ]
            ),
            None
        );
        // "You don't have anything to drink.": no question at all
        assert_eq!(run(&w, &[(Prompt::Command, key('q'))]), None);
        // another command: nothing
        assert_eq!(run(&w, &[(Prompt::Command, key('s'))]), None);
    }

    #[test]
    fn questions_in_between_do_not_end_it() {
        let w = world();
        let yn = Prompt::Choice {
            query: "There is a tin here; eat it?".into(),
            visible: vec!['y', 'n', 'q'],
            allowed: vec!['y', 'n', 'q'],
            default: Some('n'),
        };
        let u = run(
            &w,
            &[
                (Prompt::Command, key('e')),
                (yn, ch('n')),
                (getobj("eat"), ch('f')),
                (Prompt::AutoAck, Reply::Ack),
            ],
        )
        .unwrap();
        assert_eq!((u.kind, u.letter), (UseKind::Eat, Some('f')));
    }

    #[test]
    fn a_count_before_the_command_or_the_letter() {
        let w = world();
        let u = run(
            &w,
            &[
                (Prompt::Command, key('2')),
                (Prompt::Command, key('t')),
                (getobj("throw"), ch('2')),
                (Prompt::Command, key('b')),
                (getdir(), ch('l')),
            ],
        );
        let u = u.unwrap();
        assert_eq!(
            (u.kind, u.letter, u.dir),
            (UseKind::Throw, Some('b'), Some((1, 0)))
        );
    }

    #[test]
    fn casting_needs_a_spell_picked() {
        let w = world();
        let menu = Prompt::Menu {
            win: 5,
            how: nh_protocol::PickHow::One,
            title: None,
            items: Vec::new(),
        };
        let u = run(
            &w,
            &[
                (Prompt::Command, key('Z')),
                (menu.clone(), Reply::Menu(vec![(1, -1)])),
                (getdir(), ch('l')),
            ],
        )
        .unwrap();
        assert_eq!((u.kind, u.dir), (UseKind::Cast, Some((1, 0))));
        assert_eq!(
            run(&w, &[(Prompt::Command, key('Z')), (menu, Reply::Cancel)]),
            None
        );
    }

    #[test]
    fn extended_commands_and_simple_uses() {
        let w = world();
        let u = run(
            &w,
            &[
                (Prompt::Command, key('#')),
                (Prompt::ExtCmd, Reply::ExtCmd(Some("apply".into()))),
                (getobj("use or apply"), ch('i')),
            ],
        )
        .unwrap();
        assert_eq!((u.kind, u.tile), (UseKind::Apply, Some(500)));
        let u = run(&w, &[(Prompt::Command, key(','))]).unwrap();
        assert_eq!(u.kind, UseKind::PickUp);
        let u = run(
            &w,
            &[(Prompt::Command, key('w')), (getobj("wield"), ch('-'))],
        )
        .unwrap();
        assert_eq!(
            (u.kind, u.letter, u.tile),
            (UseKind::Wield, Some('-'), None)
        );
    }

    #[test]
    fn getpos_prompts_are_not_the_end_of_a_turn() {
        let mut w = world();
        let mut t = UseTracker::new();
        t.on_reply(&Prompt::Command, &key('r'), &w);
        t.on_reply(&getobj("read"), &ch('g'), &w);
        w.getpos = true;
        assert_eq!(t.on_prompt(&Prompt::Command, &w), None);
        w.getpos = false;
        assert_eq!(
            t.on_prompt(&Prompt::Command, &w).map(|u| u.kind),
            Some(UseKind::Read)
        );
    }
}
