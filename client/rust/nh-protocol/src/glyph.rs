use serde::Deserialize;

/// What a map cell shows, as classified by the engine.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Glyph {
    /// Raw glyph number. Absent for objects on purpose: it would reveal the
    /// true object type; use `tile` (the appearance) instead.
    #[serde(default)]
    pub glyph: Option<i32>,
    pub ch: i32,
    pub color: i32,
    pub flags: u32,
    pub tile: i32,
    pub kind: GlyphKind,
    /// Monster index for `Mon`, `Body` and `Statue`.
    #[serde(default)]
    pub mon: Option<i32>,
    /// defsyms index for `Cmap`.
    #[serde(default)]
    pub cmap: Option<i32>,
    /// Warning level 0..5 for `Warning`.
    #[serde(default)]
    pub level: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GlyphKind {
    Mon,
    Obj,
    Cmap,
    Zap,
    Explosion,
    Swallow,
    Warning,
    Invisible,
    Body,
    Statue,
    Unexplored,
    Nothing,
    Other,
}

/// `glyph_info.gm.glyphflags` bits (include/display.h, MG_*).
pub mod mg {
    pub const HERO: u32 = 0x00001;
    pub const CORPSE: u32 = 0x00002;
    pub const INVIS: u32 = 0x00004;
    pub const DETECT: u32 = 0x00008;
    pub const PET: u32 = 0x00010;
    pub const RIDDEN: u32 = 0x00020;
    pub const STATUE: u32 = 0x00040;
    pub const OBJPILE: u32 = 0x00080;
    pub const MALE: u32 = 0x01000;
    pub const FEMALE: u32 = 0x02000;
}
