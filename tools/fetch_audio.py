#!/usr/bin/env python3
"""Fetch the CC0 sounds of the client into the Godot project.

    python3 tools/fetch_audio.py            # fetch what is missing, check the lock
    python3 tools/fetch_audio.py --update   # re-fetch everything, rewrite the lock

The recipe is client/godot/audio/sources.json: zips at fixed URLs, the files
picked from each by pattern and written under client/godot/audio/<out>/<dest>
with the pack's licence beside them. Short sounds become mono 16-bit WAV
(Godot compresses them with QOA on import): starting an Ogg Vorbis voice
sets up a decoder, some 0.3 ms a play, where a WAV's takes microseconds; a
pack marked "keep_ogg" (music, jingles) stays Ogg. client/godot/audio/
audio.lock.json records the sha256 of every download, so a later run proves
it started from the same files. The output is committed with the .import
files Godot made for it: those of files still produced are kept, the rest
are removed. Which file plays for which cue is client/godot/audio/
manifest.json's business.

Needs the Python standard library and ffmpeg.
"""
import argparse
import fnmatch
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AUDIO = os.path.join(ROOT, "client", "godot", "audio")
RECIPE = os.path.join(AUDIO, "sources.json")
LOCK = os.path.join(AUDIO, "audio.lock.json")
UA = "renethack-fetch-audio/1.0 (+https://github.com/g0ddest/renethack)"
CACHE = os.environ.get("RENETHACK_AUDIO_CACHE",
                       os.path.join(tempfile.gettempdir(), "renethack-audio-cache"))

SOURCES = {}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def fetch(url, timeout=600):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def cached(key, produce):
    """Bytes for `key`, downloaded once per machine; its hash goes to the lock."""
    path = os.path.join(CACHE, re.sub(r"[^A-Za-z0-9._-]", "_", key))
    if os.path.exists(path):
        with open(path, "rb") as f:
            data = f.read()
    else:
        data = produce()
        os.makedirs(CACHE, exist_ok=True)
        with open(path + ".part", "wb") as f:
            f.write(data)
        os.replace(path + ".part", path)
    SOURCES[key] = sha256(data)
    return data


class Writer:
    """Writes files under the output folder, remembering which."""

    def __init__(self, out_root):
        self.out_root = out_root
        self.files = set()

    def put(self, rel, data):
        path = os.path.join(self.out_root, rel)
        if path in self.files:
            raise SystemExit(f"{rel}: written twice")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        old = None
        if os.path.exists(path):
            with open(path, "rb") as f:
                old = f.read()
        if old != data:
            with open(path, "wb") as f:
                f.write(data)
        self.files.add(path)

    def prune(self):
        """Remove what this run did not write (and Godot's .import of it)."""
        for d, _, names in os.walk(self.out_root, topdown=False):
            for n in names:
                path = os.path.join(d, n)
                if path in self.files:
                    continue
                if n.endswith(".import") and path[:-len(".import")] in self.files:
                    continue
                os.remove(path)
            if not os.listdir(d):
                os.rmdir(d)


def to_wav(data, name):
    """An Ogg Vorbis file as mono 16-bit PCM WAV, without metadata."""
    with tempfile.TemporaryDirectory() as tmp:
        src, out = os.path.join(tmp, "in.ogg"), os.path.join(tmp, "out.wav")
        with open(src, "wb") as f:
            f.write(data)
        r = subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-i", src, "-ac", "1",
                            "-c:a", "pcm_s16le", "-map_metadata", "-1", "-fflags",
                            "+bitexact", "-flags:a", "+bitexact", out],
                           capture_output=True, text=True)
        if r.returncode != 0:
            raise SystemExit(f"{name}: ffmpeg: {r.stderr.strip()}")
        with open(out, "rb") as f:
            return f.read()


def unpack(w, blob, item):
    z = zipfile.ZipFile(io.BytesIO(blob))
    root = item.get("root", "")
    picked = [n for n in z.namelist() if n.startswith(root) and not n.endswith("/")
              and any(fnmatch.fnmatch(n[len(root):], p) for p in item["files"])]
    for pattern in item["files"]:
        if not any(fnmatch.fnmatch(n[len(root):], pattern) for n in picked):
            raise SystemExit(f"{item['url']}: nothing matches {pattern!r}")
    for n in picked:
        rel = n[len(root):]
        data = z.read(n)
        if rel.endswith(".ogg") and not item.get("keep_ogg"):
            rel, data = rel[:-4] + ".wav", to_wav(data, n)
        w.put(f"{item['dest']}/{rel}", data)
    for lic in item.get("license_files", []):
        w.put(f"{item['dest']}/{os.path.basename(lic)}", z.read(lic))
    return len(picked)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--update", action="store_true",
                    help="write the lock from this run instead of checking it")
    args = ap.parse_args()
    recipe = json.load(open(RECIPE))
    if shutil.which("ffmpeg") is None:
        raise SystemExit("ffmpeg is needed (the short sounds become WAV)")
    w = Writer(os.path.join(AUDIO, recipe["out"]))
    for item in recipe["zips"]:
        print("pack", item["url"].rsplit("/", 1)[-1], "->", item["dest"], flush=True)
        blob = cached(item["url"], lambda u=item["url"]: fetch(u))
        n = unpack(w, blob, item)
        print(f"  {n} files", flush=True)
    w.prune()
    total = sum(os.path.getsize(p) for p in w.files)
    print(f"{len(w.files)} files, {total / 1e6:.1f} MB under {w.out_root}")
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
