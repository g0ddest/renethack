//! The HUD's tips (`hud-tips`): a Wizard's HUD (her bar's last slot
//! empty), two conditions staged in its status strip; the mouse rests on
//! each block that has a tip (the action bar's slots, filled and empty,
//! the small round buttons, the orbs, the XP bar, the log's button, the
//! strip's chips, the portrait, the minimap): its tip comes, one style, at
//! most `tips::MAX_W` wide and a third of the screen tall, above its
//! block (under it when there is no room above), on the screen and off
//! the map's middle, where the hero is; Godot's own tooltip never shows.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use godot::classes::{Engine, Input, InputEventMouseMotion};
use godot::prelude::*;
use nh_world::KeyProfile;

use super::{CharacterChoice, Step, fail_on_error_screen, key, set_status, start_as};
use crate::game::RenethackGame;

/// The blocks to rest on, the one the mouse is on, and the two shot (a
/// slot's tip, an orb's).
static BLOCKS: Mutex<Vec<(String, Rect2)>> = Mutex::new(Vec::new());
static AT: AtomicUsize = AtomicUsize::new(0);
static SHOT: Mutex<[usize; 2]> = Mutex::new([0, 0]);
static FRAME: AtomicI64 = AtomicI64::new(0);
/// The blocks are a panel's (it covers the map's middle) rather than the
/// HUD's; the tips on screen before the panel opened.
static PANEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BEFORE: Mutex<Vec<(String, Rect2)>> = Mutex::new(Vec::new());

/// As many rests as a HUD can have blocks with tips.
const RESTS: usize = 64;

fn wizard() -> CharacterChoice {
    CharacterChoice {
        name: "Hero".into(),
        role: "wizard".into(),
        race: "human".into(),
        gender: "female".into(),
        align: "neutral".into(),
        profile: KeyProfile::Classic,
    }
}

/// The mouse at `at` on the canvas, as the player's would move.
fn hover(g: &RenethackGame, at: Vector2) -> Result<(), String> {
    let vp = g.base().get_viewport().ok_or("no viewport")?;
    let at = vp.get_screen_transform() * at;
    let mut ev = InputEventMouseMotion::new_gd();
    ev.set_position(at);
    ev.set_global_position(at);
    Input::singleton().parse_input_event(&ev);
    Ok(())
}

/// The mouse on block `i`.
fn rest_on(g: &RenethackGame, i: usize) -> Result<(), String> {
    AT.store(i, Ordering::Relaxed);
    FRAME.store(
        Engine::singleton().get_process_frames() as i64,
        Ordering::Relaxed,
    );
    let at = BLOCKS
        .lock()
        .map_err(|e| e.to_string())?
        .get(i)
        .map(|(_, r)| r.position + r.size / 2.0);
    match at {
        Some(at) => hover(g, at),
        None => Ok(()),
    }
}

/// Godot's own tooltips on screen (a window of their own): none must show.
fn godot_tooltips(node: &Gd<Node>, out: &mut Vec<String>) {
    if let Ok(w) = node.clone().try_cast::<godot::classes::PopupPanel>()
        && w.is_visible()
    {
        out.push(format!("{:?} {:?}", w.get_position(), w.get_size()));
    }
    for c in node
        .get_children_ex()
        .include_internal(true)
        .done()
        .iter_shared()
    {
        godot_tooltips(&c, out);
    }
}

/// The tip of the block the mouse rests on: its words, its size within
/// bounds, its place.
fn tip_fits(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let i = AT.load(Ordering::Relaxed);
    let Some((want, block)) = BLOCKS.lock().map_err(|e| e.to_string())?.get(i).cloned() else {
        return Ok(true);
    };
    let mut popups = Vec::new();
    godot_tooltips(&g.base().clone().upcast(), &mut popups);
    if !popups.is_empty() {
        return Err(format!(
            "Godot's own tooltip shows for {want:?}: {popups:?}"
        ));
    }
    let ui = g.ui.as_ref().ok_or("no UI")?;
    let shown = ui.tips.shown().filter(|(t, _)| *t == want);
    let Some((_, r)) = shown else {
        // a moment's rest first; then it must come
        let waited =
            Engine::singleton().get_process_frames() as i64 - FRAME.load(Ordering::Relaxed);
        if waited > 240 {
            return Err(format!("no tip for {want:?} at {block:?}"));
        }
        // the mouse kept there: in a window the system's pointer may
        // take the hover away
        hover(g, block.position + block.size / 2.0)?;
        return Ok(false);
    };
    let view = g.canvas_size();
    let what = format!("{want:?}'s tip at {r:?} for {block:?}");
    if r.size.x > crate::tips::MAX_W + 40.0 || r.size.y > view.y / 3.0 {
        return Err(format!("{what} is too large"));
    }
    if !Rect2::new(Vector2::ZERO, view).grow(1.0).encloses(r) {
        return Err(format!("{what} is off the screen"));
    }
    // above the block, under it only when there is no room above (for a
    // HUD block, room off the map's middle)
    let above = Rect2::new(
        Vector2::new(r.position.x, block.position.y - 8.0 - r.size.y),
        r.size,
    );
    let middle = Rect2::new(view / 2.0 - Vector2::splat(110.0), Vector2::splat(220.0));
    let room = block.position.y - 8.0 >= r.size.y
        && (PANEL.load(Ordering::Relaxed) || !middle.intersects(above));
    let beside = if room {
        r.end().y <= block.position.y
    } else {
        r.position.y >= block.end().y
    };
    if !beside {
        return Err(format!(
            "{what} is not {} it",
            if room { "above" } else { "under" }
        ));
    }
    let middle = Rect2::new(view / 2.0 - Vector2::splat(80.0), Vector2::splat(160.0));
    if !PANEL.load(Ordering::Relaxed) && middle.intersects(r) {
        return Err(format!("{what} covers the map's middle, where the hero is"));
    }
    Ok(true)
}

