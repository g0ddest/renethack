//! Moved keys (the settings' controls): `rebind` opens the Classic keys'
//! list on the title, moves the search onto `a` (the apply's key: they
//! swap, after Enter) and the help onto F6 (a free key), checks what the
//! profile keeps and what a start reads back; the inventory onto F7 and
//! back (Delete), a Modern key moved and every Modern key put back. Then
//! in a game of Classic keys: `a` gives the engine `s`; F6 opens and
//! closes the help, F1 no longer does; at a getobj question the letters
//! are what they are (`a` drops the item `a`).

use godot::classes::{Input, InputEventKey};
use godot::global::Key as GKey;
use godot::prelude::*;
use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Profile, Prompt};

use super::{Step, fail_on_error_screen, quit, screen, start};
use crate::bindings::Shown;
use crate::game::{RenethackGame, parse_args, startup_settings};
use crate::settings_page::{Edit, Field, Section, SettingsInput, SettingsPage};
use crate::ui_events::UiEvent;

fn page(g: &RenethackGame) -> Result<&SettingsPage, String> {
    g.settings_page
        .as_ref()
        .ok_or_else(|| "the settings page is not open".to_string())
}

fn page_mut(g: &mut RenethackGame) -> Result<&mut SettingsPage, String> {
    g.settings_page
        .as_mut()
        .ok_or_else(|| "the settings page is not open".to_string())
}

fn ch(c: char) -> KeyInput {
    KeyInput::plain(Key::Char(c))
}

fn f(n: u8) -> KeyInput {
    KeyInput::plain(Key::F(n))
}

/// A key pressed and let go through Godot's input.
fn tap_key(k: GKey) {
    for pressed in [true, false] {
        let mut ev = InputEventKey::new_gd();
        ev.set_keycode(k);
        ev.set_physical_keycode(k);
        ev.set_pressed(pressed);
        Input::singleton().parse_input_event(&ev);
    }
}

macro_rules! keys {
    ($what:literal, $($k:ident),+) => {
        Step::Call($what, |_| {
            $( tap_key(GKey::$k); )+
            Ok(())
        })
    };
}

/// The profile as it was written.
fn profile(g: &RenethackGame) -> Result<Profile, String> {
    let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
    Ok(crate::game::load_profile(&pg))
}

/// Give the entry of the command whose own key is `command` the focus.
fn focus_command(g: &mut RenethackGame, command: KeyInput) -> Result<(), String> {
    let p = page_mut(g)?;
    let i = p
        .entry_of(command)
        .ok_or_else(|| format!("no entry for {command:?}"))?;
    p.focus_entry(i);
    Ok(())
}

