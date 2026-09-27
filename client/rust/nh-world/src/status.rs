use std::collections::{BTreeMap, BTreeSet};

use nh_protocol::{Catalog, StatusUpdate};

/// NetHack's NO_COLOR.
pub const NO_COLOR: i32 = 8;

/// Fields NetHack sends in every round that updates all fields (botl.c
/// evaluate_and_notify_windowport with update_all). A "reset" round is only
/// a full snapshot when it carries all of them: the core also ends rounds
/// with reset when just the display needs a redraw, and then sends only
/// what changed (or nothing).
const ALWAYS_SENT: [&str; 18] = [
    "title",
    "str",
    "dex",
    "con",
    "int",
    "wis",
    "cha",
    "align",
    "cap",
    "gold",
    "energy",
    "energymax",
    "ac",
    "hunger",
    "hp",
    "hpmax",
    "leveldesc",
    "condition",
];

#[derive(Debug, Clone, PartialEq)]
struct Field {
    value: String,
    color: i32,
}

/// The bottom lines, as status_update calls describe them.
#[derive(Debug, Clone, Default)]
pub struct Status {
    fields: BTreeMap<String, Field>,
    /// Fields sent since the last flush or reset.
    group: BTreeSet<String>,
    conditions: u64,
    changed: bool,
}

impl Status {
    pub fn new() -> Status {
        Status::default()
    }

    /// One status_update. "flush" ends a round; "reset" ends a round too, and
    /// a round that carries every field (update_all: start, polymorph, options)
    /// is a full snapshot: fields not sent in it are dropped (a polymorphed
    /// hero has "hitdice" instead of "xlevel"/"exp").
    pub fn apply(&mut self, u: &StatusUpdate) {
        match u.field.as_str() {
            "flush" => self.group.clear(),
            "reset" => {
                if ALWAYS_SENT.iter().all(|f| self.group.contains(*f)) {
                    let before = self.fields.len();
                    let group = std::mem::take(&mut self.group);
                    self.fields.retain(|name, _| group.contains(name));
                    self.changed |= self.fields.len() != before;
                }
                self.group.clear();
            }
            "condition" => {
                let conds = u.conds.unwrap_or(0);
                self.changed |= conds != self.conditions;
                self.conditions = conds;
                self.group.insert(u.field.clone());
            }
            name => {
                let field = Field {
                    value: clean(u.value.as_deref().unwrap_or("")),
                    color: u.color,
                };
                if self.fields.get(name) != Some(&field) {
                    self.fields.insert(name.to_string(), field);
                    self.changed = true;
                }
                self.group.insert(name.to_string());
            }
        }
    }

    /// Value as shown, trimmed, with `\G` glyph escapes removed ("Dlvl:1", ":0" for gold).
    pub fn get(&self, field: &str) -> Option<&str> {
        self.fields.get(field).map(|f| f.value.as_str())
    }

    /// NetHack colour (with attributes in the high bits), NO_COLOR when unknown.
    pub fn color(&self, field: &str) -> i32 {
        self.fields.get(field).map_or(NO_COLOR, |f| f.color)
    }

    /// The number in a field: "16" → 16, "Dlvl:1" → 1, gold ":0" → 0, "Home 2" → 2.
    pub fn number(&self, field: &str) -> Option<i64> {
        self.get(field)?
            .rsplit([':', ' '])
            .find(|s| !s.is_empty())?
            .parse()
            .ok()
    }

    /// BL_CONDITION bits.
    pub fn conditions(&self) -> u64 {
        self.conditions
    }

    /// Names of the conditions in effect, in catalog order ("Blind", "Stone").
    pub fn condition_names(&self, catalog: &Catalog) -> Vec<String> {
        catalog
            .conditions
            .iter()
            .filter(|c| self.conditions & c.mask != 0)
            .map(|c| c.name.clone())
            .collect()
    }

