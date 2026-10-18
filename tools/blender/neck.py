"""Things worn round a hero's neck (tools/fetch_art.py runs it; see the
"blender" recipe in client/godot/art/sources.json):

    blender -b --factory-startup --python neck.py -- spec.json

Spec: "out", optional "name", "thing": "stethoscope" (the default) or
"camera". Metres.

A stethoscope is built from curves: the rubber tube, the binaural (a steel
stem forking into two ear tubes with their ear tips) at one end, the chest
piece (a bell and a diaphragm) at the other; without "rig" it lies coiled
on the floor (the map's and the icons'). A camera hangs on its strap, a
cord from its sides round the back of the neck; it rests on the
breastbone, tilted as the chest under it pushes it out (worn only: the
floor's camera is made in code).

Worn round a hero's neck, with "rig" (an outfit glTF), "head" (its base
character) and "cut" (the height the head is cut from it at, as the
manifest's `heads` say): the tube or the strap rests on that outfit's
collar and shoulders and hangs in front of its chest, placed by casting
rays at the outfit and the head; the model is skinned to the outfit's
skeleton as the outfit under it is (the tube or the strap vertex by vertex
as the outfit's nearest vertices, a rigid part whole as those nearest its
middle), so it moves with the body and never further into the head than
the collar it lies on. The client wears the one fitted to the skeleton
nearest the hero's.
"""
import bmesh
import bpy
import json
import math
import os
import sys
from mathutils import Matrix, Quaternion, Vector
from mathutils.bvhtree import BVHTree
from mathutils.kdtree import KDTree

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
LOOP = [(-25, 0.086), (-10, 0.090), (5, 0.088), (20, 0.082), (35, 0.078), (50, 0.076),
        (65, 0.076), (78, 0.077), (90, 0.078)]
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


# a camera: (metres) its body's width, height and depth; how far its middle
# hangs below the neck on the man it was drawn for (another body's by the
# width of its shoulders, as the loop); its strap's cord
CAMERA = (0.115, 0.066, 0.04)
HANG = 0.17
STRAP = 0.0045
CAMERA_BODY = material("CameraBody", (0.006, 0.006, 0.007), 0.0, 0.35)
CHROME = material("CameraChrome", (0.58, 0.6, 0.64), 0.8, 0.25)
LENS = material("CameraLens", (0.05, 0.12, 0.25), 0.0, 0.05)
CORD = material("CameraStrap", (0.03, 0.02, 0.012), 0.0, 0.7)


