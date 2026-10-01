#!/usr/bin/env python3
"""Fetch the CC0 art of the client and prepare it for the Godot project.

    python3 tools/fetch_art.py            # fetch what is missing, check the lock
    python3 tools/fetch_art.py --update   # re-fetch everything, rewrite the lock

The recipe is client/godot/art/sources.json: Poly Haven textures and models
(through their public API) and the free files of itch.io packs (the site's own
"no thanks, just take me to the downloads" flow), files and zips at fixed
URLs (VFX flipbooks, particles, icons), and the OFL fonts of the UI
(the google/fonts repository at a fixed commit, into client/godot/fonts). Textures larger than
`texture_max` are scaled down and stored as JPEG; glTF files are rewritten to
point at the converted images. client/godot/art/art.lock.json records the
sha256 of every downloaded source, so a later run proves it started from the
same files (the prepared output is committed; image encoders may differ
byte for byte between Pillow versions).

Only the Python standard library and Pillow are needed.
"""
import argparse
import fnmatch
import hashlib
import http.cookiejar
import io
import json
import os
import re
import shutil
import sys
import tempfile
import urllib.parse
import urllib.request
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ART = os.path.join(ROOT, "client", "godot", "art")
FONTS = os.path.join(ROOT, "client", "godot", "fonts")
RECIPE = os.path.join(ART, "sources.json")
LOCK = os.path.join(ART, "art.lock.json")
UA = "renethack-fetch-art/1.0 (+https://github.com/g0ddest/renethack)"
CACHE = os.environ.get("RENETHACK_ART_CACHE",
                       os.path.join(tempfile.gettempdir(), "renethack-art-cache"))


def opener():
    jar = http.cookiejar.CookieJar()
    op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    op.addheaders = [("User-Agent", UA)]
    return op


def fetch(op, url, data=None, timeout=600):
    body = urllib.parse.urlencode(data).encode() if data is not None else None
    with op.open(url, body, timeout=timeout) as r:
        return r.read()


def sha256(data):
    return hashlib.sha256(data).hexdigest()


SOURCES = {}


def cached(key, produce):
    """Bytes for `key`, downloaded once per machine; its hash goes to the lock."""
    path = os.path.join(CACHE, re.sub(r"[^A-Za-z0-9._-]", "_", key))
    if os.path.exists(path):
        with open(path, "rb") as f:
            data = f.read()
        SOURCES[key] = sha256(data)
        return data
    data = produce()
    SOURCES[key] = sha256(data)
    os.makedirs(CACHE, exist_ok=True)
    with open(path + ".part", "wb") as f:
        f.write(data)
    os.replace(path + ".part", path)
    return data


# ---- sources ---------------------------------------------------------------

def polyhaven_files(op, asset):
    return json.loads(fetch(op, f"https://api.polyhaven.com/files/{asset}"))


def itch_upload(op, page, upload):
    """The free file `upload` of an itch.io page whose minimum price is 0."""
    token = lambda html: re.search(r'name="csrf_token" value="([^"]*)"', html).group(1)
    html = fetch(op, page).decode()
    if '"min_price":0' not in html:
        raise SystemExit(f"{page} is not free to download")
    dl = json.loads(fetch(op, page + "/download_url", {"csrf_token": token(html)}))["url"]
    dhtml = fetch(op, dl).decode()
    for uid, title in re.findall(r'data-upload_id="(\d+)".*?title="([^"]*)"', dhtml, re.S):
        if title == upload:
            r = json.loads(fetch(op, f"{page}/file/{uid}?source=game_download"
                                     "&after_download_lightbox=1&as_props=1",
                                 {"csrf_token": token(dhtml)}))
            if "url" not in r:
                raise SystemExit(f"{page}: {upload}: {r}")
            return fetch(op, r["url"], timeout=1800)
    raise SystemExit(f"{page}: no upload named {upload!r}")


# ---- processing -------------------------------------------------------------

