use serde_json::{Value, json};

/// An answer to a `Request`, i.e. the `"r"` object of a reply line.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// nhgetch / nh_poskey keystroke (ASCII code; 27 = ESC).
    Key(i32),
    /// nh_poskey mouse click on a map cell; `modifier` is CLICK_1 (1) or CLICK_2 (2).
    Click { x: i32, y: i32, modifier: i32 },
    /// yn_function / message_menu answer.
    Char(i32),
    /// getlin / askname answer; "\u{1b}" cancels.
    Text(String),
    /// get_ext_cmd answer; None cancels.
    ExtCmd(Option<String>),
    /// select_menu answer: (item index, count), count -1 = "all / no count".
    Menu(Vec<(u32, i64)>),
    /// select_menu cancelled (ESC).
    Cancel,
    /// Acknowledge a blocking display or a display-only menu.
    Ack,
}

pub const ESC: i32 = 27;

impl Reply {
    pub fn to_value(&self) -> Value {
        match self {
            Reply::Key(k) => json!({ "key": k }),
            Reply::Click { x, y, modifier } => json!({ "x": x, "y": y, "mod": modifier }),
            Reply::Char(c) => json!({ "ch": c }),
            Reply::Text(t) => json!({ "text": t }),
            Reply::ExtCmd(c) => json!({ "cmd": c }),
            Reply::Menu(items) => json!({ "items": items }),
            Reply::Cancel => json!({ "cancel": true }),
            Reply::Ack => json!({}),
        }
    }
}

/// Serialize a reply line (without the trailing newline).
pub fn encode_reply(id: u64, r: &Value) -> String {
    json!({ "id": id, "r": r }).to_string()
}
