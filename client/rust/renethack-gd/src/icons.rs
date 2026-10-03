//! Item icons and the paper doll's slot silhouettes (ui-design §6.5).
//!
//! An item's icon is, first, the baked icon of its appearance tile
//! (`art/icons/items/<tile>.png`, phase I); else a CC0 Flare icon for its
//! class (`art/cc0/icons/flare/`); else a class pictogram painted here
//! from distance fields: embossed metal, glass, parchment or leather with
//! a dark outline, so the grid reads as items before the bake exists.
//! Nothing here looks further than the tile and the class: a potion's
//! tint comes from the colour word of its appearance, which the character
//! sees.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::image::Format;
use godot::classes::{FileAccess, Image, ImageTexture, ResourceLoader, Texture2D};
use godot::prelude::*;
use nh_protocol::Catalog;

/// The side of a painted icon, in pixels.
const SIDE: usize = 128;

/// Where the icon bake (phase I) puts its icons.
const BAKED: &str = "res://art/icons/items";
/// The CC0 Flare icons, when they are vendored.
const FLARE: &str = "res://art/cc0/icons/flare";

thread_local! {
    static ITEMS: RefCell<HashMap<(i32, char), Gd<Texture2D>>> = RefCell::new(HashMap::new());
    static GLYPHS: RefCell<HashMap<Glyph, Gd<Texture2D>>> = RefCell::new(HashMap::new());
    /// Appearance names and class symbols by tile, for tints.
    static LOOKS: RefCell<HashMap<i32, (String, char)>> = RefCell::new(HashMap::new());
}

/// Let the cached textures go while Godot still runs: a Gd dropped with
/// the thread's locals after the engine shut down aborts the process.
pub fn clear() {
    ITEMS.with(|i| i.borrow_mut().clear());
    GLYPHS.with(|g| g.borrow_mut().clear());
    crate::theme::clear_fonts();
}

/// The catalog's appearance names (a potion's colour word); call when it
/// arrives.
pub fn set_catalog(catalog: &Catalog) {
    LOOKS.with(|l| {
        let mut l = l.borrow_mut();
        l.clear();
        for t in &catalog.object_tiles {
            let class = t.class.chars().next().unwrap_or('?');
            l.insert(t.tile, (t.appearance.clone(), class));
        }
    });
    // the items' icons may change with the catalog; the emblems do not
    ITEMS.with(|i| i.borrow_mut().retain(|(_, c), _| *c == 'E'));
}

/// The icon of an item with appearance `tile` and class symbol `class`.
pub fn item_icon(tile: i32, class: char) -> Gd<Texture2D> {
    if let Some(t) = ITEMS.with(|i| i.borrow().get(&(tile, class)).cloned()) {
        return t;
    }
    let look = LOOKS.with(|l| l.borrow().get(&tile).map(|(a, _)| a.clone()));
    // a statue has a tile per monster, none of them an appearance of the
    // catalog: they all show the statue's icon
    let baked = match look {
        None if class == '`' => statue_tile().unwrap_or(tile),
        _ => tile,
    };
    let tex = load_png(&format!("{BAKED}/{baked}.png"))
        .or_else(|| {
            let look = look.as_deref()?;
            match flare_file(class, look) {
                Some(f) => load_png(&format!("{FLARE}/{f}")).map(keyed),
                None => flare_cell(class, look).and_then(sheet_cell),
            }
        })
        .unwrap_or_else(|| {
            painted(
                class_glyph(class),
                Style::Filled(tint(class, look.as_deref())),
            )
        });
    ITEMS.with(|i| i.borrow_mut().insert((tile, class), tex.clone()));
    tex
}

/// The catalog's tile of the appearance "statue".
fn statue_tile() -> Option<i32> {
    LOOKS.with(|l| {
        l.borrow()
            .iter()
            .find(|(_, (look, class))| *class == '`' && look == "statue")
            .map(|(t, _)| *t)
    })
}

/// The class of an appearance tile, as the catalog has it ('?' unknown).
pub fn tile_class(tile: i32) -> char {
    LOOKS.with(|l| l.borrow().get(&tile).map_or('?', |(_, c)| *c))
}

/// A glyph painted as an embossed gold emblem (bar commands, spells).
pub fn emblem(glyph: Glyph) -> Gd<Texture2D> {
    let key = (-1000 - glyph as i32, 'E');
    if let Some(t) = ITEMS.with(|i| i.borrow().get(&key).cloned()) {
        return t;
    }
    let tex = painted(glyph, Style::Filled(STEEL));
    ITEMS.with(|i| i.borrow_mut().insert(key, tex.clone()));
    tex
}

/// A glyph engraved in one colour, outline only (slot silhouettes,
/// filter tabs); `white` so the caller's modulate colours it.
pub fn glyph_icon(glyph: Glyph) -> Gd<Texture2D> {
    if let Some(t) = GLYPHS.with(|g| g.borrow().get(&glyph).cloned()) {
        return t;
    }
    let tex = painted(glyph, Style::Stroke);
    GLYPHS.with(|g| g.borrow_mut().insert(glyph, tex.clone()));
    tex
}

/// An imported texture, else the PNG read from disk.
fn load_png(path: &str) -> Option<Gd<Texture2D>> {
    if ResourceLoader::singleton().exists(path) {
        return godot::tools::try_load::<Texture2D>(path).ok();
    }
    if !FileAccess::file_exists(path) {
        return None;
    }
    let image = Image::load_from_file(path)?;
    ImageTexture::create_from_image(&image).map(|t| t.upcast())
}

/// The Flare icon of an appearance, when the set has one of its kind:
/// weapons and armor by what the appearance shows ("long sword",
/// "crude dagger", "leather armor"), every wand.
fn flare_file(class: char, look: &str) -> Option<&'static str> {
    let has = |w: &[&str]| w.iter().any(|x| look.contains(x));
    let last = look.rsplit(' ').next().unwrap_or(look);
    Some(match class {
        ')' => {
            if has(&["two-handed sword", "tsurugi"]) {
                "weapons-2/0zweihander.png"
            } else if has(&["short sword"]) {
                "osare/shortsword.png"
            } else if has(&["sword", "broadsword", "saber", "scimitar"]) {
                "osare/longsword.png"
            } else if has(&["dagger", "athame", "knife", "stiletto", "scalpel"]) {
                "osare/dagger.png"
            } else if has(&["battle-axe", "double-headed axe"]) {
                "weapons-2/9battle_axe.png"
            } else if last == "axe" {
                "weapons-2/7handaxe.png"
            } else if has(&["war hammer"]) {
                "weapons-2/5warhammer.png"
            } else if last == "mace" {
                "weapons-2/3mace.png"
            } else if has(&["morning star", "flail"]) {
                "weapons-2/2reinforced_club.png"
            } else if last == "club" {
                "weapons-2/1club.png"
            } else if has(&["staff"]) {
                "osare/staff.png"
            } else if has(&["crossbow"]) {
                "osare/greatbow.png"
            } else if last == "bow" || look == "yumi" {
                "osare/longbow.png"
            } else if look == "sling" {
                "osare/slingshot.png"
            } else {
                return None;
            }
        }
        '[' => {
            if has(&["shirt"]) {
                "osare/clothes.png"
            } else if look == "small shield" {
                "osare/buckler.png"
            } else if has(&["shield"]) {
                "osare/shield.png"
            } else if has(&["leather armor", "leather jacket"]) {
                "osare/leather_armor.png"
            } else if has(&["mail", "mithril", "plate"]) {
                "osare/steel_armor.png"
            } else {
                return None;
            }
        }
        '/' => "osare/wand.png",
        _ => return None,
    })
}

