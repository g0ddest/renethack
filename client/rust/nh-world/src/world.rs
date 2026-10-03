use std::collections::BTreeMap;

use nh_protocol::{
    Catalog, LevelNotice, MenuItem, PickHow, ProgressNotice, Request, WinCall, WindowKind,
};

use crate::prompt::{choice, free_key};
use crate::{ATR_NOHISTORY, MapState, MessageLog, Pack, Prompt, Status, effect_cmaps};

/// getpos() says this (with `flags.verbose`) right after its goal ("Where
/// do you want to travel to?"): neither belongs in the history.
const GETPOS_TIP: &str = "(For instructions type a ";
/// getpos() repeats its goal so after its first-time tip window.
const GETPOS_GOAL_AGAIN: &str = "Move cursor to ";

/// Direction keys while the engine has not said otherwise (number_pad off).
pub const DEFAULT_DIRCHARS: &str = "hykulnjb><";

/// One engine window. Ids are reused, so the kind is what counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub kind: WindowKind,
    /// putstr lines (text windows, and menu windows used for text).
    pub lines: Vec<TextLine>,
    /// add_menu items since the last start_menu.
    pub menu: Vec<MenuItem>,
    /// end_menu prompt: the menu's title.
    pub menu_prompt: Option<String>,
}

impl Window {
    fn new(kind: WindowKind) -> Window {
        Window {
            kind,
            lines: Vec::new(),
            menu: Vec::new(),
            menu_prompt: None,
        }
    }

    /// Everything the window would show as text: its lines, then its items.
    fn text(&self) -> Vec<TextLine> {
        let items = self.menu.iter().map(|i| TextLine {
            attr: i.attr,
            text: i.str.clone().unwrap_or_default(),
        });
        self.lines.iter().cloned().chain(items).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub attr: i32,
    pub text: String,
}

/// Everything the client knows about the game, built from window calls.
#[derive(Debug, Clone)]
pub struct World {
    pub map: MapState,
    pub status: Status,
    /// The last inventory notice.
    pub inventory: Pack,
    /// The last level notice: where the hero is.
    pub level: Option<LevelNotice>,
    /// The last progress notice (see `progress`), and how many came.
    progress: Option<ProgressNotice>,
    progress_seq: u64,
    pub log: MessageLog,
    pub windows: BTreeMap<i32, Window>,
    pub message_win: Option<i32>,
    pub map_win: Option<i32>,
    /// curs() on the map window.
    pub cursor: Option<(i32, i32)>,
    /// cliparound().
    pub view_center: Option<(i32, i32)>,
    pub number_pad: bool,
    /// "hykulnjb><" until number_pad says otherwise.
    pub dirchars: String,
    /// Latest ATR_NOHISTORY message (autodescribe, "Unknown command").
    pub transient: Option<String>,
    /// raw_print (the top ten list at the end).
    pub raw_lines: Vec<String>,
    /// exit_nhwindows text ("Be seeing you...").
    pub exit_text: Option<String>,
    /// Last text window shown. Cleared when the message window goes (the
    /// end of the game), so from then on only the summary or tombstone.
    pub last_text: Vec<TextLine>,
    pub windows_exited: bool,
    /// The pending nh_poskey moves getpos()'s cursor (farlook, travel...).
    pub getpos: bool,
    /// What that getpos is for ("Where do you want to travel to?").
    pub getpos_goal: Option<String>,
    /// doprev_message (^P) asked for the message history; the game takes it.
    pub wants_history: bool,
    /// Where the cursor was at the last command prompt: the core puts it on
    /// the hero (flush_screen(1)), who may not be drawn (invisible, hiding).
    hero_at_command: Option<(i32, i32)>,
    input_seq: u64,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

impl World {
    pub fn new() -> World {
        World {
            map: MapState::new(),
            status: Status::new(),
            inventory: Pack::new(),
            level: None,
            progress: None,
            progress_seq: 0,
            log: MessageLog::new(),
            windows: BTreeMap::new(),
            message_win: None,
            map_win: None,
            cursor: None,
            view_center: None,
            number_pad: false,
            dirchars: DEFAULT_DIRCHARS.to_string(),
            transient: None,
            raw_lines: Vec::new(),
            exit_text: None,
            last_text: Vec::new(),
            windows_exited: false,
            getpos: false,
            getpos_goal: None,
            wants_history: false,
            hero_at_command: None,
            input_seq: 0,
        }
    }

