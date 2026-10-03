//! renethack's Godot client: a GDExtension whose root node,
//! `RenethackGame`, runs nh-engine and draws the game.

mod action_bar;
mod animator;
mod art;
mod batch;
mod branch_look;
mod dialogs;
mod gallery;
mod game;
mod gamepad;
mod hero;
mod hud;
mod icon_bake;
mod icons;
mod input;
mod inventory_panel;
mod map_view;
mod meshes;
mod minimap;
mod orb;
mod pad_view;
mod paths;
mod rehearsal;
mod screens;
mod selftest;
mod surface;
mod theme;
mod ui_events;
mod vfx;

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
