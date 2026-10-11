//! `riding`: a Knight and the pony, from the first turn. On foot the pony
//! beside him wears its saddle. `#ride` puts him on its back within the
//! turn: his own model, with the kit it wore, seated (the hips on the
//! saddle, the knees clear of the pony's sides), the rim light and the
//! ring his, the pony under him no pet's any more. The pair walks (the
//! pony's own walk, the rider sitting it out), then gallops; he strikes a
//! lizard from the saddle with his own blow, the pony under him turning
//! to it; he couches the lance, applied at a spot (which wields it) and
//! then struck with. Off again, the pony is a pet with its saddle and he
//! the same model on his feet. Then he mounts a warhorse, a bigger
//! horse, and the game is saved and restored: he is in the saddle from
//! its first frame, the Knight on his horse. The HUD's portrait and the
//! inventory's doll are his all along, never the steed's. Debug mode: the mount is
//! forced (a first-level Knight slips off half the time), the lizards
//! (asleep: a first-level pony is soon bitten to death) and the warhorse
//! are made, each lizard slain after the blow. Pictures of each with
//! `--screenshots`; in a window, the inventory's doll with the rider on
//! it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use godot::classes::{BoneAttachment3D, DisplayServer, Node, Node3D, SkeletonModifier3D};
use godot::prelude::*;
use nh_protocol::{GlyphKind, mg};
use nh_world::{Key, KeyInput, Prompt};

use super::hero::{held_on, in_use_at};
use super::{
    CharacterChoice, DialogEvent, Step, UiEvent, camera_settled, command, continue_label, ctrl_key,
    fail_on_error_screen, idle_command, key, map_view, panel, quit, screen, smoke_choice, start_as,
};
use crate::art::{SADDLE_NODE, SEATED, USE_NODE};
use crate::game::RenethackGame;
use crate::map_view::MapView;
use crate::seated::Seated;

const DIRS: [((i32, i32), char); 8] = [
    ((-1, 0), 'h'),
    ((1, 0), 'l'),
    ((0, -1), 'k'),
    ((0, 1), 'j'),
    ((-1, -1), 'y'),
    ((1, -1), 'u'),
    ((-1, 1), 'b'),
    ((1, 1), 'n'),
];

/// The hips are on the saddle within this (the skeleton's units, about a
/// metre and a half of them to a hero).
const ON_SEAT: f32 = 0.004;
/// The saddle is over the steed's own cell within this, metres.
const OVER_STEED: f32 = 0.3;
/// A couched lance is this level, and points this much the pair's way.
const LEVEL: f32 = 0.45;
const AHEAD: f32 = 0.85;

fn knight() -> CharacterChoice {
    CharacterChoice {
        // debug mode plays as "wizard" whatever the name, and saves so
        name: "wizard".into(),
        role: "knight".into(),
        gender: "male".into(),
        align: "lawful".into(),
        ..smoke_choice()
    }
}

/// What the test remembers from step to step.
struct Seen {
    /// The hero's own model's node, as first seen on foot.
    hero: Option<InstanceId>,
    /// What his bones held on foot: the right hand, the left arm, the back.
    kit: Vec<Vec<String>>,
    /// The steed to mount next, by name.
    steed: &'static str,
    /// The way the pair walks, and where the lance strikes.
    way: Option<char>,
    /// The gaits the steed's steps were seen to play.
    gaits: Vec<String>,
    /// The furthest the couched lance was seen from level, and the least
    /// it pointed ahead (None: not seen couched yet).
    couched: Option<(f32, f32)>,
}

static SEEN: Mutex<Seen> = Mutex::new(Seen {
    hero: None,
    kit: Vec::new(),
    steed: "pony",
    way: None,
    gaits: Vec::new(),
    couched: None,
});
static STEPS: AtomicU32 = AtomicU32::new(0);
static STRIKES: AtomicU32 = AtomicU32::new(0);

fn seen() -> Result<std::sync::MutexGuard<'static, Seen>, String> {
    SEEN.lock().map_err(|e| e.to_string())
}

fn name_of(g: &RenethackGame, glyph: &nh_protocol::Glyph) -> Option<String> {
    let cat = g.catalog.as_deref()?;
    let m = cat.monsters.get(usize::try_from(glyph.mon?).ok()?)?;
    Some(m.name.clone())
}

/// The saddled pet called `name` next to the hero: (its cell, the vi-key
/// towards it).
fn steed_near(g: &RenethackGame, name: &str) -> Option<((i32, i32), char)> {
    let (hx, hy) = g.world.map.hero()?;
    DIRS.into_iter().find_map(|((dx, dy), k)| {
        let at = (hx + dx, hy + dy);
        is_steed(g, at, name).then_some((at, k))
    })
}