    /// Did anything change since the last call?
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

/// Trim and drop `\G` + 8 hex digits (NetHack's encoded glyph before gold).
fn clean(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find("\\G") {
        let after = &rest[at + 2..];
        let hex = after.len() >= 8 && after.as_bytes()[..8].iter().all(u8::is_ascii_hexdigit);
        if hex {
            out.push_str(&rest[..at]);
            rest = &after[8..];
        } else {
            out.push_str(&rest[..at + 2]);
            rest = after;
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upd(field: &str, value: &str) -> StatusUpdate {
        StatusUpdate {
            field: field.to_string(),
            value: Some(value.to_string()),
            conds: None,
            chg: 0,
            percent: 0,
            color: NO_COLOR,
        }
    }

    fn end(field: &str) -> StatusUpdate {
        StatusUpdate {
            value: None,
            ..upd(field, "")
        }
    }

    fn conds(mask: u64) -> StatusUpdate {
        StatusUpdate {
            conds: Some(mask),
            ..end("condition")
        }
    }

    /// A full round as NetHack sends it at the start, with `xp` either
    /// "xlevel" plus "exp" or, polymorphed, "hitdice".
    fn full_round(s: &mut Status, xp: &[(&str, &str)]) {
        for (f, v) in [
            ("title", "Hero the Stripling            "),
            ("str", "16"),
            ("dex", "15"),
            ("con", "17"),
            ("int", "9"),
            ("wis", "11"),
            ("cha", "7"),
            ("align", "Neutral"),
            ("cap", ""),
            ("gold", "\\G0C9F0F2E:0"),
            ("energy", "2"),
            ("energymax", "2"),
            ("ac", "6"),
            ("time", "1"),
            ("hunger", ""),
            ("hp", "16"),
            ("hpmax", "16"),
            ("leveldesc", "Dlvl:1  "),
        ]
        .iter()
        .chain(xp)
        {
            s.apply(&upd(f, v));
        }
        s.apply(&conds(0));
        s.apply(&end("reset"));
    }

    #[test]
    fn status_strips_padding_and_glyph_escapes() {
        let mut s = Status::new();
        full_round(&mut s, &[("xlevel", "1"), ("exp", "0")]);
        assert_eq!(s.get("title"), Some("Hero the Stripling"));
        assert_eq!(s.get("leveldesc"), Some("Dlvl:1"));
        assert_eq!(s.get("gold"), Some(":0"));
        assert_eq!(s.number("gold"), Some(0));
        assert_eq!(s.number("leveldesc"), Some(1));
        assert_eq!(s.number("hp"), Some(16));
        assert_eq!(s.number("title"), None);
        assert_eq!(s.get("cap"), Some(""));
        assert_eq!(s.color("hp"), NO_COLOR);
        assert_eq!(s.get("nonsense"), None);
        // a backslash that is not a glyph escape stays
        s.apply(&upd("title", "a\\Gb"));
        assert_eq!(s.get("title"), Some("a\\Gb"));
        assert!(s.take_changed());
        assert!(!s.take_changed());
    }

    #[test]
    fn status_reset_drops_missing_fields() {
        let mut s = Status::new();
        full_round(&mut s, &[("xlevel", "1"), ("exp", "0")]);
        // a partial round that ends in reset (only what changed): nothing dropped
        s.apply(&upd("hp", "12"));
        s.apply(&end("reset"));
        assert_eq!(s.get("title"), Some("Hero the Stripling"));
        s.apply(&end("reset"));
        assert_eq!(s.number("hp"), Some(12));
        // polymorph: a full snapshot with hit dice instead of level and experience
        full_round(&mut s, &[("hitdice", "3")]);
        assert_eq!(s.get("hitdice"), Some("3"));
        assert_eq!(s.get("xlevel"), None);
        assert_eq!(s.get("exp"), None);
        assert_eq!(s.number("hp"), Some(16));
        // and back
        full_round(&mut s, &[("xlevel", "1"), ("exp", "0")]);
        assert_eq!(s.get("hitdice"), None);
        assert_eq!(s.get("xlevel"), Some("1"));
        // a flush never drops anything
        s.apply(&upd("hp", "11"));
        s.apply(&end("flush"));
        assert_eq!(s.get("xlevel"), Some("1"));
    }

    #[test]
    fn conditions_are_named_from_the_catalog() {
        let mut s = Status::new();
        s.apply(&conds(0x0010_0002));
        assert_eq!(s.conditions(), 0x0010_0002);
        let line = include_str!("../tests/data/catalog.jsonl");
        let nh_protocol::EngineMsg::Catalog(cat) = nh_protocol::parse_line(line).unwrap() else {
            panic!("not a catalog");
        };
        assert_eq!(s.condition_names(&cat), vec!["Blind", "Stone"]);
    }
}
