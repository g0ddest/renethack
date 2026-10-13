//! The skills' panel: #enhance's window (weapon.c) parsed from its
//! English. "Pick a skill to advance:" lists the skills that can advance
//! now as picks, "Current skills:" none; each skill is under its group
//! ("Weapon Skills"), with its level and, when some can advance, a mark:
//! "*" it may later, "#" it cannot any further. In debug mode a level is
//! followed by its practice, and the title by the slots free. The
//! window says no skill's maximum but for "#": the panel shows no more.
//!
//! No Godot here: the view is in `skills_view`.

/// A group of skills, by its heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Fighting,
    Weapon,
    Spellcasting,
    /// A heading the panel does not know: as the window says it.
    Other,
}

impl Group {
    pub fn of(heading: &str) -> Group {
        match heading.trim() {
            "Fighting Skills" => Group::Fighting,
            "Weapon Skills" => Group::Weapon,
            "Spellcasting Skills" => Group::Spellcasting,
            _ => Group::Other,
        }
    }
}

/// A skill's level, as weapon.c names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Restricted,
    Unskilled,
    Basic,
    Skilled,
    Expert,
    Master,
    GrandMaster,
}

impl Level {
    fn named(name: &str) -> Option<Level> {
        Some(match name {
            "Restricted" => Level::Restricted,
            "Unskilled" => Level::Unskilled,
            "Basic" => Level::Basic,
            "Skilled" => Level::Skilled,
            "Expert" => Level::Expert,
            "Master" => Level::Master,
            "Grand Master" => Level::GrandMaster,
            _ => return None,
        })
    }

    /// Steps above Unskilled: the pips the panel fills.
    pub fn steps(self) -> usize {
        match self {
            Level::Restricted | Level::Unskilled => 0,
            Level::Basic => 1,
            Level::Skilled => 2,
            Level::Expert => 3,
            Level::Master => 4,
            Level::GrandMaster => 5,
        }
    }
}

/// What the window marks a skill with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// It can advance now: a pick.
    Advance,
    /// "*": it may be enhanced later (more experience, or slots).
    Later,
    /// "#": it cannot be enhanced any further.
    Peaked,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Its English name ("long sword", "martial arts").
    pub name: String,
    pub level: Level,
    pub mark: Mark,
    /// The menu item's index when it can advance now.
    pub idx: Option<i32>,
    /// Debug mode's practice and what the next level needs.
    pub practice: Option<(i64, i64)>,
}

