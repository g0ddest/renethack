"""A stethoscope (tools/fetch_art.py runs it; see the "blender" recipe in
client/godot/art/sources.json):

    blender -b --factory-startup --python stethoscope.py -- spec.json

Built from curves: the rubber tube, the binaural (a steel stem forking
into two ear tubes with their ear tips) at one end, the chest piece (a
bell and a diaphragm) at the other. Metres. Spec: "out", optional "name";
without "rig" it lies coiled on the floor (the map's and the icons').

Worn round a hero's neck, with "rig" (an outfit glTF), "head" (its base
character) and "cut" (the height the head is cut from it at, as the
manifest's `heads` say): the tube rests on that outfit's collar and
shoulders and hangs in front of its chest, placed by casting rays at the
outfit and the head; the model is skinned to the outfit's skeleton (the
chest parts to spine_03, those over the shoulders partly to the
clavicles, those behind the neck partly to neck_01), so it moves with the
body. The client wears the one fitted to the skeleton nearest the hero's.
"""
import bmesh
import bpy
import json
import math
import os
import sys
from mathutils import Matrix, Quaternion, Vector
from mathutils.bvhtree import BVHTree

spec = json.load(open(sys.argv[sys.argv.index("--") + 1]))
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene

TUBE = 0.0065
# clear of the body
GAP = 0.004
# glTF's axes to Blender's
TO_BLENDER = Matrix(((1, 0, 0, 0), (0, 0, -1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))
# the loop round the neck on the man it was drawn for: (degrees from his
# left round the back, distance from the neck's axis); another body's is
# scaled by the width of its shoulders against his
LOOP = [(-25, 0.086), (-10, 0.090), (5, 0.088), (20, 0.080), (35, 0.072), (50, 0.066),
        (65, 0.062), (78, 0.060), (90, 0.060)]
HIS_SHOULDER = 0.192


def material(name, color, metallic, roughness):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    return m


RUBBER = material("Tubing", (0.006, 0.007, 0.008), 0.0, 0.5)
STEEL = material("Steel", (0.62, 0.64, 0.68), 0.5, 0.3)
TIPS = material("EarTips", (0.004, 0.004, 0.004), 0.0, 0.6)


def tube(name, points, radius, mat, resolution=12):
    """A smooth tube through `points` (Blender space: z up, the front -y)."""
    curve = bpy.data.curves.new(name, "CURVE")
    curve.dimensions = "3D"
    curve.bevel_depth = radius
    curve.bevel_resolution = 3
    curve.resolution_u = resolution
    curve.use_fill_caps = True
    s = curve.splines.new("BEZIER")
    s.bezier_points.add(len(points) - 1)
    for bp, p in zip(s.bezier_points, points):
        bp.co = Vector(p)
        bp.handle_left_type = bp.handle_right_type = "AUTO"
    obj = bpy.data.objects.new(name, curve)
    scene.collection.objects.link(obj)
    obj.data.materials.append(mat)
    return obj


def disc(name, at, radius, depth, mat, axis):
    """A short cylinder at `at`, its axis along `axis`."""
    bpy.ops.mesh.primitive_cylinder_add(vertices=20, radius=radius, depth=depth, location=at)
    obj = bpy.context.active_object
    obj.name = name
    obj.rotation_mode = "QUATERNION"
    obj.rotation_quaternion = Vector((0, 0, 1)).rotation_difference(Vector(axis))
    obj.data.materials.append(mat)
    return obj


def ball(name, at, radius, mat, segments=12, rings=8):
    """A sphere at `at`, its faces listed here: those of Blender's own
    come in an order that changes from run to run."""
    verts = [(0.0, 0.0, radius)]
    for r in range(1, rings):
        phi = math.pi * r / rings
        for k in range(segments):
            th = 2 * math.pi * k / segments
            verts.append((radius * math.sin(phi) * math.cos(th),
                          radius * math.sin(phi) * math.sin(th), radius * math.cos(phi)))
    verts.append((0.0, 0.0, -radius))

    def ring(r, k):
        return 1 + (r - 1) * segments + k % segments

    faces = []
    for k in range(segments):
        faces.append((0, ring(1, k), ring(1, k + 1)))
        for r in range(1, rings - 1):
            faces.append((ring(r, k), ring(r + 1, k), ring(r + 1, k + 1), ring(r, k + 1)))
        faces.append((len(verts) - 1, ring(rings - 1, k + 1), ring(rings - 1, k)))
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([Vector(v) + Vector(at) for v in verts], [], faces)
    obj = bpy.data.objects.new(name, mesh)
    scene.collection.objects.link(obj)
    obj.data.materials.append(mat)
    return obj


def binaural(stem, fork, tips):
    """The steel stem from the tube's end to the fork, the two ear tubes
    from the fork to `tips`, and the ear tips."""
    out = [tube("Stem", [stem, fork], 0.004, STEEL, 4)]
    for name, tip in zip("LR", tips):
        mid = Vector(fork).lerp(Vector(tip), 0.45)
        out.append(tube(f"Binaural{name}", [fork, mid, tip], 0.0028, STEEL, 8))
        out.append(ball(f"EarTip{name}", tip, 0.0065, TIPS))
    return out


def chest_piece(end, centre, face):
    """The chest piece hanging from the tube's `end`: a short stem, the bell
    and the wider diaphragm in front of it, facing `face`."""
    e, c, f = Vector(end), Vector(centre), Vector(face).normalized()
    return [
        tube("ChestStem", [e, c + (e - c).normalized() * 0.017], 0.004, STEEL, 4),
        disc("Bell", c - f * 0.004, 0.017, 0.012, STEEL, f),
        disc("Diaphragm", c + f * 0.005, 0.024, 0.006, STEEL, f),
    ]


def joint_world(gltf, name):
    """A joint's rest transform in the glTF's model space."""
    nodes = gltf["nodes"]
    parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}
    i = next(k for k, n in enumerate(nodes) if n.get("name") == name)
    m = Matrix.Identity(4)
    while i is not None:
        n = nodes[i]
        x, y, z, w = n.get("rotation", [0, 0, 0, 1])
        t = Matrix.Translation(n.get("translation", [0, 0, 0]))
        r = Quaternion((w, x, y, z)).to_matrix().to_4x4()
        sx, sy, sz = n.get("scale", [1, 1, 1])
        m = t @ r @ Matrix.Diagonal((sx, sy, sz, 1.0)) @ m
        i = parent.get(i)
    return m