/// The saddled pet called `name` within `cells` of the hero: its cell
/// (and a vi-key towards it, as `steed_near` gives one).
fn steed_by(g: &RenethackGame, name: &str, cells: i32) -> Option<((i32, i32), char)> {
    let (hx, hy) = g.world.map.hero()?;
    (-cells..=cells)
        .flat_map(|dy| (-cells..=cells).map(move |dx| (hx + dx, hy + dy)))
        .find(|&at| at != (hx, hy) && is_steed(g, at, name))
        .map(|at| (at, ' '))
}

/// A saddled pet called `name` on that cell.
fn is_steed(g: &RenethackGame, (x, y): (i32, i32), name: &str) -> bool {
    let Some(cell) = g.world.map.cell(x, y) else {
        return false;
    };
    cell.glyph.as_ref().is_some_and(|glyph| {
        glyph.kind == GlyphKind::Mon
            && glyph.flags & mg::PET != 0
            && cell.saddled
            && name_of(g, glyph).as_deref() == Some(name)
    })
}

/// A hostile next to the hero: the vi-key towards it.
fn foe_near(g: &RenethackGame) -> Option<char> {
    let (hx, hy) = g.world.map.hero()?;
    DIRS.into_iter().find_map(|((dx, dy), k)| {
        let glyph = g.world.map.cell(hx + dx, hy + dy)?.glyph.as_ref()?;
        (glyph.kind == GlyphKind::Mon && glyph.flags & (mg::PET | mg::HERO) == 0).then_some(k)
    })
}

/// The vi-key towards four open cells in a row with nothing on them: the
/// way to walk, chosen once.
fn way(g: &RenethackGame) -> Result<char, String> {
    let mut s = seen()?;
    if s.way.is_none() {
        let cat = g.catalog.clone().ok_or("no catalog")?;
        let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
        s.way = DIRS.into_iter().take(4).find_map(|((dx, dy), k)| {
            let open = |n: i32| {
                let c = g.world.map.cell(hx + dx * n, hy + dy * n);
                crate::map_view::is_open(c, &cat)
                    && c.is_some_and(|c| c.glyph.as_ref().is_none_or(|g| g.kind == GlyphKind::Cmap))
            };
            (1..=4).all(open).then_some(k)
        });
    }
    s.way
        .ok_or_else(|| "no four open cells in a row by the hero".into())
}

fn way_key(g: &RenethackGame) -> Result<KeyInput, String> {
    Ok(KeyInput::plain(Key::Char(way(g)?)))
}

fn directions(p: &Prompt) -> bool {
    matches!(
        p,
        Prompt::FreeKey {
            directions: true,
            ..
        }
    )
}

fn getobj(p: &Prompt) -> bool {
    matches!(
        p,
        Prompt::FreeKey {
            directions: false,
            ..
        }
    )
}

fn ride_command() -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::TextSubmitted("ride".into())),
    ]
}

/// `#ride` towards the steed the test wants next, the mount forced.
fn mount() -> Vec<Step> {
    let mut steps = ride_command();
    steps.extend([
        Step::Request("the way to the steed", directions),
        Step::KeyFrom("towards the steed", |g| {
            let name = seen()?.steed;
            let (_, k) =
                steed_near(g, name).ok_or_else(|| format!("no saddled {name} next to the hero"))?;
            Ok(KeyInput::plain(Key::Char(k)))
        }),
        Step::Request(
            "Force the mount?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("Force")),
        ),
        key('y'),
        Step::AnswerUntil('n', "a command after mounting", idle_command),
    ]);
    steps
}

/// The debug mode's `^G`: a monster made next to the hero.
fn genesis(what: &'static str) -> Vec<Step> {
    vec![
        Step::Key(ctrl_key('g')),
        Step::Request(
            "what to create",
            |p| matches!(p, Prompt::Text { query, .. } if query.contains("Create what")),
        ),
        Step::Dialog(DialogEvent::TextSubmitted(what.into())),
        Step::AnswerUntil('n', "a command after the creation", idle_command),
    ]
}

/// The hero's own model's node, and the seat's solver on it with how far
/// it holds (0 on foot, 1 in the saddle).
type Seat = (Gd<Node3D>, Option<Gd<Seated>>, f32);

fn seat_of(g: &RenethackGame) -> Result<Seat, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let seated = node
        .find_child_ex(SEATED)
        .owned(false)
        .done()
        .and_then(|n| n.try_cast::<Seated>().ok());
    let hold = seated.as_ref().map_or(0.0, |s| {
        s.clone().upcast::<SkeletonModifier3D>().get_influence()
    });
    Ok((m.node.clone(), seated, hold))
}

