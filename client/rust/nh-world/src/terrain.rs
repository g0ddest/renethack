use nh_protocol::{Catalog, Glyph, GlyphKind, mg};

use crate::Cell;

/// What a map feature is, for drawing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Terrain {
    Stone,
    Wall,
    Floor,
    DarkFloor,
    Corridor,
    Doorway,
    OpenDoor,
    ClosedDoor,
    /// NetHack draws broken doors as doorways (S_ndoor); nothing maps here yet.
    BrokenDoor,
    Tree,
    IronBars,
    StairsUp,
    StairsDown,
    LadderUp,
    LadderDown,
    Altar,
    Grave,
    Throne,
    Sink,
    Fountain,
    Pool,
    Water,
    Ice,
    Lava,
    LavaWall,
    DrawbridgeDown,
    DrawbridgeUp,
    Air,
    Cloud,
    Trap,
    /// Beams, boomerangs, sparkles, explosions, swallow borders: drawn for a moment.
    Effect,
    Unknown,
}

/// Classify a map symbol by its defsym.h name (`CmapInfo::sym`).
pub fn terrain_of(sym: &str) -> Terrain {
    use Terrain::*;
    match sym {
        "S_stone" => Stone,
        "S_vwall" | "S_hwall" | "S_tlcorn" | "S_trcorn" | "S_blcorn" | "S_brcorn" | "S_crwall"
        | "S_tuwall" | "S_tdwall" | "S_tlwall" | "S_trwall" => Wall,
        "S_ndoor" => Doorway,
        "S_vodoor" | "S_hodoor" => OpenDoor,
        "S_vcdoor" | "S_hcdoor" => ClosedDoor,
        "S_bars" => IronBars,
        "S_tree" => Tree,
        "S_room" | "S_engroom" => Floor,
        "S_darkroom" => DarkFloor,
        "S_corr" | "S_litcorr" | "S_engrcorr" => Corridor,
        "S_upstair" | "S_brupstair" => StairsUp,
        "S_dnstair" | "S_brdnstair" => StairsDown,
        "S_upladder" | "S_brupladder" => LadderUp,
        "S_dnladder" | "S_brdnladder" => LadderDown,
        "S_altar" => Altar,
        "S_grave" => Grave,
        "S_throne" => Throne,
        "S_sink" => Sink,
        "S_fountain" => Fountain,
        "S_pool" => Pool,
        "S_water" => Water,
        "S_ice" => Ice,
        "S_lava" => Lava,
        "S_lavawall" => LavaWall,
        "S_vodbridge" | "S_hodbridge" => DrawbridgeDown,
        "S_vcdbridge" | "S_hcdbridge" => DrawbridgeUp,
        "S_air" => Air,
        "S_cloud" | "S_poisoncloud" => Cloud,
        "S_web" | "S_vibrating_square" | "S_magic_portal" => Trap,
        s if s.ends_with("_trap")
            || matches!(
                s,
                "S_squeaky_board"
                    | "S_land_mine"
                    | "S_pit"
                    | "S_spiked_pit"
                    | "S_hole"
                    | "S_trap_door"
                    | "S_level_teleporter"
                    | "S_trapped_door"
                    | "S_trapped_chest"
            ) =>
        {
            Trap
        }
        "S_vbeam" | "S_hbeam" | "S_lslant" | "S_rslant" | "S_digbeam" | "S_flashbeam"
        | "S_boomleft" | "S_boomright" | "S_ss1" | "S_ss2" | "S_ss3" | "S_ss4" | "S_goodpos" => {
            Effect
        }
        s if s.starts_with("S_sw_") || s.starts_with("S_expl_") => Effect,
        _ => Unknown,
    }
}

/// Cmap indices of brief effects (`Terrain::Effect`), for `MapState::set_effect_cmaps`.
pub fn effect_cmaps(catalog: &Catalog) -> impl Iterator<Item = i32> + '_ {
    catalog
        .cmap
        .iter()
        .filter(|c| terrain_of(&c.sym) == Terrain::Effect)
        .map(|c| c.idx)
}

