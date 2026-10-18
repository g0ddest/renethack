//! What the things worn do to the hero's outfit (`worn`), checked on the
//! scene tree of the hero's model: a Healer's stethoscope hangs round the
//! neck, the one fitted to a man on him and to a woman on her, and comes
//! off when it is put down; so does a Tourist's camera on its strap, and
//! the tourists' own (a guide's) never shows on the hero besides. In every
//! clip the hero plays, what hangs round the neck stays on the chest,
//! below the chin, and its strap never comes up across the face. Gloves
//! cover the hands until they
//! are taken off, and the title's hero, dressed again in the role's kit,
//! wears them too; a Knight's and a Barbarian's ring mail is chain mail
//! over the tunic, a Samurai's splint mail is plate, and the tunic is the
//! outfit's own again with the armour off.

use godot::classes::base_material_3d::TextureParam;
use godot::classes::mesh::ArrayType;
use godot::classes::{BaseMaterial3D, MeshInstance3D, Node, Skeleton3D, Skin};
use godot::prelude::*;
use nh_world::{Key, KeyInput, Prompt};

use super::{
    CharacterChoice, Step, camera_settled, command, map_view, quit, smoke_choice, start_as,
};
use crate::art::{GLOVES_NODE, NECK_NODE, OWN_NECK_NODE};
use crate::game::RenethackGame;

/// What the hero's outfit shows of the things worn.
#[derive(Debug, Default, PartialEq)]
struct Looks {
    /// The meshes round the neck, by name.
    neck: Vec<String>,
    /// The texture over the tunic (an armour's, the outfit's own, or none
    /// on a tunic dyed outright), by its file.
    tunic: String,
    /// The gloves' meshes.
    gloves: usize,
    /// The meshes of the look's own thing round the neck that show (a
    /// guide's camera: never on the hero).
    own_neck: usize,
}

fn looks(g: &RenethackGame) -> Result<Looks, String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let node: Gd<Node> = m.node.clone().upcast();
    let mut l = Looks::default();
    for n in node
        .find_children_ex("*")
        .type_("MeshInstance3D")
        .owned(false)
        .done()
        .iter_shared()
    {
        if n.is_queued_for_deletion() {
            continue;
        }
        let Ok(mi) = n.try_cast::<MeshInstance3D>() else {
            continue;
        };
        let name = mi.get_name().to_string();
        if name.starts_with(NECK_NODE) {
            let mesh = mi.get_mesh().map(|m| m.get_name().to_string());
            l.neck.push(mesh.unwrap_or_default());
        } else if name.starts_with(GLOVES_NODE) {
            l.gloves += 1;
        } else if name.starts_with(OWN_NECK_NODE) && mi.is_visible() {
            l.own_neck += 1;
        } else if name.ends_with("_Body") {
            l.tunic = mi
                .get_active_material(0)
                .and_then(|m| m.try_cast::<BaseMaterial3D>().ok())
                .and_then(|m| m.get_texture(TextureParam::ALBEDO))
                .map(|t| t.get_path().to_string())
                .and_then(|p| p.rsplit('/').next().map(str::to_string))
                .unwrap_or_default();
        }
    }
    Ok(l)
}

/// The hero shows these meshes round the neck, this armour's texture over
/// the tunic (None: no armour's) and gloves or bare hands.
fn shows(
    g: &RenethackGame,
    neck: &[&str],
    armour: Option<&str>,
    gloves: bool,
) -> Result<bool, String> {
    let l = looks(g)?;
    // RENETHACK_WORN_LOOK=1: only look (the pictures of another build)
    if std::env::var_os("RENETHACK_WORN_LOOK").is_some() {
        return Ok(true);
    }
    let tunic = match armour {
        Some(a) => l.tunic.starts_with(a),
        None => !["Chainmail", "Metal"]
            .iter()
            .any(|a| l.tunic.starts_with(a)),
    };
    // a mesh is named after its scene and itself
    let hung = l.neck.len() == neck.len() && l.neck.iter().zip(neck).all(|(a, b)| a.starts_with(b));
    Ok(hung && l.own_neck == 0 && tunic && (l.gloves > 0) == gloves)
}

