"""Rig a static four-legged model with the skeleton and clips of a
Quaternius animal (tools/fetch_art.py runs it; see the "blender" recipe in
client/godot/art/sources.json):

    blender -b --factory-startup --python rig_quadruped.py -- spec.json

The spec names the template ("template": a glTF of the Ultimate Animated
Animal Pack, whose AnimalArmature drives every animal of the pack), the
model to rig ("target", glTF), the output ("out", GLB) and the clips to keep
("actions": {"new name": "template's name"}). The target is scaled to the
template's length and stood on the ground facing the template's way; the
template's joints are moved into its legs, body, neck and tail, each bone
keeping its own direction, so the clips (rotations relative to the rest
pose) move the new body as they moved the old one. The weights come from
Blender's bone heat over bones stretched to their children, and from the
nearest bones where that finds nothing.

Optional keys: "turn" (degrees about the up axis to face the template's
way), "feet", "head", "neck", "tail" (landmarks in the scaled target's
space, when finding them fails), "tail_scale", "width_scale", "smooth",
"coat" ("#rrggbb": the body's painted texture in shades of this colour, as
light on the whole as it), "materials" ({"name": "#rrggbb"}: the faces of a
body painted from a palette texture go to flat materials so named, each to
the colour nearest its own),
"parts" (tubes added along given bones, one on each side unless "mirror" is
false: antennae, a beak; "taper" is how much thinner a tube ends than it
starts, 0.8 unless said, "tip" how far its point stands out for its
radius: a flat one is an eye).
"""
import bpy
import bmesh
import json
import math
import numpy
import os
import sys
from mathutils import Matrix, Vector

spec = json.load(open(sys.argv[sys.argv.index("--") + 1]))
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene


def world_bounds(objs):
    lo = Vector((1e9,) * 3)
    hi = Vector((-1e9,) * 3)
    for o in objs:
        for v in o.data.vertices:
            w = o.matrix_world @ v.co
            lo = Vector(map(min, lo, w))
            hi = Vector(map(max, hi, w))
    return lo, hi


# ---- the template's skeleton and clips; its own meshes go -------------------
bpy.ops.import_scene.gltf(filepath=spec["template"])
arm = next(o for o in scene.objects if o.type == "ARMATURE")
body = [o for o in scene.objects if o.type == "MESH" and o.parent == arm]
t_lo, t_hi = world_bounds(body)
t_verts = [o.matrix_world @ v.co for o in body for v in o.data.vertices]
for o in list(scene.objects):
    if o.type == "MESH" or (o.type == "EMPTY" and o.name.endswith("_end")):
        bpy.data.objects.remove(o, do_unlink=True)

# ---- the target, in the template's units, as one welded surface --------------
before = set(scene.objects)
bpy.ops.import_scene.gltf(filepath=spec["target"])
new = [o for o in scene.objects if o not in before]
meshes = [o for o in new if o.type == "MESH"]
bpy.ops.object.select_all(action="DESELECT")
for o in meshes:
    o.select_set(True)
bpy.context.view_layer.objects.active = meshes[0]
bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
if len(meshes) > 1:
    bpy.ops.object.join()
target = bpy.context.view_layer.objects.active
for o in new:
    if o != target and o.name in bpy.data.objects:
        bpy.data.objects.remove(o, do_unlink=True)
target.name = target.data.name = spec["name"]
target.rotation_euler.rotate(Matrix.Rotation(math.radians(spec.get("turn", 0.0)), 3, "Z").to_euler())
bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def srgb(colour):
    return [int(colour[i:i + 2], 16) / 255 for i in (1, 3, 5)]


def painted(material):
    """The image a material takes its colour from."""
    for node in material.node_tree.nodes:
        if node.type == "TEX_IMAGE" and node.image and any(
                link.to_socket.name == "Base Color" for link in node.outputs["Color"].links):
            return node.image
    raise SystemExit(f"{spec['name']}: {material.name} is not painted from an image")


