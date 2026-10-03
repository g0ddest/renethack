"""The names the engine can print, read from its own sources.

objects.h and monsters.h are run through the C preprocessor with macros of
our own, so every entry comes out with its class and flags exactly as the
engine sees them; the smaller tables (artifacts, roles, races, terrain,
shops, skills, conditions) are read with regular expressions. Every reader
takes the root of a NetHack tree, so the same code reads a translated tree
whose files keep the upstream layout (Skadga/nethack-ru, converted to UTF-8)
and the two can be paired entry by entry.

    python3 tools/i18n/lexicon_source.py engine/upstream > /tmp/names.json
"""

import json
import os
import re
import subprocess
import sys
import tempfile

STRING = re.compile(r'"((?:[^"\\]|\\.)*)"')


def c_unescape(s):
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t"}.get(m.group(1), m.group(1)), s)


def strings(text):
    """The C string literals in text, adjacent ones joined."""
    out, last_end = [], None
    for m in STRING.finditer(text):
        s = c_unescape(m.group(1))
        if last_end is not None and text[last_end:m.start()].strip() == "" and out:
            out[-1] += s
        else:
            out.append(s)
        last_end = m.end()
    return out


def read(root, rel):
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


def clean(text):
    """C text without comments and without the lines of `#if 0` and
    `#ifdef X` blocks (none of the platform macros is defined in our
    build); other conditionals are taken as true."""
    out, i = [], 0
    while i < len(text):
        c = text[i]
        if c in "\"'":
            j = i + 1
            while text[j] != c:
                j += 2 if text[j] == "\\" else 1
            out.append(text[i : j + 1])
            i = j + 1
        elif text.startswith("/*", i):
            i = text.index("*/", i + 2) + 2
            out.append(" ")
        elif text.startswith("//", i):
            i = text.index("\n", i)
        else:
            out.append(c)
            i += 1
    lines, live = [], [True]
    for line in "".join(out).split("\n"):
        d = line.strip()
        if d.startswith("#if 0") or d.startswith("#ifdef"):
            live.append(False)
        elif d.startswith("#if"):
            live.append(live[-1])
        elif d.startswith("#else"):
            live[-1] = not live[-1] and all(live[:-1])
        elif d.startswith("#endif"):
            live.pop()
        elif all(live):
            lines.append(line)
    return "\n".join(lines)


def split_args(s):
    """Top-level comma-separated arguments of a macro call's inside."""
    args, depth, cur, i = [], 0, [], 0
    while i < len(s):
        c = s[i]
        if c in "\"'":
            j = i + 1
            while s[j] != c:
                j += 2 if s[j] == "\\" else 1
            cur.append(s[i : j + 1])
            i = j + 1
            continue
        if c in "({":
            depth += 1
        elif c in ")}":
            depth -= 1
        if c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    if cur and "".join(cur).strip():
        args.append("".join(cur).strip())
    return args


def calls(text, name):
    """The argument lists of every `name(...)` in preprocessed text."""
    out = []
    for m in re.finditer(r"\b%s\(" % re.escape(name), text):
        i, depth = m.end(), 1
        while depth:
            c = text[i]
            if c in "\"'":
                i += 1
                while text[i] != c:
                    i += 2 if text[i] == "\\" else 1
            elif c in "({":
                depth += 1
            elif c in ")}":
                depth -= 1
            i += 1
        out.append(split_args(text[m.end() : i - 1]))
    return out


def preprocess(source):
    with tempfile.NamedTemporaryFile("w", suffix=".c", delete=False, encoding="utf-8") as f:
        f.write(source)
        path = f.name
    try:
        return subprocess.run(
            ["cc", "-E", "-P", path], check=True, capture_output=True, text=True, encoding="utf-8"
        ).stdout
    finally:
        os.unlink(path)


def literal(arg):
    """A macro argument that is a string (possibly several joined), else None."""
    s = strings(arg)
    return s[0] if s and arg.strip().startswith('"') else None


# ---------------------------------------------------------------- objects

OBJECT_MACROS = """
#define NoDes 0
#define OBJ(name, desc) @NAME@ name @DESC@ desc
#define BITS(nmkn,mrg,uskn,ctnr,mgc,chrg,uniq,nwsh,big,tuf,dir,sub,mtrl) \\
    BITS(nmkn,mrg,uskn,ctnr,mgc,chrg,uniq,nwsh,big,tuf,dir,sub,mtrl)
#define OBJECT(obj,bits,prp,sym,prob,dly,wt,cost,sdam,ldam,oc1,oc2,nut,color,sn) \\
    OBJENTRY(obj, bits, sym, prob, sn)
#define MARKER(tag,sn)
"""

CLASS_OF = {
    "ILLOBJ_CLASS": "illobj", "WEAPON_CLASS": "weapon", "ARMOR_CLASS": "armor",
    "RING_CLASS": "ring", "AMULET_CLASS": "amulet", "TOOL_CLASS": "tool",
    "FOOD_CLASS": "food", "POTION_CLASS": "potion", "SCROLL_CLASS": "scroll",
    "SPBOOK_CLASS": "spellbook", "WAND_CLASS": "wand", "COIN_CLASS": "coin",
    "GEM_CLASS": "gem", "ROCK_CLASS": "rock", "BALL_CLASS": "ball",
    "CHAIN_CLASS": "chain", "VENOM_CLASS": "venom",
}

