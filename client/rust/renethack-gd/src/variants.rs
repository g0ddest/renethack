//! One of each pipeline variant a game draws, made under the start-up veil
//! by the rehearsal. Godot compiles a render pipeline for each shader over
//! each vertex layout it draws it with, for each pass, in the background
//! from when a thing comes into the scene (shown or not); its first frame
//! waits for what is not done yet (in a game, the first sight of something
//! new took 80–200 ms). The list (`art/variants.txt`: the census of what
//! the game draws, `census.rs`, merged by `tools/variants.py`) has a
//! variant a line: a shader (a standard material's settings, or a shader's
//! path) over a layout (a mesh's arrays and primitive, skinned or not), a
//! 3D label's or sprite's look, or a canvas item's shader.
//!
//! Each shader gets a material, kept for the session: Godot frees a
//! standard material's shader with the last material of its settings, and
//! its pipelines with it. Each layout gets a triangle to carry them,
//! hidden; the canvas items are drawn (a canvas item's pipeline is built
//! when it is drawn), covered by the veil. A game started before they were
//! all made makes the rest in its first frames. They are made again, all
//! but the canvas items, when the graphics change: Godot builds pipelines
//! for the window's MSAA and for the passes the settings draw.

use std::collections::HashMap;

use godot::classes::base_material_3d::{
    AlphaAntiAliasing, BillboardMode, BlendMode, CullMode, DepthDrawMode, DepthTest, DetailUv,
    DiffuseMode, DistanceFadeMode, EmissionOperator, Feature, Flags, ShadingMode, SpecularMode,
    StencilCompare, StencilMode, TextureChannel, TextureFilter, Transparency,
};
use godot::classes::image::Format as ImageFormat;
use godot::classes::mesh::{ArrayFormat, ArrayType, PrimitiveType};
use godot::classes::{
    AnimatedSprite3D, ArrayMesh, BaseMaterial3D, CanvasItem, CanvasLayer, ClassDb, Control,
    GeometryInstance3D, Image, ImageTexture, Label3D, Line2D, Material, MeshInstance3D, Node3D,
    OrmMaterial3D, Polygon2D, Shader, ShaderMaterial, Skeleton3D, Skin, Sprite3D, SpriteBase3D,
    SpriteFrames, StandardMaterial3D,
};
use godot::obj::{EngineBitfield, EngineEnum, IndexEnum};
use godot::prelude::*;