def texels(me):
    """Per face: its area, and where its middle lies in its material's
    image (the image's pixels, the offset of the texel)."""
    images = [painted(m) for m in me.materials]
    pixels = [numpy.array(image.pixels, dtype=numpy.float32) for image in images]
    uvs = me.uv_layers.active.data
    for poly in me.polygons:
        w, h = images[poly.material_index].size
        u = sum(uvs[i].uv.x for i in poly.loop_indices) / poly.loop_total
        v = sum(uvs[i].uv.y for i in poly.loop_indices) / poly.loop_total
        at = 4 * (min(int(v % 1 * h), h - 1) * w + min(int(u % 1 * w), w - 1))
        yield poly.area, pixels[poly.material_index], at


if spec.get("coat"):
    # a coat painted too dark for a dungeon, or another beast's: its
    # strokes kept in shades of the colour asked, the body as light on the
    # whole as that colour
    me = target.data
    seen = [(area, px[at:at + 3].mean()) for area, px, at in texels(me)]
    mean = sum(a * light for a, light in seen) / sum(a for a, _ in seen)
    for image in dict.fromkeys(painted(m) for m in me.materials):
        px = numpy.array(image.pixels, dtype=numpy.float32).reshape(-1, 4)
        light = px[:, :3].mean(axis=1, keepdims=True) / mean
        px[:, :3] = numpy.clip(numpy.array(srgb(spec["coat"]), dtype=numpy.float32) * light, 0, 1)
        image.pixels.foreach_set(px.ravel())
        image.pack()
if spec.get("materials"):
    # a body painted from a palette texture: its faces sorted into flat
    # materials by their colour, so the coat can be dyed apart from the skin
    def linear(c):
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    me = target.data
    names = sorted(spec["materials"])
    colours = [srgb(spec["materials"][n]) for n in names]
    slots = [min(range(len(names)),
                 key=lambda i: sum((a - b) ** 2 for a, b in zip(colours[i], px[at:at + 3])))
             for _, px, at in texels(me)]
    me.materials.clear()
    for name, colour in zip(names, colours):
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        bsdf = mat.node_tree.nodes["Principled BSDF"]
        bsdf.inputs["Base Color"].default_value = (*map(linear, colour), 1.0)
        bsdf.inputs["Roughness"].default_value = 1.0
        me.materials.append(mat)
    for poly, slot in zip(me.polygons, slots):
        poly.material_index = slot
lo, hi = world_bounds([target])
length_t = t_hi.y - t_lo.y
k = length_t / (hi.y - lo.y)
for v in target.data.vertices:
    v.co = Vector(((v.co.x - (lo.x + hi.x) / 2) * k,
                   (v.co.y - (lo.y + hi.y) / 2) * k + (t_lo.y + t_hi.y) / 2,
                   (v.co.z - lo.z) * k + t_lo.z))
# faces that only share positions get the same weights: no cracks where it
# bends (the facets stay flat)
bm = bmesh.new()
bm.from_mesh(target.data)
bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4 * length_t)
bm.to_mesh(target.data)
bm.free()
for poly in target.data.polygons:
    poly.use_smooth = spec.get("smooth", False)
target.data.update()
lo, hi = world_bounds([target])
H = hi.z - lo.z
L = hi.y - lo.y
verts = [target.matrix_world @ v.co for v in target.data.vertices]

# ---- landmarks -----------------------------------------------------------------
mw = arm.matrix_world
bones = arm.data.bones


def joint(name):
    return mw @ bones[name].head_local


# leg: (its upper bone, its IK target, side, front -1 / back +1)
LEGS = {
    "FrontL": ("FrontUpperLeg.L", "IKFrontLeg.L", +1, -1),
    "FrontR": ("FrontUpperLeg.R", "IKFrontLeg.R", -1, -1),
    "BackL": ("BackUpperLeg.L", "IKBackLeg.L", +1, +1),
    "BackR": ("BackUpperLeg.R", "IKBackLeg.R", -1, +1),
}
ymid = (lo.y + hi.y) / 2
low = [v for v in verts if v.z < lo.z + spec.get("foot_band", 0.12) * H]