/// The chin in the frame of the `Head` bone: the base characters' at rest
/// (the man's 2 cm below the bone and 8.6 cm forward, the woman's 1 cm and
/// 8.3 cm; the lower is taken).
const CHIN: Vector3 = Vector3::new(0.0, -0.02, 0.083);
/// The face in that frame: within half its breadth, in front of the
/// cheeks, from the mouth up (a strap pressed under the jaw by a bowed head
/// is not across it).
const FACE: Vector3 = Vector3::new(0.05, 0.02, 0.05);
/// What hangs round the neck is at least this far below the chin, along
/// the chest...
const BELOW_CHIN: f32 = 0.02;
/// ...its nearest point no further from the tunic than this (resting on
/// it)...
const ON_CHEST: f32 = 0.03;
/// ...nor its middle from the chest's bone by more than this beyond where
/// it is at rest (it moves with the chest, skinned as the tunic under it
/// is)...
const WITH_CHEST: f32 = 0.025;
/// ...and no point of its strap across the face. Faces of the tunic
/// further than `REACH` from what hangs are not measured.
const REACH: f32 = 0.25;

/// The materials of what hangs on a thing round the neck, and of what it
/// hangs by: a camera on its strap, a stethoscope's chest piece and
/// binaural on its tube.
const CAMERA: (&[&str], &[&str]) = (
    &["CameraBody", "CameraChrome", "CameraLens"],
    &["CameraStrap"],
);
const STETHOSCOPE: (&[&str], &[&str]) = (&["Steel", "EarTips"], &["Tubing"]);

/// A vertex of a skinned mesh: its material's name, where it is at rest,
/// its binds and their weights.
type Vertex = (String, Vector3, Vec<(usize, f32)>);

/// A skinned mesh read once: its vertices, its triangles, and its skin.
struct Skinned {
    skin: Gd<Skin>,
    verts: Vec<Vertex>,
    triangles: Vec<[usize; 3]>,
}

impl Skinned {
    fn of(mi: &Gd<MeshInstance3D>) -> Option<Skinned> {
        let mesh = mi.get_mesh()?;
        let skin = mi.get_skin()?;
        let mut verts = Vec::new();
        let mut triangles = Vec::new();
        for s in 0..mesh.get_surface_count() {
            let material = mi
                .get_active_material(s)
                .map(|m| m.get_name().to_string())
                .unwrap_or_default();
            let arrays = mesh.surface_get_arrays(s);
            let get = |a: ArrayType| arrays.at(a.ord() as usize);
            let at: PackedVector3Array = get(ArrayType::VERTEX).to();
            let bones: PackedInt32Array = get(ArrayType::BONES).try_to().unwrap_or_default();
            let weights: PackedFloat32Array = get(ArrayType::WEIGHTS).try_to().unwrap_or_default();
            let per = bones.len().checked_div(at.len()).unwrap_or(0);
            if per == 0 || weights.len() != bones.len() {
                continue;
            }
            let first = verts.len();
            let index: PackedInt32Array = get(ArrayType::INDEX).try_to().unwrap_or_default();
            let corners: Vec<usize> = match index.len() {
                0 => (0..at.len()).collect(),
                _ => index.as_slice().iter().map(|&i| i as usize).collect(),
            };
            triangles.extend(
                corners
                    .chunks_exact(3)
                    .map(|t| [first + t[0], first + t[1], first + t[2]]),
            );
            for (v, p) in at.as_slice().iter().enumerate() {
                let binds = (v * per..(v + 1) * per)
                    .filter(|&k| weights[k] > 0.0)
                    .map(|k| (bones[k] as usize, weights[k]))
                    .collect();
                verts.push((material.clone(), *p, binds));
            }
        }
        Some(Skinned {
            skin,
            verts,
            triangles,
        })
    }

    /// How far the nearest of `points` is from the mesh as posed
    /// (`posed`), its nearest face (of those within `reach` of the first
    /// point).
    fn distance(&self, posed: &[(&str, Vector3)], points: &[Vector3], reach: f32) -> f32 {
        let Some(&near) = points.first() else {
            return f32::MAX;
        };
        let faces: Vec<[Vector3; 3]> = self
            .triangles
            .iter()
            .map(|t| [posed[t[0]].1, posed[t[1]].1, posed[t[2]].1])
            .filter(|f| f[0].distance_to(near) < reach)
            .collect();
        points
            .iter()
            .flat_map(|p| faces.iter().map(|f| to_triangle(*p, f[0], f[1], f[2])))
            .fold(f32::MAX, f32::min)
    }

