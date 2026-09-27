use nh_protocol::{ESC, MenuItem, PickHow, Reply};

use crate::TextLine;

/// What the UI should do for a pending request (see the plan's reply policy).
#[derive(Debug, Clone, PartialEq)]
pub enum Prompt {
    /// nh_poskey: a command key or a map click.
    Command,
    /// nhgetch: any key.
    Key,
    /// yn_function with choices: only `allowed` characters are answers.
    Choice {
        query: String,
        /// Answers to offer as buttons, in order.
        visible: Vec<char>,
        /// Every character the engine accepts.
        allowed: Vec<char>,
        /// Enter/Space answer, when it is allowed.
        default: Option<char>,
    },
    /// yn_function without choices: getdir (`directions`) or getobj.
    FreeKey {
        query: String,
        directions: bool,
    },
    Menu {
        win: i32,
        how: PickHow,
        title: Option<String>,
        items: Vec<MenuItem>,
    },
    /// getlin, or askname (`name`).
    Text {
        query: String,
        name: bool,
    },
    ExtCmd,
    /// Read-only text: a text window, a display-only menu, a file.
    Show {
        title: Option<String>,
        lines: Vec<TextLine>,
    },
    /// "Look at the map": draw it, wait for any key or click.
    MapPause,
    /// An output pause the client acknowledges at once.
    AutoAck,
    /// message_menu: one line, and with `pick` a letter to choose.
    MessageMenu {
        letter: char,
        mesg: String,
        pick: bool,
    },
}

impl Prompt {
    /// The answer for Esc / closing the dialog.
    pub fn escape_reply(&self) -> Reply {
        match self {
            Prompt::Command | Prompt::Key => Reply::Key(ESC),
            Prompt::Choice { .. } | Prompt::FreeKey { .. } => Reply::Char(ESC),
            Prompt::Menu { .. } => Reply::Cancel,
            Prompt::Text { .. } => Reply::Text("\u{1b}".to_string()),
            Prompt::ExtCmd => Reply::ExtCmd(None),
            Prompt::Show { .. } | Prompt::MapPause | Prompt::AutoAck => Reply::Ack,
            Prompt::MessageMenu { pick: true, .. } => Reply::Char(ESC),
            Prompt::MessageMenu { pick: false, .. } => Reply::Char(0),
        }
    }

    /// May keys typed ahead answer it? (Command, Key, FreeKey)
    pub fn keeps_typeahead(&self) -> bool {
        matches!(self, Prompt::Command | Prompt::Key | Prompt::FreeKey { .. })
    }
}

/// yn_function with a choices string. Allowed: every character but space,
/// ESC and '#'. '#' asks tty to read a count (yn_number); the host never
/// reads one, so the core would take '#' as 'n'. Visible: the part before
/// ESC (the rest is hidden but valid), without spaces and the word " or ".
pub(crate) fn choice(query: &str, choices: &str, default: i32) -> Prompt {
    let esc = char::from(ESC as u8);
    let mut allowed = Vec::new();
    for c in choices
        .chars()
        .filter(|&c| c != ' ' && c != esc && c != '#')
    {
        if !allowed.contains(&c) {
            allowed.push(c);
        }
    }
    let shown = choices.split(esc).next().unwrap_or("").replace(" or ", " ");
    let mut visible = Vec::new();
    for c in shown.chars().filter(|&c| c != ' ' && c != '#') {
        if !visible.contains(&c) {
            visible.push(c);
        }
    }
    let default = u32::try_from(default)
        .ok()
        .and_then(char::from_u32)
        .filter(|c| allowed.contains(c));
    Prompt::Choice {
        query: query.to_string(),
        visible,
        allowed,
        default,
    }
}

/// yn_function without choices: a direction prompt when the query says so.
pub(crate) fn free_key(query: &str) -> Prompt {
    Prompt::FreeKey {
        query: query.to_string(),
        directions: query.to_lowercase().contains("direction"),
    }
}

