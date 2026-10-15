//! Two arrows pressed together step diagonally where a direction is
//! awaited (a laptop has no keypad): `chords` plays a Modern character
//! through Godot's input. At the command prompt each of the four chords
//! gives the engine its diagonal once, never an arrow's own step; a lone
//! arrow still steps once; held together, the diagonal repeats (an order
//! holds it) and nothing else goes; with Shift the chord runs. At a
//! direction's question (#chat's) a chord answers the diagonal. In a menu
//! (the engine's O) a chord answers nothing. The north-west step moved
//! onto `q` steps at the prompt and answers a direction's question.

use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};

use godot::classes::{Engine, Input, InputEventKey};
use godot::global::Key as GKey;
use godot::prelude::*;
use nh_protocol::Reply;
use nh_world::{Key, KeyInput, KeyProfile, Order, Prompt};

use super::{Step, command, key, modern_choice, quit, start_as};
use crate::game::RenethackGame;
use crate::ui_events::DialogEvent;

/// The engine's key code the next check wants: a direction's index (W NW
/// N NE E SE S SW), and 0x80 with Shift (number_pad's run).
static WANT: AtomicI32 = AtomicI32::new(0);
/// The request the menu was, and the frame the chord went in.
static MENU: AtomicI64 = AtomicI64::new(0);
static FRAME: AtomicI64 = AtomicI64::new(0);

const NW: i32 = 1;
const N: i32 = 2;
const NE: i32 = 3;
const SE: i32 = 5;
const SW: i32 = 7;
const RUN: i32 = 0x100;

/// A key event through Godot's input (`input()` has it next frame).
fn send(k: GKey, pressed: bool, shift: bool, echo: bool) {
    let mut ev = InputEventKey::new_gd();
    ev.set_keycode(k);
    ev.set_physical_keycode(k);
    ev.set_pressed(pressed);
    ev.set_shift_pressed(shift);
    ev.set_echo(echo);
    Input::singleton().parse_input_event(&ev);
}

/// Two keys pressed together, then let go.
fn together(a: GKey, b: GKey, shift: bool) {
    send(a, true, shift, false);
    send(b, true, shift, false);
    send(b, false, shift, false);
    send(a, false, shift, false);
}

/// The engine's code of the direction (and run) `want` stands for.
fn code(g: &RenethackGame, want: i32) -> Result<i32, String> {
    let chars: Vec<char> = g.world.dirchars.chars().collect();
    let c = *chars
        .get((want & 0xff) as usize)
        .ok_or_else(|| format!("no direction {want} in {:?}", g.world.dirchars))?;
    Ok(if want & RUN != 0 {
        0x80 | c as i32
    } else {
        c as i32
    })
}

/// The keys and characters the engine was answered since the mark.
fn answered(g: &RenethackGame) -> Vec<i32> {
    g.sent
        .iter()
        .filter_map(|(_, r)| match r {
            Reply::Key(k) | Reply::Char(k) => Some(*k),
            _ => None,
        })
        .collect()
}

fn mark(g: &mut RenethackGame) -> Result<(), String> {
    g.sent.clear();
    Ok(())
}

/// The engine got the wanted code once and nothing else, and waits for a
/// command again.
fn once(g: &RenethackGame) -> Result<bool, String> {
    let want = code(g, WANT.load(Ordering::Relaxed))?;
    let got = answered(g);
    let back = matches!(g.pending, Some((_, Prompt::Command))) && !g.world.getpos;
    match got.as_slice() {
        [] => Ok(false),
        [k] if *k == want => Ok(back),
        _ => Err(format!("the engine got {got:?}, not {want} once")),
    }
}

/// A chord (or a lone arrow) at the command prompt: the mark, the keys.
macro_rules! chord {
    ($what:literal, $want:expr, $a:ident, $b:ident) => {
        Step::Call($what, |g| {
            WANT.store($want, Ordering::Relaxed);
            mark(g)?;
            together(GKey::$a, GKey::$b, $want & RUN != 0);
            Ok(())
        })
    };
}

/// #chat from the palette: "Talk to whom? (in what direction)".
fn talk() -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("chat".into())),
        Step::Request("in what direction?", |p| {
            matches!(
                p,
                Prompt::FreeKey {
                    directions: true,
                    ..
                }
            )
        }),
    ]
}