/// The cell (row, column) of Flare's armor sheet for an appearance:
/// columns head, body, hands, legs, feet; rows cloth, robe, leather,
/// chain, plate.
fn flare_cell(class: char, look: &str) -> Option<(i32, i32)> {
    if class != '[' {
        return None;
    }
    // words, or their ends ("jackboots"): a "cape" is no "cap"
    let has = |words: &[&str]| {
        look.split([' ', '-']).any(|w| {
            words
                .iter()
                .any(|x| w == *x || (x.len() > 4 && w.ends_with(x)))
        })
    };
    Some(
        if has(&["helm", "hat", "cap", "fedora", "pot", "cornuthaum"]) {
            (4, 0)
        } else if has(&["gloves", "gauntlets"]) {
            (2, 2)
        } else if has(&["boots", "shoes"]) {
            (2, 4)
        } else if has(&[
            "cloak", "cape", "robe", "cope", "apron", "smock", "wrapping", "cloth",
        ]) {
            (1, 1)
        } else {
            return None;
        },
    )
}

/// A 64 px cell of `armor.png`.
fn sheet_cell((row, col): (i32, i32)) -> Option<Gd<Texture2D>> {
    let sheet = load_png(&format!("{FLARE}/armor.png"))?;
    let mut a = godot::classes::AtlasTexture::new_gd();
    a.set_atlas(&sheet);
    a.set_region(Rect2::new(
        Vector2::new(col as f32 * 64.0, row as f32 * 64.0),
        Vector2::new(64.0, 64.0),
    ));
    Some(a.upcast())
}

/// An icon drawn on black (the Flare set): the black made transparent.
fn keyed(tex: Gd<Texture2D>) -> Gd<Texture2D> {
    let Some(mut image) = tex.get_image() else {
        return tex;
    };
    if image.is_compressed() && image.decompress() != godot::global::Error::OK {
        return tex;
    }
    image.convert(Format::RGBA8);
    let corner = image.get_pixel(0, 0);
    if corner.a < 0.5 {
        return tex;
    }
    let (w, h) = (image.get_width(), image.get_height());
    for y in 0..h {
        for x in 0..w {
            let c = image.get_pixel(x, y);
            let v = c.r.max(c.g).max(c.b);
            let a = ((v - 0.04) / 0.14).clamp(0.0, 1.0) * c.a;
            image.set_pixel(x, y, Color::from_rgba(c.r, c.g, c.b, a));
        }
    }
    image.generate_mipmaps();
    ImageTexture::create_from_image(&image).map_or(tex, |t| t.upcast())
}

/// The pictograms and silhouettes this module paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    Sword,
    Cuirass,
    Ring,
    Amulet,
    Sack,
    Drumstick,
    Flask,
    Scroll,
    Book,
    Wand,
    Gem,
    Boulder,
    Ball,
    Chain,
    Drop,
    Coins,
    Helmet,
    Blindfold,
    Cloak,
    Shirt,
    Gloves,
    Boots,
    Lamp,
    Leash,
    Shield,
    Swap,
    Quiver,
    /// A hand: bare hands, fingers, nothing.
    Hand,
    /// "All" in the filter tabs: a grid of four.
    Grid,
    /// "Equipped": a checkmark in a ring.
    Check,
    /// "Suggested": a star.
    Star,
    /// Look here, farlook, terrain.
    Eye,
    /// Search: a magnifying glass.
    Lens,
    /// Rest, wait.
    Moon,
    /// Pray, offer, turn undead.
    Ankh,
    /// Up, down, travel.
    Stairs,
    // the achievements' emblems (`achievement_bake`)
    /// Gehennom: three tongues of fire.
    Flames,
    /// The invocation: a star in a circle.
    Pentagram,
    /// The Elemental Planes: the four elements' triangles.
    Elements,
    /// The Astral Plane: an eight-pointed star.
    Sunburst,
    /// The ascension: an ankh in a ring of rays.
    Ascension,
    /// The Big Room: a great hall and its crowd.
    Hall,
    Crown,
    /// The vibrating square: a square and its ripples.
    Ripples,
    /// The castle's drawbridge, lowered.
    Drawbridge,
    /// The passtune: two notes beamed together.
    Notes,
    Unknown,
}

impl Glyph {
    /// The glyph of this name ("Sword", "Ankh"...).
    pub fn named(name: &str) -> Option<Glyph> {
        use Glyph::*;
        let all = [
            Sword, Cuirass, Ring, Amulet, Sack, Drumstick, Flask, Scroll, Book, Wand, Gem, Boulder,
            Ball, Chain, Drop, Coins, Helmet, Blindfold, Cloak, Shirt, Gloves, Boots, Lamp, Leash,
            Shield, Swap, Quiver, Hand, Grid, Check, Star, Eye, Lens, Moon, Ankh, Stairs, Flames,
            Pentagram, Elements, Sunburst, Ascension, Hall, Crown, Ripples, Drawbridge, Notes,
        ];
        all.into_iter().find(|g| format!("{g:?}") == name)
    }
}

/// Every glyph (the warm-up paints them all).
pub const ALL_GLYPHS: [Glyph; 37] = [
    Glyph::Sword,
    Glyph::Cuirass,
    Glyph::Ring,
    Glyph::Amulet,
    Glyph::Sack,
    Glyph::Drumstick,
    Glyph::Flask,
    Glyph::Scroll,
    Glyph::Book,
    Glyph::Wand,
    Glyph::Gem,
    Glyph::Boulder,
    Glyph::Ball,
    Glyph::Chain,
    Glyph::Drop,
    Glyph::Coins,
    Glyph::Helmet,
    Glyph::Blindfold,
    Glyph::Cloak,
    Glyph::Shirt,
    Glyph::Gloves,
    Glyph::Boots,
    Glyph::Lamp,
    Glyph::Leash,
    Glyph::Shield,
    Glyph::Swap,
    Glyph::Quiver,
    Glyph::Hand,
    Glyph::Grid,
    Glyph::Check,
    Glyph::Star,
    Glyph::Eye,
    Glyph::Lens,
    Glyph::Moon,
    Glyph::Ankh,
    Glyph::Stairs,
    Glyph::Unknown,
];

thread_local! {
    /// How far the warm-up has got through `ALL_GLYPHS` (both styles).
    static WARMED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Paint the next glyph's textures, outline and emblem, ahead of their
/// first use (each is a few milliseconds of CPU: one a frame behind the
/// title screen instead of a dozen in the first frame of a game); false
/// once all are painted.
pub fn warm_step() -> bool {
    let i = WARMED.with(|w| w.get());
    let Some(&g) = ALL_GLYPHS.get(i) else {
        return false;
    };
    glyph_icon(g);
    emblem(g);
    WARMED.with(|w| w.set(i + 1));
    true
}

/// The pictogram of an object class.
pub fn class_glyph(class: char) -> Glyph {
    match class {
        ')' => Glyph::Sword,
        '[' => Glyph::Cuirass,
        '=' => Glyph::Ring,
        '"' => Glyph::Amulet,
        '(' => Glyph::Sack,
        '%' => Glyph::Drumstick,
        '!' => Glyph::Flask,
        '?' => Glyph::Scroll,
        '+' => Glyph::Book,
        '/' => Glyph::Wand,
        '*' => Glyph::Gem,
        '`' => Glyph::Boulder,
        '0' => Glyph::Ball,
        '_' => Glyph::Chain,
        '.' => Glyph::Drop,
        '$' => Glyph::Coins,
        _ => Glyph::Unknown,
    }
}

/// The emblem of an action bar command.
pub fn command_glyph(cmd: nh_world::BarCommand) -> Glyph {
    use nh_world::BarCommand::*;
    match cmd {
        Search => Glyph::Lens,
        LookHere | Farlook | Terrain => Glyph::Eye,
        Rest | Wait | Sit => Glyph::Moon,
        Kick | Jump => Glyph::Boots,
        PickUp | Chat | Untrap => Glyph::Hand,
        Travel | Up | Down | Overview => Glyph::Stairs,
        Pray | Offer | TurnUndead => Glyph::Ankh,
        Loot | Open | Close | Force => Glyph::Sack,
        Pay => Glyph::Coins,
        Fire => Glyph::Quiver,
        Swap | TwoWeapon => Glyph::Swap,
        Enhance => Glyph::Star,
        Ride => Glyph::Leash,
        Attributes => Glyph::Shirt,
        Discoveries | Cast => Glyph::Book,
        Throw => Glyph::Drop,
        Engrave => Glyph::Wand,
    }
}

// ---- materials ----

/// A surface: its colour at the top and at the bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Mat {
    top: [f32; 3],
    bottom: [f32; 3],
    /// Glass: lighter in the middle, a strong highlight.
    glass: bool,
}