def box(name, at, size, mat):
    """A box centred at `at`, its faces listed here (as the ball's)."""
    (x, y, z), (w, d, h) = at, (size[0] / 2, size[1] / 2, size[2] / 2)
    verts = [(x + sx * w, y + sy * d, z + sz * h) for sz in (-1, 1) for sy in (-1, 1) for sx in (-1, 1)]
    faces = [(0, 2, 3, 1), (4, 5, 7, 6), (0, 1, 5, 4), (2, 6, 7, 3), (0, 4, 6, 2), (1, 3, 7, 5)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    obj = bpy.data.objects.new(name, mesh)
    scene.collection.objects.link(obj)
    obj.data.materials.append(mat)
    return obj


def camera_parts():
    """The camera in its own space (x its left, -y the way the lens looks,
    z up; its middle at 0): the body, the chrome plate over it, the
    viewfinder's hump on the plate, the lens and its glass."""
    w, h, d = CAMERA
    lens = Vector((-0.006, -d / 2 - 0.017, -0.002))
    return [
        box("CameraBody", (0.0, 0.0, 0.0), (w, d, h), CAMERA_BODY),
        box("CameraPlate", (0.0, 0.0, h / 2 + 0.005), (w + 0.002, d + 0.002, 0.01), CHROME),
        box("Viewfinder", (0.036, 0.0, h / 2 + 0.017), (0.028, 0.02, 0.016), CHROME),
        disc("LensBarrel", lens, 0.025, 0.034, CAMERA_BODY, (0, -1, 0)),
        disc("LensRing", lens + Vector((0, -0.016, 0)), 0.0265, 0.005, CHROME, (0, -1, 0)),
        disc("LensGlass", lens + Vector((0, -0.0185, 0)), 0.02, 0.002, LENS, (0, -1, 0)),
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
        # the outfit's vertices and how its skin weights each
        self.weighted = []
        # the head as the client cuts it from its base character
        for path, keep in ((rig, lambda f: True), (head, lambda f: min(v.z for v in f) >= cut)):
            objs, found = import_meshes(path)
            faces += [[to_spine @ v for v in f] for f in found if keep(f)]
            for o in objs:
                if path == rig and o.type == "MESH":
                    names = [g.name for g in o.vertex_groups]
                    for v in o.data.vertices:
                        w = {names[g.group]: g.weight for g in v.groups if g.weight > 0.0}
                        if w:
                            self.weighted.append((to_spine @ (o.matrix_world @ v.co), w))
                bpy.data.objects.remove(o, do_unlink=True)
        self.near = KDTree(len(self.weighted))
        for i, (p, _) in enumerate(self.weighted):
            self.near.insert(p, i)
        self.near.balance()
        verts = [v for f in faces for v in f]
        polys, k = [], 0
        for f in faces:
            polys.append(list(range(k, k + len(f))))
            k += len(f)
        self.bvh = BVHTree.FromPolygons(verts, polys)

    def weights(self, p):
        """How the outfit's skin weights the bones at `p`: its four nearest
        vertices' weights, the nearer the more, the four heaviest bones."""
        out = {}
        for _, i, d in self.near.find_n(Vector(p), 4):
            for bone, w in self.weighted[i][1].items():
                out[bone] = out.get(bone, 0.0) + w / max(d, 1e-4)
        heaviest = sorted(out.items(), key=lambda b: (-b[1], b[0]))[:4]
        total = sum(w for _, w in heaviest)
        return {bone: w / total for bone, w in heaviest}

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


def rested(body, middle):
    """Where the camera hangs with its middle at the height `middle`: its
    back on the chest (tilted out at the foot where the chest under it
    pushes further out than under its top), as (where its middle is, its
    tilt about x in radians)."""
    w, h, d = CAMERA
    rows = [middle + h / 2 - 0.004, middle, middle - h / 2 + 0.004]
    front = {}
    for z in rows:
        hits = [body.hit((x, -1.0, z), (0, 1, 0), 2.0) for x in (-w / 2 + 0.008, 0.0, w / 2 - 0.008)]
        front[z] = min(p.y for p in hits if p is not None)
    top, foot = rows[0], rows[-1]
    tilt = math.atan2(front[top] - front[foot], top - foot)
    # the back's plane through the top's contact, its normal to the front
    up = Vector((0.0, math.sin(tilt), math.cos(tilt)))
    out = Vector((0.0, -math.cos(tilt), math.sin(tilt)))
    base = Vector((0.0, front[top], top))
    # clear of every row by the gap
    clear = max(GAP + (Vector((0.0, front[z], z)) - base).dot(out) for z in rows)
    back = base + out * clear
    centre = back + out * (d / 2) + up * (middle - top)
    return centre, tilt


def worn_camera(body):
    """The camera hanging from its strap in front of the breastbone, the
    strap from its sides up the chest, over the shoulders and round the
    back of the neck."""
    k = body.shoulder / HIS_SHOULDER
    n = body.neck
    w, h, d = CAMERA
    centre, tilt = rested(body, n - HANG * (1 + k) / 2)
    # (a turn about x by -tilt leans its top back by tilt)
    place = Matrix.Translation(centre) @ Matrix.Rotation(-tilt, 4, "X")
    parts = camera_parts()
    bpy.context.view_layer.update()
    for o in parts:
        o.matrix_world = place @ o.matrix_world
    x = 0.082 * k

    def front(side, z, dx=0.0):
        xx = side * (x + dx)
        return Vector((xx, body.in_front(xx, z, STRAP), z))

    def loop(side):
        pts = [body.resting(math.radians(90 - (90 - deg) * side), r * (1 + k) / 2)
               for deg, r in LOOP]
        for i in range(1, len(pts) - 1):
            pts[i].z = max(pts[i].z, min(pts[i - 1].z, pts[i + 1].z))
        return pts if side > 0 else pts[::-1]

    def lug(side):
        return place @ Vector((side * (w / 2 + STRAP), 0.0, h / 2 - 0.012))

    def up_chest(side):
        return [lug(side), front(side, n - 0.11, -0.012), front(side, n - 0.06), front(side, n - 0.03),
                front(side, n - 0.003)]

    left = up_chest(1) + loop(1)
    right = loop(-1)[1:] + up_chest(-1)[::-1]
    parts.append(tube("Strap", left + right, STRAP, CORD))
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


# what moves as one, by how its parts' names start (the tube and the strap
# bend, vertex by vertex)
RIGID = (("Camera", "Viewfinder", "Lens"), ("Stem", "Binaural", "EarTip"),
         ("ChestStem", "Bell", "Diaphragm"))


def weigh(parts, body):
    """Weight the parts (made in spine_03's space, meshes) as the outfit
    under them is: a tube or a strap vertex by vertex as the outfit's
    nearest vertices, a rigid thing (a camera, the binaural, the chest
    piece) whole as those nearest its middle."""
    def put(o, index, weights):
        for bone, w in weights.items():
            group = o.vertex_groups.get(bone) or o.vertex_groups.new(name=bone)
            group.add([index], w, "REPLACE")

    rigid = set()
    for starts in RIGID:
        together = [o for o in parts if o.name.startswith(starts)]
        if not together:
            continue
        rigid.update(together)
        at = [o.matrix_world @ v.co for o in together for v in o.data.vertices]
        weights = body.weights(sum(at, Vector()) / len(at))
        for o in together:
            for v in o.data.vertices:
                put(o, v.index, weights)
    for o in parts:
        if o in rigid:
            continue
        for v in o.data.vertices:
            put(o, v.index, body.weights(o.matrix_world @ v.co))


def skin(obj, rig, body):
    """Place `obj` (made in spine_03's space, weighted) on the rig's rest
    skeleton and parent it to the armature."""
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
thing = spec.get("thing", "stethoscope")
if thing == "camera":
    parts = worn_camera(body)
else:
    parts = worn(body) if body else lying()
bpy.ops.object.select_all(action="DESELECT")
for o in parts:
    o.select_set(True)
bpy.context.view_layer.objects.active = parts[0]
names = [o.name for o in parts]
bpy.ops.object.convert(target="MESH")
if body:
    weigh([bpy.data.objects[n] for n in names], body)
bpy.ops.object.join()
obj = bpy.context.view_layer.objects.active
obj.name = obj.data.name = spec.get("name", thing.capitalize())
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