/// A line of the window: a group's heading, a skill, or what else it says
/// (the legend of "*" and "#").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Heading { group: Group, english: String },
    Skill(Skill),
    Note(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skills {
    /// Skills can advance now: their rows are picks.
    pub advance: bool,
    /// Debug mode's slots free.
    pub slots: Option<i64>,
    pub rows: Vec<Row>,
}

/// The title: "Pick a skill to advance:" or "Current skills:", and debug
/// mode's "  (3 slots available)".
fn title(t: &str) -> Option<(bool, Option<i64>)> {
    let t = t.trim();
    let (head, slots) = match t.split_once("  (") {
        Some((h, rest)) => {
            let n = rest.split(' ').next()?.parse().ok()?;
            (h, Some(n))
        }
        None => (t, None),
    };
    match head {
        "Pick a skill to advance:" => Some((true, slots)),
        "Current skills:" => Some((false, slots)),
        _ => None,
    }
}

/// A skill's line: " %s %-*s [%s]" ("  long sword    [Basic]", "   *  dagger
/// [Basic]"), or debug mode's " %s%-*s %-12s %5d(%4d)".
pub fn skill(line: &str, pick: Option<i32>) -> Option<Skill> {
    let text = line.trim();
    let (mark, text) = if let Some(t) = text.strip_prefix("* ") {
        (Mark::Later, t.trim_start())
    } else if let Some(t) = text.strip_prefix("# ") {
        (Mark::Peaked, t.trim_start())
    } else {
        (
            if pick.is_some() {
                Mark::Advance
            } else {
                Mark::None
            },
            text,
        )
    };
    // "[Basic]" at the end; debug mode's level then its practice
    let (rest, level, practice) = if let Some(open) = text.rfind(" [") {
        let level = Level::named(text[open + 2..].strip_suffix(']')?)?;
        (text[..open].trim_end(), level, None)
    } else {
        let open = text.rfind('(')?;
        let need: i64 = text[open + 1..].strip_suffix(')')?.trim().parse().ok()?;
        let before = text[..open].trim_end();
        let (before, done) = before.rsplit_once(' ')?;
        let done: i64 = done.parse().ok()?;
        let before = before.trim_end();
        let (rest, level) = match before.strip_suffix("Grand Master") {
            Some(r) => (r, Level::GrandMaster),
            None => {
                let (r, l) = before.rsplit_once(' ')?;
                (r, Level::named(l)?)
            }
        };
        (rest.trim_end(), level, Some((done, need)))
    };
    let name = rest.trim();
    (!name.is_empty()).then(|| Skill {
        name: name.to_string(),
        level,
        mark,
        idx: pick,
        practice,
    })
}

/// The skills in #enhance's window: its title and lines, each line with
/// its menu item's index when it is a pick (None: no skills' window).
/// `heading` tells a group's heading (the window's bold or inverse).
pub fn skills(title_text: Option<&str>, lines: &[(String, bool, Option<i32>)]) -> Option<Skills> {
    let (advance, slots) = title(title_text?)?;
    let mut rows = Vec::new();
    let mut in_group = false;
    for (text, heading, pick) in lines {
        if text.trim().is_empty() {
            continue;
        }
        if *heading {
            in_group = true;
            rows.push(Row::Heading {
                group: Group::of(text),
                english: text.trim().to_string(),
            });
            continue;
        }
        match skill(text, *pick) {
            Some(s) if in_group => rows.push(Row::Skill(s)),
            _ => rows.push(Row::Note(text.trim().to_string())),
        }
    }
    rows.iter()
        .any(|r| matches!(r, Row::Skill(_)))
        .then_some(Skills {
            advance,
            slots,
            rows,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(rows: &[(&str, bool, Option<i32>)]) -> Vec<(String, bool, Option<i32>)> {
        rows.iter()
            .map(|(t, h, p)| (t.to_string(), *h, *p))
            .collect()
    }

    fn names(s: &Skills) -> Vec<(&str, Level, Mark)> {
        s.rows
            .iter()
            .filter_map(|r| match r {
                Row::Skill(k) => Some((k.name.as_str(), k.level, k.mark)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_samurai_s_current_skills_read() {
        let w = lines(&[
            ("Fighting Skills", true, None),
            ("  martial arts      [Basic]", false, None),
            ("  riding            [Unskilled]", false, None),
            ("Weapon Skills", true, None),
            ("  long sword        [Basic]", false, None),
            ("  two-handed sword  [Unskilled]", false, None),
            ("Spellcasting Skills", true, None),
            ("  clerical spells   [Unskilled]", false, None),
        ]);
        let s = skills(Some("Current skills:"), &w).expect("skills");
        assert!(!s.advance);
        assert_eq!(
            names(&s),
            [
                ("martial arts", Level::Basic, Mark::None),
                ("riding", Level::Unskilled, Mark::None),
                ("long sword", Level::Basic, Mark::None),
                ("two-handed sword", Level::Unskilled, Mark::None),
                ("clerical spells", Level::Unskilled, Mark::None),
            ]
        );
        assert!(matches!(
            s.rows[0],
            Row::Heading {
                group: Group::Fighting,
                ..
            }
        ));
    }

    #[test]
    fn the_picks_the_marks_and_the_legend_read() {
        let w = lines(&[
            (
                "(Skill flagged by \"*\" may be enhanced when you're more experienced.)",
                false,
                None,
            ),
            (
                "(Skill flagged by \"#\" cannot be enhanced any further.)",
                false,
                None,
            ),
            ("", false, None),
            ("Weapon Skills", true, None),
            ("  long sword         [Skilled]", false, Some(3)),
            ("   *  dagger              [Basic]", false, None),
            ("   #  axe                 [Expert]", false, None),
            ("      club                [Unskilled]", false, None),
        ]);
        let s = skills(Some("Pick a skill to advance:"), &w).expect("skills");
        assert!(s.advance);
        assert_eq!(
            names(&s),
            [
                ("long sword", Level::Skilled, Mark::Advance),
                ("dagger", Level::Basic, Mark::Later),
                ("axe", Level::Expert, Mark::Peaked),
                ("club", Level::Unskilled, Mark::None),
            ]
        );
        assert!(matches!(&s.rows[0], Row::Note(n) if n.starts_with("(Skill flagged")));
        assert!(matches!(&s.rows[3], Row::Skill(k) if k.idx == Some(3)));
    }

    #[test]
    fn debug_mode_s_practice_and_slots_read() {
        let w = lines(&[
            ("Fighting Skills", true, None),
            (
                " bare handed combat Grand Master    0(  20)",
                false,
                Some(1),
            ),
            ("Weapon Skills", true, None),
            (
                " dagger             Basic           20(  80)",
                false,
                Some(2),
            ),
        ]);
        let s = skills(Some("Current skills:  (3 slots available)"), &w).expect("skills");
        assert_eq!(s.slots, Some(3));
        let all: Vec<&Skill> = s
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Skill(k) => Some(k),
                _ => None,
            })
            .collect();
        assert_eq!(all[0].level, Level::GrandMaster);
        assert_eq!(all[0].name, "bare handed combat");
        assert_eq!(all[1].practice, Some((20, 80)));
        assert_eq!(all[1].level, Level::Basic);
    }

    #[test]
    fn another_window_is_no_skills() {
        let w = lines(&[
            ("Weapon Skills", true, None),
            ("  long sword [Basic]", false, None),
        ]);
        assert_eq!(skills(Some("Pick up what?"), &w), None);
        assert_eq!(skills(None, &w), None);
        assert_eq!(skills(Some("Current skills:"), &lines(&[])), None);
    }
}
