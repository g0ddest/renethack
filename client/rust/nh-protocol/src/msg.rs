use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Catalog, Glyph};

/// One decoded line from the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineMsg {
    Hello(Hello),
    Catalog(Box<Catalog>),
    /// A window call that needs no answer.
    Win(WinCall),
    /// A window call that blocks the engine until the client replies.
    Req {
        id: u64,
        req: Request,
    },
    /// The engine gave up on the client (bad reply or EOF); it saves and exits.
    Error {
        msg: String,
    },
    /// Clean process exit follows.
    Bye,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub engine: String,
    pub patchset: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WinCall {
    InitNhwindows,
    CreateNhwindow {
        win: i32,
        kind: WindowKind,
    },
    ClearNhwindow {
        win: i32,
    },
    DisplayNhwindow {
        win: i32,
    },
    DestroyNhwindow {
        win: i32,
    },
    Curs {
        win: i32,
        x: i32,
        y: i32,
    },
    Putstr {
        win: i32,
        attr: i32,
        text: String,
    },
    /// Only sent when the file is missing; present files arrive as a request.
    DisplayFileMissing {
        name: String,
    },
    StartMenu {
        win: i32,
        behavior: u64,
    },
    AddMenu(MenuItem),
    EndMenu {
        win: i32,
        prompt: Option<String>,
    },
    MarkSynch,
    WaitSynch,
    Cliparound {
        x: i32,
        y: i32,
    },
    PrintGlyph {
        win: i32,
        x: i32,
        y: i32,
        g: Glyph,
        bk: Option<Glyph>,
    },
    RawPrint {
        text: String,
        bold: bool,
    },
    Nhbell,
    DoprevMessage,
    NumberPad {
        state: i32,
        /// Direction keys now in effect, NetHack's order "hykulnjb><"
        /// (digits with number_pad on; swap_yz and phone layouts included).
        dirchars: Option<String>,
    },
    DelayOutput,
    PreferenceUpdate {
        pref: String,
    },
    PutMsgHistory {
        msg: Option<String>,
        restoring: bool,
    },
    StatusInit,
    StatusUpdate(StatusUpdate),
    UpdateInventory {
        arg: i32,
    },
    /// The whole inventory, sent before an input wait when it changed.
    Inventory(Inventory),
    /// The hero is on another level (or a game began): the branch and the
    /// depth only, never a special level's name.
    Level(LevelNotice),
    ExitNhwindows {
        text: Option<String>,
    },
    SuspendNhwindows {
        text: Option<String>,
    },
    ResumeNhwindows,
    /// A call this crate does not know yet; kept so newer engines still work.
    Unknown {
        name: String,
        args: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowKind {
    Message,
    Status,
    Map,
    Menu,
    Text,
    Perminvent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MenuItem {
    pub win: i32,
    /// Position in the menu; select_menu replies refer to items by it.
    pub idx: i32,
    pub glyph: Option<Glyph>,
    pub selectable: bool,
    pub ch: i32,
    pub gch: i32,
    pub attr: i32,
    pub clr: i32,
    pub str: Option<String>,
    pub preselected: bool,
    /// MENU_ITEMFLAGS_SKIPINVERT: bulk select and invert never turn it on.
    #[serde(default)]
    pub skipinvert: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StatusUpdate {
    /// "hp", "gold", ..., "condition", "flush" or "reset".
    pub field: String,
    #[serde(default)]
    pub value: Option<String>,
    /// BL_CONDITION bitmask, only for field == "condition".
    #[serde(default)]
    pub conds: Option<u64>,
    #[serde(default)]
    pub chg: i32,
    #[serde(default)]
    pub percent: i32,
    #[serde(default)]
    pub color: i32,
}

/// Where the hero is: what the status line and the overview say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LevelNotice {
    /// The dungeon's name ("The Dungeons of Doom", "The Gnomish Mines"...).
    pub dungeon: String,
    pub depth: i32,
    /// In the endgame: "earth", "air", "fire", "water" or "astral".
    #[serde(default)]
    pub plane: Option<String>,
}

/// The inventory as the character sees it: no true types, no weights.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Inventory {
    /// In inventory order.
    pub items: Vec<InvItem>,
    /// Two-weapon combat: the alternate weapon is wielded in the off hand.
    #[serde(default)]
    pub twoweap: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InvItem {
    pub letter: char,
    /// Object class symbol (`)`, `[`, `%`...).
    pub class: char,
    /// The appearance tile, shared by everything that looks the same.
    pub tile: i32,
    pub quan: i64,
    /// What it is worn or wielded as; empty when merely carried.
    #[serde(default)]
    pub slots: Vec<Slot>,
    /// A lit light source.
    #[serde(default)]
    pub lit: bool,
    /// doname(): "a +1 spear (weapon in right hand)".
    #[serde(default)]
    pub text: String,
}

/// Where an item is worn or wielded (NetHack's W_* masks).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "String")]
pub enum Slot {
    Weapon,
    /// The swap weapon; wielded in the off hand under two-weapon combat.
    Alternate,
    Quiver,
    Body,
    Cloak,
    Helmet,
    Shield,
    Gloves,
    Boots,
    Shirt,
    Amulet,
    LeftRing,
    RightRing,
    /// Blindfold, towel or lenses.
    Eyes,
    Ball,
    Chain,
    /// A slot this crate does not know yet; kept so newer engines still work.
    Other(String),
}

impl From<String> for Slot {
    fn from(s: String) -> Self {
        match s.as_str() {
            "weapon" => Slot::Weapon,
            "alternate" => Slot::Alternate,
            "quiver" => Slot::Quiver,
            "body" => Slot::Body,
            "cloak" => Slot::Cloak,
            "helmet" => Slot::Helmet,
            "shield" => Slot::Shield,
            "gloves" => Slot::Gloves,
            "boots" => Slot::Boots,
            "shirt" => Slot::Shirt,
            "amulet" => Slot::Amulet,
            "left_ring" => Slot::LeftRing,
            "right_ring" => Slot::RightRing,
            "eyes" => Slot::Eyes,
            "ball" => Slot::Ball,
            "chain" => Slot::Chain,
            _ => Slot::Other(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickHow {
    None,
    One,
    Any,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Askname,
    /// Blocking display ("--More--"); answer with `Reply::Ack`.
    DisplayNhwindow {
        win: i32,
    },
    DisplayFile {
        name: String,
        lines: Vec<String>,
    },
    SelectMenu {
        win: i32,
        how: PickHow,
    },
    MessageMenu {
        letter: i32,
        how: i32,
        mesg: Option<String>,
    },
    Nhgetch,
    /// A command key or a map click; `getpos`: inside getpos(), where the
    /// keys move a cursor to pick a spot (absent on the wire otherwise).
    NhPoskey {
        getpos: bool,
    },
    YnFunction {
        query: String,
        choices: Option<String>,
        default: i32,
    },
    Getlin {
        query: String,
    },
    GetExtCmd,
}

impl Request {
    /// The protocol name ("fn") of this request.
    pub fn name(&self) -> &'static str {
        match self {
            Request::Askname => "askname",
            Request::DisplayNhwindow { .. } => "display_nhwindow",
            Request::DisplayFile { .. } => "display_file",
            Request::SelectMenu { .. } => "select_menu",
            Request::MessageMenu { .. } => "message_menu",
            Request::Nhgetch => "nhgetch",
            Request::NhPoskey { .. } => "nh_poskey",
            Request::YnFunction { .. } => "yn_function",
            Request::Getlin { .. } => "getlin",
            Request::GetExtCmd => "get_ext_cmd",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown message type {0:?}")]
    UnknownType(String),
    #[error("{0} message without \"fn\"")]
    MissingFn(&'static str),
    #[error("request without \"id\"")]
    MissingId,
    #[error("unknown request {0:?}")]
    UnknownRequest(String),
    #[error("bad arguments for {name}: {source}")]
    Args {
        name: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Deserialize)]
struct Envelope {
    t: String,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default, rename = "fn")]
    func: Option<String>,
    #[serde(default)]
    a: Value,
}

fn args<T: DeserializeOwned>(name: &str, a: Value) -> Result<T, ProtocolError> {
    serde_json::from_value(a).map_err(|source| ProtocolError::Args {
        name: name.to_string(),
        source,
    })
}

#[derive(Deserialize)]
struct Win {
    win: i32,
}
#[derive(Deserialize)]
struct Create {
    win: i32,
    #[serde(rename = "type")]
    kind: WindowKind,
}
#[derive(Deserialize)]
struct Curs {
    win: i32,
    x: i32,
    y: i32,
}
#[derive(Deserialize)]
struct Putstr {
    win: i32,
    attr: i32,
    str: Option<String>,
}
#[derive(Deserialize)]
struct FileArgs {
    name: Option<String>,
    #[serde(default)]
    lines: Vec<Option<String>>,
}
#[derive(Deserialize)]
struct StartMenu {
    win: i32,
    behavior: u64,
}
#[derive(Deserialize)]
struct EndMenu {
    win: i32,
    prompt: Option<String>,
}
#[derive(Deserialize)]
struct Xy {
    x: i32,
    y: i32,
}
#[derive(Deserialize)]
struct PrintGlyph {
    win: i32,
    x: i32,
    y: i32,
    g: Glyph,
    bk: Option<Glyph>,
}
#[derive(Deserialize)]
struct RawPrint {
    str: Option<String>,
    bold: bool,
}
#[derive(Deserialize)]
struct State {
    state: i32,
    #[serde(default)]
    dirchars: Option<String>,
}
#[derive(Deserialize)]
struct Pref {
    pref: Option<String>,
}
#[derive(Deserialize)]
struct MsgHist {
    msg: Option<String>,
    restoring: bool,
}
#[derive(Deserialize)]
struct Arg {
    arg: i32,
}
#[derive(Deserialize)]
struct Text {
    str: Option<String>,
}
#[derive(Deserialize)]
struct SelectMenu {
    win: i32,
    how: i32,
}
#[derive(Deserialize)]
struct MessageMenu {
    #[serde(rename = "let")]
    letter: i32,
    how: i32,
    mesg: Option<String>,
}
#[derive(Deserialize)]
struct Poskey {
    #[serde(default)]
    getpos: bool,
}
#[derive(Deserialize)]
struct Yn {
    query: Option<String>,
    choices: Option<String>,
    default: i32,
}
#[derive(Deserialize)]
struct Query {
    query: Option<String>,
}
#[derive(Deserialize)]
struct ErrorArgs {
    msg: String,
}

fn win_call(name: String, a: Value) -> Result<WinCall, ProtocolError> {
    let n = name.as_str();
    Ok(match n {
        "init_nhwindows" => WinCall::InitNhwindows,
        "create_nhwindow" => {
            let c: Create = args(n, a)?;
            WinCall::CreateNhwindow {
                win: c.win,
                kind: c.kind,
            }
        }
        "clear_nhwindow" => WinCall::ClearNhwindow {
            win: args::<Win>(n, a)?.win,
        },
        "display_nhwindow" => WinCall::DisplayNhwindow {
            win: args::<Win>(n, a)?.win,
        },
        "destroy_nhwindow" => WinCall::DestroyNhwindow {
            win: args::<Win>(n, a)?.win,
        },
        "curs" => {
            let c: Curs = args(n, a)?;
            WinCall::Curs {
                win: c.win,
                x: c.x,
                y: c.y,
            }
        }
        "putstr" => {
            let p: Putstr = args(n, a)?;
            WinCall::Putstr {
                win: p.win,
                attr: p.attr,
                text: p.str.unwrap_or_default(),
            }
        }
        "display_file" => WinCall::DisplayFileMissing {
            name: args::<FileArgs>(n, a)?.name.unwrap_or_default(),
        },
        "start_menu" => {
            let s: StartMenu = args(n, a)?;
            WinCall::StartMenu {
                win: s.win,
                behavior: s.behavior,
            }
        }
        "add_menu" => WinCall::AddMenu(args(n, a)?),
        "end_menu" => {
            let e: EndMenu = args(n, a)?;
            WinCall::EndMenu {
                win: e.win,
                prompt: e.prompt,
            }
        }
        "mark_synch" => WinCall::MarkSynch,
        "wait_synch" => WinCall::WaitSynch,
        "cliparound" => {
            let c: Xy = args(n, a)?;
            WinCall::Cliparound { x: c.x, y: c.y }
        }
        "print_glyph" => {
            let p: PrintGlyph = args(n, a)?;
            WinCall::PrintGlyph {
                win: p.win,
                x: p.x,
                y: p.y,
                g: p.g,
                bk: p.bk,
            }
        }
        "raw_print" => {
            let r: RawPrint = args(n, a)?;
            WinCall::RawPrint {
                text: r.str.unwrap_or_default(),
                bold: r.bold,
            }
        }
        "nhbell" => WinCall::Nhbell,
        "doprev_message" => WinCall::DoprevMessage,
        "number_pad" => {
            let st: State = args(n, a)?;
            WinCall::NumberPad {
                state: st.state,
                dirchars: st.dirchars,
            }
        }
        "delay_output" => WinCall::DelayOutput,
        "preference_update" => WinCall::PreferenceUpdate {
            pref: args::<Pref>(n, a)?.pref.unwrap_or_default(),
        },
        "putmsghistory" => {
            let m: MsgHist = args(n, a)?;
            WinCall::PutMsgHistory {
                msg: m.msg,
                restoring: m.restoring,
            }
        }
        "status_init" => WinCall::StatusInit,
        "status_update" => WinCall::StatusUpdate(args(n, a)?),
        "update_inventory" => WinCall::UpdateInventory {
            arg: args::<Arg>(n, a)?.arg,
        },
        "inventory" => WinCall::Inventory(args(n, a)?),
        "level" => WinCall::Level(args(n, a)?),
        "exit_nhwindows" => WinCall::ExitNhwindows {
            text: args::<Text>(n, a)?.str,
        },
        "suspend_nhwindows" => WinCall::SuspendNhwindows {
            text: args::<Text>(n, a)?.str,
        },
        "resume_nhwindows" => WinCall::ResumeNhwindows,
        _ => WinCall::Unknown { name, args: a },
    })
}

fn request(name: &str, a: Value) -> Result<Request, ProtocolError> {
    Ok(match name {
        "askname" => Request::Askname,
        "display_nhwindow" => Request::DisplayNhwindow {
            win: args::<Win>(name, a)?.win,
        },
        "display_file" => {
            let f: FileArgs = args(name, a)?;
            Request::DisplayFile {
                name: f.name.unwrap_or_default(),
                lines: f.lines.into_iter().map(Option::unwrap_or_default).collect(),
            }
        }
        "select_menu" => {
            let s: SelectMenu = args(name, a)?;
            let how = match s.how {
                0 => PickHow::None,
                1 => PickHow::One,
                _ => PickHow::Any,
            };
            Request::SelectMenu { win: s.win, how }
        }
        "message_menu" => {
            let m: MessageMenu = args(name, a)?;
            Request::MessageMenu {
                letter: m.letter,
                how: m.how,
                mesg: m.mesg,
            }
        }
        "nhgetch" => Request::Nhgetch,
        "nh_poskey" => {
            let p: Poskey = args(name, a)?;
            Request::NhPoskey { getpos: p.getpos }
        }
        "yn_function" => {
            let y: Yn = args(name, a)?;
            Request::YnFunction {
                query: y.query.unwrap_or_default(),
                choices: y.choices,
                default: y.default,
            }
        }
        "getlin" => Request::Getlin {
            query: args::<Query>(name, a)?.query.unwrap_or_default(),
        },
        "get_ext_cmd" => Request::GetExtCmd,
        other => return Err(ProtocolError::UnknownRequest(other.to_string())),
    })
}

/// Decode one line (without the trailing newline).
pub fn parse_line(line: &str) -> Result<EngineMsg, ProtocolError> {
    let env: Envelope = serde_json::from_str(line)?;
    match env.t.as_str() {
        "hello" => Ok(EngineMsg::Hello(args("hello", env.a)?)),
        "catalog" => Ok(EngineMsg::Catalog(Box::new(args("catalog", env.a)?))),
        "win" => {
            let name = env.func.ok_or(ProtocolError::MissingFn("win"))?;
            Ok(EngineMsg::Win(win_call(name, env.a)?))
        }
        "req" => {
            let id = env.id.ok_or(ProtocolError::MissingId)?;
            let name = env.func.ok_or(ProtocolError::MissingFn("req"))?;
            Ok(EngineMsg::Req {
                id,
                req: request(&name, env.a)?,
            })
        }
        "error" => Ok(EngineMsg::Error {
            msg: args::<ErrorArgs>("error", env.a)?.msg,
        }),
        "bye" => Ok(EngineMsg::Bye),
        other => Err(ProtocolError::UnknownType(other.to_string())),
    }
}
