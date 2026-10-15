//! A census of what the renderer compiles pipelines for
//! (`RENETHACK_VARIANTS=<file>`). Godot compiles a pipeline for each
//! shader over each vertex layout it draws it with, in each pass: a
//! variant here is a shader (a shader material's shader, or the settings
//! that pick a standard material's), the surface's arrays (which, how
//! packed) and primitive, whether it is drawn from an unpacked copy
//! (skinned), and whether it is faded (drawn in the transparent pass).
//! A canvas item's is its material's shader, or a 2D node's kind (one of
//! Godot's own shaders draws a polygon); a control shown with neither is
//! noted (`shown|`) to tell what a frame showed, nothing to make.
//!
//! Every frame it walks the tree and writes the variants it has not seen
//! before, with the frame (numbered as the frame log numbers it), the
//! start-up veil and what drew them: `tools/perf.py` names what a frame
//! that compiled pipelines drew for the first time. A 3D variant counts
//! from when its node is in the tree (Godot compiles its pipelines then,
//! shown or not), a canvas item's once it is shown.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};

use godot::classes::geometry_instance_3d::VisibilityRangeFadeMode;
use godot::classes::sprite_base_3d::DrawFlags;
use godot::classes::{
    ArrayMesh, BaseMaterial3D, CanvasItem, CpuParticles3D, GeometryInstance3D, GpuParticles3D,
    Label3D, Material, Mesh, MeshInstance3D, MultiMeshInstance3D, Node, RenderingServer,
    ShaderMaterial, SpriteBase3D,
};
use godot::obj::{EngineBitfield, EngineEnum, Singleton};

use crate::variants::standard_key;
use godot::prelude::*;

/// The shader a material draws with, as a name.
pub fn shader_key(m: &Gd<Material>) -> String {
    if let Ok(s) = m.clone().try_cast::<ShaderMaterial>() {
        return match s.get_shader() {
            Some(sh) if !sh.get_path().is_empty() => format!("shader:{}", sh.get_path()),
            Some(sh) => format!("shader#{}", sh.instance_id()),
            None => "shader:none".to_string(),
        };
    }
    if let Ok(b) = m.clone().try_cast::<BaseMaterial3D>() {
        return standard_key(&b);
    }
    m.get_class().to_string()
}

pub struct Census {
    out: BufWriter<File>,
    seen: HashSet<String>,
    /// This scan's shader names, by material.
    names: HashMap<InstanceId, String>,
    /// A primitive mesh's arrays, by its class and surface.
    formats: HashMap<String, String>,
}