    /// Where the skeleton's pose puts each vertex now (its space), by
    /// material.
    fn posed(&self, skeleton: &Gd<Skeleton3D>) -> Vec<(&str, Vector3)> {
        let binds: Vec<Transform3D> = (0..self.skin.get_bind_count())
            .map(|b| {
                let name = self.skin.get_bind_name(b).to_string();
                let bone = if name.is_empty() {
                    self.skin.get_bind_bone(b)
                } else {
                    skeleton.find_bone(name.as_str())
                };
                skeleton.get_bone_global_pose(bone.max(0)) * self.skin.get_bind_pose(b)
            })
            .collect();
        self.verts
            .iter()
            .map(|(m, p, w)| {
                let at = w
                    .iter()
                    .filter_map(|&(b, k)| binds.get(b).map(|t| (*t * *p) * k))
                    .fold(Vector3::ZERO, |a, b| a + b);
                (m.as_str(), at)
            })
            .collect()
    }
}

/// How far `p` is from the triangle `a b c` (its nearest point: inside it,
/// on an edge or a corner).
fn to_triangle(p: Vector3, a: Vector3, b: Vector3, c: Vector3) -> f32 {
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return p.distance_to(a);
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return p.distance_to(b);
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return p.distance_to(a + ab * (d1 / (d1 - d3)));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return p.distance_to(c);
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return p.distance_to(a + ac * (d2 / (d2 - d6)));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return p.distance_to(b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6))));
    }
    let k = 1.0 / (va + vb + vc);
    p.distance_to(a + ab * (vb * k) + ac * (vc * k))
}

/// Seconds of a clip the sweep stops at: its start, its end and four
/// between.
const MOMENTS: usize = 6;

/// A clip at a moment of it.
#[derive(Clone, Default)]
struct Pose(String, f64);

impl std::fmt::Display for Pose {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} at {:.2} s", self.0, self.1)
    }
}

/// The poses the last sweep found the thing round the neck nearest the
/// chin in, furthest off the tunic, and its strap highest in front of the
/// face: their pictures.
static NEAREST: std::sync::Mutex<[Pose; 3]> = std::sync::Mutex::new([
    Pose(String::new(), 0.0),
    Pose(String::new(), 0.0),
    Pose(String::new(), 0.0),
]);

/// The hero held in a pose the last sweep found (`NEAREST`), close: the
/// chest seen from the side, the strap before the face from the front.
fn hold_nearest(g: &mut RenethackGame, which: usize) -> Result<(), String> {
    let Pose(clip, t) = NEAREST.lock().map_err(|e| e.to_string())?[which].clone();
    g.ui.as_mut().ok_or("no UI")?.map.set_distance(1.6, 1.1);
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let mut p = m.player().cloned().ok_or("no animation player")?;
    p.play_ex()
        .name(&StringName::from(clip.as_str()))
        .custom_blend(0.0)
        .done();
    p.seek_ex(t).update(true).done();
    p.pause();
    super::hero::kit_turn(g, if which == 2 { 20.0 } else { 70.0 })
}

/// The hero back on their idle, facing the way the game has them, the
/// camera as it was.
fn let_go(g: &mut RenethackGame) -> Result<(), String> {
    g.ui.as_mut().ok_or("no UI")?.map.set_distance(2.4, 0.6);
    let map = map_view(g)?;
    let (idle, _) = map.hero_fight_clips();
    let m = map.hero_model().ok_or("no hero model")?;
    let mut p = m.player().cloned().ok_or("no animation player")?;
    p.play_ex()
        .name(&StringName::from(idle.unwrap_or("Idle".into()).as_str()))
        .custom_blend(0.0)
        .done();
    super::hero::kit_turn(g, 0.0)
}

