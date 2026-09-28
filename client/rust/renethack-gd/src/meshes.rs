//! Primitive meshes by shape, in centimetres so they can be cached and
//! compared: the map's own geometry and the procedural bodies of `art.rs`.

use godot::classes::{
    BoxMesh, CapsuleMesh, CylinderMesh, Mesh, PlaneMesh, PrismMesh, SphereMesh, TorusMesh,
};
use godot::prelude::*;

/// Mesh sizes in centimetres, so meshes can be cached by shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshKey {
    /// Width (x), height (y), depth (z).
    Box(u16, u16, u16),
    /// Width (x), depth (z); faces up.
    Plane(u16, u16),
    /// Radius, total height.
    Capsule(u16, u16),
    /// Top radius, bottom radius, height.
    Cylinder(u16, u16, u16),
    Sphere(u16),
    /// The upper half of a sphere, flat side down at y = 0.
    Dome(u16),
    /// A faceted sphere (cut gems, rough stones): radius, sides.
    Facets(u16, u8),
    /// Inner and outer radius; lies flat.
    Torus(u16, u16),
    /// A triangular prism: width (x), height (y), depth (z); apex up.
    Prism(u16, u16, u16),
}

pub fn cm(metres: f32) -> u16 {
    (metres * 100.0).round().clamp(0.0, f32::from(u16::MAX)) as u16
}

pub fn metres(cm: u16) -> f32 {
    f32::from(cm) / 100.0
}

pub fn cuboid(x: f32, y: f32, z: f32) -> MeshKey {
    MeshKey::Box(cm(x), cm(y), cm(z))
}

pub fn plane(x: f32, z: f32) -> MeshKey {
    MeshKey::Plane(cm(x), cm(z))
}

pub fn capsule(radius: f32, height: f32) -> MeshKey {
    MeshKey::Capsule(cm(radius), cm(height.max(2.0 * radius)))
}

pub fn cylinder(top: f32, bottom: f32, height: f32) -> MeshKey {
    MeshKey::Cylinder(cm(top), cm(bottom), cm(height))
}

pub fn sphere(radius: f32) -> MeshKey {
    MeshKey::Sphere(cm(radius))
}

pub fn dome(radius: f32) -> MeshKey {
    MeshKey::Dome(cm(radius))
}

pub fn facets(radius: f32, sides: u8) -> MeshKey {
    MeshKey::Facets(cm(radius), sides)
}

pub fn torus(inner: f32, outer: f32) -> MeshKey {
    MeshKey::Torus(cm(inner), cm(outer))
}

pub fn prism(x: f32, y: f32, z: f32) -> MeshKey {
    MeshKey::Prism(cm(x), cm(y), cm(z))
}

impl MeshKey {
    /// Half the height of the upright mesh.
    pub fn half_height(self) -> f32 {
        match self {
            MeshKey::Box(_, y, _) | MeshKey::Prism(_, y, _) => metres(y) / 2.0,
            MeshKey::Plane(..) => 0.0,
            MeshKey::Capsule(_, h) | MeshKey::Cylinder(_, _, h) => metres(h) / 2.0,
            MeshKey::Sphere(r) | MeshKey::Facets(r, _) => metres(r),
            MeshKey::Dome(r) => metres(r) / 2.0,
            MeshKey::Torus(i, o) => metres(o.saturating_sub(i)) / 2.0,
        }
    }

    pub fn build(self) -> Gd<Mesh> {
        match self {
            MeshKey::Box(x, y, z) => {
                let mut m = BoxMesh::new_gd();
                m.set_size(Vector3::new(metres(x), metres(y), metres(z)));
                m.upcast()
            }
            MeshKey::Plane(x, z) => {
                let mut m = PlaneMesh::new_gd();
                m.set_size(Vector2::new(metres(x), metres(z)));
                m.upcast()
            }
            MeshKey::Capsule(r, h) => {
                let mut m = CapsuleMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(metres(h));
                m.set_radial_segments(16);
                m.set_rings(4);
                m.upcast()
            }
            MeshKey::Cylinder(t, b, h) => {
                let mut m = CylinderMesh::new_gd();
                m.set_top_radius(metres(t));
                m.set_bottom_radius(metres(b));
                m.set_height(metres(h));
                m.set_radial_segments(16);
                m.set_rings(1);
                m.upcast()
            }
            MeshKey::Sphere(r) => {
                let mut m = SphereMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(2.0 * metres(r));
                m.set_radial_segments(18);
                m.set_rings(9);
                m.upcast()
            }
            MeshKey::Dome(r) => {
                let mut m = SphereMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(metres(r));
                m.set_is_hemisphere(true);
                m.set_radial_segments(18);
                m.set_rings(5);
                m.upcast()
            }
            MeshKey::Facets(r, sides) => {
                let mut m = SphereMesh::new_gd();
                m.set_radius(metres(r));
                m.set_height(2.0 * metres(r));
                m.set_radial_segments(i32::from(sides.max(3)));
                m.set_rings(i32::from(sides.max(3)) / 2);
                m.upcast()
            }
            MeshKey::Torus(i, o) => {
                let mut m = TorusMesh::new_gd();
                m.set_inner_radius(metres(i));
                m.set_outer_radius(metres(o));
                m.set_rings(24);
                m.set_ring_segments(8);
                m.upcast()
            }
            MeshKey::Prism(x, y, z) => {
                let mut m = PrismMesh::new_gd();
                m.set_size(Vector3::new(metres(x), metres(y), metres(z)));
                m.upcast()
            }
        }
    }
}