impl Census {
    /// The census `RENETHACK_VARIANTS` names, if it does and can be made.
    pub fn from_env() -> Option<Census> {
        let path = std::env::var_os("RENETHACK_VARIANTS")?;
        let file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                godot_warn!(
                    "renethack: no census at {}: {e}",
                    std::path::Path::new(&path).display()
                );
                return None;
            }
        };
        let mut out = BufWriter::new(file);
        let _ = writeln!(out, "frame\tveil\tvariant\tdrawn by");
        Some(Census {
            out,
            seen: HashSet::new(),
            names: HashMap::new(),
            formats: HashMap::new(),
        })
    }

    /// What the tree under `root` draws that it did not draw before, as of
    /// `frame`.
    pub fn scan(&mut self, root: Gd<Node>, frame: u64, veil: u8) {
        self.names.clear();
        let mut stack = vec![root];
        let mut found: Vec<(String, String)> = Vec::new();
        while let Some(node) = stack.pop() {
            // the rehearsal's own one of each (`variants.rs`): not drawn by
            // the game
            if node
                .get_name()
                .to_string()
                .starts_with(crate::variants::HOLDER)
            {
                continue;
            }
            stack.extend(
                node.get_children_ex()
                    .include_internal(true)
                    .done()
                    .iter_shared(),
            );
            if let Ok(g) = node.clone().try_cast::<GeometryInstance3D>() {
                for v in self.variants(&g) {
                    if !self.seen.contains(&v) {
                        found.push((v, owner(&node)));
                    }
                }
            } else if let Ok(c) = node.clone().try_cast::<CanvasItem>()
                && c.is_visible_in_tree()
            {
                // a canvas item with no shader of its own draws with one of
                // Godot's: a 2D node's kind tells which (a polygon's, a
                // line's); a control draws boxes and text, as every screen
                // does, and what shows first only tells what a frame showed
                let v = match c.get_material() {
                    Some(m) => format!("canvas|{}|{}", self.name(&m), c.get_class()),
                    None if c.clone().try_cast::<godot::classes::Node2D>().is_ok() => {
                        format!("canvas|default|{}", c.get_class())
                    }
                    None => format!("shown|{}|{}", c.get_class(), plain_name(&node)),
                };
                if !self.seen.contains(&v) {
                    found.push((v, owner(&node)));
                }
            }
        }
        for (v, by) in found {
            if self.seen.insert(v.clone()) {
                let _ = writeln!(self.out, "{frame}\t{veil}\t{v}\t{by}");
            }
        }
        if frame.is_multiple_of(64) {
            let _ = self.out.flush();
        }
    }

    fn name(&mut self, m: &Gd<Material>) -> String {
        self.names
            .entry(m.instance_id())
            .or_insert_with(|| shader_key(m))
            .clone()
    }

    /// The names of a material and the passes it is followed by.
    fn chain(&mut self, m: Option<Gd<Material>>) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = m;
        while let Some(m) = at {
            out.push(self.name(&m));
            at = m.get_next_pass();
            if out.len() > 4 {
                break;
            }
        }
        if out.is_empty() {
            out.push("default".to_string());
        }
        out
    }

    /// A surface's arrays (which, and how packed) and primitive, `+inst`
    /// when skinned or with blend shapes: Godot draws it from a copy, its
    /// positions and normals unpacked.
    fn layout(&mut self, mesh: &Gd<Mesh>, surface: i32, instanced: bool) -> String {
        let inst = if instanced { "+inst" } else { "" };
        if let Ok(a) = mesh.clone().try_cast::<ArrayMesh>() {
            let inst = if a.get_blend_shape_count() > 0 {
                "+inst"
            } else {
                inst
            };
            return format!(
                "{:x}/{}{inst}",
                a.surface_get_format(surface).ord(),
                a.surface_get_primitive_type(surface).ord()
            );
        }
        // a primitive mesh's arrays are its class's: asked of the
        // renderer once (it copies the surface back)
        let class = mesh.get_class().to_string();
        let key = format!("{class}/{surface}");
        let format = self.formats.entry(key).or_insert_with(|| {
            let d = RenderingServer::singleton().mesh_get_surface(mesh.get_rid(), surface);
            let number = |k: &str| d.get(k).and_then(|v| v.try_to::<i64>().ok());
            match (number("format"), number("primitive")) {
                (Some(f), Some(p)) => format!("{f:x}/{p}"),
                _ => class.clone(),
            }
        });
        format!("{format}{inst}")
    }

    /// The variants a geometry instance draws.
    fn variants(&mut self, g: &Gd<GeometryInstance3D>) -> Vec<String> {
        let fade = g.get_transparency() > 0.0
            || g.get_visibility_range_fade_mode() != VisibilityRangeFadeMode::DISABLED;
        let fade = if fade { "|fade" } else { "" };
        let over = g.get_material_override();
        let overlay = g.get_material_overlay();
        // what it draws: its meshes, and each surface's own material (the
        // instance's override in its place)
        let mut instanced = false;
        let mut surfaces: Vec<(Gd<Mesh>, i32, Option<Gd<Material>>)> = Vec::new();
        let mut add = |mesh: Option<Gd<Mesh>>, own: &dyn Fn(i32) -> Option<Gd<Material>>| {
            if let Some(mesh) = mesh {
                for i in 0..mesh.get_surface_count() {
                    let m = over
                        .clone()
                        .or_else(|| own(i))
                        .or_else(|| mesh.surface_get_material(i));
                    surfaces.push((mesh.clone(), i, m));
                }
            }
        };
        if let Ok(mi) = g.clone().try_cast::<MeshInstance3D>() {
            instanced = mi.get_skin().is_some() || !mi.get_skeleton_path().is_empty();
            add(mi.get_mesh(), &|i| mi.get_surface_override_material(i));
        } else if let Ok(mm) = g.clone().try_cast::<MultiMeshInstance3D>() {
            add(mm.get_multimesh().and_then(|m| m.get_mesh()), &|_| None);
        } else if let Ok(p) = g.clone().try_cast::<GpuParticles3D>() {
            for pass in 0..p.get_draw_passes() {
                add(p.get_draw_pass_mesh(pass), &|_| None);
            }
        } else if let Ok(p) = g.clone().try_cast::<CpuParticles3D>() {
            add(p.get_mesh(), &|_| None);
        } else if let Ok(s) = g.clone().try_cast::<SpriteBase3D>() {
            if over.is_none() {
                return vec![format!(
                    "{}|2d:{}.{}.{}.{}.{}.{}.{}.{}|sprite{fade}",
                    s.get_class(),
                    s.get_alpha_cut_mode().ord(),
                    s.get_billboard_mode().ord(),
                    s.get_texture_filter().ord(),
                    s.get_alpha_antialiasing().ord(),
                    u8::from(s.get_draw_flag(DrawFlags::TRANSPARENT)),
                    u8::from(s.get_draw_flag(DrawFlags::SHADED)),
                    u8::from(s.get_draw_flag(DrawFlags::DOUBLE_SIDED)),
                    u8::from(s.get_draw_flag(DrawFlags::DISABLE_DEPTH_TEST)),
                )];
            }
            return self
                .chain(over)
                .into_iter()
                .map(|n| format!("{}|{n}|sprite{fade}", s.get_class()))
                .collect();
        } else if let Ok(l) = g.clone().try_cast::<Label3D>() {
            use godot::classes::label_3d::DrawFlags as LabelFlags;
            if over.is_none() {
                return vec![format!(
                    "Label3D|2d:{}.{}.{}.{}.{}.{}.{}|label{fade}",
                    l.get_alpha_cut_mode().ord(),
                    l.get_billboard_mode().ord(),
                    l.get_texture_filter().ord(),
                    l.get_alpha_antialiasing().ord(),
                    u8::from(l.get_draw_flag(LabelFlags::SHADED)),
                    u8::from(l.get_draw_flag(LabelFlags::DOUBLE_SIDED)),
                    u8::from(l.get_draw_flag(LabelFlags::DISABLE_DEPTH_TEST)),
                )];
            }
            return self
                .chain(over)
                .into_iter()
                .map(|n| format!("Label3D|{n}|label{fade}"))
                .collect();
        } else {
            return vec![format!("{}|{fade}", g.get_class())];
        }
        let mut out = Vec::new();
        for (mesh, i, m) in surfaces {
            let at = self.layout(&mesh, i, instanced);
            for n in self.chain(m) {
                out.push(format!("{n}|{at}{fade}"));
            }
            if let Some(o) = &overlay {
                for n in self.chain(Some(o.clone())) {
                    out.push(format!("overlay {n}|{at}{fade}"));
                }
            }
        }
        out
    }
}

/// What drew a variant: the mesh's file, else where the node is.
fn owner(node: &Gd<Node>) -> String {
    if let Ok(mi) = node.clone().try_cast::<MeshInstance3D>()
        && let Some(mesh) = mi.get_mesh()
    {
        let path = mesh.get_path().to_string();
        if !path.is_empty() {
            return path;
        }
    }
    let path = node.get_path().to_string();
    let parts: Vec<&str> = path.rsplitn(4, '/').collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// A node's name without the number Godot gives a nameless one
/// (`@Label@123`: `Label`).
fn plain_name(node: &Gd<Node>) -> String {
    let name = node.get_name().to_string();
    name.trim_start_matches('@')
        .split('@')
        .next()
        .unwrap_or("")
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}
