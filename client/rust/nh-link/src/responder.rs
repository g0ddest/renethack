use std::collections::VecDeque;

use nh_protocol::{ESC, PickHow, Reply, Request};
use serde_json::Value;

use crate::{LinkError, Recording, Responder};

/// One scripted answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Key(i32),
    Click(i32, i32),
    Ext(Option<String>),
    Yn(i32),
    Text(String),
    Menu(Vec<(u32, i64)>),
    Cancel,
    Ack,
    /// Close the engine's input (simulates the client dying).
    Hangup,
}

/// Parse a script: one step per line, `#` comments.
///
/// ```text
/// key #          keystroke; also: key ESC | ENTER | SPACE | <decimal code>
/// click 10 5     mouse click on map cell (10,5)
/// ext quit       extended command by name; `ext -` cancels
/// yn y           answer a yn_function / message_menu prompt
/// text Elbereth  answer getlin / askname; `text ESC` cancels
/// menu 0 3:2     select items 0 and 3 (count 2); `menu -` selects nothing
/// cancel         cancel a menu
/// ack            acknowledge a blocking display
/// hangup         close the engine's input
/// ```
pub fn parse_script(text: &str) -> Result<Vec<Step>, LinkError> {
    let mut steps = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        // "# note" is a comment; a bare "#" line is too ("key #" sends '#')
        if line.is_empty() || line == "#" || line.starts_with("# ") {
            continue;
        }
        let bad = |why: &str| LinkError::Script(format!("line {}: {why}: {line}", n + 1));
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let step = match word {
            "key" => Step::Key(key_code(rest).ok_or_else(|| bad("bad key"))?),
            "click" => {
                let mut it = rest.split_whitespace().map(str::parse::<i32>);
                match (it.next(), it.next()) {
                    (Some(Ok(x)), Some(Ok(y))) => Step::Click(x, y),
                    _ => return Err(bad("click needs x y")),
                }
            }
            "ext" if rest == "-" => Step::Ext(None),
            "ext" if !rest.is_empty() => Step::Ext(Some(rest.to_string())),
            "yn" => Step::Yn(key_code(rest).ok_or_else(|| bad("bad answer"))?),
            "text" if rest == "ESC" => Step::Text("\u{1b}".to_string()),
            "text" => Step::Text(rest.to_string()),
            "menu" if rest == "-" => Step::Menu(Vec::new()),
            "menu" => {
                let mut items = Vec::new();
                for tok in rest.split_whitespace() {
                    let (i, c) = tok.split_once(':').unwrap_or((tok, "-1"));
                    items.push((
                        i.parse().map_err(|_| bad("bad item index"))?,
                        c.parse().map_err(|_| bad("bad count"))?,
                    ));
                }
                if items.is_empty() {
                    return Err(bad("menu needs items or -"));
                }
                Step::Menu(items)
            }
            "cancel" => Step::Cancel,
            "ack" => Step::Ack,
            "hangup" => Step::Hangup,
            _ => return Err(bad("unknown step")),
        };
        steps.push(step);
    }
    Ok(steps)
}

fn key_code(s: &str) -> Option<i32> {
    match s {
        "ESC" => Some(ESC),
        "ENTER" => Some(10),
        "SPACE" => Some(32),
        _ if s.chars().count() == 1 => s.chars().next().map(|c| c as i32),
        _ => s.parse().ok(),
    }
}

/// Answers requests from a script. Blocking displays and display-only menus
/// are acknowledged automatically, so scripts only list real decisions.
pub struct ScriptResponder {
    steps: VecDeque<Step>,
}

impl ScriptResponder {
    pub fn new(steps: Vec<Step>) -> ScriptResponder {
        ScriptResponder {
            steps: steps.into(),
        }
    }

    pub fn remaining(&self) -> usize {
        self.steps.len()
    }
}

