//! The character sheet: `^X`'s window (insight.c's enlightenment), parsed
//! from its English into what the panel lays out. The hero's name and
//! role come from the title, the rank and level from the background's
//! first line, the hit points, energy, armour class, experience and gold
//! and the six characteristics from their lines; every other line stays a
//! line of its section, said as the engine said it. Nothing is added: the
//! sheet shows what the window says, the end of a game's disclosure ("was",
//! "Final Status:") as much as the game's.
//!
//! No Godot here: the view is in `sheet_view`.

/// A section of the window, by its heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Background,
    Basics,
    Characteristics,
    Status,
    Attributes,
    Miscellaneous,
    /// A heading the sheet does not know: its lines as they are.
    Other,
}

impl Part {
    fn of(heading: &str) -> Part {
        let h = heading.strip_prefix("Final ").unwrap_or(heading);
        match h.strip_suffix(':').unwrap_or(h) {
            "Background" => Part::Background,
            "Basics" => Part::Basics,
            "Characteristics" => Part::Characteristics,
            "Status" => Part::Status,
            "Attributes" => Part::Attributes,
            "Miscellaneous" => Part::Miscellaneous,
            _ => Part::Other,
        }
    }
}

/// A section: its heading as the engine wrote it, and the lines the sheet
/// shows as lines (the window's own, indented by one space, which goes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub part: Part,
    pub heading: String,
    pub lines: Vec<String>,
}

/// One of the six characteristics, in the bottom line's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attr {
    Str,
    Dex,
    Con,
    Int,
    Wis,
    Cha,
}

impl Attr {
    pub const ALL: [Attr; 6] = [
        Attr::Str,
        Attr::Dex,
        Attr::Con,
        Attr::Int,
        Attr::Wis,
        Attr::Cha,
    ];

    fn named(name: &str) -> Option<Attr> {
        Some(match name {
            "strength" => Attr::Str,
            "dexterity" => Attr::Dex,
            "constitution" => Attr::Con,
            "intelligence" => Attr::Int,
            "wisdom" => Attr::Wis,
            "charisma" => Attr::Cha,
            _ => return None,
        })
    }
}

/// A characteristic's value as the engine writes it ("16", "18/01",
/// "18/**"), and what it tells besides: the base the current one differs
/// from, the peak lost points come back to, the limit when it is not a
/// human's (`innate`: the current one is over it by magic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub attr: Attr,
    pub value: String,
    pub base: Option<String>,
    pub peak: Option<String>,
    pub limit: Option<String>,
    pub innate: bool,
}

/// Points of something and its most ("12 out of 16", "all 6").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Points {
    pub now: i64,
    pub most: i64,
}

/// The experience points, and what the next level needs when the window
/// says (debug mode, the end of a game).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Experience {
    pub points: i64,
    pub needed: Option<(i64, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    /// The hero's name and role as the title has them ("Hero", "Wizard").
    pub name: String,
    pub role: String,
    /// The rank and level from the background's first line ("Evoker", 1).
    pub rank: Option<String>,
    pub level: Option<i32>,
    /// Who the hero is: the background's lines of the rank and level, the
    /// alignment and the pantheon (and a form polymorphed into, a gender
    /// or alignment started out with), as the window says them.
    pub identity: Vec<String>,
    pub hit_points: Option<Points>,
    pub energy: Option<Points>,
    pub armor_class: Option<i64>,
    pub experience: Option<Experience>,
    /// The gold in the wallet.
    pub gold: Option<i64>,
    /// The six characteristics, when all six read.
    pub stats: Vec<Stat>,
    /// Every section with what is left of it as lines.
    pub sections: Vec<Section>,
}

#[cfg(test)]
impl Sheet {
    /// The lines of a section (none: no such section, or nothing left).
    pub fn lines(&self, part: Part) -> impl Iterator<Item = &str> {
        self.sections
            .iter()
            .filter(move |s| s.part == part)
            .flat_map(|s| s.lines.iter().map(String::as_str))
    }
}

/// The sentence of a section's line, without the indent.
fn sentence(line: &str) -> &str {
    line.strip_prefix(' ').unwrap_or(line).trim_end()
}

