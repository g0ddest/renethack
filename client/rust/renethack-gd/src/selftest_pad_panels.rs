//! Every panel with a gamepad alone (`pad-panels`): Start opens the
//! game's menu, → goes to its panels, ↓ to one, A opens it, B closes it
//! and the game is back at its command prompt. A wizard, who knows a
//! spell: the spells, the character, the skills, the dungeon's
//! overview, the journal, the codex, the achievements, the help and
//! every command (the palette). The menu fits the screen.

use nh_world::Prompt;

use super::{Step, command, fail_on_error_screen, screen, start_as, tap};
use crate::game::RenethackGame;
use crate::game_menu::GameMenuPick;
use crate::gamepad::PadButton;
use crate::tr;
use crate::ui_events::CharacterChoice;

fn wizard() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "wizard".into(),
        race: "elf".into(),
        gender: "male".into(),
        align: "chaotic".into(),
        profile: nh_world::KeyProfile::Classic,
    }
}

/// The game's menu up, the focus on `pick`, the whole menu on screen.
fn menu_on(g: &RenethackGame, pick: GameMenuPick) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    if !ui.game_menu.is_open() {
        return Ok(false);
    }
    let canvas = godot::prelude::Rect2::new(godot::prelude::Vector2::ZERO, g.canvas_size());
    let r = ui.game_menu.rect().ok_or("no menu")?;
    if !canvas.grow(1.0).encloses(r) {
        return Err(format!("the menu {r:?} leaves the screen {canvas:?}"));
    }
    Ok(ui.game_menu.focused() == pick)
}

/// The panel open: a dialog whose words begin with `title`.
fn panel(g: &RenethackGame, title: &str) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(ui.dialogs.is_open()
        && ui
            .dialogs
            .panel_texts()
            .iter()
            .any(|t| t.starts_with(title)))
}

/// Back in the game: nothing over it, the command prompt.
fn back(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let ui = g.ui.as_ref().ok_or("no UI")?;
    Ok(!ui.game_menu.is_open()
        && !ui.dialogs.is_open()
        && !ui.help.is_open()
        && screen(g).is_none()
        && matches!(g.pending, Some((_, Prompt::Command))))
}

/// Start, →, ↓ `down` times: the focus on `pick`; A takes it.
fn open(pick: GameMenuPick, down: usize, steps: &mut Vec<Step>) {
    steps.push(tap(PadButton::Start));
    steps.push(Step::Wait("the game's menu", |g| {
        menu_on(g, GameMenuPick::Resume)
    }));
    steps.push(tap(PadButton::Right));
    for _ in 0..down {
        steps.push(tap(PadButton::Down));
    }
    steps.push(match pick {
        GameMenuPick::Spells => Step::Wait("on the spells", |g| menu_on(g, GameMenuPick::Spells)),
        GameMenuPick::Character => {
            Step::Wait("on the character", |g| menu_on(g, GameMenuPick::Character))
        }
        GameMenuPick::Skills => Step::Wait("on the skills", |g| menu_on(g, GameMenuPick::Skills)),
        GameMenuPick::Overview => {
            Step::Wait("on the overview", |g| menu_on(g, GameMenuPick::Overview))
        }
        GameMenuPick::Journal => {
            Step::Wait("on the journal", |g| menu_on(g, GameMenuPick::Journal))
        }
        GameMenuPick::Codex => Step::Wait("on the codex", |g| menu_on(g, GameMenuPick::Codex)),
        GameMenuPick::Achievements => Step::Wait("on the achievements", |g| {
            menu_on(g, GameMenuPick::Achievements)
        }),
        GameMenuPick::Help => Step::Wait("on the help", |g| menu_on(g, GameMenuPick::Help)),
        _ => Step::Wait("on every command", |g| menu_on(g, GameMenuPick::Commands)),
    });
    steps.push(tap(PadButton::A));
}

pub(super) fn pad_panels() -> Vec<Step> {
    let mut steps = start_as(wizard());
    steps.push(tap(PadButton::Start));
    steps.push(Step::Wait("Start: the game's menu, on going back", |g| {
        menu_on(g, GameMenuPick::Resume)
    }));
    steps.push(Step::Shot("pad-panels-menu"));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("B: the game", back));
    open(GameMenuPick::Spells, 0, &mut steps);
    steps.push(Step::Wait("the spells", |g| panel(g, &tr!("spells-title"))));
    steps.push(Step::Shot("pad-panels-spells"));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the spells closed", back));
    open(GameMenuPick::Character, 1, &mut steps);
    steps.push(Step::Wait("the character", |g| {
        panel(g, &tr!("sheet-title"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the character closed", back));
    open(GameMenuPick::Skills, 2, &mut steps);
    steps.push(Step::Wait("the skills", |g| panel(g, &tr!("skills-title"))));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the skills closed", back));
    open(GameMenuPick::Overview, 3, &mut steps);
    steps.push(Step::Wait("the overview", |g| {
        Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("overview"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the overview closed", back));
    open(GameMenuPick::Journal, 4, &mut steps);
    steps.push(Step::Wait("the journal", |g| {
        Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("journal"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the journal closed", back));
    open(GameMenuPick::Codex, 5, &mut steps);
    steps.push(Step::Wait("the codex", |g| {
        Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.layout_name() == Some("codex"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the codex closed", back));
    open(GameMenuPick::Achievements, 6, &mut steps);
    steps.push(Step::Wait("the achievements", |g| {
        Ok(screen(g) == Some("achievements"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the achievements closed", back));
    open(GameMenuPick::Help, 7, &mut steps);
    steps.push(Step::Wait("the help", |g| {
        Ok(g.ui.as_ref().ok_or("no UI")?.help.is_open())
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Wait("the help closed", back));
    open(GameMenuPick::Commands, 8, &mut steps);
    steps.push(Step::Request("every command", |p| *p == Prompt::ExtCmd));
    steps.push(Step::Wait("the palette", |g| {
        Ok(g.ui.as_ref().ok_or("no UI")?.dialogs.kind_name() == Some("extcmd"))
    }));
    steps.push(tap(PadButton::B));
    steps.push(Step::Request("a command after the palette", command));
    steps.push(Step::Wait("the palette closed", back));
    steps
}
