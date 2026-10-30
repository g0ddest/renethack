//! `outfits`: each role's hero close up, as the creation form shows them
//! in the title scene (its own camera, the role's starting kit), a heroine
//! and a hero each, for sheets of the outfits before and after a change of
//! their look. RENETHACK_ROLES=valkyrie,samurai limits the roles. Nothing
//! is checked but that each is drawn; with `--screenshots`, the pictures
//! `outfit-<role>-<f|m>`, and with RENETHACK_BEHIND set each from behind
//! as well (`-back`).

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use godot::classes::Engine;
use godot::obj::Singleton;
use nh_world::KeyProfile;

use super::{Step, fail_on_error_screen, screen};
use crate::game::RenethackGame;
use crate::ui_events::{CharacterChoice, UiEvent};

const ROLES: [&str; 13] = [
    "archeologist",
    "barbarian",
    "caveman",
    "healer",
    "knight",
    "monk",
    "priest",
    "rogue",
    "ranger",
    "samurai",
    "tourist",
    "valkyrie",
    "wizard",
];

static NEXT: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicI64 = AtomicI64::new(0);

fn frames() -> i64 {
    Engine::singleton().get_process_frames() as i64
}

/// The (role, female) shot in turn.
fn heroes() -> Vec<(String, bool)> {
    let roles: Vec<String> = match std::env::var("RENETHACK_ROLES") {
        Ok(v) => v.split(',').map(|r| r.trim().to_string()).collect(),
        Err(_) => ROLES.iter().map(|r| r.to_string()).collect(),
    };
    roles
        .into_iter()
        .flat_map(|r| {
            // a Valkyrie is a woman
            let genders: &[bool] = if r == "valkyrie" {
                &[true]
            } else {
                &[true, false]
            };
            genders
                .iter()
                .map(move |&f| (r.clone(), f))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn shown(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let map = &g.ui.as_ref().ok_or("no UI")?.map;
    let loaded = map.hero_model().is_some_and(|m| !m.is_pending());
    if !(map.creation_framed() && loaded) {
        FRAME.store(frames(), Ordering::Relaxed);
        return Ok(false);
    }
    // a moment for the model's fade-in and its pose
    Ok(frames() > FRAME.load(Ordering::Relaxed) + 40)
}

pub(super) fn outfits() -> Vec<Step> {
    let mut steps = vec![
        Step::Wait("the title, the art loaded ahead", |g| {
            fail_on_error_screen(g)?;
            let loaded = g.ui.as_ref().is_some_and(|ui| ui.map.preloaded());
            Ok(screen(g) == Some("title") && loaded && g.warmed_up())
        }),
        Step::Push(UiEvent::NewGame),
        Step::Wait("the creation form", |g| Ok(screen(g) == Some("creation"))),
    ];
    for (role, female) in heroes() {
        let g = if female { "f" } else { "m" };
        let shot: &'static str = Box::leak(format!("outfit-{role}-{g}").into_boxed_str());
        steps.extend([
            Step::Call("the next hero", |g| {
                let i = NEXT.fetch_add(1, Ordering::Relaxed);
                let (role, female) = heroes().get(i).cloned().ok_or("no more heroes")?;
                g.preset_creation(&CharacterChoice {
                    name: String::new(),
                    role,
                    race: "human".into(),
                    gender: if female { "female" } else { "male" }.into(),
                    align: "random".into(),
                    profile: KeyProfile::Modern,
                });
                FRAME.store(frames(), Ordering::Relaxed);
                Ok(())
            }),
            Step::Wait("the hero drawn, framed", shown),
            Step::Shot(shot),
        ]);
        if std::env::var_os("RENETHACK_BEHIND").is_some() {
            let back: &'static str = Box::leak(format!("{shot}-back").into_boxed_str());
            steps.extend([
                Step::Call("the hero turned round", |g| turn(g, 180.0)),
                Step::Wait("a moment", |_| {
                    Ok(frames() > FRAME.load(Ordering::Relaxed) + 10)
                }),
                Step::Shot(back),
                Step::Call("the hero turned back", |g| turn(g, -180.0)),
            ]);
        }
    }
    steps
}

/// The hero's model turned `degrees` about its up.
fn turn(g: &mut RenethackGame, degrees: f32) -> Result<(), String> {
    let map = &g.ui.as_ref().ok_or("no UI")?.map;
    map.hero_model().ok_or("no hero")?.turn(degrees);
    FRAME.store(frames(), Ordering::Relaxed);
    Ok(())
}
