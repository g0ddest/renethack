"""A rigged GLB's low-poly meshes made round (tools/fetch_art.py runs it:
the "smoothed" recipe in client/godot/art/sources.json).

Each triangle of the meshes named is cut in four, `steps` times; a new
corner on an edge stands out from the edge's middle the way the surface
curves there (Phong tessellation: the middle pulled onto the tangent
planes of the edge's two ends, the normals there those of the faces
round each end's place averaged), so the old corners stay where they
were and nothing shrinks; each new corner takes its edge's texture
coordinates and the bones of both ends, weighed half and half (the four
strongest kept). The normals are then those of the faces round each
place. The skeleton, the clips and every other mesh are kept byte for
byte. A body folded into flat facets reads as origami beside the smooth
ones; Catmull-Clark in Blender rounds it as well, but its glTF round trip
moved this rig's pose.

    python3 tools/gltf_smooth.py in.glb out.glb MESH [steps]
"""
import json
import math
import struct
import sys

# how far a new corner stands out towards the curved surface (Phong's
# shape factor: 0 flat, 1 the full projection)
SHAPE = 0.75
FLOAT, USHORT, UINT = 5126, 5123, 5125
COUNT = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}
FORMAT = {FLOAT: "f", USHORT: "H", UINT: "I", 5121: "B"}
SIZE = {FLOAT: 4, USHORT: 2, UINT: 4, 5121: 1}


def chunks(data):
    """The JSON and the binary chunk of a GLB."""
    n = struct.unpack_from("<I", data, 12)[0]
    doc = json.loads(data[20:20 + n])
    off = 20 + n
    blen = struct.unpack_from("<I", data, off)[0]
    return doc, bytearray(data[off + 8:off + 8 + blen])


def read(doc, binary, index):
    """An accessor's values, as tuples."""
    a = doc["accessors"][index]
    view = doc["bufferViews"][a["bufferView"]]
    comp, kind = a["componentType"], COUNT[a["type"]]
    stride = view.get("byteStride", SIZE[comp] * kind)
    start = view.get("byteOffset", 0) + a.get("byteOffset", 0)
    fmt = "<" + FORMAT[comp] * kind
    return [struct.unpack_from(fmt, binary, start + i * stride) for i in range(a["count"])]


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(a):
    n = math.sqrt(dot(a, a))
    return (a[0] / n, a[1] / n, a[2] / n) if n > 1e-12 else (0.0, 1.0, 0.0)


def place(p):
    return tuple(round(c * 1e5) for c in p)


def smooth_normals(pos, tris):
    """Per corner: the normal of the faces round its place, by area."""
    sums = {}
    for a, b, c in tris:
        n = cross(sub(pos[b], pos[a]), sub(pos[c], pos[a]))
        for i in (a, b, c):
            k = place(pos[i])
            s = sums.get(k, (0.0, 0.0, 0.0))
            sums[k] = (s[0] + n[0], s[1] + n[1], s[2] + n[2])
    return [unit(sums[place(p)]) for p in pos]


def bones(ja, wa, jb, wb):
    """Two corners' bones, half each: the four strongest, summing to one."""
    w = {}
    for js, ws in ((ja, wa), (jb, wb)):
        for j, x in zip(js, ws):
            if x > 0:
                w[j] = w.get(j, 0.0) + x * 0.5
    top = sorted(w.items(), key=lambda kv: -kv[1])[:4]
    total = sum(x for _, x in top) or 1.0
    top += [(0, 0.0)] * (4 - len(top))
    return tuple(j for j, _ in top), tuple(x / total for _, x in top)


def tessellate(attrs, tris):
    """One step: every triangle in four, the new corners curved out."""
    pos = attrs["POSITION"]
    nrm = smooth_normals(pos, tris)
    out = {k: list(v) for k, v in attrs.items()}
    mids = {}

    def mid(a, b):
        key = (min(a, b), max(a, b))
        if key in mids:
            return mids[key]
        pa, pb, na, nb = pos[a], pos[b], nrm[a], nrm[b]
        m = tuple((x + y) / 2 for x, y in zip(pa, pb))
        da, db = dot(sub(m, pa), na), dot(sub(m, pb), nb)
        q = tuple(mc - 0.5 * da * x - 0.5 * db * y for mc, x, y in zip(m, na, nb))
        out["POSITION"].append(tuple((1 - SHAPE) * x + SHAPE * y for x, y in zip(m, q)))
        for k, v in attrs.items():
            if k in ("POSITION", "JOINTS_0", "WEIGHTS_0"):
                continue
            out[k].append(tuple((x + y) / 2 for x, y in zip(v[a], v[b])))
        if "JOINTS_0" in attrs:
            j, w = bones(attrs["JOINTS_0"][a], attrs["WEIGHTS_0"][a], attrs["JOINTS_0"][b], attrs["WEIGHTS_0"][b])
            out["JOINTS_0"].append(j)
            out["WEIGHTS_0"].append(w)
        mids[key] = len(out["POSITION"]) - 1
        return mids[key]

    new = []
    for a, b, c in tris:
        ab, bc, ca = mid(a, b), mid(b, c), mid(c, a)
        new += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
    return out, new