/// The answer a typed character gives to a Choice with these `allowed`
/// characters: itself, or, as tty lowercases the key unless an answer is a
/// capital, its lower case ('Y' with Caps Lock answers [yn]).
pub fn choice_answer(allowed: &[char], c: char) -> Option<char> {
    if allowed.contains(&c) {
        return Some(c);
    }
    let preserve_case = allowed.iter().any(|a| a.is_ascii_uppercase());
    let lower = c.to_ascii_lowercase();
    (!preserve_case && allowed.contains(&lower)).then_some(lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(p: &Prompt) -> (String, String, Option<char>) {
        let Prompt::Choice {
            visible,
            allowed,
            default,
            ..
        } = p
        else {
            panic!("not a choice: {p:?}");
        };
        (visible.iter().collect(), allowed.iter().collect(), *default)
    }

    #[test]
    fn capitals_answer_lower_case_questions_like_tty() {
        let yn: Vec<char> = "ynq".chars().collect();
        assert_eq!(choice_answer(&yn, 'y'), Some('y'));
        assert_eq!(choice_answer(&yn, 'Y'), Some('y'));
        assert_eq!(choice_answer(&yn, 'x'), None);
        // a capital among the answers: case matters
        let cased: Vec<char> = "yYnN".chars().collect();
        assert_eq!(choice_answer(&cased, 'Y'), Some('Y'));
        let mixed: Vec<char> = "abC".chars().collect();
        assert_eq!(choice_answer(&mixed, 'A'), None);
        assert_eq!(choice_answer(&mixed, 'C'), Some('C'));
    }

    #[test]
    fn choice_parses_hidden_and_count_marks() {
        // loot: a space and " or " are decoration
        let p = choice("Do what with the large box?", ":oibrs nq or ?", 'q' as i32);
        assert_eq!(
            parts(&p),
            (":oibrsnq?".into(), ":oibrsnq?".into(), Some('q'))
        );
        // after ESC: valid but not shown
        let p = choice("Do you want to see your conduct?", "ynq\x1ba", 'n' as i32);
        assert_eq!(parts(&p), ("ynq".into(), "ynqa".into(), Some('n')));
        // '#' lets tty type a count, which no reply can carry: not an answer
        let p = choice("Pick up a dagger?", "yn#aq", 'y' as i32);
        assert_eq!(parts(&p), ("ynaq".into(), "ynaq".into(), Some('y')));
        // a default that is not a choice is no default
        let p = choice("Really attack?", "yn", 0);
        assert_eq!(parts(&p), ("yn".into(), "yn".into(), None));
        let p = choice("Itemized billing?", "ynq m", 'x' as i32);
        assert_eq!(parts(&p), ("ynqm".into(), "ynqm".into(), None));
    }

    #[test]
    fn free_key_directions_by_query() {
        let dir = |q: &str| match free_key(q) {
            Prompt::FreeKey { directions, .. } => directions,
            other => panic!("{other:?}"),
        };
        assert!(dir("In what direction?"));
        assert!(dir("Talk to whom? (in what direction)"));
        assert!(dir("In what DIRECTION do you want to dig?"));
        assert!(!dir("What do you want to wield? [- ab or ?*]"));
        assert!(!dir("What do you want to throw? [$ab or ?*]"));
    }

    #[test]
    fn escape_and_typeahead_follow_the_reply_policy() {
        assert_eq!(Prompt::Command.escape_reply(), Reply::Key(ESC));
        assert_eq!(free_key("x").escape_reply(), Reply::Char(ESC));
        assert_eq!(
            Prompt::Text {
                query: String::new(),
                name: false
            }
            .escape_reply(),
            Reply::Text("\u{1b}".into())
        );
        assert_eq!(Prompt::ExtCmd.escape_reply(), Reply::ExtCmd(None));
        assert_eq!(Prompt::MapPause.escape_reply(), Reply::Ack);
        let mm = |pick| Prompt::MessageMenu {
            letter: 'a',
            mesg: String::new(),
            pick,
        };
        assert_eq!(mm(true).escape_reply(), Reply::Char(ESC));
        assert_eq!(mm(false).escape_reply(), Reply::Char(0));
        assert!(Prompt::Command.keeps_typeahead());
        assert!(Prompt::Key.keeps_typeahead());
        assert!(free_key("x").keeps_typeahead());
        assert!(!Prompt::ExtCmd.keeps_typeahead());
        assert!(!Prompt::MapPause.keeps_typeahead());
        assert!(!choice("q", "yn", 0).keeps_typeahead());
    }
}
