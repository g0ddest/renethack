//! Two arrows pressed together: a diagonal step. A laptop has no keypad,
//! and its Home, PgUp, End and PgDn are Fn and an arrow, out of the
//! compass's order; so ↑ and ← together step north-west, ↑ and → north-
//! east, ↓ and ← south-west, ↓ and → south-east. An arrow's press waits a
//! moment (`CHORD_MS`) for the arrow across it: a lone arrow steps when it
//! is let go or when the moment is over, never once alone and again in the
//! diagonal. Held together they repeat the diagonal; Shift runs it. Only
//! where a direction is awaited (the caller says so): in menus, texts and
//! panels an arrow is an arrow.

use crate::{Key, KeyInput, Mods};

/// How long an arrow waits for the one across it, in ms: under a key's
/// own press, so a lone step still feels at once.
pub const CHORD_MS: u64 = 70;

/// What the keys come to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordOut {
    /// A key pressed, as it was (an arrow perhaps a moment later).
    Press(KeyInput),
    /// A key let go.
    Release(KeyInput),
    /// Two arrows' diagonal: its step's key (Home north-west, PgUp
    /// north-east, End south-west, PgDn south-east) with their Shift. The
    /// game's own key: no key the player moved stands for it.
    Diagonal(KeyInput),
}

/// The arrows' state between key events.
#[derive(Debug, Clone, Default)]
pub struct Chords {
    /// An arrow held back for the one across it: its press, and when.
    waiting: Option<(KeyInput, u64)>,
    /// Two arrows held together, and their diagonal's key.
    held: Option<(Key, Key, Key)>,
    /// The arrow of a chord still down when the other was let go: no step
    /// of its own until it is pressed anew.
    spent: Option<Key>,
}

fn arrow(k: Key) -> bool {
    matches!(k, Key::Up | Key::Down | Key::Left | Key::Right)
}

/// The step of two arrows across each other (None: they are not).
fn diagonal(a: Key, b: Key) -> Option<Key> {
    let (v, h) = match (a, b) {
        (Key::Up | Key::Down, Key::Left | Key::Right) => (a, b),
        (Key::Left | Key::Right, Key::Up | Key::Down) => (b, a),
        _ => return None,
    };
    Some(match (v, h) {
        (Key::Up, Key::Left) => Key::Home,
        (Key::Up, _) => Key::PageUp,
        (_, Key::Left) => Key::End,
        _ => Key::PageDown,
    })
}

impl Chords {
    pub fn new() -> Chords {
        Chords::default()
    }

    /// A key pressed at `now` (ms); `steps`: a direction is awaited.
    pub fn press(&mut self, k: KeyInput, now: u64, steps: bool) -> Vec<ChordOut> {
        let mut out = Vec::new();
        if !arrow(k.key) || !steps {
            // another key, or arrows that are arrows: the waiting one first
            out.extend(self.flush());
            out.push(ChordOut::Press(k));
            return out;
        }
        if k.echo {
            // held down: the chord's diagonal again; the arrow left of a
            // chord does not step alone
            if let Some((a, b, d)) = self.held
                && (k.key == a || k.key == b)
            {
                out.push(ChordOut::Diagonal(KeyInput { key: d, ..k }));
            } else if self.spent != Some(k.key) {
                out.extend(self.flush());
                out.push(ChordOut::Press(k));
            }
            return out;
        }
        if self.spent == Some(k.key) {
            self.spent = None;
        }
        if let Some((w, _)) = self.waiting {
            if let Some(d) = diagonal(w.key, k.key) {
                self.waiting = None;
                self.held = Some((w.key, k.key, d));
                let mods = Mods {
                    shift: w.mods.shift || k.mods.shift,
                    ..k.mods
                };
                out.push(ChordOut::Diagonal(KeyInput {
                    key: d,
                    mods,
                    echo: false,
                }));
                return out;
            }
            out.extend(self.flush());
        }
        self.waiting = Some((k, now));
        out
    }

    /// A key let go.
    pub fn release(&mut self, k: KeyInput) -> Vec<ChordOut> {
        if let Some((w, _)) = self.waiting
            && w.key == k.key
        {
            // tapped: its step now
            self.waiting = None;
            return vec![ChordOut::Press(w), ChordOut::Release(k)];
        }
        if let Some((a, b, d)) = self.held
            && (k.key == a || k.key == b)
        {
            // the diagonal held ends with either arrow
            self.held = None;
            self.spent = Some(if k.key == a { b } else { a });
            return vec![ChordOut::Release(KeyInput { key: d, ..k })];
        }
        if self.spent == Some(k.key) {
            // the game never had it down
            self.spent = None;
            return Vec::new();
        }
        vec![ChordOut::Release(k)]
    }