const fn mat(top: u32, bottom: u32) -> Mat {
    Mat {
        top: rgb(top),
        bottom: rgb(bottom),
        glass: false,
    }
}

const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
    ]
}

const STEEL: Mat = mat(0xd9dde2, 0x5d6670);
const IRON: Mat = mat(0x8a8680, 0x34302c);
const BRONZE: Mat = mat(0xd4a864, 0x6b4520);
const GOLD: Mat = mat(0xf6d88a, 0x8a5a18);
const WOOD: Mat = mat(0xa8744a, 0x4a2c16);
const LEATHER: Mat = mat(0x9a5a36, 0x3e2012);
const CLOTH: Mat = mat(0xb89a74, 0x5a4430);
const PARCHMENT: Mat = mat(0xf0e2bc, 0xa88c5c);
const CRIMSON: Mat = mat(0xb03a2e, 0x4a1010);
const MEAT: Mat = mat(0xd07a4a, 0x7a2e18);
const BONE: Mat = mat(0xf2ead6, 0xa89a7a);
const STONE: Mat = mat(0xa09a90, 0x4a4640);
const FIRE: Mat = mat(0xffa83a, 0xa81e10);
const FLAME_CORE: Mat = mat(0xfff4b8, 0xffa830);
const WATER: Mat = mat(0x6ab0f4, 0x1a4890);
const AIR: Mat = mat(0xeef4fa, 0x8898aa);
const EARTH: Mat = mat(0xa8844e, 0x4a3018);
const PEARL: Mat = mat(0xffffff, 0xb0c0dc);
const CORK: Mat = mat(0xc89a60, 0x6a4a28);

/// The colour words of appearances ("ruby potion", "blue gem").
const COLOURS: [(&str, u32); 22] = [
    ("ruby", 0xd02030),
    ("red", 0xd02a24),
    ("pink", 0xf07aa8),
    ("orange", 0xf08a20),
    ("yellow", 0xf0d030),
    ("golden", 0xe8b030),
    ("emerald", 0x20b060),
    ("green", 0x3aa040),
    ("cyan", 0x30c0c8),
    ("sky blue", 0x70b8f0),
    ("blue", 0x3060e0),
    ("indigo", 0x4a3aa8),
    ("magenta", 0xc030b0),
    ("purple", 0x8a40c0),
    ("violet", 0x9a58d8),
    ("puce", 0xa05070),
    ("brown", 0x8a5a2a),
    ("black", 0x3a3a44),
    ("white", 0xe8e8ec),
    ("milky", 0xe0dccc),
    ("clear", 0xc8e0e8),
    ("silver", 0xc8ccd4),
];

fn colour_word(look: &str) -> Option<u32> {
    COLOURS
        .iter()
        .find(|(w, _)| look.split(' ').any(|x| x == *w) || look.contains(w))
        .map(|&(_, c)| c)
}

/// The main surface of a class's pictogram; a potion or gem takes the
/// colour its appearance names.
fn tint(class: char, look: Option<&str>) -> Mat {
    let named = look.and_then(colour_word);
    match class {
        '!' => {
            let c = named.unwrap_or(0x9ab8c8);
            let top = rgb(c);
            Mat {
                top: top.map(|v| (v * 0.6 + 0.4).min(1.0)),
                bottom: top.map(|v| v * 0.45),
                glass: true,
            }
        }
        '*' => {
            let c = named.unwrap_or(0xb8c8d8);
            let top = rgb(c);
            Mat {
                top: top.map(|v| (v * 0.7 + 0.3).min(1.0)),
                bottom: top.map(|v| v * 0.35),
                glass: true,
            }
        }
        _ => STEEL,
    }
}

// ---- distance fields (unit square -1..1, y down) ----

type V = (f32, f32);

fn len(p: V) -> f32 {
    (p.0 * p.0 + p.1 * p.1).sqrt()
}

fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1)
}

fn circle(p: V, c: V, r: f32) -> f32 {
    len(sub(p, c)) - r
}

/// An axis-aligned box centred at `c`, half size `h`, corners rounded `r`.
fn rect(p: V, c: V, h: V, r: f32) -> f32 {
    let q = sub(p, c);
    let d = (q.0.abs() - h.0 + r, q.1.abs() - h.1 + r);
    len((d.0.max(0.0), d.1.max(0.0))) + d.0.max(d.1).min(0.0) - r
}

/// A capsule from `a` to `b`, radius `r`.
fn capsule(p: V, a: V, b: V, r: f32) -> f32 {
    let pa = sub(p, a);
    let ba = sub(b, a);
    let h = ((pa.0 * ba.0 + pa.1 * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, 1.0);
    len((pa.0 - ba.0 * h, pa.1 - ba.1 * h)) - r
}

/// A convex or concave polygon (even-odd inside).
fn poly(p: V, pts: &[V]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + n - 1) % n];
        d = d.min(capsule(p, a, b, 0.0));
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
    }
    if inside { -d } else { d }
}

/// An ellipse (approximate: a scaled circle).
fn ellipse(p: V, c: V, r: V) -> f32 {
    let q = ((p.0 - c.0) / r.0, (p.1 - c.1) / r.1);
    (len(q) - 1.0) * r.0.min(r.1)
}

fn ring_sd(p: V, c: V, r: f32, w: f32) -> f32 {
    (len(sub(p, c)) - r).abs() - w
}

fn union(a: f32, b: f32) -> f32 {
    a.min(b)
}

fn cut(a: f32, b: f32) -> f32 {
    a.max(-b)
}

/// A shape of a pictogram and its surface.
type Layer = (Box<dyn Fn(V) -> f32>, Mat);

fn layer(f: impl Fn(V) -> f32 + 'static, m: Mat) -> Layer {
    (Box::new(f), m)
}