/// The census of what the game draws.
const LIST: &str = include_str!("../../../godot/art/variants.txt");
/// The list as a number: a first launch's marker changes with it
/// (`game::shader_cache_warm`).
pub fn list_hash() -> u64 {
    LIST.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The name of the nodes the variants are made under (the census skips
/// them).
pub const HOLDER: &str = "Variants";

/// The 3D labels and sprites, by kind (their look is Godot's own material).
const FLAT: [&str; 3] = ["Label3D", "Sprite3D", "AnimatedSprite3D"];

/// The list's variants, and what the game draws over any of them as well:
/// each overlay over every layout (the hover's outline, the x-ray, a
/// falling body's shade come over any model), and each opaque standard
/// surface faded (a body sinking away, or dissolving, fades).
fn lines() -> Vec<String> {
    let listed: Vec<&str> = LIST
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let mut out: std::collections::BTreeSet<String> =
        listed.iter().map(|l| l.to_string()).collect();
    let mut overlays = std::collections::BTreeSet::new();
    let mut layouts = std::collections::BTreeSet::new();
    for line in &listed {
        let parts: Vec<&str> = line.split('|').collect();
        let [shader, layout, rest @ ..] = parts.as_slice() else {
            continue;
        };
        if *shader == "canvas" || FLAT.contains(shader) {
            continue;
        }
        layouts.insert(*layout);
        if shader.starts_with("overlay ") {
            overlays.insert(*shader);
        } else if rest.is_empty() && shader.starts_with("std:0.") {
            out.insert(format!("{line}|fade"));
        }
    }
    for o in &overlays {
        for l in &layouts {
            out.insert(format!("{o}|{l}"));
        }
    }
    out.into_iter().collect()
}

/// The settings a standard material's shader is picked by (Godot's
/// `BaseMaterial3D::MaterialKey`): two with the same share a shader.
/// `standard_material` makes one back.
pub fn standard_key(m: &Gd<BaseMaterial3D>) -> String {
    let mut features = 0u32;
    for i in 0..Feature::ENUMERATOR_COUNT as i32 {
        if let Some(f) = Feature::try_from_ord(i)
            && m.get_feature(f)
        {
            features |= 1 << i;
        }
    }
    let mut flags = 0u32;
    for i in 0..Flags::ENUMERATOR_COUNT as i32 {
        if let Some(f) = Flags::try_from_ord(i)
            && m.get_flag(f)
        {
            flags |= 1 << i;
        }
    }
    let orm = m.clone().try_cast::<OrmMaterial3D>().is_ok();
    let modes = [
        m.get_transparency().ord(),
        m.get_shading_mode().ord(),
        m.get_blend_mode().ord(),
        m.get_cull_mode().ord(),
        m.get_depth_draw_mode().ord(),
        m.get_depth_test().ord(),
        m.get_billboard_mode().ord(),
        m.get_texture_filter().ord(),
        m.get_diffuse_mode().ord(),
        m.get_specular_mode().ord(),
        m.get_distance_fade().ord(),
        m.get_emission_operator().ord(),
        m.get_alpha_antialiasing().ord(),
        m.get_detail_uv().ord(),
        m.get_detail_blend_mode().ord(),
        m.get_roughness_texture_channel().ord(),
        m.get_stencil_mode().ord(),
        m.get_stencil_flags(),
        m.get_stencil_compare().ord(),
        m.get_stencil_reference(),
        i32::from(m.is_grow_enabled()),
        i32::from(m.is_proximity_fade_enabled()),
        i32::from(m.is_heightmap_deep_parallax_enabled()),
        i32::from(orm),
    ];
    let modes: Vec<String> = modes.iter().map(|v| v.to_string()).collect();
    format!("std:{}:f{features:x}:g{flags:x}", modes.join("."))
}

/// A standard material with the settings `standard_key` named.
pub fn standard_material(key: &str) -> Option<Gd<BaseMaterial3D>> {
    let mut parts = key.strip_prefix("std:")?.split(':');
    let modes: Vec<i32> = parts
        .next()?
        .split('.')
        .map(|v| v.parse().ok())
        .collect::<Option<_>>()?;
    let features = u32::from_str_radix(parts.next()?.strip_prefix('f')?, 16).ok()?;
    let flags = u32::from_str_radix(parts.next()?.strip_prefix('g')?, 16).ok()?;
    let &[
        tr,
        sh,
        bl,
        cu,
        dd,
        dt,
        bb,
        tf,
        di,
        sp,
        df,
        eo,
        aa,
        du,
        db,
        rc,
        sm,
        sf,
        sc,
        sr,
        gr,
        pf,
        dp,
        orm,
    ] = modes.as_slice()
    else {
        return None;
    };
    let mut m: Gd<BaseMaterial3D> = if orm == 1 {
        OrmMaterial3D::new_gd().upcast()
    } else {
        StandardMaterial3D::new_gd().upcast()
    };
    m.set_transparency(Transparency::try_from_ord(tr)?);
    m.set_shading_mode(ShadingMode::try_from_ord(sh)?);
    m.set_blend_mode(BlendMode::try_from_ord(bl)?);
    m.set_cull_mode(CullMode::try_from_ord(cu)?);
    m.set_depth_draw_mode(DepthDrawMode::try_from_ord(dd)?);
    m.set_depth_test(DepthTest::try_from_ord(dt)?);
    m.set_billboard_mode(BillboardMode::try_from_ord(bb)?);
    m.set_texture_filter(TextureFilter::try_from_ord(tf)?);
    m.set_diffuse_mode(DiffuseMode::try_from_ord(di)?);
    m.set_specular_mode(SpecularMode::try_from_ord(sp)?);
    m.set_distance_fade(DistanceFadeMode::try_from_ord(df)?);
    m.set_emission_operator(EmissionOperator::try_from_ord(eo)?);
    m.set_alpha_antialiasing(AlphaAntiAliasing::try_from_ord(aa)?);
    m.set_detail_uv(DetailUv::try_from_ord(du)?);
    m.set_detail_blend_mode(BlendMode::try_from_ord(db)?);
    m.set_roughness_texture_channel(TextureChannel::try_from_ord(rc)?);
    m.set_stencil_mode(StencilMode::try_from_ord(sm)?);
    m.set_stencil_flags(sf);
    m.set_stencil_compare(StencilCompare::try_from_ord(sc)?);
    m.set_stencil_reference(sr);
    m.set_grow_enabled(gr == 1);
    m.set_proximity_fade_enabled(pf == 1);
    m.set_heightmap_deep_parallax(dp == 1);
    for i in 0..Feature::ENUMERATOR_COUNT as i32 {
        m.set_feature(Feature::try_from_ord(i)?, features & (1 << i) != 0);
    }
    for i in 0..Flags::ENUMERATOR_COUNT as i32 {
        m.set_flag(Flags::try_from_ord(i)?, flags & (1 << i) != 0);
    }
    Some(m)
}

/// The list's standard materials whose settings do not come back as the
/// census named them (a variant made for another shader than the game's).
pub fn mismatches() -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in LIST.lines().filter(|l| !l.starts_with('#')) {
        let name = line.split('|').next().unwrap_or("");
        let key = name.strip_prefix("overlay ").unwrap_or(name);
        if !key.starts_with("std:") || !seen.insert(key) {
            continue;
        }
        match standard_material(key) {
            Some(m) if standard_key(&m) == key => {}
            Some(m) => out.push(format!("{key} made as {}", standard_key(&m))),
            None => out.push(format!("{key} not made")),
        }
    }
    out
}

/// A material for a shader as the census names it: a standard material's
/// settings, a shader file, or none (Godot's default).
fn material_of(name: &str) -> Result<Option<Gd<Material>>, String> {
    let name = name.strip_prefix("overlay ").unwrap_or(name);
    if name == "default" {
        return Ok(None);
    }
    if let Some(path) = name.strip_prefix("shader:") {
        let shader = godot::tools::try_load::<Shader>(path).map_err(|e| format!("{path}: {e}"))?;
        let mut m = ShaderMaterial::new_gd();
        m.set_shader(&shader);
        return Ok(Some(m.upcast()));
    }
    standard_material(name)
        .map(|m| Some(m.upcast()))
        .ok_or_else(|| format!("no material for {name}"))
}

/// A triangle (or what of one the primitive takes) with the arrays of a
/// census layout (`<format hex>/<primitive>`), packed as it says.
fn mesh_of(layout: &str) -> Result<Gd<ArrayMesh>, String> {
    let bad = || format!("no layout {layout:?}");
    let (format, primitive) = layout.split_once('/').ok_or_else(bad)?;
    let format = u64::from_str_radix(format, 16).map_err(|_| bad())?;
    let primitive = primitive
        .parse::<i32>()
        .ok()
        .and_then(PrimitiveType::try_from_ord)
        .ok_or_else(bad)?;
    let has = |f: ArrayFormat| format & f.ord() != 0;
    const N: usize = 3;
    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    let mut put = |t: ArrayType, v: Variant| arrays.set(t.ord() as usize, &v);
    let points = [
        Vector3::ZERO,
        Vector3::new(0.1, 0.0, 0.0),
        Vector3::new(0.0, 0.0, 0.1),
    ];
    put(
        ArrayType::VERTEX,
        PackedVector3Array::from(points.as_slice()).to_variant(),
    );
    if has(ArrayFormat::NORMAL) {
        put(
            ArrayType::NORMAL,
            PackedVector3Array::from([Vector3::UP; N].as_slice()).to_variant(),
        );
    }
    if has(ArrayFormat::TANGENT) {
        let t: Vec<f32> = [1.0, 0.0, 0.0, 1.0].repeat(N);
        put(
            ArrayType::TANGENT,
            PackedFloat32Array::from(t.as_slice()).to_variant(),
        );
    }
    if has(ArrayFormat::COLOR) {
        put(
            ArrayType::COLOR,
            PackedColorArray::from([Color::WHITE; N].as_slice()).to_variant(),
        );
    }
    let uv = [
        Vector2::ZERO,
        Vector2::new(1.0, 0.0),
        Vector2::new(0.0, 1.0),
    ];
    if has(ArrayFormat::TEX_UV) {
        put(
            ArrayType::TEX_UV,
            PackedVector2Array::from(uv.as_slice()).to_variant(),
        );
    }
    if has(ArrayFormat::TEX_UV2) {
        put(
            ArrayType::TEX_UV2,
            PackedVector2Array::from(uv.as_slice()).to_variant(),
        );
    }
    let customs = [
        (
            ArrayFormat::CUSTOM0,
            ArrayType::CUSTOM0,
            ArrayFormat::CUSTOM0_SHIFT,
        ),
        (
            ArrayFormat::CUSTOM1,
            ArrayType::CUSTOM1,
            ArrayFormat::CUSTOM1_SHIFT,
        ),
        (
            ArrayFormat::CUSTOM2,
            ArrayType::CUSTOM2,
            ArrayFormat::CUSTOM2_SHIFT,
        ),
        (
            ArrayFormat::CUSTOM3,
            ArrayType::CUSTOM3,
            ArrayFormat::CUSTOM3_SHIFT,
        ),
    ];
    for (bit, t, shift) in customs {
        if !has(bit) {
            continue;
        }
        // RGBA8 unorm and snorm, RG and RGBA half (as bytes), then R, RG,
        // RGB, RGBA float
        let v = match (format >> shift.ord()) & ArrayFormat::CUSTOM_MASK.ord() {
            0..=2 => PackedByteArray::from(vec![0u8; 4 * N]).to_variant(),
            3 => PackedByteArray::from(vec![0u8; 8 * N]).to_variant(),
            k => PackedFloat32Array::from(vec![0.0f32; (k as usize - 3) * N]).to_variant(),
        };
        put(t, v);
    }
    let weights = if has(ArrayFormat::FLAG_USE_8_BONE_WEIGHTS) {
        8
    } else {
        4
    };
    if has(ArrayFormat::BONES) {
        put(
            ArrayType::BONES,
            PackedInt32Array::from(vec![0; weights * N]).to_variant(),
        );
    }
    if has(ArrayFormat::WEIGHTS) {
        let mut w = vec![0.0f32; weights * N];
        for v in w.chunks_mut(weights) {
            v[0] = 1.0;
        }
        put(
            ArrayType::WEIGHTS,
            PackedFloat32Array::from(w.as_slice()).to_variant(),
        );
    }
    if has(ArrayFormat::INDEX) {
        let index: &[i32] = match primitive {
            PrimitiveType::LINES => &[0, 1, 1, 2],
            _ => &[0, 1, 2],
        };
        put(ArrayType::INDEX, PackedInt32Array::from(index).to_variant());
    }
    // the flags the format carries: how the arrays are packed
    let packing = ArrayFormat::FLAG_COMPRESS_ATTRIBUTES.ord()
        | ArrayFormat::FLAG_USE_8_BONE_WEIGHTS.ord()
        | ArrayFormat::FLAG_USE_2D_VERTICES.ord()
        | (0xfff << ArrayFormat::CUSTOM_BASE.ord());
    let mut mesh = ArrayMesh::new_gd();
    mesh.add_surface_from_arrays_ex(primitive, &arrays)
        .flags(ArrayFormat::from_ord(format & packing))
        .done();
    if mesh.get_surface_count() != 1 {
        return Err(bad());
    }
    Ok(mesh)
}

/// The 3D label or sprite of a census line: its kind, its look's numbers
/// (`Label3D|2d:...|label`, `Sprite3D|2d:...|sprite`).
fn flat_thing(kind: &str, look: &str) -> Option<Gd<Node3D>> {
    let n: Vec<i32> = look
        .strip_prefix("2d:")?
        .split('.')
        .map(|v| v.parse().ok())
        .collect::<Option<_>>()?;
    if kind == "Label3D" {
        use godot::classes::label_3d::{AlphaCutMode, DrawFlags};
        let &[cut, bb, tf, aa, shaded, double, no_depth] = n.as_slice() else {
            return None;
        };
        let mut l = Label3D::new_alloc();
        l.set_text("x");
        l.set_alpha_cut_mode(AlphaCutMode::try_from_ord(cut)?);
        l.set_billboard_mode(BillboardMode::try_from_ord(bb)?);
        l.set_texture_filter(TextureFilter::try_from_ord(tf)?);
        l.set_alpha_antialiasing(AlphaAntiAliasing::try_from_ord(aa)?);
        l.set_draw_flag(DrawFlags::SHADED, shaded == 1);
        l.set_draw_flag(DrawFlags::DOUBLE_SIDED, double == 1);
        l.set_draw_flag(DrawFlags::DISABLE_DEPTH_TEST, no_depth == 1);
        return Some(l.upcast());
    }
    use godot::classes::sprite_base_3d::{AlphaCutMode, DrawFlags};
    let &[cut, bb, tf, aa, transparent, shaded, double, no_depth] = n.as_slice() else {
        return None;
    };
    let image = Image::create_empty(2, 2, false, ImageFormat::RGBA8)?;
    let texture = ImageTexture::create_from_image(&image)?;
    let mut s: Gd<SpriteBase3D> = match kind {
        "AnimatedSprite3D" => {
            let mut frames = SpriteFrames::new_gd();
            frames.add_frame("default", &texture);
            let mut a = AnimatedSprite3D::new_alloc();
            a.set_sprite_frames(&frames);
            a.upcast()
        }
        _ => {
            let mut s = Sprite3D::new_alloc();
            s.set_texture(&texture);
            s.upcast()
        }
    };
    s.set_alpha_cut_mode(AlphaCutMode::try_from_ord(cut)?);
    s.set_billboard_mode(BillboardMode::try_from_ord(bb)?);
    s.set_texture_filter(TextureFilter::try_from_ord(tf)?);
    s.set_alpha_antialiasing(AlphaAntiAliasing::try_from_ord(aa)?);
    s.set_draw_flag(DrawFlags::TRANSPARENT, transparent == 1);
    s.set_draw_flag(DrawFlags::SHADED, shaded == 1);
    s.set_draw_flag(DrawFlags::DOUBLE_SIDED, double == 1);
    s.set_draw_flag(DrawFlags::DISABLE_DEPTH_TEST, no_depth == 1);
    Some(s.upcast())
}

/// Variants made a frame: a new shader compiles as the next frame starts,
/// and holds it (some 20-100 ms each with Godot's shader cache empty).
const PER_FRAME: usize = 6;
/// Frames the variants stay in the scene once made, when they are made
/// again after the graphics changed: a pass the new settings add (ambient
/// occlusion's) is known once a frame is drawn with them.
const HOLD: u32 = 60;

#[derive(Default)]
pub struct Variants {
    /// The triangles, labels and sprites, hidden; the canvas items under
    /// the veil; the skeleton and skin the skinned ones share.
    holder: Option<Gd<Node3D>>,
    canvas: Option<Gd<CanvasLayer>>,
    skeleton: Option<(Gd<Skeleton3D>, Gd<Skin>)>,
    /// The list's lines yet to make.
    todo: Vec<String>,
    /// A material per shader (None: Godot's default), kept for the
    /// session; a triangle per layout.
    materials: HashMap<String, Option<Gd<Material>>>,
    meshes: HashMap<String, Gd<ArrayMesh>>,
    /// What was made, and the lines that could not be (with why).
    made: usize,
    unmade: Vec<String>,
    /// Frames still to keep them once made (`tick`), and how many are made
    /// a frame.
    hold: u32,
    per_frame: usize,
}

impl Variants {
    /// Start making the list's variants under `parent` (`step` makes
    /// them, a few a frame); the canvas items' too unless `canvas` is
    /// false (they show, and the 3D's settings change nothing of theirs).
    /// Made again, their shaders are there: all at once.
    pub fn start(&mut self, parent: &mut Gd<Node3D>, canvas: bool) {
        self.release();
        self.per_frame = if self.materials.is_empty() {
            PER_FRAME
        } else {
            usize::MAX
        };
        self.made = 0;
        self.unmade.clear();
        self.hold = HOLD;
        let mut holder = Node3D::new_alloc();
        holder.set_name(HOLDER);
        holder.set_visible(false);
        // under the floor, out of the way
        holder.set_position(Vector3::new(0.0, -50.0, 0.0));
        parent.add_child(&holder);
        if canvas {
            let mut layer = CanvasLayer::new_alloc();
            layer.set_name(&format!("{HOLDER}Canvas"));
            layer.set_layer(-100);
            parent.add_child(&layer);
            self.canvas = Some(layer);
        }
        let mut skeleton = Skeleton3D::new_alloc();
        skeleton.add_bone("root");
        holder.add_child(&skeleton);
        let mut skin = Skin::new_gd();
        skin.add_bind(0, Transform3D::IDENTITY);
        self.todo = lines()
            .into_iter()
            .filter(|l| canvas || !l.starts_with("canvas|"))
            .rev()
            .collect();
        self.holder = Some(holder);
        self.skeleton = Some((skeleton, skin));
    }

    /// A frame of variants made again (`start`, after the rehearsal): a
    /// few made, else held a while, then let go. False when there are none.
    pub fn tick(&mut self) -> bool {
        if self.holder.is_none() {
            return false;
        }
        if !self.done() {
            self.step();
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.release();
        }
        true
    }

    /// Make the next few; false when all are made.
    pub fn step(&mut self) -> bool {
        for _ in 0..self.per_frame {
            let Some(line) = self.todo.pop() else {
                return false;
            };
            match self.make(&line) {
                Ok(()) => self.made += 1,
                Err(why) => self.unmade.push(format!("{line}: {why}")),
            }
        }
        !self.todo.is_empty()
    }

    /// All made (or none started).
    pub fn done(&self) -> bool {
        self.todo.is_empty()
    }

    /// How many of the list are made, 0 to 1.
    pub fn progress(&self) -> f32 {
        let all = self.made + self.unmade.len() + self.todo.len();
        if all == 0 {
            return 1.0;
        }
        (self.made + self.unmade.len()) as f32 / all as f32
    }

    fn make(&mut self, line: &str) -> Result<(), String> {
        let Some(holder) = self.holder.clone() else {
            return Err("not started".to_string());
        };
        let parts: Vec<&str> = line.split('|').collect();
        let fade = parts.iter().skip(2).any(|p| *p == "fade");
        match parts.as_slice() {
            ["canvas", shader, class] => match self.canvas.clone() {
                Some(canvas) => self.canvas_item(canvas, shader, class),
                None => Err("no canvas".to_string()),
            },
            [
                kind @ ("Label3D" | "Sprite3D" | "AnimatedSprite3D"),
                look,
                ..,
            ] => {
                let mut n =
                    flat_thing(kind, look).ok_or_else(|| format!("no {kind} for {look}"))?;
                if fade && let Ok(mut g) = n.clone().try_cast::<GeometryInstance3D>() {
                    g.set_transparency(0.5);
                }
                let mut holder = holder;
                holder.add_child(&n);
                n.set_visible(true);
                Ok(())
            }
            [shader, layout, ..] => self.surface(holder, shader, layout, fade),
            _ => Err("not a variant".to_string()),
        }
    }

    fn material(&mut self, name: &str) -> Result<Option<Gd<Material>>, String> {
        let key = name.strip_prefix("overlay ").unwrap_or(name);
        if let Some(m) = self.materials.get(key) {
            return Ok(m.clone());
        }
        let m = material_of(key)?;
        self.materials.insert(key.to_string(), m.clone());
        Ok(m)
    }

    fn surface(
        &mut self,
        mut holder: Gd<Node3D>,
        shader: &str,
        layout: &str,
        fade: bool,
    ) -> Result<(), String> {
        let material = self.material(shader)?;
        let (arrays, skinned) = match layout.strip_suffix("+inst") {
            Some(l) => (l, true),
            None => (layout, false),
        };
        let mesh = match self.meshes.get(arrays) {
            Some(m) => m.clone(),
            None => {
                let m = mesh_of(arrays)?;
                self.meshes.insert(arrays.to_string(), m.clone());
                m
            }
        };
        if skinned && mesh.surface_get_format(0).ord() & ArrayFormat::BONES.ord() == 0 {
            return Err("skinned, without bones".to_string());
        }
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_mesh(&mesh);
        if let Some(m) = &material {
            mi.set_surface_override_material(0, m);
        }
        if fade {
            mi.set_transparency(0.5);
        }
        holder.add_child(&mi);
        // drawn from its own copy, unpacked, as a skinned model is
        if skinned && let Some((skeleton, skin)) = &self.skeleton {
            mi.set_skin(skin);
            let path = mi.get_path_to(skeleton);
            mi.set_skeleton_path(&path);
        }
        Ok(())
    }

    fn canvas_item(
        &mut self,
        mut canvas: Gd<CanvasLayer>,
        shader: &str,
        class: &str,
    ) -> Result<(), String> {
        let material = self.material(shader)?;
        let item = ClassDb::singleton().instantiate(class);
        let mut item = item
            .try_to::<Gd<CanvasItem>>()
            .map_err(|_| format!("{class} is not a canvas item"))?;
        if let Some(m) = &material {
            item.set_material(m);
        }
        // something to draw, a pixel of it in the corner, under the veil:
        // a box, a polygon, a line
        let shape = [
            Vector2::ZERO,
            Vector2::new(1.0, 0.0),
            Vector2::new(0.0, 1.0),
        ];
        if let Ok(mut c) = item.clone().try_cast::<Control>() {
            c.set_size(Vector2::ONE);
        } else if let Ok(mut p) = item.clone().try_cast::<Polygon2D>() {
            p.set_polygon(&PackedVector2Array::from(shape.as_slice()));
        } else if let Ok(mut l) = item.clone().try_cast::<Line2D>() {
            l.set_points(&PackedVector2Array::from(shape.as_slice()));
        }
        canvas.add_child(&item);
        Ok(())
    }

    /// The triangles and canvas items go (their pipelines are built or
    /// building); the materials stay, and with them their shaders.
    pub fn release(&mut self) {
        self.todo.clear();
        self.skeleton = None;
        self.meshes.clear();
        if let Some(mut h) = self.holder.take()
            && h.is_instance_valid()
        {
            h.queue_free();
        }
        self.release_canvas();
    }

    /// The veil is up: the canvas items go (they were drawn under it, and
    /// they would show), and those still to make with them.
    pub fn release_canvas(&mut self) {
        self.todo.retain(|l| !l.starts_with("canvas|"));
        if let Some(mut c) = self.canvas.take()
            && c.is_instance_valid()
        {
            c.queue_free();
        }
    }

    /// How many variants were made, and the lines that could not be.
    pub fn made(&self) -> (usize, &[String]) {
        (self.made, &self.unmade)
    }
}