/// The sweep, then pictures of the poses it found nearest the chin,
/// furthest off the tunic and with the strap highest before the face.
fn neck_held(what: &'static str, sweep: super::Check2, shots: [&'static str; 3]) -> Vec<Step> {
    vec![
        Step::Call(what, sweep),
        Step::Call("posed as it comes nearest the chin", |g| hold_nearest(g, 0)),
        Step::Wait("a frame", |_| Ok(true)),
        Step::Shot(shots[0]),
        Step::Call("posed as it comes furthest off the chest", |g| {
            hold_nearest(g, 1)
        }),
        Step::Wait("a frame", |_| Ok(true)),
        Step::Shot(shots[1]),
        Step::Call("posed as its strap comes highest before the face", |g| {
            hold_nearest(g, 2)
        }),
        Step::Wait("a frame", |_| Ok(true)),
        Step::Shot(shots[2]),
        Step::Call("back on the idle", let_go),
    ]
}

/// The clips the hero plays: their look's own (the idle, the walk and the
/// run, a blow, a hit, a death), those of what they hold and do (`held`:
/// a torch's or a shield's idle, the blows of every weapon, a potion
/// drunk, a scroll read, a wand zapped, a spell cast, a thing thrown,
/// handled or picked up, a kick), and their states' (caught in a trap,
/// climbing out of a pit, kneeling in prayer, fallen and getting up,
/// slumped). In the saddle the body above the hips plays these too.
fn hero_clips(model: usize) -> Result<Vec<String>, String> {
    let manifest = super::hero::kit_art()?;
    let mut clips: Vec<String> = manifest
        .model_at(model)
        .1
        .anims
        .names()
        .map(str::to_string)
        .collect();
    clips.extend(
        manifest
            .held_rules()
            .clips()
            .into_iter()
            .map(|(lib, clip)| lib.map_or(clip.to_string(), |l| format!("{l}/{clip}"))),
    );
    clips.extend(crate::map_view::STATE_CLIPS.iter().map(|c| c.to_string()));
    clips.sort();
    clips.dedup();
    Ok(clips)
}