BIT_NAMES = ["nmkn", "mrg", "uskn", "ctnr", "mgc", "chrg", "uniq", "nwsh", "big", "tuf", "dir", "sub", "mtrl"]


def objects(root):
    """Every entry of objects[], in order: its enum name, class, name,
    description and the BITS() flags (sub is the armor category or the
    weapon skill, mtrl the material)."""
    text = read(root, "include/objects.h")
    # drop the file's own macro block: ours replace it
    start = text.index("#if defined(OBJECTS_DESCR_INIT)")
    end = text.index("#endif", text.index("#error Unproductive inclusion of objects.h"))
    text = OBJECT_MACROS + text[:start] + text[end + len("#endif") :]
    out = preprocess(text)
    objs = []
    for args in calls(out, "OBJENTRY"):
        obj, bits, sym, prob, sn = args
        name_part, desc_part = obj.split("@DESC@")
        name = literal(name_part.replace("@NAME@", ""))
        desc = literal(desc_part)
        bargs = calls(bits, "BITS")[0]
        flags = dict(zip(BIT_NAMES, bargs))
        objs.append({
            "sn": sn.strip(),
            "class": CLASS_OF[sym.strip()],
            "name": name,
            "desc": desc,
            "prob": prob.strip(),
            **{k: (int(v) if re.fullmatch(r"-?\d+", v) else v) for k, v in flags.items()},
        })
    return objs


# ---------------------------------------------------------------- monsters

MONSTER_MACROS = """
#define NAM(n) @N@ n @N@
#define NAMS(m, f, n) @M@ m @F@ f @N@ n @N@
#define MON(nam, sym, lvl, gen, atk, siz, mr1, mr2, flg1, flg2, flg3, d, col, bn) \\
    MONENTRY(nam, sym, gen, siz, flg1, flg2, flg3, bn)
"""


def monsters(root):
    """Every entry of mons[], in order: names (male, female, neutral), class
    symbol, generation flags, size and the M1/M2/M3 flags as text."""
    text = MONSTER_MACROS + "#define MON_DUMMY\n" + read(root, "include/monsters.h").replace(
        "#elif !defined(MON)", "#elif 0"
    )
    out = preprocess(text)
    mons = []
    for args in calls(out, "MONENTRY"):
        nam, sym, gen, siz, f1, f2, f3, bn = args
        male = female = None
        if "@M@" in nam:
            male = literal(nam.split("@M@")[1].split("@F@")[0])
            female = literal(nam.split("@F@")[1].split("@N@")[0])
        neutral = literal(nam.split("@N@")[1])
        mons.append({
            "bn": bn.strip(), "sym": sym.strip(), "gen": gen, "size": siz,
            "male": male, "female": female, "name": neutral,
            "f1": f1, "f2": f2, "f3": f3,
            "unique": "G_UNIQ" in gen, "nogen": "G_NOGEN" in gen,
            "pname": "M2_PNAME" in f2,
        })
    return mons


# ---------------------------------------------------------------- tables


def artifacts(root):
    text = read(root, "include/artilist.h")
    text = text[text.index("dummy element #0") :]
    out = []
    for m in re.finditer(r'\bA\("((?:[^"\\]|\\.)*)",\s*(\w+)', text):
        if m.group(1):
            out.append({"name": m.group(1), "otyp": m.group(2)})
    return out


def roles(root):
    """name (m, f), ranks [(m, f)] x 9, gods (lawful, neutral, chaotic),
    file code, home base, quest goal."""
    text = clean(read(root, "src/role.c"))
    body = text[text.index("const struct Role roles[") :]
    body = body[: body.index("const struct Race races[")]
    out = []
    for m in re.finditer(r"\{\s*\{\s*(\"[^\"]*\"|0)\s*,\s*(\"[^\"]*\"|0)\s*\}\s*,\s*\{((?:\s*\{[^{}]*\}\s*,?)+)\}\s*,"
                         r"\s*(\"[^\"]*\"|0)\s*,\s*(\"[^\"]*\"|0)\s*,\s*(\"[^\"]*\"|0)\s*,"
                         r"\s*(\"[^\"]*\")\s*,\s*(\"[^\"]*\")\s*,\s*(\"[^\"]*\")", body, re.S):
        def s(x):
            return None if x == "0" else x[1:-1]
        ranks = [tuple(s(x.strip()) for x in r.split(",")) for r in re.findall(r"\{([^{}]*)\}", m.group(3))]
        out.append({
            "name": (s(m.group(1)), s(m.group(2))),
            "ranks": ranks,
            "gods": (s(m.group(4)), s(m.group(5)), s(m.group(6))),
            "code": s(m.group(7)), "home": s(m.group(8)), "goal": s(m.group(9)),
        })
    return out