    /// What the world needs from the game's catalog: which map symbols are brief
    /// effects rather than terrain. Call it once the catalog is known.
    pub fn set_catalog(&mut self, catalog: &Catalog) {
        self.map.set_effect_cmaps(effect_cmaps(catalog));
    }

    fn kind(&self, win: i32) -> Option<WindowKind> {
        self.windows.get(&win).map(|w| w.kind)
    }

    /// The game turn, as the status line shows it (the `time` option is
    /// always on).
    pub fn turn(&self) -> Option<i64> {
        self.status.number("time")
    }

    pub fn apply(&mut self, call: &WinCall) {
        match call {
            WinCall::CreateNhwindow { win, kind } => {
                self.windows.insert(*win, Window::new(*kind));
                match kind {
                    WindowKind::Message => self.message_win = Some(*win),
                    WindowKind::Map => self.map_win = Some(*win),
                    _ => {
                        if self.message_win == Some(*win) {
                            self.message_win = None;
                        }
                        if self.map_win == Some(*win) {
                            self.map_win = None;
                        }
                    }
                }
            }
            WinCall::DestroyNhwindow { win } => {
                if let Some(w) = self.windows.remove(win)
                    && w.kind == WindowKind::Text
                    && !w.lines.is_empty()
                {
                    self.last_text = w.lines;
                }
                if self.message_win == Some(*win) {
                    // the game is over: a text window seen during play is
                    // not its summary (answering q skips the summary)
                    self.message_win = None;
                    self.last_text.clear();
                }
                if self.map_win == Some(*win) {
                    self.map_win = None;
                }
            }
            WinCall::ClearNhwindow { win } => match self.kind(*win) {
                Some(WindowKind::Map) => {
                    self.map.clear();
                    self.hero_at_command = None;
                }
                // tty erases the top line: the log stays, a transient goes
                Some(WindowKind::Message) => self.transient = None,
                Some(_) => {
                    let w = self.windows.get_mut(win).expect("kind came from it");
                    w.lines.clear();
                    w.menu.clear();
                }
                None => {}
            },
            WinCall::Putstr { win, attr, text } => match self.kind(*win) {
                Some(WindowKind::Message) => {
                    if attr & ATR_NOHISTORY != 0 {
                        self.transient = Some(text.clone());
                    } else if self.getpos_goal.is_some() && text.starts_with(GETPOS_GOAL_AGAIN) {
                        // the prompt line says it already
                    } else if text.starts_with(GETPOS_TIP) {
                        // the goal said just before: the prompt line shows it
                        let seq = self.log.last_seq();
                        let goal =
                            self.log.iter().next_back().filter(|m| {
                                m.seq == seq && m.seq > self.input_seq && !m.from_history
                            });
                        self.getpos_goal = goal.map(|m| m.text.clone());
                        if self.getpos_goal.is_some() {
                            self.log.retract_newest();
                        }
                        self.transient = None;
                    } else {
                        // a new message takes tty's top line
                        self.transient = None;
                        let turn = self.turn();
                        self.log.push(text.clone(), *attr, turn, false);
                    }
                }
                Some(WindowKind::Text | WindowKind::Menu) => {
                    let w = self.windows.get_mut(win).expect("kind came from it");
                    w.lines.push(TextLine {
                        attr: *attr,
                        text: text.clone(),
                    });
                }
                _ => {}
            },
            WinCall::DisplayNhwindow { win } => {
                if let Some(w) = self.windows.get(win)
                    && w.kind == WindowKind::Text
                    && !w.lines.is_empty()
                {
                    self.last_text = w.lines.clone();
                }
            }
            WinCall::StartMenu { win, .. } => {
                if let Some(w) = self.windows.get_mut(win) {
                    w.menu.clear();
                    w.lines.clear();
                    w.menu_prompt = None;
                }
            }
            WinCall::AddMenu(item) => {
                if let Some(w) = self.windows.get_mut(&item.win) {
                    w.menu.push(item.clone());
                }
            }
            WinCall::EndMenu { win, prompt } => {
                if let Some(w) = self.windows.get_mut(win) {
                    w.menu_prompt = prompt.clone();
                }
            }
            WinCall::PrintGlyph { win, x, y, g, bk } => {
                if Some(*win) == self.map_win {
                    self.map.print(*x, *y, g, bk.as_ref());
                }
            }
            WinCall::Curs { win, x, y } => {
                if Some(*win) == self.map_win {
                    self.cursor = Some((*x, *y));
                }
            }
            WinCall::Cliparound { x, y } => self.view_center = Some((*x, *y)),
            WinCall::NumberPad { state, dirchars } => {
                self.number_pad = *state != 0;
                if let Some(d) = dirchars {
                    self.dirchars = d.clone();
                }
            }
            // restoring: history from the save; otherwise said now (count echo, getobj)
            WinCall::PutMsgHistory {
                msg: Some(msg),
                restoring,
            } => {
                let turn = if *restoring { None } else { self.turn() };
                self.log.push(msg.clone(), 0, turn, *restoring);
            }
            WinCall::RawPrint { text, .. } => self.raw_lines.push(text.clone()),
            WinCall::ExitNhwindows { text } => {
                self.windows_exited = true;
                self.exit_text = text.clone();
            }
            WinCall::DoprevMessage => self.wants_history = true,
            WinCall::StatusInit => self.status = Status::new(),
            WinCall::StatusUpdate(u) => self.status.apply(u),
            WinCall::Inventory(inv) => self.inventory.replace(inv),
            WinCall::Level(l) => self.level = Some(l.clone()),
            WinCall::Progress(p) => {
                self.progress = Some(p.clone());
                self.progress_seq += 1;
            }
            _ => {}
        }
    }