def find_foot(sx, sy):
    pts = [v for v in low if v.x * sx > 0.05 * (hi.x - lo.x) and (v.y - ymid) * sy > 0]
    if not pts:
        raise SystemExit(f"{spec['name']}: no foot on side {sx}, {sy}")
    c = sum(pts, Vector()) / len(pts)
    return Vector((c.x, c.y, lo.z))


feet = {leg: Vector(spec["feet"][leg]) if leg in spec.get("feet", {}) else find_foot(sx, sy)
        for leg, (_, _, sx, sy) in LEGS.items()}


def top_at(y, vs, band, default):
    pts = [v for v in vs if abs(v.y - y) < band]
    return max(v.z for v in pts) if pts else default


tops_t = {leg: joint(up) for leg, (up, _, _, _) in LEGS.items()}
feet_t = {leg: Vector((joint(ik).x, joint(ik).y, t_lo.z)) for leg, (_, ik, _, _) in LEGS.items()}
front_y_t = (tops_t["FrontL"].y + tops_t["FrontR"].y) / 2
back_y_t = (tops_t["BackL"].y + tops_t["BackR"].y) / 2
front_y = (feet["FrontL"].y + feet["FrontR"].y) / 2
back_y = (feet["BackL"].y + feet["BackR"].y) / 2
# the height of the back over the shoulders and over the hips
withers_t = top_at(front_y_t, t_verts, 0.08 * length_t, t_hi.z) - t_lo.z
hips_t = top_at(back_y_t, t_verts, 0.08 * length_t, t_hi.z) - t_lo.z
withers = top_at(front_y, verts, 0.08 * L, hi.z) - lo.z
hips = top_at(back_y, verts, 0.08 * L, hi.z) - lo.z


def map_body(p):
    t = (p.y - front_y_t) / (back_y_t - front_y_t)
    c = max(0.0, min(1.0, t))
    kz = (withers + c * (hips - withers)) / (withers_t + c * (hips_t - withers_t))
    return Vector((p.x * spec.get("width_scale", 1.0), front_y + t * (back_y - front_y),
                   (p.z - t_lo.z) * kz + lo.z))


def map_leg(p, leg):
    top_t, foot_t = tops_t[leg], feet_t[leg]
    top, foot = map_body(top_t), feet[leg]
    s = (p.z - foot_t.z) / (top_t.z - foot_t.z)
    kl = (top.z - foot.z) / (top_t.z - foot_t.z)
    c = max(0.0, min(1.0, s))
    col_t, col = foot_t.lerp(top_t, c), foot.lerp(top, c)
    return Vector((col.x + (p.x - col_t.x) * kl, col.y + (p.y - col_t.y) * kl,
                   foot.z + (top.z - foot.z) * s))


head_t, neck_t = joint("Head"), joint("Neck1")
if "head" in spec:
    head = Vector(spec["head"])
else:
    front = [v for v in verts if v.y < lo.y + 0.18 * L and v.z > lo.z + 0.45 * H]
    head = sum(front, Vector()) / len(front)
neck = Vector(spec["neck"]) if "neck" in spec else map_body(neck_t)


def map_neck(p):
    # the similarity of the yz plane taking the template's neck and head
    # onto the target's
    a_t, b_t = Vector((neck_t.y, neck_t.z)), Vector((head_t.y, head_t.z))
    a, b = Vector((neck.y, neck.z)), Vector((head.y, head.z))
    s = (b - a).length / (b_t - a_t).length
    d, d_t = b - a, b_t - a_t
    ang = math.atan2(d.y, d.x) - math.atan2(d_t.y, d_t.x)
    q = Vector((p.y, p.z)) - a_t
    r = Vector((q.x * math.cos(ang) - q.y * math.sin(ang),
                q.x * math.sin(ang) + q.y * math.cos(ang))) * s + a
    return Vector((p.x * s, r.x, r.y))


tail_t = joint("Tail1")
if "tail" in spec:
    tail = Vector(spec["tail"])
else:
    back = [v for v in verts if v.y > hi.y - 0.06 * L]
    tail = sum(back, Vector()) / len(back)


