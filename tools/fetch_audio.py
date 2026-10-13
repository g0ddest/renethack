#!/usr/bin/env python3
"""Fetch the sounds of the client and prepare them for the Godot project.

    python3 tools/fetch_audio.py            # fetch what is missing, check the lock
    python3 tools/fetch_audio.py --update   # re-fetch everything, rewrite the lock
    python3 tools/fetch_audio.py --report r.json   # also write each file's numbers

The recipe is client/godot/audio/sources.json. Its sources are files and
archives (zip, 7z, rar) at fixed URLs, or the free files of itch.io packs
(the site's own "no thanks, just take me to the downloads" flow); the files
picked from each by pattern are prepared by their kind and written under
client/godot/audio/<dest>:

- sfx: one-shots as mono 44.1 kHz 16-bit WAV (Godot compresses them with
  QOA on import; starting an Ogg Vorbis voice sets up a decoder, some 0.3 ms
  a play, where a WAV's takes microseconds), silence trimmed off both ends;
- loop, music, jingle: stereo Ogg Vorbis; a loop's end can be crossfaded
  into its start so that it goes round without a seam;
- emitter: a loop as mono Ogg Vorbis, for a sound placed in the world (a
  lava pool's, a fountain's), which the engine pans itself;
- mono: an Ogg as mono WAV, otherwise untouched (the placeholders);
- keep: the file as it is.

The files of an item are then made equally loud (integrated loudness, EBU
R128): as loud as the recipe's target for the kind, unless the true peak
would pass the kind's ceiling first; then as loud as most of them can be.

Mixes are ambience beds made from several sources: beds looped to length,
events (drips, distant rumbles) placed at times drawn from a fixed seed,
the whole crossfaded at its seam and levelled like a loop.

client/godot/audio/audio.lock.json records the sha256 of every download, so
a later run proves it started from the same files. The output is committed
with the .import files Godot made for it: those of files still produced are
kept, the rest are removed. CREDITS.md is written from the recipe's credits.
Which file plays for which cue is client/godot/audio/manifest.json's
business.

Needs the Python standard library, ffmpeg, bsdtar for 7z and rar archives
(macOS's tar is bsdtar), and libsndfile with Vorbis for the Ogg files.
"""
import argparse
import array
import ctypes
import ctypes.util
import fnmatch
import hashlib
import html
import http.cookiejar
import io
import json
import math
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
import wave
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AUDIO = os.path.join(ROOT, "client", "godot", "audio")
RECIPE = os.path.join(AUDIO, "sources.json")
LOCK = os.path.join(AUDIO, "audio.lock.json")
CREDITS = os.path.join(AUDIO, "CREDITS.md")
UA = "renethack-fetch-audio/1.0 (+https://github.com/g0ddest/renethack)"
CACHE = os.environ.get("RENETHACK_AUDIO_CACHE",
                       os.path.join(tempfile.gettempdir(), "renethack-audio-cache"))
RATE = 44100

SOURCES = {}
REPORT = {}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


# ---- downloads --------------------------------------------------------------

def opener():
    jar = http.cookiejar.CookieJar()
    op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    op.addheaders = [("User-Agent", UA)]
    return op


def fetch(op, url, data=None, timeout=1800):
    body = urllib.parse.urlencode(data).encode() if data is not None else None
    with op.open(urllib.request.Request(url, body), timeout=timeout) as r:
        return r.read()


def itch_upload(op, page, upload):
    """The free file `upload` of an itch.io page whose minimum price is 0."""
    token = lambda text: re.search(r'name="csrf_token" value="([^"]*)"', text).group(1)
    text = fetch(op, page).decode()
    if '"min_price":0' not in text:
        raise SystemExit(f"{page} is not free to download")
    dl = json.loads(fetch(op, page + "/download_url", {"csrf_token": token(text)}))["url"]
    dhtml = fetch(op, dl).decode()
    for uid, title in re.findall(r'data-upload_id="(\d+)".*?title="([^"]*)"', dhtml, re.S):
        if html.unescape(title) == upload:
            r = json.loads(fetch(op, f"{page}/file/{uid}?source=game_download"
                                     "&after_download_lightbox=1&as_props=1",
                                 {"csrf_token": token(dhtml)}))
            if "url" not in r:
                raise SystemExit(f"{page}: {upload}: {r}")
            return fetch(op, r["url"])
    raise SystemExit(f"{page}: no upload named {upload!r}")


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