/// Every clip the hero plays (`hero_clips`), at `MOMENTS` moments of
/// each: what hangs round the neck rests on the chest and moves with it,
/// stays below the chin, and its strap never comes up across the face. The
/// worst of each is logged; the clip the hero had plays again after.
fn neck_in_every_clip(g: &RenethackGame, (hangs, by): (&[&str], &[&str])) -> Result<(), String> {
    let map = map_view(g)?;
    let m = map.hero_model().ok_or("no hero model")?;
    let wanted = hero_clips(m.model_index())?;
    let node: Gd<Node> = m.node.clone().upcast();
    let skeleton = node
        .find_children_ex("*")
        .type_("Skeleton3D")
        .owned(false)
        .done()
        .iter_shared()
        .find_map(|n| n.try_cast::<Skeleton3D>().ok())
        .ok_or("no skeleton")?;
    let mut player = m.player().cloned().ok_or("no animation player")?;
    let meshes: Vec<Gd<MeshInstance3D>> = skeleton
        .get_children()
        .iter_shared()
        .filter_map(|n| n.try_cast::<MeshInstance3D>().ok())
        .filter(|mi| mi.is_visible() && !mi.is_queued_for_deletion())
        .collect();
    let hung: Vec<Skinned> = meshes
        .iter()
        .filter(|mi| mi.get_name().to_string().starts_with(NECK_NODE))
        .filter_map(Skinned::of)
        .collect();
    let tunic = meshes
        .iter()
        .find(|mi| mi.get_name().to_string().ends_with("_Body"))
        .and_then(Skinned::of)
        .ok_or("no tunic")?;
    if hung.is_empty() {
        return Err("nothing round the neck".into());
    }
    let (chest, head) = (skeleton.find_bone("spine_03"), skeleton.find_bone("Head"));
    if chest < 0 || head < 0 {
        return Err("no chest or head bone".into());
    }
    let was = player.get_current_animation();
    let rest_off = {
        let posed: Vec<(&str, Vector3)> = hung.iter().flat_map(|s| s.posed(&skeleton)).collect();
        let hanging: Vec<Vector3> = posed
            .iter()
            .filter(|(m, _)| hangs.contains(m))
            .map(|(_, p)| *p)
            .collect();
        tunic.distance(&tunic.posed(&skeleton), &hanging, REACH)
    };
    // the worst: the least below the chin, the furthest off the tunic, the
    // most moved from the chest's bone, the strap highest before the face
    let (mut below, mut off, mut moved, mut clear) = (
        (f32::MAX, Pose::default()),
        (0.0f32, Pose::default()),
        (0.0f32, Pose::default()),
        (f32::MIN, Pose::default()),
    );
    let mut rest: Option<f32> = None;
    let mut seen = 0;
    for clip in player.get_animation_list().as_slice() {
        if !wanted.contains(&clip.to_string()) {
            continue;
        }
        let Some(anim) = player.get_animation(&StringName::from(clip)) else {
            continue;
        };
        let length = f64::from(anim.get_length());
        player
            .play_ex()
            .name(&StringName::from(clip))
            .custom_blend(0.0)
            .done();
        for k in 0..MOMENTS {
            let t = length * k as f64 / (MOMENTS - 1) as f64;
            player.seek_ex(t).update(true).done();
            let at = Pose(clip.to_string(), t);
            let c = skeleton.get_bone_global_pose(chest);
            let h = skeleton.get_bone_global_pose(head);
            let up = c.basis.col_b().normalized();
            let along = |p: Vector3| (p - c.origin).dot(up);
            let posed: Vec<(&str, Vector3)> =
                hung.iter().flat_map(|s| s.posed(&skeleton)).collect();
            let hanging: Vec<Vector3> = posed
                .iter()
                .filter(|(m, _)| hangs.contains(m))
                .map(|(_, p)| *p)
                .collect();
            if hanging.is_empty() {
                return Err(format!("nothing of {hangs:?} round the neck"));
            }
            let middle = hanging.iter().fold(Vector3::ZERO, |a, b| a + *b) / hanging.len() as f32;
            let top = hanging.iter().map(|p| along(*p)).fold(f32::MIN, f32::max);
            let gap = along(h * CHIN) - top;
            if gap < below.0 {
                below = (gap, at.clone());
            }
            let skin = tunic.distance(&tunic.posed(&skeleton), &hanging, REACH);
            if skin > off.0 {
                off = (skin, at.clone());
            }
            let from = middle.distance_to(c.origin);
            let drift = (from - *rest.get_or_insert(from)).abs();
            if drift > moved.0 {
                moved = (drift, at.clone());
            }
            // how high the strap comes in front of the face, in the head's
            // frame
            let into_head = h.affine_inverse();
            let rise = posed
                .iter()
                .filter(|(m, _)| by.contains(m))
                .map(|(_, p)| into_head * *p)
                .filter(|p| p.z > FACE.z && p.x.abs() < FACE.x)
                .map(|p| p.y)
                .fold(f32::MIN, f32::max);
            if rise > clear.0 {
                clear = (rise, at.clone());
            }
            seen += 1;
        }
    }
    player
        .play_ex()
        .name(&StringName::from(&was))
        .custom_blend(0.0)
        .done();
    godot_print!(
        "selftest: worn: round the neck in {seen} poses: {:.3} below the chin at least ({}), \
         {:.3} off the tunic at most ({}; {rest_off:.3} as the hero stood), {:.3} moved from the \
         chest at most ({}), the strap in front of the face {:.3} below the mouth at least ({})",
        below.0,
        below.1,
        off.0,
        off.1,
        moved.0,
        moved.1,
        FACE.y - clear.0,
        clear.1
    );
    *NEAREST.lock().map_err(|e| e.to_string())? = [below.1.clone(), off.1.clone(), clear.1.clone()];
    if std::env::var_os("RENETHACK_WORN_LOOK").is_some() {
        return Ok(());
    }
    if below.0 < BELOW_CHIN {
        return Err(format!(
            "{hangs:?} {:.3} below the chin in {}",
            below.0, below.1
        ));
    }
    if off.0 > ON_CHEST {
        return Err(format!("{hangs:?} {:.3} off the chest in {}", off.0, off.1));
    }
    if moved.0 > WITH_CHEST {
        return Err(format!(
            "{hangs:?} moved {:.3} from the chest in {}",
            moved.0, moved.1
        ));
    }
    if clear.0 > FACE.y {
        return Err(format!("{by:?} across the face in {}", clear.1));
    }
    Ok(())
}

