//! The map's solids drawn in batches: one `MultiMesh` per mesh, material,
//! shadow setting and chunk of the level, so a level of walls, caps, rock
//! and floor tiles is tens of draw calls rather than thousands of nodes.
//! A cell holds slots in batches; a freed slot is hidden (scaled to
//! nothing) and taken again by the next solid of its batch. The chunks
//! keep the batches small enough for the camera and the shadowed lights to
//! cull.

use std::collections::{HashMap, HashSet};

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{Material, Mesh, MultiMesh, MultiMeshInstance3D, Node3D};
use godot::prelude::*;

use crate::meshes::MeshKey;

/// Cells per chunk across and down.
const CHUNK: (i32, i32) = (20, 11);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BatchKey {
    mesh: MeshKey,
    /// The material's instance id.
    material: i64,
    shadow: bool,
    chunk: (i32, i32),
}

/// A solid's place in a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    key: BatchKey,
    index: usize,
}

struct Batch {
    node: Gd<MultiMeshInstance3D>,
    mm: Gd<MultiMesh>,
    xforms: Vec<Transform3D>,
    free: Vec<usize>,
    /// Where hidden slots wait: the chunk's middle, so they do not stretch
    /// its bounds.
    rest: Vector3,
}

pub struct Batches {
    root: Gd<Node3D>,
    layers: u32,
    batches: HashMap<BatchKey, Batch>,
    dirty: HashSet<BatchKey>,
}

/// The chunk of a cell.
fn chunk_of(x: i32, y: i32) -> (i32, i32) {
    (x.div_euclid(CHUNK.0), y.div_euclid(CHUNK.1))
}

impl Batches {
    /// Batches are children of `root`, drawn on render `layers`.
    pub fn new(root: Gd<Node3D>, layers: u32) -> Batches {
        Batches {
            root,
            layers,
            batches: HashMap::new(),
            dirty: HashSet::new(),
        }
    }

    /// Put a solid of cell `cell` in its batch.
    pub fn add(
        &mut self,
        cell: (i32, i32),
        mesh: (MeshKey, &Gd<Mesh>),
        material: &Gd<Material>,
        shadow: bool,
        xform: Transform3D,
    ) -> Slot {
        let chunk = chunk_of(cell.0, cell.1);
        let key = BatchKey {
            mesh: mesh.0,
            material: material.instance_id().to_i64(),
            shadow,
            chunk,
        };
        let batch = self.batches.entry(key).or_insert_with(|| {
            let mut mm = MultiMesh::new_gd();
            mm.set_transform_format(TransformFormat::TRANSFORM_3D);
            mm.set_mesh(mesh.1);
            let mut node = MultiMeshInstance3D::new_alloc();
            node.set_multimesh(&mm);
            node.set_material_override(material);
            node.set_layer_mask(self.layers);
            node.set_cast_shadows_setting(if shadow {
                ShadowCastingSetting::ON
            } else {
                ShadowCastingSetting::OFF
            });
            self.root.add_child(&node);
            let rest = Vector3::new(
                ((chunk.0 as f32) + 0.5) * CHUNK.0 as f32,
                0.0,
                ((chunk.1 as f32) + 0.5) * CHUNK.1 as f32,
            );
            Batch {
                node,
                mm,
                xforms: Vec::new(),
                free: Vec::new(),
                rest,
            }
        });
        let index = match batch.free.pop() {
            Some(i) => {
                batch.xforms[i] = xform;
                i
            }
            None => {
                batch.xforms.push(xform);
                batch.xforms.len() - 1
            }
        };
        self.dirty.insert(key);
        Slot { key, index }
    }

    /// Take a solid out of its batch.
    pub fn remove(&mut self, slot: Slot) {
        if let Some(b) = self.batches.get_mut(&slot.key)
            && slot.index < b.xforms.len()
        {
            b.xforms[slot.index] = Transform3D::new(Basis::from_scale(Vector3::ZERO), b.rest);
            b.free.push(slot.index);
            self.dirty.insert(slot.key);
        }
    }

    /// Hand the changed batches to the renderer (once a frame).
    pub fn flush(&mut self) {
        for key in self.dirty.drain() {
            let Some(b) = self.batches.get_mut(&key) else {
                continue;
            };
            let n = b.xforms.len();
            if b.mm.get_instance_count() as usize != n {
                b.mm.set_instance_count(n as i32);
            }
            let mut buf = PackedFloat32Array::new();
            buf.resize(n * 12);
            let out = buf.as_mut_slice();
            for (i, t) in b.xforms.iter().enumerate() {
                let (r, o) = (t.basis.rows, t.origin);
                out[i * 12..i * 12 + 12].copy_from_slice(&[
                    r[0].x, r[0].y, r[0].z, o.x, r[1].x, r[1].y, r[1].z, o.y, r[2].x, r[2].y,
                    r[2].z, o.z,
                ]);
            }
            b.mm.set_buffer(&buf);
            // a batch of hidden slots only is not drawn at all
            let live = b.free.len() < n;
            if b.node.is_visible() != live {
                b.node.set_visible(live);
            }
        }
    }

    /// Batches and the solids in them (self-tests, profiling).
    pub fn counts(&self) -> (usize, usize) {
        let solids = self
            .batches
            .values()
            .map(|b| b.xforms.len() - b.free.len())
            .sum();
        (self.batches.len(), solids)
    }
}
