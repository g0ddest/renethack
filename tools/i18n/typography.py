#!/usr/bin/env python3
"""The typography of the Russian translations, client/i18n/ru/*.toml, as
ru-editor's review of 2026-10-18 asks:

- one space after a sentence (or a colon), where NetHack's English has
  two;
- the full stop after the closing guillemet: «…сказал». (a ! ? or …
  stays inside, and so does the dot of an ellipsis);
- «…» for a quote: "…", `…' and a word in single quotes («„…“» inside
  guillemets); a key in quotes ('f', '{1}', '<') stays as it is.

Prose only. A string the catalog shows in a window, a layout or raw, or
one laid out in columns (a width, a run of spaces inside the line that
ends no sentence), keeps its spaces and marks: there they are layout.
Running it twice changes nothing.

    python3 tools/i18n/typography.py          # rewrite the files
    python3 tools/i18n/typography.py --check  # list what needs it, fail if any
"""

import argparse
import glob
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
I18N = os.path.join(HERE, "..", "..", "client", "i18n")

# the uses whose texts are laid out: a window's lines, the tombstone and
# its kin, what goes out raw
LAYOUT_USES = {"window", "layout", "raw"}

# a width ("%-20s", "%*d")
WIDTH = re.compile(r"%-?(\*|\d+)[sdcl]")
# a run of spaces inside a line
GAP = re.compile(r"(?<=\S) {2,}(?=\S)")

# a sentence's end or a colon (a mark, a closing quote, placeholders that
# may render as nothing), then two spaces before what follows
SENTENCE_GAP = re.compile(r"(?<=[.!?…:])([»\"“]?)((?:\{[^{}]*\})*)  (?=\S)")
# a full stop (not an ellipsis's) inside closing guillemets
STOP_INSIDE = re.compile(r"(?<!\.)\.(»+)")
DOUBLE_QUOTED = re.compile(r"\"([^\"\n]+)\"")
BACKTICKED = re.compile(r"`([^`'\n]+)'")
# words in single quotes; a key ('f', '{1}', '<') is no word
SINGLE_QUOTED = re.compile(r"'([A-Za-zА-Яа-яЁё][A-Za-zА-Яа-яЁё -]*[A-Za-zА-Яа-яЁё])'")


def columns(line):
    """Is a line laid out in columns: a run of spaces inside it after no
    sentence's end or colon ("Status of %s:  Level %d  HP %d", "%s
    a trap")? NetHack's two spaces after a sentence are no column."""
    for m in GAP.finditer(line):
        if not line[: m.start()].rstrip("\"'»)").endswith((".", "!", "?", ":")):
            return True
    return False


def prose(entry):
    """Is a catalog entry's text prose, its spaces and marks the
    typography's to change?"""
    if LAYOUT_USES & set(entry.get("uses", [])):
        return False
    fmt = entry.get("fmt", "")
    return not (WIDTH.search(fmt) or any(columns(line) for line in fmt.split("\n")))


def guillemets(text, pattern):
    """`pattern`'s quotes made «…», or „…“ inside guillemets already open."""
    out, last = [], 0
    for m in pattern.finditer(text):
        before = text[: m.start()]
        nested = before.count("«") > before.count("»")
        out += [text[last : m.start()], ("„%s“" if nested else "«%s»") % m.group(1)]
        last = m.end()
    return "".join(out) + text[last:]


def fix(ru):
    """The Russian of a prose string with its typography made right."""
    for pattern in (DOUBLE_QUOTED, BACKTICKED, SINGLE_QUOTED):
        ru = guillemets(ru, pattern)
    ru = STOP_INSIDE.sub(r"\1.", ru)
    return SENTENCE_GAP.sub(r"\1\2 ", ru)


def catalog():
    with open(os.path.join(I18N, "catalog.en.json"), encoding="utf-8") as f:
        return {e["id"]: e for e in json.load(f)["entries"]}


def run(write):
    """The translations whose typography is not right yet, as (file, id,
    old, new); with `write`, made right in their files."""
    entries = catalog()
    changes = []
    for path in sorted(glob.glob(os.path.join(I18N, "ru", "*.toml"))):
        with open(path, encoding="utf-8") as f:
            lines = f.read().split("\n")
        current = None
        touched = False
        for k, line in enumerate(lines):
            if line.startswith("[") and line.endswith("]"):
                current = line[1:-1].strip('"')
                continue
            entry = entries.get(current)
            if not line.startswith("ru = ") or entry is None or not prose(entry):
                continue
            ru = json.loads(line[5:])
            new = fix(ru)
            if new != ru:
                changes.append((os.path.basename(path), current, ru, new))
                lines[k] = "ru = " + json.dumps(new, ensure_ascii=False)
                touched = True
        if write and touched:
            with open(path, "w", encoding="utf-8") as f:
                f.write("\n".join(lines))
    return changes


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="change nothing, fail if a translation needs it")
    args = ap.parse_args()
    changes = run(write=not args.check)
    for name, id_, old, new in changes[:20]:
        print(f"{name} [{id_}]: {json.dumps(old, ensure_ascii=False)} -> {json.dumps(new, ensure_ascii=False)}")
    if args.check:
        if changes:
            print(f"{len(changes)} translations need typography: python3 tools/i18n/typography.py", file=sys.stderr)
            return 1
        return 0
    print(f"{len(changes)} translations changed", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