def map_tail(p):
    return tail + (p - tail_t) * spec.get("tail_scale", 0.6)


# ---- fit the skeleton: joints moved, bones keep their directions ---------------
def leg_of(name):
    side = name.rsplit(".", 1)[-1]
    if side not in ("L", "R") or name.startswith("Ear"):
        return None
    front = "Front" in name or name.startswith(("FF.", "PoleTarget."))
    return ("Front" if front else "Back") + side


bpy.context.view_layer.objects.active = arm
bpy.ops.object.mode_set(mode="EDIT")
inv = mw.inverted()
for b in arm.data.edit_bones:
    h, t = mw @ b.head, mw @ b.tail
    leg = leg_of(b.name)
    if leg:
        nh = map_leg(h, leg)
    elif b.name.startswith(("Neck", "Head", "Ear")):
        nh = map_neck(h)
    elif b.name.startswith("Tail"):
        nh = map_tail(h)
    else:
        nh = map_body(h)
    b.head, b.tail = inv @ nh, inv @ (nh + (t - h))
bpy.ops.object.mode_set(mode="OBJECT")

# ---- parts: tapered tubes along bones (antennae) -------------------------------
for part in spec.get("parts", []):
    mat = bpy.data.materials.new(part["material"])
    mat.diffuse_color = (*part["color"], 1.0)
    mat.use_nodes = True
    mat.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*part["color"], 1.0)
    target.data.materials.append(mat)
    slot = len(target.data.materials) - 1
    for side in (1, -1) if part.get("mirror", True) else (1,):
        pts = [Vector((p[0] * side, p[1], p[2])) for p in part["points"]]
        bm = bmesh.new()
        bm.from_mesh(target.data)
        rings = []
        for i, p in enumerate(pts):
            d = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
            u = d.cross(Vector((0, 0, 1)) if abs(d.z) < 0.9 else Vector((1, 0, 0))).normalized()
            w = d.cross(u)
            r = part["radius"] * (1 - part.get("taper", 0.8) * i / (len(pts) - 1))
            rings.append([bm.verts.new(p + (u * math.cos(a) + w * math.sin(a)) * r)
                          for a in (2 * math.pi * j / 5 for j in range(5))])
        for a, b in zip(rings, rings[1:]):
            for j in range(5):
                f = bm.faces.new((a[j], a[(j + 1) % 5], b[(j + 1) % 5], b[j]))
                f.material_index = slot
        tip = bm.verts.new(pts[-1] + (pts[-1] - pts[-2]).normalized() * part["radius"] * part.get("tip", 1.0))
        for j in range(5):
            bm.faces.new((rings[-1][j], rings[-1][(j + 1) % 5], tip)).material_index = slot
        bm.to_mesh(target.data)
        bm.free()
        names = [n.replace(".L", ".L" if side > 0 else ".R") for n in part["bones"]]
        part.setdefault("_rings", []).append((len(target.data.vertices), names))
target.data.update()
verts = [target.matrix_world @ v.co for v in target.data.vertices]

# ---- weights ---------------------------------------------------------------------
for b in arm.data.bones:
    if b.name.startswith(("IK", "PoleTarget", "FF")):
        b.use_deform = False
deform = {b.name for b in arm.data.bones if b.use_deform}
# bones that reach their child (for the weights only)
PRIMARY = {"Torso3": "Neck1", "Back": "Torso", "Body": "Back"}
segs = {}
for b in arm.data.bones:
    if b.name not in deform:
        continue
    kids = [c for c in b.children if c.name in deform]
    kid = next((c for c in kids if c.name == PRIMARY.get(b.name)), None)
    kid = kid or (kids[0] if len(kids) == 1 else None)
    h = mw @ b.head_local
    if kid is not None:
        t = mw @ kid.head_local
    elif b.name.startswith(("FrontLowerLeg", "BackLowerLeg")):
        t = Vector((h.x, h.y, lo.z))
    elif b.name == "Head":
        t = h + (min(verts, key=lambda v: v.y) - h) * 0.8
    elif b.parent is not None:
        t = h + (h - mw @ b.parent.head_local) * 0.6
    else:
        t = h + Vector((0, 0, 0.1))
    segs[b.name] = (h, t)
