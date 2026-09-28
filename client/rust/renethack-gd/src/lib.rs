//! renethack's Godot client: a GDExtension whose root node,
//! `RenethackGame`, runs nh-engine and draws the game.

mod animator;
mod art;
mod dialogs;
mod gallery;
mod game;
mod hud;
mod input;
mod map_view;
mod meshes;
mod paths;
mod screens;
mod selftest;
mod theme;
mod ui_events;

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