pub(super) fn chords() -> Vec<Step> {
    let mut steps = start_as(modern_choice());
    // each diagonal at the command prompt, once
    steps.extend([
        chord!("↑ and ← together", NW, UP, LEFT),
        Step::Wait("north-west, once", once),
        chord!("→ and ↑ together", NE, RIGHT, UP),
        Step::Wait("north-east, once", once),
        chord!("↓ and → together", SE, DOWN, RIGHT),
        Step::Wait("south-east, once", once),
        chord!("← and ↓ together", SW, LEFT, DOWN),
        Step::Wait("south-west, once", once),
        // a lone arrow: its own step, once
        Step::Call("↑ alone", |g| {
            WANT.store(N, Ordering::Relaxed);
            mark(g)?;
            send(GKey::UP, true, false, false);
            send(GKey::UP, false, false, false);
            Ok(())
        }),
        Step::Wait("north, once", once),
        // with Shift: a run
        chord!("Shift, ↑ and ← together", NW | RUN, UP, LEFT),
        Step::Wait("a run north-west, once", once),
        // held together: an order holds the diagonal
        Step::Call("↓ and → held", |g| {
            WANT.store(SE, Ordering::Relaxed);
            mark(g)?;
            send(GKey::DOWN, true, false, false);
            send(GKey::RIGHT, true, false, false);
            Ok(())
        }),
        Step::Wait("south-east, once", once),
        Step::Call("→ repeats as the keyboard holds it", |_| {
            send(GKey::RIGHT, true, false, true);
            Ok(())
        }),
        Step::Wait("an order holds the diagonal", |g| {
            // (a wall there ends it at once: its step went all the same)
            let want = code(g, SE)?;
            let holding =
                matches!(g.driver.order(), Some(Order::Hold { key }) if *key as i32 == want);
            Ok(holding || answered(g).iter().filter(|&&k| k == want).count() >= 2)
        }),
        Step::Call("let go", |_| {
            send(GKey::RIGHT, false, false, false);
            send(GKey::DOWN, false, false, false);
            Ok(())
        }),
        Step::Wait("the order over: only the diagonal went", |g| {
            let want = code(g, SE)?;
            let got = answered(g);
            if got.iter().any(|&k| k != want) {
                return Err(format!("the engine got {got:?}, not only {want}"));
            }
            Ok(g.driver.order().is_none()
                && matches!(g.pending, Some((_, Prompt::Command)))
                && !got.is_empty())
        }),
        // a direction's question: the chord answers it
    ]);
    steps.extend(talk());
    steps.extend([
        chord!("↑ and → for the direction", NE, UP, RIGHT),
        Step::Wait("north-east answered, once", once),
        // a menu: arrows are its own, nothing answers it
        key('O'),
        Step::Request("the engine's options", |p| matches!(p, Prompt::Menu { .. })),
        Step::Call("↑ and ← in the menu", |g| {
            let (id, _) = g.pending.as_ref().ok_or("no request")?;
            MENU.store(*id as i64, Ordering::Relaxed);
            FRAME.store(
                Engine::singleton().get_process_frames() as i64,
                Ordering::Relaxed,
            );
            mark(g)?;
            together(GKey::UP, GKey::LEFT, false);
            Ok(())
        }),
        Step::Wait("the menu still asks, nothing answered", |g| {
            let frame = Engine::singleton().get_process_frames() as i64;
            if frame < FRAME.load(Ordering::Relaxed) + 10 {
                return Ok(false);
            }
            let same = g.pending.as_ref().is_some_and(|(id, p)| {
                *id as i64 == MENU.load(Ordering::Relaxed) && matches!(p, Prompt::Menu { .. })
            });
            if !same || !g.sent.is_empty() {
                return Err(format!("the menu was answered: {:?}", g.sent));
            }
            Ok(true)
        }),
        Step::Dialog(DialogEvent::MenuCancel),
        Step::Request("a command", command),
        // the north-west step moved onto q
        Step::Call("north-west moved onto q", |g| {
            let mut s = g.settings.clone();
            s.keymaps.get_mut(KeyProfile::Modern).rebind(
                KeyInput::plain(Key::Home),
                KeyInput::plain(Key::Char('q')),
                true,
            );
            g.set_settings(s);
            WANT.store(NW, Ordering::Relaxed);
            mark(g)
        }),
        key('q'),
        Step::Wait("q steps north-west, once", once),
    ]);
    steps.extend(talk());
    steps.extend([
        Step::Call("q for the direction", |g| {
            WANT.store(NW, Ordering::Relaxed);
            mark(g)
        }),
        key('q'),
        Step::Wait("q answers north-west, once", once),
    ]);
    steps.extend(quit());
    steps
}