/// The layers of a glyph, back to front; `main` is the class's surface
/// where the glyph has one (a potion's liquid, a gem).
fn layers(g: Glyph, main: Mat) -> Vec<Layer> {
    match g {
        Glyph::Sword => vec![
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.2, 0.28),
                            (0.62, -0.62),
                            (0.72, -0.72),
                            (0.6, -0.54),
                            (-0.12, 0.36),
                        ],
                    )
                },
                STEEL,
            ),
            layer(|p| capsule(p, (-0.44, 0.08), (0.04, 0.56), 0.075), GOLD),
            layer(|p| capsule(p, (-0.2, 0.32), (-0.5, 0.62), 0.07), LEATHER),
            layer(|p| circle(p, (-0.58, 0.7), 0.11), GOLD),
        ],
        Glyph::Cuirass => vec![
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.62, -0.62),
                            (-0.28, -0.72),
                            (-0.16, -0.5),
                            (0.16, -0.5),
                            (0.28, -0.72),
                            (0.62, -0.62),
                            (0.56, -0.1),
                            (0.44, 0.0),
                            (0.5, 0.62),
                            (0.0, 0.76),
                            (-0.5, 0.62),
                            (-0.44, 0.0),
                            (-0.56, -0.1),
                        ],
                    )
                },
                STEEL,
            ),
            layer(|p| rect(p, (0.0, 0.08), (0.03, 0.5), 0.02), BRONZE),
            layer(|p| capsule(p, (-0.4, 0.3), (0.4, 0.3), 0.04), BRONZE),
        ],
        Glyph::Ring => vec![
            layer(|p| ring_sd(p, (0.0, 0.18), 0.48, 0.11), GOLD),
            layer(
                |p| {
                    poly(
                        p,
                        &[(0.0, -0.66), (0.26, -0.38), (0.0, -0.18), (-0.26, -0.38)],
                    )
                },
                mat(0xa8e0ff, 0x2a4aa0),
            ),
        ],
        Glyph::Amulet => vec![
            layer(
                |p| {
                    let r = ring_sd(p, (0.0, -0.1), 0.6, 0.045);
                    cut(r, p.1 - 0.05)
                },
                GOLD,
            ),
            layer(|p| circle(p, (0.0, 0.42), 0.3), GOLD),
            layer(|p| circle(p, (0.0, 0.42), 0.17), mat(0xf07060, 0x7a1010)),
        ],
        Glyph::Sack => vec![
            layer(
                |p| {
                    union(
                        ellipse(p, (0.0, 0.26), (0.62, 0.5)),
                        poly(
                            p,
                            &[(-0.2, -0.3), (0.2, -0.3), (0.34, -0.66), (-0.34, -0.66)],
                        ),
                    )
                },
                CLOTH,
            ),
            layer(|p| capsule(p, (-0.3, -0.3), (0.3, -0.3), 0.06), LEATHER),
            layer(|p| capsule(p, (0.2, -0.3), (0.44, -0.06), 0.045), LEATHER),
        ],
        Glyph::Drumstick => vec![
            layer(|p| capsule(p, (0.1, -0.1), (0.52, 0.52), 0.08), BONE),
            layer(|p| circle(p, (0.6, 0.48), 0.1), BONE),
            layer(|p| circle(p, (0.48, 0.62), 0.1), BONE),
            layer(|p| ellipse(p, (-0.14, -0.2), (0.5, 0.42)), MEAT),
        ],
        Glyph::Flask => vec![
            layer(
                |p| {
                    union(
                        circle(p, (0.0, 0.3), 0.48),
                        rect(p, (0.0, -0.3), (0.16, 0.3), 0.04),
                    )
                },
                Mat {
                    top: [0.85, 0.9, 0.95],
                    bottom: [0.35, 0.42, 0.5],
                    glass: true,
                },
            ),
            layer(|p| cut(circle(p, (0.0, 0.3), 0.4), -(p.1 - 0.12)), main),
            layer(|p| rect(p, (0.0, -0.66), (0.2, 0.1), 0.04), CORK),
        ],
        Glyph::Scroll => vec![
            layer(|p| rect(p, (0.0, 0.0), (0.44, 0.52), 0.02), PARCHMENT),
            layer(
                |p| capsule(p, (-0.56, -0.56), (0.56, -0.56), 0.13),
                PARCHMENT,
            ),
            layer(|p| capsule(p, (-0.56, 0.56), (0.56, 0.56), 0.13), PARCHMENT),
            layer(|p| rect(p, (0.0, 0.0), (0.1, 0.1), 0.1), CRIMSON),
        ],
        Glyph::Book => vec![
            layer(|p| rect(p, (0.06, 0.0), (0.5, 0.66), 0.06), CRIMSON),
            layer(|p| rect(p, (-0.44, 0.0), (0.08, 0.66), 0.04), LEATHER),
            layer(
                |p| poly(p, &[(0.1, -0.3), (0.2, -0.06), (0.1, 0.18), (0.0, -0.06)]),
                GOLD,
            ),
            layer(|p| rect(p, (0.56, 0.0), (0.06, 0.14), 0.02), GOLD),
        ],
        Glyph::Wand => vec![
            layer(|p| capsule(p, (-0.6, 0.6), (0.44, -0.44), 0.07), WOOD),
            layer(|p| capsule(p, (0.36, -0.36), (0.5, -0.5), 0.09), GOLD),
            layer(
                |p| {
                    let c = (0.58, -0.58);
                    union(
                        capsule(p, (c.0 - 0.2, c.1), (c.0 + 0.2, c.1), 0.025),
                        capsule(p, (c.0, c.1 - 0.2), (c.0, c.1 + 0.2), 0.025),
                    )
                },
                mat(0xffffff, 0xd0e8ff),
            ),
        ],
        Glyph::Gem => vec![
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.36, -0.5),
                            (0.36, -0.5),
                            (0.66, -0.16),
                            (0.0, 0.66),
                            (-0.66, -0.16),
                        ],
                    )
                },
                main,
            ),
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.2, -0.5),
                            (0.2, -0.5),
                            (0.3, -0.16),
                            (0.0, 0.5),
                            (-0.3, -0.16),
                        ],
                    )
                },
                Mat {
                    top: main.top.map(|v| (v * 1.2).min(1.0)),
                    bottom: main.bottom,
                    glass: true,
                },
            ),
        ],
        Glyph::Boulder => vec![layer(
            |p| {
                poly(
                    p,
                    &[
                        (-0.6, 0.5),
                        (-0.66, -0.06),
                        (-0.36, -0.52),
                        (0.14, -0.6),
                        (0.56, -0.3),
                        (0.66, 0.2),
                        (0.4, 0.58),
                    ],
                )
            },
            STONE,
        )],
        Glyph::Ball => vec![
            layer(|p| circle(p, (0.08, 0.1), 0.54), IRON),
            layer(|p| capsule(p, (-0.66, -0.6), (-0.36, -0.3), 0.06), IRON),
        ],
        Glyph::Chain => vec![
            layer(|p| ring_sd(p, (-0.44, 0.34), 0.22, 0.07), IRON),
            layer(|p| ring_sd(p, (0.0, 0.0), 0.22, 0.07), IRON),
            layer(|p| ring_sd(p, (0.44, -0.34), 0.22, 0.07), IRON),
        ],
        Glyph::Drop => vec![layer(
            |p| {
                union(
                    circle(p, (0.0, 0.24), 0.4),
                    poly(p, &[(0.0, -0.66), (0.36, 0.08), (-0.36, 0.08)]),
                )
            },
            mat(0xa0e070, 0x2a6010),
        )],
        Glyph::Coins => vec![
            layer(|p| ellipse(p, (-0.18, 0.44), (0.44, 0.2)), GOLD),
            layer(|p| ellipse(p, (0.22, 0.24), (0.44, 0.2)), GOLD),
            layer(|p| ellipse(p, (-0.06, 0.0), (0.44, 0.2)), GOLD),
            layer(|p| ellipse(p, (0.1, -0.26), (0.44, 0.2)), GOLD),
        ],
        Glyph::Helmet => vec![
            layer(
                |p| {
                    union(
                        cut(circle(p, (0.0, 0.1), 0.56), p.1 - 0.12),
                        rect(p, (0.0, 0.2), (0.66, 0.08), 0.03),
                    )
                },
                STEEL,
            ),
            layer(|p| rect(p, (0.0, -0.1), (0.05, 0.5), 0.02), BRONZE),
        ],
        Glyph::Blindfold => vec![layer(
            |p| {
                union(
                    rect(p, (0.0, 0.0), (0.66, 0.16), 0.08),
                    capsule(p, (0.6, 0.0), (0.72, 0.5), 0.05),
                )
            },
            CLOTH,
        )],
        Glyph::Cloak => vec![layer(
            |p| {
                poly(
                    p,
                    &[
                        (-0.18, -0.66),
                        (0.18, -0.66),
                        (0.36, -0.44),
                        (0.66, 0.66),
                        (0.0, 0.54),
                        (-0.66, 0.66),
                        (-0.36, -0.44),
                    ],
                )
            },
            CRIMSON,
        )],
        Glyph::Shirt => vec![layer(
            |p| {
                poly(
                    p,
                    &[
                        (-0.2, -0.6),
                        (0.2, -0.6),
                        (0.7, -0.34),
                        (0.54, -0.06),
                        (0.38, -0.14),
                        (0.38, 0.62),
                        (-0.38, 0.62),
                        (-0.38, -0.14),
                        (-0.54, -0.06),
                        (-0.7, -0.34),
                    ],
                )
            },
            CLOTH,
        )],
        Glyph::Gloves => vec![layer(
            |p| {
                let palm = rect(p, (0.0, 0.2), (0.34, 0.3), 0.14);
                let mut d = palm;
                for (i, x) in [-0.24f32, -0.08, 0.08, 0.24].iter().enumerate() {
                    let top = if i == 1 || i == 2 { -0.62 } else { -0.5 };
                    d = union(d, capsule(p, (*x, 0.0), (*x, top), 0.075));
                }
                d = union(d, capsule(p, (-0.3, 0.26), (-0.6, -0.04), 0.08));
                union(d, rect(p, (0.0, 0.6), (0.3, 0.12), 0.03))
            },
            LEATHER,
        )],
        Glyph::Boots => vec![
            // the shaft, the instep and the toe as one leather shape
            layer(
                |p| {
                    let shaft = poly(
                        p,
                        &[
                            (-0.4, -0.66),
                            (0.12, -0.66),
                            (0.1, 0.12),
                            (0.34, 0.24),
                            (0.66, 0.34),
                            (0.7, 0.52),
                            (-0.44, 0.52),
                        ],
                    );
                    smooth(shaft, circle(p, (0.5, 0.42), 0.14), 0.06)
                },
                LEATHER,
            ),
            // the cuff, the sole and the heel
            layer(
                |p| rect(p, (-0.14, -0.6), (0.3, 0.08), 0.03),
                mat(0x6a3e22, 0x2e180a),
            ),
            layer(
                |p| {
                    union(
                        rect(p, (0.12, 0.6), (0.62, 0.07), 0.04),
                        rect(p, (-0.3, 0.6), (0.16, 0.1), 0.02),
                    )
                },
                mat(0x3a2a20, 0x120c08),
            ),
        ],
        Glyph::Lens => vec![
            layer(|p| capsule(p, (0.2, 0.2), (0.62, 0.62), 0.09), WOOD),
            layer(|p| ring_sd(p, (-0.16, -0.16), 0.4, 0.07), BRONZE),
            layer(
                |p| circle(p, (-0.16, -0.16), 0.33),
                Mat {
                    top: [0.8, 0.9, 1.0],
                    bottom: [0.3, 0.4, 0.5],
                    glass: true,
                },
            ),
        ],
        Glyph::Lamp => vec![
            layer(|p| ring_sd(p, (0.0, -0.46), 0.16, 0.04), IRON),
            layer(|p| rect(p, (0.0, 0.1), (0.36, 0.44), 0.08), BRONZE),
            layer(
                |p| ellipse(p, (0.0, 0.06), (0.2, 0.28)),
                mat(0xfff0a0, 0xf08020),
            ),
        ],
        Glyph::Leash => vec![
            // a collar and the rope from it, in three lazy bends
            layer(|p| ring_sd(p, (-0.34, 0.34), 0.26, 0.07), LEATHER),
            layer(
                |p| {
                    union(
                        capsule(p, (-0.14, 0.14), (0.1, -0.2), 0.05),
                        union(
                            capsule(p, (0.1, -0.2), (0.4, -0.18), 0.05),
                            capsule(p, (0.4, -0.18), (0.6, -0.6), 0.05),
                        ),
                    )
                },
                mat(0xc8a878, 0x6a5030),
            ),
            layer(|p| rect(p, (0.6, -0.64), (0.1, 0.06), 0.03), BRONZE),
        ],
        Glyph::Shield => vec![
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.6, -0.62),
                            (0.6, -0.62),
                            (0.56, 0.06),
                            (0.0, 0.72),
                            (-0.56, 0.06),
                        ],
                    )
                },
                WOOD,
            ),
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.4, -0.46),
                            (0.4, -0.46),
                            (0.38, 0.02),
                            (0.0, 0.48),
                            (-0.38, 0.02),
                        ],
                    )
                },
                CRIMSON,
            ),
            layer(|p| circle(p, (0.0, -0.08), 0.12), GOLD),
        ],
        Glyph::Swap => vec![
            layer(|p| capsule(p, (-0.56, 0.56), (0.52, -0.52), 0.06), STEEL),
            layer(|p| capsule(p, (0.56, 0.56), (-0.52, -0.52), 0.06), STEEL),
            layer(|p| capsule(p, (-0.64, 0.22), (-0.22, 0.64), 0.06), GOLD),
            layer(|p| capsule(p, (0.64, 0.22), (0.22, 0.64), 0.06), GOLD),
        ],
        Glyph::Quiver => vec![
            layer(|p| capsule(p, (-0.3, -0.62), (0.08, 0.3), 0.035), WOOD),
            layer(|p| capsule(p, (0.0, -0.68), (0.2, 0.3), 0.035), WOOD),
            layer(
                |p| poly(p, &[(-0.44, -0.72), (-0.18, -0.74), (-0.3, -0.5)]),
                mat(0xe0e0e0, 0x808080),
            ),
            layer(
                |p| poly(p, &[(-0.14, -0.78), (0.12, -0.76), (0.0, -0.54)]),
                mat(0xe0e0e0, 0x808080),
            ),
            layer(
                |p| poly(p, &[(-0.26, 0.0), (0.38, -0.1), (0.46, 0.7), (-0.1, 0.76)]),
                LEATHER,
            ),
        ],
        Glyph::Hand => vec![layer(
            |p| {
                let mut d = rect(p, (0.02, 0.26), (0.32, 0.32), 0.16);
                for (x, top) in [
                    (-0.22f32, -0.36f32),
                    (-0.07, -0.56),
                    (0.08, -0.6),
                    (0.23, -0.46),
                ] {
                    d = union(d, capsule(p, (x, 0.0), (x, top), 0.07));
                }
                union(d, capsule(p, (-0.28, 0.3), (-0.6, 0.02), 0.08))
            },
            mat(0xe8c8a0, 0x9a7050),
        )],
        Glyph::Grid => vec![
            layer(|p| rect(p, (-0.3, -0.3), (0.22, 0.22), 0.04), GOLD),
            layer(|p| rect(p, (0.3, -0.3), (0.22, 0.22), 0.04), GOLD),
            layer(|p| rect(p, (-0.3, 0.3), (0.22, 0.22), 0.04), GOLD),
            layer(|p| rect(p, (0.3, 0.3), (0.22, 0.22), 0.04), GOLD),
        ],
        Glyph::Check => vec![
            layer(|p| ring_sd(p, (0.0, 0.0), 0.6, 0.06), GOLD),
            layer(
                |p| {
                    union(
                        capsule(p, (-0.3, 0.02), (-0.08, 0.26), 0.07),
                        capsule(p, (-0.08, 0.26), (0.34, -0.24), 0.07),
                    )
                },
                GOLD,
            ),
        ],
        Glyph::Star => vec![layer(
            |p| {
                let pts: Vec<V> = (0..10)
                    .map(|i| {
                        let a =
                            std::f32::consts::PI * (i as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
                        let r = if i % 2 == 0 { 0.7 } else { 0.3 };
                        (r * a.cos(), r * a.sin() + 0.06)
                    })
                    .collect();
                poly(p, &pts)
            },
            GOLD,
        )],
        Glyph::Eye => vec![
            layer(
                |p| {
                    union(
                        ellipse(p, (0.0, 0.0), (0.7, 0.36)),
                        capsule(p, (-0.7, 0.0), (0.7, 0.0), 0.02),
                    )
                },
                PARCHMENT,
            ),
            layer(|p| circle(p, (0.0, 0.0), 0.24), mat(0x6a9ad0, 0x1a3a70)),
            layer(|p| circle(p, (0.0, 0.0), 0.1), mat(0x202020, 0x000000)),
        ],
        Glyph::Moon => vec![
            layer(
                |p| cut(circle(p, (0.0, 0.0), 0.6), circle(p, (0.3, -0.18), 0.5)),
                GOLD,
            ),
            layer(|p| circle(p, (0.44, 0.36), 0.08), GOLD),
            layer(|p| circle(p, (0.56, -0.5), 0.06), GOLD),
        ],
        Glyph::Ankh => vec![
            layer(|p| ring_sd(p, (0.0, -0.4), 0.22, 0.07), GOLD),
            layer(|p| rect(p, (0.0, 0.26), (0.07, 0.46), 0.02), GOLD),
            layer(|p| rect(p, (0.0, -0.08), (0.46, 0.07), 0.02), GOLD),
        ],
        Glyph::Stairs => vec![layer(
            |p| {
                poly(
                    p,
                    &[
                        (-0.7, 0.66),
                        (-0.7, 0.24),
                        (-0.24, 0.24),
                        (-0.24, -0.2),
                        (0.22, -0.2),
                        (0.22, -0.64),
                        (0.7, -0.64),
                        (0.7, 0.66),
                    ],
                )
            },
            STONE,
        )],
        Glyph::Flames => {
            // a flame: a drop whose tip rises to a point
            let tongue = |cx: f32, h: f32, w: f32| {
                move |p: V| {
                    let base = circle(p, (cx, 0.62 - w), w);
                    let tip = poly(
                        p,
                        &[
                            (cx - w * 0.97, 0.62 - w),
                            (cx + w * 0.25, 0.62 - w - h),
                            (cx + w * 0.97, 0.62 - w),
                        ],
                    );
                    smooth(base, tip, 0.06)
                }
            };
            vec![
                layer(tongue(-0.44, 0.62, 0.22), FIRE),
                layer(tongue(0.44, 0.58, 0.22), FIRE),
                layer(tongue(0.0, 1.0, 0.32), FIRE),
                layer(tongue(0.0, 0.52, 0.17), FLAME_CORE),
            ]
        }
        Glyph::Pentagram => {
            let pts: Vec<V> = (0..5)
                .map(|k| {
                    let a = (-90.0 + k as f32 * 72.0f32).to_radians();
                    (0.66 * a.cos(), 0.66 * a.sin())
                })
                .collect();
            vec![
                layer(|p| ring_sd(p, (0.0, 0.0), 0.78, 0.06), GOLD),
                layer(
                    move |p| {
                        (0..5)
                            .map(|k| capsule(p, pts[k], pts[(k + 2) % 5], 0.045))
                            .fold(f32::MAX, f32::min)
                    },
                    GOLD,
                ),
            ]
        }
        Glyph::Elements => {
            // the alchemists' triangles: fire and air point up, water and
            // earth down; air and earth are barred
            let tri = |c: V, up: bool, bar: bool| {
                move |p: V| {
                    let (s, (x, y)) = (0.34, c);
                    let k = if up { 1.0 } else { -1.0 };
                    let pts = [
                        (x, y - s * k),
                        (x + s * 0.95, y + s * 0.62 * k),
                        (x - s * 0.95, y + s * 0.62 * k),
                    ];
                    let d = poly(p, &pts).abs() - 0.055;
                    if bar {
                        d.min(capsule(p, (x - s * 0.66, y), (x + s * 0.66, y), 0.05))
                    } else {
                        d
                    }
                }
            };
            vec![
                layer(tri((-0.44, -0.4), true, false), FIRE),
                layer(tri((0.44, -0.4), false, false), WATER),
                layer(tri((-0.44, 0.46), true, true), AIR),
                layer(tri((0.44, 0.46), false, true), EARTH),
            ]
        }
        Glyph::Sunburst => {
            let star: Vec<V> = (0..16)
                .map(|k| {
                    let a = (k as f32 * 22.5 - 90.0f32).to_radians();
                    let r = if k % 2 == 0 { 0.95 } else { 0.34 };
                    (r * a.cos(), r * a.sin())
                })
                .collect();
            vec![
                layer(move |p| poly(p, &star), GOLD),
                layer(|p| circle(p, (0.0, 0.0), 0.2), PEARL),
            ]
        }
        Glyph::Ascension => {
            let mut v = vec![layer(
                |p| {
                    (0..16)
                        .map(|k| {
                            let a = (k as f32 * 22.5f32).to_radians();
                            let (c, s) = (a.cos(), a.sin());
                            capsule(p, (0.66 * c, 0.66 * s), (0.94 * c, 0.94 * s), 0.035)
                        })
                        .fold(f32::MAX, f32::min)
                },
                GOLD,
            )];
            v.extend(layers(Glyph::Ankh, main));
            v
        }
        Glyph::Hall => vec![
            // its walls, a door below
            layer(
                |p| {
                    let walls = rect(p, (0.0, 0.0), (0.86, 0.62), 0.04).abs() - 0.07;
                    cut(walls, rect(p, (0.0, 0.62), (0.16, 0.12), 0.0))
                },
                STONE,
            ),
            layer(
                |p| {
                    [(-0.5, -0.26), (0.5, -0.26), (-0.5, 0.26), (0.5, 0.26)]
                        .into_iter()
                        .map(|c| rect(p, c, (0.08, 0.08), 0.01))
                        .fold(f32::MAX, f32::min)
                },
                STONE,
            ),
            // the crowd
            layer(
                |p| {
                    [
                        (-0.16, -0.12),
                        (0.2, 0.04),
                        (0.02, 0.3),
                        (-0.28, 0.16),
                        (0.12, -0.3),
                    ]
                    .into_iter()
                    .map(|c| circle(p, c, 0.065))
                    .fold(f32::MAX, f32::min)
                },
                CRIMSON,
            ),
        ],
        Glyph::Crown => vec![
            layer(
                |p| {
                    poly(
                        p,
                        &[
                            (-0.72, 0.42),
                            (-0.72, -0.3),
                            (-0.38, 0.06),
                            (0.0, -0.52),
                            (0.38, 0.06),
                            (0.72, -0.3),
                            (0.72, 0.42),
                        ],
                    )
                },
                GOLD,
            ),
            layer(|p| rect(p, (0.0, 0.46), (0.76, 0.13), 0.05), GOLD),
            layer(
                |p| {
                    circle(p, (0.0, -0.6), 0.09)
                        .min(circle(p, (-0.72, -0.38), 0.08))
                        .min(circle(p, (0.72, -0.38), 0.08))
                },
                GOLD,
            ),
            layer(
                |p| {
                    circle(p, (0.0, 0.46), 0.075)
                        .min(circle(p, (-0.42, 0.46), 0.06))
                        .min(circle(p, (0.42, 0.46), 0.06))
                },
                CRIMSON,
            ),
        ],
        Glyph::Ripples => vec![
            layer(
                |p| rect(p, (0.0, 0.0), (0.84, 0.84), 0.12).abs() - 0.03,
                STEEL,
            ),
            layer(
                |p| rect(p, (0.0, 0.0), (0.6, 0.6), 0.09).abs() - 0.035,
                STEEL,
            ),
            layer(
                |p| rect(p, (0.0, 0.0), (0.36, 0.36), 0.06).abs() - 0.04,
                GOLD,
            ),
            layer(|p| rect(p, (0.0, 0.0), (0.14, 0.14), 0.03), GOLD),
        ],
        Glyph::Drawbridge => vec![
            // the gatehouse: two towers with merlons, the arch cut out
            layer(
                |p| {
                    let mut d = rect(p, (-0.56, -0.12), (0.24, 0.62), 0.02)
                        .min(rect(p, (0.56, -0.12), (0.24, 0.62), 0.02))
                        .min(rect(p, (0.0, -0.22), (0.42, 0.42), 0.02));
                    for x in [-0.72, -0.4, 0.4, 0.72] {
                        d = cut(d, rect(p, (x, -0.74), (0.06, 0.08), 0.0));
                    }
                    let arch =
                        rect(p, (0.0, 0.04), (0.22, 0.24), 0.0).min(circle(p, (0.0, -0.2), 0.22));
                    cut(d, arch)
                },
                STONE,
            ),
            // the bridge, let down toward the viewer
            layer(
                |p| poly(p, &[(-0.24, 0.28), (0.24, 0.28), (0.36, 0.9), (-0.36, 0.9)]),
                WOOD,
            ),
            layer(
                |p| {
                    capsule(p, (-0.3, -0.36), (-0.34, 0.66), 0.028).min(capsule(
                        p,
                        (0.3, -0.36),
                        (0.34, 0.66),
                        0.028,
                    ))
                },
                IRON,
            ),
        ],
        Glyph::Notes => vec![
            // two heads, their stems and the beam joining them
            layer(
                |p| {
                    let heads = ellipse(p, (-0.42, 0.52), (0.24, 0.17)).min(ellipse(
                        p,
                        (0.4, 0.36),
                        (0.24, 0.17),
                    ));
                    let stems = rect(p, (-0.21, -0.06), (0.05, 0.56), 0.0).min(rect(
                        p,
                        (0.61, -0.22),
                        (0.05, 0.56),
                        0.0,
                    ));
                    let beam = poly(
                        p,
                        &[(-0.26, -0.58), (0.66, -0.78), (0.66, -0.56), (-0.26, -0.36)],
                    );
                    heads.min(stems).min(beam)
                },
                GOLD,
            ),
        ],
        Glyph::Unknown => vec![layer(|p| circle(p, (0.0, 0.0), 0.5), STONE)],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Style {
    /// Embossed surfaces with an outline and a shadow; the class's main
    /// surface.
    Filled(Mat),
    /// White strokes along the outlines (tinted by the caller).
    Stroke,
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// "Over" compositing of straight-alpha colours.
fn over(dst: [f32; 4], c: [f32; 3], a: f32) -> [f32; 4] {
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let f = |i: usize| (c[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    [f(0), f(1), f(2), out_a]
}

/// The RGBA8 pixels of a glyph, `n`×`n`.
fn paint(g: Glyph, style: Style, n: usize) -> Vec<u8> {
    let main = match style {
        Style::Filled(m) => m,
        Style::Stroke => STEEL,
    };
    let ls = layers(g, main);
    let px = 2.0 / n as f32;
    let cover = |d: f32| (0.5 - d / px).clamp(0.0, 1.0);
    let all = |p: V| ls.iter().map(|(f, _)| f(p)).fold(f32::MAX, f32::min);
    let mut out = Vec::with_capacity(n * n * 4);
    // light from the top left
    let light = (-0.6f32, -0.8f32);
    for y in 0..n {
        for x in 0..n {
            // a little margin: the glyph's unit square is 90 % of the icon
            let p = (
                ((x as f32 + 0.5) * px - 1.0) / 0.9,
                ((y as f32 + 0.5) * px - 1.0) / 0.9,
            );
            let mut c = [0.0f32; 4];
            match style {
                Style::Stroke => {
                    let w = 0.045;
                    let d = ls
                        .iter()
                        .map(|(f, _)| f(p).abs() - w)
                        .fold(f32::MAX, f32::min);
                    let a = cover(d / 0.9);
                    c = [1.0, 1.0, 1.0, a];
                }
                Style::Filled(_) => {
                    // shadow, then the outline, then each surface
                    let s = all((p.0 - 0.05, p.1 - 0.07));
                    let sa = (1.0 - (s / 0.12).clamp(0.0, 1.0)) * 0.55;
                    c = over(c, [0.0, 0.0, 0.0], sa);
                    let d = all(p);
                    let oa = cover((d - 0.045) / 0.9);
                    c = over(c, [0.03, 0.02, 0.015], oa);
                    for (f, m) in &ls {
                        let d = f(p);
                        let a = cover(d / 0.9);
                        if a <= 0.0 {
                            continue;
                        }
                        let e = 0.02;
                        let gx = f((p.0 + e, p.1)) - f((p.0 - e, p.1));
                        let gy = f((p.0, p.1 + e)) - f((p.0, p.1 - e));
                        let gl = (gx * gx + gy * gy).sqrt().max(1e-5);
                        let (nx, ny) = (gx / gl, gy / gl);
                        // how close to the edge, 1 at the rim
                        let rim = 1.0 - (-d / 0.12).clamp(0.0, 1.0);
                        let facing = -(nx * light.0 + ny * light.1);
                        let t = ((p.1 + 0.9) / 1.8).clamp(0.0, 1.0);
                        let mut col = mix(m.top, m.bottom, t);
                        let lit = facing * rim * 0.45;
                        col = col.map(|v| (v * (1.0 + lit)).clamp(0.0, 1.0));
                        if m.glass {
                            // a highlight on the upper left
                            let h = (1.0 - len(sub(p, (-0.2, -0.1))) / 0.5).clamp(0.0, 1.0);
                            col = mix(col, [1.0, 1.0, 1.0], h * h * 0.35);
                        }
                        // a dark line where one surface meets another
                        let seam = ((-d) / 0.035).clamp(0.0, 1.0);
                        col = col.map(|v| v * (0.45 + 0.55 * seam));
                        c = over(c, col, a);
                    }
                }
            }
            out.extend(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

/// A smooth union: shapes that meet flow into each other.
fn smooth(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b + (a - b) * h - k * h * (1.0 - h)
}

/// The paper doll's figure (y -1..1, x about ±0.57), negative inside.
fn figure_sd(p: V) -> f32 {
    let head = circle(p, (0.0, -0.8), 0.12);
    let neck = capsule(p, (0.0, -0.7), (0.0, -0.6), 0.05);
    let torso = poly(
        p,
        &[
            (-0.27, -0.58),
            (0.27, -0.58),
            (0.21, -0.12),
            (0.17, 0.08),
            (-0.17, 0.08),
            (-0.21, -0.12),
        ],
    );
    let mut d = smooth(head, neck, 0.04);
    d = smooth(d, torso, 0.06);
    for s in [-1.0f32, 1.0] {
        let arm = union(
            capsule(p, (0.27 * s, -0.54), (0.36 * s, -0.12), 0.06),
            capsule(p, (0.36 * s, -0.12), (0.4 * s, 0.24), 0.05),
        );
        let hand = circle(p, (0.41 * s, 0.3), 0.06);
        let leg = union(
            capsule(p, (0.1 * s, 0.06), (0.12 * s, 0.52), 0.085),
            capsule(p, (0.12 * s, 0.52), (0.13 * s, 0.88), 0.065),
        );
        let foot = rect(p, (0.16 * s, 0.93), (0.08, 0.035), 0.03);
        d = smooth(d, arm, 0.05);
        d = union(d, hand);
        d = smooth(d, leg, 0.05);
        d = smooth(d, foot, 0.03);
    }
    d
}

/// The engraved figure between the doll's columns: a soft fill, an
/// outline and a plinth; white, for the caller's modulate.
pub fn figure() -> Gd<Texture2D> {
    let (w, h) = (256usize, 448usize);
    let s = 2.0 / h as f32;
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let p = (
                (x as f32 + 0.5 - w as f32 / 2.0) * s,
                (y as f32 + 0.5 - h as f32 / 2.0) * s / 0.95,
            );
            let d = figure_sd(p);
            let fill = (0.5 - d / s).clamp(0.0, 1.0) * (0.16 + 0.12 * (1.0 - (p.1 + 1.0) / 2.0));
            let line = (0.5 - (d.abs() - 0.006) / s).clamp(0.0, 1.0);
            // the plinth: an ellipse under the feet
            let plinth = ellipse(p, (0.0, 0.97), (0.36, 0.035));
            let pl = (0.5 - (plinth.abs() - 0.004) / s).clamp(0.0, 1.0) * 0.8;
            let glow = (1.0 - len(((p.0) / 0.5, (p.1 + 0.1) / 0.9))).clamp(0.0, 1.0) * 0.12;
            let a = fill.max(line).max(pl).max(glow);
            data.extend_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    let image = Image::create_from_data(
        w as i32,
        h as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(data),
    )
    .expect("the figure image");
    ImageTexture::create_from_image(&image)
        .expect("the figure texture")
        .upcast()
}

/// A glyph embossed at `side` pixels, its main surface gold or steel (an
/// achievement's medallion).
pub fn glyph_image(glyph: Glyph, side: usize, gold: bool) -> Option<Gd<Image>> {
    let style = Style::Filled(if gold { GOLD } else { STEEL });
    Image::create_from_data(
        side as i32,
        side as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(paint(glyph, style, side)),
    )
}

fn painted(g: Glyph, style: Style) -> Gd<Texture2D> {
    let data = paint(g, style, SIDE);
    let image = Image::create_from_data(
        SIDE as i32,
        SIDE as i32,
        false,
        Format::RGBA8,
        &PackedByteArray::from(data),
    )
    .expect("an icon image");
    let mut image = image;
    image.generate_mipmaps();
    ImageTexture::create_from_image(&image)
        .expect("an icon texture")
        .upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_pictogram() {
        for c in nh_world::PACK_ORDER.chars() {
            assert_ne!(class_glyph(c), Glyph::Unknown, "class {c:?}");
        }
        assert_eq!(class_glyph('~'), Glyph::Unknown);
    }

    #[test]
    fn a_pictogram_covers_its_middle_and_leaves_the_corners_clear() {
        for g in [
            Glyph::Sword,
            Glyph::Flask,
            Glyph::Ring,
            Glyph::Cuirass,
            Glyph::Coins,
        ] {
            let n = 32;
            let px = paint(g, Style::Filled(STEEL), n);
            let alpha = |x: usize, y: usize| px[(y * n + x) * 4 + 3];
            assert_eq!(alpha(0, 0), 0, "{g:?}");
            let covered = (0..n * n).filter(|i| px[i * 4 + 3] > 128).count();
            assert!(covered > n * n / 10, "{g:?}: {covered} pixels");
        }
    }

    #[test]
    fn potions_take_the_colour_their_appearance_names() {
        let ruby = tint('!', Some("ruby"));
        let clear = tint('!', Some("clear"));
        assert!(ruby.top[0] > ruby.top[2]);
        assert_ne!(ruby, clear);
        assert_eq!(colour_word("sky blue"), Some(0x70b8f0));
        assert_eq!(colour_word("bubbly"), None);
    }

    #[test]
    fn the_default_loadouts_commands_look_different() {
        use nh_world::BarCommand::*;
        let glyphs: Vec<Glyph> = [
            Search, Rest, Kick, PickUp, LookHere, Pray, Enhance, Swap, Fire,
        ]
        .into_iter()
        .map(command_glyph)
        .collect();
        for (i, g) in glyphs.iter().enumerate() {
            assert!(!glyphs[i + 1..].contains(g), "{g:?} twice");
        }
    }

    #[test]
    fn flare_icons_go_by_what_the_appearance_shows() {
        assert_eq!(flare_file(')', "long sword"), Some("osare/longsword.png"));
        assert_eq!(flare_file(')', "crude dagger"), Some("osare/dagger.png"));
        assert_eq!(
            flare_file(')', "crude short sword"),
            Some("osare/shortsword.png")
        );
        assert_eq!(
            flare_file(')', "double-headed axe"),
            Some("weapons-2/9battle_axe.png")
        );
        assert_eq!(flare_file(')', "spear"), None);
        assert_eq!(flare_file('[', "small shield"), Some("osare/buckler.png"));
        assert_eq!(
            flare_file('[', "crystal plate mail"),
            Some("osare/steel_armor.png")
        );
        assert_eq!(flare_file('[', "conical hat"), None);
        assert_eq!(flare_cell('[', "conical hat"), Some((4, 0)));
        assert_eq!(flare_cell('[', "jackboots"), Some((2, 4)));
        assert_eq!(flare_cell(')', "jackboots"), None);
        assert_eq!(flare_file('/', "oak wand"), Some("osare/wand.png"));
        assert_eq!(flare_file('!', "ruby potion"), None);
    }

    #[test]
    fn the_achievement_emblems_cover_their_middle() {
        use Glyph::*;
        for g in [
            Flames, Pentagram, Elements, Sunburst, Ascension, Hall, Crown, Ripples, Drawbridge,
            Notes,
        ] {
            let n = 48;
            let px = paint(g, Style::Filled(GOLD), n);
            let alpha = |x: usize, y: usize| px[(y * n + x) * 4 + 3];
            assert_eq!(alpha(0, 0), 0, "{g:?}");
            let covered = (0..n * n).filter(|i| px[i * 4 + 3] > 128).count();
            assert!(covered > n * n / 8, "{g:?}: {covered} pixels");
        }
    }

    #[test]
    fn glyphs_go_by_their_names() {
        assert_eq!(Glyph::named("Ankh"), Some(Glyph::Ankh));
        assert_eq!(Glyph::named("Drawbridge"), Some(Glyph::Drawbridge));
        assert_eq!(Glyph::named("ankh"), None);
        assert_eq!(Glyph::named("Unknown"), None);
    }

    #[test]
    fn a_polygon_is_negative_inside() {
        let sq = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
        assert!(poly((0.0, 0.0), &sq) < 0.0);
        assert!(poly((0.9, 0.0), &sq) > 0.0);
        assert!((poly((0.9, 0.0), &sq) - 0.4).abs() < 1e-4);
    }
}