def races(root):
    text = read(root, "src/role.c")
    body = text[text.index("const struct Race races[") : text.index("const struct Gender genders[")]
    out = []
    for m in re.finditer(r'\{\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*\{\s*("[^"]*"|0)\s*,\s*("[^"]*"|0)\s*\}', body):
        def s(x):
            return None if x == "0" else x[1:-1]
        out.append({"noun": m.group(1), "adj": m.group(2), "coll": m.group(3), "code": m.group(4),
                    "individual": (s(m.group(5)), s(m.group(6)))})
    return out


def aligns(root):
    text = read(root, "src/role.c")
    body = text[text.index("const struct Align aligns[") :]
    body = body[: body.index("};")]
    return [{"noun": a, "adj": b} for a, b in re.findall(r'\{\s*"([^"]*)",\s*"([^"]*)"', body)]


def genders(root):
    text = read(root, "src/role.c")
    body = text[text.index("const struct Gender genders[") :]
    body = body[: body.index("};")]
    return [{"adj": a, "he": b, "him": c, "his": d} for a, b, c, d in
            re.findall(r'\{\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)",\s*"([^"]*)"', body)]


def terrain(root):
    """defsym.h map symbols: (S_ name, description); descriptions empty for
    the beams, swallow and explosion pieces."""
    text = read(root, "include/defsym.h")
    out = []
    for m in re.finditer(r"PCHAR2?\(\s*\d+\s*,\s*'(?:[^'\\]|\\.)*'\s*,\s*(S_\w+)\s*,([^)]*)\)", text):
        ss = strings(m.group(2))
        out.append({"sym": m.group(1), "desc": ss[-1] if ss else ""})
    return out


def monclasses(root):
    text = read(root, "include/defsym.h")
    return [{"sym": m.group(1), "desc": m.group(2)} for m in
            re.finditer(r'MONSYM\(\s*\d+\s*,\s*\'(?:[^\'\\]|\\.)*\'\s*,\s*\w+\s*,\s*(S_\w+)\s*,\s*"([^"]*)"\)', text)]


def objclasses(root):
    text = read(root, "include/defsym.h")
    text = text[text.index("OBJCLASS( 1,") :]
    out = []
    for args in calls(text, "OBJCLASS") + calls(text, "OBJCLASS2"):
        if re.fullmatch(r"\d+", args[0]):
            out.append((int(args[0]), {"name": literal(args[-2]), "explain": literal(args[-1])}))
    return [o for _, o in sorted(out, key=lambda x: x[0])]


def shops(root):
    text = read(root, "src/shknam.c")
    body = text[text.index("const struct shclass shtypes[") :]
    out = []
    for m in re.finditer(r'\{\s*"([^"]*)",\s*("[^"]*"|NULL)', body):
        out.append({"name": m.group(1), "short": None if m.group(2) == "NULL" else m.group(2)[1:-1]})
    return out


def shopkeepers(root):
    """Every shopkeeper name with its list (the shop type) and prefix code."""
    text = clean(read(root, "src/shknam.c"))
    out = []
    for m in re.finditer(r"static const char \*const (shk\w+)\[\] = \{(.*?)\};", text, re.S):
        for s in strings(m.group(2)):
            code = s[0] if s[:1] in "-_+|=" else ""
            out.append({"list": m.group(1), "name": s[len(code):], "code": code})
    return out


def skills(root):
    text = read(root, "src/weapon.c")
    body = text[text.index("odd_skill_names[] = {") :]
    odd = strings(body[: body.index("};")])
    body = text[text.index("barehands_or_martial[] = {") :]
    bare = strings(body[: body.index("};")])
    return {"odd": odd, "barehands": bare}


def conditions(root):
    text = read(root, "src/botl.c")
    body = text[text.index("const struct conditions_t conditions[] = {") :]
    body = body[: body.index("};")]
    out = [{"id": m.group(1), "txt": strings(m.group(2))} for m in
           re.finditer(r"(bl_\w+),\s*\{([^}]*)\}", body)]
    enc = strings(text[text.index("enc_stat[] = {") :].split("};")[0])
    eat = read(root, "src/eat.c")
    hu = [s.strip() for s in strings(eat[eat.index("hu_stat[] = {") :].split("};")[0])]
    return {"conditions": out, "encumbrance": [e for e in enc if e], "hunger": [h for h in hu if h]}


def dungeons(root):
    """Dungeon and level names from dat/dungeon.lua."""
    text = read(root, "dat/dungeon.lua")
    return {
        "dungeons": re.findall(r'\bname\s*=\s*"([^"]*)"', text),
        "bones": re.findall(r'\bbonetag\s*=\s*"([^"]*)"', text),
    }


def everything(root):
    return {
        "objects": objects(root), "monsters": monsters(root), "artifacts": artifacts(root),
        "roles": roles(root), "races": races(root), "aligns": aligns(root),
        "genders": genders(root), "terrain": terrain(root), "monclasses": monclasses(root),
        "objclasses": objclasses(root), "shops": shops(root), "shopkeepers": shopkeepers(root),
        "skills": skills(root), "conditions": conditions(root), "dungeons": dungeons(root),
    }


if __name__ == "__main__":
    json.dump(everything(sys.argv[1]), sys.stdout, ensure_ascii=False, indent=1)
