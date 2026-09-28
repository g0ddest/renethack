//! The minimap (ui-design §1.5): the known level, 5 px a cell, drawn from
//! the map window's cells only (what the hero saw or remembers). A click
//! walks there like a click on the map.

use godot::classes::control::MouseFilter;
use godot::classes::image::Format;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    CanvasItem, Image, ImageTexture, InputEvent, InputEventMouseButton, TextureRect,
};
use godot::global::MouseButton;
use godot::prelude::*;
use nh_protocol::{Catalog, GlyphKind, mg};
use nh_world::{COLNO, MapState, ROWNO, Terrain, cell_terrain};

use crate::ui_events::{UiEvent, UiQueue, push};

/// Pixels per cell.
pub const CELL: i32 = 5;
pub const WIDTH: i32 = COLNO * CELL;
pub const HEIGHT: i32 = ROWNO * CELL;

type Rgba = [u8; 4];

const HERO: Rgba = [0xe7, 0xc2, 0x7a, 255];
const PET: Rgba = [0xf0, 0x8c, 0xc8, 255];
const HOSTILE: Rgba = [0xff, 0x3b, 0x24, 255];
const OBJECT: Rgba = [0xc9, 0xb4, 0x8a, 255];
const STAIRS: Rgba = [0xff, 0xe2, 0x9a, 255];
const FLOOR: Rgba = [0x5c, 0x4f, 0x3f, 240];

/// A known feature's colour on the minimap; None leaves the cell clear.
pub fn terrain_color(t: Terrain) -> Option<Rgba> {
    use Terrain::*;
    let c = match t {
        Stone | Effect | Unknown => return None,
        Wall => [0x8a, 0x80, 0x72, 255],
        // stairs are a chevron (a mark) on the floor
        Floor | StairsUp | StairsDown | LadderUp | LadderDown => FLOOR,
        DarkFloor => [0x3a, 0x33, 0x2a, 230],
        Corridor => [0x46, 0x3e, 0x33, 235],
        Doorway | BrokenDoor | OpenDoor | ClosedDoor => [0xa8, 0x72, 0x3a, 255],
        Tree => [0x3f, 0x6a, 0x35, 255],
        IronBars => [0x6c, 0x74, 0x7c, 255],
        Altar | Throne | Grave => [0xb8, 0xa8, 0x90, 255],
        Sink | Fountain => [0x5f, 0x9f, 0xd0, 255],
        Pool | Water => [0x26, 0x4f, 0x8c, 255],
        Ice => [0xa6, 0xd2, 0xe0, 255],
        Lava | LavaWall => [0xe0, 0x5a, 0x1c, 255],
        DrawbridgeDown | DrawbridgeUp => [0x7a, 0x55, 0x30, 255],
        Air | Cloud => [0x6e, 0x76, 0x86, 180],
        Trap => [0xb0, 0x3a, 0x8a, 255],
    };
    Some(c)
}

/// What sits on a cell, drawn as a mark over its terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Hero,
    Pet,
    Hostile,
    Object,
    Up,
    Down,
}

/// A 5×5 mark: rows top to bottom, bit 4 the leftmost pixel.
fn mark_bits(m: Mark) -> [u8; 5] {
    match m {
        Mark::Hero | Mark::Pet | Mark::Hostile => [0b01110, 0b11111, 0b11111, 0b11111, 0b01110],
        Mark::Object => [0b00000, 0b00100, 0b01110, 0b00100, 0b00000],
        // chevrons
        Mark::Up => [0b00100, 0b01110, 0b11011, 0b10001, 0b00000],
        Mark::Down => [0b00000, 0b10001, 0b11011, 0b01110, 0b00100],
    }
}