/// The terrain a cell remembers, if any.
pub fn cell_terrain(cell: &Cell, catalog: &Catalog) -> Option<Terrain> {
    let idx = cell.terrain.as_ref()?.cmap?;
    let info = catalog.cmap.get(usize::try_from(idx).ok()?)?;
    Some(terrain_of(&info.sym))
}

fn monster_name(catalog: &Catalog, mon: Option<i32>, flags: u32) -> Option<String> {
    let m = catalog.monsters.get(usize::try_from(mon?).ok()?)?;
    let gendered = if flags & mg::FEMALE != 0 {
        m.female.as_ref()
    } else if flags & mg::MALE != 0 {
        m.male.as_ref()
    } else {
        None
    };
    Some(gendered.unwrap_or(&m.name).clone())
}

fn describe_entity(g: &Glyph, catalog: &Catalog) -> Option<String> {
    match g.kind {
        GlyphKind::Mon => {
            let name = monster_name(catalog, g.mon, g.flags)?;
            let mut text = if g.flags & mg::HERO != 0 {
                format!("you ({name})")
            } else if g.flags & mg::PET != 0 {
                format!("tame {name}")
            } else {
                name
            };
            if g.flags & mg::INVIS != 0 {
                text = format!("invisible {text}");
            }
            if g.flags & mg::RIDDEN != 0 {
                text.push_str(", ridden");
            }
            if g.flags & mg::DETECT != 0 {
                text.push_str(" (detected)");
            }
            Some(text)
        }
        // appearance only: the tile is shared by everything that looks alike
        GlyphKind::Obj => {
            let t = catalog.object_tiles.iter().find(|t| t.tile == g.tile)?;
            let mut text = format!("{} ({})", t.appearance, t.class_name);
            if g.flags & mg::OBJPILE != 0 {
                text.push_str(" and more");
            }
            Some(text)
        }
        GlyphKind::Body => Some(format!("{} corpse", monster_name(catalog, g.mon, g.flags)?)),
        GlyphKind::Statue => Some(format!(
            "statue of {}",
            monster_name(catalog, g.mon, g.flags)?
        )),
        GlyphKind::Invisible => Some("remembered, unseen, creature".to_string()),
        GlyphKind::Warning => Some(match g.level {
            Some(level) => format!("unseen monster (warning level {level})"),
            None => "unseen monster".to_string(),
        }),
        GlyphKind::Swallow => {
            // swallow glyphs: 8 per monster
            let mon = g.glyph?.checked_sub(catalog.glyphs.swallow)? / 8;
            Some(format!(
                "interior of {}",
                monster_name(catalog, Some(mon), 0)?
            ))
        }
        _ => None,
    }
}

/// Hover text from the catalog only: monster name (pet/peaceful markers from flags),
/// object appearance by tile, map symbol explanation. Peacefulness is not in the
/// glyph flags, so only pets, invisible, ridden and detected monsters are marked.
pub fn describe_cell(cell: &Cell, catalog: &Catalog) -> Option<String> {
    let entity = cell.entity().and_then(|g| describe_entity(g, catalog));
    let terrain = cell
        .terrain
        .as_ref()
        .and_then(|g| g.cmap)
        .and_then(|i| catalog.cmap.get(usize::try_from(i).ok()?))
        .map(|c| c.name.clone())
        .filter(|n| !n.is_empty());
    match (entity, terrain) {
        (Some(e), Some(t)) => Some(format!("{e}\n{t}")),
        (e, t) => e.or(t),
    }
}

#[cfg(test)]
mod tests {
    use nh_protocol::{EngineMsg, parse_line};

    use super::*;
    use crate::map::tests::{floor, glyph, monster};

