//! The journal: #chronicle's window (insight.c's show_gamelog) parsed from
//! its English. "Logged events:" (at a game's end "Major events:"), the
//! column's " Turn", then each event's turn and words ("    1: Hero the
//! neutral female human Wizard entered the dungeon"), or " none".
//!
//! No Godot here: the view is in `journal_view`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub turn: i64,
    /// The event's words as the engine logged them.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journal {
    /// The end of a game's major events, rather than the events logged.
    pub major: bool,
    pub events: Vec<Event>,
}

/// "   12: entered the Gnomish Mines".
fn event(line: &str) -> Option<Event> {
    let (turn, text) = line.trim_start().split_once(": ")?;
    Some(Event {
        turn: turn.parse().ok()?,
        text: text.to_string(),
    })
}

/// The journal in a window's lines: None unless it is #chronicle's.
pub fn journal<S: AsRef<str>>(lines: &[S]) -> Option<Journal> {
    let mut texts = lines
        .iter()
        .map(AsRef::as_ref)
        .filter(|l| !l.trim().is_empty());
    let major = match texts.next()?.trim() {
        "Logged events:" => false,
        "Major events:" => true,
        _ => return None,
    };
    let mut events = Vec::new();
    for line in texts {
        match line.trim() {
            "Turn" | "none" => {}
            _ => events.push(event(line)?),
        }
    }
    Some(Journal { major, events })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_events_read() {
        let j = journal(&[
            "Logged events:",
            " Turn",
            "    1: Hero the neutral female human Wizard entered the dungeon",
            "  712: entered the Gnomish Mines",
            "12345: reached experience level 10",
        ])
        .expect("a journal");
        assert!(!j.major);
        assert_eq!(j.events.len(), 3);
        assert_eq!(j.events[1].turn, 712);
        assert_eq!(j.events[1].text, "entered the Gnomish Mines");
        assert_eq!(j.events[2].turn, 12345);
    }

    #[test]
    fn the_end_s_major_events_and_none_read() {
        let j = journal(&["Major events:", " none"]).expect("a journal");
        assert!(j.major && j.events.is_empty());
        assert_eq!(
            journal(&["Voluntary challenges:", " You were an atheist."]),
            None
        );
        // a line that is no event: no journal
        assert_eq!(journal(&["Logged events:", " Turn", " what?"]), None);
    }
}