/// The title: "Hero the Wizard's attributes:" (a name may hold " the ").
fn title(line: &str) -> Option<(String, String)> {
    let head = line.trim().strip_suffix("'s attributes:")?;
    let (name, role) = head.rsplit_once(" the ")?;
    (!name.is_empty() && !role.is_empty() && !role.contains(' '))
        .then(|| (name.to_string(), role.to_string()))
}

/// "You are an Evoker, a level 1 female human Wizard." (or "were",
/// "actually", or "level 3 human" when the rank is the role's name): the
/// rank and the level.
fn rank_and_level(s: &str) -> Option<(String, i32)> {
    let rest = s
        .strip_prefix("You are ")
        .or_else(|| s.strip_prefix("You were "))?;
    let rest = rest.strip_prefix("actually ").unwrap_or(rest);
    let (rank, after) = rest
        .split_once(", a level ")
        .or_else(|| rest.split_once(", level "))?;
    let rank = rank
        .strip_prefix("an ")
        .or_else(|| rank.strip_prefix("a "))
        .unwrap_or(rank);
    let level = after.split(' ').next()?.parse().ok()?;
    Some((rank.to_string(), level))
}

/// A line of who the hero is: the alignment and its god ("You are
/// neutral, on a mission for Thoth"), the pantheon that finishes it, a form
/// polymorphed into, the alignment worn or started out with.
fn tells_identity(s: &str) -> bool {
    s.contains("on a mission for ")
        || s.starts_with("who is opposed by ")
        || s.starts_with("who was opposed by ")
        || s.starts_with("You started out ")
        || s.ends_with(" form.")
        || s.starts_with("You are actually ")
        || s.starts_with("You were actually ")
}

/// "You have all 12 hit points.", "You have 5 out of 12 hit points."
fn points(s: &str, what: &str) -> Option<Points> {
    let rest = s
        .strip_prefix("You have ")
        .or_else(|| s.strip_prefix("You had "))?;
    let rest = rest.strip_suffix('.')?;
    let count = rest
        .strip_suffix(what)
        .or_else(|| rest.strip_suffix(what.strip_suffix('s').unwrap_or(what)))?
        .trim_end();
    match count.split(' ').collect::<Vec<_>>().as_slice() {
        ["all", n] => {
            let n = n.parse().ok()?;
            Some(Points { now: n, most: n })
        }
        ["both"] => Some(Points { now: 2, most: 2 }),
        ["no"] => Some(Points { now: 0, most: 0 }),
        [now, "out", "of", most] => Some(Points {
            now: now.parse().ok()?,
            most: most.parse().ok()?,
        }),
        _ => None,
    }
}

/// "Your armor class is 9." ("was", "-40, the best possible").
fn armor_class(s: &str) -> Option<i64> {
    let rest = s
        .strip_prefix("Your armor class is ")
        .or_else(|| s.strip_prefix("Your armor class was "))?;
    let n = rest.strip_suffix('.')?.split(',').next()?;
    n.parse().ok()
}

/// "You have 0 experience points." with what the next level needs when
/// it is said (", 20 needed to attain level 2", ", 5 more needed for
/// level 19", ", 1 more was needed...").
fn experience(s: &str) -> Option<Experience> {
    let rest = s
        .strip_prefix("You have ")
        .or_else(|| s.strip_prefix("You had "))?;
    let rest = rest.strip_suffix('.')?;
    let (points, more) = match rest.split_once(", ") {
        Some((p, m)) => (p, Some(m)),
        None => (rest, None),
    };
    let points = points
        .strip_suffix(" experience points")
        .or_else(|| points.strip_suffix(" experience point"))?
        .parse()
        .ok()?;
    let needed = match more {
        None => None,
        Some(m) => {
            let n = m.split(' ').next()?.parse().ok()?;
            let level = m.rsplit(' ').next()?.parse().ok()?;
            if !m.contains(" level ") {
                return None;
            }
            Some((n, level))
        }
    };
    Some(Experience { points, needed })
}

/// "Your wallet is empty." or "Your wallet contains 12 zorkmids." (a
/// wallet line that goes on to gold stashed away is no simple line).
fn wallet(s: &str) -> Option<i64> {
    if s == "Your wallet is empty." || s == "Your wallet was empty." {
        return Some(0);
    }
    let rest = s
        .strip_prefix("Your wallet contains ")
        .or_else(|| s.strip_prefix("Your wallet contained "))?;
    let rest = rest.strip_suffix('.')?;
    rest.split(' ').next()?.parse().ok()
}

