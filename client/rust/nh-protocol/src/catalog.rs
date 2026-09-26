use serde::Deserialize;

/// Static game data, sent once before the first window is created.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Catalog {
    pub glyphs: GlyphOffsets,
    pub tiles: TileRanges,
    pub monsters: Vec<MonsterInfo>,
    /// One entry per object tile, described by appearance only.
    pub object_tiles: Vec<ObjectTile>,
    pub cmap: Vec<CmapInfo>,
    pub roles: Vec<RoleInfo>,
    pub races: Vec<RaceInfo>,
    pub genders: Vec<NamedCode>,
    pub aligns: Vec<NamedCode>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GlyphOffsets {
    pub max: i32,
    pub mon: i32,
    pub pet: i32,
    pub invisible: i32,
    pub detect: i32,
    pub body: i32,
    pub ridden: i32,
    pub obj: i32,
    pub cmap: i32,
    pub zap: i32,
    pub swallow: i32,
    pub explode: i32,
    pub warning: i32,
    pub statue: i32,
    pub unexplored: i32,
    pub nothing: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TileRanges {
    pub last_monster: i32,
    pub last_object: i32,
    pub last_other: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MonsterInfo {
    pub idx: i32,
    pub name: String,
    pub male: Option<String>,
    pub female: Option<String>,
    pub class: String,
    pub class_name: String,
    pub size: String,
    pub color: i32,
    pub light: i32,
    pub body: Vec<String>,
    pub tile_male: i32,
    pub tile_female: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ObjectTile {
    pub tile: i32,
    pub class: String,
    pub class_name: String,
    pub appearance: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CmapInfo {
    pub idx: i32,
    pub name: String,
    pub ch: i32,
    pub color: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RoleInfo {
    pub idx: i32,
    pub name: String,
    pub name_female: Option<String>,
    pub code: String,
    pub combos: Vec<RoleCombo>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RoleCombo {
    pub race: i32,
    pub genders: Vec<i32>,
    pub aligns: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RaceInfo {
    pub idx: i32,
    pub noun: String,
    pub adj: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NamedCode {
    pub idx: i32,
    pub adj: String,
    pub code: String,
}