/// The frame now, to wait from.
fn mark(_: &mut RenethackGame) -> Result<(), String> {
    FRAME.store(
        Engine::singleton().get_process_frames() as i64,
        Ordering::Relaxed,
    );
    Ok(())
}

/// A few frames after the mark, the tip still shown: drawn (the mouse
/// kept on its block).
fn drawn(g: &RenethackGame) -> Result<bool, String> {
    let i = AT.load(Ordering::Relaxed);
    if let Some((_, r)) = BLOCKS.lock().map_err(|e| e.to_string())?.get(i) {
        hover(g, r.position + r.size / 2.0)?;
    }
    if g.ui.as_ref().and_then(|ui| ui.tips.shown()).is_none() {
        return Err("the tip went before its picture".into());
    }
    Ok(Engine::singleton().get_process_frames() as i64 > FRAME.load(Ordering::Relaxed) + 4)
}

/// The mouse on each block in turn, its tip checked.
fn rests() -> Vec<Step> {
    let mut steps = Vec::new();
    for _ in 0..RESTS {
        steps.extend([
            Step::Call("the mouse on the next block", |g| {
                let next = AT.load(Ordering::Relaxed).wrapping_add(1);
                let n = BLOCKS.lock().map_err(|e| e.to_string())?.len();
                if next < n {
                    rest_on(g, next)
                } else {
                    AT.store(n, Ordering::Relaxed);
                    Ok(())
                }
            }),
            Step::Wait("its tip, one style, its size and place", tip_fits),
        ]);
    }
    steps
}

/// Every tip on screen now (the HUD's and any panel's).
fn all_tipped(g: &RenethackGame) -> Vec<(String, Rect2)> {
    crate::tips::tipped(&g.base().clone().upcast())
}

/// The blocks a panel brought: the tips on screen now that were not
/// before it opened, `least` of them at least.
fn panel_blocks(g: &mut RenethackGame, least: usize) -> Result<(), String> {
    let before = BEFORE.lock().map_err(|e| e.to_string())?.clone();
    let mut blocks: Vec<(String, Rect2)> = Vec::new();
    for b in all_tipped(g) {
        if b.1.size.x > 4.0
            && b.1.size.y > 4.0
            && !before.contains(&b)
            && !blocks.iter().any(|(_, r)| *r == b.1)
        {
            blocks.push(b);
        }
    }
    if blocks.len() < least || blocks.len() > RESTS {
        return Err(format!("{} blocks in the panel: {blocks:?}", blocks.len()));
    }
    *BLOCKS.lock().map_err(|e| e.to_string())? = blocks;
    PANEL.store(true, Ordering::Relaxed);
    AT.store(usize::MAX, Ordering::Relaxed);
    Ok(())
}

/// The tips on screen before a panel opens.
fn before_panel(g: &mut RenethackGame) -> Result<(), String> {
    *BEFORE.lock().map_err(|e| e.to_string())? = all_tipped(g);
    // and the mouse off whatever it rested on
    hover(g, Vector2::new(4.0, 4.0))
}

/// Ten frames after the mark: what opened is laid out.
fn settled(_: &RenethackGame) -> Result<bool, String> {
    Ok(Engine::singleton().get_process_frames() as i64 > FRAME.load(Ordering::Relaxed) + 10)
}