/// A bone attachment the test puts on a bone of the hero's own model: it
/// shows the bone as the solvers leave it. None in the frame it is made.
fn probe(g: &RenethackGame, bone: &str) -> Result<Option<Gd<Node3D>>, String> {
    let (node, ..) = seat_of(g)?;
    let node: Gd<Node> = node.upcast();
    let name = format!("Probe_{bone}");
    if let Some(p) = node.find_child_ex(name.as_str()).owned(false).done() {
        return Ok(Some(p.cast::<Node3D>()));
    }
    let mut skeleton = node
        .find_children_ex("*")
        .type_("Skeleton3D")
        .owned(false)
        .done()
        .iter_shared()
        .next()
        .ok_or("no skeleton in the hero's model")?;
    let mut probe = BoneAttachment3D::new_alloc();
    probe.set_name(name.as_str());
    probe.set_bone_name(bone);
    skeleton.add_child(&probe);
    Ok(None)
}

/// The saddle shown on the back of what stands on a cell.
fn saddle_on(g: &RenethackGame, (x, y): (i32, i32)) -> Result<bool, String> {
    let map = map_view(g)?;
    let Some(m) = map.entity_model(x, y) else {
        return Ok(false);
    };
    let node: Gd<Node> = m.node.clone().upcast();
    let saddle = node.find_child_ex(SADDLE_NODE).owned(false).done();
    Ok(saddle
        .and_then(|s| s.try_cast::<Node3D>().ok())
        .is_some_and(|s| s.is_visible_in_tree()))
}

/// What the hero's bones hold now: the right hand, the left arm, the back.
fn kit(g: &RenethackGame) -> Result<Vec<Vec<String>>, String> {
    ["hand_r", "lowerarm_l", "spine_03"]
        .into_iter()
        .map(|bone| Ok(held_on(g, bone)?.0))
        .collect()
}

