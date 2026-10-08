"""A centaur: a Quaternius horse without its neck and head, a base
character's upper body in its place (tools/fetch_art.py runs it; see the
"blender" recipe in client/godot/art/sources.json):

    blender -b --factory-startup --python centaur.py -- spec.json

Spec: "horse" (a glTF of the Ultimate Animated Animal Pack), "body" (a
Universal Base Character), "hair" (its hair and beard, skinned to their own
Head bones), "pose" (a glTF with the Universal Animation Library's clips)
and "clip" (the one whose first frame poses the body), "cut" (a plane in
the horse's space, {"point", "normal"}, beyond which its neck and head go),
"seat" (where the body's waist sits, horse space), "height" (waist to the
crown), "waist" (the body's waist height, its own space), "bone" (the horse
bone the body rides on), "skin" (another albedo for the body), "cloth"
("#rrggbb": the linen painted on the body's skin dyed so, leather at the
waist), "actions" ({"new name": "horse clip"}), "name", "out".
"""
import bpy
import bmesh
import json
import numpy
import os
import sys
from mathutils import Matrix, Vector

spec = json.load(open(sys.argv[sys.argv.index("--") + 1]))
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene


def imported(path):
    before = set(scene.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in scene.objects if o not in before]
    stray = [o for o in new if o.type == "MESH" and o.name.startswith("Icosphere") and o.parent is None]
    for o in stray:
        bpy.data.objects.remove(o, do_unlink=True)
    return [o for o in new if o not in stray]


# ---- the horse, its neck and head cut away ------------------------------------
horse_objs = imported(spec["horse"])
arm = next(o for o in horse_objs if o.type == "ARMATURE")
horse = next(o for o in horse_objs if o.type == "MESH" and o.parent == arm)
for o in horse_objs:
    if o.type == "EMPTY" and o.name.endswith("_end"):
        bpy.data.objects.remove(o, do_unlink=True)
to_local = horse.matrix_world.inverted()
point = to_local @ Vector(spec["cut"]["point"])
normal = (to_local.to_3x3() @ Vector(spec["cut"]["normal"])).normalized()
bm = bmesh.new()
bm.from_mesh(horse.data)
geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
cut = bmesh.ops.bisect_plane(bm, geom=geom, plane_co=point, plane_no=normal, clear_outer=True)
rim = [e for e in bm.edges if e.is_boundary]
if rim:
    bmesh.ops.holes_fill(bm, edges=rim, sides=0)
bm.to_mesh(horse.data)
bm.free()


# ---- the body in its pose, cut at the waist -----------------------------------
def bake_pose(objs, action):
    """Apply the clip's first frame to the skinned meshes of `objs`."""
    rig = next(o for o in objs if o.type == "ARMATURE")
    ad = rig.animation_data or rig.animation_data_create()
    ad.action = action
    if len(getattr(action, "slots", [])):
        ad.action_slot = action.slots[0]
    scene.frame_set(int(action.frame_range[0]))
    meshes = [o for o in objs if o.type == "MESH" and o.parent == rig]
    for m in meshes:
        bpy.context.view_layer.objects.active = m
        for mod in [mod for mod in m.modifiers if mod.type == "ARMATURE"]:
            bpy.ops.object.modifier_apply(modifier=mod.name)
    return rig, meshes


pose_objs = imported(spec["pose"])
clip = bpy.data.actions[spec["clip"]]
for o in pose_objs:
    bpy.data.objects.remove(o, do_unlink=True)


def dyed(image, colour):
    """The pale linen painted on the skin (far bluer than any skin) dyed
    `colour`, its folds kept."""
    px = numpy.empty(len(image.pixels), dtype=numpy.float32)
    image.pixels.foreach_get(px)
    px = px.reshape(-1, 4)
    cloth = px[:, 2] > 0.75 * px[:, 0]
    light = px[cloth, :3].mean(axis=1, keepdims=True)
    px[cloth, :3] = numpy.array(colour, dtype=numpy.float32) * light / light.mean()
    image.pixels.foreach_set(px.ravel())
    image.pack()