    /// Time passes: an arrow that waited its moment steps alone.
    pub fn tick(&mut self, now: u64) -> Vec<ChordOut> {
        match self.waiting {
            Some((_, at)) if now.saturating_sub(at) >= CHORD_MS => {
                self.flush().into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    /// The arrow waiting, pressed now.
    fn flush(&mut self) -> Option<ChordOut> {
        self.waiting.take().map(|(w, _)| ChordOut::Press(w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key) -> KeyInput {
        KeyInput::plain(k)
    }

    fn echo(k: Key) -> KeyInput {
        KeyInput {
            echo: true,
            ..key(k)
        }
    }

    fn shift(k: Key) -> KeyInput {
        KeyInput {
            mods: Mods {
                shift: true,
                ..Mods::default()
            },
            ..key(k)
        }
    }

    #[test]
    fn two_arrows_across_each_other_are_their_diagonal_once() {
        for (a, b, d) in [
            (Key::Up, Key::Left, Key::Home),
            (Key::Left, Key::Up, Key::Home),
            (Key::Up, Key::Right, Key::PageUp),
            (Key::Down, Key::Left, Key::End),
            (Key::Right, Key::Down, Key::PageDown),
        ] {
            let mut c = Chords::new();
            assert!(c.press(key(a), 0, true).is_empty(), "{a:?} waits");
            assert_eq!(c.press(key(b), 30, true), vec![ChordOut::Diagonal(key(d))]);
            // let go: the diagonal ends, nothing of either arrow alone
            assert_eq!(c.release(key(b)), vec![ChordOut::Release(key(d))]);
            assert!(c.release(key(a)).is_empty());
            assert!(c.tick(1000).is_empty());
        }
    }

    #[test]
    fn a_lone_arrow_steps_when_let_go_or_after_its_moment() {
        let mut c = Chords::new();
        c.press(key(Key::Up), 0, true);
        assert_eq!(
            c.release(key(Key::Up)),
            vec![
                ChordOut::Press(key(Key::Up)),
                ChordOut::Release(key(Key::Up))
            ]
        );
        c.press(key(Key::Left), 100, true);
        assert!(c.tick(100 + CHORD_MS - 1).is_empty());
        assert_eq!(
            c.tick(100 + CHORD_MS),
            vec![ChordOut::Press(key(Key::Left))]
        );
        // once it stepped, the other arrow is a step of its own
        assert!(c.press(key(Key::Up), 200, true).is_empty());
        assert_eq!(c.tick(300), vec![ChordOut::Press(key(Key::Up))]);
        // two arrows along one line are two steps
        c.press(key(Key::Up), 400, true);
        assert_eq!(
            c.press(key(Key::Down), 410, true),
            vec![ChordOut::Press(key(Key::Up))]
        );
        assert_eq!(c.tick(500), vec![ChordOut::Press(key(Key::Down))]);
    }

    #[test]
    fn held_together_the_diagonal_repeats_and_shift_runs_it() {
        let mut c = Chords::new();
        c.press(shift(Key::Down), 0, true);
        assert_eq!(
            c.press(shift(Key::Right), 10, true),
            vec![ChordOut::Diagonal(shift(Key::PageDown))]
        );
        let held = KeyInput {
            echo: true,
            ..shift(Key::Right)
        };
        assert_eq!(
            c.press(held, 500, true),
            vec![ChordOut::Diagonal(KeyInput {
                echo: true,
                ..shift(Key::PageDown)
            })]
        );
        // one let go: the other's repeats are no steps of their own
        assert_eq!(
            c.release(shift(Key::Right)),
            vec![ChordOut::Release(shift(Key::PageDown))]
        );
        assert!(c.press(echo(Key::Down), 600, true).is_empty());
        assert!(c.release(key(Key::Down)).is_empty());
    }

    #[test]
    fn where_no_direction_is_awaited_an_arrow_is_an_arrow() {
        let mut c = Chords::new();
        assert_eq!(
            c.press(key(Key::Up), 0, false),
            vec![ChordOut::Press(key(Key::Up))]
        );
        assert_eq!(
            c.press(key(Key::Left), 5, false),
            vec![ChordOut::Press(key(Key::Left))]
        );
        assert_eq!(
            c.release(key(Key::Up)),
            vec![ChordOut::Release(key(Key::Up))]
        );
        // another key goes after the arrow that waited
        c.press(key(Key::Up), 10, true);
        assert_eq!(
            c.press(key(Key::Char('s')), 20, true),
            vec![
                ChordOut::Press(key(Key::Up)),
                ChordOut::Press(key(Key::Char('s')))
            ]
        );
    }
}
