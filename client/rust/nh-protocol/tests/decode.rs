//! Decoding real engine output. The fixture is what `engine/tests/smoke.sh`
//! records: a new game (seed 42, 2026-01-18) whose client hangs up at the
//! first request.

use nh_protocol::*;

const SESSION: &str = include_str!("data/eof-session.jsonl");

fn decoded() -> Vec<EngineMsg> {
    SESSION
        .lines()
        .map(|l| parse_line(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect()
}

fn catalog(msgs: &[EngineMsg]) -> &Catalog {
    match &msgs[1] {
        EngineMsg::Catalog(c) => c,
        other => panic!("expected catalog second, got {other:?}"),
    }
}

#[test]
fn every_recorded_line_decodes_to_a_known_call() {
    for msg in decoded() {
        if let EngineMsg::Win(WinCall::Unknown { name, .. }) = msg {
            panic!("unmapped window call {name}");
        }
    }
}

#[test]
fn session_is_hello_catalog_play_error_bye() {
    let msgs = decoded();
    let EngineMsg::Hello(hello) = &msgs[0] else {
        panic!("expected hello first")
    };
    assert_eq!(hello.protocol, PROTOCOL_VERSION);
    assert_eq!(hello.engine, "5.0.0");
    let cat = catalog(&msgs);
    assert!(cat.monsters.len() > 300);
    assert_eq!(cat.roles.len(), 13);
    assert_eq!(cat.races.len(), 5);
    assert!(msgs.contains(&EngineMsg::Error {
        msg: "client closed the connection".into()
    }));
    assert_eq!(msgs.last(), Some(&EngineMsg::Bye));
}

#[test]
fn catalog_describes_monsters_by_their_visible_traits() {
    let msgs = decoded();
    let jackal = catalog(&msgs)
        .monsters
        .iter()
        .find(|m| m.name == "jackal")
        .unwrap();
    assert_eq!(jackal.class, "d");
    assert_eq!(jackal.size, "small");
    assert!(jackal.body.contains(&"animal".to_string()));
    // M1_NOLIMBS is a composite mask; jackals have limbs
    assert!(!jackal.body.contains(&"nolimbs".to_string()));
}

#[test]
fn catalog_names_map_symbols_commands_and_conditions() {
    let msgs = decoded();
    let cat = catalog(&msgs);
    // every map symbol has its unique defsym.h name, in index order
    assert!(cat.cmap.iter().all(|c| c.sym.starts_with("S_")));
    assert_eq!(cat.cmap[1].sym, "S_vwall");
    let syms: std::collections::HashSet<_> = cat.cmap.iter().map(|c| &c.sym).collect();
    assert_eq!(syms.len(), cat.cmap.len());
    // extended commands a player may type, wizard-mode ones left out
    let pray = cat.extcmds.iter().find(|e| e.name == "pray").unwrap();
    assert!(pray.flags.contains(&"autocomplete".to_string()));
    assert!(cat.extcmds.iter().all(|e| !e.name.starts_with("wiz")));
    // conditions: one bit each
    let stone = cat.conditions.iter().find(|c| c.name == "Stone").unwrap();
    assert_eq!(stone.mask, 0x0010_0000);
    assert!(cat.conditions.iter().all(|c| c.mask.count_ones() == 1));
}

#[test]
fn object_tiles_never_name_the_hidden_identity() {
    let msgs = decoded();
    let cat = catalog(&msgs);
    for t in &cat.object_tiles {
        for secret in [
            "potion of",
            "scroll of",
            "wand of",
            "ring of",
            "spellbook of",
        ] {
            assert!(
                !t.appearance.contains(secret),
                "{} leaks identity",
                t.appearance
            );
        }
    }
    assert!(
        cat.object_tiles
            .iter()
            .any(|t| t.appearance == "runed dagger")
    );
}

#[test]
fn object_tiles_list_each_appearance_once() {
    // NetHack gives look-alike objects it never shuffles (sack / bag of
    // holding, oil / magic lamp, the gray stones...) tiles of their own;
    // listing those would map tiles back to object types
    let msgs = decoded();
    let mut seen = std::collections::HashSet::new();
    for t in &catalog(&msgs).object_tiles {
        assert!(
            seen.insert((t.class.clone(), t.appearance.clone())),
            "{} {:?} listed twice",
            t.class,
            t.appearance
        );
    }
}

#[test]
fn object_glyphs_carry_no_glyph_number() {
    let objects: Vec<Glyph> = decoded()
        .into_iter()
        .filter_map(|m| match m {
            EngineMsg::Win(WinCall::PrintGlyph { g, .. }) if g.kind == GlyphKind::Obj => Some(g),
            _ => None,
        })
        .collect();
    assert!(!objects.is_empty(), "fixture shows no object");
    assert!(objects.iter().all(|g| g.glyph.is_none() && g.tile > 0));
}

#[test]
fn hero_is_a_flagged_valkyrie_glyph() {
    let msgs = decoded();
    let hero = msgs
        .iter()
        .find_map(|m| match m {
            EngineMsg::Win(WinCall::PrintGlyph { g, .. }) if g.flags & mg::HERO != 0 => Some(g),
            _ => None,
        })
        .expect("hero drawn");
    assert_eq!(hero.kind, GlyphKind::Mon);
    let mon = &catalog(&msgs).monsters[hero.mon.unwrap() as usize];
    assert_eq!(mon.name, "valkyrie");
}

#[test]
fn welcome_message_goes_to_the_message_window() {
    let msgs = decoded();
    let message_win = msgs
        .iter()
        .find_map(|m| match m {
            EngineMsg::Win(WinCall::CreateNhwindow {
                win,
                kind: WindowKind::Message,
            }) => Some(*win),
            _ => None,
        })
        .unwrap();
    assert!(msgs.iter().any(|m| matches!(m,
        EngineMsg::Win(WinCall::Putstr { win, text, .. })
            if *win == message_win && text.contains("welcome to NetHack"))));
}

#[test]
fn menu_items_and_number_pad_carry_their_additions() {
    // what the D menu sends for "Auto-select every relevant item"
    let item = r#"{"t":"win","fn":"add_menu","a":{"win":4,"idx":0,"glyph":null,"selectable":true,"ch":65,"gch":0,"attr":0,"clr":8,"str":"Auto-select every relevant item","preselected":false,"skipinvert":true}}"#;
    let EngineMsg::Win(WinCall::AddMenu(m)) = parse_line(item).unwrap() else {
        panic!("expected add_menu")
    };
    assert!(m.skipinvert);
    // older engines send neither field
    let old = item.replace(r#","skipinvert":true"#, "");
    let EngineMsg::Win(WinCall::AddMenu(m)) = parse_line(&old).unwrap() else {
        panic!("expected add_menu")
    };
    assert!(!m.skipinvert);
    let pad = r#"{"t":"win","fn":"number_pad","a":{"state":1,"dirchars":"47896321><"}}"#;
    assert_eq!(
        parse_line(pad).unwrap(),
        EngineMsg::Win(WinCall::NumberPad {
            state: 1,
            dirchars: Some("47896321><".into())
        })
    );
}

#[test]
fn requests_decode_with_their_arguments() {
    let yn = r#"{"t":"req","id":3,"fn":"yn_function","a":{"query":"Really quit without saving?","choices":"yn","default":0}}"#;
    assert_eq!(
        parse_line(yn).unwrap(),
        EngineMsg::Req {
            id: 3,
            req: Request::YnFunction {
                query: "Really quit without saving?".into(),
                choices: Some("yn".into()),
                default: 0
            }
        }
    );
    let menu = r#"{"t":"req","id":4,"fn":"select_menu","a":{"win":5,"how":1}}"#;
    assert_eq!(
        parse_line(menu).unwrap(),
        EngineMsg::Req {
            id: 4,
            req: Request::SelectMenu {
                win: 5,
                how: PickHow::One
            }
        }
    );
    let getlin = r#"{"t":"req","id":5,"fn":"getlin","a":{"query":"For what do you wish?"}}"#;
    assert!(matches!(
        parse_line(getlin).unwrap(),
        EngineMsg::Req {
            req: Request::Getlin { .. },
            ..
        }
    ));
    let first = decoded()
        .into_iter()
        .find_map(|m| match m {
            EngineMsg::Req { req, .. } => Some(req),
            _ => None,
        })
        .unwrap();
    assert_eq!(first, Request::NhPoskey { getpos: false });
}

#[test]
fn replies_encode_to_the_wire_shapes() {
    assert_eq!(
        encode_reply(3, &Reply::Key(104).to_value()),
        r#"{"id":3,"r":{"key":104}}"#
    );
    assert_eq!(
        encode_reply(4, &Reply::Menu(vec![(2, -1)]).to_value()),
        r#"{"id":4,"r":{"items":[[2,-1]]}}"#
    );
    assert_eq!(
        encode_reply(5, &Reply::ExtCmd(None).to_value()),
        r#"{"id":5,"r":{"cmd":null}}"#
    );
    assert_eq!(
        encode_reply(6, &Reply::Ack.to_value()),
        r#"{"id":6,"r":{}}"#
    );
    let click = Reply::Click {
        x: 3,
        y: 4,
        modifier: 1,
    }
    .to_value();
    assert_eq!(click["mod"], 1);
}

#[test]
fn malformed_lines_are_errors() {
    assert!(matches!(parse_line("nope"), Err(ProtocolError::Json(_))));
    assert!(matches!(
        parse_line(r#"{"t":"zzz","a":{}}"#),
        Err(ProtocolError::UnknownType(_))
    ));
    assert!(matches!(
        parse_line(r#"{"t":"req","fn":"nhgetch","a":{}}"#),
        Err(ProtocolError::MissingId)
    ));
    assert!(matches!(
        parse_line(r#"{"t":"req","id":1,"fn":"teleport_me","a":{}}"#),
        Err(ProtocolError::UnknownRequest(_))
    ));
    // unknown notifications are tolerated, so newer engines keep working
    assert!(matches!(
        parse_line(r#"{"t":"win","fn":"future_call","a":{"x":1}}"#),
        Ok(EngineMsg::Win(WinCall::Unknown { .. }))
    ));
}

#[test]
fn inventory_decodes_items_slots_and_twoweap() {
    let line = r#"{"t":"win","fn":"inventory","a":{"items":[{"letter":"a","class":")","tile":816,"quan":1,"slots":["weapon"],"lit":false,"text":"a +1 spear (weapon in right hand)"},{"letter":"b","class":")","tile":823,"quan":1,"slots":["alternate"],"lit":false,"text":"a +0 dagger (wielded in left hand)"},{"letter":"e","class":"(","tile":1018,"quan":1,"slots":[],"lit":true,"text":"an oil lamp (lit)"},{"letter":"f","class":"=","tile":1100,"quan":1,"slots":["left_ring","tail_ring"],"lit":false,"text":"a jade ring (on left hand)"}],"twoweap":true}}"#;
    let EngineMsg::Win(WinCall::Inventory(inv)) = parse_line(line).unwrap() else {
        panic!("expected inventory")
    };
    assert!(inv.twoweap);
    assert_eq!(inv.items.len(), 4);
    let spear = &inv.items[0];
    assert_eq!(spear.letter, 'a');
    assert_eq!(spear.class, ')');
    assert_eq!(spear.tile, 816);
    assert_eq!(spear.quan, 1);
    assert_eq!(spear.slots, vec![Slot::Weapon]);
    assert_eq!(spear.text, "a +1 spear (weapon in right hand)");
    assert_eq!(inv.items[1].slots, vec![Slot::Alternate]);
    assert!(inv.items[2].lit);
    // a slot a newer engine adds is kept, not an error
    assert_eq!(
        inv.items[3].slots,
        vec![Slot::LeftRing, Slot::Other("tail_ring".into())]
    );
}

#[test]
fn level_decodes_the_branch_the_depth_and_a_plane() {
    let line = r#"{"t":"win","fn":"level","a":{"dungeon":"The Gnomish Mines","depth":3}}"#;
    let EngineMsg::Win(WinCall::Level(l)) = parse_line(line).unwrap() else {
        panic!("expected level")
    };
    assert_eq!(l.dungeon, "The Gnomish Mines");
    assert_eq!(l.depth, 3);
    assert_eq!(l.plane, None);
    let line = r#"{"t":"win","fn":"level","a":{"dungeon":"The Elemental Planes","depth":-2,"plane":"air"}}"#;
    let EngineMsg::Win(WinCall::Level(l)) = parse_line(line).unwrap() else {
        panic!("expected level")
    };
    assert_eq!(l.plane.as_deref(), Some("air"));
}

#[test]
fn progress_decodes_achievements_events_conducts_and_the_end() {
    let line = r#"{"t":"win","fn":"progress","a":{"mode":"normal","achieved":[15,23,21],"events":{"qcalled":true,"uheard_tune":2,"udemigod":false},"deepest":7,"conduct":{"unvegan":0,"wishes":2},"roleplay":{"blind":false,"nudist":true},"gameover":true,"how":"ascended"}}"#;
    let EngineMsg::Win(WinCall::Progress(p)) = parse_line(line).unwrap() else {
        panic!("expected progress")
    };
    assert_eq!(p.mode, "normal");
    assert_eq!(p.achieved, vec![15, 23, 21]);
    assert!(p.achieved(21) && !p.achieved(10));
    // a flag and a stage both read as numbers
    assert_eq!(p.events["qcalled"], 1);
    assert_eq!(p.events["uheard_tune"], 2);
    assert!(p.event("qcalled") && p.event("uheard_tune") && !p.event("udemigod"));
    assert_eq!(p.deepest, 7);
    assert_eq!(p.conduct["wishes"], 2);
    assert!(p.roleplay["nudist"]);
    assert!(p.gameover);
    assert_eq!(p.how.as_deref(), Some("ascended"));
}

#[test]
fn recorded_session_tells_a_fresh_game_has_earned_nothing() {
    let p = decoded()
        .into_iter()
        .find_map(|m| match m {
            EngineMsg::Win(WinCall::Progress(p)) => Some(p),
            _ => None,
        })
        .expect("a progress notice");
    assert_eq!(p.mode, "normal");
    assert!(p.achieved.is_empty());
    assert!(p.events.values().all(|&n| n == 0), "{:?}", p.events);
    assert_eq!(p.deepest, 1);
    assert!(!p.gameover);
    assert_eq!(p.how, None);
    // the valkyrie's kitten: a pet, no other conduct broken
    assert_eq!(p.conduct["pets"], 1);
    assert_eq!(p.conduct["unvegan"], 0);
}

#[test]
fn recorded_session_carries_the_starting_inventory() {
    let inv = decoded()
        .into_iter()
        .find_map(|m| match m {
            EngineMsg::Win(WinCall::Inventory(inv)) => Some(inv),
            _ => None,
        })
        .expect("an inventory notice");
    assert!(!inv.twoweap);
    let letters: String = inv.items.iter().map(|i| i.letter).collect();
    assert_eq!(letters, "abcde");
    assert_eq!(inv.items[0].slots, vec![Slot::Weapon]);
    assert!(inv.items[0].text.contains("spear"));
    assert_eq!(inv.items[2].slots, vec![Slot::Shield]);
    assert!(inv.items[3].slots.is_empty());
    // nothing identifying is on the wire
    let raw = SESSION
        .lines()
        .find(|l| l.starts_with(r#"{"t":"win","fn":"inventory""#))
        .unwrap();
    for key in ["\"glyph\"", "\"otyp\"", "\"weight\""] {
        assert!(!raw.contains(key), "{key} in {raw}");
    }
}