heat = arm.copy()
heat.data = arm.data.copy()
heat.animation_data_clear()
scene.collection.objects.link(heat)
bpy.ops.object.select_all(action="DESELECT")
bpy.context.view_layer.objects.active = heat
bpy.ops.object.mode_set(mode="EDIT")
hinv = heat.matrix_world.inverted()
for n, (h, t) in segs.items():
    e = heat.data.edit_bones[n]
    e.use_connect = False
    e.head, e.tail = hinv @ h, hinv @ t
bpy.ops.object.mode_set(mode="OBJECT")
bpy.ops.object.select_all(action="DESELECT")
target.select_set(True)
heat.select_set(True)
bpy.context.view_layer.objects.active = heat
bpy.ops.object.parent_set(type="ARMATURE_AUTO")
groups = {g.name: g for g in target.vertex_groups}


def group(n):
    if n not in groups:
        groups[n] = target.vertex_groups.new(name=n)
    return groups[n]


# the parts follow their own bones only
for part in spec.get("parts", []):
    count = len(part["points"]) * 5 + 1
    for end, names in part["_rings"]:
        idx = list(range(end - count, end))
        for g in groups.values():
            g.remove(idx)
        for i in idx:
            p = target.matrix_world @ target.data.vertices[i].co
            n = min(names, key=lambda n: (p - segs[n][0]).length if n in segs else 1e9)
            group(n).add([i], 1.0, "REPLACE")


def seg_dist(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / ab.length_squared)) if ab.length_squared else 0.0
    return (p - (a + ab * t)).length


missed = [v.index for v in target.data.vertices if not any(g.weight > 1e-4 for g in v.groups)]
for i in missed:
    p = target.matrix_world @ target.data.vertices[i].co
    near = sorted((seg_dist(p, a, b), n) for n, (a, b) in segs.items())[:3]
    w = [1 / max(d, 1e-3) ** 4 for d, _ in near]
    for (_, n), wi in zip(near, w):
        group(n).add([i], wi / sum(w), "REPLACE")
print(f"{spec['name']}: {len(target.data.vertices)} vertices, {len(missed)} weighted by distance")
for m in list(target.modifiers):
    target.modifiers.remove(m)
target.parent = None
bpy.data.objects.remove(heat, do_unlink=True)
# in the armature's own space, as the template's meshes were (an exporter
# would scale a parent inverse into the skin)
target.data.transform(arm.matrix_world.inverted() @ target.matrix_world)
target.parent = arm
target.matrix_parent_inverse = Matrix.Identity(4)
target.matrix_basis = Matrix.Identity(4)
target.modifiers.new("Armature", "ARMATURE").object = arm
for b in arm.data.bones:
    group(b.name)

# ---- the clips, one NLA track each (the exporter's animations) ------------------
wanted = {old: new for new, old in spec["actions"].items()}
for a in list(bpy.data.actions):
    if a.name not in wanted:
        bpy.data.actions.remove(a)
missing = set(wanted) - {a.name for a in bpy.data.actions}
if missing:
    raise SystemExit(f"{spec['name']}: no clips {sorted(missing)}")
ad = arm.animation_data or arm.animation_data_create()
ad.action = None
for t in list(ad.nla_tracks):
    ad.nla_tracks.remove(t)
for a in list(bpy.data.actions):
    a.name = "_" + wanted[a.name]
for a in list(bpy.data.actions):
    a.name = a.name[1:]
    track = ad.nla_tracks.new()
    track.name = a.name
    track.strips.new(a.name, int(a.frame_range[0]), a)
    track.mute = True

os.makedirs(os.path.dirname(spec["out"]), exist_ok=True)
bpy.ops.export_scene.gltf(filepath=spec["out"], export_format="GLB",
                          export_animation_mode="NLA_TRACKS", export_def_bones=False,
                          export_optimize_animation_size=True)
if spec.get("save_blend"):
    bpy.ops.wm.save_as_mainfile(filepath=spec["save_blend"])