def compacted(doc, binary):
    """The binary without what no accessor or image reads any more (the
    meshes' old data), the accessors and views renumbered."""
    used = set()
    for mesh in doc["meshes"]:
        for prim in mesh["primitives"]:
            used.update(prim["attributes"].values())
            if "indices" in prim:
                used.add(prim["indices"])
            for target in prim.get("targets", []):
                used.update(target.values())
    for skin in doc.get("skins", []):
        if "inverseBindMatrices" in skin:
            used.add(skin["inverseBindMatrices"])
    for anim in doc.get("animations", []):
        for sampler in anim["samplers"]:
            used.update((sampler["input"], sampler["output"]))
    acc_map = {old: new for new, old in enumerate(sorted(used))}
    accessors = [doc["accessors"][i] for i in sorted(used)]
    views = {a["bufferView"] for a in accessors if "bufferView" in a}
    views |= {img["bufferView"] for img in doc.get("images", []) if "bufferView" in img}
    view_map = {}
    out = bytearray()
    new_views = []
    for old in sorted(views):
        v = dict(doc["bufferViews"][old])
        while len(out) % 4:
            out.append(0)
        start = v.get("byteOffset", 0)
        chunk = binary[start:start + v["byteLength"]]
        v["byteOffset"] = len(out)
        out += chunk
        view_map[old] = len(new_views)
        new_views.append(v)
    while len(out) % 4:
        out.append(0)
    for a in accessors:
        if "bufferView" in a:
            a["bufferView"] = view_map[a["bufferView"]]
    for img in doc.get("images", []):
        if "bufferView" in img:
            img["bufferView"] = view_map[img["bufferView"]]
    for mesh in doc["meshes"]:
        for prim in mesh["primitives"]:
            prim["attributes"] = {k: acc_map[v] for k, v in prim["attributes"].items()}
            if "indices" in prim:
                prim["indices"] = acc_map[prim["indices"]]
            if "targets" in prim:
                prim["targets"] = [{k: acc_map[v] for k, v in t.items()} for t in prim["targets"]]
    for skin in doc.get("skins", []):
        if "inverseBindMatrices" in skin:
            skin["inverseBindMatrices"] = acc_map[skin["inverseBindMatrices"]]
    for anim in doc.get("animations", []):
        for sampler in anim["samplers"]:
            sampler["input"], sampler["output"] = acc_map[sampler["input"]], acc_map[sampler["output"]]
    doc["accessors"] = accessors
    doc["bufferViews"] = new_views
    doc["buffers"][0]["byteLength"] = len(out)
    return out


def smoothed(data, mesh_name, steps=2):
    doc, binary = chunks(data)
    mesh = next(m for m in doc["meshes"] if m.get("name") == mesh_name)
    tail = bytearray()
    base = len(binary)

    def put(values, comp, kind, target=None):
        """A new accessor of `values`, its bytes after the old ones."""
        nonlocal tail
        while (base + len(tail)) % 4:
            tail.append(0)
        fmt = "<" + FORMAT[comp] * COUNT[kind]
        offset = base + len(tail)
        for v in values:
            tail += struct.pack(fmt, *v)
        view = {"buffer": 0, "byteOffset": offset, "byteLength": base + len(tail) - offset}
        if target:
            view["target"] = target
        doc["bufferViews"].append(view)
        acc = {"bufferView": len(doc["bufferViews"]) - 1, "componentType": comp, "count": len(values), "type": kind}
        if kind == "VEC3" and comp == FLOAT:
            acc["min"] = [min(v[i] for v in values) for i in range(3)]
            acc["max"] = [max(v[i] for v in values) for i in range(3)]
        doc["accessors"].append(acc)
        return len(doc["accessors"]) - 1

    for prim in mesh["primitives"]:
        attrs = {k: read(doc, binary, i) for k, i in prim["attributes"].items()}
        flat = [i[0] for i in read(doc, binary, prim["indices"])]
        tris = [tuple(flat[i:i + 3]) for i in range(0, len(flat) - 2, 3)]
        for _ in range(steps):
            attrs, tris = tessellate(attrs, tris)
        attrs["NORMAL"] = smooth_normals(attrs["POSITION"], tris)
        kinds = {k: doc["accessors"][i]["type"] for k, i in prim["attributes"].items()}
        comps = {k: doc["accessors"][i]["componentType"] for k, i in prim["attributes"].items()}
        for k in prim["attributes"]:
            prim["attributes"][k] = put(attrs[k], comps[k], kinds[k], 34962)
        big = max(max(t) for t in tris) > 65535
        prim["indices"] = put([(i,) for t in tris for i in t], UINT if big else USHORT, "SCALAR", 34963)
    binary += tail
    binary = compacted(doc, binary)
    text = json.dumps(doc, separators=(",", ":")).encode()
    while len(text) % 4:
        text += b" "
    total = 12 + 8 + len(text) + 8 + len(binary)
    return (struct.pack("<III", 0x46546C67, 2, total) + struct.pack("<II", len(text), 0x4E4F534A) + text
            + struct.pack("<II", len(binary), 0x004E4942) + bytes(binary))


if __name__ == "__main__":
    src, dst, name = sys.argv[1:4]
    steps = int(sys.argv[4]) if len(sys.argv) > 4 else 2
    with open(src, "rb") as f:
        out = smoothed(f.read(), name, steps)
    with open(dst, "wb") as f:
        f.write(out)
