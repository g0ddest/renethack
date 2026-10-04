//! The help (localization phase R8): `help` opens it from the HUD's
//! button in English, finds "Elbereth" and shows the chapter it is in;
//! switched to Russian, the panel shows the Russian Guidebook, finds
//! "Амулет Йендора"; a gamepad's ↓ turns to the next chapter, B closes
//! it and the game goes on.

use super::{Step, command, quit, start, tap};
use crate::game::RenethackGame;
use crate::gamepad::PadButton;
use crate::help_panel::HelpInput;
use crate::i18n::Lang;
use crate::ui_events::UiEvent;

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
            let (chapters, shown, _) =
                g.ui.as_ref()
                    .and_then(|ui| ui.help.view())
                    .ok_or("no book")?;
            if chapters.len() < 40 || shown != 0 {
                return Err(format!(
                    "{} chapters, chapter {shown} shown",
                    chapters.len()
                ));
            }
            Ok(true)
        }),
        Step::Shot("help-en"),
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