class Writer:
    """Writes output files under ART and remembers their hashes."""

    def __init__(self):
        self.files = {}

    def put(self, rel, data):
        path = os.path.join(ART, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
        self.files[rel] = sha256(data)


def convert_image(data, max_size, keep_alpha):
    """Scale down to max_size and encode: JPEG unless the alpha matters."""
    from PIL import Image
    img = Image.open(io.BytesIO(data))
    img.load()
    if max(img.size) > max_size:
        scale = max_size / max(img.size)
        img = img.resize((max(1, round(img.width * scale)), max(1, round(img.height * scale))),
                         Image.LANCZOS)
    out = io.BytesIO()
    has_alpha = img.mode in ("RGBA", "LA") and img.getextrema()[-1][0] < 255
    if keep_alpha and has_alpha:
        img.save(out, "PNG", optimize=True)
        return out.getvalue(), ".png"
    img.convert("RGB").save(out, "JPEG", quality=88, optimize=True)
    return out.getvalue(), ".jpg"


def put_gltf(w, dest_dir, name, gltf_bytes, read_rel, max_size):
    """Write a .gltf with its buffers and converted images. `read_rel(uri)`
    returns the bytes of a file the glTF refers to."""
    doc = json.loads(gltf_bytes)
    for buf in doc.get("buffers", []):
        uri = buf.get("uri")
        if uri and not uri.startswith("data:"):
            w.put(f"{dest_dir}/{os.path.basename(urllib.parse.unquote(uri))}",
                  read_rel(urllib.parse.unquote(uri)))
            buf["uri"] = os.path.basename(urllib.parse.unquote(uri))
    for img in doc.get("images", []):
        uri = img.get("uri")
        if not uri or uri.startswith("data:"):
            continue
        src = urllib.parse.unquote(uri)
        base = os.path.splitext(os.path.basename(src))[0]
        data, ext = convert_image(read_rel(src), max_size, keep_alpha=True)
        w.put(f"{dest_dir}/textures/{base}{ext}", data)
        img["uri"] = f"textures/{base}{ext}"
        img.pop("mimeType", None)
    w.put(f"{dest_dir}/{name}", json.dumps(doc, indent=1).encode())


def shrink_glb(data, max_size):
    """A GLB with its embedded images scaled down and re-encoded; buffer
    views are laid out again, everything else is kept."""
    import struct
    magic, version, _ = struct.unpack_from("<III", data, 0)
    if magic != 0x46546C67 or version != 2:
        return data
    jlen, jtype = struct.unpack_from("<II", data, 12)
    doc = json.loads(data[20:20 + jlen])
    off = 20 + jlen
    blen, _btype = struct.unpack_from("<II", data, off)
    bin_in = data[off + 8:off + 8 + blen]
    views = doc.get("bufferViews", [])
    image_views = {img["bufferView"]: img for img in doc.get("images", []) if "bufferView" in img}
    out = bytearray()
    for i, v in enumerate(views):
        start = v.get("byteOffset", 0)
        chunk = bin_in[start:start + v["byteLength"]]
        if i in image_views:
            chunk, ext = convert_image(chunk, max_size, keep_alpha=True)
            image_views[i]["mimeType"] = "image/png" if ext == ".png" else "image/jpeg"
            # Godot extracts the image under this name; make needs paths without spaces
            if "name" in image_views[i]:
                image_views[i]["name"] = image_views[i]["name"].replace(" ", "_")
        while len(out) % 4:
            out.append(0)
        v["byteOffset"] = len(out)
        v["byteLength"] = len(chunk)
        out += chunk
    while len(out) % 4:
        out.append(0)
    doc["buffers"][0]["byteLength"] = len(out)
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    total = 12 + 8 + len(js) + 8 + len(out)
    return (struct.pack("<III", 0x46546C67, 2, total) + struct.pack("<II", len(js), 0x4E4F534A)
            + js + struct.pack("<II", len(out), 0x004E4942) + bytes(out))


def do_polyhaven_texture(op, w, item, max_size):
    files = polyhaven_files(op, item["id"])
    res = item.get("res", "1k")
    for m, suffix in (("Diffuse", "albedo"), ("nor_gl", "normal"), ("arm", "arm")):
        url = files[m][res]["jpg"]["url"]
        data = cached(url, lambda: fetch(op, url))
        out, ext = convert_image(data, max_size, keep_alpha=False)
        w.put(f"polyhaven/textures/{item['id']}/{item['id']}_{suffix}{ext}", out)


def do_polyhaven_model(op, w, item, max_size):
    files = polyhaven_files(op, item["id"])
    g = files["gltf"][item.get("res", "1k")]["gltf"]
    main = cached(g["url"], lambda: fetch(op, g["url"]))
    inc = {rel: v["url"] for rel, v in g.get("include", {}).items()}

    def read_rel(rel):
        url = inc[rel]
        return cached(url, lambda: fetch(op, url))
    put_gltf(w, f"polyhaven/models/{item['id']}", f"{item['id']}.gltf", main, read_rel, max_size)


def do_itch(op, w, item, max_size):
    blob = cached(item["upload"], lambda: itch_upload(op, item["page"], item["upload"]))
    unpack(w, blob, item["upload"], item, max_size)


def do_direct(op, w, item, max_size):
    """A file at a fixed URL: a zip unpacked like an itch.io pack, or a
    single file stored under `dest` by its own name (or `name`)."""
    blob = cached(item["url"], lambda: fetch(op, item["url"]))
    if "files" in item:
        unpack(w, blob, item["url"], item, max_size)
    else:
        name = item.get("name") or os.path.basename(urllib.parse.urlparse(item["url"]).path)
        w.put(f"{item['dest']}/{name}", blob)


def unpack(w, blob, what, item, max_size):
    max_size = item.get("texture_max", max_size)
    z = zipfile.ZipFile(io.BytesIO(blob))
    names = z.namelist()
    root = item.get("root", "")
    dest = item["dest"]
    picked = [n for n in names if n.startswith(root)
              and any(fnmatch.fnmatch(n[len(root):], pat) for pat in item["files"])]
    if not picked:
        raise SystemExit(f"{what}: nothing matches {item['files']}")
    for n in picked:
        rel = n[len(root):]
        if n.endswith(".gltf"):
            folder = os.path.dirname(n)

            def read_rel(r, folder=folder):
                for cand in (f"{folder}/{r}", f"{folder}/{os.path.basename(r)}"):
                    if cand in names:
                        return z.read(cand)
                # packs keep one texture folder next to the exports
                hits = [x for x in names if x.endswith("/" + os.path.basename(r))]
                if not hits:
                    raise SystemExit(f"{n}: missing {r}")
                return z.read(hits[0])
            put_gltf(w, f"{dest}/{os.path.dirname(rel)}".rstrip("/"),
                     os.path.basename(rel), z.read(n), read_rel, max_size)
        elif n.endswith(".glb"):
            w.put(f"{dest}/{rel}", shrink_glb(z.read(n), max_size))
        elif n.endswith(".tga"):
            # flipbooks: kept at full size (the frames are small already), lossless
            from PIL import Image
            out = io.BytesIO()
            Image.open(io.BytesIO(z.read(n))).save(out, "PNG", optimize=True)
            w.put(f"{dest}/{rel[:-4]}.png", out.getvalue())
        else:
            w.put(f"{dest}/{rel}", z.read(n))
    for lic in item.get("license_files", []):
        w.put(f"{dest}/{os.path.basename(lic)}", z.read(lic))


def do_fonts(op, item):
    """OFL fonts go to client/godot/fonts/<dest>/, outside the CC0 tree."""
    for name in item["files"]:
        url = item["base"] + name
        data = cached(url, lambda url=url: fetch(op, url))
        path = os.path.join(FONTS, item["dest"], name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--update", action="store_true",
                    help="write the lock from this run instead of checking it")
    args = ap.parse_args()
    recipe = json.load(open(RECIPE))
    max_size = recipe.get("texture_max", 1024)
    op = opener()
    w = Writer()
    out_root = os.path.join(ART, recipe["out"])
    if os.path.isdir(out_root):
        shutil.rmtree(out_root)
    w_prefix = recipe["out"]
    real_put = w.put
    w.put = lambda rel, data: real_put(f"{w_prefix}/{rel}", data)
    for item in recipe["polyhaven_textures"]:
        print("texture", item["id"], flush=True)
        do_polyhaven_texture(op, w, item, max_size)
    for item in recipe["polyhaven_models"]:
        print("model", item["id"], flush=True)
        do_polyhaven_model(op, w, item, max_size)
    for item in recipe["itch"]:
        print("pack", item["upload"], flush=True)
        do_itch(op, w, item, max_size)
    for item in recipe.get("direct", []):
        print("file", item["url"], flush=True)
        do_direct(op, w, item, max_size)
    for item in recipe.get("fonts", []):
        print("fonts", item["dest"], flush=True)
        do_fonts(op, item)
    total = sum(os.path.getsize(os.path.join(ART, p)) for p in w.files)
    print(f"{len(w.files)} files, {total / 1e6:.1f} MB under {out_root}")
    # dynamic itch.io links change per download: key them by the upload name
    lock = {"format": 1, "sources": dict(sorted(SOURCES.items()))}
    if args.update or not os.path.exists(LOCK):
        with open(LOCK, "w") as f:
            json.dump(lock, f, indent=1)
            f.write("\n")
        print("lock written:", LOCK)
        return 0
    old = json.load(open(LOCK))["sources"]
    diff = sorted(k for k in set(old) | set(SOURCES) if old.get(k) != SOURCES.get(k))
    if diff:
        print("sources differ from the lock:", *diff, sep="\n  ")
        return 1
    print("sources match the lock")
    return 0


if __name__ == "__main__":
    sys.exit(main())
