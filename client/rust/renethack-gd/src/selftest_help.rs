//! The help (localization phase R8): `help` opens it from the HUD's
//! button in English, on the game's own guide ("Your quest", first under
//! the Guide); every title in its list reads whole; the Guidebook's
//! chapters on the screen are marked as its terminal version's, and say so
//! first. It finds "Elbereth" and shows the chapter it is in; switched to
//! Russian, the panel shows the Russian Guidebook, its titles whole, finds
//! "Амулет Йендора"; a gamepad's ↓ turns to the next chapter, B closes it
//! and the game goes on.

use super::{Step, command, quit, start, tap};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::help_panel::HelpInput;
use crate::i18n::Lang;
use crate::tr;
use crate::ui_events::UiEvent;

/// Every title in the help's list reads whole.
fn titles_whole(g: &RenethackGame) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if !ui.help.list_laid_out() {
        return Ok(false);
    }
    let cut = ui.help.cut_rows();
    if cut.is_empty() {
        Ok(true)
    } else {
        Err(format!("titles cut in the help's list: {cut:?}"))
    }
}

/// The help is open on "Your quest", first under the Guide.
fn on_the_guide(g: &RenethackGame) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((titles, shown, _)) = ui.help.view() else {
        return Ok(false);
    };
    if titles.first() != Some(&tr!("help-guide"))
        || titles.get(1) != Some(&tr!("guide-quest-title"))
    {
        return Err(format!(
            "the list starts {:?}",
            &titles[..titles.len().min(3)]
        ));
    }
    if shown != 1 {
        return Err(format!("chapter {shown} shown, not \"Your quest\""));
    }
    Ok(ui.help.shown_text().contains(&tr!("guide-quest")))
}

/// The row of the Guidebook's chapter on the status lines.
fn status_lines_row(g: &RenethackGame) -> Result<usize, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    ui.help
        .row_texts()
        .iter()
        .position(|t| t.starts_with("3.1 "))
        .ok_or_else(|| "no chapter 3.1 in the list".to_string())
}

static ROW: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The help is open on the book in `lang` ("en", "ru").
fn open_in(g: &RenethackGame, lang: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(ui.help.is_open() && ui.help.book_lang().as_deref() == Some(lang))
}

/// A search found something, and the chapter shown has `words`.
fn found(g: &RenethackGame, words: &str) -> Result<bool, String> {
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let Some((_, _, hits)) = ui.help.view() else {
        return Ok(false);
    };
    Ok(hits.is_some_and(|n| n > 0) && ui.help.shown_text().contains(words))
}

pub(super) fn help() -> Vec<Step> {
    let mut steps = start();
    steps.extend([
        Step::Push(UiEvent::SetLanguage(Lang::En)),
        Step::Push(UiEvent::ToggleHelp),
        Step::Wait("the help open on the English Guidebook", |g| {
            if !open_in(g, "en")? {
                return Ok(false);
            }
            let (chapters, _, _) =
                g.ui.as_ref()
                    .and_then(|ui| ui.help.view())
                    .ok_or("no book")?;
            if chapters.len() < 40 {
                return Err(format!("{} chapters", chapters.len()));
            }
            Ok(true)
        }),
        Step::Wait("the help open on the guide", on_the_guide),
        Step::Wait("every title whole", titles_whole),
        Step::Shot("help-en"),
        // the chapters on the screen: the terminal version's
        Step::Call("the chapter on the status lines picked", |g| {
            let row = status_lines_row(g)?;
            let tag = tr!("help-terminal-tag");
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let text = &ui.help.row_texts()[row];
            if !text.ends_with(&tag) {
                return Err(format!("{text:?} is not marked {tag:?}"));
            }
            ROW.store(row, std::sync::atomic::Ordering::Relaxed);
            g.push_ui(UiEvent::Help(HelpInput::Row(row)));
            Ok(())
        }),
        Step::Wait("the terminal version's screen said first", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let (_, shown, _) = ui.help.view().ok_or("no book")?;
            if shown != ROW.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(false);
            }
            let note = tr!("help-terminal-note", page = tr!("guide-quest-title"));
            let text = ui.help.shown_text();
            let first = text.lines().filter(|l| !l.trim().is_empty()).nth(1);
            if first != Some(note.as_str()) {
                return Err(format!("the chapter's first words: {first:?}"));
            }
            Ok(true)
        }),
        Step::Shot("help-terminal"),
        Step::Call("\"Elbereth\" searched for", |g| {
            g.ui.as_mut().ok_or("no UI")?.help.type_search("Elbereth");
            Ok(())
        }),
        Step::Wait("finds of Elbereth", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .help
                .view()
                .is_some_and(|(_, _, hits)| hits.is_some_and(|n| n > 0)))
        }),
        Step::Push(UiEvent::Help(HelpInput::Row(0))),
        Step::Wait("the chapter with Elbereth", |g| found(g, "Elbereth")),
        Step::Shot("help-search"),
        // the language switched with the help open: the Russian book
        Step::Push(UiEvent::SetLanguage(Lang::Ru)),
        Step::Wait("the help open on the Russian Guidebook", |g| {
            open_in(g, "ru")
        }),
        Step::Wait("the help open on the guide", on_the_guide),
        Step::Wait("every Russian title whole", titles_whole),
        Step::Shot("help-ru"),
        Step::Call("\"Амулет Йендора\" searched for", |g| {
            g.ui.as_mut()
                .ok_or("no UI")?
                .help
                .type_search("амулет йендора");
            Ok(())
        }),
        Step::Wait("finds of the Amulet", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui
                .help
                .view()
                .is_some_and(|(_, _, hits)| hits.is_some_and(|n| n > 0)))
        }),
        Step::Push(UiEvent::Help(HelpInput::Row(0))),
        Step::Wait("the chapter with the Amulet", |g| {
            found(g, "Амулет Йендора")
        }),
        Step::Shot("help-ru-search"),
        // a gamepad: the next row, then out
        tap(PadButton::Down),
        Step::Wait("the next find's chapter", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(ui.help.view().is_some_and(|(_, shown, _)| shown > 0))
        }),
        Step::Shot("help-pad"),
        tap(PadButton::B),
        Step::Wait("the help closed", |g| {
            Ok(!g.ui.as_ref().ok_or("no UI")?.help.is_open())
        }),
        Step::Push(UiEvent::SetLanguage(Lang::En)),
        Step::Wait("the game waits for a command again", |g| {
            Ok(g.pending.as_ref().is_some_and(|(_, p)| command(p)))
        }),
    ]);
    steps.extend(quit());
    steps
}