    /// The UI's view of a request; records what showing it implies (last_text,
    /// message_menu text into the log).
    pub fn on_request(&mut self, req: &Request) -> Prompt {
        match req {
            Request::NhPoskey { getpos } => {
                self.getpos = *getpos;
                if !getpos {
                    self.getpos_goal = None;
                    self.hero_at_command = self.cursor;
                }
                Prompt::Command
            }
            Request::Nhgetch => Prompt::Key,
            Request::YnFunction {
                query,
                choices: Some(choices),
                default,
            } if !choices.is_empty() => choice(query, choices, *default),
            Request::YnFunction { query, .. } => free_key(query),
            Request::SelectMenu { win, how } => {
                let w = self.windows.get(win);
                let title = w.and_then(|w| w.menu_prompt.clone());
                match how {
                    PickHow::None => Prompt::Show {
                        title,
                        lines: w.map(Window::text).unwrap_or_default(),
                    },
                    _ => Prompt::Menu {
                        win: *win,
                        how: *how,
                        title,
                        items: w.map(|w| w.menu.clone()).unwrap_or_default(),
                    },
                }
            }
            Request::Askname => Prompt::Text {
                query: "Who are you?".to_string(),
                name: true,
            },
            Request::Getlin { query } => Prompt::Text {
                query: query.clone(),
                name: false,
            },
            Request::GetExtCmd => Prompt::ExtCmd,
            Request::DisplayNhwindow { win } => match self.windows.get(win) {
                // an empty text or menu window has nothing to show
                Some(w)
                    if matches!(w.kind, WindowKind::Text | WindowKind::Menu)
                        && w.lines.is_empty()
                        && w.menu.is_empty() =>
                {
                    Prompt::AutoAck
                }
                Some(w) if w.kind == WindowKind::Text => {
                    if !w.lines.is_empty() {
                        self.last_text = w.lines.clone();
                    }
                    Prompt::Show {
                        title: None,
                        lines: w.lines.clone(),
                    }
                }
                Some(w) if w.kind == WindowKind::Menu => Prompt::Show {
                    title: w.menu_prompt.clone(),
                    lines: w.text(),
                },
                Some(w) if w.kind == WindowKind::Map => Prompt::MapPause,
                _ => Prompt::AutoAck,
            },
            Request::DisplayFile { name, lines } => Prompt::Show {
                title: Some(name.clone()),
                lines: lines
                    .iter()
                    .map(|l| TextLine {
                        attr: 0,
                        text: l.clone(),
                    })
                    .collect(),
            },
            Request::MessageMenu { letter, how, mesg } => {
                let mesg = mesg.clone().unwrap_or_default();
                if !mesg.is_empty() {
                    let turn = self.turn();
                    self.log.push(mesg.clone(), 0, turn, false);
                }
                Prompt::MessageMenu {
                    letter: u32::try_from(*letter)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap_or('\0'),
                    mesg,
                    pick: *how == 1,
                }
            }
        }
    }

    /// Called by the game whenever the player answers Command/Key/FreeKey/Choice.
    /// A transient message described the moment before the key: it goes
    /// (getpos describes the new cursor spot again).
    pub fn note_player_input(&mut self) {
        self.input_seq = self.log.last_seq();
        self.transient = None;
    }