def download(op, item):
    """A source's bytes and the key the lock knows them by (an itch.io link
    changes at every download, so those are keyed by page and upload)."""
    if "itch" in item:
        page, upload = item["itch"]["page"], item["itch"]["upload"]
        key = f"{page}#{upload}"
        return cached(key, lambda: itch_upload(op, page, upload)), key
    return cached(item["url"], lambda: fetch(op, item["url"])), item["url"]


ARCHIVES = {}


def members(blob, key):
    """The files of an archive (zip, 7z, rar, tar.gz) by path, or the one
    file a plain download is, under its URL's name."""
    if key in ARCHIVES:
        return ARCHIVES[key]
    if blob[:4] == b"PK\x03\x04":
        z = zipfile.ZipFile(io.BytesIO(blob))
        files = {n: z.read(n) for n in z.namelist() if not n.endswith("/")}
    elif blob[:6] == b"7z\xbc\xaf\x27\x1c" or blob[:4] == b"Rar!" or blob[:2] == b"\x1f\x8b":
        files = {}
        with tempfile.TemporaryDirectory() as tmp:
            arc, out = os.path.join(tmp, "archive"), os.path.join(tmp, "out")
            with open(arc, "wb") as f:
                f.write(blob)
            os.makedirs(out)
            r = subprocess.run([bsdtar(), "-xf", arc, "-C", out], capture_output=True, text=True)
            if r.returncode != 0:
                raise SystemExit(f"{key}: {r.stderr.strip()}")
            for d, _, names in os.walk(out):
                for n in names:
                    p = os.path.join(d, n)
                    with open(p, "rb") as f:
                        files[os.path.relpath(p, out)] = f.read()
    else:
        path = urllib.parse.urlparse(key).path
        files = {urllib.parse.unquote(os.path.basename(path)) or "file": blob}
    # resource forks and Finder litter
    files = {n: d for n, d in files.items()
             if "__MACOSX/" not in n and not os.path.basename(n).startswith("._")
             and os.path.basename(n) != ".DS_Store"}
    ARCHIVES[key] = files
    return files


def bsdtar():
    for cand in ("bsdtar", "tar"):
        path = shutil.which(cand)
        if path and "bsdtar" in subprocess.run([path, "--version"], capture_output=True,
                                               text=True).stdout:
            return path
    raise SystemExit("bsdtar is needed for 7z and rar archives (libarchive-tools)")


def pick(files, item, what):
    """The archive members an item asks for, in its patterns' order."""
    root = item.get("root", "")
    picked = []
    for pattern in item["files"]:
        hits = sorted(n for n in files if n.startswith(root)
                      and fnmatch.fnmatch(n[len(root):], pattern) and n not in picked)
        if not hits:
            raise SystemExit(f"{what}: nothing matches {pattern!r}")
        picked += hits
    return [n for n in picked
            if not any(fnmatch.fnmatch(n[len(root):], x) for x in item.get("exclude", []))]


# ---- sound ------------------------------------------------------------------

def ffmpeg(args, data=None, name="?", level="error"):
    r = subprocess.run(["ffmpeg", "-hide_banner", "-nostats", "-v", level, "-nostdin", *args],
                       input=data, capture_output=True)
    if r.returncode != 0:
        raise SystemExit(f"{name}: ffmpeg: {r.stderr.decode(errors='replace').strip()[-400:]}")
    return r


def decode(data, channels, name):
    """Any sound file as 32-bit float samples at RATE, interleaved."""
    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "in")
        with open(src, "wb") as f:
            f.write(data)
        out = ffmpeg(["-i", src, "-ac", str(channels), "-ar", str(RATE), "-f", "f32le", "-"],
                     name=name).stdout
    a = array.array("f")
    a.frombytes(out)
    if sys.byteorder != "little":
        a.byteswap()
    return a


def measure(samples, channels, name):
    """Integrated loudness (LUFS) and true peak (dBTP). A sound shorter
    than one 400 ms block is measured as if silence followed it."""
    raw = samples.tobytes() if sys.byteorder == "little" else _swapped(samples)
    r = ffmpeg(["-f", "f32le", "-ar", str(RATE), "-ac", str(channels), "-i", "-", "-af",
                "apad=whole_dur=0.4,ebur128=peak=true:framelog=quiet", "-f", "null", "-"],
               raw, name, level="info")
    text = r.stderr.decode(errors="replace")
    summary = text[text.rfind("Integrated loudness"):]
    i = re.search(r"I:\s+(-?[\d.]+|-inf) LUFS", summary)
    tp = re.search(r"True peak:\s+Peak:\s+(-?[\d.]+|-inf) dBFS", summary)
    if not i or not tp:
        raise SystemExit(f"{name}: no loudness in ffmpeg's output")
    return float(i.group(1)), float(tp.group(1))