fn letter_key(g: &RenethackGame, what: &str) -> Result<KeyInput, String> {
    let items = g.world.inventory.items();
    let item = items.iter().find(|i| i.text.contains(what));
    let item = item.ok_or_else(|| format!("no {what} in the pack"))?;
    Ok(KeyInput::plain(Key::Char(item.letter)))
}

/// A new game as `role`, the camera close on the hero.
fn hero(role: &str, gender: &str, align: &str) -> Vec<Step> {
    let mut steps = start_as(CharacterChoice {
        role: role.into(),
        gender: gender.into(),
        align: align.into(),
        ..smoke_choice()
    });
    steps.extend([
        Step::Wait("the hero on the map", |g| {
            Ok(map_view(g)?.hero_model().is_some())
        }),
        Step::Call("close up, the HUD out of the picture", |g| {
            let ui = g.ui.as_mut().ok_or("no UI")?;
            ui.map.set_distance(2.4, 0.6);
            ui.hud.set_visible(false);
            Ok(())
        }),
    ]);
    steps
}

/// Wait for the look, log it, take its picture.
fn shown(what: &'static str, check: super::Check, shot: &'static str) -> Vec<Step> {
    vec![
        Step::Wait(what, check),
        Step::Wait("the camera on the hero", camera_settled),
        Step::Call("what the outfit shows", |g| {
            // what is worn, by its appearance (the looks follow that alone)
            let cat = g.catalog.as_deref().ok_or("no catalog")?;
            let worn: Vec<&str> = g
                .world
                .inventory
                .items()
                .iter()
                .filter(|i| i.text.contains("worn") || i.text.contains("stethoscope"))
                .filter_map(|i| cat.object_tiles.iter().find(|t| t.tile == i.tile))
                .map(|t| t.appearance.as_str())
                .collect();
            godot_print!("selftest: worn: {:?} of {worn:?}", looks(g)?);
            Ok(())
        }),
        Step::Wait("a frame", |_| Ok(true)),
        Step::Shot(shot),
    ]
}

fn next_hero() -> Vec<Step> {
    let mut steps = vec![Step::Call("the HUD back", |g| {
        g.ui.as_mut().ok_or("no UI")?.hud.set_visible(true);
        Ok(())
    })];
    steps.extend(quit());
    steps.push(Step::Push(super::UiEvent::BackToTitle));
    steps
}