    /// Where the hero is: the cell drawn with MG_HERO, else (an invisible
    /// or hiding hero is not drawn) where the core put the cursor for the
    /// last command.
    pub fn hero(&self) -> Option<(i32, i32)> {
        self.map.hero().or(self.hero_at_command)
    }

    /// What the prompt line says while getpos waits.
    pub fn getpos_line(&self) -> Option<String> {
        self.getpos.then(|| {
            let goal = self.getpos_goal.as_deref().unwrap_or("Pick a spot.");
            format!("{goal}  (. , ; : pick, ? help, Esc cancel)")
        })
    }

    /// Log seq at the last player input: newer messages are "new".
    /// The hero's progress as last told (achievements are made of it):
    /// None before the game's first input wait.
    pub fn progress(&self) -> Option<&ProgressNotice> {
        self.progress.as_ref()
    }

    /// How many progress notices came: a new one moves it on (what is
    /// worked out from the progress, once per notice).
    pub fn progress_seq(&self) -> u64 {
        self.progress_seq
    }

    pub fn input_seq(&self) -> u64 {
        self.input_seq
    }
}

#[cfg(test)]
mod tests {
    use nh_protocol::{EngineMsg, parse_line};

    use super::*;
    use crate::{Terrain, cell_terrain, describe_cell};

    /// Apply engine lines; the prompt for the last request, if any.
    fn feed(world: &mut World, lines: &str) -> Option<Prompt> {
        let mut prompt = None;
        for line in lines.lines().map(str::trim).filter(|l| !l.is_empty()) {
            match parse_line(line).unwrap_or_else(|e| panic!("{e}: {line}")) {
                EngineMsg::Win(call) => world.apply(&call),
                EngineMsg::Req { req, .. } => prompt = Some(world.on_request(&req)),
                other => panic!("unexpected {other:?}"),
            }
        }
        prompt
    }

    const START: &str = r#"
        {"t":"win","fn":"create_nhwindow","a":{"win":1,"type":"message"}}
        {"t":"win","fn":"create_nhwindow","a":{"win":2,"type":"map"}}
        {"t":"win","fn":"create_nhwindow","a":{"win":3,"type":"menu"}}
        {"t":"win","fn":"status_update","a":{"field":"time","value":"7","chg":0,"percent":0,"color":8}}
    "#;

    fn texts(lines: &[TextLine]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn the_last_progress_notice_is_the_hero_s_progress() {
        let mut w = World::new();
        assert!(w.progress().is_none());
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"progress","a":{"mode":"debug","role":"Wiz","achieved":[],"events":{},"deepest":1,"conduct":{},"roleplay":{},"gameover":false,"how":null}}
            {"t":"win","fn":"progress","a":{"mode":"debug","role":"Wiz","achieved":[15],"events":{"qcalled":true},"deepest":3,"conduct":{"pets":1},"roleplay":{},"gameover":false,"how":null}}
            "#,
        );
        let p = w.progress().expect("progress");
        assert_eq!(w.progress_seq(), 2);
        assert_eq!(p.mode, "debug");
        assert!(!p.earns());
        assert!(p.achieved(15));
        assert!(p.event("qcalled"));
        assert_eq!(p.deepest, 3);
    }