pub(super) fn rebind() -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the title screen, its scene drawn", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("title") && ui.map.preloaded() && g.warmed_up())
        }),
        Step::Push(UiEvent::OpenSettings),
        Step::Wait("the settings", |g| Ok(screen(g) == Some("settings"))),
        Step::Push(UiEvent::Settings(SettingsInput::Tab(Section::Controls))),
        Step::Wait("the controls", |g| {
            Ok(page(g)?.section() == Section::Controls)
        }),
        // the Classic keys' list (a new character's keys are Modern)
        keys!("→", RIGHT),
        Step::Wait("the Classic keys", |g| {
            Ok(page(g)?.controls() == Shown::Keys(KeyProfile::Classic))
        }),
        // the search onto a: the apply has it, Enter swaps them
        Step::Call("the search's entry", |g| focus_command(g, ch('s'))),
        keys!("Enter", ENTER),
        Step::Wait("the search waits for its key", |g| {
            Ok(matches!(page(g)?.edit(), Edit::Press(_)))
        }),
        keys!("a", A),
        Step::Wait("a is the apply's: swap?", |g| {
            Ok(matches!(page(g)?.edit(), Edit::Swap { key, .. } if key == ch('a')))
        }),
        Step::Shot("rebind-swap"),
        keys!("Enter", ENTER),
        Step::Wait("the two swapped, kept in the profile", |g| {
            let kept = profile(g)?.settings.keymaps;
            let map = kept.get(KeyProfile::Classic);
            Ok(page(g)?.edit() == Edit::None
                && map.translate(ch('a')) == Some(ch('s'))
                && map.translate(ch('s')) == Some(ch('a'))
                && kept.get(KeyProfile::Modern).is_empty())
        }),
        // the help onto F6, a key of nothing's: F1 then gives nothing
        Step::Call("the help's entry", |g| focus_command(g, f(1))),
        keys!("Enter, F6", ENTER, F6),
        Step::Wait("the help on F6, kept", |g| {
            let kept = profile(g)?.settings.keymaps;
            let map = kept.get(KeyProfile::Classic);
            Ok(map.translate(f(6)) == Some(f(1)) && map.translate(f(1)).is_none())
        }),
        Step::Shot("rebind-list"),
        Step::Call("what a start reads back", |g| {
            let pg = g.paths.as_ref().ok_or("no paths")?.playground.clone();
            let args = parse_args(&[format!("--playground={}", pg.display())]);
            let read = startup_settings(&args);
            if read.keymaps != g.settings.keymaps {
                return Err(format!(
                    "a start reads {:?}, the page has {:?}",
                    read.keymaps, g.settings.keymaps
                ));
            }
            Ok(())
        }),
        // the inventory onto F7, then back on its own key (Delete)
        Step::Call("the inventory's entry", |g| focus_command(g, ch('i'))),
        keys!("Enter, F7", ENTER, F7),
        Step::Wait("the inventory on F7", |g| {
            let kept = profile(g)?.settings.keymaps;
            Ok(kept.get(KeyProfile::Classic).translate(f(7)) == Some(ch('i')))
        }),
        keys!("Delete", DELETE),
        Step::Wait("the inventory on i again, the others kept", |g| {
            let kept = profile(g)?.settings.keymaps;
            let map = kept.get(KeyProfile::Classic);
            Ok(map.translate(f(7)) == Some(f(7))
                && map.translate(ch('i')) == Some(ch('i'))
                && map.translate(ch('a')) == Some(ch('s'))
                && map.translate(f(6)) == Some(f(1)))
        }),
        // a Modern key moved, then every Modern key put back: the Classic
        // ones stay
        Step::Call("the list's row", |g| {
            page_mut(g)?.focus_field(Field::Controls);
            Ok(())
        }),
        keys!("←", LEFT),
        Step::Wait("the Modern keys", |g| {
            Ok(page(g)?.controls() == Shown::Keys(KeyProfile::Modern))
        }),
        Step::Call("the Modern search's entry", |g| focus_command(g, ch('s'))),
        keys!("Enter, F7", ENTER, F7),
        Step::Wait("the Modern search on F7", |g| {
            let kept = profile(g)?.settings.keymaps;
            Ok(kept.get(KeyProfile::Modern).translate(f(7)) == Some(ch('s')))
        }),
        Step::Call("the reset's entry", |g| {
            page_mut(g)?.focus_entry(0);
            Ok(())
        }),
        keys!("Enter", ENTER),
        Step::Wait("put every key back?", |g| {
            Ok(page(g)?.edit() == Edit::ResetAll)
        }),
        keys!("Enter", ENTER),
        Step::Wait("the Modern keys their own, the Classic kept", |g| {
            let kept = profile(g)?.settings.keymaps;
            let classic = kept.get(KeyProfile::Classic);
            Ok(page(g)?.edit() == Edit::None
                && kept.get(KeyProfile::Modern).is_empty()
                && classic.translate(ch('a')) == Some(ch('s'))
                && classic.translate(f(6)) == Some(f(1)))
        }),
        Step::Push(UiEvent::CloseSettings),
        Step::Wait("the title again", |g| Ok(screen(g) == Some("title"))),
    ];
    // a game of Classic keys
    steps.extend(start());
    steps.extend([
        Step::Wait("the first command, the level drawn", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))) && g.world.map.hero().is_some())
        }),
        Step::Call("a", |g| {
            g.sent.clear();
            tap_key(GKey::A);
            Ok(())
        }),
        Step::Wait("the engine got s for a", |g| {
            let got = g.sent.iter().find(|(p, _)| *p == Prompt::Command);
            match got {
                None => Ok(false),
                Some((_, Reply::Key(k))) if *k == 's' as i32 => Ok(true),
                Some((_, r)) => Err(format!("the engine got {r:?} for a")),
            }
        }),
        Step::Wait("the next command", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // F6 opens the help; F1, moved away, does nothing (F6 after it
        // still opens it)
        keys!("F1, F6", F1, F6),
        Step::Wait("the help open on F6", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.help.is_open()))
        }),
        Step::Shot("rebind-help"),
        keys!("F6", F6),
        Step::Wait("and closed on it", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| !ui.help.is_open()))
        }),
        // a getobj question's letters are what they are: a drops the item a
        Step::Call("d", |g| {
            g.sent.clear();
            tap_key(GKey::D);
            Ok(())
        }),
        Step::Wait("what to drop?", |g| {
            Ok(
                matches!(g.pending, Some((_, ref p)) if *p != Prompt::Command)
                    && g.sent.iter().any(|(_, r)| *r == Reply::Key('d' as i32)),
            )
        }),
        Step::Call("a", |g| {
            g.sent.clear();
            tap_key(GKey::A);
            Ok(())
        }),
        Step::Wait("the question answered a, not s", |g| {
            let Some((_, r)) = g.sent.front() else {
                return Ok(false);
            };
            match r {
                Reply::Char(c) | Reply::Key(c) if *c == 'a' as i32 => Ok(true),
                other => Err(format!("the question got {other:?} for a")),
            }
        }),
        Step::Wait("the command prompt again", |g| {
            Ok(matches!(g.pending, Some((_, Prompt::Command))))
        }),
    ]);
    steps.extend(quit());
    steps
}
