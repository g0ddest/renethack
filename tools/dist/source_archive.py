#!/usr/bin/env python3
"""The complete source of the game a package holds, as the NetHack General
Public License asks of a copy in executable form (its 3(a)): one tar.gz of
the repository's files as they were built, NetHack at the commit built, the
Lua release it links, and SOURCE.txt, which says how to build them. The art
and the sounds stay out: they are not the program, and keep their own
licences (their manifests, sources, locks and credits are in).

The archive is the same, byte for byte, for the same files: names sorted,
every entry dated at the commit built and owned by no one, gzip without a
time. tools/dist/common.sh (legal_sources) makes it for each version.

usage: source_archive.py OUT NAME REPO UPSTREAM LUA SOURCE_TXT VERSION
  OUT         the .tar.gz to write
  NAME        the folder it unpacks to (renethack-<version>)
  REPO        the repository (its tracked files, as they are on disk)
  UPSTREAM    the NetHack checkout the engine was built from (git archive)
  LUA         the Lua source the engine build fetched (lua-<version>/);
              what that build made in it (*.o, *.a) is left out
  SOURCE_TXT  the build notes, @VERSION@ replaced
  VERSION     the version the packages carry
"""

import gzip
import io
import os
import re
import subprocess
import sys
import tarfile

# Not the program's source: the art and the sounds, but for the files that
# say what they are, where they come from and whose they are; the pictures
# Steamworks takes.
ASSET_DIRS = ("client/godot/art/", "client/godot/audio/")
LEFT_OUT = ("steam/store/", "steam/achievements/", "steam/icon/")
# How a compiled program begins: ELF, Mach-O (thin and fat), PE.
PROGRAMS = (b"\x7fELF", b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"MZ")


def left_out(path):
    for d in ASSET_DIRS:
        if path.startswith(d):
            rest = path[len(d):]
            return "/" in rest or not rest.endswith((".json", ".md"))
    return path.startswith(LEFT_OUT)


def git(repo, *args):
    return subprocess.run(
        ["git", "-C", repo] + list(args), check=True, capture_output=True
    ).stdout


def repo_entries(repo):
    """The tracked files, with the content on disk: what was built, even
    from a tree with changes not committed."""
    out = {}
    for rec in git(repo, "ls-files", "-s", "-z").split(b"\0"):
        if not rec:
            continue
        meta, path = rec.split(b"\t", 1)
        mode = meta.split()[0].decode()
        path = path.decode()
        full = os.path.join(repo, path)
        if mode == "160000" or left_out(path) or not os.path.lexists(full):
            continue
        if mode == "120000":
            out[path] = ("link", os.readlink(full))
        else:
            with open(full, "rb") as f:
                out[path] = ("file", f.read(), mode == "100755")
    return out


def upstream_entries(upstream, prefix):
    """NetHack as the engine build exports it (git archive of HEAD)."""
    out = {}
    data = git(upstream, "archive", "--format=tar", "HEAD")
    with tarfile.open(fileobj=io.BytesIO(data)) as t:
        for m in t.getmembers():
            name = prefix + m.name.rstrip("/")
            if m.isfile():
                out[name] = ("file", t.extractfile(m).read(), bool(m.mode & 0o111))
            elif m.issym():
                out[name] = ("link", m.linkname)
            elif m.isdir() and name != prefix.rstrip("/"):
                out[name] = ("dir",)  # an empty one too: a submodule's place
    rev = git(upstream, "rev-parse", "HEAD").decode().strip()
    return out, rev


def lua_folder(lua):
    """lua-<major>.<minor>.<release>, as the release unpacks (the builds
    copy it under other names)."""
    with open(os.path.join(lua, "src", "lua.h"), encoding="latin-1") as f:
        text = f.read()
    parts = [
        re.search(r'#define LUA_VERSION_%s\s+"(\d+)"' % p, text).group(1)
        for p in ("MAJOR", "MINOR", "RELEASE")
    ]
    return "lua-" + ".".join(parts)


def lua_entries(lua, prefix):
    """The Lua release's files, without the objects and library NetHack's
    build compiled beside them."""
    out = {}
    for root, dirs, files in os.walk(lua):
        dirs.sort()
        for f in files:
            full = os.path.join(root, f)
            if f.endswith((".o", ".a")) or os.path.islink(full):
                continue
            with open(full, "rb") as h:
                data = h.read()
            if data.startswith(PROGRAMS):
                continue  # a program the build linked (lua, luac)
            rel = os.path.relpath(full, lua).replace(os.sep, "/")
            out[prefix + rel] = ("file", data, bool(os.stat(full).st_mode & 0o111))
    return out


def write(out_path, name, entries, mtime):
    dirs = set()
    for path in entries:
        parts = path.split("/")[:-1]
        for i in range(1, len(parts) + 1):
            dirs.add("/".join(parts[:i]))
    names = sorted(set(entries) | dirs)
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.PAX_FORMAT) as t:
        root = tarfile.TarInfo(name)
        root.type, root.mode, root.mtime = tarfile.DIRTYPE, 0o755, mtime
        t.addfile(root)
        for n in names:
            info = tarfile.TarInfo(name + "/" + n)
            info.mtime = mtime
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            e = entries.get(n)
            if e is None or e[0] == "dir":
                info.type, info.mode = tarfile.DIRTYPE, 0o755
                t.addfile(info)
            elif e[0] == "link":
                info.type, info.mode, info.linkname = tarfile.SYMTYPE, 0o777, e[1]
                t.addfile(info)
            else:
                info.mode = 0o755 if e[2] else 0o644
                info.size = len(e[1])
                t.addfile(info, io.BytesIO(e[1]))
    with open(out_path, "wb") as f:
        f.write(gzip.compress(buf.getvalue(), compresslevel=9, mtime=0))


def main():
    if len(sys.argv) != 8:
        sys.exit(__doc__)
    out, name, repo, upstream, lua, source_txt, version = sys.argv[1:]
    entries = repo_entries(repo)
    up, rev = upstream_entries(upstream, "engine/upstream/")
    entries.update(up)
    entries["engine/upstream.rev"] = ("file", (rev + "\n").encode(), False)
    entries.update(lua_entries(lua, "engine/" + lua_folder(lua) + "/"))
    with open(source_txt, encoding="utf-8") as f:
        text = f.read().replace("@VERSION@", version)
    entries["SOURCE.txt"] = ("file", text.encode("utf-8"), False)
    mtime = int(git(repo, "log", "-1", "--format=%ct", "HEAD").decode().strip())
    write(out, name, entries, mtime)


if __name__ == "__main__":
    main()