body_objs = imported(spec["body"])
body_rig, body = bake_pose(body_objs, clip)
# another albedo for the skin (the base characters come dark)
skin = bpy.data.images.load(spec["skin"]) if "skin" in spec else None
for m in body:
    for slot in m.material_slots:
        nodes = slot.material.node_tree.nodes if slot.material and slot.material.use_nodes else []
        for n in nodes:
            if (n.type == "TEX_IMAGE" and n.image and n.image.name.startswith("T_Superhero")
                    and not any(w in n.image.name for w in ("Normal", "Roughness"))):
                n.image = skin or n.image
                skin = n.image
if "cloth" in spec:
    dyed(skin, [int(spec["cloth"][i:i + 2], 16) / 255 for i in (1, 3, 5)])
head = body_rig.pose.bones["Head"]
head_posed = body_rig.matrix_world @ head.matrix
parts = list(body)
for hair in spec.get("hair", []):
    objs = imported(hair)
    rig = next(o for o in objs if o.type == "ARMATURE")
    rest = rig.matrix_world @ rig.data.bones["Head"].matrix_local
    for m in [o for o in objs if o.type == "MESH"]:
        # skinned to its own Head: carried by the posed body's
        m.modifiers.clear()
        m.data.transform(m.matrix_world)
        m.data.transform(head_posed @ rest.inverted())
        m.parent = None
        m.matrix_world = Matrix.Identity(4)
        parts.append(m)
    bpy.data.objects.remove(rig, do_unlink=True)
for m in body:
    mw = m.matrix_world.copy()
    m.parent = None
    m.matrix_world = Matrix.Identity(4)
    m.data.transform(mw)
bpy.data.objects.remove(body_rig, do_unlink=True)
for o in list(scene.objects):
    if o.type == "EMPTY" and o not in (arm,) and o.parent is None and not o.children:
        bpy.data.objects.remove(o, do_unlink=True)
# the legs and hips go: what the thighs move, and the pelvis below the waist
LEGS = ("thigh_", "calf_", "foot_", "ball_")
waist = spec["waist"]
for m in body:
    names = {g.index: g.name for g in m.vertex_groups}
    bm = bmesh.new()
    bm.from_mesh(m.data)
    dl = bm.verts.layers.deform.active
    drop = []
    for v in bm.verts:
        w = v[dl] if dl else {}
        top = max(w.items(), key=lambda kv: kv[1])[0] if w else None
        name = names.get(top, "")
        if name.startswith(LEGS) or (v.co.z < waist and name in ("pelvis", "root", "")):
            drop.append(v)
    bmesh.ops.delete(bm, geom=drop, context="VERTS")
    bm.to_mesh(m.data)
    bm.free()
    m.vertex_groups.clear()
# scaled and set on the horse where its neck was
lo = Vector((1e9,) * 3)
hi = Vector((-1e9,) * 3)
for m in parts:
    for v in m.data.vertices:
        lo = Vector(map(min, lo, v.co))
        hi = Vector(map(max, hi, v.co))
k = spec["height"] / (hi.z - waist)
seat = Vector(spec["seat"])
place = Matrix.Translation(seat) @ Matrix.Scale(k, 4) @ Matrix.Translation(
    Vector((-(lo.x + hi.x) / 2, -spec.get("waist_y", 0.0), -waist)))
for m in parts:
    m.data.transform(place)

# ---- one mesh, the body riding on its bone ------------------------------------
bpy.ops.object.select_all(action="DESELECT")
for m in parts:
    m.select_set(True)
    for poly in m.data.polygons:
        poly.use_smooth = True
horse.select_set(True)
bpy.context.view_layer.objects.active = horse
n_horse = len(horse.data.vertices)
bpy.ops.object.join()
group = horse.vertex_groups.get(spec["bone"]) or horse.vertex_groups.new(name=spec["bone"])
rider = list(range(n_horse, len(horse.data.vertices)))
for g in horse.vertex_groups:
    if g != group:
        g.remove(rider)
group.add(rider, 1.0, "REPLACE")
horse.name = horse.data.name = spec["name"]

# ---- the clips ------------------------------------------------------------------
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
print(f"{spec['name']}: {n_horse} horse vertices, {len(rider)} rider vertices")
os.makedirs(os.path.dirname(spec["out"]), exist_ok=True)
bpy.ops.export_scene.gltf(filepath=spec["out"], export_format="GLB",
                          export_animation_mode="NLA_TRACKS", export_def_bones=False,
                          export_optimize_animation_size=True)
if spec.get("save_blend"):
    bpy.ops.wm.save_as_mainfile(filepath=spec["save_blend"])