/// The hero rides as the test wants it: in the saddle of the steed on his
/// cell, his own model with its kit, the hips on the seat and the knees
/// clear of the steed's sides, the rim light and the ring his. False
/// while the measure cannot be taken yet (the solver settling, a probe
/// just made); an error once it can and is wrong.
fn rides(g: &RenethackGame) -> Result<bool, String> {
    // what keeps it waiting, now and then (a timed-out wait says no more)
    let not_yet = |why: &str| {
        static SAID: AtomicU32 = AtomicU32::new(0);
        if SAID.fetch_add(1, Ordering::Relaxed).is_multiple_of(120) {
            godot_print!("selftest: riding: not in the saddle yet: {why}");
        }
        Ok(false)
    };
    fail_on_error_screen(g)?;
    let map = map_view(g)?;
    let Some((steed, mount)) = map.steed() else {
        let form = g.world.map.hero_form().and_then(|f| name_of(g, f));
        return not_yet(&format!("no steed drawn under the hero ({form:?})"));
    };
    let (node, seated, hold) = seat_of(g)?;
    let hips = probe(g, "pelvis")?;
    let knees = [probe(g, "calf_l")?, probe(g, "calf_r")?];
    let (Some(seated), Some(hips), [Some(left), Some(right)]) = (seated, hips, knees) else {
        return not_yet("the seat's solver or the probes just made");
    };
    if steady_seat(hold)? < 1.0 || map.hero_walking() {
        return not_yet(&format!("hold {hold}, walking {}", map.hero_walking()));
    }
    // the hips where the solver put them when the skeleton was last
    // solved: the probe shows that pose
    let Some(put) = seated.bind().hips() else {
        return not_yet("the seat never solved");
    };
    let off = (hips.get_transform().origin - put).length();
    if off > ON_SEAT {
        return Err(format!("the rider's hips are {off:.4} off the saddle"));
    }
    // the saddle over the steed, above the ground
    let seat = mount.at().ok_or("the steed's back is gone")?;
    let under = steed.node.get_global_position();
    let aside = Vector2::new(seat.x - under.x, seat.z - under.z).length();
    if aside > OVER_STEED || seat.y - under.y < 0.25 {
        return Err(format!(
            "the saddle is {aside:.2} m beside the steed, {:.2} m up",
            seat.y - under.y
        ));
    }
    // astride: a knee out on each side beyond the back's breadth, below
    // the hips
    let across = steed.node.get_global_transform().basis.col_a().normalized();
    let out = |knee: &Gd<Node3D>| (knee.get_global_position() - seat).dot(across);
    let (l, r) = (out(&left), out(&right));
    if l < mount.half_width || -r < mount.half_width {
        return Err(format!(
            "the knees are {l:.2} and {r:.2} m out, the back {:.2} m broad each way",
            mount.half_width
        ));
    }
    if left.get_global_position().y > hips.get_global_position().y {
        return Err("the rider's knee is above the hips".into());
    }
    // one body with the steed: the same way round, on its spot
    let (a, b) = (
        node.get_global_transform(),
        steed.node.get_global_transform(),
    );
    if (a.origin - b.origin).length() > 0.01 || a.basis.col_c().dot(b.basis.col_c()) < 0.999 {
        return Err("the rider does not stand with the steed".into());
    }
    // his own model, with what it held on foot
    let s = seen()?;
    if s.hero.is_some_and(|id| id != node.instance_id()) {
        return Err("the rider is another model than the hero's on foot".into());
    }
    let now = kit(g)?;
    if !s.kit.is_empty() && now != s.kit && in_use_at(g)?.is_none() {
        let hand: Gd<Node> = node.clone().upcast();
        let att = hand.find_child_ex("Gear_hand_r").owned(false).done();
        let kids: Vec<String> = att
            .map(|a| {
                a.get_children()
                    .iter_shared()
                    .map(|c| {
                        let vis = c.clone().try_cast::<Node3D>().is_ok_and(|n| n.is_visible());
                        format!(
                            "{} vis {vis} q {}",
                            c.get_name(),
                            c.is_queued_for_deletion()
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Err(format!(
            "the rider holds {now:?}, on foot {:?}; the right hand has {kids:?}",
            s.kit
        ));
    }
    // the rim light and the ring are the hero's, not the steed's
    if !MapView::on_rim(&node) || MapView::on_rim(&steed.node) {
        return Err("the rim light is not on the rider alone".into());
    }
    let (hx, hy) = g.world.map.hero().ok_or("no hero")?;
    for (whose, cell, gap) in map.ring_gaps() {
        if whose == "pet" && cell == (hx, hy) {
            return Err("a pet's ring under the ridden steed".into());
        }
        if whose == "hero" && gap.is_none_or(|d| d > 0.05) {
            return Err(format!("the hero's ring is {gap:?} from the pair"));
        }
    }
    Ok(true)
}

/// The frame the seat's hold was last seen to change in, and the hold
/// since.
static HOLD: Mutex<(u64, f32)> = Mutex::new((0, -1.0));

/// The seat's hold once the skeleton has been solved with it (the probes
/// show the pose of the frame before: a hold reached this frame is not in
/// them yet); 0 while it is fresh.
fn steady_seat(hold: f32) -> Result<f32, String> {
    let frame = godot::classes::Engine::singleton().get_process_frames();
    let mut last = HOLD.lock().map_err(|e| e.to_string())?;
    if last.1 != hold {
        *last = (frame, hold);
    }
    Ok(if frame > last.0 { hold } else { 0.0 })
}

/// On foot: no steed under the hero, the seat's solver asleep, the model
/// the hero's own from before.
fn afoot(g: &RenethackGame) -> Result<bool, String> {
    fail_on_error_screen(g)?;
    let map = map_view(g)?;
    let (node, _, hold) = seat_of(g)?;
    if map.steed().is_some() || hold > 0.0 || map.hero_walking() {
        return Ok(false);
    }
    if seen()?.hero.is_some_and(|id| id != node.instance_id()) {
        return Err("on foot again the hero is another model".into());
    }
    if !MapView::on_rim(&node) {
        return Err("the rim light left the hero as he got off".into());
    }
    Ok(true)
}

/// The steed's own clip playing now (its model's player).
fn steed_clip(g: &RenethackGame) -> Result<Option<String>, String> {
    let map = map_view(g)?;
    let (steed, _) = map.steed().ok_or("no steed")?;
    Ok(steed
        .player()
        .filter(|p| p.is_playing())
        .map(|p| p.get_current_animation().to_string()))
}

/// A step of the pair: the key, then the steed's gait noted as it plays.
fn step() -> Vec<Step> {
    vec![
        Step::Call("the steps so far", |g| {
            STEPS.store(map_view(g)?.motion_stats().hero_steps, Ordering::Relaxed);
            Ok(())
        }),
        Step::KeyFrom("a step", way_key),
        Step::Wait("the steed steps", |g| {
            let map = map_view(g)?;
            if map.motion_stats().hero_steps == STEPS.load(Ordering::Relaxed) {
                return Ok(false);
            }
            // the gait is the steed's, and the rider sits it out: no walk
            // of his own
            let (gait, _) = map.hero_clips();
            let gait = gait.ok_or("the steed steps with no gait of its own")?;
            let riding = map.hero_clip();
            if riding.as_deref() == Some("Walk") || riding.as_deref() == Some("Jog_Fwd") {
                return Err(format!("the rider plays {riding:?} on a walking steed"));
            }
            seen()?.gaits.push(gait);
            Ok(true)
        }),
        Step::Request("a command after the step", command),
    ]
}

fn headless() -> bool {
    DisplayServer::singleton().get_name() == "headless"
}

/// Where the couch's clip is now, as a share of it (None: it does not
/// play); the lance's level and how far it points ahead, measured while
/// the clip holds it down (between `COUCHED`'s shares), noted in `SEEN`.
fn couched(g: &RenethackGame) -> Result<Option<f32>, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let p = m.player().ok_or("no animation player")?;
    if map.hero_clip().as_deref() != Some("proc/couch") {
        return Ok(None);
    }
    let share = p.get_current_animation_position() as f32
        / (p.get_current_animation_length() as f32).max(1e-3);
    if !COUCHED.contains(&share) {
        return Ok(Some(share));
    }
    // the lance in his right hand, alone
    let Some((hand, held)) = in_use_at(g)? else {
        return Err("the couch plays with nothing in hand".into());
    };
    if (hand, held.as_str()) != ("hand_r", "lance") {
        return Err(format!("the couch plays with the {held} in {hand}"));
    }
    let node: Gd<Node> = m.node.clone().upcast();
    let lance = node
        .find_child_ex(&format!("{USE_NODE}_lance"))
        .owned(false)
        .done()
        .and_then(|n| n.get_child(0))
        .ok_or("no lance in hand")?
        .cast::<Node3D>();
    // a pole lies along its model's z
    let along = (lance.get_global_transform().basis * Vector3::BACK).normalized();
    let ahead = m.node.get_global_transform().basis.col_c().normalized();
    let (level, front) = (along.y.abs(), along.dot(ahead).abs());
    let mut s = seen()?;
    let worst = s.couched.unwrap_or((0.0, 1.0));
    s.couched = Some((worst.0.max(level), worst.1.min(front)));
    Ok(Some(share))
}

/// The shares of the couch's clip that hold the lance down.
const COUCHED: std::ops::RangeInclusive<f32> = 0.3..=0.62;

/// The lance comes down under the arm as its clip plays: level, its point
/// the pair's way ahead while the clip holds it there. A picture then.
fn couch(what: &'static str, shot: &'static str) -> Vec<Step> {
    vec![
        Step::Call("the lance not seen couched yet", |_| {
            seen()?.couched = None;
            Ok(())
        }),
        Step::Wait(what, |g| Ok(couched(g)?.is_some_and(|s| s >= 0.36))),
        Step::Shot(shot),
        Step::Wait("the couch over", |g| {
            Ok(couched(g)?.is_none_or(|s| s > 0.62))
        }),
        Step::Call("level, its point ahead", |_| {
            let (level, front) = seen()?.couched.ok_or("the lance was never seen couched")?;
            godot_print!(
                "selftest: riding: the couched lance at most {level:.2} off level, at least {front:.2} ahead"
            );
            if level > LEVEL || front < AHEAD {
                return Err(format!(
                    "the couched lance is {level:.2} off level and points {front:.2} ahead"
                ));
            }
            Ok(())
        }),
    ]
}

/// The debug mode's `#wizkill` on the monster nearest the hero, if
/// there is one by them: the fight is over before the foe bites the
/// pony twice (a first-level pony is soon killed).
fn slay() -> Vec<Step> {
    vec![
        key('#'),
        Step::Request("the command palette", |p| *p == Prompt::ExtCmd),
        Step::Dialog(DialogEvent::ExtCmd(Some("wizkill".into()))),
        // the game's first getpos shows its tip first
        Step::AnswerUntil('n', "the monster to slay", |g| {
            fail_on_error_screen(g)?;
            Ok(g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        // the cursor to the nearest monster, if any (else it stays put)
        key('m'),
        Step::Request("the cursor on it", command),
        Step::KeyFrom("it slain, or none", |g| {
            let near = g
                .world
                .cursor
                .zip(g.world.map.hero())
                .is_some_and(|(c, h)| c != h && (c.0 - h.0).abs().max((c.1 - h.1).abs()) <= 2);
            Ok(KeyInput::plain(if near {
                Key::Char('.')
            } else {
                Key::Escape
            }))
        }),
        Step::Request("the next monster to slay, or a command", command),
        Step::KeyFrom("no more", |g| {
            Ok(KeyInput::plain(if g.world.getpos {
                Key::Escape
            } else {
                Key::Char('s')
            }))
        }),
        Step::AnswerUntil('n', "a command after the slaying", idle_command),
        Step::Call("nothing left to fight", |g| match foe_near(g) {
            Some(k) => Err(format!("a foe is still next to the pair ('{k}')")),
            None => Ok(()),
        }),
    ]
}

/// A picture is the next frame as it is drawn: the steps after one wait
/// for it, so nothing they change (the camera, a panel) is in it.
fn pictures_first(steps: Vec<Step>) -> Vec<Step> {
    let mut out = Vec::with_capacity(steps.len());
    for step in steps {
        let picture = matches!(step, Step::Shot(_) | Step::ShotIf(..));
        out.push(step);
        if picture {
            out.push(Step::Wait("the picture taken", |_| {
                Ok(super::NEXT_FRAME
                    .lock()
                    .map_err(|e| e.to_string())?
                    .is_empty())
            }));
        }
    }
    out
}

pub(super) fn riding() -> Vec<Step> {
    let mut steps = vec![Step::Call("debug mode, seed 1", |g| {
        g.debug_mode = true;
        g.seed = Some(crate::game::env_number("RENETHACK_RIDE_SEED").unwrap_or(1));
        *seen()? = Seen {
            hero: None,
            kit: Vec::new(),
            steed: "pony",
            way: None,
            gaits: Vec::new(),
            couched: None,
        };
        Ok(())
    })];
    steps.extend(start_as(knight()));
    // on foot, the pony beside him saddled
    steps.extend([
        Step::Wait("the hero on the map", |g| {
            Ok(map_view(g)?.hero_model().is_some_and(|m| !m.is_pending()))
        }),
        Step::Call("close up, the HUD out of the picture", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(3.4, 0.8);
            ui.hud.set_visible(false);
            Ok(())
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Wait("the pony beside him wears its saddle", |g| {
            let (cell, _) = steed_near(g, "pony").ok_or("no saddled pony next to the hero")?;
            saddle_on(g, cell)
        }),
        Step::Call("the hero on foot: his model and what it holds", |g| {
            let (node, _, hold) = seat_of(g)?;
            let kit = kit(g)?;
            godot_print!("selftest: riding: on foot the hero holds {kit:?}");
            if hold != 0.0 || map_view(g)?.steed().is_some() {
                return Err("the hero rides before he mounted".into());
            }
            if kit[0].is_empty() {
                return Err("the Knight holds nothing in the right hand".into());
            }
            let mut s = seen()?;
            s.hero = Some(node.instance_id());
            s.kit = kit;
            Ok(())
        }),
        Step::Shot("ride-on-foot"),
    ]);
    // into the saddle
    steps.extend(mount());
    steps.extend([
        Step::Wait("the hero in the pony's saddle", rides),
        Step::Call("a man on a mare: the hero's form is his own", |g| {
            let (x, y) = g.world.map.hero().ok_or("no hero")?;
            let cell = g.world.map.cell(x, y).ok_or("no cell")?;
            let steed = cell.glyph.as_ref().ok_or("nothing on the hero's cell")?;
            if steed.flags & mg::RIDDEN == 0 || name_of(g, steed).as_deref() != Some("pony") {
                return Err(format!("the hero's cell shows {steed:?}"));
            }
            let form = g.world.map.hero_form().ok_or("nobody rides")?;
            if name_of(g, form).as_deref() != Some("knight") || form.flags & mg::MALE == 0 {
                return Err(format!("the rider is {form:?}"));
            }
            if g.hero_female != Some(false) {
                return Err(format!("the hero's gender is {:?}", g.hero_female));
            }
            Ok(())
        }),
        Step::Wait(
            "the HUD's portrait and the doll are his, not the pony's",
            |g| {
                let ui = g.ui.as_ref().ok_or("no UI")?;
                let role = ui.hud.role();
                if role.is_some_and(|r| r != "Knight") {
                    return Err(format!("the portrait reads {role:?}"));
                }
                // the doll looks at what the map gives it for the hero
                let map = map_view(g)?;
                let (steed, _) = map.steed().ok_or("no steed")?;
                let view = map.hero_view().ok_or("nothing for the doll")?;
                if view.instance_id() == steed.node.instance_id() {
                    return Err("the doll is given the steed".into());
                }
                Ok(role.is_some())
            },
        ),
        Step::Wait("the camera on the pair", camera_settled),
        Step::Shot("ride-mounted"),
        // as the game's camera has them: nothing of the pair's elsewhere
        Step::Call("the game's own distance", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(11.0, 0.0);
            Ok(())
        }),
        Step::Wait("the camera at its distance", camera_settled),
        Step::Shot("ride-wide"),
        Step::Call("close up again", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(3.4, 0.8);
            Ok(())
        }),
        Step::Wait("the camera on the pair again", camera_settled),
        Step::Call("the inventory open", |g| {
            if !headless() {
                g.ui.as_mut().ok_or("no UI")?.inventory.open();
            }
            Ok(())
        }),
        Step::Wait("the doll shows the rider", |g| {
            // a headless run renders no doll
            let p = panel(g)?;
            Ok(headless() || (p.doll_rendered() && p.doll_drawn().is_some_and(|s| s > 0.05)))
        }),
        Step::ShotIf("ride-doll", |_| Ok(!headless())),
        Step::Call("the inventory away", |g| {
            g.ui.as_mut().ok_or("no UI")?.inventory.close();
            Ok(())
        }),
    ]);
    // a walk, then a gallop: the first step at leisure, the second
    // stopped midway, the third taken while it is (the second is cut
    // short: the pair hurries)
    steps.extend(step());
    steps.extend([
        Step::Wait("the pair stands", |g| Ok(!map_view(g)?.hero_walking())),
        Step::Wait("still in the saddle after a step", rides),
        Step::Call("the next step stopped midway", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.hold_motions(Some(0.5));
            Ok(())
        }),
    ]);
    steps.extend(step());
    steps.extend([
        Step::Wait("the step stopped midway", |g| {
            Ok(map_view(g)?.motions_held())
        }),
        Step::Shot("ride-step"),
    ]);
    steps.extend(step());
    steps.extend([
        Step::Call("on", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.hold_motions(None);
            Ok(())
        }),
        Step::Wait("the pair stands", |g| Ok(!map_view(g)?.hero_walking())),
        Step::Wait("still in the saddle after the ride", rides),
        Step::Call("the steed walked, then galloped", |g| {
            let s = seen()?;
            godot_print!("selftest: riding: the steed's gaits {:?}", s.gaits);
            if s.gaits.first().map(String::as_str) != Some("Walk")
                || !s.gaits.iter().any(|g| g == "Gallop")
            {
                return Err(format!("the steed's steps played {:?}", s.gaits));
            }
            // at rest again: its idle
            match steed_clip(g)?.as_deref() {
                Some("Idle") => Ok(()),
                other => Err(format!("the steed at rest plays {other:?}")),
            }
        }),
        Step::Wait("the camera on the pair", camera_settled),
        Step::Shot("ride-walked"),
    ]);
    // a fight from the saddle: a lizard (sleeping: the pony is young),
    // the long sword
    steps.extend(genesis("sleeping lizard"));
    steps.extend([
        Step::Call("the blows so far", |g| {
            STRIKES.store(map_view(g)?.motion_stats().strikes, Ordering::Relaxed);
            Ok(())
        }),
        // NetHack's F is a prefix: the next command is its direction
        key('F'),
        Step::Request("the fight's direction", command),
        Step::KeyFrom("at the lizard", |g| {
            let k = foe_near(g).ok_or("no lizard next to the pair")?;
            Ok(KeyInput::plain(Key::Char(k)))
        }),
        Step::Wait("the rider's own blow, no head butt of the steed's", |g| {
            let map = map_view(g)?;
            if map.motion_stats().strikes == STRIKES.load(Ordering::Relaxed) {
                return Ok(false);
            }
            let (_, attack) = map.hero_fight_clips();
            let attack = attack.ok_or("the rider has no blow")?;
            if map.hero_clip().as_deref() != Some(attack.as_str()) {
                return Err(format!(
                    "the rider plays {:?}, not {attack}",
                    map.hero_clip()
                ));
            }
            match steed_clip(g)?.as_deref() {
                Some(c) if c.starts_with("Attack") => {
                    Err(format!("the steed plays {c} for the rider's blow"))
                }
                _ => Ok(true),
            }
        }),
        Step::Shot("ride-blow"),
        Step::AnswerUntil('n', "a command after the blow", idle_command),
    ]);
    steps.extend(slay());
    steps.extend([
        Step::Wait("the pair stands", |g| Ok(!map_view(g)?.hero_walking())),
        Step::Wait("in the saddle after the fight", rides),
    ]);
    // the lance couched: applied at a spot two cells on
    steps.extend([
        Step::Call("the lance's way", |g| {
            seen()?.way = None;
            way(g).map(|_| ())
        }),
        key('a'),
        Step::Request("what to apply", getobj),
        Step::KeyFrom("the lance", |g| {
            let items = g.world.inventory.items();
            let lance = items.iter().find(|i| i.text.contains("lance"));
            Ok(KeyInput::plain(Key::Char(lance.ok_or("no lance")?.letter)))
        }),
        // the game's first getpos shows its tip first
        Step::AnswerUntil('n', "where to hit", |g| {
            fail_on_error_screen(g)?;
            Ok(g.world.getpos && matches!(g.pending, Some((_, Prompt::Command))))
        }),
        Step::KeyFrom("a step of the cursor", way_key),
        Step::Request("the cursor a cell off", command),
        Step::KeyFrom("a step of the cursor", way_key),
        Step::Request("the cursor two cells off", command),
        key('.'),
    ]);
    steps.extend(couch("the lance applied is couched", "ride-lance-applied"));
    steps.extend([
        Step::AnswerUntil('n', "a command after the lance", idle_command),
        Step::Wait("the pair stands", |g| Ok(!map_view(g)?.hero_walking())),
        // applied, the lance was wielded: it is in hand from now on
        Step::Wait("in the saddle, the lance in hand", |g| {
            if in_use_at(g)?.is_some() {
                return Ok(false);
            }
            let now = kit(g)?;
            if now[0] != ["lance"] {
                return Err(format!("after the lance's use the hero holds {now:?}"));
            }
            seen()?.kit = now;
            rides(g)
        }),
    ]);
    // wielded, it is couched for a blow too
    steps.extend(genesis("sleeping lizard"));
    steps.extend([
        // NetHack's F is a prefix: the next command is its direction
        key('F'),
        Step::Request("the fight's direction", command),
        Step::KeyFrom("at the lizard", |g| {
            let k = foe_near(g).ok_or("no lizard next to the pair")?;
            Ok(KeyInput::plain(Key::Char(k)))
        }),
    ]);
    steps.extend(couch(
        "the lance wielded is couched for a blow",
        "ride-joust",
    ));
    steps.push(Step::AnswerUntil(
        'n',
        "a command after the joust",
        idle_command,
    ));
    steps.extend(slay());
    // off the pony: a pet with its saddle, the hero on his feet
    steps.extend([
        Step::Wait("the pair stands", |g| Ok(!map_view(g)?.hero_walking())),
        Step::Wait("in the saddle, nothing in use", |g| {
            Ok(in_use_at(g)?.is_none() && rides(g)?)
        }),
    ]);
    steps.extend(ride_command());
    steps.extend([
        Step::AnswerUntil('n', "a command after dismounting", idle_command),
        Step::Wait("the hero on foot", afoot),
        Step::Wait("the pony a pet again, its saddle on", |g| {
            // it had its turn as he got off: by him, or a cell or two on
            let (cell, _) = steed_by(g, "pony", 3).ok_or("no saddled pony by the hero")?;
            let map = map_view(g)?;
            let Some((node, rim)) = map.entity_rim(cell.0, cell.1) else {
                return Ok(false);
            };
            if rim || !node.is_visible() {
                return Err("the pony keeps the hero's rim light".into());
            }
            saddle_on(g, cell)
        }),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Shot("ride-dismounted"),
    ]);
    // a warhorse, a bigger horse, made by him
    steps.extend(genesis("tame saddled warhorse"));
    steps.push(Step::Call("the warhorse next", |_| {
        seen()?.steed = "warhorse";
        Ok(())
    }));
    steps.extend(mount());
    steps.extend([
        Step::Wait("in the warhorse's saddle", rides),
        Step::Call("a higher seat", |g| {
            let map = map_view(g)?;
            let (steed, mount) = map.steed().ok_or("no steed")?;
            let up = mount.at().ok_or("no seat")?.y - steed.node.get_global_position().y;
            godot_print!(
                "selftest: riding: the warhorse's saddle {up:.2} m up, its back {:.2} m broad",
                mount.half_width * 2.0
            );
            if up < 0.9 {
                return Err(format!("the warhorse's seat is {up:.2} m up"));
            }
            Ok(())
        }),
        Step::Call("a step back: the warhorse is tall", |g| {
            g.ui.as_mut().ok_or("no UI")?.map.set_distance(4.4, 1.3);
            Ok(())
        }),
        Step::Wait("the camera on the pair", camera_settled),
        Step::Shot("ride-warhorse"),
    ]);
    // saved and restored in the saddle
    steps.extend([
        key('S'),
        Step::Request(
            "Really save?",
            |p| matches!(p, Prompt::Choice { query, .. } if query.contains("save")),
        ),
        key('y'),
        Step::Wait("\"Continue: wizard\" on the title screen", |g| {
            fail_on_error_screen(g)?;
            let ui = g.ui.as_ref().ok_or("no UI")?;
            Ok(screen(g) == Some("title") && ui.screens.has_button(&continue_label("wizard")))
        }),
        Step::Call("the title's hero is the Knight, not his horse", |g| {
            let ui = g.ui.as_ref().ok_or("no UI")?;
            match ui.map.title_hero() {
                ("Kni", false) => Ok(()),
                other => Err(format!("the title's hero is {other:?}")),
            }
        }),
        Step::Call("another model for the restored hero", |_| {
            seen()?.hero = None;
            Ok(())
        }),
        Step::Push(UiEvent::ContinueGame("wizard".into())),
        Step::AnswerUntil('n', "the first command of the restored game", idle_command),
        Step::Call("close up again", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(4.4, 1.3);
            ui.hud.set_visible(false);
            Ok(())
        }),
        Step::Wait("restored in the saddle", rides),
        Step::Call("the rider is the Knight, not the horse", |g| {
            let form = g.world.map.hero_form().ok_or("nobody rides")?;
            if name_of(g, form).as_deref() != Some("knight") || g.hero_female != Some(false) {
                return Err(format!(
                    "restored, the rider is {form:?}, the hero's gender {:?}",
                    g.hero_female
                ));
            }
            Ok(())
        }),
        Step::Wait("the portrait still the Knight's", |g| {
            let role = g.ui.as_ref().ok_or("no UI")?.hud.role();
            match role {
                Some("Knight") | None => Ok(role.is_some()),
                Some(other) => Err(format!("restored, the portrait reads {other}")),
            }
        }),
        Step::Wait("the camera on the pair", camera_settled),
        Step::Shot("ride-restored"),
        Step::Call("the HUD back", |g| {
            g.ui.as_mut().ok_or("no UI")?.hud.set_visible(true);
            Ok(())
        }),
    ]);
    steps.extend(quit());
    pictures_first(steps)
}