impl Responder for ScriptResponder {
    fn respond(&mut self, _id: u64, req: &Request) -> Result<Option<Value>, LinkError> {
        match req {
            Request::DisplayNhwindow { .. }
            | Request::DisplayFile { .. }
            | Request::SelectMenu {
                how: PickHow::None, ..
            } => {
                return Ok(Some(Reply::Ack.to_value()));
            }
            _ => {}
        }
        let step = self
            .steps
            .pop_front()
            .ok_or_else(|| LinkError::Script(format!("script exhausted; engine asks {req:?}")))?;
        let reply = match (req, step) {
            (_, Step::Hangup) => return Ok(None),
            (Request::Nhgetch | Request::NhPoskey, Step::Key(k)) => Reply::Key(k),
            (Request::NhPoskey, Step::Click(x, y)) => Reply::Click { x, y, modifier: 1 },
            (Request::GetExtCmd, Step::Ext(c)) => Reply::ExtCmd(c),
            (Request::YnFunction { .. } | Request::MessageMenu { .. }, Step::Yn(c)) => {
                Reply::Char(c)
            }
            (Request::Getlin { .. } | Request::Askname, Step::Text(t)) => Reply::Text(t),
            (Request::SelectMenu { .. }, Step::Menu(items)) => Reply::Menu(items),
            (Request::SelectMenu { .. }, Step::Cancel) => Reply::Cancel,
            (req, step) => {
                return Err(LinkError::Script(format!(
                    "step {step:?} cannot answer {req:?}"
                )));
            }
        };
        Ok(Some(reply.to_value()))
    }
}

/// Answers requests with the replies of a recording, in order, and fails
/// as soon as the engine asks something the recording did not.
pub struct ReplayResponder {
    replies: VecDeque<crate::RecordedReply>,
    index: usize,
}

impl ReplayResponder {
    pub fn new(recording: &Recording) -> ReplayResponder {
        ReplayResponder {
            replies: recording.replies.clone().into(),
            index: 0,
        }
    }
}

impl Responder for ReplayResponder {
    fn respond(&mut self, _id: u64, req: &Request) -> Result<Option<Value>, LinkError> {
        let next = self.replies.pop_front();
        self.index += 1;
        match next {
            // a null reply is a recorded hangup: close the input again
            Some(rec) if rec.func == req.name() => Ok((!rec.r.is_null()).then_some(rec.r)),
            Some(rec) => Err(LinkError::Divergence {
                index: self.index,
                expected: rec.func,
                actual: req.name().to_string(),
            }),
            None => Err(LinkError::Divergence {
                index: self.index,
                expected: "end of recording".to_string(),
                actual: req.name().to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_step_kind() {
        let steps = parse_script(
            "# a comment\n\nkey #\nkey ESC\nkey 104\nclick 10 5\next quit\next -\n\
             yn y\ntext Elbereth here\ntext ESC\nmenu 0 3:2\nmenu -\ncancel\nack\nhangup\n",
        )
        .unwrap();
        assert_eq!(
            steps,
            vec![
                Step::Key('#' as i32),
                Step::Key(27),
                Step::Key(104),
                Step::Click(10, 5),
                Step::Ext(Some("quit".into())),
                Step::Ext(None),
                Step::Yn('y' as i32),
                Step::Text("Elbereth here".into()),
                Step::Text("\u{1b}".into()),
                Step::Menu(vec![(0, -1), (3, 2)]),
                Step::Menu(vec![]),
                Step::Cancel,
                Step::Ack,
                Step::Hangup,
            ]
        );
    }

    #[test]
    fn rejects_bad_lines_with_their_number() {
        let err = parse_script("key #\nfly away\n").unwrap_err().to_string();
        assert!(err.contains("line 2"), "{err}");
        assert!(parse_script("click 1\n").is_err());
        assert!(parse_script("menu x\n").is_err());
        assert!(parse_script("key\n").is_err());
    }

    #[test]
    fn auto_acks_displays_and_checks_step_kinds() {
        let mut r = ScriptResponder::new(vec![Step::Yn('y' as i32)]);
        let ack = r.respond(1, &Request::DisplayNhwindow { win: 1 }).unwrap();
        assert_eq!(ack, Some(Reply::Ack.to_value()));
        assert_eq!(r.remaining(), 1);
        let err = r.respond(2, &Request::GetExtCmd).unwrap_err();
        assert!(matches!(err, LinkError::Script(_)));
    }
}
