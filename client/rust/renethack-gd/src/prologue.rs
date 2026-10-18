//! The prologue: NetHack's own legacy text ("It is written in the Book of
//! ..."), word for word as the engine wrote it for this hero and in the
//! player's language, told in six beats before the first game, each over a
//! scene of its own (`prologue_stage`): the Book of the hero's god; Moloch's
//! rebellion against Marduk; the Amulet hidden in Gehennom; the hero's god
//! seeking it; the hero, newly trained; and go bravely, down the stairs into
//! the dungeon. Here: the text cut into the beats, and how long each is read.

/// The beats, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Beat {
    /// "It is written in the Book of <god>:".
    Book,
    /// Moloch rebels against Marduk the Creator.
    Rebellion,
    /// Moloch steals the Amulet of Yendor and hides it in Gehennom.
    Amulet,
    /// The hero's god seeks the Amulet.
    God,
    /// The hero, newly trained, the instrument of their god.
    Hero,
    /// "Go bravely with <god>!": down into the dungeon.
    Descent,
}

impl Beat {
    pub const ALL: [Beat; 6] = [
        Beat::Book,
        Beat::Rebellion,
        Beat::Amulet,
        Beat::God,
        Beat::Hero,
        Beat::Descent,
    ];

    /// Its place among them.
    pub fn index(self) -> usize {
        Beat::ALL.iter().position(|&b| b == self).unwrap_or(0)
    }

    /// The beat after it, None after the last.
    pub fn next(self) -> Option<Beat> {
        Beat::ALL.get(self.index() + 1).copied()
    }

    /// Its name in self-tests' pictures.
    pub fn name(self) -> &'static str {
        match self {
            Beat::Book => "book",
            Beat::Rebellion => "rebellion",
            Beat::Amulet => "amulet",
            Beat::God => "god",
            Beat::Hero => "hero",
            Beat::Descent => "descent",
        }
    }
}

/// The engine's text window is the legacy text (the English, whatever the
/// player reads it in): its first line opens the Book, its words tell of
/// Moloch, Marduk and the Amulet.
pub fn is_legacy(lines: &[&str]) -> bool {
    let first = lines.iter().find(|l| !l.trim().is_empty());
    let whole = lines.join(" ");
    first.is_some_and(|l| l.trim_start().starts_with("It is written in the Book of "))
        && whole.contains("Moloch")
        && whole.contains("Marduk")
        && whole.contains("Amulet of Yendor")
}