def import_meshes(path):
    """The glTF's objects, and its meshes' faces in Blender space."""
    before = set(scene.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    objs = sorted(set(scene.objects) - before, key=lambda o: o.name)
    faces = []
    for o in objs:
        if o.type != "MESH":
            continue
        vs = [o.matrix_world @ v.co for v in o.data.vertices]
        faces += [[vs[i] for i in p.vertices] for p in o.data.polygons]
    return objs, faces


class Body:
    """An outfit and its base head in spine_03's rest space (Blender axes:
    x its left, -y its front, z up the spine), to cast rays at."""

    def __init__(self, rig, head, cut):
        gltf = json.load(open(rig))
        self.place = TO_BLENDER @ joint_world(gltf, "spine_03") @ TO_BLENDER.inverted()
        to_spine = self.place.inverted()

        def at(joint):
            return to_spine @ (TO_BLENDER @ joint_world(gltf, joint).to_translation())

        self.neck = at("neck_01").z
        self.shoulder = abs(at("upperarm_l").x)
        faces = []
        # the head as the client cuts it from its base character
        for path, keep in ((rig, lambda f: True), (head, lambda f: min(v.z for v in f) >= cut)):
            objs, found = import_meshes(path)
            faces += [[to_spine @ v for v in f] for f in found if keep(f)]
            for o in objs:
                bpy.data.objects.remove(o, do_unlink=True)
        verts = [v for f in faces for v in f]
        polys, k = [], 0
        for f in faces:
            polys.append(list(range(k, k + len(f))))
            k += len(f)
        self.bvh = BVHTree.FromPolygons(verts, polys)

    def hit(self, origin, direction, reach):
        return self.bvh.ray_cast(Vector(origin), Vector(direction), reach)[0]

    def resting(self, phi, r):
        """Where a tube lies at distance r toward phi round the neck: on what
        is under it (not down a gap by the collar), clear of the neck."""
        while True:
            h = None
            for dr in (-TUBE - GAP, -TUBE / 2, 0.0, TUBE / 2, TUBE + GAP):
                for dphi in (-0.12, -0.06, 0.0, 0.06, 0.12):
                    a = phi + dphi
                    p = self.hit((math.cos(a) * (r + dr), math.sin(a) * (r + dr),
                                  self.neck + 0.10), (0, 0, -1), 1.0)
                    if p is not None and p.z > self.neck - 0.03 and (h is None or p.z > h):
                        h = p.z
            z = (h if h is not None else self.neck) + TUBE + GAP
            d = Vector((math.cos(phi), math.sin(phi), 0))
            neck = self.hit(Vector((0, 0, z)) + d * 0.3, -d, 0.3)
            if neck is None or (neck - Vector((0, 0, z))).dot(d) + TUBE + GAP <= r:
                return Vector((d.x * r, d.y * r, z))
            r += 0.002

    def in_front(self, x, z, size=TUBE, depth=None):
        """The y of the middle of a thing resting in front of the chest at
        (x, z): `size` round, `depth` from its middle to its back (`size`)."""
        y = 1.0
        for dx in (-size, 0.0, size):
            for dz in (-size, 0.0, size):
                p = self.hit((x + dx, -1.0, z + dz), (0, 1, 0), 2.0)
                if p is not None:
                    y = min(y, p.y)
        return y - (size if depth is None else depth) - GAP


def worn(body):
    """The tube from the binaural's stem on the left of the chest, over the
    left shoulder, round the back of the neck and down the right of the
    chest to the chest piece, which hangs lower."""
    k = body.shoulder / HIS_SHOULDER
    n = body.neck
    x = 0.082 * k

    def front(side, dz):
        return Vector((side * x, body.in_front(side * x, n + dz), n + dz))

    def loop(side):
        # a slighter body's loop is a little closer to the neck, on the
        # shoulders still; a point is never lower than both its neighbours
        # (the tube bridges a dip in the collar)
        pts = [body.resting(math.radians(90 - (90 - deg) * side), r * (1 + k) / 2)
               for deg, r in LOOP]
        for i in range(1, len(pts) - 1):
            pts[i].z = max(pts[i].z, min(pts[i - 1].z, pts[i + 1].z))
        return pts if side > 0 else pts[::-1]

    left = [front(1, -0.05), front(1, -0.03), front(1, -0.003)] + loop(1)
    right = loop(-1)[1:] + [front(-1, dz) for dz in (-0.003, -0.03, -0.055, -0.075, -0.097)]
    parts = [tube("Tube", left + right, TUBE, RUBBER)]
    # the ear tubes hang from the stem, the tips down
    stem = left[0]
    z = stem.z - 0.02
    fork = Vector((stem.x, body.in_front(stem.x, z, 0.004), z))
    tips = []
    for dx in (0.02 * k, -0.02 * k):
        z = fork.z - 0.05
        tips.append(Vector((fork.x + dx, body.in_front(fork.x + dx, z), z)))
    parts += binaural(stem, fork, tips)
    end = right[-1]
    z = end.z - 0.034
    centre = Vector((end.x, body.in_front(end.x, z, 0.024, 0.012), z))
    parts += chest_piece(end, centre, (0.0, -1.0, -0.2))
    return parts


def lying():
    """Coiled on the floor (z up, resting on z = 0): the binaural at one end
    of a loose loop of tube, the chest piece flat at the other."""
    z = TUBE
    path = [(0.085, 0.07, z), (0.03, 0.105, z), (-0.045, 0.09, z), (-0.095, 0.03, z),
            (-0.09, -0.045, z), (-0.035, -0.095, z), (0.03, -0.1, z)]
    parts = [tube("Tube", path, TUBE, RUBBER)]
    fork = (0.105, 0.077, z)
    parts += binaural((0.085, 0.07, z), fork, [(0.15, 0.06, z), (0.148, 0.11, z)])
    end = Vector((0.03, -0.1, z))
    centre = end + Vector((1.0, -0.15, 0.0)).normalized() * 0.042
    parts += [
        tube("ChestStem", [end, end + (centre - end).normalized() * 0.016], 0.004, STEEL, 4),
        disc("Diaphragm", (centre.x, centre.y, 0.003), 0.024, 0.006, STEEL, (0, 0, 1)),
        disc("Bell", (centre.x, centre.y, 0.012), 0.017, 0.012, STEEL, (0, 0, 1)),
    ]
    return parts


def smooth(a, b, x):
    t = min(1.0, max(0.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def skin(obj, rig, body):
    """Weight `obj` (made in spine_03's space) by where each vertex lies,
    place it on the rig's rest skeleton and parent it to the armature."""
    k = body.shoulder / HIS_SHOULDER
    groups = {n: obj.vertex_groups.new(name=n)
              for n in ("spine_03", "neck_01", "clavicle_l", "clavicle_r")}
    for v in obj.data.vertices:
        x, y, z = v.co
        up = smooth(body.neck - 0.06, body.neck - 0.01, z)
        behind = smooth(0.0, 0.045, y)
        side = smooth(0.03 * k, 0.065 * k, abs(x))
        clav = 0.6 * up * side * (1.0 - behind)
        neck = 0.5 * up * behind
        groups["clavicle_l" if x > 0 else "clavicle_r"].add([v.index], clav, "REPLACE")
        groups["neck_01"].add([v.index], neck, "REPLACE")
        groups["spine_03"].add([v.index], max(0.0, 1.0 - clav - neck), "REPLACE")
    obj.data.transform(body.place)
    objs, _ = import_meshes(rig)
    arm = next(o for o in objs if o.type == "ARMATURE")
    for o in objs:
        if o.type == "MESH":
            bpy.data.objects.remove(o, do_unlink=True)
    obj.parent = arm
    mod = obj.modifiers.new("Armature", "ARMATURE")
    mod.object = arm


body = Body(spec["rig"], spec["head"], spec["cut"]) if "rig" in spec else None
parts = worn(body) if body else lying()
bpy.ops.object.select_all(action="DESELECT")
for o in parts:
    o.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
bpy.ops.object.convert(target="MESH")
bpy.ops.object.join()
obj = bpy.context.view_layer.objects.active
obj.name = obj.data.name = spec.get("name", "Stethoscope")
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
# quads cut the same way every time
bm = bmesh.new()
bm.from_mesh(obj.data)
bmesh.ops.triangulate(bm, faces=bm.faces[:], quad_method="FIXED", ngon_method="EAR_CLIP")
bm.to_mesh(obj.data)
bm.free()
for poly in obj.data.polygons:
    poly.use_smooth = True
if body:
    skin(obj, spec["rig"], body)
print(f"{obj.name}: {len(obj.data.vertices)} vertices, "
      f"{sum(len(p.vertices) - 2 for p in obj.data.polygons)} triangles")
os.makedirs(os.path.dirname(spec["out"]), exist_ok=True)
bpy.ops.export_scene.gltf(filepath=spec["out"], export_format="GLB", export_skins=True,
                          export_animations=False)
if spec.get("save_blend"):
    bpy.ops.wm.save_as_mainfile(filepath=spec["save_blend"])