pub(super) fn hud_tips() -> Vec<Step> {
    let mut steps = start_as(wizard());
    steps.extend([
        // the status strip's chips: two conditions
        Step::Call("blind and levitating", |g| {
            let cat = g.catalog.clone().ok_or("no catalog")?;
            let mask = |name: &str| {
                cat.conditions
                    .iter()
                    .find(|c| c.name == name)
                    .map(|c| c.mask)
                    .ok_or(format!("no condition {name}"))
            };
            let conds = mask("Blind")? | mask("Lev")?;
            set_status(g, "condition", "", Some(conds));
            Ok(())
        }),
        Step::Wait("the conditions' chips in the strip", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let blind = crate::i18n::engine(crate::i18n::EngineKind::Status, "Blind");
            Ok(ui.hud.tipped().iter().any(|(t, _)| *t == *blind))
        }),
        // laid out where they stay
        Step::Call("", |_| {
            FRAME.store(
                Engine::singleton().get_process_frames() as i64,
                Ordering::Relaxed,
            );
            Ok(())
        }),
        Step::Wait("a few frames", |_| {
            Ok(
                Engine::singleton().get_process_frames() as i64
                    > FRAME.load(Ordering::Relaxed) + 10,
            )
        }),
        Step::Call("the blocks with a tip", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            let view = g.canvas_size();
            let mut blocks: Vec<(String, Rect2)> = Vec::new();
            for (t, r) in ui.hud.tipped() {
                // a block once: a panel and its button can share a rect
                if r.size.x > 4.0 && r.size.y > 4.0 && !blocks.iter().any(|(_, b)| *b == r) {
                    blocks.push((t, r));
                }
            }
            let foot: Vec<usize> = (0..blocks.len())
                .filter(|&i| blocks[i].1.position.y > view.y / 2.0)
                .collect();
            let empty = crate::tr!("bar-slot-empty-tip", key = "0");
            // the pictures: the bar's first slot (a tip of two lines at the
            // foot), the HP orb (the foot's widest square, on the left)
            let slot = foot.iter().copied().find(|&i| blocks[i].0.contains('\n'));
            let orb = foot
                .iter()
                .copied()
                .filter(|&i| blocks[i].1.size.x == blocks[i].1.size.y && blocks[i].1.size.x > 100.0)
                .min_by(|&a, &b| blocks[a].1.position.x.total_cmp(&blocks[b].1.position.x));
            let (Some(slot), Some(orb)) = (slot, orb) else {
                return Err(format!("no slot or orb among {blocks:?}"));
            };
            if foot.len() < 15 || !blocks.iter().any(|(t, _)| *t == empty) || blocks.len() > RESTS {
                return Err(format!(
                    "{} blocks ({} at the foot): {blocks:?}",
                    blocks.len(),
                    foot.len()
                ));
            }
            *SHOT.lock().map_err(|e| e.to_string())? = [slot, orb];
            *BLOCKS.lock().map_err(|e| e.to_string())? = blocks;
            // before the first
            AT.store(usize::MAX, Ordering::Relaxed);
            Ok(())
        }),
    ]);
    steps.extend(rests());
    // the pictures: a slot's tip, an orb's
    steps.extend([
        Step::Call("the mouse on a slot", |g| {
            let [slot, _] = *SHOT.lock().map_err(|e| e.to_string())?;
            rest_on(g, slot)
        }),
        Step::Wait("the slot's tip", tip_fits),
        Step::Call("", mark),
        Step::Wait("drawn", drawn),
        Step::Shot("hud-tips-slot"),
        Step::Call("the mouse on an orb", |g| {
            let [_, orb] = *SHOT.lock().map_err(|e| e.to_string())?;
            rest_on(g, orb)
        }),
        Step::Wait("the orb's tip", tip_fits),
        Step::Call("", mark),
        Step::Wait("drawn", drawn),
        Step::Shot("hud-tips-orb"),
        // the inventory panel's tips: its tabs, the doll's slots, its
        // buttons
        Step::Call("the tips before the inventory", before_panel),
        key('i'),
        Step::Wait("the inventory", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.inventory.is_open()))
        }),
        Step::Call("", mark),
        Step::Wait("laid out", settled),
        Step::Call("the inventory's blocks", |g| panel_blocks(g, 8)),
    ]);
    steps.extend(rests());
    steps.extend([
        Step::Key(nh_world::KeyInput::plain(nh_world::Key::Escape)),
        Step::Wait("the inventory closed", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| !ui.inventory.is_open()))
        }),
        // the help's
        Step::Call("the tips before the help", before_panel),
        Step::Push(crate::ui_events::UiEvent::ToggleHelp),
        Step::Wait("the help", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| ui.help.is_open()))
        }),
        Step::Call("", mark),
        Step::Wait("laid out", settled),
        Step::Call("the help's blocks", |g| panel_blocks(g, 1)),
    ]);
    steps.extend(rests());
    steps.extend([
        Step::Push(crate::ui_events::UiEvent::ToggleHelp),
        Step::Wait("the help closed", |g| {
            Ok(g.ui.as_ref().is_some_and(|ui| !ui.help.is_open()))
        }),
    ]);
    steps.extend(super::quit());
    steps
}
