//! The art gallery (spec 6.5): pages of monsters, objects and features laid
//! out on a lit stone hall in place of the level, for the `gallery`
//! self-test's screenshots. It only writes the client's own map; the engine
//! never hears of it.

use nh_protocol::{Catalog, Glyph, GlyphKind, mg};
use nh_world::World;

/// One thing on a gallery page.
#[derive(Clone, Copy)]
pub enum Entry {
    /// A monster by name, with glyph flags (MG_FEMALE...).
    Mon(&'static str, u32),
    /// An object tile by class symbol and appearance.
    Obj(&'static str, &'static str),
    /// A pile whose top object is this.
    Pile(&'static str, &'static str),
    /// The corpse of a monster.
    Body(&'static str),
    Statue(&'static str),
    /// A map feature by its defsym name.
    Feature(&'static str),
}

use Entry::*;

const F: u32 = mg::FEMALE;

pub const PAGES: [&[Entry]; 5] = [
    // people and humanoids
    &[
        Mon("human", 0),
        Mon("human", F),
        Mon("elf", 0),
        Mon("Woodland-elf", F),
        Mon("dwarf", 0),
        Mon("hobbit", 0),
        Mon("gnome", 0),
        Mon("gnome leader", 0),
        Mon("Keystone Kop", 0),
        Mon("soldier", 0),
        Mon("watchman", 0),
        Mon("shopkeeper", F),
        Mon("nurse", F),
        Mon("wood nymph", F),
        Mon("leprechaun", 0),
        Mon("human zombie", 0),
        Mon("gnome zombie", 0),
        Mon("human mummy", 0),
        Mon("skeleton", 0),
        Mon("vampire", 0),
        Mon("wraith", 0),
        Mon("ghost", 0),
        Mon("lich", 0),
        Mon("stone golem", 0),
        Mon("iron golem", 0),
        Mon("clay golem", 0),
        Mon("hill giant", 0),
        Mon("ettin", 0),
        Mon("Angel", 0),
        Mon("Death", 0),
        Mon("wizard", 0),
        Mon("knight", 0),
    ],
    // imps, goblinoids, beasts
    &[
        Mon("imp", 0),
        Mon("homunculus", 0),
        Mon("gargoyle", 0),
        Mon("horned devil", 0),
        Mon("balrog", 0),
        Mon("goblin", 0),
        Mon("hobgoblin", 0),
        Mon("hill orc", 0),
        Mon("Uruk-hai", 0),
        Mon("kobold", 0),
        Mon("large kobold", 0),
        Mon("bugbear", 0),
        Mon("ogre", 0),
        Mon("troll", 0),
        Mon("ape", 0),
        Mon("yeti", 0),
        Mon("jackal", 0),
        Mon("coyote", 0),
        Mon("little dog", 0),
        Mon("dog", 0),
        Mon("wolf", 0),
        Mon("warg", 0),
        Mon("hell hound", 0),
        Mon("kitten", 0),
        Mon("tiger", 0),
        Mon("sewer rat", 0),
        Mon("giant rat", 0),
        Mon("rock mole", 0),
        Mon("pony", 0),
        Mon("horse", 0),
        Mon("white unicorn", 0),
        Mon("rothe", 0),
        Mon("mumak", 0),
        Mon("rust monster", 0),
        Mon("plains centaur", 0),
    ],
    // bodies built in code
    &[
        Mon("newt", 0),
        Mon("gecko", 0),
        Mon("crocodile", 0),
        Mon("garter snake", 0),
        Mon("cobra", 0),
        Mon("python", 0),
        Mon("red naga", 0),
        Mon("giant ant", 0),
        Mon("killer bee", 0),
        Mon("giant beetle", 0),
        Mon("cave spider", 0),
        Mon("giant spider", 0),
        Mon("centipede", 0),
        Mon("bat", 0),
        Mon("giant bat", 0),
        Mon("raven", 0),
        Mon("chickatrice", 0),
        Mon("cockatrice", 0),
        Mon("acid blob", 0),
        Mon("gelatinous cube", 0),
        Mon("brown pudding", 0),
        Mon("floating eye", 0),
        Mon("yellow light", 0),
        Mon("flaming sphere", 0),
        Mon("fog cloud", 0),
        Mon("dust vortex", 0),
        Mon("lichen", 0),
        Mon("red mold", 0),
        Mon("violet fungus", 0),
        Mon("baby blue dragon", 0),
        Mon("red dragon", 0),
        Mon("piranha", 0),
        Mon("shark", 0),
        Mon("giant eel", 0),
        Mon("rock piercer", 0),
        Mon("long worm", 0),
        Mon("purple worm", 0),
        Mon("air elemental", 0),
        Mon("earth elemental", 0),
        Mon("xorn", 0),
        Mon("small mimic", 0),
        Mon("grid bug", 0),
    ],
    // objects
    &[
        Obj(")", "long sword"),
        Obj(")", "runed dagger"),
        Obj(")", "axe"),
        Obj(")", "bow"),
        Obj(")", "arrow"),
        Obj(")", "spear"),
        Obj(")", "mace"),
        Obj("[", "large round shield"),
        Obj("[", "plumed helmet"),
        Obj("[", "combat boots"),
        Obj("[", "padded gloves"),
        Obj("[", "hooded cloak"),
        Obj("[", "plate mail"),
        Obj("!", "ruby"),
        Obj("!", "emerald"),
        Obj("!", "milky"),
        Obj("!", "black"),
        Obj("?", "ZELGO MER"),
        Obj("?", "FOOBIE BLETCH"),
        Obj("+", "red"),
        Obj("+", "dark blue"),
        Obj("=", "ruby"),
        Obj("\"", "circular"),
        Obj("/", "oak"),
        Obj("/", "silver"),
        Obj("*", "red"),
        Obj("*", "rock"),
        Obj("$", "gold piece"),
        Obj("`", "boulder"),
        Obj("(", "chest"),
        Obj("(", "large box"),
        Obj("(", "bag"),
        Obj("(", "key"),
        Obj("(", "candle"),
        Obj("(", "brass lantern"),
        Obj("(", "pick-axe"),
        Obj("(", "horn"),
        Obj("%", "apple"),
        Obj("%", "carrot"),
        Obj("%", "egg"),
        Obj("%", "tin"),
        Obj("%", "food ration"),
        Obj("0", "heavy iron ball"),
        Obj("_", "iron chain"),
        Pile("!", "ruby"),
        Body("jackal"),
        Body("human"),
        Statue("dog"),
        Statue("human"),
    ],
    // map features
    &[
        Feature("S_altar"),
        Feature("S_throne"),
        Feature("S_fountain"),
        Feature("S_sink"),
        Feature("S_grave"),
        Feature("S_tree"),
        Feature("S_upstair"),
        Feature("S_dnstair"),
        Feature("S_upladder"),
        Feature("S_dnladder"),
        Feature("S_vcdoor"),
        Feature("S_hodoor"),
        Feature("S_ndoor"),
        Feature("S_bars"),
        Feature("S_pool"),
        Feature("S_lava"),
        Feature("S_ice"),
        Feature("S_hodbridge"),
        Feature("S_hcdbridge"),
        Feature("S_bear_trap"),
        Feature("S_web"),
        Feature("S_corr"),
        Feature("S_engroom"),
        Feature("S_darkroom"),
    ],
];

/// Columns per row of a page, and the spacing of its entries in cells.
const COLUMNS: i32 = 8;
const STEP: i32 = 2;
const X0: i32 = 30;
const Y0: i32 = 3;

fn cmap(cat: &Catalog, sym: &str) -> Result<Glyph, String> {
    let info = cat
        .cmap
        .iter()
        .find(|c| c.sym == sym)
        .ok_or_else(|| format!("no cmap {sym}"))?;
    Ok(Glyph {
        glyph: None,
        ch: info.ch,
        color: info.color,
        flags: 0,
        tile: 0,
        kind: GlyphKind::Cmap,
        mon: None,
        cmap: Some(info.idx),
        level: None,
    })
}

/// A monster's name and sex flag from a name that may end in " (F)" (the
/// female, where a kind has its own look for her).
pub fn named(name: &str) -> (&str, u32) {
    match name.strip_suffix(" (F)") {
        Some(n) => (n, mg::FEMALE),
        None => (name, 0),
    }
}

fn monster(cat: &Catalog, name: &str, flags: u32, kind: GlyphKind) -> Result<Glyph, String> {
    let (name, female) = named(name);
    let flags = flags | female;
    let m = cat
        .monsters
        .iter()
        .find(|m| m.name == name)
        .ok_or_else(|| format!("no monster {name}"))?;
    Ok(Glyph {
        glyph: None,
        ch: m.class.chars().next().map_or(' ' as i32, |c| c as i32),
        color: m.color,
        flags,
        tile: m.tile_male,
        kind,
        mon: Some(m.idx),
        cmap: None,
        level: None,
    })
}

/// A NetHack colour for an appearance (objects carry their colour in the
/// glyph; the gallery has no engine to ask).
fn appearance_color(appearance: &str) -> i32 {
    let words = [
        ("ruby", 1),
        ("red", 1),
        ("emerald", 2),
        ("green", 2),
        ("oak", 3),
        ("bag", 3),
        ("chest", 3),
        ("box", 3),
        ("bow", 3),
        ("blue", 4),
        ("magenta", 5),
        ("cyan", 6),
        ("milky", 15),
        ("white", 15),
        ("black", 8),
        ("gold", 11),
        ("brass", 11),
        ("candle", 15),
        ("silver", 7),
    ];
    words
        .iter()
        .find(|(w, _)| appearance.contains(w))
        .map_or(7, |(_, c)| *c)
}

fn object(cat: &Catalog, class: &str, appearance: &str, pile: bool) -> Result<Glyph, String> {
    let t = cat
        .object_tiles
        .iter()
        .find(|t| t.class == class && t.appearance == appearance)
        .ok_or_else(|| format!("no object {class} {appearance}"))?;
    Ok(Glyph {
        glyph: None,
        ch: class.chars().next().map_or(' ' as i32, |c| c as i32),
        color: appearance_color(appearance),
        flags: if pile { mg::OBJPILE } else { 0 },
        tile: t.tile,
        kind: GlyphKind::Obj,
        mon: None,
        cmap: None,
        level: None,
    })
}

/// One monster alone on a small lit floor, the hero two cells to its side
/// for scale (the `bestiary` self-test's close-ups); the view centres on
/// the monster.
pub fn lay_out_one(world: &mut World, cat: &Catalog, name: &str) -> Result<(i32, i32), String> {
    let map = &mut world.map;
    map.clear();
    let floor = cmap(cat, "S_room")?;
    let (cx, cy) = (40, 10);
    for y in cy - 4..=cy + 4 {
        for x in cx - 6..=cx + 6 {
            map.print(x, y, &floor, None);
        }
    }
    let g = monster(cat, name, 0, GlyphKind::Mon)?;
    map.print(cx, cy, &g, Some(&floor));
    let hero = monster(cat, "valkyrie", mg::HERO | mg::FEMALE, GlyphKind::Mon)?;
    map.print(cx + 2, cy, &hero, Some(&floor));
    world.view_center = Some((cx, cy));
    Ok((cx, cy))
}

/// Rows of monsters side by side under the same light (the `bestiary`
/// self-test's colour rows: those a player tells apart by colour), each
/// row its spacing apart, the hero at the end of the last for scale; the
/// view centres a cell north of the rows, which keeps the far row clear
/// of the HUD's top panels.
pub fn lay_out_rows(
    world: &mut World,
    cat: &Catalog,
    rows: &[(i32, &[&str])],
) -> Result<(i32, i32), String> {
    let map = &mut world.map;
    map.clear();
    let floor = cmap(cat, "S_room")?;
    let (cx, cy) = (40, 10);
    let span = |&(gap, names): &(i32, &[&str])| gap * (names.len() as i32 - 1);
    let wide = rows.iter().map(span).max().unwrap_or(0);
    let top = cy - (rows.len() as i32 - 1);
    let bottom = top + 2 * (rows.len() as i32 - 1);
    for y in top - 2..=bottom + 2 {
        for x in cx - wide / 2 - 2..=cx + wide / 2 + 4 {
            map.print(x, y, &floor, None);
        }
    }
    for (i, row) in rows.iter().enumerate() {
        let left = cx - span(row) / 2;
        for (j, name) in row.1.iter().enumerate() {
            let g = monster(cat, name, 0, GlyphKind::Mon)?;
            map.print(
                left + row.0 * j as i32,
                top + 2 * i as i32,
                &g,
                Some(&floor),
            );
        }
    }
    let hero = monster(cat, "valkyrie", mg::HERO | mg::FEMALE, GlyphKind::Mon)?;
    map.print(cx + wide / 2 + 3, bottom, &hero, Some(&floor));
    world.view_center = Some((cx, cy - 1));
    Ok((cx, cy))
}

/// The whole map at once: rooms of lit and remembered floor between walls,
/// corridors, doors, a few monsters and objects. The heaviest redraw a
/// level can ask for (magic mapping, a return to an explored level).
pub fn lay_out_full(world: &mut World, cat: &Catalog) -> Result<(i32, i32), String> {
    let map = &mut world.map;
    map.clear();
    let floor = cmap(cat, "S_room")?;
    let dark = cmap(cat, "S_darkroom")?;
    let hwall = cmap(cat, "S_hwall")?;
    let vwall = cmap(cat, "S_vwall")?;
    let corr = cmap(cat, "S_corr")?;
    let door = cmap(cat, "S_vcdoor")?;
    for y in 0..nh_world::ROWNO {
        for x in 1..nh_world::COLNO {
            let g = if y % 7 == 0 {
                &hwall
            } else if x % 13 == 0 {
                if y % 7 == 3 { &door } else { &vwall }
            } else if x % 13 == 6 && y % 7 == 3 {
                &corr
            } else if (x / 13 + y / 7) % 2 == 0 {
                &floor
            } else {
                &dark
            };
            map.print(x, y, g, None);
        }
    }
    let things = [
        monster(cat, "jackal", 0, GlyphKind::Mon)?,
        monster(cat, "gnome", 0, GlyphKind::Mon)?,
        monster(cat, "newt", 0, GlyphKind::Mon)?,
        object(cat, "!", "ruby", false)?,
        object(cat, ")", "long sword", false)?,
        object(cat, "$", "gold piece", false)?,
    ];
    for (i, g) in things.iter().enumerate() {
        let i = i as i32;
        map.print(4 + i * 12, 2 + (i % 3) * 7, g, Some(&floor));
    }
    let hero = monster(cat, "valkyrie", mg::HERO | mg::FEMALE, GlyphKind::Mon)?;
    map.print(40, 10, &hero, Some(&floor));
    world.view_center = Some((40, 10));
    Ok((40, 10))
}

/// Replace the map with page `page`: a lit hall, a brick wall behind it,
/// the hero at its south edge. Returns the cell to look at.
pub fn lay_out(world: &mut World, cat: &Catalog, page: usize) -> Result<(i32, i32), String> {
    let entries = PAGES.get(page).ok_or("no such page")?;
    let rows = (entries.len() as i32 + COLUMNS - 1) / COLUMNS;
    let (x1, y1) = (X0 + COLUMNS * STEP, Y0 + rows * STEP + 1);
    let map = &mut world.map;
    map.clear();
    let floor = cmap(cat, "S_room")?;
    let wall = cmap(cat, "S_hwall")?;
    let side = cmap(cat, "S_vwall")?;
    for y in Y0 - 2..=y1 + 1 {
        for x in X0 - 3..=x1 + 2 {
            let g = if y == Y0 - 2 {
                &wall
            } else if x == X0 - 3 || x == x1 + 2 {
                &side
            } else {
                &floor
            };
            map.print(x, y, g, None);
        }
    }
    for (i, e) in entries.iter().enumerate() {
        let (i, n) = (i as i32, COLUMNS);
        let (x, y) = (X0 + (i % n) * STEP, Y0 + (i / n) * STEP);
        let g = match *e {
            Mon(name, flags) => monster(cat, name, flags, GlyphKind::Mon)?,
            Body(name) => monster(cat, name, 0, GlyphKind::Body)?,
            Statue(name) => monster(cat, name, 0, GlyphKind::Statue)?,
            Obj(class, a) => object(cat, class, a, false)?,
            Pile(class, a) => object(cat, class, a, true)?,
            Feature(sym) => cmap(cat, sym)?,
        };
        let bk = (g.kind != GlyphKind::Cmap).then(|| floor.clone());
        map.print(x, y, &g, bk.as_ref());
    }
    let hero = monster(cat, "valkyrie", mg::HERO | mg::FEMALE, GlyphKind::Mon)?;
    let hx = X0 + COLUMNS * STEP / 2 - 1;
    map.print(hx, y1, &hero, Some(&floor));
    let centre = (hx, (Y0 + y1) / 2);
    world.view_center = Some(centre);
    Ok(centre)
}
