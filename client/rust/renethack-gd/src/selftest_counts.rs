//! Counts are the engine's (the critics' pass, vet 1 and 10): a count
//! and its command go to NetHack as tty types them, and NetHack reads the
//! count itself. A samurai drops their ya and picks five of the stack up
//! again: `n5,` with Modern keys, `5,` with Classic ones, whose digits
//! count as in NetHack.

use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Prompt};

use super::{Check, Step, command, idle_command, key, letter_of, quit, smoke_choice, start_as};
use crate::game::RenethackGame;
use crate::ui_events::{CharacterChoice, UiEvent};

fn samurai(profile: KeyProfile) -> CharacterChoice {
    CharacterChoice {
        role: "samurai".into(),
        align: "lawful".into(),
        profile,
        ..smoke_choice()
    }
}

/// The ya in the pack: how many.
fn ya(g: &RenethackGame) -> Option<i64> {
    g.world
        .inventory
        .items()
        .iter()
        .find(|i| i.text.contains(" ya"))
        .map(|i| i.quan)
}

/// The keys the engine was answered at its command prompt, as text.
fn command_keys(g: &RenethackGame) -> String {
    g.sent
        .iter()
        .filter(|(p, _)| *p == Prompt::Command)
        .filter_map(|(_, r)| match r {
            Reply::Key(k) => u8::try_from(*k).ok().map(char::from),
            _ => None,
        })
        .collect()
}

/// Drop every ya, then pick five up with `count` typed before `,`: five
/// ya in the pack, and `sent` holds for what the engine was sent.
fn pick_five(count: Vec<Step>, sent: Check) -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the ya in the pack", |g| Ok(ya(g).is_some_and(|n| n > 5))),
        key('d'),
        Step::Request(
            "What do you want to drop?",
            |p| matches!(p, Prompt::FreeKey { query, .. } if query.contains("drop")),
        ),
        Step::KeyFrom("the ya's letter", |g| {
            let l = letter_of(g, " ya").ok_or("no ya")?;
            Ok(KeyInput::plain(Key::Char(l)))
        }),
        Step::Request("a command after the drop", command),
        Step::Wait("no ya in the pack", |g| {
            Ok(idle_command(g)? && ya(g).is_none())
        }),
        Step::Call("forget what was sent", |g| {
            g.sent.clear();
            Ok(())
        }),
    ];
    steps.extend(count);
    steps.extend([
        key(','),
        Step::Wait("five ya picked up", |g| {
            if !idle_command(g)? {
                return Ok(false);
            }
            match ya(g) {
                None => Ok(false),
                Some(5) => Ok(true),
                Some(n) => Err(format!(
                    "{n} ya picked up, the engine sent {:?}",
                    command_keys(g)
                )),
            }
        }),
        Step::Wait("the count went to the engine", sent),
    ]);
    steps
}

/// The engine was sent `want` at its command prompt since the drop.
fn keys_were(g: &RenethackGame, want: &str) -> Result<bool, String> {
    let sent = command_keys(g);
    if sent == want {
        Ok(true)
    } else {
        Err(format!("the engine was sent {sent:?}, not {want:?}"))
    }
}

pub(super) fn counts() -> Vec<Step> {
    let mut steps = start_as(samurai(KeyProfile::Modern));
    steps.extend(pick_five(vec![key('n'), key('5')], |g| keys_were(g, "n5,")));
    steps.extend(quit());
    steps.extend([
        Step::Push(UiEvent::BackToTitle),
        Step::Wait(
            "the title screen",
            |g| Ok(super::screen(g) == Some("title")),
        ),
        Step::Push(UiEvent::StartCharacter(samurai(KeyProfile::Classic))),
        Step::Request("the first command (Classic)", command),
    ]);
    steps.extend(pick_five(vec![key('5')], |g| keys_were(g, "5,")));
    steps.extend(quit());
    steps
}