    fn catalog() -> Catalog {
        let line = include_str!("../tests/data/catalog.jsonl");
        match parse_line(line).unwrap() {
            EngineMsg::Catalog(c) => *c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn terrain_of_every_catalog_sym_is_known() {
        let cat = catalog();
        assert!(cat.cmap.len() > 100);
        for c in &cat.cmap {
            assert_ne!(terrain_of(&c.sym), Terrain::Unknown, "{}", c.sym);
        }
        let of = |name: &str| terrain_of(&cat.cmap.iter().find(|c| c.sym == name).unwrap().sym);
        assert_eq!(of("S_tlcorn"), Terrain::Wall);
        assert_eq!(of("S_bear_trap"), Terrain::Trap);
        assert_eq!(of("S_magic_portal"), Terrain::Trap);
        assert_eq!(of("S_brdnstair"), Terrain::StairsDown);
        assert_eq!(of("S_engrcorr"), Terrain::Corridor);
        assert_eq!(of("S_expl_mc"), Terrain::Effect);
        assert_eq!(terrain_of("S_nonsense"), Terrain::Unknown);
        let mut cell = Cell {
            terrain: Some(floor()),
            ..Cell::default()
        };
        assert_eq!(cell_terrain(&cell, &cat), Some(Terrain::Floor));
        cell.terrain = None;
        assert_eq!(cell_terrain(&cell, &cat), None);
    }

    #[test]
    fn object_hover_uses_appearance_only() {
        let cat = catalog();
        // two bags that look alike share a tile; any glyph number is ignored
        let bag = cat
            .object_tiles
            .iter()
            .find(|t| t.appearance == "bag")
            .unwrap();
        let sack = Glyph {
            tile: bag.tile,
            glyph: Some(3448 + 364),
            ..glyph(GlyphKind::Obj, '(')
        };
        let holding = Glyph {
            glyph: Some(3448 + 365),
            ..sack.clone()
        };
        let cell = |g: &Glyph| Cell {
            glyph: Some(g.clone()),
            bk: None,
            terrain: Some(floor()),
        };
        let text = describe_cell(&cell(&sack), &cat).unwrap();
        assert_eq!(text, "bag (tools)\nfloor of a room");
        assert_eq!(describe_cell(&cell(&holding), &cat).unwrap(), text);
        let pile = Glyph {
            flags: mg::OBJPILE,
            ..sack.clone()
        };
        assert!(
            describe_cell(&cell(&pile), &cat)
                .unwrap()
                .starts_with("bag (tools) and more")
        );
        // an unknown tile says nothing rather than guessing
        let odd = Glyph { tile: 1, ..sack };
        assert_eq!(
            describe_cell(&cell(&odd), &cat).as_deref(),
            Some("floor of a room")
        );
    }

    #[test]
    fn monsters_are_described_by_name_and_flags() {
        let cat = catalog();
        let at = |g: Glyph| {
            describe_cell(
                &Cell {
                    glyph: Some(g),
                    bk: None,
                    terrain: None,
                },
                &cat,
            )
            .unwrap()
        };
        assert_eq!(at(monster(16, mg::PET)), "tame little dog");
        assert_eq!(at(monster(16, mg::DETECT)), "little dog (detected)");
        assert_eq!(at(monster(342, mg::HERO | mg::FEMALE)), "you (valkyrie)");
        let corpse = Glyph {
            mon: Some(322),
            ..glyph(GlyphKind::Body, '%')
        };
        assert_eq!(at(corpse), "newt corpse");
        let engulfer = Glyph {
            glyph: Some(cat.glyphs.swallow + 8 * 16 + 3),
            ..glyph(GlyphKind::Swallow, '|')
        };
        assert_eq!(at(engulfer), "interior of little dog");
        // a malformed glyph number says nothing rather than overflowing
        let odd = Glyph {
            glyph: Some(i32::MIN),
            ..glyph(GlyphKind::Swallow, '|')
        };
        assert_eq!(
            describe_cell(
                &Cell {
                    glyph: Some(odd),
                    bk: None,
                    terrain: None,
                },
                &cat
            ),
            None
        );
        assert_eq!(
            describe_cell(&Cell::default(), &cat),
            None,
            "an unexplored cell"
        );
    }
}
