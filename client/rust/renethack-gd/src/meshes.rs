//! Primitive meshes by shape, in centimetres so they can be cached and
//! compared: the map's own geometry and the procedural bodies of `art.rs`.

use godot::classes::mesh::PrimitiveType;
use godot::classes::{
    BoxMesh, CapsuleMesh, CylinderMesh, Mesh, PlaneMesh, PrismMesh, SphereMesh, SurfaceTool,
    TorusMesh,
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
    /// A box with chamfered edges (masonry: the chamfers catch the light):
    /// width, height, depth, chamfer.
    Bevel(u16, u16, u16, u16),
    /// A block of rock: a box standing on y = 0 whose top and sides are
    /// fine grids (the map's shader breaks them up), without a bottom:
    /// width, height, depth.
    Rock(u16, u16, u16),
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

/// A box with 3 cm chamfers (less on a small one).
pub fn bevel(x: f32, y: f32, z: f32) -> MeshKey {
    let r = (x.min(y).min(z) * 0.3).min(0.03);
    MeshKey::Bevel(cm(x), cm(y), cm(z), cm(r).max(1))
}

pub fn rock(x: f32, y: f32, z: f32) -> MeshKey {
    MeshKey::Rock(cm(x), cm(y), cm(z))
}

impl MeshKey {
    /// Half the height of the upright mesh.
    pub fn half_height(self) -> f32 {
        match self {
            MeshKey::Box(_, y, _) | MeshKey::Prism(_, y, _) | MeshKey::Bevel(_, y, _, _) => {
                metres(y) / 2.0
            }
            MeshKey::Rock(_, y, _) => metres(y),
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
            MeshKey::Bevel(x, y, z, r) => {
                let half = Vector3::new(metres(x), metres(y), metres(z)) / 2.0;
                bevel_mesh(half, metres(r))
            }
            MeshKey::Rock(x, y, z) => rock_mesh(Vector3::new(metres(x), metres(y), metres(z))),
        }
    }
}

/// Triangles with flat normals, each wound to face along its normal.
struct Tris {
    st: Gd<SurfaceTool>,
}

impl Tris {
    fn new() -> Tris {
        let mut st = SurfaceTool::new_gd();
        st.begin(PrimitiveType::TRIANGLES);
        Tris { st }
    }

    fn tri(&mut self, a: Vector3, b: Vector3, c: Vector3, n: Vector3) {
        // Godot's front faces wind clockwise seen from outside
        let (b, c) = if (b - a).cross(c - a).dot(n) > 0.0 {
            (c, b)
        } else {
            (b, c)
        };
        for v in [a, b, c] {
            self.st.set_normal(n);
            // UVs from the position, for materials that are not triplanar
            self.st.set_uv(Vector2::new(v.x + v.z, v.y + v.z));
            self.st.add_vertex(v);
        }
    }

    fn quad(&mut self, q: [Vector3; 4], n: Vector3) {
        self.tri(q[0], q[1], q[2], n);
        self.tri(q[0], q[2], q[3], n);
    }

    fn done(mut self) -> Gd<Mesh> {
        // corners shared by triangles of one face are one vertex
        self.st.index();
        self.st.generate_tangents();
        match self.st.commit() {
            Some(m) => m.upcast(),
            None => BoxMesh::new_gd().upcast(),
        }
    }
}

/// A box of these half sizes with its edges cut at 45 degrees by `r`: six
/// faces, twelve chamfer strips and eight corner triangles.
fn bevel_mesh(h: Vector3, r: f32) -> Gd<Mesh> {
    let r = r.min(h.x).min(h.y).min(h.z).max(0.0);
    let i = h - Vector3::new(r, r, r);
    // the three points of a corner (signs s), pushed out along x, y or z
    let px = |s: Vector3| Vector3::new(s.x * h.x, s.y * i.y, s.z * i.z);
    let py = |s: Vector3| Vector3::new(s.x * i.x, s.y * h.y, s.z * i.z);
    let pz = |s: Vector3| Vector3::new(s.x * i.x, s.y * i.y, s.z * h.z);
    let v = Vector3::new;
    let mut t = Tris::new();
    for a in [-1.0f32, 1.0] {
        let quads = [
            // faces
            (
                [
                    px(v(a, -1., -1.)),
                    px(v(a, 1., -1.)),
                    px(v(a, 1., 1.)),
                    px(v(a, -1., 1.)),
                ],
                v(a, 0., 0.),
            ),
            (
                [
                    py(v(-1., a, -1.)),
                    py(v(1., a, -1.)),
                    py(v(1., a, 1.)),
                    py(v(-1., a, 1.)),
                ],
                v(0., a, 0.),
            ),
            (
                [
                    pz(v(-1., -1., a)),
                    pz(v(1., -1., a)),
                    pz(v(1., 1., a)),
                    pz(v(-1., 1., a)),
                ],
                v(0., 0., a),
            ),
        ];
        for (q, n) in quads {
            t.quad(q, n);
        }
        for b in [-1.0f32, 1.0] {
            // chamfers along z (between x and y faces), x and y
            t.quad(
                [
                    px(v(a, b, -1.)),
                    py(v(a, b, -1.)),
                    py(v(a, b, 1.)),
                    px(v(a, b, 1.)),
                ],
                v(a, b, 0.).normalized(),
            );
            t.quad(
                [
                    py(v(-1., a, b)),
                    pz(v(-1., a, b)),
                    pz(v(1., a, b)),
                    py(v(1., a, b)),
                ],
                v(0., a, b).normalized(),
            );
            t.quad(
                [
                    px(v(a, -1., b)),
                    pz(v(a, -1., b)),
                    pz(v(a, 1., b)),
                    px(v(a, 1., b)),
                ],
                v(a, 0., b).normalized(),
            );
            for c in [-1.0f32, 1.0] {
                let s = v(a, b, c);
                t.tri(px(s), py(s), pz(s), s.normalized());
            }
        }
    }
    t.done()
}

/// Cells of the grid on a rock block's faces (metres).
const ROCK_GRID: f32 = 0.2;

/// A block standing on y = 0 of this size, its top and four sides in
/// squares of about `ROCK_GRID`, open at the bottom.
fn rock_mesh(size: Vector3) -> Gd<Mesh> {
    let (hx, hz, y) = (size.x / 2.0, size.z / 2.0, size.y);
    let n = |len: f32| ((len / ROCK_GRID).round() as usize).max(1);
    let mut t = Tris::new();
    // a face from its corner `o` along `u` and `w`
    let mut face = |o: Vector3, u: Vector3, w: Vector3, normal: Vector3| {
        let (nu, nw) = (n(u.length()), n(w.length()));
        for i in 0..nu {
            for j in 0..nw {
                let p = |a: usize, b: usize| {
                    o + u * (a as f32 / nu as f32) + w * (b as f32 / nw as f32)
                };
                t.quad([p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)], normal);
            }
        }
    };
    let v = Vector3::new;
    face(
        v(-hx, y, -hz),
        v(size.x, 0., 0.),
        v(0., 0., size.z),
        Vector3::UP,
    );
    face(
        v(-hx, 0., hz),
        v(size.x, 0., 0.),
        v(0., y, 0.),
        Vector3::BACK,
    );
    face(
        v(-hx, 0., -hz),
        v(size.x, 0., 0.),
        v(0., y, 0.),
        Vector3::FORWARD,
    );
    face(
        v(-hx, 0., -hz),
        v(0., 0., size.z),
        v(0., y, 0.),
        Vector3::LEFT,
    );
    face(
        v(hx, 0., -hz),
        v(0., 0., size.z),
        v(0., y, 0.),
        Vector3::RIGHT,
    );
    t.done()
}
