#!/usr/bin/env python3
"""The licences of the Rust crates linked into the extension (renethack-gd),
for legal/ in a package: each crate with its version, licence and source,
then the licence files the crates ship, each distinct text once.

    rust_licenses.py --manifest-path client/rust/Cargo.toml \
        [--target <triple>] [--features steam] > THIRD-PARTY-rust.txt

Only what ends up in the binary: normal dependencies, for the target, no
proc-macros (they run at build time), not the workspace's own crates.
"""

import argparse
import json
import pathlib
import subprocess
import sys

ROOT_CRATE = "renethack-gd"
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifest-path", required=True)
    ap.add_argument("--target")
    ap.add_argument("--features", default="")
    args = ap.parse_args()

    cmd = ["cargo", "metadata", "--format-version", "1", "--locked",
           "--manifest-path", args.manifest_path]
    if args.target:
        cmd += ["--filter-platform", args.target]
    if args.features:
        cmd += ["--features", args.features]
    meta = json.loads(subprocess.check_output(cmd))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    root = next(i for i in members if packages[i]["name"] == ROOT_CRATE)

    linked, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in linked:
            continue
        linked.add(pid)
        for dep in nodes[pid]["deps"]:
            normal = any(k["kind"] is None for k in dep["dep_kinds"])
            macro = any("proc-macro" in t["kind"] for t in packages[dep["pkg"]]["targets"])
            if normal and not macro:
                stack.append(dep["pkg"])
    crates = sorted((packages[i] for i in linked - members),
                    key=lambda p: (p["name"], p["version"]))

    texts = {}  # text -> crates shipping it
    out = sys.stdout
    out.write("Rust crates linked into the renethack extension\n"
              "(name, version, licence, source; their licence texts follow)\n\n")
    for p in crates:
        source = p.get("repository") or f"https://crates.io/crates/{p['name']}"
        label = f"{p['name']} {p['version']}"
        out.write(f"{label}  {p.get('license') or p.get('license_file') or '?'}  {source}\n")
        crate_dir = pathlib.Path(p["manifest_path"]).parent
        files = sorted(f for f in crate_dir.iterdir()
                       if f.is_file() and f.name.lower().startswith(LICENSE_PREFIXES))
        if not files:
            texts.setdefault(f"(no licence file in the crate; licence: {p.get('license')})",
                             []).append(label)
        for f in files:
            text = f.read_text(encoding="utf-8", errors="replace").strip()
            texts.setdefault(text, []).append(label)
    out.write("\n")
    for text, users in texts.items():
        out.write("=" * 78 + "\n" + "Used by: " + ", ".join(users) + "\n\n" + text + "\n\n")


if __name__ == "__main__":
    main()