/// The legacy text (in any language, its lines as the engine laid them
/// out) cut into the beats' texts, each one paragraph of words: NetHack's
/// four paragraphs, the Book's quotation parted after its first sentence
/// (the rebellion, then the theft) and the hero's paragraph before its last
/// (the call to go bravely). None when it is not laid out so.
pub fn beats(lines: &[String]) -> Option<[String; 6]> {
    let mut paragraphs: Vec<Vec<&str>> = Vec::new();
    let mut now: Vec<&str> = Vec::new();
    for line in lines.iter().flat_map(|l| l.split('\n')) {
        if line.trim().is_empty() {
            if !now.is_empty() {
                paragraphs.push(std::mem::take(&mut now));
            }
        } else {
            now.push(line.trim());
        }
    }
    if !now.is_empty() {
        paragraphs.push(now);
    }
    let [book, quote, god, hero] = paragraphs.as_slice() else {
        return None;
    };
    let words = |lines: &[&str]| {
        lines
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (rebellion, amulet) = cut_after_first_sentence(&words(quote))?;
    let (you, go) = cut_before_last_sentence(&words(hero))?;
    Some([words(book), rebellion, amulet, words(god), you, go])
}

/// A text cut after its first sentence: (that sentence, the rest).
fn cut_after_first_sentence(text: &str) -> Option<(String, String)> {
    let at = text.find(". ")? + 1;
    let (a, b) = text.split_at(at);
    Some((a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
}

/// A text cut before its last sentence: (all before it, that sentence).
fn cut_before_last_sentence(text: &str) -> Option<(String, String)> {
    let at = text.rfind(". ")? + 1;
    let (a, b) = text.split_at(at);
    Some((a.trim().to_string(), b.trim().to_string()))
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
}

/// Seconds a beat of `text` is read for: a moment to see its scene, then
/// its words at an unhurried reader's pace.
pub fn reading_secs(text: &str) -> f64 {
    READ_FIRST + text.chars().count() as f64 / READ_CHARS
}

/// What every beat is given before its words, and the characters read a
/// second after (the whole prologue about forty to sixty seconds).
const READ_FIRST: f64 = 2.0;
const READ_CHARS: f64 = 18.0;

/// A beat's scene comes up out of black over this long, and goes back
/// into it over this long; its words begin this far in and come at this
/// many characters a second (the reader keeps up, and is ahead).
pub const FADE_IN: f64 = 0.9;
pub const FADE_OUT: f64 = 0.8;
const WORDS_AT: f64 = 0.6;
const WORDS_CHARS: f64 = 34.0;
/// After the last words the hero goes down the stairs for this long, and
/// the game comes up out of black over this long.
pub const DESCENT_SECS: f64 = 3.0;
pub const GAME_FADE: f64 = 1.0;
/// A skip goes to black this quickly.
const SKIP_FADE: f64 = 0.5;
/// Seconds into a beat when the next one's stage is built ahead: its
/// fade-in over, its words coming.
pub const BUILD_NEXT_AT: f64 = 1.2;

/// What happened in a frame of the prologue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A beat begins (in black): its scene is to be laid.
    Beat(Beat),
    /// It is over, in black: the game (or the title) comes up out of it.
    Done,
}

/// The prologue playing: its beats' texts, the beat now and how far into
/// it, and whether it is leaving (to the next beat, or for good).
#[derive(Debug, Clone)]
pub struct Run {
    texts: [String; 6],
    beat: Beat,
    t: f64,
    /// A key showed the beat's words at once.
    hurried: bool,
    /// Going to black since this time into the beat, and for good (a skip,
    /// or after the last beat) or on to the next.
    leaving: Option<(f64, bool)>,
    started: bool,
    done: bool,
}

impl Run {
    pub fn new(texts: [String; 6]) -> Run {
        Run {
            texts,
            beat: Beat::Book,
            t: 0.0,
            hurried: false,
            leaving: None,
            started: false,
            done: false,
        }
    }

    pub fn beat(&self) -> Beat {
        self.beat
    }

    /// Seconds into the beat.
    pub fn secs(&self) -> f64 {
        self.t
    }

    /// The beat's words.
    pub fn text(&self) -> &str {
        &self.texts[self.beat.index()]
    }

    pub fn texts(&self) -> &[String; 6] {
        &self.texts
    }

    /// The beats' texts said again (agreeing with a hero now known to be
    /// a woman, or a man), the beat and its time as they were.
    pub fn set_texts(&mut self, texts: [String; 6]) {
        self.texts = texts;
    }

    #[cfg(test)]
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// How many of the beat's characters show now.
    pub fn shown_chars(&self) -> usize {
        let all = self.text().chars().count();
        if self.hurried {
            return all;
        }
        let n = ((self.t - WORDS_AT) * WORDS_CHARS).max(0.0) as usize;
        n.min(all)
    }

    /// All the beat's words show.
    pub fn all_shown(&self) -> bool {
        self.shown_chars() >= self.text().chars().count()
    }

    /// How long the beat is, from black to black: read, then (the last)
    /// the descent.
    fn length(&self) -> f64 {
        let read = reading_secs(self.text())
            .max(WORDS_AT + self.text().chars().count() as f64 / WORDS_CHARS + 1.0);
        match self.beat {
            Beat::Descent => read + DESCENT_SECS,
            _ => read,
        }
    }

    /// The black over everything: 1 in black, 0 clear.
    pub fn shade(&self) -> f32 {
        let fade_in = 1.0 - (self.t / FADE_IN).clamp(0.0, 1.0);
        let fade_out = match self.leaving {
            Some((since, all)) => {
                let secs = if all && self.beat != Beat::Descent {
                    SKIP_FADE
                } else {
                    FADE_OUT
                };
                ((self.t - since) / secs).clamp(0.0, 1.0)
            }
            None => 0.0,
        };
        fade_in.max(fade_out) as f32
    }

    /// The words fade with the scene as it leaves; they never outlast it.
    pub fn words_alpha(&self) -> f32 {
        1.0 - self.shade().min(1.0)
    }

    /// Seconds into the descent (the last beat, after its words), None
    /// before it.
    pub fn descent(&self) -> Option<f64> {
        if self.beat != Beat::Descent {
            return None;
        }
        let start = self.length() - DESCENT_SECS;
        (self.t >= start).then_some(self.t - start)
    }

    /// A key, a button or a click: the beat's words at once if they are
    /// still coming, else on to the next beat.
    pub fn advance(&mut self) {
        if self.leaving.is_some() || !self.started {
            return;
        }
        if !self.all_shown() {
            self.hurried = true;
        } else {
            self.leaving = Some((self.t, false));
        }
    }

    /// Esc or B: the rest of it skipped, to black and out.
    pub fn skip(&mut self) {
        if self.done {
            return;
        }
        match self.leaving {
            Some((_, true)) => {}
            _ => self.leaving = Some((self.t, true)),
        }
    }

    /// The prologue `delta` seconds on: what began or ended.
    pub fn tick(&mut self, delta: f64) -> Option<Event> {
        if self.done {
            return None;
        }
        if !self.started {
            self.started = true;
            return Some(Event::Beat(self.beat));
        }
        self.t += delta;
        if self.leaving.is_none() && self.t >= self.length() {
            self.leaving = Some((self.t, self.beat == Beat::Descent));
        }
        let (since, all) = self.leaving?;
        let secs = if all && self.beat != Beat::Descent {
            SKIP_FADE
        } else {
            FADE_OUT
        };
        if self.t - since < secs {
            return None;
        }
        match self.beat.next().filter(|_| !all) {
            Some(next) => {
                self.beat = next;
                self.t = 0.0;
                self.hurried = false;
                self.leaving = None;
                Some(Event::Beat(next))
            }
            None => {
                self.done = true;
                Some(Event::Done)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The legacy text as NetHack 5.0 writes it for a Valkyrie of Tyr.
    pub(crate) fn valkyrie() -> Vec<String> {
        [
            "It is written in the Book of Tyr:",
            "",
            "    After the Creation, the cruel god Moloch rebelled",
            "    against the authority of Marduk the Creator.",
            "    Moloch stole from Marduk the most powerful of all",
            "    the artifacts of the gods, the Amulet of Yendor,",
            "    and he hid it in the dark cavities of Gehennom, the",
            "    Under World, where he now lurks, and bides his time.",
            "",
            "Your god Tyr seeks to possess the Amulet, and with it",
            "to gain deserved ascendance over the other gods.",
            "",
            "You, a newly trained Stripling, have been heralded",
            "from birth as the instrument of Tyr.  You are destined",
            "to recover the Amulet for your deity, or die in the",
            "attempt.  Your hour of destiny has come.  For the sake",
            "of us all:  Go bravely with Tyr!",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn the_legacy_text_is_told_apart() {
        let lines = valkyrie();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert!(is_legacy(&refs));
        assert!(!is_legacy(&["It is written in the Book of Tyr:"]));
        assert!(!is_legacy(&["Things that are here:", "a spear"]));
    }

    #[test]
    fn the_text_falls_into_six_beats_word_for_word() {
        let [book, rebellion, amulet, god, hero, go] = beats(&valkyrie()).expect("beats");
        assert_eq!(book, "It is written in the Book of Tyr:");
        assert_eq!(
            rebellion,
            "After the Creation, the cruel god Moloch rebelled against the authority of Marduk the Creator."
        );
        assert!(amulet.starts_with("Moloch stole from Marduk"), "{amulet}");
        assert!(
            amulet.ends_with("where he now lurks, and bides his time."),
            "{amulet}"
        );
        assert!(god.starts_with("Your god Tyr seeks"), "{god}");
        assert!(
            hero.starts_with("You, a newly trained Stripling,"),
            "{hero}"
        );
        assert!(hero.ends_with("Your hour of destiny has come."), "{hero}");
        assert_eq!(go, "For the sake of us all: Go bravely with Tyr!");
        // nothing lost or added but the lines' breaks and indents
        let all = [book, rebellion, amulet, god, hero, go].join(" ");
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(all, words(&valkyrie().join(" ")));
    }

    #[test]
    fn the_russian_text_falls_into_the_same_beats() {
        let ru: Vec<String> = "Написано в Книге Тира:\n\n    После Сотворения жестокий бог Молох восстал\n    против власти Мардука-Творца.\n    Молох похитил у Мардука могущественнейший\n    из артефактов богов — Амулет Йендора —\n    и спрятал его в тёмных пустотах Геенны,\n    Подземного мира, где ныне таится, выжидая своего часа.\n\nВаш бог Тир стремится завладеть Амулетом, а с ним\nобрести заслуженное превосходство над прочими богами.\n\nВы, только что обученная Юница, с самого рождения\nбыли предназначены стать орудием Тира.  Вам суждено\nвернуть Амулет своему божеству или погибнуть, пытаясь.\nПробил час вашей судьбы.  Ради всех нас: ступайте смело, и да пребудет с вами Тир!"
            .split('\n')
            .map(str::to_string)
            .collect();
        let [book, rebellion, amulet, god, hero, go] = beats(&ru).expect("beats");
        assert_eq!(book, "Написано в Книге Тира:");
        assert!(
            rebellion.ends_with("против власти Мардука-Творца."),
            "{rebellion}"
        );
        assert!(amulet.starts_with("Молох похитил"), "{amulet}");
        assert!(god.starts_with("Ваш бог Тир"), "{god}");
        assert!(hero.ends_with("Пробил час вашей судьбы."), "{hero}");
        assert_eq!(
            go,
            "Ради всех нас: ступайте смело, и да пребудет с вами Тир!"
        );
    }

    #[test]
    fn the_game_s_own_russian_of_it_falls_into_the_beats() {
        use crate::i18n::{EngineText, Lang};
        let t = crate::engine_text::EngineTranslator::new();
        let english = valkyrie();
        let refs: Vec<&str> = english.iter().map(String::as_str).collect();
        // (the translator loads in the background)
        let start = std::time::Instant::now();
        let ru = loop {
            if let Some(ru) = t.window(Lang::Ru, &refs) {
                break ru;
            }
            assert!(
                start.elapsed().as_secs() < 30,
                "the translator never loaded"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let beats = beats(&ru).expect("the Russian in beats");
        assert!(beats[0].starts_with("Написано в Книге"), "{beats:?}");
        assert!(beats[5].contains("ступайте смело"), "{beats:?}");
    }

    /// The run ticked on at a game's frame rate until `until` holds, or
    /// for a minute and a half at most; the events on the way.
    fn run_until(run: &mut Run, until: impl Fn(&Run) -> bool) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..(90 * 60) {
            if until(run) {
                break;
            }
            events.extend(run.tick(1.0 / 60.0));
        }
        events
    }

    #[test]
    fn the_beats_play_in_turn_each_out_of_black_and_into_it() {
        let mut run = Run::new(beats(&valkyrie()).unwrap());
        assert_eq!(run.tick(0.0), Some(Event::Beat(Beat::Book)));
        assert_eq!(run.shade(), 1.0, "it begins in black");
        run_until(&mut run, |r| r.secs() > FADE_IN);
        assert_eq!(run.shade(), 0.0);
        // the words come after the scene, all of them in time
        run_until(&mut run, Run::all_shown);
        assert!(run.secs() < reading_secs(run.text()));
        let events = run_until(&mut run, Run::is_done);
        let begun: Vec<Event> = events
            .into_iter()
            .filter(|e| matches!(e, Event::Beat(_)))
            .collect();
        let rest: Vec<Event> = Beat::ALL[1..].iter().map(|&b| Event::Beat(b)).collect();
        assert_eq!(begun, rest);
        assert!(run.is_done() && run.shade() == 1.0);
    }

    #[test]
    fn a_key_shows_the_words_then_goes_on() {
        let mut run = Run::new(beats(&valkyrie()).unwrap());
        run.tick(0.0);
        run.tick(1.0);
        assert!(!run.all_shown());
        run.advance();
        assert!(run.all_shown(), "the words at once");
        assert_eq!(run.beat(), Beat::Book);
        run.advance();
        let events = run_until(&mut run, |r| r.beat() != Beat::Book);
        assert_eq!(events, vec![Event::Beat(Beat::Rebellion)]);
        assert!(run.secs() < 0.1, "the next beat from its start");
    }

    #[test]
    fn a_skip_goes_out_at_once_whatever_the_beat() {
        let mut run = Run::new(beats(&valkyrie()).unwrap());
        run.tick(0.0);
        run.tick(2.0);
        run.skip();
        let events = run_until(&mut run, Run::is_done);
        assert_eq!(events, vec![Event::Done]);
        assert!(run.secs() < 2.0 + SKIP_FADE + 0.1);
    }

    #[test]
    fn the_last_beat_goes_down_the_stairs_after_its_words() {
        let mut run = Run::new(beats(&valkyrie()).unwrap());
        run_until(&mut run, |r| r.beat() == Beat::Descent);
        assert_eq!(run.descent(), None);
        run_until(&mut run, |r| r.descent().is_some());
        assert!(run.all_shown());
        let events = run_until(&mut run, Run::is_done);
        assert_eq!(events, vec![Event::Done]);
    }

    #[test]
    fn a_text_laid_out_otherwise_has_no_beats() {
        assert!(beats(&["It is written in the Book of Tyr:".to_string()]).is_none());
    }

    #[test]
    fn the_whole_prologue_takes_about_a_minute() {
        let secs: f64 = beats(&valkyrie())
            .expect("beats")
            .iter()
            .map(|t| reading_secs(t))
            .sum();
        assert!((40.0..=62.0).contains(&secs), "{secs}");
    }
}
