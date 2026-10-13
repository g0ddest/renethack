//! renethack's Godot client: a GDExtension whose root node,
//! `RenethackGame`, runs nh-engine and draws the game.

mod achievement_bake;
mod achievement_view;
mod action_bar;
mod animator;
mod art;
mod audio;
mod batch;
mod bearing;
mod bindings;
mod branch_look;
mod credits;
mod credits_page;
mod cues;
mod dialogs;
mod engine_text;
mod frame_log;
mod gallery;
mod game;
mod gamepad;
mod hearing;
mod help_panel;
mod hero;
mod hud;
mod i18n;
mod icon_bake;
mod icons;
mod input;
mod inventory_panel;
mod journal;
mod journal_view;
mod kits;
mod layouts;
mod map_view;
mod meshes;
mod minimap;
mod off_hand;
mod orb;
mod organic;
mod pad_view;
mod paths;
mod pickers;
mod rebind;
mod rehearsal;
mod screens;
mod seated;
mod selftest;
mod settings_page;
mod sheet;
mod sheet_view;
mod skills;
mod skills_view;
mod spells;
mod spells_view;
mod steam;
mod surface;
mod theme;
mod title_scene;
mod tombstone;
mod ui_events;
mod vfx;
mod video;

use godot::init::InitStage;
use godot::prelude::*;

struct RenethackExtension;

#[gdextension]
unsafe impl ExtensionLibrary for RenethackExtension {
    fn on_stage_init(stage: InitStage) {
        if stage == InitStage::Scene {
            ignore_sigpipe();
        }
    }

    fn on_stage_deinit(stage: InitStage) {
        if stage == InitStage::Scene {
            icons::clear();
            i18n::clear();
            settings_page::clear();
        }
    }
}

/// Writing a reply to an engine that just died must fail with EPIPE, not
/// kill the client with SIGPIPE. Child processes get the default handler
/// back from std::process::Command.
fn ignore_sigpipe() {
    #[cfg(unix)]
    // SAFETY: setting a signal disposition to SIG_IGN has no preconditions.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}
