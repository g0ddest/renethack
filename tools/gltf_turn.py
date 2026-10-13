"""A rigged GLB whose clips hold some bones turned from where they were
animated (tools/fetch_art.py runs it: the "turned" recipe in
client/godot/art/sources.json): a dragon's wings beating raised over its
back. Only the clips' rotation keys of those bones change (and the bones'
own rest rotation, for a clip that does not key them): the file is
otherwise byte for byte the one it came from. A bone may also be set
further from its parent (`stretch`: its rest offset and its clips'
translation keys scaled), the skin between them drawn out: a neck made
long.

Only the Python standard library is needed.
"""
import json
import math
import struct


def _mul(a, b):
    """Quaternion product a·b, [x, y, z, w]."""
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [aw * bx + ax * bw + ay * bz - az * by,
            aw * by - ax * bz + ay * bw + az * bx,
            aw * bz + ax * by - ay * bx + az * bw,
            aw * bw - ax * bx - ay * by - az * bz]


def _axis_angle(axis, radians):
    s = math.sin(radians / 2)
    return [axis[0] * s, axis[1] * s, axis[2] * s, math.cos(radians / 2)]


def _rotate(q, v):
    """v turned by q."""
    p = _mul(_mul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]])
    return p[:3]


def _unit(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]


def turned(glb, raise_bones, front="Head", back="Body", stretch=None):
    """`glb` (bytes) with each bone of `raise_bones` ({"name": degrees})
    turned that far about the body's forward axis (from `back` to
    `front`, level), the way that lifts the bone's tip; each of `stretch`
    ({"name": factor}) that much further from its parent."""
    magic, version, _ = struct.unpack_from("<III", glb, 0)
    if magic != 0x46546C67 or version != 2:
        raise ValueError("not a GLB")
    jlen = struct.unpack_from("<I", glb, 12)[0]
    doc = json.loads(glb[20:20 + jlen])
    bin_at = 20 + jlen + 8
    data = bytearray(glb[bin_at:])
    nodes = doc["nodes"]
    parent = {c: i for i, n in enumerate(nodes) for c in n.get("children", [])}
    by_name = {n.get("name"): i for i, n in enumerate(nodes)}

    def world(i):
        """A node's rest position and rotation in the scene."""
        chain = []
        while i is not None:
            chain.append(i)
            i = parent.get(i)
        pos, rot, scale = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]
        for j in reversed(chain):
            n = nodes[j]
            t = n.get("translation", [0.0, 0.0, 0.0])
            local = [t[k] * scale[k] for k in range(3)]
            pos = [p + d for p, d in zip(pos, _rotate(rot, local))]
            rot = _mul(rot, n.get("rotation", [0.0, 0.0, 0.0, 1.0]))
            s = n.get("scale", [1.0, 1.0, 1.0])
            scale = [scale[k] * s[k] for k in range(3)]
        return pos, rot

    forward = [a - b for a, b in zip(world(by_name[front])[0], world(by_name[back])[0])]
    forward[1] = 0.0
    forward = _unit(forward)

    def accessor_floats(index):
        """The floats of an accessor (VEC4 of float), and where they lie."""
        acc = doc["accessors"][index]
        view = doc["bufferViews"][acc["bufferView"]]
        if acc["componentType"] != 5126 or acc["type"] != "VEC4" or view.get("byteStride", 16) != 16:
            raise ValueError(f"accessor {index}: not packed float quaternions")
        at = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
        return at, acc["count"]

    for name, degrees in sorted(raise_bones.items()):
        bone = by_name[name]
        up_from = world(bone)[0]
        kids = nodes[bone].get("children", [])
        tip = [a - b for a, b in zip(world(kids[0])[0], up_from)] if kids else [0.0, 0.0, 0.0]
        # the turn in the scene that lifts the tip, then in the parent's frame
        # (a bone's rotation turns it in its parent's space)
        turn = max((_axis_angle(forward, math.radians(degrees * s)) for s in (1, -1)),
                   key=lambda q: _rotate(q, tip)[1])
        p_rot = world(parent[bone])[1] if bone in parent else [0.0, 0.0, 0.0, 1.0]
        inv = [-p_rot[0], -p_rot[1], -p_rot[2], p_rot[3]]
        local = _mul(_mul(inv, turn), p_rot)
        nodes[bone]["rotation"] = _mul(local, nodes[bone].get("rotation", [0.0, 0.0, 0.0, 1.0]))
        for anim in doc.get("animations", []):
            for ch in anim["channels"]:
                if ch["target"].get("node") != bone or ch["target"]["path"] != "rotation":
                    continue
                at, count = accessor_floats(anim["samplers"][ch["sampler"]]["output"])
                for k in range(count):
                    q = list(struct.unpack_from("<4f", data, at + 16 * k))
                    struct.pack_into("<4f", data, at + 16 * k, *_mul(local, q))
    for name, factor in sorted((stretch or {}).items()):
        bone = by_name[name]
        t = nodes[bone].get("translation", [0.0, 0.0, 0.0])
        nodes[bone]["translation"] = [x * factor for x in t]
        for anim in doc.get("animations", []):
            for ch in anim["channels"]:
                if ch["target"].get("node") != bone or ch["target"]["path"] != "translation":
                    continue
                acc = doc["accessors"][anim["samplers"][ch["sampler"]]["output"]]
                view = doc["bufferViews"][acc["bufferView"]]
                if acc["componentType"] != 5126 or acc["type"] != "VEC3" or view.get("byteStride", 12) != 12:
                    raise ValueError(f"{name}: not packed float translations")
                at = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
                for k in range(acc["count"]):
                    v = struct.unpack_from("<3f", data, at + 12 * k)
                    struct.pack_into("<3f", data, at + 12 * k, *(x * factor for x in v))
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    total = 12 + 8 + len(js) + 8 + len(data)
    return (struct.pack("<III", 0x46546C67, 2, total) + struct.pack("<II", len(js), 0x4E4F534A)
            + js + struct.pack("<II", len(data), 0x004E4942) + bytes(data))