pub(super) fn worn() -> Vec<Step> {
    let mut steps = vec![Step::Call("seed 1", |g| {
        g.seed = Some(1);
        Ok(())
    })];
    // a Healer: the stethoscope fitted to a man, and his gloves
    steps.extend(hero("healer", "male", "neutral"));
    steps.extend(shown(
        "the man's stethoscope round the neck, the hands gloved",
        |g| shows(g, &["StethoscopeMan"], None, true),
        "worn-healer-m",
    ));
    steps.extend(neck_held(
        "the stethoscope on the chest in every clip, below the chin",
        |g| neck_in_every_clip(g, STETHOSCOPE),
        [
            "worn-healer-m-nearest-chin",
            "worn-healer-m-off-chest",
            "worn-healer-m-strap-face",
        ],
    ));
    // put down, it is off the neck; picked up, it is back
    steps.extend([
        super::key('d'),
        Step::Request("what to drop", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the stethoscope's letter", |g| letter_key(g, "stethoscope")),
        Step::Request("a command after dropping it", command),
    ]);
    steps.extend(shown(
        "nothing round the neck",
        |g| shows(g, &[], None, true),
        "worn-healer-m-dropped",
    ));
    steps.extend([
        super::key(','),
        Step::Request("a command after picking it up", command),
        Step::Wait("the stethoscope round the neck again", |g| {
            shows(g, &["StethoscopeMan"], None, true)
        }),
    ]);
    steps.extend(next_hero());
    // the title shows the hero just played, kept and dressed again in the
    // role's kit: the gloves made in the frame the old ones go are under
    // their own name (Godot renames a node whose name a sibling has)
    steps.push(Step::Wait(
        "the title's Healer in gloves under their name",
        |g| {
            let map = map_view(g)?;
            let shown = map.title_drawn() && map.hero_model().is_some_and(|m| !m.is_pending());
            Ok(shown && looks(g)?.gloves > 0)
        },
    ));
    // on a woman, the one fitted to her
    steps.extend(hero("healer", "female", "neutral"));
    steps.extend(shown(
        "the woman's stethoscope round the neck, the hands gloved",
        |g| shows(g, &["StethoscopeWoman"], None, true),
        "worn-healer-f",
    ));
    steps.extend(next_hero());
    // a Tourist: the camera on its strap round the neck (the tourists' own,
    // a guide's, not besides), on the chest in every clip; put down, it is
    // off the neck, picked up, back; a woman wears the one fitted to her
    steps.extend(hero("tourist", "male", "neutral"));
    steps.extend(shown(
        "the man's camera round the neck",
        |g| shows(g, &["CameraMan"], None, false),
        "worn-tourist-m",
    ));
    steps.extend(neck_held(
        "the camera on the chest in every clip, below the chin",
        |g| neck_in_every_clip(g, CAMERA),
        [
            "worn-tourist-m-nearest-chin",
            "worn-tourist-m-off-chest",
            "worn-tourist-m-strap-face",
        ],
    ));
    steps.extend([
        super::key('d'),
        Step::Request("what to drop", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the camera's letter", |g| letter_key(g, "camera")),
        Step::Request("a command after dropping it", command),
    ]);
    steps.extend(shown(
        "no camera round the neck",
        |g| shows(g, &[], None, false),
        "worn-tourist-m-dropped",
    ));
    steps.extend([
        super::key(','),
        Step::Request("a command after picking it up", command),
        Step::Wait("the camera round the neck again", |g| {
            shows(g, &["CameraMan"], None, false)
        }),
    ]);
    steps.extend(next_hero());
    steps.extend(hero("tourist", "female", "neutral"));
    steps.extend(shown(
        "the woman's camera round the neck",
        |g| shows(g, &["CameraWoman"], None, false),
        "worn-tourist-f",
    ));
    steps.extend(neck_held(
        "her camera on the chest in every clip, below the chin",
        |g| neck_in_every_clip(g, CAMERA),
        [
            "worn-tourist-f-nearest-chin",
            "worn-tourist-f-off-chest",
            "worn-tourist-f-strap-face",
        ],
    ));
    steps.extend(next_hero());
    // a Knight: ring mail over the tunic, gloves; the gloves off
    steps.extend(hero("knight", "female", "lawful"));
    steps.extend(shown(
        "chain mail over the tunic, the hands gloved",
        |g| shows(g, &[], Some("Chainmail004"), true),
        "worn-knight-f",
    ));
    steps.extend([
        super::key('T'),
        Step::Request("what to take off", |p| matches!(p, Prompt::FreeKey { .. })),
        Step::KeyFrom("the gloves' letter", |g| letter_key(g, "gloves")),
        Step::Request("a command after taking them off", command),
    ]);
    steps.extend(shown(
        "the hands bare",
        |g| shows(g, &[], Some("Chainmail004"), false),
        "worn-knight-f-bare-hands",
    ));
    steps.extend(next_hero());
    // a Barbarian: ring mail on a body with bare arms; off, the tunic is
    // the outfit's own again
    steps.extend(hero("barbarian", "male", "neutral"));
    steps.extend(shown(
        "chain mail over the tunic",
        |g| shows(g, &[], Some("Chainmail004"), false),
        "worn-barbarian-m",
    ));
    steps.extend([
        super::key('T'),
        Step::Request("a command after taking the mail off", command),
    ]);
    steps.extend(shown(
        "the outfit's own tunic",
        |g| shows(g, &[], None, false),
        "worn-barbarian-m-unarmoured",
    ));
    steps.extend(next_hero());
    // a Samurai: splint mail is plate
    steps.extend(hero("samurai", "male", "lawful"));
    steps.extend(shown(
        "plate over the tunic",
        |g| shows(g, &[], Some("Metal038"), false),
        "worn-samurai-m",
    ));
    steps.extend(next_hero());
    // a Monk: gloves, and no armour
    steps.extend(hero("monk", "female", "neutral"));
    steps.extend(shown(
        "the hands gloved, the outfit's own tunic",
        |g| shows(g, &[], None, true),
        "worn-monk-f",
    ));
    steps.extend(quit());
    steps
}