    #[test]
    fn window_ids_are_reused_by_kind() {
        let mut w = World::new();
        feed(&mut w, START);
        assert_eq!((w.message_win, w.map_win), (Some(1), Some(2)));
        let p = feed(
            &mut w,
            r#"
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Really quit?"}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":3}}
            {"t":"req","id":10,"fn":"display_nhwindow","a":{"win":1}}
            "#,
        );
        assert_eq!(p, Some(Prompt::AutoAck));
        let p = feed(
            &mut w,
            r#"
            {"t":"win","fn":"destroy_nhwindow","a":{"win":2}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":1}}
            {"t":"win","fn":"create_nhwindow","a":{"win":1,"type":"text"}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Farvel Hero the Valkyrie..."}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":""}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"You quit in The Dungeons of Doom on dungeon level 1 with 0 points,"}}
            {"t":"req","id":11,"fn":"display_nhwindow","a":{"win":1}}
            "#,
        );
        assert_eq!((w.message_win, w.map_win), (None, None));
        // the summary went to the text window, not to the log
        let logged: Vec<_> = w.log.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(logged, vec!["Really quit?"]);
        let Some(Prompt::Show { title: None, lines }) = p else {
            panic!("{p:?}");
        };
        assert_eq!(lines.len(), 3);
        assert!(texts(&w.last_text).iter().any(|t| t.contains("You quit")));
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"destroy_nhwindow","a":{"win":1}}
            {"t":"win","fn":"exit_nhwindows","a":{"str":null}}
            {"t":"win","fn":"raw_print","a":{"str":" No  Points     Name","bold":false}}
            "#,
        );
        assert!(w.windows.is_empty());
        assert!(w.windows_exited);
        assert_eq!(w.raw_lines, vec![" No  Points     Name"]);
        assert_eq!(w.last_text.len(), 3);
    }

    #[test]
    fn a_destroyed_text_window_leaves_its_lines() {
        let mut w = World::new();
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"create_nhwindow","a":{"win":4,"type":"text"}}
            {"t":"win","fn":"putstr","a":{"win":4,"attr":0,"str":"You die..."}}
            {"t":"win","fn":"display_nhwindow","a":{"win":4}}
            "#,
        );
        assert_eq!(texts(&w.last_text), vec!["You die..."]);
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"create_nhwindow","a":{"win":5,"type":"text"}}
            {"t":"win","fn":"putstr","a":{"win":5,"attr":0,"str":"Goodbye"}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":5}}
            {"t":"win","fn":"create_nhwindow","a":{"win":5,"type":"text"}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":5}}
            "#,
        );
        // an empty window (ESC at the first disclosure question) changes nothing
        assert_eq!(texts(&w.last_text), vec!["Goodbye"]);
    }

    #[test]
    fn text_seen_during_play_is_not_the_summary() {
        let mut w = World::new();
        feed(&mut w, START);
        let p = feed(
            &mut w,
            r#"
            {"t":"win","fn":"create_nhwindow","a":{"win":4,"type":"text"}}
            {"t":"win","fn":"putstr","a":{"win":4,"attr":0,"str":"Unix NetHack Version 5.0.0"}}
            {"t":"req","id":5,"fn":"display_nhwindow","a":{"win":4}}
            "#,
        );
        assert!(matches!(p, Some(Prompt::Show { .. })), "{p:?}");
        // #quit, then q to the first disclosure question: the summary
        // window is created and destroyed empty
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"destroy_nhwindow","a":{"win":4}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":2}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":1}}
            {"t":"win","fn":"create_nhwindow","a":{"win":1,"type":"text"}}
            {"t":"win","fn":"destroy_nhwindow","a":{"win":1}}
            "#,
        );
        assert!(w.last_text.is_empty(), "{:?}", w.last_text);
    }

    #[test]
    fn an_empty_text_window_is_not_shown() {
        let mut w = World::new();
        feed(&mut w, START);
        let p = feed(
            &mut w,
            r#"
            {"t":"win","fn":"create_nhwindow","a":{"win":4,"type":"text"}}
            {"t":"req","id":5,"fn":"display_nhwindow","a":{"win":4}}
            "#,
        );
        assert_eq!(p, Some(Prompt::AutoAck));
        let p = feed(
            &mut w,
            r#"{"t":"req","id":6,"fn":"display_nhwindow","a":{"win":3}}"#,
        );
        assert_eq!(p, Some(Prompt::AutoAck));
    }

    #[test]
    fn log_skips_nohistory() {
        let mut w = World::new();
        feed(
            &mut w,
            &format!(
                "{START}{}",
                r#"
                {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Pick a monster, object or location."}}
                {"t":"win","fn":"putstr","a":{"win":1,"attr":32,"str":"floor of a room"}}
                {"t":"win","fn":"putstr","a":{"win":1,"attr":48,"str":"wall"}}
                {"t":"win","fn":"putstr","a":{"win":1,"attr":17,"str":"You hear the shopkeeper."}}
                "#
            ),
        );
        let logged: Vec<_> = w
            .log
            .iter()
            .map(|m| (m.text.as_str(), m.attr, m.urgent, m.turn))
            .collect();
        assert_eq!(
            logged,
            vec![
                ("Pick a monster, object or location.", 0, false, Some(7)),
                ("You hear the shopkeeper.", 1, true, Some(7)),
            ]
        );
        // the shopkeeper's message took the top line from "wall"
        assert_eq!(w.transient, None);
        feed(
            &mut w,
            r#"{"t":"win","fn":"putstr","a":{"win":1,"attr":32,"str":"doorway"}}"#,
        );
        assert_eq!(w.transient.as_deref(), Some("doorway"));
        feed(&mut w, r#"{"t":"win","fn":"clear_nhwindow","a":{"win":1}}"#);
        assert_eq!(w.transient, None);
        // and a key the player pressed takes it too
        feed(
            &mut w,
            r#"{"t":"win","fn":"putstr","a":{"win":1,"attr":32,"str":"corridor"}}"#,
        );
        w.note_player_input();
        assert_eq!(w.transient, None);
        assert_eq!(w.log.len(), 2);
        // restored history is marked
        feed(
            &mut w,
            r#"{"t":"win","fn":"putmsghistory","a":{"msg":"old news","restoring":true}}"#,
        );
        let last = w.log.iter().next_back().unwrap();
        assert!(last.from_history && last.turn.is_none());
        // putmsghistory during play belongs to this session
        feed(
            &mut w,
            r#"{"t":"win","fn":"putmsghistory","a":{"msg":"Count: 20 s","restoring":false}}"#,
        );
        let last = w.log.iter().next_back().unwrap();
        assert_eq!(
            (last.text.as_str(), last.from_history, last.turn),
            ("Count: 20 s", false, Some(7))
        );
        // the end of a restore: no message
        feed(
            &mut w,
            r#"{"t":"win","fn":"putmsghistory","a":{"msg":null,"restoring":true}}"#,
        );
        assert_eq!(w.log.len(), 4);
    }

    #[test]
    fn getpos_prompts_stay_out_of_the_log() {
        let mut w = World::new();
        feed(&mut w, START);
        w.note_player_input();
        let p = feed(
            &mut w,
            r#"
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"You see here a dagger."}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Where do you want to travel to?"}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"(For instructions type a '?')"}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Move cursor to the desired destination:"}}
            {"t":"win","fn":"putstr","a":{"win":1,"attr":32,"str":"doorway"}}
            {"t":"req","id":1,"fn":"nh_poskey","a":{"getpos":true}}
            "#,
        );
        assert_eq!(p, Some(Prompt::Command));
        let logged: Vec<&str> = w.log.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(logged, ["You see here a dagger."]);
        let line = w.getpos_line().unwrap();
        assert!(
            line.starts_with("Where do you want to travel to?"),
            "{line}"
        );
        assert_eq!(w.transient.as_deref(), Some("doorway"));
        // the command after getpos: no prompt line
        feed(&mut w, r#"{"t":"req","id":2,"fn":"nh_poskey","a":{}}"#);
        assert_eq!(w.getpos_line(), None);
        // without a goal said since the last key, the tip goes and the log stays
        w.note_player_input();
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"(For instructions type a '?')"}}
            {"t":"req","id":3,"fn":"nh_poskey","a":{"getpos":true}}
            "#,
        );
        let logged: Vec<&str> = w.log.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(logged, ["You see here a dagger."]);
        assert!(w.getpos_line().unwrap().starts_with("Pick a spot."));
    }

    #[test]
    fn an_undrawn_hero_is_where_the_command_cursor_was() {
        let mut w = World::new();
        feed(&mut w, START);
        let hero = r#"{"t":"win","fn":"print_glyph","a":{"win":2,"x":5,"y":4,"g":{"glyph":341,"ch":64,"color":15,"flags":1,"tile":0,"kind":"mon","mon":341}}}"#;
        feed(&mut w, hero);
        assert_eq!(w.hero(), Some((5, 4)));
        // invisible: the cell shows the floor, the core's cursor the hero
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"print_glyph","a":{"win":2,"x":5,"y":4,"g":{"glyph":2400,"ch":46,"color":7,"flags":0,"tile":0,"kind":"cmap","cmap":19}}}
            {"t":"win","fn":"curs","a":{"win":2,"x":5,"y":4}}
            {"t":"req","id":1,"fn":"nh_poskey","a":{}}
            "#,
        );
        assert_eq!(w.map.hero(), None);
        assert_eq!(w.hero(), Some((5, 4)));
        // a getpos cursor is not the hero
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"curs","a":{"win":2,"x":9,"y":4}}
            {"t":"req","id":2,"fn":"nh_poskey","a":{"getpos":true}}
            "#,
        );
        assert_eq!(w.hero(), Some((5, 4)));
        // a new level forgets it
        feed(&mut w, r#"{"t":"win","fn":"clear_nhwindow","a":{"win":2}}"#);
        assert_eq!(w.hero(), None);
    }

    #[test]
    fn doprev_message_asks_for_the_history() {
        let mut w = World::new();
        feed(&mut w, START);
        assert!(!w.wants_history);
        feed(&mut w, r#"{"t":"win","fn":"doprev_message","a":{}}"#);
        assert!(std::mem::take(&mut w.wants_history));
    }

    #[test]
    fn map_pause_waits() {
        let mut w = World::new();
        feed(&mut w, START);
        let pause = |w: &mut World, win: i32| w.on_request(&Request::DisplayNhwindow { win });
        assert_eq!(pause(&mut w, 2), Prompt::MapPause);
        assert_eq!(pause(&mut w, 1), Prompt::AutoAck);
        assert_eq!(pause(&mut w, 99), Prompt::AutoAck);
        feed(
            &mut w,
            r#"
            {"t":"win","fn":"start_menu","a":{"win":3,"behavior":0}}
            {"t":"win","fn":"add_menu","a":{"win":3,"idx":0,"glyph":null,"selectable":false,"ch":0,"gch":0,"attr":7,"clr":8,"str":"Weapons","preselected":false}}
            {"t":"win","fn":"end_menu","a":{"win":3,"prompt":"Discoveries"}}
            "#,
        );
        let Prompt::Show { title, lines } = pause(&mut w, 3) else {
            panic!("menu window shows as text");
        };
        assert_eq!(title.as_deref(), Some("Discoveries"));
        assert_eq!(texts(&lines), vec!["Weapons"]);
        // select_menu PICK_NONE is read-only too; PICK_ONE is a menu
        let p = w.on_request(&Request::SelectMenu {
            win: 3,
            how: PickHow::None,
        });
        assert!(matches!(p, Prompt::Show { .. }), "{p:?}");
        let p = w.on_request(&Request::SelectMenu {
            win: 3,
            how: PickHow::One,
        });
        let Prompt::Menu { items, title, .. } = p else {
            panic!("{p:?}");
        };
        assert_eq!((items.len(), title.as_deref()), (1, Some("Discoveries")));
    }

    #[test]
    fn message_menu_goes_to_the_log() {
        let mut w = World::new();
        feed(&mut w, START);
        let p = feed(
            &mut w,
            r#"{"t":"req","id":3,"fn":"message_menu","a":{"let":102,"how":1,"mesg":"f - an uncursed food ration."}}"#,
        );
        assert_eq!(
            p,
            Some(Prompt::MessageMenu {
                letter: 'f',
                mesg: "f - an uncursed food ration.".into(),
                pick: true
            })
        );
        let p = feed(
            &mut w,
            r#"{"t":"req","id":4,"fn":"message_menu","a":{"let":97,"how":0,"mesg":"a - a +1 spear."}}"#,
        );
        assert!(matches!(p, Some(Prompt::MessageMenu { pick: false, .. })));
        let logged: Vec<_> = w.log.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(
            logged,
            vec!["f - an uncursed food ration.", "a - a +1 spear."]
        );
    }

    #[test]
    fn requests_map_to_prompts() {
        let mut w = World::new();
        feed(&mut w, START);
        let p = |w: &mut World, line: &str| feed(w, line).unwrap();
        assert_eq!(
            p(&mut w, r#"{"t":"req","id":1,"fn":"nh_poskey","a":{}}"#),
            Prompt::Command
        );
        assert_eq!(
            p(&mut w, r#"{"t":"req","id":1,"fn":"nhgetch","a":{}}"#),
            Prompt::Key
        );
        assert_eq!(
            p(&mut w, r#"{"t":"req","id":1,"fn":"get_ext_cmd","a":{}}"#),
            Prompt::ExtCmd
        );
        assert_eq!(
            p(&mut w, r#"{"t":"req","id":1,"fn":"askname","a":{}}"#),
            Prompt::Text {
                query: "Who are you?".into(),
                name: true
            }
        );
        assert_eq!(
            p(
                &mut w,
                r#"{"t":"req","id":1,"fn":"getlin","a":{"query":"For what do you wish?"}}"#
            ),
            Prompt::Text {
                query: "For what do you wish?".into(),
                name: false
            }
        );
        assert_eq!(
            p(
                &mut w,
                r#"{"t":"req","id":1,"fn":"yn_function","a":{"query":"In what direction?","choices":null,"default":0}}"#
            ),
            Prompt::FreeKey {
                query: "In what direction?".into(),
                directions: true
            }
        );
        let yn = p(
            &mut w,
            r#"{"t":"req","id":1,"fn":"yn_function","a":{"query":"Really quit without saving?","choices":"yn","default":110}}"#,
        );
        assert!(
            matches!(
                yn,
                Prompt::Choice {
                    default: Some('n'),
                    ..
                }
            ),
            "{yn:?}"
        );
        let file = p(
            &mut w,
            r#"{"t":"req","id":1,"fn":"display_file","a":{"name":"history","lines":["NetHack",null]}}"#,
        );
        let Prompt::Show { title, lines } = file else {
            panic!("{file:?}");
        };
        assert_eq!(
            (title.as_deref(), texts(&lines)),
            (Some("history"), vec!["NetHack", ""])
        );
    }

    #[test]
    fn map_calls_follow_the_map_window() {
        let mut w = World::new();
        feed(
            &mut w,
            &format!(
                "{START}{}",
                r#"
                {"t":"win","fn":"print_glyph","a":{"win":2,"x":18,"y":5,"g":{"ch":64,"color":15,"flags":1,"tile":350,"kind":"mon","mon":344,"glyph":344},"bk":null}}
                {"t":"win","fn":"print_glyph","a":{"win":3,"x":19,"y":5,"g":{"ch":64,"color":15,"flags":1,"tile":350,"kind":"mon","mon":344,"glyph":344},"bk":null}}
                {"t":"win","fn":"curs","a":{"win":2,"x":18,"y":5}}
                {"t":"win","fn":"curs","a":{"win":1,"x":3,"y":0}}
                {"t":"win","fn":"cliparound","a":{"x":18,"y":6}}
                {"t":"win","fn":"number_pad","a":{"state":1,"dirchars":"47896321><"}}
                "#
            ),
        );
        assert_eq!(w.map.hero(), Some((18, 5)));
        assert_eq!(w.map.take_dirty(), vec![(18, 5)]);
        assert_eq!(w.cursor, Some((18, 5)));
        assert_eq!(w.view_center, Some((18, 6)));
        assert!(w.number_pad);
        assert_eq!(w.dirchars, "47896321><");
        feed(&mut w, r#"{"t":"win","fn":"clear_nhwindow","a":{"win":2}}"#);
        assert_eq!(w.map.generation(), 1);
        assert_eq!(w.map.hero(), None);
    }

    #[test]
    fn a_sparkle_over_a_monster_keeps_the_floor() {
        let mut w = World::new();
        let EngineMsg::Catalog(cat) =
            parse_line(include_str!("../tests/data/catalog.jsonl")).expect("catalog")
        else {
            panic!("not a catalog");
        };
        w.set_catalog(&cat);
        // floor, a little dog (bk is never useful), shieldeff's S_ss1, the dog again
        feed(
            &mut w,
            &format!(
                "{START}{}",
                r#"
                {"t":"win","fn":"print_glyph","a":{"win":2,"x":10,"y":5,"g":{"ch":46,"color":7,"flags":0,"tile":0,"kind":"cmap","cmap":19},"bk":{"ch":32,"color":0,"flags":0,"tile":0,"kind":"unexplored"}}}
                {"t":"win","fn":"print_glyph","a":{"win":2,"x":10,"y":5,"g":{"ch":100,"color":7,"flags":0,"tile":0,"kind":"mon","mon":16},"bk":{"ch":32,"color":0,"flags":0,"tile":0,"kind":"unexplored"}}}
                {"t":"win","fn":"print_glyph","a":{"win":2,"x":10,"y":5,"g":{"ch":48,"color":12,"flags":0,"tile":0,"kind":"cmap","cmap":82},"bk":{"ch":32,"color":0,"flags":0,"tile":0,"kind":"unexplored"}}}
                {"t":"win","fn":"print_glyph","a":{"win":2,"x":10,"y":5,"g":{"ch":100,"color":7,"flags":0,"tile":0,"kind":"mon","mon":16},"bk":{"ch":32,"color":0,"flags":0,"tile":0,"kind":"unexplored"}}}
                "#
            ),
        );
        let cell = w.map.cell(10, 5).unwrap();
        assert_eq!(cell_terrain(cell, &cat), Some(Terrain::Floor));
        assert_eq!(
            describe_cell(cell, &cat).as_deref(),
            Some("little dog\nfloor of a room")
        );
    }

    #[test]
    fn player_input_marks_the_log() {
        let mut w = World::new();
        feed(
            &mut w,
            &format!(
                "{START}{}",
                r#"{"t":"win","fn":"putstr","a":{"win":1,"attr":0,"str":"Hello"}}"#
            ),
        );
        assert_eq!(w.input_seq(), 0);
        w.note_player_input();
        assert_eq!(w.input_seq(), 1);
        assert_eq!(w.log.since(w.input_seq()).count(), 0);
    }
}
