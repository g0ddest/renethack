#!/usr/bin/env python3
"""The pipeline variants the game draws (client/godot/art/variants.txt).

Godot compiles a render pipeline for each shader over each vertex layout it
draws, for each pass; the rehearsal behind the start-up veil makes one of
each variant on the list (client/rust/renethack-gd/src/variants.rs), so
that no first sight of anything in a game waits for one. The list is the
census of what the game draws: `tools/perf.py --census` writes one per run
(RENETHACK_VARIANTS, census.rs: a variant a line, with the frame it was
first drawn in), and this merges them into the list.

    tools/variants.py merge build/perf          # add what the runs drew
    tools/variants.py merge --replace DIR...    # the list is what they drew
    tools/variants.py check build/perf          # drawn after the veil, not listed

The scenes to take a census of: `variants` (every creature and thing, with
the hover's outline and the x-ray), `deep`, the soaks, the effects and
rays, the states, the branches.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIST = ROOT / "client" / "godot" / "art" / "variants.txt"
HEADER = """\
# The pipeline variants the game draws, one of each made by the rehearsal
# under the start-up veil (client/rust/renethack-gd/src/variants.rs): a
# shader (a standard material's settings, or a shader's file) over a layout
# (a mesh's arrays in hex, its primitive, +inst when skinned), |fade when
# drawn faded; a 3D label's or sprite's look; a canvas item's shader and
# class. Made by `tools/variants.py merge` from the census of
# `tools/perf.py --census`: edit by running them, not by hand.
"""

LAYOUT = re.compile(r"^[0-9a-f]+/[0-9]+(\+inst)?$")
FLAT = ("Label3D", "Sprite3D", "AnimatedSprite3D")


def makeable(v):
    """Whether the rehearsal can make this variant (else why not)."""
    parts = v.split("|")
    if parts[0] == "shown":
        return "shown"
    if parts[0] == "canvas":
        if len(parts) == 3 and (parts[1].startswith("shader:res://") or parts[1] == "default"):
            return None
        return "a canvas item without a shader file"
    if parts[0] in FLAT:
        return None if len(parts) >= 3 and parts[1].startswith("2d:") else "a label or sprite with its own material"
    if len(parts) < 2 or not LAYOUT.match(parts[1]):
        return "no layout"
    shader = parts[0].removeprefix("overlay ")
    if shader == "default" or shader.startswith("std:") or shader.startswith("shader:res://"):
        return None
    return "a shader made at run time"


def census_files(paths):
    for p in paths:
        p = Path(p)
        if p.is_file():
            yield p
        else:
            yield from sorted(p.glob("**/variants.tsv"))


def read(path):
    """(frame, veil, variant, drawn by) of a census."""
    with open(path) as f:
        next(f, None)
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) >= 3:
                yield int(parts[0]), parts[1], parts[2], parts[3] if len(parts) > 3 else ""


def listed():
    if not LIST.exists():
        return set()
    return {
        line.strip()
        for line in LIST.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    }


def merge(args):
    seen = {}
    files = list(census_files(args.paths))
    if not files:
        sys.exit("variants: no census (variants.tsv) under " + " ".join(args.paths))
    for path in files:
        for _, _, v, by in read(path):
            # the rehearsal's own one of each (variants.rs): not the game's
            if "/Variants/" not in by and "/VariantsCanvas/" not in by:
                seen.setdefault(v, by)
    keep = {v for v in seen if makeable(v) is None}
    # a canvas item shown without a shader of its own: the census notes it
    # to tell what a frame showed, there is nothing to make
    skipped = {v: makeable(v) for v in seen if makeable(v) not in (None, "shown")}
    old = listed()
    new = keep if args.replace else keep | old
    LIST.write_text(HEADER + "".join(f"{v}\n" for v in sorted(new)))
    print(f"variants: {len(files)} censuses, {len(seen)} variants drawn, {len(new)} listed"
          f" ({len(new - old)} new, {len(old - new)} gone)")
    for v in sorted(new - old):
        print(f"  + {v}    ({seen.get(v, '')})")
    for v in sorted(old - new):
        print(f"  - {v}")
    for v, why in sorted(skipped.items()):
        print(f"  not made ({why}): {v}    ({seen[v]})")


def check(args):
    known = listed()
    missing = 0
    for path in census_files(args.paths):
        late = [
            (f, v, by)
            for f, veil, v, by in read(path)
            if veil == "0" and v not in known and not v.startswith("shown|")
        ]
        if not late:
            continue
        print(f"{path.parent.name}: drawn first after the veil, not listed:")
        for f, v, by in late:
            missing += 1
            print(f"  #{f} {v}    ({by})")
    if missing:
        print("variants: `tools/variants.py merge` adds them (then the rehearsal makes them)")
        sys.exit(1)
    print("variants: everything drawn after the veil is listed")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("merge", help="add what the censuses drew to the list")
    m.add_argument("paths", nargs="+", help="census files, or directories with them (a perf --out)")
    m.add_argument("--replace", action="store_true", help="the list becomes exactly what they drew")
    c = sub.add_parser("check", help="what was drawn first after the veil and is not listed")
    c.add_argument("paths", nargs="+")
    args = ap.parse_args()
    {"merge": merge, "check": check}[args.cmd](args)


if __name__ == "__main__":
    main()
