//! `late-monsters`: the big monsters of the late game under the gallery's
//! light, dragons in one picture and the titan, the giants, the mumak, the
//! mastodon and the purple worm in another, hides and coats in a third
//! (the owlbear, trolls, dark dragons), the demons, the demon lords and
//! princes, and the undead and the alien of the human kind (liches, mind
//! flayers, vampires, Medusa), each at the default distance and close up,
//! for sheets before and after a change of their look; and those whose
//! colour tells the player something (a unicorn's alignment, a dragon's
//! breath, a pudding's, a mold's, a light's), in the gallery's light and in
//! the game's. Nothing is checked but that each is drawn; with
//! `--screenshots`, the pictures `late-dragons`, `late-giants`,
//! `late-hides`, `late-demons`, `late-lords`, `late-casters`,
//! `late-colours`, `late-colours-game`, `late-hues`, `late-hues-game` and
//! their `-close` (RENETHACK_LATE=demons,lords: only those).

use super::{Step, camera_settled, quit, start, zoom_by};
use crate::game::RenethackGame;

/// The rows of each picture: the gap between those of a row, and who.
type Rows = &'static [(i32, &'static [&'static str])];

/// Lays a picture's rows out.
type Lay = fn(&mut RenethackGame) -> Result<(), String>;

const DRAGONS: Rows = &[
    (3, &["red dragon", "black dragon", "silver dragon"]),
    (3, &["gold dragon", "blue dragon", "white dragon"]),
];

const GIANTS: Rows = &[
    (3, &["titan", "storm giant", "giant"]),
    (3, &["ettin", "hill giant", "fire giant"]),
    (3, &["mumak", "mastodon", "purple worm"]),
];

const HIDES: Rows = &[
    (3, &["owlbear", "troll", "ice troll"]),
    (3, &["black dragon", "green dragon", "Olog-hai"]),
];

const DEMONS: Rows = &[
    (3, &["imp", "hezrou", "vrock", "nalfeshnee"]),
    (3, &["pit fiend", "balrog", "horned devil", "barbed devil"]),
    (
        3,
        &["ice devil", "bone devil", "amorous demon", "water demon"],
    ),
];

const LORDS: Rows = &[
    (3, &["Juiblex", "Yeenoghu", "Orcus", "Geryon"]),
    (3, &["Dispater", "Baalzebub", "Asmodeus", "Demogorgon"]),
];

const CASTERS: Rows = &[
    (3, &["master lich", "lich", "demilich", "arch-lich"]),
    (
        3,
        &[
            "mind flayer",
            "master mind flayer",
            "vampire leader",
            "Vlad the Impaler",
        ],
    ),
    (3, &["Medusa", "vampire"]),
];

/// Those whose glyph's colour means something, and others NetHack draws
/// black, the unicorns first (a long body's row at the top).
const COLOURS: Rows = &[
    (
        3,
        &["white unicorn", "gray unicorn", "black unicorn", "warg"],
    ),
    (
        2,
        &[
            "black light",
            "yellow light",
            "black pudding",
            "brown pudding",
            "gray ooze",
            "green slime",
        ],
    ),
    (
        2,
        &[
            "black naga",
            "red naga",
            "golden naga",
            "raven",
            "vampire bat",
            "giant beetle",
        ],
    ),
    (
        2,
        &[
            "panther",
            "Uruk-hai",
            "wraith",
            "ghoul",
            "shade",
            "carnivorous ape",
        ],
    ),
];

/// The dragons by colour, young (the grown ones share their colours), and
/// the molds.
const HUES: Rows = &[
    (
        3,
        &[
            "baby gray dragon",
            "baby silver dragon",
            "baby white dragon",
            "baby black dragon",
            "baby blue dragon",
        ],
    ),
    (
        3,
        &[
            "baby red dragon",
            "baby orange dragon",
            "baby yellow dragon",
            "baby gold dragon",
            "baby green dragon",
        ],
    ),
    (
        2,
        &[
            "brown mold",
            "yellow mold",
            "green mold",
            "red mold",
            "violet fungus",
            "blue jelly",
        ],
    ),
];

/// The pilot of generated bodies: today's black dragon and lich, each
/// beside the generated one (worn by the next monster of the row in the
/// shooting manifest: the silver dragon, the demilich, the barbed devil).
pub(super) const GENERATED: Rows = &[
    (4, &["black dragon", "silver dragon"]),
    (3, &["lich", "demilich", "master lich"]),
];

/// And the demon: today's horned devil, the generated one (the balrog's
/// in the shooting manifest), today's pit fiend.
pub(super) const GENERATED_DEMON: Rows = &[(3, &["horned devil", "balrog", "pit fiend"])];

/// The generated bodies in the game's light beside those they replace,
/// a picture a kind (`dark-creatures` with RENETHACK_DARK=<kind>): one of
/// each row, in its middle (the camera's wide angle tilts what stands at
/// the frame's edge), wears the generated body in the shooting manifest:
/// a monster of the kind's size not otherwise in the row.
pub(super) const PILOT: &[(&str, Rows)] = &[
    ("generated", GENERATED),
    ("demon", GENERATED_DEMON),
    ("dragons", &[(4, &["black dragon", "silver dragon"])]),
    (
        "liches",
        &[(2, &["lich", "master lich", "demilich", "arch-lich"])],
    ),
    (
        "demons",
        &[(2, &["horned devil", "pit fiend", "nalfeshnee", "balrog"])],
    ),
    (
        "flayers",
        &[(2, &["mind flayer", "human zombie", "master mind flayer"])],
    ),
    (
        "vampires",
        &[(
            2,
            &[
                "vampire",
                "vampire leader",
                "human mummy",
                "Vlad the Impaler",
            ],
        )],
    ),
    (
        "trolls",
        &[(3, &["troll", "ice troll", "water troll", "rock troll"])],
    ),
    // the kinds on the generated bodies, as the manifest now dresses them
    (
        "kinds-a",
        &[
            (2, &["lich", "demilich", "master lich", "arch-lich"]),
            (
                2,
                &[
                    "mind flayer",
                    "master mind flayer",
                    "vampire",
                    "vampire leader",
                ],
            ),
        ],
    ),
    (
        "kinds-b",
        &[
            (3, &["troll", "ice troll", "rock troll", "water troll"]),
            (3, &["Olog-hai", "pit fiend", "balrog", "Vlad the Impaler"]),
        ],
    ),
    // the generated giant, dwarf and gnome on the hero's skeleton
    (
        "giants-gen",
        &[
            (3, &["giant", "hill giant", "stone giant"]),
            (3, &["fire giant", "frost giant", "ettin"]),
        ],
    ),
    (
        "mines-gen",
        &[
            (2, &["dwarf", "dwarf leader", "gnome", "gnome leader"]),
            (
                2,
                &["dwarf ruler", "gnome ruler", "hobbit", "gnomish wizard"],
            ),
        ],
    ),
    // the generated dog, weighted from the husky's skeleton
    (
        "dogs-gen",
        &[
            (3, &["little dog", "dog", "large dog"]),
            (3, &["jackal", "wolf", "warg"]),
        ],
    ),
    (
        "kinds-c",
        &[(
            2,
            &["horned devil", "barbed devil", "bone devil", "ice devil"],
        )],
    ),
];

static SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub(super) fn late_monsters() -> Vec<Step> {
    let mut steps = start();
    steps.push(Step::Wait("the hero on the map", |g| {
        Ok(g.world.map.hero().is_some())
    }));
    let pictures: [(Lay, &str, &str); 12] = [
        (
            |g| lay_out(g, GENERATED_DEMON),
            "late-demon",
            "late-demon-close",
        ),
        (
            |g| lay_out(g, GENERATED),
            "late-generated",
            "late-generated-close",
        ),
        (
            |g| lay_out(g, DRAGONS),
            "late-dragons",
            "late-dragons-close",
        ),
        (|g| lay_out(g, GIANTS), "late-giants", "late-giants-close"),
        (|g| lay_out(g, HIDES), "late-hides", "late-hides-close"),
        (|g| lay_out(g, DEMONS), "late-demons", "late-demons-close"),
        (|g| lay_out(g, LORDS), "late-lords", "late-lords-close"),
        (
            |g| lay_out(g, CASTERS),
            "late-casters",
            "late-casters-close",
        ),
        (
            |g| lay_out(g, COLOURS),
            "late-colours",
            "late-colours-close",
        ),
        (
            |g| lay_out_lit(g, COLOURS, false),
            "late-colours-game",
            "late-colours-game-close",
        ),
        (|g| lay_out(g, HUES), "late-hues", "late-hues-close"),
        (
            |g| lay_out_lit(g, HUES, false),
            "late-hues-game",
            "late-hues-game-close",
        ),
    ];
    // RENETHACK_LATE=demons,lords: only the pictures so named
    let only = std::env::var("RENETHACK_LATE").ok();
    let wanted = |far: &str| {
        only.as_deref()
            .is_none_or(|o| o.split(',').any(|w| far == format!("late-{w}")))
    };
    for (rows, far, close) in pictures.into_iter().filter(|p| wanted(p.1)) {
        steps.extend([
            Step::Call("lay out the rows", rows),
            Step::Wait("the rows drawn", |g| {
                let drawn = g.ui.as_ref().and_then(|ui| ui.map.drawn_generation());
                Ok(drawn == Some(g.world.map.generation()))
            }),
            Step::Wait("the camera on them", camera_settled),
            Step::Shot(far),
            // (the pilot's pictures at 7 m, the others at 8)
            if far == "late-generated" || far == "late-demon" {
                Step::Call("closer", |g| zoom_by(g, -4.0))
            } else {
                Step::Call("closer", |g| zoom_by(g, -3.0))
            },
            Step::Wait("the camera closer", camera_settled),
            Step::Shot(close),
        ]);
        // RENETHACK_LATE_LIFE=1: five more of the close picture a second
        // apart (`<close>-t<n>`): what stands idle is seen to live
        if std::env::var("RENETHACK_LATE_LIFE").is_ok() {
            for k in 1..=5 {
                let name: &'static str = format!("{close}-t{k}").leak();
                steps.extend([
                    Step::Call("a second begins", |_| {
                        *SINCE.lock().unwrap() = Some(std::time::Instant::now());
                        Ok(())
                    }),
                    Step::Wait("a second on", |_| {
                        Ok(SINCE
                            .lock()
                            .unwrap()
                            .is_some_and(|t| t.elapsed().as_secs_f32() >= 1.0))
                    }),
                    Step::Shot(name),
                ]);
            }
        }
        steps.extend([if far == "late-generated" || far == "late-demon" {
            Step::Call("back", |g| zoom_by(g, 4.0))
        } else {
            Step::Call("back", |g| zoom_by(g, 3.0))
        }]);
    }
    steps.extend(quit());
    steps
}

fn lay_out(g: &mut RenethackGame, rows: Rows) -> Result<(), String> {
    lay_out_lit(g, rows, true)
}

/// The rows under the gallery's light (`showcase`) or the game's.
fn lay_out_lit(g: &mut RenethackGame, rows: Rows, showcase: bool) -> Result<(), String> {
    let cat = g.catalog.clone().ok_or("no catalog")?;
    crate::gallery::lay_out_rows(&mut g.world, &cat, rows)?;
    g.ui.as_mut().ok_or("no UI")?.map.set_showcase(showcase);
    Ok(())
}