def _swapped(samples):
    a = array.array("f", samples)
    a.byteswap()
    return a.tobytes()


def fade(samples, channels, inn, out):
    frames = len(samples) // channels
    fi, fo = min(frames, int(RATE * inn)), min(frames, int(RATE * out))
    for f in range(fi):
        for c in range(channels):
            samples[f * channels + c] *= f / fi
    for k in range(fo):
        f = frames - 1 - k
        for c in range(channels):
            samples[f * channels + c] *= k / fo


def trim(samples, channels, floor_db=-50.0, head_ms=4, tail_ms=40):
    """Silence off both ends: what is `floor_db` under the peak and quieter,
    keeping a few milliseconds before the sound and a short fade after it."""
    p = max((abs(x) for x in samples), default=0.0)
    if p == 0:
        return samples
    gate = p * 10 ** (floor_db / 20)
    loud = [i // channels for i, x in enumerate(samples) if abs(x) > gate]
    frames = len(samples) // channels
    a = max(0, loud[0] - RATE * head_ms // 1000)
    b = min(frames, loud[-1] + 1 + RATE * tail_ms // 1000)
    out = samples[a * channels:b * channels]
    fade(out, channels, 0.002, 0.03)
    return out


def rms_windows(samples, channels, ms):
    """RMS per window of `ms` milliseconds (all channels together)."""
    n = max(1, RATE * ms // 1000) * channels
    out = []
    for s in range(0, len(samples), n):
        w = samples[s:s + n]
        out.append(math.sqrt(sum(x * x for x in w) / len(w)))
    return out


def split(samples, channels, gap, floor_db, min_len):
    """A file of several takes, cut at each silence longer than `gap`
    seconds (`floor_db` under the loudest 10 ms)."""
    win = 10
    rms = rms_windows(samples, channels, win)
    gate = max(rms, default=0.0) * 10 ** (floor_db / 20)
    takes, start, quiet = [], None, 0
    for i, v in enumerate(rms + [0.0] * (int(gap * 1000 / win) + 1)):
        if v > gate:
            start = i if start is None else start
            quiet = 0
        elif start is not None:
            quiet += 1
            if quiet * win >= gap * 1000:
                end = i - quiet + 1
                if (end - start) * win >= min_len * 1000:
                    takes.append((start, end))
                start, quiet = None, 0
    per = RATE * win // 1000 * channels
    return [samples[max(0, a - 2) * per:(b + 2) * per] for a, b in takes]


def cut(samples, channels, start, end=None):
    a = int(start * RATE) * channels
    b = len(samples) if end is None else int(end * RATE) * channels
    return samples[a:b]


def loop_seam(samples, channels, seconds):
    """A loop that goes round smoothly: its last `seconds` faded into its
    first (equal power), the result that much shorter."""
    frames = len(samples) // channels
    n = min(int(seconds * RATE), frames // 3)
    out = samples[:(frames - n) * channels]
    tail = (frames - n) * channels
    for f in range(n):
        t = f / n
        a, b = math.sin(t * math.pi / 2), math.cos(t * math.pi / 2)
        for c in range(channels):
            i = f * channels + c
            out[i] = samples[i] * a + samples[tail + i] * b
    return out


def level(samples, channels, target, ceiling, name, gain_db=0.0):
    """Made as loud as `target` LUFS (plus `gain_db`), unless the true peak
    would pass `ceiling` dBTP first."""
    lufs, tp = measure(samples, channels, name)
    if lufs <= -69.9:
        raise SystemExit(f"{name}: silent")
    gain = min(target + gain_db - lufs, ceiling - tp)
    g = 10 ** (gain / 20)
    out = array.array("f", (x * g for x in samples))
    return out, {"lufs_in": round(lufs, 1), "tp_in": round(tp, 1), "gain": round(gain, 1),
                 "lufs": round(lufs + gain, 1), "tp": round(tp + gain, 1),
                 "peak_limited": gain < target + gain_db - lufs - 0.05}


def wav_bytes(samples):
    """Mono float samples as 16-bit PCM WAV, without metadata."""
    pcm = array.array("h", (max(-32768, min(32767, round(x * 32767))) for x in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    out = io.BytesIO()
    with wave.open(out, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    return out.getvalue()


class SfInfo(ctypes.Structure):
    _fields_ = [("frames", ctypes.c_int64), ("samplerate", ctypes.c_int),
                ("channels", ctypes.c_int), ("format", ctypes.c_int),
                ("sections", ctypes.c_int), ("seekable", ctypes.c_int)]


SNDFILE = []


def sndfile():
    if not SNDFILE:
        for cand in (ctypes.util.find_library("sndfile"), "/opt/homebrew/lib/libsndfile.dylib",
                     "/usr/local/lib/libsndfile.dylib", "libsndfile.so.1"):
            try:
                lib = ctypes.CDLL(cand) if cand else None
            except OSError:
                lib = None
            if lib:
                break
        else:
            raise SystemExit("libsndfile (with Vorbis) is needed for the Ogg files")
        lib.sf_open.restype = ctypes.c_void_p
        lib.sf_open.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.POINTER(SfInfo)]
        lib.sf_command.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
        lib.sf_writef_float.restype = ctypes.c_int64
        lib.sf_writef_float.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_float),
                                        ctypes.c_int64]
        lib.sf_close.argtypes = [ctypes.c_void_p]
        lib.sf_strerror.restype = ctypes.c_char_p
        lib.sf_strerror.argtypes = [ctypes.c_void_p]
        SNDFILE.append(lib)
    return SNDFILE[0]


def ogg_bytes(samples, channels, quality, name):
    """Float samples as Ogg Vorbis (libsndfile; `quality` 0..1 as oggenc's
    -q 0..10), the stream's serial number drawn from the name so that a run
    gives the same bytes as the last."""
    lib = sndfile()
    with tempfile.TemporaryDirectory() as tmp:
        path = os.path.join(tmp, "out.ogg")
        info = SfInfo(0, RATE, channels, 0x200000 | 0x0060, 0, 0)  # OGG | VORBIS
        h = lib.sf_open(path.encode(), 0x20, ctypes.byref(info))  # SFM_WRITE
        if not h:
            raise SystemExit(f"{name}: libsndfile: {lib.sf_strerror(None).decode()}")
        q = ctypes.c_double(quality)
        lib.sf_command(h, 0x1300, ctypes.byref(q), ctypes.sizeof(q))  # VBR quality
        buf = (ctypes.c_float * len(samples)).from_buffer(samples)
        # in blocks: libvorbis is handed one buffer per call, and a minute
        # or more of samples in one call crashes it
        frames, step = len(samples) // channels, 1 << 16
        for at in range(0, frames, step):
            n = min(step, frames - at)
            ptr = ctypes.cast(ctypes.addressof(buf) + 4 * at * channels,
                              ctypes.POINTER(ctypes.c_float))
            lib.sf_writef_float(h, ptr, n)
        lib.sf_close(h)
        with open(path, "rb") as f:
            data = bytearray(f.read())
    return bytes(ogg_reserial(data, int(sha256(name.encode())[:8], 16)))


OGG_CRC = []


def ogg_crc(data):
    if not OGG_CRC:
        for i in range(256):
            r = i << 24
            for _ in range(8):
                r = ((r << 1) ^ 0x04C11DB7) if r & 0x80000000 else (r << 1)
            OGG_CRC.append(r & 0xFFFFFFFF)
    crc = 0
    for b in data:
        crc = ((crc << 8) & 0xFFFFFFFF) ^ OGG_CRC[((crc >> 24) ^ b) & 0xFF]
    return crc


def ogg_reserial(data, serial):
    """Every page of a one-stream Ogg file given `serial`, its CRC redone."""
    pos = 0
    while pos < len(data):
        if data[pos:pos + 4] != b"OggS":
            raise SystemExit("not an Ogg page")
        nseg = data[pos + 26]
        size = 27 + nseg + sum(data[pos + 27:pos + 27 + nseg])
        data[pos + 14:pos + 18] = serial.to_bytes(4, "little")
        data[pos + 22:pos + 26] = b"\0\0\0\0"
        data[pos + 22:pos + 26] = ogg_crc(data[pos:pos + size]).to_bytes(4, "little")
        pos += size
    return data


# ---- output -----------------------------------------------------------------

class Writer:
    """Writes files under the output folders, remembering which."""

    def __init__(self, roots):
        self.roots = [os.path.join(AUDIO, r) for r in roots]
        self.files = set()

    def put(self, rel, data):
        path = os.path.join(AUDIO, rel)
        if not any(path.startswith(r + os.sep) for r in self.roots):
            raise SystemExit(f"{rel}: not under {', '.join(self.roots)}")
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
        for root in self.roots:
            for d, _, names in os.walk(root, topdown=False):
                for n in names:
                    path = os.path.join(d, n)
                    if path in self.files:
                        continue
                    if n.endswith(".import") and path[:-len(".import")] in self.files:
                        continue
                    os.remove(path)
                if not os.listdir(d):
                    os.rmdir(d)


def out_name(rel, ext):
    """A member's path as a name the manifest can write: no spaces or
    brackets, the kind's extension."""
    stem = re.sub(r"[^A-Za-z0-9/_-]+", "_", os.path.splitext(rel)[0])
    stem = re.sub(r"_+", "_", stem)
    stem = "/".join(p.strip("_") or "_" for p in stem.split("/"))
    return f"{stem}{ext}"


def to_mono_wav(data, name):
    """The placeholders' way: an Ogg Vorbis file as mono 16-bit PCM WAV."""
    with tempfile.TemporaryDirectory() as tmp:
        src, out = os.path.join(tmp, "in.ogg"), os.path.join(tmp, "out.wav")
        with open(src, "wb") as f:
            f.write(data)
        ffmpeg(["-i", src, "-ac", "1", "-c:a", "pcm_s16le", "-map_metadata", "-1", "-fflags",
                "+bitexact", "-flags:a", "+bitexact", out], name=name)
        with open(out, "rb") as f:
            return f.read()


def per_file(value, rel, default):
    """An item's setting: one for all its files, or a map by file."""
    if isinstance(value, dict):
        return value.get(rel, default)
    return default if value is None else value


def prepare(recipe, item, rel, data, name):
    """The takes of one picked file, cut and trimmed, not yet levelled:
    [(name under dest, samples)]."""
    t = recipe["targets"][item["kind"]]
    ch = t["channels"]
    s = decode(data, ch, name)
    span = per_file(item.get("cut"), rel, None)
    if span:
        s = cut(s, ch, *span)
    parts = [s]
    if "split" in item:
        sp = item["split"]
        parts = split(s, ch, sp.get("gap", 0.3), sp.get("floor_db", -40), sp.get("min", 0.12))
        if not parts:
            raise SystemExit(f"{name}: no takes found")
        parts = [parts[i] for i in sp["keep"]] if "keep" in sp else parts
    ext = ".wav" if t["format"] == "wav" else ".ogg"
    base = per_file(item.get("rename"), rel, None) or out_name(rel, ext)
    outs = []
    for i, p in enumerate(parts):
        if item["kind"] in ("sfx", "jingle"):
            p = trim(p, ch)
        longest = per_file(item.get("max"), rel, None)
        if longest and len(p) > longest * RATE * ch:
            # a long ring cut short, faded out
            p = p[:int(longest * RATE) * ch]
            fade(p, ch, 0.0, min(0.3, longest / 4))
        seam = per_file(item.get("seam"), rel, 0)
        if seam:
            p = loop_seam(p, ch, seam)
        if item["kind"] == "music" and item.get("fade"):
            fade(p, ch, 0.0, item["fade"])
        declick = per_file(item.get("declick"), rel, 0)
        if declick:
            # a loop whose last sample is far from its first: a few
            # milliseconds faded at both ends, the length kept
            fade(p, ch, declick / 1000, declick / 1000)
        out = base if len(parts) == 1 else f"{os.path.splitext(base)[0]}_{i + 1:02d}{ext}"
        outs.append((out, p, rel))
    return outs


def limit(samples, channels, gain, ceiling, name):
    """`gain` dB applied and the peaks held under `ceiling` by a limiter
    (a fast attack, a short release); what still passes is turned down."""
    lin = 10 ** ((ceiling - 0.5) / 20)
    raw = samples.tobytes() if sys.byteorder == "little" else _swapped(samples)
    out = ffmpeg(["-f", "f32le", "-ar", str(RATE), "-ac", str(channels), "-i", "-", "-af",
                  f"volume={gain}dB,alimiter=limit={lin}:attack=2:release=60:level=0",
                  "-f", "f32le", "-"], raw, name).stdout
    p = array.array("f")
    p.frombytes(out)
    if sys.byteorder != "little":
        p.byteswap()
    _, tp = measure(p, channels, name)
    if tp > ceiling:
        g = 10 ** ((ceiling - tp) / 20)
        p = array.array("f", (x * g for x in p))
    return p


def level_group(recipe, item, takes):
    """An item's takes made equally loud: as loud as the kind's target, or
    if the true peak of more than a quarter of them would pass the ceiling
    first, as loud as three quarters of them can be; the peakiest stop at
    the ceiling. An item may let a limiter take up to `limit` dB off its
    peaks (voices, roars; not the attack of a blow). Per-file gain in the
    recipe is added after."""
    t = recipe["targets"][item["kind"]]
    ch, target, ceiling = t["channels"], t["lufs"], t["ceiling"]
    room = item.get("limit", 0.0)
    measured = []
    for out, p, rel in takes:
        lufs, tp = measure(p, ch, f"{item['dest']}/{out}")
        if lufs <= -69.9:
            raise SystemExit(f"{item['dest']}/{out}: silent")
        measured.append((lufs, tp))
    reach = sorted(min(target, lufs + ceiling + room - tp) for lufs, tp in measured)
    group = reach[len(reach) // 4] if len(reach) >= 4 else reach[0]
    group = min(target, group + item.get("level", 0.0))
    done = []
    for (out, p, rel), (lufs, tp) in zip(takes, measured):
        gain = min(group + per_file(item.get("gain"), rel, 0.0) - lufs, ceiling + room - tp)
        name = f"{item['dest']}/{out}"
        if tp + gain > ceiling:
            p = limit(p, ch, gain, ceiling, name)
        else:
            g = 10 ** (gain / 20)
            p = array.array("f", (x * g for x in p))
        lufs_out, tp_out = measure(p, ch, name)
        REPORT[name] = {
            "lufs_in": round(lufs, 1), "tp_in": round(tp, 1), "gain": round(gain, 1),
            "lufs": round(lufs_out, 1), "tp": round(tp_out, 1),
            "limited": round(max(0.0, tp + gain - ceiling), 1),
            "seconds": round(len(p) / ch / RATE, 2)}
        if t["format"] == "wav":
            done.append((out, wav_bytes(p)))
        else:
            done.append((out, ogg_bytes(p, ch, t["quality"], name)))
    return done


# the prepared files of an item are kept by the hash of what made them
# (the item, its download, the kind's target and the preparation), so that
# a run redoes only what changed. PREPARATION names how files are made:
# give it a new value whenever a change here alters what an item prepares
# (an option no item uses yet, or a message, need not).
PREPARATION = "2026-10-13: Vorbis written in blocks"


def prepared(recipe, item, key):
    made = json.dumps([item, SOURCES[key], recipe["targets"].get(item["kind"]), PREPARATION],
                      sort_keys=True)
    return os.path.join(CACHE, "prepared", sha256(made.encode()))


def do_source(op, w, recipe, item):
    blob, key = download(op, item)
    if item["kind"] not in ("keep", "mono"):
        done = prepared(recipe, item, key)
        if os.path.isfile(os.path.join(done, "index.json")):
            index = json.load(open(os.path.join(done, "index.json")))
            for out, report in index["files"]:
                with open(os.path.join(done, out.replace("/", "__")), "rb") as f:
                    w.put(f"{item['dest']}/{out}", f.read())
                REPORT[f"{item['dest']}/{out}"] = report
            if item.get("license_files"):
                for rel, data in license_files(item, members(blob, key)):
                    w.put(f"{item['dest']}/{rel}", data)
            return len(index["files"])
    files = members(blob, key)
    picked = pick(files, item, key) if "files" in item else sorted(files)
    root = item.get("root", "")
    takes = []
    for n in picked:
        rel = n[len(root):]
        if item["kind"] == "keep":
            w.put(f"{item['dest']}/{rel}", files[n])
        elif item["kind"] == "mono":
            if rel.endswith(".ogg"):
                w.put(f"{item['dest']}/{rel[:-4]}.wav", to_mono_wav(files[n], f"{key}:{n}"))
            else:
                w.put(f"{item['dest']}/{rel}", files[n])
        else:
            takes += prepare(recipe, item, rel, files[n], f"{key}:{n}")
    if takes:
        done = prepared(recipe, item, key)
        os.makedirs(done, exist_ok=True)
        index = []
        for out, data in level_group(recipe, item, takes):
            w.put(f"{item['dest']}/{out}", data)
            with open(os.path.join(done, out.replace("/", "__")), "wb") as f:
                f.write(data)
            index.append([out, REPORT[f"{item['dest']}/{out}"]])
        with open(os.path.join(done, "index.json"), "w") as f:
            json.dump({"files": index}, f)
    for rel, data in license_files(item, files):
        w.put(f"{item['dest']}/{rel}", data)
    return len(picked)


def license_files(item, files):
    """The licence files an item keeps, where they lie under its root (packs
    keep one per folder), with the line ends git keeps (LF), so that a run
    changes nothing."""
    root = item.get("root", "")
    out = []
    for lic in item.get("license_files", []):
        rel = lic[len(root):] if lic.startswith(root) else os.path.basename(lic)
        text = files[lic]
        while b"\r\n" in text:  # "\r\r\n" too
            text = text.replace(b"\r\n", b"\n")
        out.append((rel, text))
    return out


def layer_samples(op, recipe, layer, name):
    """A file of a mix's layer, from a source the recipe lists (by its id),
    as stereo samples, cut and looped as the layer asks."""
    src = next((s for s in recipe["sources"] + recipe.get("layers", [])
                if s.get("id") == layer["source"]), None)
    if src is None:
        raise SystemExit(f"no source with id {layer['source']!r}")
    blob, key = download(op, src)
    files = members(blob, key)
    if name not in files:
        raise SystemExit(f"{key}:{name}: not there")
    s = decode(files[name], 2, f"{key}:{name}")
    if "cut" in layer:
        s = cut(s, 2, *layer["cut"])
    if layer.get("seam"):
        s = loop_seam(s, 2, layer["seam"])
    return s


def add_at(out, s, at, lg, rg):
    for f in range(len(s) // 2):
        o = 2 * (at + f)
        out[o] += s[2 * f] * lg
        out[o + 1] += s[2 * f + 1] * rg


def do_mix(op, w, recipe, mix):
    """An ambience bed: layers summed, the seam crossfaded, levelled. A bed
    layer goes round from a point drawn from the seed; an event layer
    ("every": [min, max] seconds apart) plays one of its files at a time,
    louder or softer and panned at random (the seed again)."""
    t = recipe["targets"][mix.get("kind", "loop")]
    srcs = [next((s for s in recipe["sources"] + recipe.get("layers", [])
                  if s.get("id") == l["source"]), {}) for l in mix["layers"]]
    keys = [download(op, s)[1] for s in srcs]
    done = os.path.join(CACHE, "prepared", sha256(json.dumps(
        [mix, [SOURCES[k] for k in keys], t, PREPARATION], sort_keys=True).encode()))
    if os.path.isfile(done + ".ogg"):
        with open(done + ".ogg", "rb") as f:
            w.put(mix["out"], f.read())
        REPORT[mix["out"]] = json.load(open(done + ".json"))
        return
    seam = mix.get("seam", 4.0)
    frames = int((mix["seconds"] + seam) * RATE)
    out = array.array("f", bytes(4 * 2 * frames))
    rng = random.Random(mix["out"])
    for layer in mix["layers"]:
        names = layer["file"] if isinstance(layer["file"], list) else [layer["file"]]
        takes = [layer_samples(op, recipe, layer, n) for n in names]
        g = 10 ** (layer.get("gain", 0.0) / 20)
        if "every" in layer:
            lo, hi = layer["every"]
            spread, width = layer.get("spread", [-6.0, 0.0]), layer.get("pan", 0.6)
            at = int(rng.uniform(0, hi) * RATE)
            while True:
                s = rng.choice(takes)
                n = len(s) // 2
                if at + n >= frames:
                    break
                gg = g * 10 ** (rng.uniform(*spread) / 20) * math.sqrt(2)
                pan = (rng.uniform(-width, width) + 1) * math.pi / 4
                add_at(out, s, at, gg * math.cos(pan), gg * math.sin(pan))
                at += n + int(rng.uniform(lo, hi) * RATE)
        else:
            s = takes[0]
            n = len(s) // 2
            pos = rng.randrange(n)
            looped = (s[2 * pos:] + s * (frames // n + 1))[:2 * frames]
            add_at(out, looped, 0, g, g)
    out = loop_seam(out, 2, seam)
    out, stats = level(out, 2, t["lufs"], t["ceiling"], mix["out"], mix.get("gain", 0.0))
    stats["seconds"] = round(len(out) / 2 / RATE, 2)
    REPORT[mix["out"]] = {"from": "mix", **stats}
    data = ogg_bytes(out, 2, t["quality"], mix["out"])
    os.makedirs(os.path.dirname(done), exist_ok=True)
    with open(done + ".ogg", "wb") as f:
        f.write(data)
    with open(done + ".json", "w") as f:
        json.dump(REPORT[mix["out"]], f)
    w.put(mix["out"], data)


# ---- credits ----------------------------------------------------------------

CREDITS_HEAD = """# Sound credits

The sounds under `cc0/` are dedicated to the public domain (CC0 1.0,
https://creativecommons.org/publicdomain/zero/1.0/) or are works of the U.S.
federal government in the public domain; credit is not required, and it is
given here gladly. The sounds under `ccby/` are licensed under Creative
Commons Attribution by the authors named below (CC BY 3.0,
https://creativecommons.org/licenses/by/3.0/, or CC BY 4.0,
https://creativecommons.org/licenses/by/4.0/, as each line says; OGA-BY 3.0,
https://opengameart.org/content/oga-by-30-faq, is CC BY 3.0 without its
clause on technical measures).

Changes made: the files picked from each source were cut (silence trimmed,
takes split apart, a stretch taken out of a long recording), turned into
mono 44.1 kHz WAV (short sounds) or re-encoded as Ogg Vorbis (ambience,
music, jingles; in mono, the loops placed on lava, water and fountains),
and made louder or quieter to one loudness per kind; a loop's end was
crossfaded into its start. The ambience beds listed under Mixes are made
of the sources they name.

`tools/fetch_audio.py` fetches these files again from the recipe in
`sources.json`, checks the downloads against `audio.lock.json` and writes
this file from the recipe's credits; `manifest.json` says which file plays
for which cue.
"""


def write_credits(recipe):
    rows = {"cc0": set(), "ccby": set()}
    for s in recipe["sources"] + recipe.get("layers", []):
        what = s.get("url") or s.get("itch")
        c = s.get("credit")
        if not c or any(k not in c for k in ("what", "author", "license", "page")):
            raise SystemExit(f"{what}: a credit needs what, author, license and page")
        where = f"`{s['dest']}/`" if "dest" in s else "mixes"
        free = c["license"] in ("CC0", "public domain")
        if "dest" in s and s["dest"].startswith("cc0/") != free:
            raise SystemExit(f"{what}: {c['license']} goes under " + ("cc0/" if free else "ccby/"))
        # the credits page shows What, Author and Source: the licence and
        # what the author asks to be said go with the author
        by = f"{c['author']}, {c['license']}" + (f"; {c['notice']}" if c.get("notice") else "")
        rows["cc0" if free else "ccby"].add(f"| {where} | {c['what']} | {by} | {c['page']} |")
    text = CREDITS_HEAD
    for title, key in (("Public domain", "cc0"), ("Creative Commons Attribution", "ccby")):
        if rows[key]:
            text += (f"\n## {title}\n\n| Where | What | Author | Source |\n"
                     "|---|---|---|---|\n" + "\n".join(sorted(rows[key])) + "\n")
    if recipe.get("mixes"):
        text += "\n## Mixes\n\n| File | What | Made from |\n|---|---|---|\n"
        text += "".join(f"| `{m['out']}` | {m['what']} | {m['made_from']} |\n"
                        for m in recipe["mixes"])
    return text


# ---- main -------------------------------------------------------------------

def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--update", action="store_true",
                    help="write the lock from this run instead of checking it")
    ap.add_argument("--report", help="write each prepared file's loudness numbers here (JSON)")
    args = ap.parse_args()
    recipe = json.load(open(RECIPE))
    if shutil.which("ffmpeg") is None:
        raise SystemExit("ffmpeg is needed (the sounds are converted)")
    op = opener()
    w = Writer(recipe["roots"])
    for item in recipe["sources"]:
        what = item.get("url") or f"{item['itch']['page']} {item['itch']['upload']}"
        print("source", urllib.parse.unquote(what.rsplit("/", 1)[-1]), "->", item["dest"],
              flush=True)
        print(f"  {do_source(op, w, recipe, item)} files", flush=True)
    for item in recipe.get("layers", []):
        download(op, item)
    for mix in recipe.get("mixes", []):
        print("mix", mix["out"], flush=True)
        do_mix(op, w, recipe, mix)
    w.prune()
    with open(CREDITS, "w") as f:
        f.write(write_credits(recipe))
    total = sum(os.path.getsize(p) for p in w.files)
    print(f"{len(w.files)} files, {total / 1e6:.1f} MB under {', '.join(recipe['roots'])}")
    if args.report:
        with open(args.report, "w") as f:
            json.dump(REPORT, f, indent=1, sort_keys=True)
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