/// "Your dexterity is 13 (current; base:11, limit:18)."
fn stat(s: &str) -> Option<Stat> {
    let rest = s.strip_prefix("Your ")?.strip_suffix('.')?;
    let (name, value) = rest
        .split_once(" is ")
        .or_else(|| rest.split_once(" was "))?;
    let attr = Attr::named(name)?;
    let (value, notes) = match value.split_once(" (") {
        Some((v, n)) => (v, n.strip_suffix(')')?),
        None => (value, ""),
    };
    let mut out = Stat {
        attr,
        value: value.to_string(),
        base: None,
        peak: None,
        limit: None,
        innate: false,
    };
    let notes = notes.strip_prefix("current; ").unwrap_or(notes);
    for note in notes.split(", ").filter(|n| !n.is_empty()) {
        let (key, v) = note.split_once(':')?;
        let v = Some(v.to_string());
        match key {
            "base" => out.base = v,
            "peak" => out.peak = v,
            "limit" => out.limit = v,
            "innate limit" => {
                out.limit = v;
                out.innate = true;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The sheet in a window's lines: None unless it is `^X`'s (or the end of
/// a game's attributes).
pub fn sheet<S: AsRef<str>>(lines: &[S]) -> Option<Sheet> {
    let mut texts = lines.iter().map(AsRef::as_ref);
    let (name, role) = texts
        .by_ref()
        .find(|l| !l.trim().is_empty())
        .and_then(title)?;
    let mut out = Sheet {
        name,
        role,
        rank: None,
        level: None,
        identity: Vec::new(),
        hit_points: None,
        energy: None,
        armor_class: None,
        experience: None,
        gold: None,
        stats: Vec::new(),
        sections: Vec::new(),
    };
    let mut stats = Vec::new();
    let mut stat_lines = Vec::new();
    for line in texts {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            out.sections.push(Section {
                part: Part::of(line.trim()),
                heading: line.trim().to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let s = sentence(line);
        let part = out.sections.last().map(|s| s.part);
        let taken = match part {
            Some(Part::Background) => {
                if out.rank.is_none()
                    && let Some((rank, level)) = rank_and_level(s)
                {
                    out.rank = Some(rank);
                    out.level = Some(level);
                    out.identity.push(s.to_string());
                    true
                } else if tells_identity(s) {
                    out.identity.push(s.to_string());
                    true
                } else if out.experience.is_none()
                    && let Some(xp) = experience(s)
                {
                    out.experience = Some(xp);
                    true
                } else {
                    false
                }
            }
            Some(Part::Basics) => {
                if out.hit_points.is_none()
                    && let Some(p) = points(s, " hit points")
                {
                    out.hit_points = Some(p);
                    true
                } else if out.energy.is_none()
                    && let Some(p) = points(s, " energy points (spell power)")
                {
                    out.energy = Some(p);
                    true
                } else if out.armor_class.is_none()
                    && let Some(ac) = armor_class(s)
                {
                    out.armor_class = Some(ac);
                    true
                } else if out.gold.is_none()
                    && let Some(g) = wallet(s)
                {
                    out.gold = Some(g);
                    true
                } else {
                    false
                }
            }
            Some(Part::Characteristics) => match stat(s) {
                Some(st) => {
                    stats.push(st);
                    stat_lines.push(s.to_string());
                    true
                }
                None => false,
            },
            _ => false,
        };
        if !taken && let Some(sec) = out.sections.last_mut() {
            sec.lines.push(s.to_string());
        }
    }
    // the six, in their order, or the lines as they came
    let all_six = stats.len() == 6 && Attr::ALL.iter().zip(&stats).all(|(a, s)| s.attr == *a);
    if all_six {
        out.stats = stats;
    } else if let Some(sec) = out
        .sections
        .iter_mut()
        .find(|s| s.part == Part::Characteristics)
    {
        let rest = std::mem::take(&mut sec.lines);
        sec.lines = stat_lines.into_iter().chain(rest).collect();
    }
    Some(out)
}

/// A Wizard's `^X` at the start of a game, as the engine wrote it (the
/// dialogs' warm-up builds its sheet behind the title).
pub const WIZARD: &[&str] = &[
    "Hero the Wizard's attributes:",
    "",
    "Background:",
    " You are an Evoker, a level 1 female human Wizard.",
    " You are neutral, on a mission for Thoth",
    " who is opposed by Ptah (lawful) and Anhur (chaotic).",
    " You are left-handed.",
    " You are in the Dungeons of Doom, on level 1.",
    " You have just started your adventure.",
    " It is the midnight hour.",
    " There is a new moon in effect.",
    " You have 0 experience points.",
    "",
    "Basics:",
    " You have all 12 hit points.",
    " You have all 6 energy points (spell power).",
    " Your armor class is 9.",
    " Your wallet is empty.",
    " Autopickup is off.",
    "",
    "Characteristics:",
    " Your strength is 9.",
    " Your dexterity is 11.",
    " Your constitution is 18.",
    " Your intelligence is 14.",
    " Your wisdom is 12.",
    " Your charisma is 11.",
    "",
    "Status:",
    " You aren't hungry.",
    " You are unencumbered.",
    " You are wielding a quarterstaff.",
    " You have basic skill with quarterstaff.",
    "",
    "Miscellaneous:",
    " Total elapsed playing time is none.",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A Valkyrie's in debug mode: the attributes, the next level's need.
    const VALKYRIE: &[&str] = &[
        "Wizard the Valkyrie's attributes:",
        "",
        "Background:",
        " You are a Stripling, a level 1 human Valkyrie.",
        " You are neutral, on a mission for Odin",
        " who is opposed by Tyr (lawful) and Loki (chaotic).",
        " You are right-handed.",
        " You are in the Dungeons of Doom, on level 1.",
        " You have just started your adventure.",
        " It is the midnight hour.",
        " There is a new moon in effect.",
        " You have 0 experience points, 20 needed to attain level 2.",
        "",
        "Basics:",
        " You have all 16 hit points.",
        " You have both energy points (spell power).",
        " Your armor class is 6.",
        " Your wallet is empty.",
        " Autopickup is off.",
        "",
        "Characteristics:",
        " Your strength is 18/01 (current; peak:18/50, limit:18/**).",
        " Your dexterity is 13 (current; base:11).",
        " Your constitution is 17.",
        " Your intelligence is 9.",
        " Your wisdom is 11.",
        " Your charisma is 9 (current; base:10, innate limit:8).",
        "",
        "Status:",
        " You aren't hungry <900>.",
        " You are unencumbered <-785>.",
        " You are wielding a spear.",
        " You have basic skill with spear.",
        "",
        "Attributes:",
        " You are nominally aligned.",
        " Your alignment is 0.",
        " You are cold resistant innately.",
        " Your luck is zero.",
        " You can't safely pray (300).",
        "",
        "Miscellaneous:",
        " You are running in debug mode.",
        " You haven't encountered any bones levels.",
        " Total elapsed playing time is none.",
    ];

    #[test]
    fn the_wizard_s_sheet_reads_whole() {
        let s = sheet(WIZARD).expect("a sheet");
        assert_eq!((s.name.as_str(), s.role.as_str()), ("Hero", "Wizard"));
        assert_eq!((s.rank.as_deref(), s.level), (Some("Evoker"), Some(1)));
        assert_eq!(
            s.identity,
            [
                "You are an Evoker, a level 1 female human Wizard.",
                "You are neutral, on a mission for Thoth",
                "who is opposed by Ptah (lawful) and Anhur (chaotic).",
            ]
        );
        assert_eq!(s.hit_points, Some(Points { now: 12, most: 12 }));
        assert_eq!(s.energy, Some(Points { now: 6, most: 6 }));
        assert_eq!((s.armor_class, s.gold), (Some(9), Some(0)));
        assert_eq!(
            s.experience,
            Some(Experience {
                points: 0,
                needed: None
            })
        );
        let values: Vec<&str> = s.stats.iter().map(|s| s.value.as_str()).collect();
        assert_eq!(values, ["9", "11", "18", "14", "12", "11"]);
        assert_eq!(
            s.lines(Part::Background).collect::<Vec<_>>(),
            [
                "You are left-handed.",
                "You are in the Dungeons of Doom, on level 1.",
                "You have just started your adventure.",
                "It is the midnight hour.",
                "There is a new moon in effect.",
            ]
        );
        assert_eq!(
            s.lines(Part::Basics).collect::<Vec<_>>(),
            ["Autopickup is off."]
        );
        assert_eq!(s.lines(Part::Characteristics).count(), 0);
        assert_eq!(s.lines(Part::Status).count(), 4);
        assert_eq!(
            s.lines(Part::Miscellaneous).collect::<Vec<_>>(),
            ["Total elapsed playing time is none."]
        );
    }

    #[test]
    fn the_notes_of_a_characteristic_and_the_next_level_s_need_read() {
        let s = sheet(VALKYRIE).expect("a sheet");
        assert_eq!((s.rank.as_deref(), s.level), (Some("Stripling"), Some(1)));
        assert_eq!(s.energy, Some(Points { now: 2, most: 2 }));
        assert_eq!(
            s.experience,
            Some(Experience {
                points: 0,
                needed: Some((20, 2))
            })
        );
        let st = &s.stats[0];
        assert_eq!(
            (st.value.as_str(), st.base.as_deref(), st.peak.as_deref()),
            ("18/01", None, Some("18/50"))
        );
        assert_eq!((st.limit.as_deref(), st.innate), (Some("18/**"), false));
        assert_eq!(s.stats[1].base.as_deref(), Some("11"));
        let ch = &s.stats[5];
        assert_eq!((ch.limit.as_deref(), ch.innate), (Some("8"), true));
        assert_eq!(s.lines(Part::Attributes).count(), 5);
        assert_eq!(s.lines(Part::Miscellaneous).count(), 3);
    }

    #[test]
    fn every_line_of_the_window_is_on_the_sheet() {
        for window in [WIZARD, VALKYRIE] {
            let s = sheet(window).unwrap();
            // read into a value (the experience; the hit points, energy,
            // armour class and gold; the six), or a line of its section
            let parsed = s.identity.len() + 1 + 4 + s.stats.len();
            let kept: usize = s.sections.iter().map(|s| s.lines.len()).sum();
            let headings = s.sections.len();
            let shown = window.iter().filter(|l| !l.trim().is_empty()).count();
            // the title, the headings, the values, the lines
            assert_eq!(1 + headings + parsed + kept, shown, "{window:?}");
        }
    }

    #[test]
    fn the_end_of_a_game_s_words_read_too() {
        let end = [
            "Hero the Valkyrie's attributes:",
            "",
            "Background:",
            " You were a Stripling, a level 3 human Valkyrie.",
            " You were neutral, on a mission for Odin",
            " who was opposed by Tyr (lawful) and Loki (chaotic).",
            " You had 40 experience points, 40 more were needed to attain level 4.",
            "",
            "Basics:",
            " You had 0 out of 30 hit points.",
            " You had no energy points (spell power).",
            " Your armor class was -40, the best possible.",
            " Your wallet contained 125 zorkmids.",
            "",
            "Final Characteristics:",
            " Your strength was 16 (base:16, peak:18, limit:18/**).",
        ];
        let s = sheet(&end).expect("a sheet");
        assert_eq!(s.level, Some(3));
        assert_eq!(s.hit_points, Some(Points { now: 0, most: 30 }));
        assert_eq!(s.energy, Some(Points { now: 0, most: 0 }));
        assert_eq!((s.armor_class, s.gold), (Some(-40), Some(125)));
        assert_eq!(s.experience.and_then(|x| x.needed), Some((40, 4)));
        // one characteristic of six: a line
        assert!(s.stats.is_empty());
        assert_eq!(
            s.lines(Part::Characteristics).collect::<Vec<_>>(),
            ["Your strength was 16 (base:16, peak:18, limit:18/**)."]
        );
    }

    #[test]
    fn another_window_is_no_sheet() {
        assert_eq!(
            sheet(&["Voluntary challenges:", " You have been an atheist."]),
            None
        );
        assert_eq!(sheet::<&str>(&[]), None);
        // a polymorphed hero's form and the wallet that goes on stay lines
        let s = sheet(&[
            "Hero the Wizard's attributes:",
            "Background:",
            " You are currently in newt form.",
            " You are actually an Evoker, a level 1 female human Wizard.",
            "Basics:",
            " You have 1 hit die.",
            " Your wallet contains 3 zorkmids, and",
            " you have 50 more zorkmids stashed away in your pack.",
        ])
        .unwrap();
        assert_eq!(s.identity.len(), 2);
        assert_eq!(s.rank.as_deref(), Some("Evoker"));
        assert_eq!(s.gold, None);
        assert_eq!(s.lines(Part::Basics).count(), 3);
    }
}