/// The minimap's pixels, RGBA, WIDTH×HEIGHT.
pub fn pixels(map: &MapState, catalog: &Catalog) -> Vec<u8> {
    let mut px = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let mut put = |x: i32, y: i32, c: Rgba| {
        if (0..WIDTH).contains(&x) && (0..HEIGHT).contains(&y) {
            let i = ((y * WIDTH + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&c);
        }
    };
    let mut hero = None;
    for y in 0..ROWNO {
        for x in 1..COLNO {
            let Some(cell) = map.cell(x, y) else {
                continue;
            };
            let terrain = cell_terrain(cell, catalog);
            let (ox, oy) = (x * CELL, y * CELL);
            if let Some(c) = terrain.and_then(terrain_color) {
                // a hairline gap between cells keeps rooms from reading as blobs
                for dy in 0..CELL {
                    for dx in 0..CELL {
                        let edge = dx == CELL - 1 || dy == CELL - 1;
                        let c = if edge && c[3] < 255 {
                            let d = |v: u8| (v as u16 * 4 / 5) as u8;
                            [d(c[0]), d(c[1]), d(c[2]), c[3]]
                        } else {
                            c
                        };
                        put(ox + dx, oy + dy, c);
                    }
                }
            }
            let mark = match (cell.entity(), terrain) {
                (Some(g), _) if g.kind == GlyphKind::Mon && g.flags & mg::HERO != 0 => {
                    hero = Some((x, y));
                    continue;
                }
                (Some(g), _) if g.kind == GlyphKind::Mon && g.flags & mg::PET != 0 => {
                    Some(Mark::Pet)
                }
                (Some(g), _) if g.kind == GlyphKind::Mon => Some(Mark::Hostile),
                (Some(g), _) if matches!(g.kind, GlyphKind::Obj | GlyphKind::Body) => {
                    Some(Mark::Object)
                }
                (_, Some(Terrain::StairsUp | Terrain::LadderUp)) => Some(Mark::Up),
                (_, Some(Terrain::StairsDown | Terrain::LadderDown)) => Some(Mark::Down),
                _ => None,
            };
            if let Some(m) = mark {
                stamp(&mut put, ox, oy, m);
            }
        }
    }
    // the hero last, over everything, with a dark ring so it stands out
    if let Some((x, y)) = hero.or_else(|| map.hero()) {
        let (ox, oy) = (x * CELL, y * CELL);
        for dy in -1..=CELL {
            for dx in -1..=CELL {
                put(ox + dx, oy + dy, [0, 0, 0, 200]);
            }
        }
        stamp(&mut put, ox, oy, Mark::Hero);
    }
    px
}

fn stamp(put: &mut impl FnMut(i32, i32, Rgba), ox: i32, oy: i32, m: Mark) {
    let color = match m {
        Mark::Hero => HERO,
        Mark::Pet => PET,
        Mark::Hostile => HOSTILE,
        Mark::Object => OBJECT,
        Mark::Up | Mark::Down => STAIRS,
    };
    for (dy, row) in mark_bits(m).iter().enumerate() {
        for dx in 0..CELL {
            if row & (1 << (CELL - 1 - dx)) != 0 {
                put(ox + dx, oy + dy as i32, color);
            }
        }
    }
}

/// The map picture and its texture.
pub struct Minimap {
    rect: Gd<TextureRect>,
    texture: Option<Gd<ImageTexture>>,
    /// (map generation, hero, frame count) of the last picture.
    last: Option<(u64, Option<(i32, i32)>)>,
    /// Seconds of the last rebuild.
    built_at: f64,
}

/// Seconds between rebuilds while nothing obvious changed (monsters move,
/// cells are remembered).
const REFRESH: f64 = 0.25;

impl Minimap {
    pub fn new(queue: &UiQueue) -> Minimap {
        let mut rect = TextureRect::new_alloc();
        rect.set_expand_mode(ExpandMode::IGNORE_SIZE);
        rect.set_stretch_mode(StretchMode::SCALE);
        rect.set_custom_minimum_size(Vector2::new(WIDTH as f32, HEIGHT as f32));
        rect.set_mouse_filter(MouseFilter::STOP);
        rect.set_texture_filter(godot::classes::canvas_item::TextureFilter::NEAREST);
        rect.set_tooltip_text("The level as far as you know it. Click: walk there.");
        let q = queue.clone();
        let r = rect.clone();
        rect.signals()
            .gui_input()
            .connect(move |ev: Gd<InputEvent>| {
                let Ok(b) = ev.try_cast::<InputEventMouseButton>() else {
                    return;
                };
                if !b.is_pressed() || b.get_button_index() != MouseButton::LEFT {
                    return;
                }
                let size = r.get_size();
                if size.x <= 0.0 || size.y <= 0.0 {
                    return;
                }
                let p = b.get_position();
                let x = (p.x / size.x * COLNO as f32).floor() as i32;
                let y = (p.y / size.y * ROWNO as f32).floor() as i32;
                push(&q, UiEvent::MapClick { x, y, button: 1 });
            });
        Minimap {
            rect,
            texture: None,
            last: None,
            built_at: f64::NEG_INFINITY,
        }
    }

    pub fn node(&self) -> Gd<TextureRect> {
        self.rect.clone()
    }

    /// Redraw when the level or the hero changed, else a few times a second.
    pub fn sync(&mut self, map: &MapState, catalog: &Catalog, now: f64) {
        let key = (map.generation(), map.hero());
        if self.last == Some(key) && now - self.built_at < REFRESH {
            return;
        }
        self.last = Some(key);
        self.built_at = now;
        let data = PackedByteArray::from(pixels(map, catalog));
        let Some(image) = Image::create_from_data(WIDTH, HEIGHT, false, Format::RGBA8, &data)
        else {
            return;
        };
        match &mut self.texture {
            Some(t) => t.update(&image),
            None => {
                self.texture = ImageTexture::create_from_image(&image);
                if let Some(t) = &self.texture {
                    self.rect.set_texture(t);
                }
            }
        }
    }

    /// Forget the picture (a new game).
    pub fn reset(&mut self) {
        self.last = None;
        self.built_at = f64::NEG_INFINITY;
        self.texture = None;
        self.rect
            .set_texture(Option::<&Gd<godot::classes::Texture2D>>::None);
        self.rect.clone().upcast::<CanvasItem>().queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_rock_is_clear_and_stairs_stand_out() {
        assert_eq!(terrain_color(Terrain::Stone), None);
        assert_eq!(terrain_color(Terrain::Unknown), None);
        // stairs are drawn as a chevron over the floor
        assert_eq!(terrain_color(Terrain::StairsDown), Some(FLOOR));
        let floor = terrain_color(Terrain::Floor).unwrap();
        let wall = terrain_color(Terrain::Wall).unwrap();
        let sum = |c: Rgba| c[0] as u32 + c[1] as u32 + c[2] as u32;
        assert!(sum(wall) > sum(floor), "walls lighter than floors");
    }

    #[test]
    fn marks_are_five_by_five() {
        for m in [Mark::Hero, Mark::Object, Mark::Up, Mark::Down] {
            assert!(mark_bits(m).iter().all(|r| *r < 32));
        }
        assert_eq!(mark_bits(Mark::Up)[0], 0b00100);
    }
}
